//! Query repository for graph traversal and search operations.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use neo4rs::{query, Graph, Row};

use crate::context::Context;
use crate::di::FromContext;
use crate::error::AppError;
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
    graph: Arc<Graph>,
}

impl QueryRepository {
    /// Get entity with full context: classifications, references, parents, children, related.
    pub async fn get_entity_with_context(&self, id: &str) -> Result<EntityWithContext, AppError> {
        let mut result = self
            .graph
            .execute(
                query(
                    "MATCH (e:Entity {id: $id})
                     OPTIONAL MATCH (e)-[:CLASSIFIED_AS]->(c:Category)-[:IN_SCOPE]->(s:Scope)
                     OPTIONAL MATCH (e)-[:HAS_REFERENCE]->(code_ref:CodeReference)
                     OPTIONAL MATCH (e)-[:HAS_REFERENCE]->(text_ref:TextReference)
                     OPTIONAL MATCH (e)-[:BELONGS_TO]->(parent:Entity)
                     OPTIONAL MATCH (child:Entity)-[:BELONGS_TO]->(e)
                     OPTIONAL MATCH (e)-[:RELATED_TO]->(related:Entity)
                     RETURN e,
                            collect(DISTINCT {id: c.id, name: c.name, scope: s.name}) AS classifications,
                            collect(DISTINCT code_ref) AS code_refs,
                            collect(DISTINCT text_ref) AS text_refs,
                            collect(DISTINCT parent) AS parents,
                            collect(DISTINCT child) AS children,
                            collect(DISTINCT related) AS related",
                )
                .param("id", id),
            )
            .await?;

        let row = result
            .next()
            .await?
            .ok_or_else(|| AppError::EntityNotFound(id.to_string()))?;

        // Parse entity
        let entity = Self::row_to_entity(&row, "e")?;

        // Parse classifications
        let classifications_raw: Vec<neo4rs::BoltMap> =
            row.get("classifications").unwrap_or_default();
        let classifications: Vec<CategoryClassification> = classifications_raw
            .into_iter()
            .filter_map(|m| {
                let id: Option<String> = m.get("id").ok();
                let name: Option<String> = m.get("name").ok();
                let scope: Option<String> = m.get("scope").ok();
                match (id, name, scope) {
                    (Some(id), Some(name), Some(scope)) if !id.is_empty() => {
                        Some(CategoryClassification { id, name, scope })
                    }
                    _ => None,
                }
            })
            .collect();

        // Parse code references
        let code_refs_raw: Vec<neo4rs::Node> = row.get("code_refs").unwrap_or_default();
        let mut references: Vec<Reference> = code_refs_raw
            .into_iter()
            .filter_map(|node| Self::node_to_code_reference(&node).ok())
            .map(Reference::Code)
            .collect();

        // Parse text references
        let text_refs_raw: Vec<neo4rs::Node> = row.get("text_refs").unwrap_or_default();
        references.extend(
            text_refs_raw
                .into_iter()
                .filter_map(|node| Self::node_to_text_reference(&node).ok())
                .map(Reference::Text),
        );

        // Parse parents
        let parents_raw: Vec<neo4rs::Node> = row.get("parents").unwrap_or_default();
        let parents: Vec<Entity> = parents_raw
            .into_iter()
            .filter_map(|node| Self::node_to_entity(&node).ok())
            .collect();

        // Parse children
        let children_raw: Vec<neo4rs::Node> = row.get("children").unwrap_or_default();
        let children: Vec<Entity> = children_raw
            .into_iter()
            .filter_map(|node| Self::node_to_entity(&node).ok())
            .collect();

        // Parse related
        let related_raw: Vec<neo4rs::Node> = row.get("related").unwrap_or_default();
        let related: Vec<Entity> = related_raw
            .into_iter()
            .filter_map(|node| Self::node_to_entity(&node).ok())
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

        let mut q = query(query_str).param("limit", limit);

        if let Some(scope) = scope {
            q = q.param("scope", scope);
        }
        if let Some(category) = category {
            q = q.param("category", category);
        }
        if let Some(parent_id) = parent_id {
            q = q.param("parent_id", parent_id);
        }

        let mut result = self.graph.execute(q).await?;

        let mut entities = Vec::new();
        while let Some(row) = result.next().await? {
            entities.push(Self::row_to_entity(&row, "e")?);
        }

        Ok(entities)
    }

