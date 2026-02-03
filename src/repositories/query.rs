//! Query repository for graph traversal and search operations.

use chrono::{DateTime, Utc};

use crate::context::{AppGraph, Context};
use crate::di::FromContext;
use crate::error::AppError;
use crate::graph::{Node, Row};
use crate::models::{
    CategoryClassification, CodeReference, Entity, EntityWithContext, EntityWithReference,
    ProjectEntitySummary, Reference, SearchResult, TextReference,
};

// ============================================================================
// Internal Types for Graph Traversal
// ============================================================================

/// Node in a subgraph traversal - either an Entity or DocumentReference.
/// Internal type used by BFS algorithm.
#[derive(Debug, Clone)]
pub enum SubgraphNode {
    /// An entity node in the subgraph.
    Entity {
        /// Entity ID.
        id: String,
        /// Entity name.
        name: String,
        /// Entity description.
        description: String,
        /// Distance from the starting node.
        distance: u32,
        /// Category classification (if any).
        category: Option<String>,
    },
    /// A document reference node in the subgraph.
    DocumentReference {
        /// Reference ID.
        id: String,
        /// Path to the document.
        document_path: String,
        /// Starting line number (1-indexed).
        start_line: u32,
        /// Ending line number (1-indexed).
        end_line: u32,
        /// Description of what this reference points to.
        description: String,
        /// Distance from the starting node.
        distance: u32,
    },
}

/// Edge in a subgraph traversal.
/// Internal type used by BFS algorithm.
#[derive(Debug, Clone)]
pub struct SubgraphEdge {
    /// Source node ID.
    pub from_id: String,
    /// Target node ID.
    pub to_id: String,
    /// Relationship type (e.g., BELONGS_TO, HAS_REFERENCE, CALLS).
    pub relationship: String,
    /// Optional note on the relationship.
    pub note: Option<String>,
}

/// A complete subgraph with nodes and edges.
/// Internal type used by BFS algorithm.
#[derive(Debug, Clone)]
pub struct Subgraph {
    /// All nodes in the subgraph.
    pub nodes: Vec<SubgraphNode>,
    /// All edges in the subgraph.
    pub edges: Vec<SubgraphEdge>,
}

// ============================================================================
// Repository
// ============================================================================

/// Repository for graph traversal and search queries.
#[derive(FromContext, Clone)]
pub struct QueryRepository {
    graph: AppGraph,
}

impl QueryRepository {
    /// Get entity with full context: classifications, references, parents, children, related.
    pub async fn get_entity_with_context(&self, id: &str) -> Result<EntityWithContext, AppError> {
        // Get base entity first
        let entity_row = self
            .graph
            .query("MATCH (e:Entity {id: $id}) RETURN e")
            .param("id", id)
            .fetch_one()
            .await?
            .ok_or_else(|| AppError::EntityNotFound(id.to_string()))?;

        let entity = Self::row_to_entity(&entity_row, "e")?;

        // Get classifications
        let class_rows = self
            .graph
            .query(
                "MATCH (e:Entity {id: $id})-[:CLASSIFIED_AS]->(c:Category)-[:IN_SCOPE]->(s:Scope)
                 RETURN c.id AS id, c.name AS name, s.name AS scope",
            )
            .param("id", id)
            .fetch_all()
            .await?;

        let classifications: Vec<CategoryClassification> = class_rows
            .iter()
            .filter_map(|row| {
                let id: String = row.get("id").ok()?;
                let name: String = row.get("name").ok()?;
                let scope: String = row.get("scope").ok()?;
                if id.is_empty() {
                    None
                } else {
                    Some(CategoryClassification { id, name, scope })
                }
            })
            .collect();

        // Get code references
        let code_rows = self
            .graph
            .query(
                "MATCH (e:Entity {id: $id})-[:HAS_REFERENCE]->(ref:CodeReference)
                 RETURN ref",
            )
            .param("id", id)
            .fetch_all()
            .await?;

        let mut references: Vec<Reference> = code_rows
            .iter()
            .filter_map(|row| Self::row_to_code_reference(row, "ref").ok())
            .map(Reference::Code)
            .collect();

        // Get text references
        let text_rows = self
            .graph
            .query(
                "MATCH (e:Entity {id: $id})-[:HAS_REFERENCE]->(ref:TextReference)
                 RETURN ref",
            )
            .param("id", id)
            .fetch_all()
            .await?;

        references.extend(
            text_rows
                .iter()
                .filter_map(|row| Self::row_to_text_reference(row, "ref").ok())
                .map(Reference::Text),
        );

        // Get parents
        let parent_rows = self
            .graph
            .query(
                "MATCH (e:Entity {id: $id})-[:BELONGS_TO]->(parent:Entity)
                 RETURN parent",
            )
            .param("id", id)
            .fetch_all()
            .await?;

        let parents: Vec<Entity> = parent_rows
            .iter()
            .filter_map(|row| Self::row_to_entity(row, "parent").ok())
            .collect();

        // Get children
        let child_rows = self
            .graph
            .query(
                "MATCH (child:Entity)-[:BELONGS_TO]->(e:Entity {id: $id})
                 RETURN child",
            )
            .param("id", id)
            .fetch_all()
            .await?;

        let children: Vec<Entity> = child_rows
            .iter()
            .filter_map(|row| Self::row_to_entity(row, "child").ok())
            .collect();

        // Get related
        let related_rows = self
            .graph
            .query(
                "MATCH (e:Entity {id: $id})-[:RELATED_TO]->(related:Entity)
                 RETURN related",
            )
            .param("id", id)
            .fetch_all()
            .await?;

        let related: Vec<Entity> = related_rows
            .iter()
            .filter_map(|row| Self::row_to_entity(row, "related").ok())
            .collect();

        Ok(EntityWithContext {
            entity,
            classifications,
            references,
            parents,
            children,
            related,
        })
    }

