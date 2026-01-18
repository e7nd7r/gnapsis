//! Project management tools - initialization and statistics.

use rmcp::{
    handler::server::wrapper::Parameters,
    model::CallToolResult,
    schemars::{self, JsonSchema},
    tool, tool_router, ErrorData as McpError,
};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::mcp::server::McpServer;
use crate::migrations::run_migrations;
use crate::repositories::{ProjectStats, SchemaRepository, ScopeInfo};

// ============================================================================
// Parameter Types
// ============================================================================

/// Parameters for init_project tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct InitProjectParams {
    /// Force re-run migrations even if already at latest version.
    #[serde(default)]
    pub force: bool,
}

// ============================================================================
// Response Types
// ============================================================================

/// Result of project initialization.
#[derive(Debug, Serialize)]
pub struct InitProjectResult {
    /// Schema version before migration.
    pub previous_version: u32,
    /// Schema version after migration.
    pub current_version: u32,
    /// List of migrations that were applied.
    pub applied_migrations: Vec<String>,
    /// Whether the project was already initialized.
    pub was_initialized: bool,
}

/// Project statistics response.
#[derive(Debug, Serialize)]
pub struct ProjectStatsResult {
    /// Total entity count.
    pub entity_count: i64,
    /// Total category count.
    pub category_count: i64,
    /// Total document count.
    pub document_count: i64,
    /// Total document reference count.
    pub reference_count: i64,
    /// Current schema version.
    pub schema_version: u32,
    /// Available scopes with hierarchy info.
    pub scopes: Vec<ScopeInfoResult>,
}

/// Scope information for stats response.
#[derive(Debug, Serialize)]
pub struct ScopeInfoResult {
    pub name: String,
    pub depth: u8,
    pub description: String,
}

impl From<ScopeInfo> for ScopeInfoResult {
    fn from(info: ScopeInfo) -> Self {
        Self {
            name: info.name,
            depth: info.depth,
            description: info.description,
        }
    }
}

// ============================================================================
// Tool Router
// ============================================================================

#[tool_router(router = project_tools, vis = "pub(crate)")]
impl McpServer {
    /// Initialize the project database schema and seed data.
    ///
    /// This runs all pending migrations to set up:
    /// - Node constraints (Entity, Category, Document, DocumentReference, Scope)
    /// - APOC triggers for data integrity
    /// - Default scopes (Domain → Feature → Namespace → Component → Unit)
    /// - Default categories for each scope
    /// - Vector indexes for semantic search
    #[tool(
        description = "Initialize the project database schema and seed data. Runs pending migrations."
    )]
    pub async fn init_project(
        &self,
        Parameters(_params): Parameters<InitProjectParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!("Running init_project tool");

        let schema_repo = self.resolve::<SchemaRepository>();

        // Check if already initialized
        let was_initialized = schema_repo
            .is_initialized()
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        // Run migrations
        let result = run_migrations(&self.ctx.graph)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let response = InitProjectResult {
            previous_version: result.previous_version,
            current_version: result.current_version,
            applied_migrations: result.applied_migrations,
            was_initialized,
        };

        tracing::info!(
            previous = response.previous_version,
            current = response.current_version,
            applied = ?response.applied_migrations,
            "Project initialization complete"
        );

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::to_value(response).unwrap(),
        )
        .unwrap()]))
    }

    /// Get project statistics including entity counts and schema info.
    ///
    /// Returns counts for entities, categories, documents, and references,
    /// along with the current schema version and available scopes.
    #[tool(
        description = "Get project statistics: entity counts, schema version, and available scopes."
    )]
    pub async fn get_project_stats(&self) -> Result<CallToolResult, McpError> {
        tracing::info!("Running get_project_stats tool");

        let schema_repo = self.resolve::<SchemaRepository>();

        // Get stats
        let stats: ProjectStats = schema_repo
            .get_project_stats()
            .await
            .map_err(|e: AppError| McpError::from(e))?;

        // Get scopes
        let scopes: Vec<ScopeInfo> = schema_repo
            .list_scopes()
            .await
            .map_err(|e: AppError| McpError::from(e))?;

        let response = ProjectStatsResult {
            entity_count: stats.entity_count,
            category_count: stats.category_count,
            document_count: stats.document_count,
            reference_count: stats.reference_count,
            schema_version: stats.schema_version,
            scopes: scopes.into_iter().map(Into::into).collect(),
        };

        tracing::info!(
            entities = response.entity_count,
            categories = response.category_count,
            documents = response.document_count,
            references = response.reference_count,
            version = response.schema_version,
            "Project stats retrieved"
        );

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::to_value(response).unwrap(),
        )
        .unwrap()]))
    }
}
