//! Schema repository for migration queries and schema version tracking.

use crate::context::{AppGraph, Context};
use crate::di::FromContext;
use crate::error::AppError;
use crate::graph::Row;
use crate::models::Scope;

/// Repository for schema-related queries.
#[derive(FromContext, Clone)]
pub struct SchemaRepository {
    graph: AppGraph,
}

/// Scope with its hierarchy information.
#[derive(Debug, Clone)]
pub struct ScopeInfo {
    pub name: String,
    pub depth: u8,
    pub description: String,
}

/// Project statistics.
#[derive(Debug, Clone)]
pub struct ProjectStats {
    pub entity_count: i64,
    pub category_count: i64,
    pub document_count: i64,
    pub reference_count: i64,
    pub schema_version: u32,
}

impl SchemaRepository {
    /// Get the current schema version.
    pub async fn get_schema_version(&self) -> Result<u32, AppError> {
        let row = self
            .graph
            .query("MATCH (sv:SchemaVersion) RETURN sv.version AS version LIMIT 1")
            .fetch_one()
            .await?;

        match row {
            Some(row) => {
                let version: i64 = row.get("version")?;
                Ok(version as u32)
            }
            None => Ok(0),
        }
    }

    /// Get all applied migrations.
    pub async fn get_applied_migrations(&self) -> Result<Vec<String>, AppError> {
        let row = self
            .graph
            .query("MATCH (sv:SchemaVersion) RETURN sv.applied_migrations AS migrations LIMIT 1")
            .fetch_one()
            .await?;

        match row {
            Some(row) => {
                let migrations: Vec<String> = row.get_opt("migrations")?.unwrap_or_default();
                Ok(migrations)
            }
            None => Ok(vec![]),
        }
    }

    /// List all scopes with their hierarchy.
    pub async fn list_scopes(&self) -> Result<Vec<ScopeInfo>, AppError> {
        let rows = self
            .graph
            .query(
                "MATCH (s:Scope)
                 RETURN s.name AS name, s.depth AS depth, s.description AS description
                 ORDER BY s.depth",
            )
            .fetch_all()
            .await?;

        rows.iter().map(Self::row_to_scope_info).collect()
    }

    /// Get the scope hierarchy (which scope composes which).
    pub async fn get_scope_hierarchy(&self) -> Result<Vec<(String, String)>, AppError> {
        let rows = self
            .graph
            .query(
                "MATCH (parent:Scope)-[:COMPOSES]->(child:Scope)
                 RETURN parent.name AS parent, child.name AS child
                 ORDER BY parent.depth",
            )
            .fetch_all()
            .await?;

        let mut hierarchy = Vec::new();
        for row in &rows {
            let parent: String = row.get("parent")?;
            let child: String = row.get("child")?;
            hierarchy.push((parent, child));
        }
        Ok(hierarchy)
    }

    /// Get project statistics.
    ///
    /// Uses separate queries per node type to avoid Cartesian product explosion
    /// from chained OPTIONAL MATCH clauses in Apache AGE.
    pub async fn get_project_stats(&self) -> Result<ProjectStats, AppError> {
        let sv_row = self
            .graph
            .query("MATCH (sv:SchemaVersion) RETURN sv.version AS schema_version LIMIT 1")
            .fetch_one()
            .await?;

        let schema_version: u32 = match sv_row {
            Some(row) => row.get_opt::<i64>("schema_version")?.unwrap_or(0) as u32,
            None => return Err(AppError::NotInitialized),
        };

        let entity_count = self.count_nodes("Entity").await?;
        let category_count = self.count_nodes("Category").await?;
        let document_count = self.count_nodes("Document").await?;
        let code_ref_count = self.count_nodes("CodeReference").await?;
        let text_ref_count = self.count_nodes("TextReference").await?;

        Ok(ProjectStats {
            entity_count,
            category_count,
            document_count,
            reference_count: code_ref_count + text_ref_count,
            schema_version,
        })
    }

    /// Count nodes of a given label.
    async fn count_nodes(&self, label: &str) -> Result<i64, AppError> {
        let query = format!("MATCH (n:{}) RETURN count(n) AS cnt", label);
        let row = self.graph.query(&query).fetch_one().await?;
        match row {
            Some(row) => Ok(row.get_opt::<i64>("cnt")?.unwrap_or(0)),
            None => Ok(0),
        }
    }

    /// Check if the project is initialized (has schema version).
    pub async fn is_initialized(&self) -> Result<bool, AppError> {
        let version = self.get_schema_version().await?;
        Ok(version > 0)
    }

    /// Parse a scope name string to Scope enum.
    pub fn parse_scope(name: &str) -> Result<Scope, AppError> {
        match name {
            "Domain" => Ok(Scope::Domain),
            "Feature" => Ok(Scope::Feature),
            "Namespace" => Ok(Scope::Namespace),
            "Component" => Ok(Scope::Component),
            "Unit" => Ok(Scope::Unit),
            _ => Err(AppError::ScopeNotFound(name.to_string())),
        }
    }

    /// Convert a row to ScopeInfo.
    fn row_to_scope_info(row: &Row) -> Result<ScopeInfo, AppError> {
        let name: String = row.get("name")?;
        let depth: i64 = row.get("depth")?;
        let description: String = row.get_opt("description")?.unwrap_or_default();

        Ok(ScopeInfo {
            name,
            depth: depth as u8,
            description,
        })
    }
}
