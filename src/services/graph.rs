//! Graph service for business logic around graph queries and search.

use std::collections::{BinaryHeap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::context::{AppEmbedder, Context};
use crate::di::FromContext;
use crate::error::AppError;
use crate::models::{
    Entity, EntityWithContext, EntityWithReference, QueryEntitySummary, QueryGraph, QueryGraphEdge,
    QueryGraphNode, QueryGraphStats, SearchResult,
};
use crate::repositories::{QueryRepository, Subgraph, SubgraphNode};

// ============================================================================
// Types for Unified Search
// ============================================================================

/// What to search: entities, references, or both.
#[derive(Debug, Clone, Default)]
pub enum SearchTarget {
    Entities,
    References,
    #[default]
    All,
}

/// An entity match from unified search.
#[derive(Debug, Clone, Serialize)]
pub struct EntityMatch {
    pub id: String,
    pub name: String,
    pub description: String,
    pub score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
}

/// A reference match from unified search.
#[derive(Debug, Clone, Serialize)]
pub struct ReferenceMatch {
    pub id: String,
    pub entity_id: String,
    pub entity_name: String,
    pub document_path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub description: String,
    pub score: f32,
}

/// Result of unified search.
#[derive(Debug, Clone, Serialize)]
pub struct UnifiedSearchResult {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entities: Vec<EntityMatch>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<ReferenceMatch>,
}

// ============================================================================
// Types for Semantic Query (Best-First Search)
// ============================================================================

/// Scoring strategy for semantic subgraph extraction.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoringStrategy {
    /// Only global token accumulation affects scoring (simpler, may go deep).
    #[default]
    Global,
    /// Also penalize deep branches to encourage breadth.
    BranchPenalty,
}

/// Parameters for semantic subgraph query.
#[derive(Debug, Clone)]
pub struct SemanticQueryParams {
    /// Starting entity ID (optional - if omitted, searches for best match).
    pub entity_id: Option<String>,
    /// Semantic query for relevance scoring.
    pub semantic_query: Option<String>,
    /// Maximum nodes in result (default: 50).
    pub max_nodes: usize,
    /// Maximum estimated tokens (default: 4000).
    pub max_tokens: usize,
    /// Minimum relevance to include node (default: 0.3).
    pub min_relevance: f32,
    /// Scoring strategy.
    pub scoring_strategy: ScoringStrategy,
    /// Filter relationship types.
    pub relationship_types: Option<Vec<String>>,
}

impl Default for SemanticQueryParams {
    fn default() -> Self {
        Self {
            entity_id: None,
            semantic_query: None,
            max_nodes: 50,
            max_tokens: 4000,
            min_relevance: 0.3,
            scoring_strategy: ScoringStrategy::default(),
            relationship_types: None,
        }
    }
}

// ============================================================================
// Internal Types for BFS
// ============================================================================

/// Cached entity data during BFS traversal.
struct CacheEntry {
    entity: Entity,
    relevance: f32,
}

/// A node in the priority queue for Best-First Search.
#[derive(Debug, Clone)]
struct PQNode {
    entity_id: String,
    score: f32,
    branch_tokens: usize,
}

impl PartialEq for PQNode {
    fn eq(&self, other: &Self) -> bool {
        self.entity_id == other.entity_id
    }
}

impl Eq for PQNode {}

