//! Graph service for business logic around graph queries and search.

use crate::context::{AppEmbedder, Context};
use crate::di::FromContext;
use crate::error::AppError;
use crate::models::{
    CompositionGraph, Entity, EntityWithContext, EntityWithReference, SearchResult, Subgraph,
};
use crate::repositories::QueryRepository;

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

    /// Get composition graph (ancestors and descendants via BELONGS_TO).
    pub async fn get_composition_graph(
        &self,
        id: &str,
        direction: &str,
        max_depth: u32,
    ) -> Result<CompositionGraph, AppError> {
        let max_depth = if max_depth == 0 { 10 } else { max_depth };

        // Get the starting entity
        let entity = self.query_repo.get_entity_for_composition(id).await?;

        // Get ancestors and/or descendants based on direction
        let ancestors = if direction == "ancestors" || direction == "both" {
            self.query_repo
                .get_composition_ancestors(id, max_depth)
                .await?
        } else {
            Vec::new()
        };

        let descendants = if direction == "descendants" || direction == "both" {
            self.query_repo
                .get_composition_descendants(id, max_depth)
                .await?
        } else {
            Vec::new()
        };

        Ok(CompositionGraph {
            entity,
            ancestors,
            descendants,
        })
    }

    /// Query subgraph around an entity within N hops.
    pub async fn query_subgraph(
        &self,
        id: &str,
        hops: u32,
        rel_types: Option<Vec<String>>,
    ) -> Result<Subgraph, AppError> {
        let hops = if hops == 0 { 2 } else { hops };
        self.query_repo
            .query_subgraph(id, hops, rel_types.as_deref())
            .await
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
}
