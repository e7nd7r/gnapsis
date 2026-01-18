//! Query and search tools for the knowledge graph.
//!
//! These tools provide thin MCP handlers that delegate to GraphService.

use rmcp::{
    handler::server::wrapper::Parameters,
    model::CallToolResult,
    schemars::{self, JsonSchema},
    tool, tool_router, ErrorData as McpError,
};
use serde::{Deserialize, Serialize};

use crate::mcp::server::McpServer;
use crate::models::{
    CategoryClassification, CompositionGraph, CompositionNode, DocumentReference, Entity,
    EntityWithContext, EntityWithReference, SearchResult, Subgraph, SubgraphEdge, SubgraphNode,
};
use crate::services::GraphService;

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
// Response Types (MCP-specific serialization)
// ============================================================================

/// Category info for MCP response.
#[derive(Debug, Serialize)]
pub struct CategoryInfoResponse {
    pub id: String,
    pub name: String,
    pub scope: String,
}

impl From<CategoryClassification> for CategoryInfoResponse {
    fn from(c: CategoryClassification) -> Self {
        Self {
            id: c.id,
            name: c.name,
            scope: c.scope,
        }
    }
}

/// Document reference info for MCP response.
#[derive(Debug, Serialize)]
pub struct ReferenceInfoResponse {
    pub id: String,
    pub document_path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub description: String,
    pub commit_sha: String,
}

impl From<DocumentReference> for ReferenceInfoResponse {
    fn from(r: DocumentReference) -> Self {
        Self {
            id: r.id,
            document_path: r.document_path,
            start_line: r.start_line,
            end_line: r.end_line,
            description: r.description,
            commit_sha: r.commit_sha,
        }
    }
}

/// Entity summary for MCP response.
#[derive(Debug, Serialize)]
pub struct EntitySummaryResponse {
    pub id: String,
    pub name: String,
    pub description: String,
}

impl From<Entity> for EntitySummaryResponse {
    fn from(e: Entity) -> Self {
        Self {
            id: e.id,
            name: e.name,
            description: e.description,
        }
    }
}

/// Full entity details for MCP response.
#[derive(Debug, Serialize)]
pub struct EntityDetailsResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub classifications: Vec<CategoryInfoResponse>,
    pub references: Vec<ReferenceInfoResponse>,
    pub parents: Vec<EntitySummaryResponse>,
    pub children: Vec<EntitySummaryResponse>,
    pub related: Vec<EntitySummaryResponse>,
}

impl From<EntityWithContext> for EntityDetailsResponse {
    fn from(ctx: EntityWithContext) -> Self {
        Self {
            id: ctx.entity.id,
            name: ctx.entity.name,
            description: ctx.entity.description,
            classifications: ctx.classifications.into_iter().map(Into::into).collect(),
            references: ctx.references.into_iter().map(Into::into).collect(),
            parents: ctx.parents.into_iter().map(Into::into).collect(),
            children: ctx.children.into_iter().map(Into::into).collect(),
            related: ctx.related.into_iter().map(Into::into).collect(),
        }
    }
}

/// Response for get_entity tool.
#[derive(Debug, Serialize)]
pub struct GetEntityResult {
    pub entity: EntityDetailsResponse,
}

/// Response for find_entities tool.
#[derive(Debug, Serialize)]
pub struct FindEntitiesResult {
    pub entities: Vec<EntitySummaryResponse>,
    pub count: usize,
}

/// Entity with reference for MCP response.
#[derive(Debug, Serialize)]
pub struct EntityWithReferenceResponse {
    pub entity: EntitySummaryResponse,
    pub reference: ReferenceInfoResponse,
}

impl From<EntityWithReference> for EntityWithReferenceResponse {
    fn from(er: EntityWithReference) -> Self {
        Self {
            entity: er.entity.into(),
            reference: er.reference.into(),
        }
    }
}

/// Response for get_document_entities tool.
#[derive(Debug, Serialize)]
pub struct GetDocumentEntitiesResult {
    pub document_path: String,
    pub entities: Vec<EntityWithReferenceResponse>,
    pub count: usize,
}

