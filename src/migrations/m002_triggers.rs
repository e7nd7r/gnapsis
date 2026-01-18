//! APOC triggers migration for domain constraint enforcement.

use async_trait::async_trait;
use neo4rs::{query, Txn};

use crate::error::AppError;

use super::Migration;

/// APOC triggers migration.
pub struct M002Triggers;

#[async_trait]
impl Migration for M002Triggers {
    fn id(&self) -> &'static str {
        "m002_triggers"
    }

    fn version(&self) -> u32 {
        2
    }

    fn description(&self) -> &'static str {
        "APOC triggers for domain constraints"
    }

    async fn up(&self, txn: &mut Txn) -> Result<(), AppError> {
        self.create_apoc_triggers(txn).await
    }
}

impl M002Triggers {
    /// Create APOC triggers for domain constraint enforcement.
    ///
    /// Requires `apoc.trigger.enabled=true` in neo4j.conf.
    /// These may fail if APOC is not installed - log warning but don't fail.
    async fn create_apoc_triggers(&self, txn: &mut Txn) -> Result<(), AppError> {
        // Trigger 1: Validate BELONGS_TO scope constraint
        // Child scope depth must be greater than parent scope depth
        let validate_belongs_to = r#"
            CALL apoc.trigger.add('validate_belongs_to',
              "UNWIND $createdRelationships AS r
               WITH r
               WHERE type(r) = 'BELONGS_TO'
               MATCH (child)-[r]->(parent)
               MATCH (child)-[:CLASSIFIED_AS]->(:Category)-[:IN_SCOPE]->(cs:Scope)
               MATCH (parent)-[:CLASSIFIED_AS]->(:Category)-[:IN_SCOPE]->(ps:Scope)
               CALL apoc.util.validate(
                 cs.depth <= ps.depth,
                 'Invalid BELONGS_TO: %s (depth %d) cannot belong to %s (depth %d)',
                 [child.name, cs.depth, parent.name, ps.depth]
               )
               RETURN null",
              {phase: 'before'})
        "#;

        // Trigger 2: Validate single category per scope
        // Entity can have at most ONE category per scope
        let validate_single_category = r#"
            CALL apoc.trigger.add('validate_single_category_per_scope',
              "UNWIND $createdRelationships AS r
               WITH r
               WHERE type(r) = 'CLASSIFIED_AS'
               MATCH (e:Entity)-[r]->(newCat:Category)-[:IN_SCOPE]->(s:Scope)
               MATCH (e)-[:CLASSIFIED_AS]->(existingCat:Category)-[:IN_SCOPE]->(s)
               WHERE existingCat <> newCat
               CALL apoc.util.validate(
                 true,
                 'Entity %s already has category %s at scope %s, cannot add %s',
                 [e.name, existingCat.name, s.name, newCat.name]
               )
               RETURN null",
              {phase: 'before'})
        "#;

        // Trigger 3: Prevent delete with children
        // Cannot delete entity that has children
        let prevent_delete_with_children = r#"
            CALL apoc.trigger.add('prevent_delete_with_children',
              "UNWIND $deletedNodes AS n
               WITH n
               WHERE n:Entity
               MATCH (child:Entity)-[:BELONGS_TO]->(n)
               CALL apoc.util.validate(
                 true,
                 'Cannot delete entity %s: has children',
                 [n.name]
               )
               RETURN null",
              {phase: 'before'})
        "#;

        // Trigger 4: Cascade delete entity
        // Cleanup DocumentReferences when entity is deleted
        let cascade_delete_entity = r#"
            CALL apoc.trigger.add('cascade_delete_entity',
              "UNWIND $deletedNodes AS n
               WITH n
               WHERE n:Entity
               OPTIONAL MATCH (n)-[:HAS_REFERENCE]->(ref:DocumentReference)
               DETACH DELETE ref",
              {phase: 'after'})
        "#;

        let triggers = [
            ("validate_belongs_to", validate_belongs_to),
            (
                "validate_single_category_per_scope",
                validate_single_category,
            ),
            ("prevent_delete_with_children", prevent_delete_with_children),
            ("cascade_delete_entity", cascade_delete_entity),
        ];

        for (name, trigger_query) in triggers {
            if let Err(e) = txn.run(query(trigger_query)).await {
                tracing::warn!("Could not create APOC trigger '{}': {}", name, e);
            }
        }
        Ok(())
    }
}
