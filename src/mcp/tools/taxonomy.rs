//! Taxonomy management tools - category creation.

use rmcp::{
    handler::server::wrapper::Parameters,
    model::CallToolResult,
    schemars::{self, JsonSchema},
    tool, tool_router, ErrorData as McpError,
};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::mcp::protocol::Response;
use crate::mcp::server::McpServer;
use crate::models::{Category, Scope};
use crate::repositories::CategoryRepository;

// ============================================================================
// Parameter Types
// ============================================================================

/// Parameters for create_category tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateCategoryParams {
    /// Category name (e.g., "orchestration", "struct", "method").
    pub name: String,

    /// Scope for this category (Domain, Feature, Namespace, Component, Unit).
    pub scope: String,

    /// Optional description of what this category represents.
    #[serde(default)]
    pub description: Option<String>,
}

// ============================================================================
// Response Types
// ============================================================================

/// Category information.
#[derive(Debug, Serialize)]
pub struct CategoryResult {
    /// Unique category ID (ULID).
    pub id: String,
    /// Category name.
    pub name: String,
    /// Scope this category belongs to.
    pub scope: String,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl From<Category> for CategoryResult {
    fn from(cat: Category) -> Self {
        Self {
            id: cat.id,
            name: cat.name,
            scope: cat.scope.to_string(),
            description: cat.description,
        }
    }
}

/// Response for create_category tool.
#[derive(Debug, Serialize)]
pub struct CreateCategoryResult {
    /// The created category.
    pub category: CategoryResult,
}

// ============================================================================
// Tool Router
// ============================================================================

#[tool_router(router = taxonomy_tools, vis = "pub(crate)")]
impl McpServer {
    /// Create a new category at a specific scope.
    ///
    /// Categories are used to classify entities. Each category belongs to
    /// exactly one scope in the hierarchy.
    ///
    /// Example: Create "async_function" category at Unit scope.
    #[tool(description = "Create a new category at a scope.")]
    pub async fn create_category(
        &self,
        Parameters(params): Parameters<CreateCategoryParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(
            name = %params.name,
            scope = %params.scope,
            "Running create_category tool"
        );

        let category_repo = self.resolve::<CategoryRepository>();

        // Parse scope from string
        let scope: Scope = params
            .scope
            .parse()
            .map_err(|e: String| McpError::invalid_params(e, None))?;

        // Check if category already exists
        let existing = category_repo
            .find_by_name(&params.name, scope)
            .await
            .map_err(|e: AppError| McpError::from(e))?;

        if existing.is_some() {
            return Err(McpError::invalid_params(
                format!(
                    "Category '{}' already exists at scope '{}'",
                    params.name, scope
                ),
                None,
            ));
        }

        // Create the category
        let category = category_repo
            .create(&params.name, scope, params.description.as_deref())
            .await
            .map_err(|e: AppError| McpError::from(e))?;

        let response = CreateCategoryResult {
            category: category.into(),
        };

        tracing::info!(
            id = %response.category.id,
            name = %response.category.name,
            scope = %response.category.scope,
            "Created category"
        );

        Response(response).into()
    }
}