    /// Find entities by scope, category, or parent.
    pub async fn find_entities(
        &self,
        scope: Option<&str>,
        category: Option<&str>,
        parent_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Entity>, AppError> {
        let limit = limit.min(100) as i64;

        // Build query based on filters
        let query_str = match (scope, category, parent_id) {
            (Some(_), Some(_), Some(_)) => {
                "MATCH (e:Entity)-[:CLASSIFIED_AS]->(c:Category {name: $category})-[:IN_SCOPE]->(s:Scope {name: $scope})
                 MATCH (e)-[:BELONGS_TO]->(parent:Entity {id: $parent_id})
                 RETURN e ORDER BY e.name LIMIT $limit"
            }
            (Some(_), Some(_), None) => {
                "MATCH (e:Entity)-[:CLASSIFIED_AS]->(c:Category {name: $category})-[:IN_SCOPE]->(s:Scope {name: $scope})
                 RETURN e ORDER BY e.name LIMIT $limit"
            }
            (Some(_), None, Some(_)) => {
                "MATCH (e:Entity)-[:CLASSIFIED_AS]->(c:Category)-[:IN_SCOPE]->(s:Scope {name: $scope})
                 MATCH (e)-[:BELONGS_TO]->(parent:Entity {id: $parent_id})
                 RETURN e ORDER BY e.name LIMIT $limit"
            }
            (None, Some(_), Some(_)) => {
                "MATCH (e:Entity)-[:CLASSIFIED_AS]->(c:Category {name: $category})
                 MATCH (e)-[:BELONGS_TO]->(parent:Entity {id: $parent_id})
                 RETURN e ORDER BY e.name LIMIT $limit"
            }
            (Some(_), None, None) => {
                "MATCH (e:Entity)-[:CLASSIFIED_AS]->(c:Category)-[:IN_SCOPE]->(s:Scope {name: $scope})
                 RETURN e ORDER BY e.name LIMIT $limit"
            }
            (None, Some(_), None) => {
                "MATCH (e:Entity)-[:CLASSIFIED_AS]->(c:Category {name: $category})
                 RETURN e ORDER BY e.name LIMIT $limit"
            }
            (None, None, Some(_)) => {
                "MATCH (e:Entity)-[:BELONGS_TO]->(parent:Entity {id: $parent_id})
                 RETURN e ORDER BY e.name LIMIT $limit"
            }
            (None, None, None) => "MATCH (e:Entity) RETURN e ORDER BY e.name LIMIT $limit",
        };

        let mut q = self.graph.query(query_str).param("limit", limit);

        if let Some(scope) = scope {
            q = q.param("scope", scope);
        }
        if let Some(category) = category {
            q = q.param("category", category);
        }
        if let Some(parent_id) = parent_id {
            q = q.param("parent_id", parent_id);
        }

        let rows = q.fetch_all().await?;

        rows.iter()
            .map(|row| Self::row_to_entity(row, "e"))
            .collect()
    }

    /// Get all entities with references in a document.
    pub async fn get_document_entities(
        &self,
        path: &str,
    ) -> Result<Vec<EntityWithReference>, AppError> {
        let mut entities = Vec::new();

        // Get CodeReferences
        let code_rows = self
            .graph
            .query(
                "MATCH (e:Entity)-[:HAS_REFERENCE]->(ref:CodeReference)-[:IN_DOCUMENT]->(d:Document {path: $path})
                 RETURN e, ref
                 ORDER BY ref.lsp_symbol",
            )
            .param("path", path)
            .fetch_all()
            .await?;

        for row in &code_rows {
            let entity = Self::row_to_entity(row, "e")?;
            let reference = Reference::Code(Self::row_to_code_reference(row, "ref")?);
            entities.push(EntityWithReference { entity, reference });
        }

        // Get TextReferences
        let text_rows = self
            .graph
            .query(
                "MATCH (e:Entity)-[:HAS_REFERENCE]->(ref:TextReference)-[:IN_DOCUMENT]->(d:Document {path: $path})
                 RETURN e, ref
                 ORDER BY ref.start_line",
            )
            .param("path", path)
            .fetch_all()
            .await?;

        for row in &text_rows {
            let entity = Self::row_to_entity(row, "e")?;
            let reference = Reference::Text(Self::row_to_text_reference(row, "ref")?);
            entities.push(EntityWithReference { entity, reference });
        }

        Ok(entities)
    }

    /// Query subgraph around an entity within N hops.
    ///
    /// Note: This is a simplified implementation for AGE. The full path-based
    /// traversal with variable-length relationships is limited in AGE.
    pub async fn query_subgraph(
        &self,
        id: &str,
        _hops: u32,
        _rel_types: Option<&[String]>,
    ) -> Result<Subgraph, AppError> {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut seen_nodes = std::collections::HashSet::new();

        // Add starting node
        let start_row = self
            .graph
            .query(
                "MATCH (e:Entity {id: $id})
                 OPTIONAL MATCH (e)-[:CLASSIFIED_AS]->(c:Category)
                 RETURN e, c.name AS category",
            )
            .param("id", id)
            .fetch_one()
            .await?;

        if let Some(row) = start_row {
            let node: Node = row.get("e")?;
            let node_id: String = node.get("id")?;
            let category: Option<String> = row.get_opt("category")?;

            seen_nodes.insert(node_id.clone());
            nodes.push(SubgraphNode::Entity {
                id: node_id,
                name: node.get_opt("name")?.unwrap_or_default(),
                description: node.get_opt("description")?.unwrap_or_default(),
                distance: 0,
                category,
            });
        }

        // Get direct neighbors (1 hop) - simplified for AGE
        let neighbor_rows = self
            .graph
            .query(
                "MATCH (start:Entity {id: $id})-[r]-(neighbor:Entity)
                 OPTIONAL MATCH (neighbor)-[:CLASSIFIED_AS]->(c:Category)
                 RETURN DISTINCT neighbor,
                        CASE WHEN type(r) = 'LINK' THEN r.type ELSE type(r) END AS rel_type,
                        c.name AS category,
                        startNode(r).id AS from_id, endNode(r).id AS to_id,
                        r.note AS note",
            )
            .param("id", id)
            .fetch_all()
            .await?;

        for row in &neighbor_rows {
            let node: Node = row.get("neighbor")?;
            let node_id: String = node.get("id")?;

            if !seen_nodes.contains(&node_id) {
                seen_nodes.insert(node_id.clone());
                let category: Option<String> = row.get_opt("category")?;
                nodes.push(SubgraphNode::Entity {
                    id: node_id.clone(),
                    name: node.get_opt("name")?.unwrap_or_default(),
                    description: node.get_opt("description")?.unwrap_or_default(),
                    distance: 1,
                    category,
                });
            }

            let rel_type: String = row.get("rel_type")?;
            let from_id: String = row.get("from_id")?;
            let to_id: String = row.get("to_id")?;
            let note: Option<String> = row.get_opt("note")?;

            edges.push(SubgraphEdge {
                from_id,
                to_id,
                relationship: rel_type,
                note,
            });
        }

        Ok(Subgraph { nodes, edges })
    }

    /// Search entities by embedding similarity.
    ///
    /// Fetches entities with embeddings and computes cosine similarity in Rust.
    pub async fn search_entities_by_embedding(
        &self,
        embedding: &[f64],
        limit: u32,
        min_score: f32,
        scope: Option<&str>,
    ) -> Result<Vec<SearchResult<Entity>>, AppError> {
        let limit = limit.min(50) as usize;

        let query_str = if scope.is_some() {
            "MATCH (e:Entity)-[:CLASSIFIED_AS]->(c:Category)-[:IN_SCOPE]->(s:Scope {name: $scope})
             WHERE e.embedding IS NOT NULL
             RETURN e"
        } else {
            "MATCH (e:Entity)
             WHERE e.embedding IS NOT NULL
             RETURN e"
        };

        let mut q = self.graph.query(query_str);
        if let Some(scope) = scope {
            q = q.param("scope", scope);
        }

        let rows = q.fetch_all().await?;

        let mut results: Vec<SearchResult<Entity>> = rows
            .iter()
            .filter_map(|row| {
                let entity = Self::row_to_entity(row, "e").ok()?;
                let entity_embedding = entity.embedding.as_ref()?;
                let entity_embedding_f64: Vec<f64> =
                    entity_embedding.iter().map(|&f| f as f64).collect();
                let score = cosine_similarity(embedding, &entity_embedding_f64) as f32;
                if score >= min_score {
                    Some(SearchResult {
                        item: entity,
                        score,
                    })
                } else {
                    None
                }
            })
            .collect();

        // Sort by score descending
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);

        Ok(results)
    }

