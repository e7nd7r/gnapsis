//! Entity repository for CRUD operations on Entity nodes.

use chrono::{DateTime, Utc};

use std::str::FromStr;

use crate::context::{AppGraph, Context};
use crate::di::FromContext;
use crate::error::AppError;
use crate::graph::{Node, Row};
use crate::models::{Entity, Scope};

/// Check if child scope can belong to parent scope.
///
/// Valid when child is strictly deeper, or same depth for Namespace/Component.
fn is_valid_scope_hierarchy(child_depth: i64, child_name: &str, parent_depth: i64) -> bool {
    child_depth > parent_depth
        || (child_depth == parent_depth && (child_name == "Namespace" || child_name == "Component"))
}

/// Repository for Entity CRUD operations and relationships.
#[derive(FromContext, Clone)]
pub struct EntityRepository {
    graph: AppGraph,
}

impl EntityRepository {
    /// Create a new entity in the graph.
    pub async fn create(&self, entity: &Entity) -> Result<Entity, AppError> {
        let embedding_json = entity
            .embedding
            .as_ref()
            .map(|e| serde_json::to_value(e).unwrap_or_default());

        self.graph
            .query(
                "CREATE (e:Entity {
                    id: $id,
                    name: $name,
                    description: $description,
                    embedding: $embedding,
                    created_at: $created_at
                })",
            )
            .param("id", &entity.id)
            .param("name", &entity.name)
            .param("description", &entity.description)
            .param_raw(
                "embedding",
                embedding_json.unwrap_or(serde_json::Value::Null),
            )
            .param("created_at", entity.created_at.to_rfc3339())
            .run()
            .await?;

        Ok(entity.clone())
    }

    /// Find an entity by ID.
    pub async fn find_by_id(&self, id: &str) -> Result<Option<Entity>, AppError> {
        let row = self
            .graph
            .query("MATCH (e:Entity {id: $id}) RETURN e")
            .param("id", id)
            .fetch_one()
            .await?;

        match row {
            Some(row) => Ok(Some(Self::row_to_entity(&row)?)),
            None => Ok(None),
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
        let embedding_json = embedding.map(|e| serde_json::to_value(e).unwrap_or_default());
        let now = chrono::Utc::now().to_rfc3339();

        let row = self
            .graph
            .query(
                "MATCH (e:Entity {id: $id})
                 SET e.name = coalesce($name, e.name),
                     e.description = coalesce($description, e.description),
                     e.embedding = coalesce($embedding, e.embedding),
                     e.updated_at = $now
                 RETURN e",
            )
            .param("id", id)
            .param("name", name)
            .param("description", description)
            .param("now", &now)
            .param_raw(
                "embedding",
                embedding_json.unwrap_or(serde_json::Value::Null),
            )
            .fetch_one()
            .await?;

        match row {
            Some(row) => Ok(Self::row_to_entity(&row)?),
            None => Err(AppError::EntityNotFound(id.to_string())),
        }
    }

    /// Delete an entity by ID.
    ///
    /// Performs programmatic validation:
    /// - Checks entity has no children (BELONGS_TO relationships)
    /// - Cascades deletion to DocumentReferences
    pub async fn delete(&self, id: &str) -> Result<(), AppError> {
        // Check for children first (programmatic validation)
        if self.has_children(id).await? {
            return Err(AppError::HasChildren(id.to_string()));
        }

        // Verify entity exists before deleting
        let exists = self
            .graph
            .query("MATCH (e:Entity {id: $id}) RETURN e.id AS id")
            .param("id", id)
            .fetch_one()
            .await?;

        if exists.is_none() {
            return Err(AppError::EntityNotFound(id.to_string()));
        }

        // Cascade delete references
        self.delete_entity_references(id).await?;

        // Delete the entity (AGE doesn't support RETURN count(*) after DELETE)
        self.graph
            .query("MATCH (e:Entity {id: $id}) DETACH DELETE e")
            .param("id", id)
            .run()
            .await?;

        Ok(())
    }

    /// Check if an entity has children (entities with BELONGS_TO pointing to it).
    pub async fn has_children(&self, id: &str) -> Result<bool, AppError> {
        // AGE: Use direct MATCH and check if any rows returned (count() fails on empty)
        let rows = self
            .graph
            .query(
                "MATCH (child:Entity)-[:BELONGS_TO]->(parent:Entity {id: $id})
                 RETURN child.id AS child_id LIMIT 1",
            )
            .param("id", id)
            .fetch_all()
            .await?;

        Ok(!rows.is_empty())
    }

    /// Delete all references attached to an entity.
    async fn delete_entity_references(&self, entity_id: &str) -> Result<(), AppError> {
        // Delete CodeReferences
        self.graph
            .query(
                "MATCH (e:Entity {id: $entity_id})-[:HAS_REFERENCE]->(r:CodeReference)
                 DETACH DELETE r",
            )
            .param("entity_id", entity_id)
            .run()
            .await?;

        // Delete TextReferences
        self.graph
            .query(
                "MATCH (e:Entity {id: $entity_id})-[:HAS_REFERENCE]->(r:TextReference)
                 DETACH DELETE r",
            )
            .param("entity_id", entity_id)
            .run()
            .await?;

        Ok(())
    }

    /// Validate scope hierarchy: child scope must be deeper than parent's.
    ///
    /// Allows same-scope nesting for Namespace and Component only.
    /// Use this when the child entity doesn't exist yet (e.g., during creation).
    pub async fn validate_scope_for_parent(
        &self,
        child_scope: &str,
        parent_id: &str,
    ) -> Result<(), AppError> {
        let parent_info = self.get_entity_scope_info(parent_id).await?;

        if let Some((parent_depth, _)) = parent_info {
            let child_depth = Scope::from_str(child_scope)
                .map_err(AppError::Validation)?
                .depth() as i64;

            if !is_valid_scope_hierarchy(child_depth, child_scope, parent_depth) {
                return Err(AppError::InvalidBelongsTo {
                    child: format!("(new {} entity)", child_scope),
                    parent: parent_id.to_string(),
                    reason: format!(
                        "child scope depth ({}) must be greater than parent scope depth ({}) \
                         (Namespace and Component allow same-scope nesting)",
                        child_depth, parent_depth
                    ),
                });
            }
        }

        Ok(())
    }

    /// Validate that a BELONGS_TO relationship is valid between two existing entities.
    ///
    /// Delegates to the shared scope hierarchy rule.
    pub async fn validate_belongs_to(
        &self,
        child_id: &str,
        parent_id: &str,
    ) -> Result<(), AppError> {
        let child_scope = self.get_entity_scope_info(child_id).await?;
        let parent_scope = self.get_entity_scope_info(parent_id).await?;

        if let (Some((child_depth, child_name)), Some((parent_depth, _))) =
            (child_scope, parent_scope)
        {
            if !is_valid_scope_hierarchy(child_depth, &child_name, parent_depth) {
                return Err(AppError::InvalidBelongsTo {
                    child: child_id.to_string(),
                    parent: parent_id.to_string(),
                    reason: format!(
                        "child scope depth ({}) must be greater than parent scope depth ({}) \
                         (Namespace and Component allow same-scope nesting)",
                        child_depth, parent_depth
                    ),
                });
            }
        }

        Ok(())
    }

    /// Get the scope depth and name of an entity (via its classification).
    async fn get_entity_scope_info(
        &self,
        entity_id: &str,
    ) -> Result<Option<(i64, String)>, AppError> {
        let row = self
            .graph
            .query(
                "MATCH (e:Entity {id: $id})-[:CLASSIFIED_AS]->(:Category)-[:IN_SCOPE]->(s:Scope)
                 RETURN s.depth AS depth, s.name AS name",
            )
            .param("id", entity_id)
            .fetch_one()
            .await?;

        match row {
            Some(row) => {
                let depth: Option<i64> = row.get_opt("depth")?;
                let name: Option<String> = row.get_opt("name")?;
                match (depth, name) {
                    (Some(d), Some(n)) => Ok(Some((d, n))),
                    _ => Ok(None),
                }
            }
            None => Ok(None),
        }
    }

    /// Classify an entity with a category.
    pub async fn classify(&self, entity_id: &str, category_id: &str) -> Result<(), AppError> {
        self.graph
            .query(
                "MATCH (e:Entity {id: $entity_id})
                 MATCH (c:Category {id: $category_id})
                 MERGE (e)-[:CLASSIFIED_AS]->(c)",
            )
            .param("entity_id", entity_id)
            .param("category_id", category_id)
            .run()
            .await?;
        Ok(())
    }

    /// Remove classification from an entity.
    pub async fn unclassify(&self, entity_id: &str, category_id: &str) -> Result<(), AppError> {
        self.graph
            .query(
                "MATCH (e:Entity {id: $entity_id})-[r:CLASSIFIED_AS]->(c:Category {id: $category_id})
                 DELETE r",
            )
            .param("entity_id", entity_id)
            .param("category_id", category_id)
            .run()
            .await?;
        Ok(())
    }

    /// Add a BELONGS_TO relationship between entities.
    ///
    /// Performs programmatic validation of scope hierarchy.
    pub async fn add_belongs(
        &self,
        child_id: &str,
        parent_id: &str,
        note: Option<&str>,
    ) -> Result<(), AppError> {
        // Validate scope hierarchy
        self.validate_belongs_to(child_id, parent_id).await?;

        self.graph
            .query(
                "MATCH (child:Entity {id: $child_id})
                 MATCH (parent:Entity {id: $parent_id})
                 MERGE (child)-[r:BELONGS_TO]->(parent)
                 SET r.note = $note",
            )
            .param("child_id", child_id)
            .param("parent_id", parent_id)
            .param("note", note)
            .run()
            .await?;
        Ok(())
    }

    /// Remove a BELONGS_TO relationship.
    pub async fn remove_belongs(&self, child_id: &str, parent_id: &str) -> Result<(), AppError> {
        self.graph
            .query(
                "MATCH (child:Entity {id: $child_id})-[r:BELONGS_TO]->(parent:Entity {id: $parent_id})
                 DELETE r",
            )
            .param("child_id", child_id)
            .param("parent_id", parent_id)
            .run()
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
        let embedding_json = embedding.map(|e| serde_json::to_value(e).unwrap_or_default());

        self.graph
            .query(
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
            .param_raw(
                "embedding",
                embedding_json.unwrap_or(serde_json::Value::Null),
            )
            .run()
            .await?;
        Ok(())
    }

    /// Add a link relationship (CALLS, IMPORTS, IMPLEMENTS, INSTANTIATES).
    ///
    /// Note: AGE doesn't support dynamic relationship types like APOC,
    /// so we use a fixed LINK relationship with a type property.
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

        // Use a LINK relationship with type property (AGE doesn't support dynamic rel types)
        self.graph
            .query(
                "MATCH (from:Entity {id: $from_id})
                 MATCH (to:Entity {id: $to_id})
                 MERGE (from)-[r:LINK {type: $link_type}]->(to)",
            )
            .param("from_id", from_id)
            .param("to_id", to_id)
            .param("link_type", link_type)
            .run()
            .await?;
        Ok(())
    }

    /// Remove a RELATED_TO relationship between entities.
    pub async fn remove_related(&self, from_id: &str, to_id: &str) -> Result<(), AppError> {
        self.graph
            .query(
                "MATCH (from:Entity {id: $from_id})-[r:RELATED_TO]->(to:Entity {id: $to_id})
                 DELETE r",
            )
            .param("from_id", from_id)
            .param("to_id", to_id)
            .run()
            .await?;
        Ok(())
    }

    /// Remove a link relationship (CALLS, IMPORTS, IMPLEMENTS, INSTANTIATES).
    pub async fn remove_link(
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
                query: "remove_link".to_string(),
            });
        }

        self.graph
            .query(
                "MATCH (from:Entity {id: $from_id})-[r:LINK {type: $link_type}]->(to:Entity {id: $to_id})
                 DELETE r",
            )
            .param("from_id", from_id)
            .param("to_id", to_id)
            .param("link_type", link_type)
            .run()
            .await?;
        Ok(())
    }

    /// Get entity with its children (BELONGS_TO relationships).
    pub async fn get_children(&self, entity_id: &str) -> Result<Vec<Entity>, AppError> {
        let rows = self
            .graph
            .query(
                "MATCH (child:Entity)-[:BELONGS_TO]->(parent:Entity {id: $id})
                 RETURN child",
            )
            .param("id", entity_id)
            .fetch_all()
            .await?;

        rows.iter()
            .map(|row| Self::row_to_entity_field(row, "child"))
            .collect()
    }

    /// Get entity's parent (BELONGS_TO target).
    pub async fn get_parent(&self, entity_id: &str) -> Result<Option<Entity>, AppError> {
        let row = self
            .graph
            .query(
                "MATCH (child:Entity {id: $id})-[:BELONGS_TO]->(parent:Entity)
                 RETURN parent",
            )
            .param("id", entity_id)
            .fetch_one()
            .await?;

        match row {
            Some(row) => Ok(Some(Self::row_to_entity_field(&row, "parent")?)),
            None => Ok(None),
        }
    }

    /// Convert a row to an Entity (default field name "e").
    fn row_to_entity(row: &Row) -> Result<Entity, AppError> {
        Self::row_to_entity_field(row, "e")
    }

    /// Convert a row to an Entity with specified field name.
    fn row_to_entity_field(row: &Row, field: &str) -> Result<Entity, AppError> {
        let node: Node = row.get(field)?;

        let id: String = node.get("id")?;
        let name: String = node.get("name")?;
        let description: String = node.get("description")?;

        let embedding: Option<Vec<f64>> = node.get_opt("embedding")?;
        let embedding = embedding.map(|e| e.iter().map(|&f| f as f32).collect());

        // Parse datetime - AGE returns it as a string
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
}
