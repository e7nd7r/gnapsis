//! Validation service for checking graph integrity.

use std::sync::Arc;

use neo4rs::{query, Graph};
use serde::Serialize;

use crate::context::Context;
use crate::di::FromContext;
use crate::error::AppError;

/// A validation issue with an entity.
#[derive(Debug, Serialize)]
pub struct ValidationIssue {
    /// Entity ID.
    pub entity_id: String,
    /// Entity name.
    pub entity_name: String,
    /// Description of the issue.
    pub issue: String,
}

/// Service for validating graph integrity.
///
/// Checks for common issues like orphan entities, cycles in composition,
/// scope violations, and missing classifications.
#[derive(FromContext, Clone)]
pub struct ValidationService {
    graph: Arc<Graph>,
}

impl ValidationService {
    /// Find entities at non-Domain scope that have no parent (orphans).
    ///
    /// Entities at Domain scope are allowed to have no parent since they're
    /// at the top of the hierarchy. All other scopes should have a parent.
    pub async fn find_orphan_entities(&self) -> Result<Vec<ValidationIssue>, AppError> {
        let mut result = self
            .graph
            .execute(query(
                "MATCH (e:Entity)-[:CLASSIFIED_AS]->(c:Category)-[:IN_SCOPE]->(s:Scope)
                 WHERE s.name <> 'Domain'
                 AND NOT (e)-[:BELONGS_TO]->(:Entity)
                 RETURN e.id AS id, e.name AS name, s.name AS scope",
            ))
            .await?;

        let mut issues = Vec::new();
        while let Some(row) = result.next().await? {
            let id: String = row.get("id").unwrap_or_default();
            let name: String = row.get("name").unwrap_or_default();
            let scope: String = row.get("scope").unwrap_or_default();

            issues.push(ValidationIssue {
                entity_id: id,
                entity_name: name,
                issue: format!("Entity at {} scope has no parent", scope),
            });
        }

        Ok(issues)
    }

    /// Find entities involved in BELONGS_TO cycles.
    ///
    /// A cycle means an entity eventually belongs to itself through the
    /// composition hierarchy, which is invalid.
    pub async fn find_cycles(&self) -> Result<Vec<ValidationIssue>, AppError> {
        // Use a path query to detect cycles - entities that can reach themselves
        let mut result = self
            .graph
            .execute(query(
                "MATCH (e:Entity)
                 WHERE EXISTS {
                     MATCH path = (e)-[:BELONGS_TO*]->(e)
                 }
                 RETURN DISTINCT e.id AS id, e.name AS name",
            ))
            .await?;

        let mut issues = Vec::new();
        while let Some(row) = result.next().await? {
            let id: String = row.get("id").unwrap_or_default();
            let name: String = row.get("name").unwrap_or_default();

            issues.push(ValidationIssue {
                entity_id: id,
                entity_name: name,
                issue: "Entity is part of a BELONGS_TO cycle".to_string(),
            });
        }

        Ok(issues)
    }

    /// Find scope violations where child scope is not deeper than parent.
    ///
    /// The hierarchy flows: Domain(1) → Feature(2) → Namespace(3) → Component(4) → Unit(5)
    /// A child's scope depth must be greater than its parent's scope depth.
    pub async fn find_scope_violations(&self) -> Result<Vec<ValidationIssue>, AppError> {
        let mut result = self
            .graph
            .execute(query(
                "MATCH (child:Entity)-[:BELONGS_TO]->(parent:Entity)
                 MATCH (child)-[:CLASSIFIED_AS]->(:Category)-[:IN_SCOPE]->(childScope:Scope)
                 MATCH (parent)-[:CLASSIFIED_AS]->(:Category)-[:IN_SCOPE]->(parentScope:Scope)
                 WHERE childScope.depth <= parentScope.depth
                 RETURN child.id AS child_id, child.name AS child_name,
                        parent.id AS parent_id, parent.name AS parent_name,
                        childScope.name AS child_scope, parentScope.name AS parent_scope",
            ))
            .await?;

        let mut issues = Vec::new();
        while let Some(row) = result.next().await? {
            let child_id: String = row.get("child_id").unwrap_or_default();
            let child_name: String = row.get("child_name").unwrap_or_default();
            let parent_name: String = row.get("parent_name").unwrap_or_default();
            let child_scope: String = row.get("child_scope").unwrap_or_default();
            let parent_scope: String = row.get("parent_scope").unwrap_or_default();

            let issue_msg = format!(
                "Scope violation: {} ({}) belongs to {} ({}) - child must be deeper",
                &child_name, child_scope, parent_name, parent_scope
            );
            issues.push(ValidationIssue {
                entity_id: child_id,
                entity_name: child_name,
                issue: issue_msg,
            });
        }

        Ok(issues)
    }

    /// Find entities without any classification.
    ///
    /// All entities should be classified with at least one category.
    pub async fn find_unclassified_entities(&self) -> Result<Vec<ValidationIssue>, AppError> {
        let mut result = self
            .graph
            .execute(query(
                "MATCH (e:Entity)
                 WHERE NOT (e)-[:CLASSIFIED_AS]->(:Category)
                 RETURN e.id AS id, e.name AS name",
            ))
            .await?;

        let mut issues = Vec::new();
        while let Some(row) = result.next().await? {
            let id: String = row.get("id").unwrap_or_default();
            let name: String = row.get("name").unwrap_or_default();

            issues.push(ValidationIssue {
                entity_id: id,
                entity_name: name,
                issue: "Entity has no classification".to_string(),
            });
        }

        Ok(issues)
    }

    /// Find entities without any document references.
    ///
    /// All entities should have at least one reference anchoring them to code
    /// or documentation. Entities without references are "floating knowledge"
    /// that can't be verified against the codebase.
    pub async fn find_entities_without_references(&self) -> Result<Vec<ValidationIssue>, AppError> {
        let mut result = self
            .graph
            .execute(query(
                "MATCH (e:Entity)
                 WHERE NOT (e)-[:HAS_REFERENCE]->(:CodeReference)
                   AND NOT (e)-[:HAS_REFERENCE]->(:TextReference)
                 RETURN e.id AS id, e.name AS name",
            ))
            .await?;

        let mut issues = Vec::new();
        while let Some(row) = result.next().await? {
            let id: String = row.get("id").unwrap_or_default();
            let name: String = row.get("name").unwrap_or_default();

            issues.push(ValidationIssue {
                entity_id: id,
                entity_name: name,
                issue: "Entity has no document references".to_string(),
            });
        }

        Ok(issues)
    }
}