    /// Get all entities with references in a document.
    pub async fn get_document_entities(
        &self,
        path: &str,
    ) -> Result<Vec<EntityWithReference>, AppError> {
        let mut entities = Vec::new();

        // Get CodeReferences
        let mut code_result = self
            .graph
            .execute(
                query(
                    "MATCH (e:Entity)-[:HAS_REFERENCE]->(ref:CodeReference)-[:IN_DOCUMENT]->(d:Document {path: $path})
                     RETURN e, ref, labels(ref) AS refLabels
                     ORDER BY ref.lsp_symbol",
                )
                .param("path", path),
            )
            .await?;

        while let Some(row) = code_result.next().await? {
            let entity = Self::row_to_entity(&row, "e")?;
            let ref_node: neo4rs::Node = row.get("ref").map_err(|e| AppError::Query {
                message: e.to_string(),
                query: "get ref node".to_string(),
            })?;
            let reference = Reference::Code(Self::node_to_code_reference(&ref_node)?);
            entities.push(EntityWithReference { entity, reference });
        }

        // Get TextReferences
        let mut text_result = self
            .graph
            .execute(
                query(
                    "MATCH (e:Entity)-[:HAS_REFERENCE]->(ref:TextReference)-[:IN_DOCUMENT]->(d:Document {path: $path})
                     RETURN e, ref
                     ORDER BY ref.start_line",
                )
                .param("path", path),
            )
            .await?;

        while let Some(row) = text_result.next().await? {
            let entity = Self::row_to_entity(&row, "e")?;
            let ref_node: neo4rs::Node = row.get("ref").map_err(|e| AppError::Query {
                message: e.to_string(),
                query: "get ref node".to_string(),
            })?;
            let reference = Reference::Text(Self::node_to_text_reference(&ref_node)?);
            entities.push(EntityWithReference { entity, reference });
        }

        Ok(entities)
    }

