//! Configuration with layered resolution using figment.
//!
//! Resolution order (highest priority last):
//! 1. User config: `~/.config/gnapsis/config.toml` (XDG) or platform config dir
//! 2. Project config: `.gnapsis.toml`
//! 3. Environment variables: `GNAPSIS_*`

use std::ops::Deref;

use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use serde::Deserialize;

/// Boxed wrapper for figment::Error to reduce Result size on the stack.
#[derive(Debug)]
pub struct ConfigError(Box<figment::Error>);

impl Deref for ConfigError {
    type Target = figment::Error;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl From<figment::Error> for ConfigError {
    fn from(err: figment::Error) -> Self {
        Self(Box::new(err))
    }
}

/// Root configuration structure.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub neo4j: Neo4jConfig,
    pub embedding: EmbeddingConfig,
    #[serde(default)]
    pub project: ProjectConfig,
    #[serde(default)]
    pub server: ServerConfig,
}

/// Neo4j database configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct Neo4jConfig {
    pub uri: String,
    pub user: String,
    pub password: Option<String>,
    #[serde(default = "default_database")]
    pub database: String,
    #[serde(default = "default_pool_size")]
    pub pool_size: usize,
}

fn default_database() -> String {
    "neo4j".to_string()
}

fn default_pool_size() -> usize {
    10
}

/// Embedding provider configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct EmbeddingConfig {
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_dimensions")]
    pub dimensions: usize,
}

fn default_provider() -> String {
    "fastembed".to_string()
}

fn default_model() -> String {
    "BAAI/bge-small-en-v1.5".to_string()
}

fn default_dimensions() -> usize {
    384
}

/// Project-specific configuration.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProjectConfig {
    pub name: Option<String>,
    pub repo_path: Option<String>,
}

/// HTTP server configuration.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ServerConfig {
    /// API key for authentication (Bearer token).
    /// Can also be set via GNAPSIS_SERVER__API_KEY env var.
    pub api_key: Option<String>,
    /// OAuth 2.0 authorization server URL (e.g., WorkOS).
    /// Set via GNAPSIS_SERVER__OAUTH_AUTHORIZATION_SERVER env var.
    pub oauth_authorization_server: Option<String>,
    /// Public URL of this resource server.
    /// Set via GNAPSIS_SERVER__RESOURCE_URL env var.
    pub resource_url: Option<String>,
}

impl Config {
    /// Load config with layered resolution (user → project → env).
    pub fn load() -> Result<Self, ConfigError> {
        let user_config = Self::user_config_path();

        Figment::new()
            // Layer 1: User config (lowest priority)
            .merge(Toml::file(user_config))
            // Layer 2: Project config
            .merge(Toml::file(".gnapsis.toml"))
            // Layer 3: Environment variables (highest priority)
            // Use double underscore for nesting (e.g., GNAPSIS_SERVER__API_KEY -> server.api_key)
            .merge(Env::prefixed("GNAPSIS_").map(|key| key.as_str().replace("__", ".").into()))
            .extract()
            .map_err(ConfigError::from)
    }

    /// User config path: ~/.config/gnapsis/config.toml (XDG) or platform config dir.
    fn user_config_path() -> std::path::PathBuf {
        // Prefer XDG config location (~/.config) on all platforms
        if let Some(home) = dirs::home_dir() {
            let xdg_path = home.join(".config").join("gnapsis").join("config.toml");
            if xdg_path.exists() {
                return xdg_path;
            }
        }
        // Fall back to platform-specific config dir
        dirs::config_dir()
            .map(|p| p.join("gnapsis").join("config.toml"))
            .unwrap_or_default()
    }
}
