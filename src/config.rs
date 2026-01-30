//! Configuration with layered resolution using figment.
//!
//! Resolution order (highest priority last):
//! 1. User config: `~/.config/gnapsis/config.toml` (XDG) or platform config dir
//! 2. Project config: `.gnapsis.toml`
//! 3. Environment variables: `GNAPSIS_*`
//!
//! # Intended Usage
//!
//! **Global config** (`~/.config/gnapsis/config.toml`):
//! ```toml
//! [postgres]
//! uri = "postgresql://postgres:password@host:5432/gnapsis_db"
//!
//! [embedding]
//! provider = "fastembed"
//! model = "BAAI/bge-small-en-v1.5"
//! dimensions = 384
//! ```
//!
//! **Project config** (`.gnapsis.toml` in source directory):
//! ```toml
//! [project]
//! name = "my-project"
//!
//! [[project.sources]]
//! id = "code"
//! path = "/path/to/code-repo"
//!
//! [[project.sources]]
//! id = "docs"
//! path = "/path/to/documentation-vault"
//! ```
//!
//! Sources define directories containing project files. References use `source_id`
//! to indicate which source they belong to. If no sources are configured, the
//! current directory is used as a default source. The graph name is always
//! `gnapsis_<project_name>` - all sources share the same graph.

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
    pub postgres: PostgresConfig,
    pub embedding: EmbeddingConfig,
    pub project: ProjectConfig,
}

/// PostgreSQL + Apache AGE database configuration.
///
/// Typically defined in global config (`~/.config/gnapsis/config.toml`).
#[derive(Debug, Clone, Deserialize)]
pub struct PostgresConfig {
    /// PostgreSQL connection string (required).
    /// Example: `postgresql://user:pass@host:5432/database`
    pub uri: String,
}

/// Embedding provider configuration.
///
/// Typically defined in global config (`~/.config/gnapsis/config.toml`).
#[derive(Debug, Clone, Deserialize)]
pub struct EmbeddingConfig {
    /// Embedding provider name (e.g., "fastembed").
    pub provider: String,
    /// Model identifier (e.g., "BAAI/bge-small-en-v1.5").
    pub model: String,
    /// Embedding vector dimensions (e.g., 384).
    pub dimensions: usize,
}

/// A source directory for the project.
///
/// Sources allow a project to span multiple directories (e.g., code repo and
/// documentation vault). Each source has a unique ID used by references to
/// indicate which source they belong to. All sources share the same graph.
#[derive(Debug, Clone, Deserialize)]
pub struct Source {
    /// Unique identifier for this source (e.g., "code", "docs", "vault").
    pub id: String,
    /// Absolute path to the source directory.
    pub path: String,
}

/// Project-specific configuration.
///
/// Typically defined in project config (`.gnapsis.toml` in source directory).
#[derive(Debug, Clone, Deserialize)]
pub struct ProjectConfig {
    /// Project name (required).
    pub name: String,
    /// Source directories for the project.
    /// If not specified, the current working directory is used as the default source.
    #[serde(default)]
    pub sources: Vec<Source>,
}

/// Default source ID used when no sources are configured.
pub const DEFAULT_SOURCE_ID: &str = "default";

/// Serde default function for source_id fields.
pub fn default_source_id() -> String {
    DEFAULT_SOURCE_ID.to_string()
}

impl ProjectConfig {
    /// Returns the graph name: `gnapsis_<name>`.
    pub fn graph_name(&self) -> String {
        format!("gnapsis_{}", self.name)
    }

    /// Find a source by ID.
    /// Returns None if the source doesn't exist and it's not the default.
    pub fn get_source(&self, id: &str) -> Option<&Source> {
        self.sources.iter().find(|s| s.id == id)
    }

    /// Get all sources, or a default source at cwd if none configured.
    pub fn effective_sources(&self) -> Vec<Source> {
        if self.sources.is_empty() {
            vec![Source {
                id: DEFAULT_SOURCE_ID.to_string(),
                path: std::env::current_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| ".".to_string()),
            }]
        } else {
            self.sources.clone()
        }
    }

    /// Resolve a path relative to a source.
    /// If no sources configured and source_id is "default", uses cwd.
    pub fn resolve_path(&self, source_id: &str, relative_path: &str) -> Option<String> {
        if let Some(source) = self.get_source(source_id) {
            Some(format!(
                "{}/{}",
                source.path.trim_end_matches('/'),
                relative_path
            ))
        } else if source_id == DEFAULT_SOURCE_ID && self.sources.is_empty() {
            // Default source at cwd when no sources configured
            std::env::current_dir()
                .map(|p| {
                    format!(
                        "{}/{}",
                        p.to_string_lossy().trim_end_matches('/'),
                        relative_path
                    )
                })
                .ok()
        } else {
            None
        }
    }
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
            .merge(Env::prefixed("GNAPSIS_").split("_"))
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
