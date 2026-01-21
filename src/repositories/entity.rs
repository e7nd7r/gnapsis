//! Entity repository for CRUD operations on Entity nodes.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use neo4rs::{query, Graph, Row};

use crate::context::Context;
use crate::di::FromContext;
use crate::error::AppError;
use crate::models::Entity;

/// Repository for Entity CRUD operations and relationships.
#[derive(FromContext, Clone)]
pub struct EntityRepository {
    graph: Arc<Graph>,
}

impl EntityRepository {
    /// Create a new entity in the graph.
    pub async fn create(&self, entity: &Entity) -> Result<Entity, AppError> {
        let embedding_param: Option<Vec<f64>> = entity
            .embedding
            .as_ref()
            .map(|e| e.iter().map(|&f| f as f64).collect());

        self.graph
            .run(
                query(
                    "CREATE (e:Entity {
                        id: $id,
                        name: $name,
                        description: $description,
                        embedding: $embedding,
                        created_at: datetime($created_at)
                    })",
                )
                .param("id", entity.id.clone())
                .param("name", entity.name.clone())
                .param("description", entity.description.clone())
                .param("embedding", embedding_param)
                .param("created_at", entity.created_at.to_rfc3339()),
            )
            .await?;

        Ok(entity.clone())
    }

    /// Find an entity by ID.
    pub async fn find_by_id(&self, id: &str) -> Result<Option<Entity>, AppError> {
        let mut result = self
            .graph
            .execute(query("MATCH (e:Entity {id: $id}) RETURN e").param("id", id))
            .await?;

        if let Some(row) = result.next().await? {
            Ok(Some(Self::row_to_entity(&row)?))
        } else {
            Ok(None)
        }
    }

    /// Update an entity's name and description.
    pub async fn update(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
        embedding: Option<&[f32]>,
    ) -> Result<Entity, AppError> {
        let embedding_param: Option<Vec<f64>> =
            embedding.map(|e| e.iter().map(|&f| f as f64).collect());

        let mut result = self
            .graph
            .execute(
                query(
                    "MATCH (e:Entity {id: $id})
                     SET e.name = coalesce($name, e.name),
                         e.description = coalesce($description, e.description),
                         e.embedding = coalesce($embedding, e.embedding),
                         e.updated_at = datetime()
                     RETURN e",
                )
                .param("id", id)
                .param("name", name)
                .param("description", description)
                .param("embedding", embedding_param),
            )
            .await?;

        if let Some(row) = result.next().await? {
            Ok(Self::row_to_entity(&row)?)
        } else {
            Err(AppError::EntityNotFound(id.to_string()))
        }
    }

    /// Delete an entity by ID.
    ///
    /// Note: APOC trigger will prevent deletion if entity has children.
    /// APOC trigger will cascade delete DocumentReferences.
    pub async fn delete(&self, id: &str) -> Result<(), AppError> {
        let mut result = self
            .graph
            .execute(
                query(
                    "MATCH (e:Entity {id: $id})
                     DETACH DELETE e
                     RETURN count(*) AS deleted",
                )
                .param("id", id),
            )
            .await?;

        if let Some(row) = result.next().await? {
            let deleted: i64 = row.get("deleted").map_err(|e| AppError::Query {
                message: e.to_string(),
                query: "delete entity".to_string(),
            })?;
            if deleted == 0 {
                return Err(AppError::EntityNotFound(id.to_string()));
            }
        }

        Ok(())
    }

    /// Classify an entity with a category.
    pub async fn classify(&self, entity_id: &str, category_id: &str) -> Result<(), AppError> {
        self.graph
            .run(
                query(
                    "MATCH (e:Entity {id: $entity_id})
                     MATCH (c:Category {id: $category_id})
                     MERGE (e)-[:CLASSIFIED_AS]->(c)",
                )
                .param("entity_id", entity_id)
                .param("category_id", category_id),
            )
            .await?;
        Ok(())
    }

    /// Remove classification from an entity.
    pub async fn unclassify(&self, entity_id: &str, category_id: &str) -> Result<(), AppError> {
        self.graph
            .run(
                query(
                    "MATCH (e:Entity {id: $entity_id})-[r:CLASSIFIED_AS]->(c:Category {id: $category_id})
                     DELETE r",
                )
                .param("entity_id", entity_id)
                .param("category_id", category_id),
            )
            .await?;
        Ok(())
    }

    /// Add a BELONGS_TO relationship between entities.
    ///
    /// Note: APOC trigger validates that child scope depth > parent scope depth.
    pub async fn add_belongs(
        &self,
        child_id: &str,
        parent_id: &str,
        note: Option<&str>,
    ) -> Result<(), AppError> {
        let mut q = query(
            "MATCH (child:Entity {id: $child_id})
             MATCH (parent:Entity {id: $parent_id})
             MERGE (child)-[r:BELONGS_TO]->(parent)
             SET r.note = $note",
        )
        .param("child_id", child_id)
        .param("parent_id", parent_id);

        q = q.param("note", note);

        self.graph.run(q).await?;
        Ok(())
    }

    /// Remove a BELONGS_TO relationship.
    pub async fn remove_belongs(&self, child_id: &str, parent_id: &str) -> Result<(), AppError> {
        self.graph
            .run(
                query(
                    "MATCH (child:Entity {id: $child_id})-[r:BELONGS_TO]->(parent:Entity {id: $parent_id})
                     DELETE r",
                )
                .param("child_id", child_id)
                .param("parent_id", parent_id),
            )
            .await?;
        Ok(())
    }

    /// Add a RELATED_TO relationship between entities.
    ///
    /// The `note` describes the relationship and can have an embedding for semantic search.
    pub async fn add_related(
        &self,
        from_id: &str,
        to_id: &str,
        relation_type: Option<&str>,
        note: Option<&str>,
        embedding: Option<&[f32]>,
    ) -> Result<(), AppError> {
        let embedding_param: Option<Vec<f64>> =
            embedding.map(|e| e.iter().map(|&f| f as f64).collect());

        self.graph
            .run(
                query(
                    "MATCH (from:Entity {id: $from_id})
                     MATCH (to:Entity {id: $to_id})
                     MERGE (from)-[r:RELATED_TO]->(to)
                     SET r.type = $relation_type,
                         r.note = $note,
                         r.embedding = $embedding",
                )
                .param("from_id", from_id)
                .param("to_id", to_id)
                .param("relation_type", relation_type)
                .param("note", note)
                .param("embedding", embedding_param),
            )
            .await?;
        Ok(())
    }

    /// Add a link relationship (CALLS, IMPORTS, IMPLEMENTS, INSTANTIATES).
    pub async fn add_link(
        &self,
        from_id: &str,
        to_id: &str,
        link_type: &str,
    ) -> Result<(), AppError> {
        // Validate link type
        let valid_types = ["CALLS", "IMPORTS", "IMPLEMENTS", "INSTANTIATES"];
        if !valid_types.contains(&link_type) {
            return Err(AppError::Query {
                message: format!(
                    "Invalid link type: {}. Must be one of {:?}",
                    link_type, valid_types
                ),
                query: "add_link".to_string(),
            });
        }

        // Use APOC to create dynamic relationship type
        self.graph
            .run(
                query(
                    "MATCH (from:Entity {id: $from_id})
                     MATCH (to:Entity {id: $to_id})
                     CALL apoc.merge.relationship(from, $link_type, {}, {}, to, {}) YIELD rel
                     RETURN rel",
                )
                .param("from_id", from_id)
                .param("to_id", to_id)
                .param("link_type", link_type),
            )
            .await?;
        Ok(())
    }

    /// Get entity with its children (BELONGS_TO relationships).
    pub async fn get_children(&self, entity_id: &str) -> Result<Vec<Entity>, AppError> {
        let mut result = self
            .graph
            .execute(
                query(
                    "MATCH (child:Entity)-[:BELONGS_TO]->(parent:Entity {id: $id})
                     RETURN child",
                )
                .param("id", entity_id),
            )
            .await?;

        let mut children = Vec::new();
        while let Some(row) = result.next().await? {
            children.push(Self::row_to_entity_field(&row, "child")?);
        }
        Ok(children)
    }

    /// Get entity's parent (BELONGS_TO target).
    pub async fn get_parent(&self, entity_id: &str) -> Result<Option<Entity>, AppError> {
        let mut result = self
            .graph
            .execute(
                query(
                    "MATCH (child:Entity {id: $id})-[:BELONGS_TO]->(parent:Entity)
                     RETURN parent",
                )
                .param("id", entity_id),
            )
            .await?;

        if let Some(row) = result.next().await? {
            Ok(Some(Self::row_to_entity_field(&row, "parent")?))
        } else {
            Ok(None)
        }
    }

    /// Convert a Neo4j row to an Entity (default field name "e").
    fn row_to_entity(row: &Row) -> Result<Entity, AppError> {
        Self::row_to_entity_field(row, "e")
    }

    /// Convert a Neo4j row to an Entity with specified field name.
    fn row_to_entity_field(row: &Row, field: &str) -> Result<Entity, AppError> {
        let node: neo4rs::Node = row.get(field).map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "parse entity node".to_string(),
        })?;

        let id: String = node.get("id").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "get entity id".to_string(),
        })?;

        let name: String = node.get("name").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "get entity name".to_string(),
        })?;

        let description: String = node.get("description").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "get entity description".to_string(),
        })?;

        let embedding: Option<Vec<f64>> = node.get("embedding").ok();
        let embedding = embedding.map(|e| e.iter().map(|&f| f as f32).collect());

        // Parse datetime - neo4rs returns it as a string in RFC3339 format
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
}
