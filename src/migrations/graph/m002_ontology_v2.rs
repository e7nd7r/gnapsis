//! Ontology V2 schema migration - indexes for CodeReference and TextReference.

use futures::future::BoxFuture;
use futures::FutureExt;

use crate::error::AppError;
use crate::migrations::{GraphMigration, GraphMigrationContext, Migration};

pub struct M002OntologyV2 {
    graph_name: String,
}

impl M002OntologyV2 {
    pub fn new(graph_name: &str) -> Self {
        Self {
            graph_name: graph_name.to_string(),
        }
    }

    async fn create_reference_indexes(
        &self,
        ctx: &(dyn GraphMigrationContext + Sync),
    ) -> Result<(), AppError> {
        let graph = &self.graph_name;

        let sql = format!(
            r#"
            CREATE OR REPLACE FUNCTION create_reference_indexes_{graph}()
            RETURNS void AS $$
            BEGIN
                IF EXISTS (
                    SELECT 1 FROM information_schema.tables
                    WHERE table_schema = '{graph}' AND table_name = 'CodeReference'
                ) THEN
                    EXECUTE 'CREATE INDEX IF NOT EXISTS idx_{graph}_codereference_id
                        ON {graph}."CodeReference" ((ag_catalog.agtype_access_operator(properties, ''"id"'')::text))';
                    EXECUTE 'CREATE INDEX IF NOT EXISTS idx_{graph}_codereference_path
                        ON {graph}."CodeReference" ((ag_catalog.agtype_access_operator(properties, ''"path"'')::text))';
                END IF;

                IF EXISTS (
                    SELECT 1 FROM information_schema.tables
                    WHERE table_schema = '{graph}' AND table_name = 'TextReference'
                ) THEN
                    EXECUTE 'CREATE INDEX IF NOT EXISTS idx_{graph}_textreference_id
                        ON {graph}."TextReference" ((ag_catalog.agtype_access_operator(properties, ''"id"'')::text))';
                    EXECUTE 'CREATE INDEX IF NOT EXISTS idx_{graph}_textreference_path
                        ON {graph}."TextReference" ((ag_catalog.agtype_access_operator(properties, ''"path"'')::text))';
                END IF;

                IF EXISTS (
                    SELECT 1 FROM information_schema.tables
                    WHERE table_schema = '{graph}' AND table_name = 'Entity'
                ) THEN
                    EXECUTE 'CREATE INDEX IF NOT EXISTS idx_{graph}_entity_id
                        ON {graph}."Entity" ((ag_catalog.agtype_access_operator(properties, ''"id"'')::text))';
                    EXECUTE 'CREATE INDEX IF NOT EXISTS idx_{graph}_entity_name
                        ON {graph}."Entity" ((ag_catalog.agtype_access_operator(properties, ''"name"'')::text))';
                END IF;

                IF EXISTS (
                    SELECT 1 FROM information_schema.tables
                    WHERE table_schema = '{graph}' AND table_name = 'Category'
                ) THEN
                    EXECUTE 'CREATE INDEX IF NOT EXISTS idx_{graph}_category_id
                        ON {graph}."Category" ((ag_catalog.agtype_access_operator(properties, ''"id"'')::text))';
                    EXECUTE 'CREATE INDEX IF NOT EXISTS idx_{graph}_category_name
                        ON {graph}."Category" ((ag_catalog.agtype_access_operator(properties, ''"name"'')::text))';
                END IF;
            END;
            $$ LANGUAGE plpgsql;
            "#,
            graph = graph
        );

        ctx.execute_sql(&sql).await?;
        ctx.execute_sql(&format!("SELECT create_reference_indexes_{}()", graph))
            .await?;

        tracing::info!("Created reference indexes for graph '{}'", graph);
        Ok(())
    }
}

impl Migration for M002OntologyV2 {
    type Context = dyn GraphMigrationContext + Sync;

    fn id(&self) -> &'static str {
        "graph002_ontology_v2"
    }
    fn version(&self) -> u32 {
        2
    }
    fn description(&self) -> &'static str {
        "Ontology V2 schema (reference indexes)"
    }

    fn up<'a>(&'a self, ctx: &'a Self::Context) -> BoxFuture<'a, Result<(), AppError>> {
        async move { self.create_reference_indexes(ctx).await }.boxed()
    }
}

impl GraphMigration for M002OntologyV2 {
    fn graph_name(&self) -> &str {
        &self.graph_name
    }
}
