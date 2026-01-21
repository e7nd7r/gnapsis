//! Ontology V2 data migration - migrates existing DocumentReference nodes.

use async_trait::async_trait;
use neo4rs::{query, Txn};

use crate::error::AppError;

use super::Migration;

/// Ontology V2 data migration - migrate DocumentReference to CodeReference/TextReference.
pub struct M005OntologyV2Data;

#[async_trait]
impl Migration for M005OntologyV2Data {
    fn id(&self) -> &'static str {
        "m005_ontology_v2_data"
    }

    fn version(&self) -> u32 {
        5
    }

    fn description(&self) -> &'static str {
        "Migrate DocumentReference nodes to CodeReference/TextReference"
    }

    async fn up(&self, txn: &mut Txn) -> Result<(), AppError> {
        self.migrate_references(txn).await
    }
}

impl M005OntologyV2Data {
    /// Migrate existing DocumentReference nodes to CodeReference or TextReference.
    ///
    /// Migration logic:
    /// - If content_type starts with "code:" -> CodeReference
    /// - Otherwise (markdown, text, etc.) -> TextReference
    async fn migrate_references(&self, txn: &mut Txn) -> Result<(), AppError> {
        // Migrate code references (content_type starts with "code:")
        txn.run(query(
            "MATCH (old:DocumentReference)
             WHERE old.content_type STARTS WITH 'code:'
             WITH old
             CREATE (new:CodeReference {
                 id: old.id,
                 path: old.document_path,
                 language: CASE
                     WHEN old.content_type STARTS WITH 'code:'
                     THEN substring(old.content_type, 5)
                     ELSE 'unknown'
                 END,
                 commit_sha: coalesce(old.commit_sha, ''),
                 description: coalesce(old.description, ''),
                 embedding: old.embedding,
                 lsp_symbol: coalesce(old.lsp_symbol, ''),
                 lsp_kind: coalesce(old.lsp_kind, 0),
                 lsp_range: coalesce(old.lsp_range, ''),
                 created_at: coalesce(old.created_at, datetime())
             })
             WITH old, new
             MATCH (old)<-[r:HAS_REFERENCE]-(e:Entity)
             CREATE (e)-[:HAS_REFERENCE]->(new)
             WITH old, new
             MATCH (old)-[:IN_DOCUMENT]->(d:Document)
             CREATE (new)-[:IN_DOCUMENT]->(d)",
        ))
        .await?;

        // Migrate text references (markdown, text, or anything else)
        txn.run(query(
            "MATCH (old:DocumentReference)
             WHERE NOT old.content_type STARTS WITH 'code:'
             WITH old
             CREATE (new:TextReference {
                 id: old.id,
                 path: old.document_path,
                 content_type: coalesce(old.content_type, 'markdown'),
                 commit_sha: coalesce(old.commit_sha, ''),
                 description: coalesce(old.description, ''),
                 embedding: old.embedding,
                 start_line: coalesce(old.start_line, 0),
                 end_line: coalesce(old.end_line, 0),
                 anchor: old.anchor,
                 created_at: coalesce(old.created_at, datetime())
             })
             WITH old, new
             MATCH (old)<-[r:HAS_REFERENCE]-(e:Entity)
             CREATE (e)-[:HAS_REFERENCE]->(new)
             WITH old, new
             MATCH (old)-[:IN_DOCUMENT]->(d:Document)
             CREATE (new)-[:IN_DOCUMENT]->(d)",
        ))
        .await?;

        // Note: We don't delete old DocumentReference nodes to keep migrations additive.
        // They can be cleaned up manually or in a future cleanup task.

        tracing::info!("Migrated DocumentReference nodes to CodeReference and TextReference");
        Ok(())
    }
}