    /// Query subgraph around an entity within N hops.
    pub async fn query_subgraph(
        &self,
        id: &str,
        hops: u32,
        rel_types: Option<&[String]>,
    ) -> Result<Subgraph, AppError> {
        let hops = hops.min(5) as i64;

        // Build relationship type filter
        let rel_filter = if let Some(types) = rel_types {
            if types.is_empty() {
                String::new()
            } else {
                format!(":{}", types.join("|"))
            }
        } else {
            String::new()
        };

        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut seen_nodes = std::collections::HashSet::new();
        let mut seen_edges = std::collections::HashSet::new();

        // Add starting node
        let mut start_result = self
            .graph
            .execute(
                query(
                    "MATCH (e:Entity {id: $id})
                     OPTIONAL MATCH (e)-[:CLASSIFIED_AS]->(c:Category)
                     RETURN e, collect(c.name)[0] AS category",
                )
                .param("id", id),
            )
            .await?;

        if let Some(row) = start_result.next().await? {
            let node: neo4rs::Node = row.get("e").map_err(|e| AppError::Query {
                message: e.to_string(),
                query: "get start node".to_string(),
            })?;
            let node_id: String = node.get("id").unwrap_or_default();
            let category: Option<String> = row.get("category").ok();

            seen_nodes.insert(node_id.clone());
            nodes.push(SubgraphNode::Entity {
                id: node_id,
                name: node.get("name").unwrap_or_default(),
                description: node.get("description").unwrap_or_default(),
                distance: 0,
                category,
            });
        }

        // Query for connected nodes (Entity and DocumentReference) with relationships
        let query_str = format!(
            "MATCH path = (start:Entity {{id: $id}})-[r{}*1..{}]-(connected)
             WHERE connected:Entity OR connected:DocumentReference
             WITH connected, relationships(path) AS rels, length(path) AS distance, labels(connected) AS nodeLabels
             OPTIONAL MATCH (connected)-[:CLASSIFIED_AS]->(c:Category)
             RETURN DISTINCT connected, distance, nodeLabels, collect(DISTINCT c.name)[0] AS category,
                    [rel IN rels | [type(rel), startNode(rel).id, endNode(rel).id, coalesce(rel.note, '')]] AS relData",
            rel_filter, hops
        );

        let mut result = self
            .graph
            .execute(query(&query_str).param("id", id))
            .await?;

        while let Some(row) = result.next().await? {
            let node: neo4rs::Node = row.get("connected").map_err(|e| AppError::Query {
                message: e.to_string(),
                query: "get connected node".to_string(),
            })?;
            let distance: i64 = row.get("distance").unwrap_or(1);
            let node_labels: Vec<String> = row.get("nodeLabels").unwrap_or_default();
            let node_id: String = node.get("id").unwrap_or_default();

            if !seen_nodes.contains(&node_id) {
                seen_nodes.insert(node_id.clone());

                if node_labels.contains(&"DocumentReference".to_string()) {
                    nodes.push(SubgraphNode::DocumentReference {
                        id: node_id,
                        document_path: node.get("document_path").unwrap_or_default(),
                        start_line: node.get::<i64>("start_line").unwrap_or(0) as u32,
                        end_line: node.get::<i64>("end_line").unwrap_or(0) as u32,
                        description: node.get("description").unwrap_or_default(),
                        distance: distance as u32,
                    });
                } else {
                    let category: Option<String> = row.get("category").ok();
                    nodes.push(SubgraphNode::Entity {
                        id: node_id,
                        name: node.get("name").unwrap_or_default(),
                        description: node.get("description").unwrap_or_default(),
                        distance: distance as u32,
                        category,
                    });
                }
            }

            // Extract relationships with node IDs and notes
            let rel_data: Vec<Vec<String>> = row.get("relData").unwrap_or_default();
            for rel_info in rel_data {
                if rel_info.len() >= 3 {
                    let rel_type = &rel_info[0];
                    let from_id = &rel_info[1];
                    let to_id = &rel_info[2];
                    let note = rel_info.get(3).cloned().filter(|s| !s.is_empty());
                    let edge_key = format!("{}-{}-{}", from_id, rel_type, to_id);

                    if !seen_edges.contains(&edge_key) {
                        seen_edges.insert(edge_key);
                        edges.push(SubgraphEdge {
                            from_id: from_id.clone(),
                            to_id: to_id.clone(),
                            relationship: rel_type.clone(),
                            note,
                        });
                    }
                }
            }
        }

        Ok(Subgraph { nodes, edges })
    }