/// Composition node for MCP response.
#[derive(Debug, Serialize)]
pub struct CompositionNodeResponse {
    pub id: String,
    pub name: String,
    pub depth: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

impl From<CompositionNode> for CompositionNodeResponse {
    fn from(n: CompositionNode) -> Self {
        Self {
            id: n.id,
            name: n.name,
            depth: n.depth,
            category: n.category,
        }
    }
}

/// Response for get_composition_graph tool.
#[derive(Debug, Serialize)]
pub struct GetCompositionGraphResult {
    pub entity: CompositionNodeResponse,
    pub ancestors: Vec<CompositionNodeResponse>,
    pub descendants: Vec<CompositionNodeResponse>,
}

impl From<CompositionGraph> for GetCompositionGraphResult {
    fn from(g: CompositionGraph) -> Self {
        Self {
            entity: g.entity.into(),
            ancestors: g.ancestors.into_iter().map(Into::into).collect(),
            descendants: g.descendants.into_iter().map(Into::into).collect(),
        }
    }
}

/// Subgraph node for MCP response.
#[derive(Debug, Serialize)]
#[serde(tag = "node_type")]
pub enum SubgraphNodeResponse {
    Entity {
        id: String,
        name: String,
        description: String,
        distance: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        category: Option<String>,
    },
    DocumentReference {
        id: String,
        document_path: String,
        start_line: u32,
        end_line: u32,
        description: String,
        distance: u32,
    },
}

impl From<SubgraphNode> for SubgraphNodeResponse {
    fn from(n: SubgraphNode) -> Self {
        match n {
            SubgraphNode::Entity {
                id,
                name,
                description,
                distance,
                category,
            } => SubgraphNodeResponse::Entity {
                id,
                name,
                description,
                distance,
                category,
            },
            SubgraphNode::DocumentReference {
                id,
                document_path,
                start_line,
                end_line,
                description,
                distance,
            } => SubgraphNodeResponse::DocumentReference {
                id,
                document_path,
                start_line,
                end_line,
                description,
                distance,
            },
        }
    }
}

/// Subgraph edge for MCP response.
#[derive(Debug, Serialize)]
pub struct SubgraphEdgeResponse {
    pub from_id: String,
    pub to_id: String,
    pub relationship: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl From<SubgraphEdge> for SubgraphEdgeResponse {
    fn from(e: SubgraphEdge) -> Self {
        Self {
            from_id: e.from_id,
            to_id: e.to_id,
            relationship: e.relationship,
            note: e.note,
        }
    }
}

/// Response for query_subgraph tool.
#[derive(Debug, Serialize)]
pub struct QuerySubgraphResult {
    pub nodes: Vec<SubgraphNodeResponse>,
    pub edges: Vec<SubgraphEdgeResponse>,
}

impl From<Subgraph> for QuerySubgraphResult {
    fn from(s: Subgraph) -> Self {
        Self {
            nodes: s.nodes.into_iter().map(Into::into).collect(),
            edges: s.edges.into_iter().map(Into::into).collect(),
        }
    }
}

/// Document search result for MCP response.
#[derive(Debug, Serialize)]
pub struct DocumentSearchResultResponse {
    pub reference_id: String,
    pub entity_id: String,
    pub entity_name: String,
    pub document_path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub description: String,
    pub score: f32,
}

impl From<SearchResult<EntityWithReference>> for DocumentSearchResultResponse {
    fn from(r: SearchResult<EntityWithReference>) -> Self {
        Self {
            reference_id: r.item.reference.id,
            entity_id: r.item.entity.id,
            entity_name: r.item.entity.name,
            document_path: r.item.reference.document_path,
            start_line: r.item.reference.start_line,
            end_line: r.item.reference.end_line,
            description: r.item.reference.description,
            score: r.score,
        }
    }
}

/// Response for search_documents tool.
#[derive(Debug, Serialize)]
pub struct SearchDocumentsResult {
    pub results: Vec<DocumentSearchResultResponse>,
    pub count: usize,
}

/// Entity search result for MCP response.
#[derive(Debug, Serialize)]
pub struct EntitySearchResultResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

/// Response for semantic_search tool.
#[derive(Debug, Serialize)]
pub struct SemanticSearchResult {
    pub results: Vec<EntitySearchResultResponse>,
    pub count: usize,
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
        let response = GetEntityResult {
            entity: entity.into(),
        };

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::to_value(response).unwrap(),
        )
        .unwrap()]))
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

        let count = entities.len();
        let response = FindEntitiesResult {
            entities: entities.into_iter().map(Into::into).collect(),
            count,
        };

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::to_value(response).unwrap(),
        )
        .unwrap()]))
    }

    /// Get all entities with references to a document.
    #[tool(description = "Get all entities in a document with their reference details.")]
    pub async fn get_document_entities(
        &self,
        Parameters(params): Parameters<GetDocumentEntitiesParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(path = %params.document_path, "Running get_document_entities tool");

        let service = self.resolve::<GraphService>();
        let entities = service
            .get_document_entities(&params.document_path)
            .await?;

        let count = entities.len();
        let response = GetDocumentEntitiesResult {
            document_path: params.document_path,
            entities: entities.into_iter().map(Into::into).collect(),
            count,
        };

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::to_value(response).unwrap(),
        )
        .unwrap()]))
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

        let response: GetCompositionGraphResult = graph.into();

        tracing::info!(
            id = %params.entity_id,
            ancestors = response.ancestors.len(),
            descendants = response.descendants.len(),
            "Retrieved composition graph"
        );

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::to_value(response).unwrap(),
        )
        .unwrap()]))
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

        let service = self.resolve::<GraphService>();
        let subgraph = service
            .query_subgraph(
                &params.entity_id,
                params.hops.unwrap_or(2),
                params.relationship_types,
            )
            .await?;

        let response: QuerySubgraphResult = subgraph.into();

        tracing::info!(
            nodes = response.nodes.len(),
            edges = response.edges.len(),
            "Retrieved subgraph"
        );

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::to_value(response).unwrap(),
        )
        .unwrap()]))
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

        let count = results.len();
        let response = SearchDocumentsResult {
            results: results.into_iter().map(Into::into).collect(),
            count,
        };

        tracing::info!(count = count, "Search completed");

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::to_value(response).unwrap(),
        )
        .unwrap()]))
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

        let count = results.len();
        let response = SemanticSearchResult {
            results: results
                .into_iter()
                .map(|r| EntitySearchResultResponse {
                    id: r.item.id,
                    name: r.item.name,
                    description: r.item.description,
                    score: r.score,
                    category: None, // Category not returned from search
                })
                .collect(),
            count,
        };

        tracing::info!(count = count, "Search completed");

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::to_value(response).unwrap(),
        )
        .unwrap()]))
    }
}
