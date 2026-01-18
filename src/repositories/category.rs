//! Category repository for managing classification categories.

use std::sync::Arc;

use neo4rs::{query, Graph, Row};

use crate::context::Context;
use crate::di::FromContext;
use crate::error::AppError;
use crate::models::{generate_ulid, Category, Scope};

/// Repository for Category CRUD operations.
#[derive(FromContext, Clone)]
pub struct CategoryRepository {
    graph: Arc<Graph>,
}

impl CategoryRepository {
    /// Create a new category in the specified scope.
    pub async fn create(
        &self,
        name: &str,
        scope: Scope,
        description: Option<&str>,
    ) -> Result<Category, AppError> {
        let id = generate_ulid();

        self.graph
            .run(
                query(
                    "MATCH (s:Scope {name: $scope})
                     CREATE (c:Category {
                         id: $id,
                         name: $name,
                         description: $description,
                         created_at: datetime()
                     })-[:IN_SCOPE]->(s)",
                )
                .param("id", id.clone())
                .param("name", name)
                .param("scope", scope.to_string())
                .param("description", description),
            )
            .await?;

        Ok(Category {
            id,
            name: name.to_string(),
            scope,
            description: description.map(|s| s.to_string()),
        })
    }

    /// Find a category by ID.
    pub async fn find_by_id(&self, id: &str) -> Result<Option<Category>, AppError> {
        let mut result = self
            .graph
            .execute(
                query(
                    "MATCH (c:Category {id: $id})-[:IN_SCOPE]->(s:Scope)
                     RETURN c, s.name AS scope_name",
                )
                .param("id", id),
            )
            .await?;

        if let Some(row) = result.next().await? {
            Ok(Some(Self::row_to_category(&row)?))
        } else {
            Ok(None)
        }
    }

    /// Find a category by name and scope.
    pub async fn find_by_name(
        &self,
        name: &str,
        scope: Scope,
    ) -> Result<Option<Category>, AppError> {
        let mut result = self
            .graph
            .execute(
                query(
                    "MATCH (c:Category {name: $name})-[:IN_SCOPE]->(s:Scope {name: $scope})
                     RETURN c, s.name AS scope_name",
                )
                .param("name", name)
                .param("scope", scope.to_string()),
            )
            .await?;

        if let Some(row) = result.next().await? {
            Ok(Some(Self::row_to_category(&row)?))
        } else {
            Ok(None)
        }
    }

    /// List all categories.
    pub async fn list(&self) -> Result<Vec<Category>, AppError> {
        let mut result = self
            .graph
            .execute(query(
                "MATCH (c:Category)-[:IN_SCOPE]->(s:Scope)
                 RETURN c, s.name AS scope_name
                 ORDER BY s.depth, c.name",
            ))
            .await?;

        let mut categories = Vec::new();
        while let Some(row) = result.next().await? {
            categories.push(Self::row_to_category(&row)?);
        }
        Ok(categories)
    }

    /// List categories by scope.
    pub async fn list_by_scope(&self, scope: Scope) -> Result<Vec<Category>, AppError> {
        let mut result = self
            .graph
            .execute(
                query(
                    "MATCH (c:Category)-[:IN_SCOPE]->(s:Scope {name: $scope})
                     RETURN c, s.name AS scope_name
                     ORDER BY c.name",
                )
                .param("scope", scope.to_string()),
            )
            .await?;

        let mut categories = Vec::new();
        while let Some(row) = result.next().await? {
            categories.push(Self::row_to_category(&row)?);
        }
        Ok(categories)
    }

    /// Delete a category by ID.
    pub async fn delete(&self, id: &str) -> Result<(), AppError> {
        let mut result = self
            .graph
            .execute(
                query(
                    "MATCH (c:Category {id: $id})
                     DETACH DELETE c
                     RETURN count(*) AS deleted",
                )
                .param("id", id),
            )
            .await?;

        if let Some(row) = result.next().await? {
            let deleted: i64 = row.get("deleted").map_err(|e| AppError::Query {
                message: e.to_string(),
                query: "delete category".to_string(),
            })?;
            if deleted == 0 {
                return Err(AppError::CategoryNotFound(id.to_string()));
            }
        }

        Ok(())
    }

    /// Convert a Neo4j row to a Category.
    fn row_to_category(row: &Row) -> Result<Category, AppError> {
        let node: neo4rs::Node = row.get("c").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "parse category node".to_string(),
        })?;

        let scope_name: String = row.get("scope_name").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "get scope name".to_string(),
        })?;

        let id: String = node.get("id").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "get category id".to_string(),
        })?;

        let name: String = node.get("name").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "get category name".to_string(),
        })?;

        let description: Option<String> = node.get("description").ok();

        let scope = Self::parse_scope(&scope_name)?;

        Ok(Category {
            id,
            name,
            scope,
            description,
        })
    }

    /// Parse scope name to Scope enum.
    fn parse_scope(name: &str) -> Result<Scope, AppError> {
        match name {
            "Domain" => Ok(Scope::Domain),
            "Feature" => Ok(Scope::Feature),
            "Namespace" => Ok(Scope::Namespace),
            "Component" => Ok(Scope::Component),
            "Unit" => Ok(Scope::Unit),
            _ => Err(AppError::ScopeNotFound(name.to_string())),
        }
    }
}
