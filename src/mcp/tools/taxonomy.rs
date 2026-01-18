//! Taxonomy management tools - scopes and categories.

use rmcp::{
    handler::server::wrapper::Parameters,
    model::CallToolResult,
    schemars::{self, JsonSchema},
    tool, tool_router, ErrorData as McpError,
};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::mcp::server::McpServer;
use crate::models::{Category, Scope};
use crate::repositories::{CategoryRepository, SchemaRepository};

// ============================================================================
// Parameter Types
// ============================================================================

/// Parameters for list_categories tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListCategoriesParams {
    /// Filter by scope name (Domain, Feature, Namespace, Component, Unit).
    /// If not provided, returns all categories grouped by scope.
    #[serde(default)]
    pub scope: Option<String>,
}

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

/// Scope information in the hierarchy.
#[derive(Debug, Serialize)]
pub struct ScopeResult {
    /// Scope name (Domain, Feature, Namespace, Component, Unit).
    pub name: String,
    /// Hierarchy depth (1-5, Domain=1, Unit=5).
    pub depth: u8,
    /// Human-readable description.
    pub description: String,
}

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

/// Response for list_scopes tool.
#[derive(Debug, Serialize)]
pub struct ListScopesResult {
    /// All scopes in hierarchical order.
    pub scopes: Vec<ScopeResult>,
}

/// Response for list_categories tool.
#[derive(Debug, Serialize)]
pub struct ListCategoriesResult {
    /// Categories matching the filter.
    pub categories: Vec<CategoryResult>,
    /// Total count of returned categories.
    pub count: usize,
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
    /// List all scopes in the compositional hierarchy.
    ///
    /// Returns the fixed scope hierarchy:
    /// Domain (1) → Feature (2) → Namespace (3) → Component (4) → Unit (5)
    ///
    /// Scopes define the levels at which categories are defined.
    /// Each scope has a depth indicating its position in the hierarchy.
    #[tool(description = "List all scopes and their composition hierarchy.")]
    pub async fn list_scopes(&self) -> Result<CallToolResult, McpError> {
        tracing::info!("Running list_scopes tool");

        let schema_repo = self.resolve::<SchemaRepository>();

        let scopes = schema_repo
            .list_scopes()
            .await
            .map_err(|e: AppError| McpError::from(e))?;

        let response = ListScopesResult {
            scopes: scopes
                .into_iter()
                .map(|s| ScopeResult {
                    name: s.name,
                    depth: s.depth,
                    description: s.description,
                })
                .collect(),
        };

        tracing::info!(count = response.scopes.len(), "Listed scopes");

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::to_value(response).unwrap(),
        )
        .unwrap()]))
    }

    /// List categories, optionally filtered by scope.
    ///
    /// Categories are classification values at each scope level.
    /// For example: "struct" at Component scope, "method" at Unit scope.
    #[tool(
        description = "List categories by scope. If scope is not provided, returns all categories."
    )]
    pub async fn list_categories(
        &self,
        Parameters(params): Parameters<ListCategoriesParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(scope = ?params.scope, "Running list_categories tool");

        let category_repo = self.resolve::<CategoryRepository>();

        let categories = if let Some(scope_name) = params.scope {
            // Parse scope from string
            let scope: Scope = scope_name
                .parse()
                .map_err(|e: String| McpError::invalid_params(e, None))?;

            category_repo
                .list_by_scope(scope)
                .await
                .map_err(|e: AppError| McpError::from(e))?
        } else {
            category_repo
                .list()
                .await
                .map_err(|e: AppError| McpError::from(e))?
        };

        let count = categories.len();
        let response = ListCategoriesResult {
            categories: categories.into_iter().map(Into::into).collect(),
            count,
        };

        tracing::info!(count = response.count, "Listed categories");

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::to_value(response).unwrap(),
        )
        .unwrap()]))
    }

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

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::to_value(response).unwrap(),
        )
        .unwrap()]))
    }
}
