//! HTTP server command handler.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;
use color_eyre::Result;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use tower::ServiceBuilder;

use crate::config::Config;
use crate::context::Context;
use crate::mcp::McpServer;

use super::App;

/// Authentication middleware state.
#[derive(Clone)]
struct AuthState {
    api_key: Option<String>,
}

/// Authentication middleware that checks for Bearer token.
async fn auth_middleware(
    axum::extract::State(state): axum::extract::State<AuthState>,
    req: Request,
    next: Next,
) -> Response {
    // Skip auth if no key configured
    let Some(expected_key) = &state.api_key else {
        return next.run(req).await;
    };

    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok());

    match auth_header {
        Some(h) if h == format!("Bearer {}", expected_key) => next.run(req).await,
        _ => StatusCode::UNAUTHORIZED.into_response(),
    }
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
        };

        let ctx = Context::from(config).await?;

        let service = StreamableHttpService::new(
            move || Ok(McpServer::new(ctx.clone())),
            Arc::new(LocalSessionManager::default()),
            StreamableHttpServerConfig::default(),
        );

        let app = Router::new()
            .fallback_service(ServiceBuilder::new().service(service))
            .layer(middleware::from_fn_with_state(auth_state, auth_middleware));

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
