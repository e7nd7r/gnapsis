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
    pub async fn get_project_stats(&self) -> Result<ProjectStats, AppError> {
        let row = self
            .graph
            .query(
                "MATCH (sv:SchemaVersion)
                 OPTIONAL MATCH (e:Entity)
                 OPTIONAL MATCH (c:Category)
                 OPTIONAL MATCH (d:Document)
                 OPTIONAL MATCH (r:CodeReference)
                 OPTIONAL MATCH (r2:TextReference)
                 RETURN sv.version AS schema_version,
                        count(DISTINCT e) AS entity_count,
                        count(DISTINCT c) AS category_count,
                        count(DISTINCT d) AS document_count,
                        count(DISTINCT r) + count(DISTINCT r2) AS reference_count",
            )
            .fetch_one()
            .await?;

        match row {
            Some(row) => {
                let schema_version: i64 = row.get_opt("schema_version")?.unwrap_or(0);
                let entity_count: i64 = row.get_opt("entity_count")?.unwrap_or(0);
                let category_count: i64 = row.get_opt("category_count")?.unwrap_or(0);
                let document_count: i64 = row.get_opt("document_count")?.unwrap_or(0);
                let reference_count: i64 = row.get_opt("reference_count")?.unwrap_or(0);

                Ok(ProjectStats {
                    entity_count,
                    category_count,
                    document_count,
                    reference_count,
                    schema_version: schema_version as u32,
                })
            }
            None => Err(AppError::NotInitialized),
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
