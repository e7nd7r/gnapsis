//! HTTP server command handler.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use color_eyre::Result;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tower::ServiceBuilder;

use crate::config::Config;
use crate::context::Context;
use crate::mcp::McpServer;

use super::App;

/// JWKS (JSON Web Key Set) response from OAuth server.
#[derive(Debug, Clone, Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

/// Individual JSON Web Key.
#[derive(Debug, Clone, Deserialize)]
struct Jwk {
    kid: String,
    kty: String,
    n: Option<String>,
    e: Option<String>,
    alg: Option<String>,
}

/// JWT claims from WorkOS.
#[derive(Debug, Deserialize)]
struct Claims {
    sub: String,
    iss: String,
    exp: u64,
}

/// Cached JWKS with timestamp.
struct JwksCache {
    jwks: Option<Jwks>,
    fetched_at: Option<std::time::Instant>,
}

impl JwksCache {
    fn new() -> Self {
        Self {
            jwks: None,
            fetched_at: None,
        }
    }

    fn is_stale(&self) -> bool {
        match self.fetched_at {
            Some(t) => t.elapsed() > std::time::Duration::from_secs(300), // 5 min cache
            None => true,
        }
    }
}

/// Authentication middleware state.
#[derive(Clone)]
struct AuthState {
    api_key: Option<String>,
    oauth_authorization_server: Option<String>,
    resource_url: Option<String>,
    jwks_cache: Arc<RwLock<JwksCache>>,
}

/// OAuth 2.0 Protected Resource Metadata (RFC 9728).
#[derive(Serialize)]
struct ProtectedResourceMetadata {
    resource: String,
    authorization_servers: Vec<String>,
}

/// Handler for /.well-known/oauth-protected-resource
async fn oauth_protected_resource(
    axum::extract::State(state): axum::extract::State<AuthState>,
) -> Response {
    match (&state.oauth_authorization_server, &state.resource_url) {
        (Some(auth_server), Some(resource)) => Json(ProtectedResourceMetadata {
            resource: resource.clone(),
            authorization_servers: vec![auth_server.clone()],
        })
        .into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

/// OpenID Connect discovery response.
#[derive(Debug, Deserialize)]
struct OidcDiscovery {
    jwks_uri: String,
}

/// Fetch JWKS from the OAuth authorization server using OpenID Connect discovery.
async fn fetch_jwks(auth_server: &str) -> Option<Jwks> {
    let base_url = auth_server.trim_end_matches('/');

    // First, try OpenID Connect discovery to get the JWKS URI
    let discovery_url = format!("{}/.well-known/openid-configuration", base_url);
    tracing::debug!("Fetching OpenID discovery from: {}", discovery_url);

    let jwks_url = match reqwest::get(&discovery_url).await {
        Ok(resp) => match resp.json::<OidcDiscovery>().await {
            Ok(discovery) => {
                tracing::debug!("Found JWKS URI: {}", discovery.jwks_uri);
                discovery.jwks_uri
            }
            Err(e) => {
                tracing::debug!("OpenID discovery failed: {}, trying default path", e);
                format!("{}/.well-known/jwks.json", base_url)
            }
        },
        Err(e) => {
            tracing::debug!("OpenID discovery request failed: {}, trying default path", e);
            format!("{}/.well-known/jwks.json", base_url)
        }
    };

    tracing::debug!("Fetching JWKS from: {}", jwks_url);

    match reqwest::get(&jwks_url).await {
        Ok(resp) => match resp.json::<Jwks>().await {
            Ok(jwks) => {
                tracing::debug!("Fetched {} keys from JWKS", jwks.keys.len());
                Some(jwks)
            }
            Err(e) => {
                tracing::warn!("Failed to parse JWKS: {}", e);
                None
            }
        },
        Err(e) => {
            tracing::warn!("Failed to fetch JWKS: {}", e);
            None
        }
    }
}

/// Validate a JWT token against the JWKS.
async fn validate_jwt(token: &str, state: &AuthState) -> bool {
    let auth_server = match &state.oauth_authorization_server {
        Some(s) => s,
        None => return false,
    };

    // Get or refresh JWKS cache
    let jwks = {
        let needs_refresh = {
            let cache = state.jwks_cache.read().await;
            cache.is_stale()
        };

        if needs_refresh {
            if let Some(new_jwks) = fetch_jwks(auth_server).await {
                let mut cache = state.jwks_cache.write().await;
                cache.jwks = Some(new_jwks);
                cache.fetched_at = Some(std::time::Instant::now());
            }
        }

        let cache = state.jwks_cache.read().await;
        cache.jwks.clone()
    };

    let jwks = match jwks {
        Some(j) => j,
        None => {
            tracing::warn!("No JWKS available for JWT validation");
            return false;
        }
    };

    // Decode JWT header to get the key ID
    let header = match decode_header(token) {
        Ok(h) => h,
        Err(e) => {
            tracing::debug!("Failed to decode JWT header: {}", e);
            return false;
        }
    };

    let kid = match &header.kid {
        Some(k) => k,
        None => {
            tracing::debug!("JWT has no kid in header");
            return false;
        }
    };

    // Find matching key in JWKS
    let jwk = match jwks.keys.iter().find(|k| &k.kid == kid) {
        Some(k) => k,
        None => {
            tracing::debug!("No matching key found for kid: {}", kid);
            return false;
        }
    };

    // Build decoding key from JWK
    let decoding_key = match (&jwk.n, &jwk.e) {
        (Some(n), Some(e)) => match DecodingKey::from_rsa_components(n, e) {
            Ok(k) => k,
            Err(e) => {
                tracing::debug!("Failed to create decoding key: {}", e);
                return false;
            }
        },
        _ => {
            tracing::debug!("JWK missing n or e components");
            return false;
        }
    };

    // Set up validation
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[auth_server.as_str()]);
    validation.validate_exp = true;
    // Disable audience validation - WorkOS sets audience to the client ID
    // and we don't have it configured. We rely on issuer validation instead.
    validation.validate_aud = false;

    // Validate the token
    match decode::<Claims>(token, &decoding_key, &validation) {
        Ok(token_data) => {
            tracing::debug!("JWT validated for subject: {}", token_data.claims.sub);
            true
        }
        Err(e) => {
            tracing::debug!("JWT validation failed: {}", e);
            false
        }
    }
}

/// Authentication middleware that checks for Bearer token or JWT.
async fn auth_middleware(
    axum::extract::State(state): axum::extract::State<AuthState>,
    req: Request,
    next: Next,
) -> Response {
    // Skip auth for well-known endpoints (OAuth discovery)
    if req.uri().path().starts_with("/.well-known/") {
        return next.run(req).await;
    }

    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok());

    let token = match auth_header {
        Some(h) if h.starts_with("Bearer ") => &h[7..],
        _ => return StatusCode::UNAUTHORIZED.into_response(),
    };

    // First check simple API key if configured
    if let Some(expected_key) = &state.api_key {
        if token == expected_key {
            return next.run(req).await;
        }
    }

    // Then try JWT validation if OAuth is configured
    if state.oauth_authorization_server.is_some() {
        if validate_jwt(token, &state).await {
            return next.run(req).await;
        }
    }

    // If no auth method succeeded but none were configured, allow
    if state.api_key.is_none() && state.oauth_authorization_server.is_none() {
        return next.run(req).await;
    }

    StatusCode::UNAUTHORIZED.into_response()
}

