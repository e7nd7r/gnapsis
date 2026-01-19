//! Query and search tools for the knowledge graph.
//!
//! These tools provide thin MCP handlers that delegate to GraphService.

use std::process::Command;

use rmcp::{
    handler::server::wrapper::Parameters,
    model::CallToolResult,
    schemars::{self, JsonSchema},
    tool, tool_router, ErrorData as McpError,
};
use serde::{Deserialize, Serialize};

use crate::mcp::protocol::Response;
use crate::mcp::server::McpServer;
use crate::services::GraphService;

/// Spawn the visualizer process with the given JSON data.
fn spawn_visualizer<T: Serialize>(data: &T) -> Result<(), McpError> {
    // Write to temp file
    let temp_file = tempfile::NamedTempFile::with_suffix(".json").map_err(|e| {
        McpError::internal_error(format!("Failed to create temp file: {}", e), None)
    })?;

    let json = serde_json::to_string_pretty(data)
        .map_err(|e| McpError::internal_error(format!("Failed to serialize: {}", e), None))?;

    std::fs::write(temp_file.path(), &json)
        .map_err(|e| McpError::internal_error(format!("Failed to write temp file: {}", e), None))?;

    // Keep the temp file around (don't delete on drop)
    let temp_path = temp_file.into_temp_path();
    let path_str = temp_path.to_string_lossy().to_string();

    // Spawn detached process
    let exe = std::env::current_exe()
        .map_err(|e| McpError::internal_error(format!("Failed to get current exe: {}", e), None))?;

    Command::new(exe)
        .arg("visualize")
        .arg(&path_str)
        .spawn()
        .map_err(|e| {
            McpError::internal_error(format!("Failed to spawn visualizer: {}", e), None)
        })?;

    // Prevent temp file from being deleted (it will be cleaned up by OS eventually)
    std::mem::forget(temp_path);

    tracing::info!(path = %path_str, "Spawned visualizer process");
    Ok(())
}

// ============================================================================
// Parameter Types
// ============================================================================

/// Parameters for get_entity tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetEntityParams {
    /// Entity ID to retrieve.
    pub entity_id: String,
}

/// Parameters for find_entities tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindEntitiesParams {
    /// Filter by scope name (Domain, Feature, Namespace, Component, Unit).
    #[serde(default)]
    pub scope: Option<String>,
    /// Filter by category name.
    #[serde(default)]
    pub category: Option<String>,
    /// Filter by parent entity ID.
    #[serde(default)]
    pub parent_id: Option<String>,
    /// Maximum number of results (default: 50).
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Parameters for get_document_entities tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetDocumentEntitiesParams {
    /// Document path to search.
    pub document_path: String,
}

/// Parameters for get_composition_graph tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetCompositionGraphParams {
    /// Entity ID to get composition for.
    pub entity_id: String,
    /// Direction: "ancestors", "descendants", or "both" (default: "both").
    #[serde(default)]
    pub direction: Option<String>,
    /// Maximum depth to traverse (default: unlimited).
    #[serde(default)]
    pub max_depth: Option<u32>,
    /// Open 3D visualization window (spawns separate process).
    #[serde(default)]
    pub visualize: Option<bool>,
}

/// Parameters for query_subgraph tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct QuerySubgraphParams {
    /// Starting entity ID.
    pub entity_id: String,
    /// Maximum number of hops (1-5, default: 2).
    #[serde(default)]
    pub hops: Option<u32>,
    /// Filter by relationship types (e.g., ["BELONGS_TO", "CALLS"]).
    #[serde(default)]
    pub relationship_types: Option<Vec<String>>,
    /// Optional semantic query to filter results.
    #[serde(default)]
    pub semantic_query: Option<String>,
    /// Open 3D visualization window (spawns separate process).
    #[serde(default)]
    pub visualize: Option<bool>,
}

/// Parameters for search_documents tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchDocumentsParams {
    /// Natural language search query.
    pub query: String,
    /// Maximum number of results (default: 10).
    #[serde(default)]
    pub limit: Option<u32>,
    /// Minimum similarity score (0.0 to 1.0, default: 0.5).
    #[serde(default)]
    pub min_score: Option<f32>,
}

/// Parameters for semantic_search tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SemanticSearchParams {
    /// Natural language search query.
    pub query: String,
    /// Maximum number of results (default: 10).
    #[serde(default)]
    pub limit: Option<u32>,
    /// Minimum similarity score (0.0 to 1.0, default: 0.5).
    #[serde(default)]
    pub min_score: Option<f32>,
    /// Filter by scope name.
    #[serde(default)]
    pub scope: Option<String>,
}

// ============================================================================
// Tool Router
// ============================================================================

