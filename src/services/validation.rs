//! Validation service for checking graph integrity.

use serde::Serialize;

use crate::context::{AppGraph, Context};
use crate::di::FromContext;
use crate::error::AppError;
use crate::graph::Row;

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
    graph: AppGraph,
}

impl ValidationService {
    /// Find entities at non-Domain scope that have no parent (orphans).
    ///
    /// Entities at Domain scope are allowed to have no parent since they're
    /// at the top of the hierarchy. All other scopes should have a parent.
    pub async fn find_orphan_entities(&self) -> Result<Vec<ValidationIssue>, AppError> {
        // Use NOT EXISTS { pattern } to find entities without parents
        let rows = self
            .graph
            .query(
                "MATCH (e:Entity)-[:CLASSIFIED_AS]->(c:Category)-[:IN_SCOPE]->(s:Scope)
                 WHERE s.name <> 'Domain' AND NOT EXISTS { (e)-[:BELONGS_TO]->(:Entity) }
                 RETURN e.id AS id, e.name AS name, s.name AS scope",
            )
            .fetch_all()
            .await?;

        rows.iter().map(Self::row_to_orphan_issue).collect()
    }

    /// Find entities involved in BELONGS_TO cycles.
    ///
    /// A cycle means an entity eventually belongs to itself through the
    /// composition hierarchy, which is invalid.
    pub async fn find_cycles(&self) -> Result<Vec<ValidationIssue>, AppError> {
        // Use EXISTS { pattern } to detect cycles - entities that can reach themselves
        let rows = self
            .graph
            .query(
                "MATCH (e:Entity)
                 WHERE EXISTS { (e)-[:BELONGS_TO*]->(e) }
                 RETURN e.id AS id, e.name AS name",
            )
            .fetch_all()
            .await?;

        rows.iter().map(Self::row_to_cycle_issue).collect()
    }

    /// Find scope violations where child scope is not deeper than parent.
    ///
    /// The hierarchy flows: Domain(1) → Feature(2) → Namespace(3) → Component(4) → Unit(5)
    /// A child's scope depth must be greater than its parent's scope depth.
    pub async fn find_scope_violations(&self) -> Result<Vec<ValidationIssue>, AppError> {
        // AGE requires named variables for all nodes in path patterns
        let rows = self
            .graph
            .query(
                "MATCH (child:Entity)-[:BELONGS_TO]->(parent:Entity)
                 MATCH (child)-[:CLASSIFIED_AS]->(childCat:Category)-[:IN_SCOPE]->(childScope:Scope)
                 MATCH (parent)-[:CLASSIFIED_AS]->(parentCat:Category)-[:IN_SCOPE]->(parentScope:Scope)
                 WHERE childScope.depth <= parentScope.depth
                 RETURN child.id AS child_id, child.name AS child_name,
                        parent.id AS parent_id, parent.name AS parent_name,
                        childScope.name AS child_scope, parentScope.name AS parent_scope",
            )
            .fetch_all()
            .await?;

        rows.iter().map(Self::row_to_scope_violation).collect()
    }

    /// Find entities without any classification.
    ///
    /// All entities should be classified with at least one category.
    pub async fn find_unclassified_entities(&self) -> Result<Vec<ValidationIssue>, AppError> {
        // Use NOT EXISTS { pattern } to find unclassified entities
        let rows = self
            .graph
            .query(
                "MATCH (e:Entity)
                 WHERE NOT EXISTS { (e)-[:CLASSIFIED_AS]->(:Category) }
                 RETURN e.id AS id, e.name AS name",
            )
            .fetch_all()
            .await?;

        rows.iter().map(Self::row_to_unclassified_issue).collect()
    }

    /// Find entities without any document references.
    ///
    /// All entities should have at least one reference anchoring them to code
    /// or documentation. Entities without references are "floating knowledge"
    /// that can't be verified against the codebase.
    pub async fn find_entities_without_references(&self) -> Result<Vec<ValidationIssue>, AppError> {
        // Use NOT EXISTS { pattern } to find entities without any references
        let rows = self
            .graph
            .query(
                "MATCH (e:Entity)
                 WHERE NOT EXISTS { (e)-[:HAS_REFERENCE]->(:CodeReference) }
                   AND NOT EXISTS { (e)-[:HAS_REFERENCE]->(:TextReference) }
                 RETURN e.id AS id, e.name AS name",
            )
            .fetch_all()
            .await?;

        rows.iter().map(Self::row_to_no_references_issue).collect()
    }

    // Row conversion helpers

    fn row_to_orphan_issue(row: &Row) -> Result<ValidationIssue, AppError> {
        let id: String = row.get_opt("id")?.unwrap_or_default();
        let name: String = row.get_opt("name")?.unwrap_or_default();
        let scope: String = row.get_opt("scope")?.unwrap_or_default();

        Ok(ValidationIssue {
            entity_id: id,
            entity_name: name,
            issue: format!("Entity at {} scope has no parent", scope),
        })
    }

    fn row_to_cycle_issue(row: &Row) -> Result<ValidationIssue, AppError> {
        let id: String = row.get_opt("id")?.unwrap_or_default();
        let name: String = row.get_opt("name")?.unwrap_or_default();

        Ok(ValidationIssue {
            entity_id: id,
            entity_name: name,
            issue: "Entity is part of a BELONGS_TO cycle".to_string(),
        })
    }

    fn row_to_scope_violation(row: &Row) -> Result<ValidationIssue, AppError> {
        let child_id: String = row.get_opt("child_id")?.unwrap_or_default();
        let child_name: String = row.get_opt("child_name")?.unwrap_or_default();
        let parent_name: String = row.get_opt("parent_name")?.unwrap_or_default();
        let child_scope: String = row.get_opt("child_scope")?.unwrap_or_default();
        let parent_scope: String = row.get_opt("parent_scope")?.unwrap_or_default();

        Ok(ValidationIssue {
            entity_id: child_id,
            entity_name: child_name.clone(),
            issue: format!(
                "Scope violation: {} ({}) belongs to {} ({}) - child must be deeper",
                child_name, child_scope, parent_name, parent_scope
            ),
        })
    }

    fn row_to_unclassified_issue(row: &Row) -> Result<ValidationIssue, AppError> {
        let id: String = row.get_opt("id")?.unwrap_or_default();
        let name: String = row.get_opt("name")?.unwrap_or_default();

        Ok(ValidationIssue {
            entity_id: id,
            entity_name: name,
            issue: "Entity has no classification".to_string(),
        })
    }

    fn row_to_no_references_issue(row: &Row) -> Result<ValidationIssue, AppError> {
        let id: String = row.get_opt("id")?.unwrap_or_default();
        let name: String = row.get_opt("name")?.unwrap_or_default();

        Ok(ValidationIssue {
            entity_id: id,
            entity_name: name,
            issue: "Entity has no document references".to_string(),
        })
    }
}
