//! Ontology V2 data migration - migrates existing DocumentReference nodes.

use futures::future::BoxFuture;
use futures::FutureExt;

use crate::error::AppError;
use crate::graph::Query;
use crate::migrations::{GraphMigration, GraphMigrationContext, Migration};

pub struct M003OntologyV2Data {
    graph_name: String,
}

impl M003OntologyV2Data {
    pub fn new(graph_name: &str) -> Self {
        Self {
            graph_name: graph_name.to_string(),
        }
    }

    async fn migrate_references(
        &self,
        ctx: &(dyn GraphMigrationContext + Sync),
    ) -> Result<(), AppError> {
        let now = chrono::Utc::now().to_rfc3339();

        // Migrate code references
        Query::new(
            ctx,
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

        // Migrate text references
        Query::new(
            ctx,
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

        tracing::info!("Migrated DocumentReference nodes to CodeReference and TextReference");
        Ok(())
    }
}

impl Migration for M003OntologyV2Data {
    type Context = dyn GraphMigrationContext + Sync;

    fn id(&self) -> &'static str {
        "graph003_ontology_v2_data"
    }
    fn version(&self) -> u32 {
        3
    }
    fn description(&self) -> &'static str {
        "Migrate DocumentReference to CodeReference/TextReference"
    }

    fn up<'a>(&'a self, ctx: &'a Self::Context) -> BoxFuture<'a, Result<(), AppError>> {
        async move { self.migrate_references(ctx).await }.boxed()
    }
}

impl GraphMigration for M003OntologyV2Data {
    fn graph_name(&self) -> &str {
        &self.graph_name
    }
}
