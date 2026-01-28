//! Schema migration - AGE graph creation and indexes.

use crate::error::AppError;
use crate::graph::{CypherExecutor, SqlExecutor};

/// Schema setup migration (DDL only - graph creation and indexes).
pub struct M001Schema;

impl M001Schema {
    /// Apply the migration.
    pub async fn up<T>(&self, txn: &T) -> Result<(), AppError>
    where
        T: CypherExecutor + SqlExecutor + Sync,
    {
        self.create_graph(txn).await?;
        self.create_indexes(txn).await?;
        Ok(())
    }

    /// Create the AGE graph.
    ///
    /// Uses `create_graph` if it doesn't exist.
    /// AGE requires the graph to exist before running Cypher queries.
    async fn create_graph<T: SqlExecutor + Sync>(&self, txn: &T) -> Result<(), AppError> {
        // Check if graph exists first, create if not
        // AGE doesn't have IF NOT EXISTS for create_graph, so we check manually
        txn.execute_sql(
            r#"
            DO $$
            BEGIN
                IF NOT EXISTS (
                    SELECT 1 FROM ag_catalog.ag_graph WHERE name = 'knowledge_graph'
                ) THEN
                    PERFORM ag_catalog.create_graph('knowledge_graph');
                END IF;
            END $$;
            "#,
        )
        .await?;

        Ok(())
    }

    /// Create indexes for efficient lookups.
    ///
    /// In AGE, nodes are stored in PostgreSQL tables under the graph's schema.
    /// We create GIN indexes on the properties JSONB column for efficient lookups.
    ///
    /// Note: AGE label tables are created lazily when nodes of that type are first created.
    /// These indexes will be applied after the seed data migration creates the labels.
    async fn create_indexes<T: SqlExecutor + Sync>(&self, txn: &T) -> Result<(), AppError> {
        // AGE stores node properties in an 'properties' column of type agtype
        // We'll create indexes after labels exist (in a later migration or via triggers)
        //
        // For now, we create the pgvector extension for embedding support
        txn.execute_sql("CREATE EXTENSION IF NOT EXISTS vector")
            .await?;

        // Create embeddings table for vector search
        // Separate from graph for efficient vector operations with pgvector
        txn.execute_sql(
            r#"
            CREATE TABLE IF NOT EXISTS embeddings (
                id TEXT PRIMARY KEY,
                entity_type TEXT NOT NULL,
                embedding vector(384),
                created_at TIMESTAMPTZ DEFAULT NOW()
            );

            CREATE INDEX IF NOT EXISTS embeddings_entity_type_idx
            ON embeddings (entity_type);

            CREATE INDEX IF NOT EXISTS embeddings_vector_idx
            ON embeddings USING ivfflat (embedding vector_cosine_ops)
            WITH (lists = 100);
            "#,
        )
        .await?;

        Ok(())
    }
}