    /// Get entity summaries by scope with category info.
    /// Returns entities with their primary category for project overview.
    pub async fn get_entity_summaries_by_scope(
        &self,
        scope: &str,
    ) -> Result<Vec<ProjectEntitySummary>, AppError> {
        let rows = self
            .graph
            .query(
                "MATCH (e:Entity)-[:CLASSIFIED_AS]->(c:Category)-[:IN_SCOPE]->(s:Scope {name: $scope})
                 OPTIONAL MATCH (e)-[:BELONGS_TO]->(parent:Entity)
                 RETURN e.id AS id, e.name AS name, e.description AS description,
                        c.name AS category, parent.id AS parent_id
                 ORDER BY e.name",
            )
            .param("scope", scope)
            .fetch_all()
            .await?;

        let mut summaries = Vec::new();
        for row in &rows {
            summaries.push(ProjectEntitySummary {
                id: row.get_opt("id")?.unwrap_or_default(),
                name: row.get_opt("name")?.unwrap_or_default(),
                description: row.get_opt("description")?.unwrap_or_default(),
                category: row.get_opt("category")?,
                parent_id: row.get_opt("parent_id")?,
            });
        }

        Ok(summaries)
    }

    /// Search document references by embedding similarity.
    ///
    /// Fetches references with embeddings and computes cosine similarity in Rust.
    pub async fn search_documents_by_embedding(
        &self,
        embedding: &[f64],
        limit: u32,
        min_score: f32,
    ) -> Result<Vec<SearchResult<EntityWithReference>>, AppError> {
        let limit = limit.min(50) as usize;

        let mut results = Vec::new();

        // Search CodeReferences
        let code_rows = self
            .graph
            .query(
                "MATCH (e:Entity)-[:HAS_REFERENCE]->(ref:CodeReference)
                 WHERE ref.embedding IS NOT NULL
                 RETURN e, ref",
            )
            .fetch_all()
            .await?;

        for row in &code_rows {
            if let (Ok(entity), Ok(code_ref)) = (
                Self::row_to_entity(row, "e"),
                Self::row_to_code_reference(row, "ref"),
            ) {
                if let Some(ref_embedding) = &code_ref.embedding {
                    let ref_embedding_f64: Vec<f64> =
                        ref_embedding.iter().map(|&f| f as f64).collect();
                    let score = cosine_similarity(embedding, &ref_embedding_f64) as f32;
                    if score >= min_score {
                        results.push(SearchResult {
                            item: EntityWithReference {
                                entity,
                                reference: Reference::Code(code_ref),
                            },
                            score,
                        });
                    }
                }
            }
        }

        // Search TextReferences
        let text_rows = self
            .graph
            .query(
                "MATCH (e:Entity)-[:HAS_REFERENCE]->(ref:TextReference)
                 WHERE ref.embedding IS NOT NULL
                 RETURN e, ref",
            )
            .fetch_all()
            .await?;

        for row in &text_rows {
            if let (Ok(entity), Ok(text_ref)) = (
                Self::row_to_entity(row, "e"),
                Self::row_to_text_reference(row, "ref"),
            ) {
                if let Some(ref_embedding) = &text_ref.embedding {
                    let ref_embedding_f64: Vec<f64> =
                        ref_embedding.iter().map(|&f| f as f64).collect();
                    let score = cosine_similarity(embedding, &ref_embedding_f64) as f32;
                    if score >= min_score {
                        results.push(SearchResult {
                            item: EntityWithReference {
                                entity,
                                reference: Reference::Text(text_ref),
                            },
                            score,
                        });
                    }
                }
            }
        }

        // Sort by score descending and limit
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);