impl PartialOrd for PQNode {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PQNode {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Higher score = higher priority
        self.score
            .partial_cmp(&other.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

// ============================================================================
// Constants
// ============================================================================

/// Estimated tokens per character in descriptions.
const TOKENS_PER_CHAR: f32 = 0.25;

/// Branch budget for BranchPenalty strategy.
const BRANCH_BUDGET: f32 = 1000.0;

// ============================================================================
// GraphService
// ============================================================================

/// Service for graph traversal and semantic search operations.
///
/// Orchestrates the QueryRepository and AppEmbedder to provide
/// high-level graph operations with embedding support.
#[derive(FromContext, Clone)]
pub struct GraphService {
    query_repo: QueryRepository,
    embedder: AppEmbedder,
}

impl GraphService {
    /// Get entity with full context: classifications, references, and hierarchy.
    pub async fn get_entity(&self, id: &str) -> Result<EntityWithContext, AppError> {
        self.query_repo.get_entity_with_context(id).await
    }

    /// Find entities by scope, category, or parent.
    pub async fn find_entities(
        &self,
        scope: Option<&str>,
        category: Option<&str>,
        parent_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Entity>, AppError> {
        let limit = if limit == 0 { 50 } else { limit };
        self.query_repo
            .find_entities(scope, category, parent_id, limit)
            .await
    }

    /// Get all entities with references in a document.
    pub async fn get_document_entities(
        &self,
        path: &str,
    ) -> Result<Vec<EntityWithReference>, AppError> {
        self.query_repo.get_document_entities(path).await
    }

    /// Search entities by semantic similarity to a query string.
    pub async fn semantic_search(
        &self,
        query: &str,
        limit: u32,
        min_score: f32,
        scope: Option<&str>,
    ) -> Result<Vec<SearchResult<Entity>>, AppError> {
        let limit = if limit == 0 { 10 } else { limit };
        let min_score = if min_score == 0.0 { 0.5 } else { min_score };

        // Generate embedding for query
        let embedding = self
            .embedder
            .embed(query)
            .map_err(|e| AppError::Embedding(e.to_string()))?;
        let embedding_f64: Vec<f64> = embedding.iter().map(|&f| f as f64).collect();

        self.query_repo
            .search_entities_by_embedding(&embedding_f64, limit, min_score, scope)
            .await
    }

    /// Search document references by semantic similarity to a query string.
    pub async fn search_documents(
        &self,
        query: &str,
        limit: u32,
        min_score: f32,
    ) -> Result<Vec<SearchResult<EntityWithReference>>, AppError> {
        let limit = if limit == 0 { 10 } else { limit };
        let min_score = if min_score == 0.0 { 0.5 } else { min_score };

        // Generate embedding for query
        let embedding = self
            .embedder
            .embed(query)
            .map_err(|e| AppError::Embedding(e.to_string()))?;
        let embedding_f64: Vec<f64> = embedding.iter().map(|&f| f as f64).collect();

        self.query_repo
            .search_documents_by_embedding(&embedding_f64, limit, min_score)
            .await
    }

    // ========================================================================
    // New Unified Search & Query Methods
    // ========================================================================

    /// Unified semantic search across entities and/or references.
    pub async fn unified_search(
        &self,
        query: &str,
        target: SearchTarget,
        limit: u32,
        min_score: f32,
        scope: Option<&str>,
    ) -> Result<UnifiedSearchResult, AppError> {
        let limit = if limit == 0 { 20 } else { limit };
        let min_score = if min_score == 0.0 { 0.3 } else { min_score };

        let mut result = UnifiedSearchResult {
            entities: Vec::new(),
            references: Vec::new(),
        };

        // Search entities if target includes them
        if matches!(target, SearchTarget::Entities | SearchTarget::All) {
            let entity_results = self.semantic_search(query, limit, min_score, scope).await?;

            result.entities = entity_results
                .into_iter()
                .map(|r| EntityMatch {
                    id: r.item.id,
                    name: r.item.name,
                    description: r.item.description,
                    score: r.score,
                    scope: None,
                    categories: Vec::new(),
                })
                .collect();
        }

        // Search references if target includes them
        if matches!(target, SearchTarget::References | SearchTarget::All) {
            let ref_results = self.search_documents(query, limit, min_score).await?;

            result.references = ref_results
                .into_iter()
                .map(|r| {
                    let (doc_path, start_line, end_line, ref_id) = match &r.item.reference {
                        crate::models::Reference::Code(c) => {
                            (c.path.clone(), 0u32, 0u32, c.id.clone())
                        }
                        crate::models::Reference::Text(t) => {
                            (t.path.clone(), t.start_line, t.end_line, t.id.clone())
                        }
                    };
                    ReferenceMatch {
                        id: ref_id,
                        entity_id: r.item.entity.id.clone(),
                        entity_name: r.item.entity.name.clone(),
                        document_path: doc_path,
                        start_line,
                        end_line,
                        description: r.item.entity.description.clone(),
                        score: r.score,
                    }
                })
                .collect();
        }

        Ok(result)
    }

    /// Semantic subgraph extraction with Best-First Search.
    ///
    /// Returns an optimized subgraph within budget constraints using
    /// relevance-based pruning.
    pub async fn semantic_query(
        &self,
        params: SemanticQueryParams,
    ) -> Result<QueryGraph, AppError> {
        // Validate: need at least one of entity_id or semantic_query
        if params.entity_id.is_none() && params.semantic_query.is_none() {
            return Err(AppError::Validation(
                "Either entity_id or semantic_query must be provided".to_string(),
            ));
        }

        // Determine starting entity and query text
        let (start_entity, query_text) = self.resolve_start_entity(&params).await?;

        // Generate query embedding
        let query_embedding = self
            .embedder
            .embed(&query_text)
            .map_err(|e| AppError::Embedding(e.to_string()))?;

        // Run Best-First Search
        let (visited, edges, entity_cache, stats) = self
            .best_first_search(&start_entity, &query_embedding, &params)
            .await?;

        // Build result
        let result = self
            .build_query_result(start_entity, visited, edges, entity_cache, stats)
            .await;

        Ok(result)
    }

    // ========================================================================
    // Private Helper Methods
    // ========================================================================

    /// Resolve the starting entity and query text from params.
    async fn resolve_start_entity(
        &self,
        params: &SemanticQueryParams,
    ) -> Result<(Entity, String), AppError> {
        match (&params.entity_id, &params.semantic_query) {
            (Some(id), Some(q)) => {
                // Have both: use entity as start, query for scoring
                let entity = self.get_entity(id).await?;
                Ok((entity.entity, q.clone()))
            }
            (Some(id), None) => {
                // Entity only: use its description as query
                let entity = self.get_entity(id).await?;
                let query = entity.entity.description.clone();
                Ok((entity.entity, query))
            }
            (None, Some(q)) => {
                // Query only: find best matching entity
                let results = self.semantic_search(q, 1, 0.0, None).await?;
                if results.is_empty() {
                    return Err(AppError::Validation(
                        "No entities found matching the semantic query".to_string(),
                    ));
                }
                Ok((results[0].item.clone(), q.clone()))
            }
            (None, None) => unreachable!(), // Already validated
        }
    }

    /// Run Best-First Search algorithm.
    async fn best_first_search(
        &self,
        start_entity: &Entity,
        query_embedding: &[f32],
        params: &SemanticQueryParams,
    ) -> Result<
        (
            HashSet<String>,
            Vec<QueryGraphEdge>,
            HashMap<String, CacheEntry>,
            QueryGraphStats,
        ),
        AppError,
    > {
        let mut pq = BinaryHeap::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut total_tokens = 0usize;
        let mut nodes_pruned = 0usize;
        let mut entity_cache: HashMap<String, CacheEntry> = HashMap::new();
        let mut edges: Vec<QueryGraphEdge> = Vec::new();

        // Calculate initial relevance for start entity
        let start_relevance = start_entity
            .embedding
            .as_ref()
            .map(|emb| cosine_similarity(emb, query_embedding))
            .unwrap_or(1.0);

        // Add start entity to cache
        entity_cache.insert(
            start_entity.id.clone(),
            CacheEntry {
                entity: start_entity.clone(),
                relevance: start_relevance,
            },
        );

        // Push start node to priority queue
        pq.push(PQNode {
            entity_id: start_entity.id.clone(),
            score: start_relevance,
            branch_tokens: 0,
        });

        while let Some(current) = pq.pop() {
            if visited.contains(&current.entity_id) {
                continue;
            }

            // Get entity from cache
            let cache_entry = entity_cache
                .get(&current.entity_id)
                .ok_or_else(|| AppError::Internal("Entity not in cache during BFS".to_string()))?;

            let entity_tokens = estimate_tokens(&cache_entry.entity);

            // Check token budget
            if total_tokens + entity_tokens > params.max_tokens {
                nodes_pruned += 1;
                continue;
            }

            // Check node limit
            if visited.len() >= params.max_nodes {
                nodes_pruned += 1;
                continue;
            }

            // Mark as visited and update budget
            visited.insert(current.entity_id.clone());
            total_tokens += entity_tokens;

            // Get 1-hop neighbors from the graph
            let subgraph = self
                .query_repo
                .query_subgraph(&current.entity_id, 1, params.relationship_types.as_deref())
                .await?;

            // Process each neighbor
            for edge in &subgraph.edges {
                let neighbor_id = if edge.from_id == current.entity_id {
                    &edge.to_id
                } else {
                    &edge.from_id
                };

                if visited.contains(neighbor_id) {
                    continue;
                }

                // Get neighbor entity data (skip if not found - dangling relationship)
                let neighbor_entity = match self
                    .get_or_fetch_entity(neighbor_id, &subgraph, &mut entity_cache)
                    .await
                {
                    Ok(entity) => entity,
                    Err(AppError::EntityNotFound(_)) => {
                        tracing::warn!(
                            neighbor_id,
                            "Skipping dangling relationship to non-existent entity"
                        );
                        continue;
                    }
                    Err(e) => return Err(e),
                };

                // Add edge (only after confirming neighbor exists)
                edges.push(QueryGraphEdge {
                    from_id: edge.from_id.clone(),
                    to_id: edge.to_id.clone(),
                    relationship: edge.relationship.clone(),
                    note: edge.note.clone(),
                    relevance: current.score,
                });

                // Calculate relevance
                let neighbor_relevance = neighbor_entity
                    .embedding
                    .as_ref()
                    .map(|emb| cosine_similarity(emb, query_embedding))
                    .unwrap_or(0.5);

                // Check minimum relevance
                if neighbor_relevance < params.min_relevance {
                    nodes_pruned += 1;
                    continue;
                }

                // Calculate final score
                let neighbor_tokens = estimate_tokens(&neighbor_entity);
                let final_score = self.calculate_score(
                    neighbor_relevance,
                    neighbor_tokens,
                    total_tokens,
                    current.branch_tokens,
                    params,
                );

                // Cache the entity
                entity_cache.insert(
                    neighbor_entity.id.clone(),
                    CacheEntry {
                        entity: neighbor_entity.clone(),
                        relevance: neighbor_relevance,
                    },
                );

                // Add to priority queue
                pq.push(PQNode {
                    entity_id: neighbor_entity.id.clone(),
                    score: final_score,
                    branch_tokens: current.branch_tokens + neighbor_tokens,
                });
            }
        }

        let stats = QueryGraphStats {
            nodes_visited: visited.len(),
            nodes_pruned,
            estimated_tokens: total_tokens,
        };

        Ok((visited, edges, entity_cache, stats))
    }

    /// Get entity from cache or fetch from subgraph/database.
    async fn get_or_fetch_entity(
        &self,
        entity_id: &str,
        subgraph: &Subgraph,
        cache: &mut HashMap<String, CacheEntry>,
    ) -> Result<Entity, AppError> {
        // Check cache first
        if let Some(entry) = cache.get(entity_id) {
            return Ok(entry.entity.clone());
        }

        // Try to find in subgraph response
        let subgraph_entity = subgraph.nodes.iter().find_map(|n| match n {
            SubgraphNode::Entity {
                id,
                name,
                description,
                ..
            } if id == entity_id => Some((name.clone(), description.clone())),
            _ => None,
        });

        // Fetch full entity from database to get embedding
        match self.get_entity(entity_id).await {
            Ok(ctx) => Ok(ctx.entity),
            Err(_) => {
                // Fallback: create entity from subgraph data without embedding
                if let Some((name, description)) = subgraph_entity {
                    Ok(Entity {
                        id: entity_id.to_string(),
                        name,
                        description,
                        embedding: None,
                        created_at: chrono::Utc::now(),
                    })
                } else {
                    Err(AppError::EntityNotFound(entity_id.to_string()))
                }
            }
        }
    }

    /// Calculate score based on scoring strategy.
    fn calculate_score(
        &self,
        relevance: f32,
        node_tokens: usize,
        total_tokens: usize,
        branch_tokens: usize,
        params: &SemanticQueryParams,
    ) -> f32 {
        let global_factor = 1.0 / (1.0 + (total_tokens as f32) / (params.max_tokens as f32));

        match params.scoring_strategy {
            ScoringStrategy::Global => relevance * global_factor / (node_tokens as f32).max(1.0),
            ScoringStrategy::BranchPenalty => {
                let branch_factor = 1.0 / (1.0 + (branch_tokens as f32) / BRANCH_BUDGET);
                relevance * global_factor * branch_factor / (node_tokens as f32).max(1.0)
            }
        }
    }

    /// Build the final query result from BFS output.
    async fn build_query_result(
        &self,
        start_entity: Entity,
        visited: HashSet<String>,
        edges: Vec<QueryGraphEdge>,
        entity_cache: HashMap<String, CacheEntry>,
        stats: QueryGraphStats,
    ) -> QueryGraph {
        let mut nodes: Vec<QueryGraphNode> = Vec::new();
        let mut result_edges = edges;
        let mut seen_refs: HashSet<String> = HashSet::new();

        // Convert visited entities to nodes and fetch their references
        for id in &visited {
            if let Some(entry) = entity_cache.get(id) {
                // Fetch full context for scope and references
                let ctx = self.get_entity(id).await.ok();
                let scope = ctx
                    .as_ref()
                    .and_then(|c| c.classifications.first())
                    .map(|c| c.scope.clone());

                // Add entity node
                nodes.push(QueryGraphNode::Entity {
                    id: entry.entity.id.clone(),
                    name: entry.entity.name.clone(),
                    description: entry.entity.description.clone(),
                    scope,
                    relevance: entry.relevance,
                });

                // Add references from context
                if let Some(ctx) = ctx {
                    for reference in ctx.references {
                        let (ref_id, doc_path, start_line, end_line, description) = match &reference
                        {
                            crate::models::Reference::Code(r) => {
                                // Parse lsp_range JSON to extract line numbers
                                let (start, end) = parse_lsp_range(&r.lsp_range);
                                (
                                    r.id.clone(),
                                    r.path.clone(),
                                    start,
                                    end,
                                    r.description.clone(),
                                )
                            }
                            crate::models::Reference::Text(r) => (
                                r.id.clone(),
                                r.path.clone(),
                                r.start_line,
                                r.end_line,
                                r.description.clone(),
                            ),
                        };

                        // Only add each reference once
                        if seen_refs.insert(ref_id.clone()) {
                            nodes.push(QueryGraphNode::Reference {
                                id: ref_id.clone(),
                                document_path: doc_path,
                                start_line,
                                end_line,
                                description,
                                relevance: entry.relevance, // Inherit from parent entity
                            });

                            // Add HAS_REFERENCE edge
                            result_edges.push(QueryGraphEdge {
                                from_id: id.clone(),
                                to_id: ref_id,
                                relationship: "HAS_REFERENCE".to_string(),
                                note: None,
                                relevance: entry.relevance,
                            });
                        }
                    }
                }
            }
        }

        // Sort entities by relevance (references stay after their parent entities)
        nodes.sort_by(|a, b| {
            let rel_a = match a {
                QueryGraphNode::Entity { relevance, .. } => *relevance,
                QueryGraphNode::Reference { relevance, .. } => *relevance,
            };
            let rel_b = match b {
                QueryGraphNode::Entity { relevance, .. } => *relevance,
                QueryGraphNode::Reference { relevance, .. } => *relevance,
            };
            rel_b
                .partial_cmp(&rel_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Deduplicate edges (entity edges only - HAS_REFERENCE edges are already unique)
        let mut seen_edges: HashSet<String> = HashSet::new();
        let final_edges: Vec<QueryGraphEdge> = result_edges
            .into_iter()
            .filter(|e| {
                let key = format!("{}-{}-{}", e.from_id, e.relationship, e.to_id);
                if seen_edges.contains(&key) {
                    false
                } else {
                    // For entity-to-entity edges, both must be in visited set
                    // For HAS_REFERENCE edges, from must be in visited (to is a reference)
                    let is_valid = if e.relationship == "HAS_REFERENCE" {
                        visited.contains(&e.from_id)
                    } else {
                        visited.contains(&e.from_id) && visited.contains(&e.to_id)
                    };
                    if is_valid {
                        seen_edges.insert(key);
                        true
                    } else {
                        false
                    }
                }
            })
            .collect();

        QueryGraph {
            root_entity: QueryEntitySummary {
                id: start_entity.id,
                name: start_entity.name,
                description: start_entity.description,
                scope: None,
                category: None,
            },
            nodes,
            edges: final_edges,
            stats,
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Estimate token count for an entity.
fn estimate_tokens(entity: &Entity) -> usize {
    let char_count = entity.name.len() + entity.description.len();
    (char_count as f32 * TOKENS_PER_CHAR).ceil() as usize
}

/// Parse LSP range to extract start and end line numbers.
///
/// Supports formats:
/// - Simple: "startLine:startChar-endLine:endChar" (e.g., "173:0-247:0")
/// - JSON: {"start":{"line":X,"character":Y},"end":{"line":Z,"character":W}}
///
/// Returns (start_line, end_line) or (1, 1) if parsing fails.
fn parse_lsp_range(lsp_range: &str) -> (u32, u32) {
    // Try simple format first: "startLine:startChar-endLine:endChar"
    if let Some((start_part, end_part)) = lsp_range.split_once('-') {
        if let (Some(start_line), Some(end_line)) = (
            start_part
                .split(':')
                .next()
                .and_then(|s| s.parse::<u32>().ok()),
            end_part
                .split(':')
                .next()
                .and_then(|s| s.parse::<u32>().ok()),
        ) {
            // Already 1-indexed in simple format
            return (start_line, end_line);
        }
    }

    // Try JSON format: {"start":{"line":X,"character":Y},"end":{"line":Z,"character":W}}
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(lsp_range) {
        if let (Some(start), Some(end)) = (
            value
                .get("start")
                .and_then(|s| s.get("line"))
                .and_then(|l| l.as_u64()),
            value
                .get("end")
                .and_then(|e| e.get("line"))
                .and_then(|l| l.as_u64()),
        ) {
            // LSP is 0-indexed, convert to 1-indexed
            return (start as u32 + 1, end as u32 + 1);
        }
    }

    // Fallback
    (1, 1)
}

/// Calculate cosine similarity between two embeddings.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}
