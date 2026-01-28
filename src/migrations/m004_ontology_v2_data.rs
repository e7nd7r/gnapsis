//! Ontology V2 data migration - migrates existing DocumentReference nodes.

use crate::error::AppError;
use crate::graph::{CypherExecutor, QueryExt, SqlExecutor};

/// Ontology V2 data migration - migrate DocumentReference to CodeReference/TextReference.
pub struct M004OntologyV2Data;

impl M004OntologyV2Data {
    /// Apply the migration.
    pub async fn up<T>(&self, txn: &T) -> Result<(), AppError>
    where
        T: CypherExecutor + SqlExecutor + Sync,
    {
        self.migrate_references(txn).await
    }

    /// Migrate existing DocumentReference nodes to CodeReference or TextReference.
    ///
    /// Migration logic:
    /// - If content_type starts with "code:" -> CodeReference
    /// - Otherwise (markdown, text, etc.) -> TextReference
    async fn migrate_references<T: CypherExecutor + Sync>(&self, txn: &T) -> Result<(), AppError> {
        // Get current timestamp as ISO 8601 string for AGE compatibility
        let now = chrono::Utc::now().to_rfc3339();

        // Migrate code references (content_type starts with "code:")
        txn.query(
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
                 lsp_symbol: coalesce(old.lsp_symbol, ''),
                 lsp_kind: coalesce(old.lsp_kind, 0),
                 lsp_range: coalesce(old.lsp_range, ''),
                 created_at: coalesce(old.created_at, $now)
             })
             WITH old, new
             MATCH (old)<-[r:HAS_REFERENCE]-(e:Entity)
             CREATE (e)-[:HAS_REFERENCE]->(new)
             WITH old, new
             MATCH (old)-[:IN_DOCUMENT]->(d:Document)
             CREATE (new)-[:IN_DOCUMENT]->(d)",
        )
        .param("now", &now)
        .run()
        .await?;

        // Migrate text references (markdown, text, or anything else)
        txn.query(
            "MATCH (old:DocumentReference)
             WHERE NOT old.content_type STARTS WITH 'code:'
             WITH old
             CREATE (new:TextReference {
                 id: old.id,
                 path: old.document_path,
                 content_type: coalesce(old.content_type, 'markdown'),
                 commit_sha: coalesce(old.commit_sha, ''),
                 description: coalesce(old.description, ''),
                 start_line: coalesce(old.start_line, 0),
                 end_line: coalesce(old.end_line, 0),
                 anchor: old.anchor,
                 created_at: coalesce(old.created_at, $now)
             })
             WITH old, new
             MATCH (old)<-[r:HAS_REFERENCE]-(e:Entity)
             CREATE (e)-[:HAS_REFERENCE]->(new)
             WITH old, new
             MATCH (old)-[:IN_DOCUMENT]->(d:Document)
             CREATE (new)-[:IN_DOCUMENT]->(d)",
        )
        .param("now", &now)
        .run()
        .await?;

        // Note: We don't delete old DocumentReference nodes to keep migrations additive.
        // They can be cleaned up manually or in a future cleanup task.

        tracing::info!("Migrated DocumentReference nodes to CodeReference and TextReference");
        Ok(())
    }
}