        Ok(results)
    }

    // ============================================================================
    // Helper methods
    // ============================================================================

    /// Convert a row to an Entity.
    fn row_to_entity(row: &Row, field: &str) -> Result<Entity, AppError> {
        let node: Node = row.get(field)?;

        let id: String = node.get("id")?;
        let name: String = node.get_opt("name")?.unwrap_or_default();
        let description: String = node.get_opt("description")?.unwrap_or_default();

        let embedding: Option<Vec<f64>> = node.get_opt("embedding")?;
        let embedding = embedding.map(|e| e.iter().map(|&f| f as f32).collect());

        let created_at: DateTime<Utc> = node
            .get_opt::<String>("created_at")?
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        Ok(Entity {
            id,
            name,
            description,
            embedding,
            created_at,
        })
    }

    /// Convert a row to a CodeReference.
    fn row_to_code_reference(row: &Row, field: &str) -> Result<CodeReference, AppError> {
        let node: Node = row.get(field)?;

        let embedding: Option<Vec<f64>> = node.get_opt("embedding")?;
        let embedding = embedding.map(|e| e.iter().map(|&f| f as f32).collect());

        Ok(CodeReference {
            id: node.get("id")?,
            source_id: node.get_opt("source_id")?.unwrap_or_default(),
            path: node.get_opt("path")?.unwrap_or_default(),
            language: node.get_opt("language")?.unwrap_or_default(),
            commit_sha: node.get_opt("commit_sha")?.unwrap_or_default(),
            description: node.get_opt("description")?.unwrap_or_default(),
            embedding,
            lsp_symbol: node.get_opt("lsp_symbol")?.unwrap_or_default(),
            lsp_kind: node.get_opt::<i64>("lsp_kind")?.unwrap_or(0) as i32,
            lsp_range: node.get_opt("lsp_range")?.unwrap_or_default(),
        })
    }

    /// Convert a row to a TextReference.
    fn row_to_text_reference(row: &Row, field: &str) -> Result<TextReference, AppError> {
        let node: Node = row.get(field)?;

        let embedding: Option<Vec<f64>> = node.get_opt("embedding")?;
        let embedding = embedding.map(|e| e.iter().map(|&f| f as f32).collect());

        Ok(TextReference {
            id: node.get("id")?,
            source_id: node.get_opt("source_id")?.unwrap_or_default(),
            path: node.get_opt("path")?.unwrap_or_default(),
            content_type: node
                .get_opt("content_type")?
                .unwrap_or_else(|| "markdown".to_string()),
            commit_sha: node.get_opt("commit_sha")?.unwrap_or_default(),
            description: node.get_opt("description")?.unwrap_or_default(),
            embedding,
            start_line: node.get_opt::<i64>("start_line")?.unwrap_or(0) as u32,
            end_line: node.get_opt::<i64>("end_line")?.unwrap_or(0) as u32,
            anchor: node.get_opt("anchor")?,
        })
    }
}

// ============================================================================
// Utility functions
// ============================================================================

/// Compute cosine similarity between two vectors.
fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot_product: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot_product / (norm_a * norm_b)
}
