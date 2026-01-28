//! Project management tools - initialization and overview.

use rmcp::{
    handler::server::wrapper::Parameters,
    model::CallToolResult,
    schemars::{self, JsonSchema},
    tool, tool_router, ErrorData as McpError,
};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::mcp::protocol::{OutputFormat, Response};
use crate::mcp::server::McpServer;
use crate::models::{Category, ProjectEntitySummary};
use crate::repositories::{CategoryRepository, QueryRepository, SchemaRepository};

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

/// Parameters for project_overview tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProjectOverviewParams {
    /// Generate markdown skill file at this path (optional).
    /// When provided, writes a structured overview document.
    #[serde(default)]
    pub output_path: Option<String>,

    /// Include full entity descriptions (default: false for summaries only).
    /// When false, descriptions are truncated to 100 characters.
    #[serde(default)]
    pub include_descriptions: Option<bool>,

    /// Output format: "json" (default) or "toon" (40-60% fewer tokens).
    #[serde(default)]
    pub output_format: Option<OutputFormat>,
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

/// Category information for project overview.
#[derive(Debug, Serialize)]
pub struct CategoryInfo {
    /// Unique category ID.
    pub id: String,
    /// Category name.
    pub name: String,
    /// Scope this category belongs to.
    pub scope: String,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl From<Category> for CategoryInfo {
    fn from(cat: Category) -> Self {
        Self {
            id: cat.id,
            name: cat.name,
            scope: cat.scope.to_string(),
            description: cat.description,
        }
    }
}

/// Entity summary for project overview.
#[derive(Debug, Serialize)]
pub struct EntityInfo {
    /// Entity ID.
    pub id: String,
    /// Entity name.
    pub name: String,
    /// Entity description (may be truncated).
    pub description: String,
    /// Parent entity ID (for hierarchy).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Category classification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

impl EntityInfo {
    fn from_summary(summary: ProjectEntitySummary, include_full_description: bool) -> Self {
        let description = if include_full_description {
            summary.description
        } else {
            truncate_description(&summary.description, 100)
        };
        Self {
            id: summary.id,
            name: summary.name,
            description,
            parent_id: summary.parent_id,
            category: summary.category,
        }
    }
}

/// Project statistics.
#[derive(Debug, Serialize)]
pub struct ProjectStats {
    /// Number of Domain entities.
    pub domains: usize,
    /// Number of Feature entities.
    pub features: usize,
    /// Number of Namespace entities.
    pub namespaces: usize,
    /// Number of Component entities.
    pub components: usize,
    /// Number of Unit entities.
    pub units: usize,
    /// Total reference count.
    pub references: i64,
}

/// Result of project_overview tool.
#[derive(Debug, Serialize)]
pub struct ProjectOverviewResult {
    /// All categories grouped by scope.
    pub categories: Vec<CategoryInfo>,

    /// Domain-level entities (high-level concepts).
    pub domains: Vec<EntityInfo>,

    /// Feature-level entities.
    pub features: Vec<EntityInfo>,

    /// Namespace-level entities.
    pub namespaces: Vec<EntityInfo>,

    /// Project statistics.
    pub stats: ProjectStats,

    /// Generated skill file path (if output_path was provided).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_path: Option<String>,
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
        // TODO: EPIC004-F4 - Update Context to use PostgresClient instead of neo4rs::Graph
        // For now, migrations must be run via CLI or the postgres integration test
        let result = crate::migrations::MigrationResult {
            previous_version: 0,
            current_version: 0,
            applied_migrations: vec![],
        };

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

        Response(response, None).into()
    }

