//! Query and search tools for the knowledge graph.
//!
//! Provides unified search across entities and references, plus semantic
//! subgraph extraction with Best-First Search.

use std::process::Command;

use rmcp::{
    handler::server::wrapper::Parameters,
    model::CallToolResult,
    schemars::{self, JsonSchema},
    tool, tool_router, ErrorData as McpError,
};
use serde::Deserialize;

use crate::mcp::protocol::Response;
use crate::mcp::server::McpServer;
use crate::models::QueryGraph;
use crate::services::{
    GraphService, ScoringStrategy as ServiceScoringStrategy, SearchTarget as ServiceSearchTarget,
    SemanticQueryParams,
};

/// Spawn the visualizer process with the given JSON data.
fn spawn_visualizer(data: &QueryGraph) -> Result<(), McpError> {
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
// Parameter Types (with JsonSchema for MCP)
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

/// What to search: entities, references, or both.
#[derive(Debug, Clone, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum SearchTarget {
    /// Search only entities.
    Entities,
    /// Search only document references.
    References,
    /// Search both entities and references.
    #[default]
    All,
}

impl From<SearchTarget> for ServiceSearchTarget {
    fn from(target: SearchTarget) -> Self {
        match target {
            SearchTarget::Entities => ServiceSearchTarget::Entities,
            SearchTarget::References => ServiceSearchTarget::References,
            SearchTarget::All => ServiceSearchTarget::All,
        }
    }
}

/// Parameters for unified search tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    /// Natural language search query.
    pub query: String,
    /// What to search: "entities", "references", or "all" (default).
    #[serde(default)]
    pub target: Option<SearchTarget>,
    /// Maximum results (default: 20).
    #[serde(default)]
    pub limit: Option<u32>,
    /// Minimum similarity score (0.0 to 1.0, default: 0.3).
    #[serde(default)]
    pub min_score: Option<f32>,
    /// Filter by scope (entities only).
    #[serde(default)]
    pub scope: Option<String>,
}

/// Scoring strategy for semantic subgraph extraction.
#[derive(Debug, Clone, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScoringStrategy {
    /// Only global token accumulation affects scoring (simpler, may go deep).
    #[default]
    Global,
    /// Also penalize deep branches to encourage breadth.
    BranchPenalty,
}

impl From<ScoringStrategy> for ServiceScoringStrategy {
    fn from(strategy: ScoringStrategy) -> Self {
        match strategy {
            ScoringStrategy::Global => ServiceScoringStrategy::Global,
            ScoringStrategy::BranchPenalty => ServiceScoringStrategy::BranchPenalty,
        }
    }
}

/// Parameters for semantic subgraph query tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct QueryParams {
    /// Starting entity (optional - if omitted, searches for best match).
    #[serde(default)]
    pub entity_id: Option<String>,
    /// Semantic query for relevance scoring (uses entity.description if omitted).
    #[serde(default)]
    pub semantic_query: Option<String>,
    /// Maximum nodes in result (default: 50).
    #[serde(default)]
    pub max_nodes: Option<u32>,
    /// Maximum estimated tokens (default: 4000).
    #[serde(default)]
    pub max_tokens: Option<u32>,
    /// Minimum relevance to include node (default: 0.3).
    #[serde(default)]
    pub min_relevance: Option<f32>,
    /// Scoring strategy: "global" or "branch_penalty" (default: "global").
    #[serde(default)]
    pub scoring_strategy: Option<ScoringStrategy>,
    /// Filter relationship types (e.g., ["BELONGS_TO", "CALLS"]).
    #[serde(default)]
    pub relationship_types: Option<Vec<String>>,
    /// Open 3D visualization window (spawns separate process).
    #[serde(default)]
    pub visualize: Option<bool>,
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

    /// Unified semantic search across entities and references.
    #[tool(
        description = "Unified semantic search. Returns entities and/or references based on target parameter."
    )]
    pub async fn search(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(query = %params.query, target = ?params.target, "Running search tool");

        let service = self.resolve::<GraphService>();
        let target: ServiceSearchTarget = params.target.unwrap_or_default().into();
        let limit = params.limit.unwrap_or(20);
        let min_score = params.min_score.unwrap_or(0.3);

        let result = service
            .unified_search(
                &params.query,
                target,
                limit,
                min_score,
                params.scope.as_deref(),
            )
            .await?;

        tracing::info!(
            entities = result.entities.len(),
            references = result.references.len(),
            "Search completed"
        );

        Response(result).into()
    }

    /// Semantic subgraph extraction with Best-First Search.
    #[tool(
        description = "Semantic subgraph extraction with relevance-based pruning. Returns optimized graph within budget."
    )]
    pub async fn query(
        &self,
        Parameters(params): Parameters<QueryParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(
            entity_id = ?params.entity_id,
            semantic_query = ?params.semantic_query,
            "Running query tool"
        );

        // Validate: need at least one of entity_id or semantic_query
        if params.entity_id.is_none() && params.semantic_query.is_none() {
            return Err(McpError::invalid_params(
                "Either entity_id or semantic_query must be provided",
                None,
            ));
        }

        let service = self.resolve::<GraphService>();

        // Build service params
        let service_params = SemanticQueryParams {
            entity_id: params.entity_id,
            semantic_query: params.semantic_query,
            max_nodes: params.max_nodes.unwrap_or(50) as usize,
            max_tokens: params.max_tokens.unwrap_or(4000) as usize,
            min_relevance: params.min_relevance.unwrap_or(0.3),
            scoring_strategy: params.scoring_strategy.unwrap_or_default().into(),
            relationship_types: params.relationship_types,
        };

        // Execute semantic query - returns QueryGraph directly
        let result = service.semantic_query(service_params).await?;

        tracing::info!(
            nodes = result.nodes.len(),
            edges = result.edges.len(),
            tokens = result.stats.estimated_tokens,
            pruned = result.stats.nodes_pruned,
            "Query completed"
        );

        // Spawn visualizer if requested
        if params.visualize == Some(true) {
            spawn_visualizer(&result)?;
        }

        Response(result).into()
    }
}
