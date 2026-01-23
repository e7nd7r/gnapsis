//! Application error types with MCP protocol conversion.

use rmcp::model::ErrorCode;
use thiserror::Error;

/// Application-level errors for Gnapsis.
#[derive(Error, Debug)]
pub enum AppError {
    // Neo4j errors
    #[error("Neo4j connection error: {0}")]
    Connection(#[from] neo4rs::Error),

    #[error("Neo4j query error: {message}")]
    Query { message: String, query: String },

    // Domain errors
    #[error("Entity not found: {0}")]
    EntityNotFound(String),

    #[error("Category not found: {0}")]
    CategoryNotFound(String),

    #[error("Scope not found: {0}")]
    ScopeNotFound(String),

    #[error("Invalid BELONGS_TO: {child} cannot belong to {parent} - {reason}")]
    InvalidBelongsTo {
        child: String,
        parent: String,
        reason: String,
    },

    #[error("Entity has children and cannot be deleted: {0}")]
    HasChildren(String),

    #[error("Validation error: {0}")]
    Validation(String),

    // Git errors
    #[error("Git error: {0}")]
    Git(#[from] git2::Error),

    #[error("Git error: {message}")]
    GitMessage { message: String },

    #[error("Repository not found at: {0}")]
    RepoNotFound(String),

    // Embedding errors
    #[error("Embedding generation failed: {0}")]
    Embedding(String),

    // LSP errors
    #[error("LSP unavailable: {0}")]
    LspUnavailable(String),

    #[error("Symbol '{symbol}' not found in '{path}'")]
    SymbolNotFound { symbol: String, path: String },

    // Config errors
    #[error("Configuration error: {0}")]
    Config(#[from] crate::config::ConfigError),

    #[error("Project not initialized. Run init_project first.")]
    NotInitialized,
}

impl From<AppError> for rmcp::model::ErrorData {
    fn from(err: AppError) -> Self {
        let (code, app_code) = match &err {
            AppError::EntityNotFound(_) => (ErrorCode::RESOURCE_NOT_FOUND, "ENTITY_NOT_FOUND"),
            AppError::CategoryNotFound(_) => (ErrorCode::RESOURCE_NOT_FOUND, "CATEGORY_NOT_FOUND"),
            AppError::ScopeNotFound(_) => (ErrorCode::RESOURCE_NOT_FOUND, "SCOPE_NOT_FOUND"),
            AppError::InvalidBelongsTo { .. } => (ErrorCode::INVALID_PARAMS, "INVALID_BELONGS_TO"),
            AppError::HasChildren(_) => (ErrorCode::INVALID_PARAMS, "HAS_CHILDREN"),
            AppError::Validation(_) => (ErrorCode::INVALID_PARAMS, "VALIDATION_ERROR"),
            AppError::NotInitialized => (ErrorCode::INVALID_REQUEST, "NOT_INITIALIZED"),
            AppError::Config(_) => (ErrorCode::INTERNAL_ERROR, "CONFIG_ERROR"),
            AppError::Connection(_) => (ErrorCode::INTERNAL_ERROR, "CONNECTION_ERROR"),
            AppError::Query { .. } => (ErrorCode::INTERNAL_ERROR, "QUERY_ERROR"),
            AppError::Git(_) => (ErrorCode::INTERNAL_ERROR, "GIT_ERROR"),
            AppError::GitMessage { .. } => (ErrorCode::INTERNAL_ERROR, "GIT_ERROR"),
            AppError::RepoNotFound(_) => (ErrorCode::RESOURCE_NOT_FOUND, "REPO_NOT_FOUND"),
            AppError::Embedding(_) => (ErrorCode::INTERNAL_ERROR, "EMBEDDING_ERROR"),
            AppError::LspUnavailable(_) => (ErrorCode::INTERNAL_ERROR, "LSP_UNAVAILABLE"),
            AppError::SymbolNotFound { .. } => (ErrorCode::INVALID_PARAMS, "SYMBOL_NOT_FOUND"),
        };

        rmcp::model::ErrorData::new(code, format!("[{}] {}", app_code, err), None)
    }
}