    /// Search entities by embedding similarity.
    pub async fn search_entities_by_embedding(
        &self,
        embedding: &[f64],
        limit: u32,
        min_score: f32,
        scope: Option<&str>,
    ) -> Result<Vec<SearchResult<Entity>>, AppError> {
        let limit = limit.min(50) as i64;

        let query_str = if scope.is_some() {
            "MATCH (e:Entity)-[:CLASSIFIED_AS]->(c:Category)-[:IN_SCOPE]->(s:Scope {name: $scope})
             WHERE e.embedding IS NOT NULL
             WITH e, c, gds.similarity.cosine(e.embedding, $embedding) AS score
             WHERE score >= $min_score
             RETURN e, score, c.name AS category
             ORDER BY score DESC
             LIMIT $limit"
        } else {
            "MATCH (e:Entity)
             WHERE e.embedding IS NOT NULL
             OPTIONAL MATCH (e)-[:CLASSIFIED_AS]->(c:Category)
             WITH e, c, gds.similarity.cosine(e.embedding, $embedding) AS score
             WHERE score >= $min_score
             RETURN e, score, collect(c.name)[0] AS category
             ORDER BY score DESC
             LIMIT $limit"
        };

        let mut q = query(query_str)
            .param("embedding", embedding.to_vec())
            .param("min_score", min_score as f64)
            .param("limit", limit);

        if let Some(scope) = scope {
            q = q.param("scope", scope);
        }

        let mut result = self.graph.execute(q).await?;

        let mut results = Vec::new();
        while let Some(row) = result.next().await? {
            let entity = Self::row_to_entity(&row, "e")?;
            let score: f64 = row.get("score").unwrap_or(0.0);
            results.push(SearchResult {
                item: entity,
                score: score as f32,
            });
        }

        Ok(results)
    }

    /// Get entity summaries by scope with category info.
    /// Returns entities with their primary category for project overview.
    pub async fn get_entity_summaries_by_scope(
        &self,
        scope: &str,
    ) -> Result<Vec<ProjectEntitySummary>, AppError> {
        let mut result = self
            .graph
            .execute(
                query(
                    "MATCH (e:Entity)-[:CLASSIFIED_AS]->(c:Category)-[:IN_SCOPE]->(s:Scope {name: $scope})
                     OPTIONAL MATCH (e)-[:BELONGS_TO]->(parent:Entity)
                     RETURN e.id AS id, e.name AS name, e.description AS description,
                            collect(DISTINCT c.name)[0] AS category,
                            collect(DISTINCT parent.id)[0] AS parent_id
                     ORDER BY e.name",
                )
                .param("scope", scope),
            )
            .await?;

        let mut summaries = Vec::new();
        while let Some(row) = result.next().await? {
            summaries.push(ProjectEntitySummary {
                id: row.get("id").unwrap_or_default(),
                name: row.get("name").unwrap_or_default(),
                description: row.get("description").unwrap_or_default(),
                category: row.get("category").ok(),
                parent_id: row.get("parent_id").ok(),
            });
        }

        Ok(summaries)
    }

    /// Search document references by embedding similarity.
    pub async fn search_documents_by_embedding(
        &self,
        embedding: &[f64],
        limit: u32,
        min_score: f32,
    ) -> Result<Vec<SearchResult<EntityWithReference>>, AppError> {
        let limit = limit.min(50) as i64;

        let mut results = Vec::new();

        // Search CodeReferences
        let mut code_result = self
            .graph
            .execute(
                query(
                    "MATCH (e:Entity)-[:HAS_REFERENCE]->(ref:CodeReference)
                     WHERE ref.embedding IS NOT NULL
                     WITH e, ref, gds.similarity.cosine(ref.embedding, $embedding) AS score
                     WHERE score >= $min_score
                     RETURN e, ref, score
                     ORDER BY score DESC
                     LIMIT $limit",
                )
                .param("embedding", embedding.to_vec())
                .param("min_score", min_score as f64)
                .param("limit", limit),
            )
            .await?;

        while let Some(row) = code_result.next().await? {
            let entity = Self::row_to_entity(&row, "e")?;
            let ref_node: neo4rs::Node = row.get("ref").map_err(|e| AppError::Query {
                message: e.to_string(),
                query: "get ref node".to_string(),
            })?;
            let reference = Reference::Code(Self::node_to_code_reference(&ref_node)?);
            let score: f64 = row.get("score").unwrap_or(0.0);
            results.push(SearchResult {
                item: EntityWithReference { entity, reference },
                score: score as f32,
            });
        }

        // Search TextReferences
        let mut text_result = self
            .graph
            .execute(
                query(
                    "MATCH (e:Entity)-[:HAS_REFERENCE]->(ref:TextReference)
                     WHERE ref.embedding IS NOT NULL
                     WITH e, ref, gds.similarity.cosine(ref.embedding, $embedding) AS score
                     WHERE score >= $min_score
                     RETURN e, ref, score
                     ORDER BY score DESC
                     LIMIT $limit",
                )
                .param("embedding", embedding.to_vec())
                .param("min_score", min_score as f64)
                .param("limit", limit),
            )
            .await?;

        while let Some(row) = text_result.next().await? {
            let entity = Self::row_to_entity(&row, "e")?;
            let ref_node: neo4rs::Node = row.get("ref").map_err(|e| AppError::Query {
                message: e.to_string(),
                query: "get ref node".to_string(),
            })?;
            let reference = Reference::Text(Self::node_to_text_reference(&ref_node)?);
            let score: f64 = row.get("score").unwrap_or(0.0);
            results.push(SearchResult {
                item: EntityWithReference { entity, reference },
                score: score as f32,
            });
        }

        // Sort by score descending and limit
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit as usize);