impl App {
    /// Run the MCP server with HTTP transport.
    pub async fn run_serve(&self, host: &str, port: u16) -> Result<()> {
        tracing::info!("Starting Gnapsis HTTP server");

        let config = Config::load()?;

        // Log auth status
        if config.server.api_key.is_some() {
            tracing::info!("API key authentication enabled");
        } else {
            tracing::warn!("No API key configured - server is unprotected!");
        }

        let auth_state = AuthState {
            api_key: config.server.api_key.clone(),
            oauth_authorization_server: config.server.oauth_authorization_server.clone(),
            resource_url: config.server.resource_url.clone(),
            jwks_cache: Arc::new(RwLock::new(JwksCache::new())),
        };

        // Log OAuth status
        if auth_state.oauth_authorization_server.is_some() {
            tracing::info!(
                "OAuth protected resource enabled, auth server: {}",
                auth_state.oauth_authorization_server.as_ref().unwrap()
            );
        }

        let ctx = Context::from(config).await?;

        let service = StreamableHttpService::new(
            move || Ok(McpServer::new(ctx.clone())),
            Arc::new(LocalSessionManager::default()),
            StreamableHttpServerConfig::default(),
        );

        let app = Router::new()
            .route(
                "/.well-known/oauth-protected-resource",
                get(oauth_protected_resource),
            )
            .fallback_service(ServiceBuilder::new().service(service))
            .layer(middleware::from_fn_with_state(
                auth_state.clone(),
                auth_middleware,
            ))
            .with_state(auth_state);

        let addr: SocketAddr = format!("{}:{}", host, port)
            .parse()
            .map_err(|e| color_eyre::eyre::eyre!("Invalid address {}:{}: {}", host, port, e))?;

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| color_eyre::eyre::eyre!("Failed to bind to {}: {}", addr, e))?;

        tracing::info!("Gnapsis HTTP server listening on http://{}", addr);

        axum::serve(listener, app).await.map_err(|e| {
            tracing::error!(error = %e, "HTTP server error");
            color_eyre::eyre::eyre!("HTTP server error: {}", e)
        })?;

        tracing::info!("HTTP server shutting down");
        Ok(())
    }
}