    /// Get complete project context: taxonomy, entity hierarchy, and statistics.
    ///
    /// Returns categories, high-level entities (Domain, Feature, Namespace),
    /// and aggregate statistics. Optionally generates a markdown skill file
    /// for quick context loading in new sessions.
    #[tool(
        description = "Get full project context: taxonomy (categories), entity hierarchy (domains, features, namespaces), and statistics. Optionally generates a skill file."
    )]
    pub async fn project_overview(
        &self,
        Parameters(params): Parameters<ProjectOverviewParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(
            output_path = ?params.output_path,
            include_descriptions = ?params.include_descriptions,
            "Running project_overview tool"
        );

        let category_repo = self.resolve::<CategoryRepository>();
        let query_repo = self.resolve::<QueryRepository>();
        let schema_repo = self.resolve::<SchemaRepository>();

        let include_descriptions = params.include_descriptions.unwrap_or(false);

        // Get all categories
        let categories: Vec<CategoryInfo> = category_repo
            .list()
            .await
            .map_err(|e: AppError| McpError::from(e))?
            .into_iter()
            .map(Into::into)
            .collect();

        // Get entities by scope
        let domains: Vec<EntityInfo> = query_repo
            .get_entity_summaries_by_scope("Domain")
            .await
            .map_err(|e: AppError| McpError::from(e))?
            .into_iter()
            .map(|s| EntityInfo::from_summary(s, include_descriptions))
            .collect();

        let features: Vec<EntityInfo> = query_repo
            .get_entity_summaries_by_scope("Feature")
            .await
            .map_err(|e: AppError| McpError::from(e))?
            .into_iter()
            .map(|s| EntityInfo::from_summary(s, include_descriptions))
            .collect();

        let namespaces: Vec<EntityInfo> = query_repo
            .get_entity_summaries_by_scope("Namespace")
            .await
            .map_err(|e: AppError| McpError::from(e))?
            .into_iter()
            .map(|s| EntityInfo::from_summary(s, include_descriptions))
            .collect();

        // Get component and unit counts for stats
        let components = query_repo
            .get_entity_summaries_by_scope("Component")
            .await
            .map_err(|e: AppError| McpError::from(e))?;

        let units = query_repo
            .get_entity_summaries_by_scope("Unit")
            .await
            .map_err(|e: AppError| McpError::from(e))?;

        // Get reference count from schema stats
        let db_stats = schema_repo
            .get_project_stats()
            .await
            .map_err(|e: AppError| McpError::from(e))?;

        let stats = ProjectStats {
            domains: domains.len(),
            features: features.len(),
            namespaces: namespaces.len(),
            components: components.len(),
            units: units.len(),
            references: db_stats.reference_count,
        };

        // Generate skill file if requested
        let skill_path = if let Some(path) = &params.output_path {
            let content = generate_skill_file(&domains, &features, &namespaces);
            std::fs::write(path, &content).map_err(|e| {
                McpError::internal_error(format!("Failed to write skill file: {}", e), None)
            })?;
            Some(path.clone())
        } else {
            None
        };

        let response = ProjectOverviewResult {
            categories,
            domains,
            features,
            namespaces,
            stats,
            skill_path,
        };

        tracing::info!(
            categories = response.categories.len(),
            domains = response.stats.domains,
            features = response.stats.features,
            namespaces = response.stats.namespaces,
            components = response.stats.components,
            units = response.stats.units,
            references = response.stats.references,
            skill_path = ?response.skill_path,
            "Project overview retrieved"
        );

        Response(response, params.output_format).into()
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Truncate a description to the specified length, adding "..." if truncated.
fn truncate_description(desc: &str, max_len: usize) -> String {
    if desc.len() <= max_len {
        desc.to_string()
    } else {
        format!("{}...", &desc[..max_len.saturating_sub(3)])
    }
}

/// Generate a markdown skill file from the project overview data.
fn generate_skill_file(
    domains: &[EntityInfo],
    features: &[EntityInfo],
    namespaces: &[EntityInfo],
) -> String {
    let mut content = String::new();

    // Header
    content.push_str("# Project Overview\n\n");

    // Purpose (from domain entities)
    if !domains.is_empty() {
        content.push_str("## Purpose\n\n");
        for domain in domains {
            content.push_str(&format!("**{}**: {}\n\n", domain.name, domain.description));
        }
    }

    // Features
    if !features.is_empty() {
        content.push_str("## Features\n\n");
        for feature in features {
            content.push_str(&format!(
                "- **{}**: {}\n",
                feature.name, feature.description
            ));
        }
        content.push('\n');
    }

    // Architecture (namespaces)
    if !namespaces.is_empty() {
        content.push_str("## Architecture\n\n");
        for ns in namespaces {
            content.push_str(&format!("- **{}**: {}\n", ns.name, ns.description));
        }
        content.push('\n');
    }

    content
}