        Ok(results)
    }

    // ============================================================================
    // Helper methods
    // ============================================================================

    /// Convert a Neo4j row to an Entity.
    fn row_to_entity(row: &Row, field: &str) -> Result<Entity, AppError> {
        let node: neo4rs::Node = row.get(field).map_err(|e| AppError::Query {
            message: e.to_string(),
            query: format!("get {} node", field),
        })?;
        Self::node_to_entity(&node)
    }

    /// Convert a Neo4j node to an Entity.
    fn node_to_entity(node: &neo4rs::Node) -> Result<Entity, AppError> {
        let id: String = node.get("id").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "get entity id".to_string(),
        })?;

        let name: String = node.get("name").unwrap_or_default();
        let description: String = node.get("description").unwrap_or_default();

        let embedding: Option<Vec<f64>> = node.get("embedding").ok();
        let embedding = embedding.map(|e| e.iter().map(|&f| f as f32).collect());

        let created_at: DateTime<Utc> = node
            .get::<String>("created_at")
            .ok()
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

    /// Convert a Neo4j node to a CodeReference.
    fn node_to_code_reference(node: &neo4rs::Node) -> Result<CodeReference, AppError> {
        let id: String = node.get("id").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "get code reference id".to_string(),
        })?;

        let embedding: Option<Vec<f64>> = node.get("embedding").ok();
        let embedding = embedding.map(|e| e.iter().map(|&f| f as f32).collect());

        Ok(CodeReference {
            id,
            path: node.get("path").unwrap_or_default(),
            language: node.get("language").unwrap_or_default(),
            commit_sha: node.get("commit_sha").unwrap_or_default(),
            description: node.get("description").unwrap_or_default(),
            embedding,
            lsp_symbol: node.get("lsp_symbol").unwrap_or_default(),
            lsp_kind: node.get::<i64>("lsp_kind").unwrap_or(0) as i32,
            lsp_range: node.get("lsp_range").unwrap_or_default(),
        })
    }

    /// Convert a Neo4j node to a TextReference.
    fn node_to_text_reference(node: &neo4rs::Node) -> Result<TextReference, AppError> {
        let id: String = node.get("id").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "get text reference id".to_string(),
        })?;

        let embedding: Option<Vec<f64>> = node.get("embedding").ok();
        let embedding = embedding.map(|e| e.iter().map(|&f| f as f32).collect());

        Ok(TextReference {
            id,
            path: node.get("path").unwrap_or_default(),
            content_type: node
                .get("content_type")
                .unwrap_or_else(|_| "markdown".to_string()),
            commit_sha: node.get("commit_sha").unwrap_or_default(),
            description: node.get("description").unwrap_or_default(),
            embedding,
            start_line: node.get::<i64>("start_line").unwrap_or(0) as u32,
            end_line: node.get::<i64>("end_line").unwrap_or(0) as u32,
            anchor: node.get("anchor").ok(),
        })
    }
}
