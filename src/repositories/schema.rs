//! Schema repository for migration queries and schema version tracking.

use std::sync::Arc;

use neo4rs::{query, Graph, Row};

use crate::context::Context;
use crate::di::FromContext;
use crate::error::AppError;
use crate::models::Scope;

/// Repository for schema-related queries.
#[derive(FromContext, Clone)]
pub struct SchemaRepository {
    graph: Arc<Graph>,
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
        let mut result = self
            .graph
            .execute(query(
                "MATCH (sv:SchemaVersion) RETURN sv.version AS version LIMIT 1",
            ))
            .await?;

        if let Some(row) = result.next().await? {
            let version: i64 = row.get("version").map_err(|e| AppError::Query {
                message: e.to_string(),
                query: "get schema version".to_string(),
            })?;
            Ok(version as u32)
        } else {
            Ok(0)
        }
    }

    /// Get all applied migrations.
    pub async fn get_applied_migrations(&self) -> Result<Vec<String>, AppError> {
        let mut result = self
            .graph
            .execute(query(
                "MATCH (sv:SchemaVersion) RETURN sv.applied_migrations AS migrations LIMIT 1",
            ))
            .await?;

        if let Some(row) = result.next().await? {
            let migrations: Vec<String> = row.get("migrations").unwrap_or_default();
            Ok(migrations)
        } else {
            Ok(vec![])
        }
    }

    /// List all scopes with their hierarchy.
    pub async fn list_scopes(&self) -> Result<Vec<ScopeInfo>, AppError> {
        let mut result = self
            .graph
            .execute(query(
                "MATCH (s:Scope)
                 RETURN s.name AS name, s.depth AS depth, s.description AS description
                 ORDER BY s.depth",
            ))
            .await?;

        let mut scopes = Vec::new();
        while let Some(row) = result.next().await? {
            scopes.push(Self::row_to_scope_info(&row)?);
        }
        Ok(scopes)
    }

    /// Get the scope hierarchy (which scope composes which).
    pub async fn get_scope_hierarchy(&self) -> Result<Vec<(String, String)>, AppError> {
        let mut result = self
            .graph
            .execute(query(
                "MATCH (parent:Scope)-[:COMPOSES]->(child:Scope)
                 RETURN parent.name AS parent, child.name AS child
                 ORDER BY parent.depth",
            ))
            .await?;

        let mut hierarchy = Vec::new();
        while let Some(row) = result.next().await? {
            let parent: String = row.get("parent").map_err(|e| AppError::Query {
                message: e.to_string(),
                query: "get parent scope".to_string(),
            })?;
            let child: String = row.get("child").map_err(|e| AppError::Query {
                message: e.to_string(),
                query: "get child scope".to_string(),
            })?;
            hierarchy.push((parent, child));
        }
        Ok(hierarchy)
    }

    /// Get project statistics.
    pub async fn get_project_stats(&self) -> Result<ProjectStats, AppError> {
        let mut result = self
            .graph
            .execute(query(
                "MATCH (sv:SchemaVersion)
                 OPTIONAL MATCH (e:Entity)
                 OPTIONAL MATCH (c:Category)
                 OPTIONAL MATCH (d:Document)
                 OPTIONAL MATCH (r:DocumentReference)
                 RETURN sv.version AS schema_version,
                        count(DISTINCT e) AS entity_count,
                        count(DISTINCT c) AS category_count,
                        count(DISTINCT d) AS document_count,
                        count(DISTINCT r) AS reference_count",
            ))
            .await?;

        if let Some(row) = result.next().await? {
            let schema_version: i64 = row.get("schema_version").unwrap_or(0);
            let entity_count: i64 = row.get("entity_count").unwrap_or(0);
            let category_count: i64 = row.get("category_count").unwrap_or(0);
            let document_count: i64 = row.get("document_count").unwrap_or(0);
            let reference_count: i64 = row.get("reference_count").unwrap_or(0);

            Ok(ProjectStats {
                entity_count,
                category_count,
                document_count,
                reference_count,
                schema_version: schema_version as u32,
            })
        } else {
            // No schema version means not initialized
            Err(AppError::NotInitialized)
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

    /// Convert a Neo4j row to ScopeInfo.
    fn row_to_scope_info(row: &Row) -> Result<ScopeInfo, AppError> {
        let name: String = row.get("name").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "get scope name".to_string(),
        })?;

        let depth: i64 = row.get("depth").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "get scope depth".to_string(),
        })?;

        let description: String = row.get("description").unwrap_or_default();

        Ok(ScopeInfo {
            name,
            depth: depth as u8,
            description,
        })
    }
}