#[tool_router(router = query_tools, vis = "pub(crate)")]
impl McpServer {
    /// Get full entity details including classifications, references, and hierarchy.
    #[tool(
        description = "Get entity details including classifications, references, and hierarchy."
    )]
    pub async fn get_entity(
        &self,
        Parameters(params): Parameters<GetEntityParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(id = %params.entity_id, "Running get_entity tool");

        let service = self.resolve::<GraphService>();
        let entity = service.get_entity(&params.entity_id).await?;

        Response(entity).into()
    }

    /// Find entities by classification criteria.
    #[tool(description = "Find entities by scope, category, or parent. Returns entity summaries.")]
    pub async fn find_entities(
        &self,
        Parameters(params): Parameters<FindEntitiesParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(
            scope = ?params.scope,
            category = ?params.category,
            parent_id = ?params.parent_id,
            "Running find_entities tool"
        );

        let service = self.resolve::<GraphService>();
        let entities = service
            .find_entities(
                params.scope.as_deref(),
                params.category.as_deref(),
                params.parent_id.as_deref(),
                params.limit.unwrap_or(50),
            )
            .await?;

        Response(entities).into()
    }

    /// Get all entities with references to a document.
    #[tool(description = "Get all entities in a document with their reference details.")]
    pub async fn get_document_entities(
        &self,
        Parameters(params): Parameters<GetDocumentEntitiesParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(path = %params.document_path, "Running get_document_entities tool");

        let service = self.resolve::<GraphService>();
        let entities = service.get_document_entities(&params.document_path).await?;

        Response(entities).into()
    }

    /// Get composition graph (ancestors and descendants via BELONGS_TO).
    #[tool(
        description = "Get entity's composition subgraph (ancestors, descendants) via BELONGS_TO."
    )]
    pub async fn get_composition_graph(
        &self,
        Parameters(params): Parameters<GetCompositionGraphParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(id = %params.entity_id, "Running get_composition_graph tool");

        let service = self.resolve::<GraphService>();
        let graph = service
            .get_composition_graph(
                &params.entity_id,
                params.direction.as_deref().unwrap_or("both"),
                params.max_depth.unwrap_or(10),
            )
            .await?;

        tracing::info!(
            id = %params.entity_id,
            ancestors = graph.ancestors.len(),
            descendants = graph.descendants.len(),
            "Retrieved composition graph"
        );

        // Spawn visualizer if requested
        if params.visualize == Some(true) {
            spawn_visualizer(&graph)?;
        }

        Response(graph).into()
    }

    /// Query subgraph around an entity within N hops.
    #[tool(
        description = "Extract subgraph around an entity within N hops. Returns nodes and edges."
    )]
    pub async fn query_subgraph(
        &self,
        Parameters(params): Parameters<QuerySubgraphParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(
            id = %params.entity_id,
            hops = ?params.hops,
            "Running query_subgraph tool"
        );

        let visualize = params.visualize;

        let service = self.resolve::<GraphService>();
        let subgraph = service
            .query_subgraph(
                &params.entity_id,
                params.hops.unwrap_or(2),
                params.relationship_types,
            )
            .await?;

        tracing::info!(
            nodes = subgraph.nodes.len(),
            edges = subgraph.edges.len(),
            "Retrieved subgraph"
        );

        // Spawn visualizer if requested
        if visualize == Some(true) {
            spawn_visualizer(&subgraph)?;
        }

        Response(subgraph).into()
    }

    /// Search document references by semantic similarity.
    #[tool(
        description = "Search document references by semantic similarity. Returns matching references with scores."
    )]
    pub async fn search_documents(
        &self,
        Parameters(params): Parameters<SearchDocumentsParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(query = %params.query, "Running search_documents tool");

        let service = self.resolve::<GraphService>();
        let results = service
            .search_documents(
                &params.query,
                params.limit.unwrap_or(10),
                params.min_score.unwrap_or(0.5),
            )
            .await?;

        tracing::info!(count = results.len(), "Search completed");

        Response(results).into()
    }

    /// Search entities by semantic similarity.
    #[tool(
        description = "Search entities by semantic similarity to a query. Returns matching entities with scores."
    )]
    pub async fn semantic_search(
        &self,
        Parameters(params): Parameters<SemanticSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(query = %params.query, "Running semantic_search tool");

        let service = self.resolve::<GraphService>();
        let results = service
            .semantic_search(
                &params.query,
                params.limit.unwrap_or(10),
                params.min_score.unwrap_or(0.5),
                params.scope.as_deref(),
            )
            .await?;

        tracing::info!(count = results.len(), "Search completed");

        Response(results).into()
    }
}
