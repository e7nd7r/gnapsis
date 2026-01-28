//! Ontology V2 schema migration - indexes for CodeReference and TextReference.

use crate::error::AppError;
use crate::graph::{CypherExecutor, SqlExecutor};

/// Ontology V2 schema migration - indexes for new reference types.
pub struct M004OntologyV2;

impl M004OntologyV2 {
    /// Apply the migration.
    pub async fn up<T>(&self, txn: &T) -> Result<(), AppError>
    where
        T: CypherExecutor + SqlExecutor + Sync,
    {
        self.create_reference_indexes(txn).await?;
        Ok(())
    }

    /// Create indexes for CodeReference and TextReference nodes.
    ///
    /// In AGE, we create PostgreSQL indexes on the label tables.
    /// These tables are created lazily, so we use a helper function
    /// that checks if the table exists before creating indexes.
    ///
    /// Note: AGE stores properties as `agtype`. To index, we use
    /// `ag_catalog.agtype_access_operator` which extracts values.
    async fn create_reference_indexes<T: SqlExecutor + Sync>(
        &self,
        txn: &T,
    ) -> Result<(), AppError> {
        // Create a function to set up reference indexes
        // This can be called again later if needed
        // Note: AGE uses agtype for properties, so we use agtype_access_operator
        txn.execute_sql(
            r#"
            CREATE OR REPLACE FUNCTION create_reference_indexes()
            RETURNS void AS $$
            BEGIN
                -- CodeReference indexes
                IF EXISTS (
                    SELECT 1 FROM information_schema.tables
                    WHERE table_schema = 'knowledge_graph' AND table_name = 'CodeReference'
                ) THEN
                    -- Index on id property for lookups (using agtype accessor)
                    EXECUTE 'CREATE INDEX IF NOT EXISTS idx_codereference_id
                        ON knowledge_graph."CodeReference" ((ag_catalog.agtype_access_operator(properties, ''"id"'')::text))';

                    -- Index on path for file-based queries
                    EXECUTE 'CREATE INDEX IF NOT EXISTS idx_codereference_path
                        ON knowledge_graph."CodeReference" ((ag_catalog.agtype_access_operator(properties, ''"path"'')::text))';

                    RAISE NOTICE 'Created indexes on CodeReference table';
                END IF;

                -- TextReference indexes
                IF EXISTS (
                    SELECT 1 FROM information_schema.tables
                    WHERE table_schema = 'knowledge_graph' AND table_name = 'TextReference'
                ) THEN
                    -- Index on id property for lookups
                    EXECUTE 'CREATE INDEX IF NOT EXISTS idx_textreference_id
                        ON knowledge_graph."TextReference" ((ag_catalog.agtype_access_operator(properties, ''"id"'')::text))';

                    -- Index on path for file-based queries
                    EXECUTE 'CREATE INDEX IF NOT EXISTS idx_textreference_path
                        ON knowledge_graph."TextReference" ((ag_catalog.agtype_access_operator(properties, ''"path"'')::text))';

                    RAISE NOTICE 'Created indexes on TextReference table';
                END IF;

                -- Entity indexes (may not have been created in M001 if table didn't exist)
                IF EXISTS (
                    SELECT 1 FROM information_schema.tables
                    WHERE table_schema = 'knowledge_graph' AND table_name = 'Entity'
                ) THEN
                    EXECUTE 'CREATE INDEX IF NOT EXISTS idx_entity_id
                        ON knowledge_graph."Entity" ((ag_catalog.agtype_access_operator(properties, ''"id"'')::text))';

                    EXECUTE 'CREATE INDEX IF NOT EXISTS idx_entity_name
                        ON knowledge_graph."Entity" ((ag_catalog.agtype_access_operator(properties, ''"name"'')::text))';

                    RAISE NOTICE 'Created indexes on Entity table';
                END IF;

                -- Category indexes
                IF EXISTS (
                    SELECT 1 FROM information_schema.tables
                    WHERE table_schema = 'knowledge_graph' AND table_name = 'Category'
                ) THEN
                    EXECUTE 'CREATE INDEX IF NOT EXISTS idx_category_id
                        ON knowledge_graph."Category" ((ag_catalog.agtype_access_operator(properties, ''"id"'')::text))';

                    EXECUTE 'CREATE INDEX IF NOT EXISTS idx_category_name
                        ON knowledge_graph."Category" ((ag_catalog.agtype_access_operator(properties, ''"name"'')::text))';

                    RAISE NOTICE 'Created indexes on Category table';
                END IF;
            END;
            $$ LANGUAGE plpgsql;
            "#,
        )
        .await?;

        // Try to create indexes now (tables may exist from M003)
        txn.execute_sql("SELECT create_reference_indexes()").await?;

        tracing::info!("Created reference index setup function and applied available indexes");
        Ok(())
    }
}
