//! Category repository for managing classification categories.

use crate::context::{AppGraph, Context};
use crate::di::FromContext;
use crate::error::AppError;
use crate::graph::{Node, Row};
use crate::models::{generate_ulid, Category, Scope};

/// Repository for Category CRUD operations.
#[derive(FromContext, Clone)]
pub struct CategoryRepository {
    graph: AppGraph,
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
        let now = chrono::Utc::now().to_rfc3339();

        self.graph
            .query(
                "MATCH (s:Scope {name: $scope})
                 CREATE (c:Category {
                     id: $id,
                     name: $name,
                     description: $description,
                     created_at: $now
                 })-[:IN_SCOPE]->(s)",
            )
            .param("id", &id)
            .param("name", name)
            .param("scope", scope.to_string())
            .param("description", description)
            .param("now", &now)
            .run()
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
        let row = self
            .graph
            .query(
                "MATCH (c:Category {id: $id})-[:IN_SCOPE]->(s:Scope)
                 RETURN c, s.name AS scope_name",
            )
            .param("id", id)
            .fetch_one()
            .await?;

        match row {
            Some(row) => Ok(Some(Self::row_to_category(&row)?)),
            None => Ok(None),
        }
    }

    /// Find a category by name and scope.
    pub async fn find_by_name(
        &self,
        name: &str,
        scope: Scope,
    ) -> Result<Option<Category>, AppError> {
        let row = self
            .graph
            .query(
                "MATCH (c:Category {name: $name})-[:IN_SCOPE]->(s:Scope {name: $scope})
                 RETURN c, s.name AS scope_name",
            )
            .param("name", name)
            .param("scope", scope.to_string())
            .fetch_one()
            .await?;

        match row {
            Some(row) => Ok(Some(Self::row_to_category(&row)?)),
            None => Ok(None),
        }
    }

    /// List all categories.
    pub async fn list(&self) -> Result<Vec<Category>, AppError> {
        let rows = self
            .graph
            .query(
                "MATCH (c:Category)-[:IN_SCOPE]->(s:Scope)
                 RETURN c, s.name AS scope_name
                 ORDER BY s.depth, c.name",
            )
            .fetch_all()
            .await?;

        rows.iter().map(Self::row_to_category).collect()
    }

    /// List categories by scope.
    pub async fn list_by_scope(&self, scope: Scope) -> Result<Vec<Category>, AppError> {
        let rows = self
            .graph
            .query(
                "MATCH (c:Category)-[:IN_SCOPE]->(s:Scope {name: $scope})
                 RETURN c, s.name AS scope_name
                 ORDER BY c.name",
            )
            .param("scope", scope.to_string())
            .fetch_all()
            .await?;

        rows.iter().map(Self::row_to_category).collect()
    }

    /// Delete a category by ID.
    pub async fn delete(&self, id: &str) -> Result<(), AppError> {
        let row = self
            .graph
            .query(
                "MATCH (c:Category {id: $id})
                 DETACH DELETE c
                 RETURN count(*) AS deleted",
            )
            .param("id", id)
            .fetch_one()
            .await?;

        match row {
            Some(row) => {
                let deleted: i64 = row.get("deleted")?;
                if deleted == 0 {
                    return Err(AppError::CategoryNotFound(id.to_string()));
                }
                Ok(())
            }
            None => Err(AppError::CategoryNotFound(id.to_string())),
        }
    }

    /// Convert a row to a Category.
    fn row_to_category(row: &Row) -> Result<Category, AppError> {
        let node: Node = row.get("c")?;
        let scope_name: String = row.get("scope_name")?;

        let id: String = node.get("id")?;
        let name: String = node.get("name")?;
        let description: Option<String> = node.get_opt("description")?;

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
