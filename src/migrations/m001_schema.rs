//! Schema migration - indexes and extensions.
//!
//! Note: Graph creation is handled by `PostgresClient::ensure_graph_exists()`
//! before migrations run. This migration sets up indexes and extensions.

use async_trait::async_trait;

use crate::error::AppError;
use crate::migrations::{Migration, MigrationContext};

/// Schema setup migration (DDL only - indexes and extensions).
pub struct M001Schema {
    graph_name: String,
}

impl M001Schema {
    /// Create a new schema migration for the given graph.
    pub fn new(graph_name: &str) -> Self {
        Self {
            graph_name: graph_name.to_string(),
        }
    }

    /// Create indexes for efficient lookups.
    ///
    /// In AGE, nodes are stored in PostgreSQL tables under the graph's schema.
    /// We create GIN indexes on the properties JSONB column for efficient lookups.
    ///
    /// Note: AGE label tables are created lazily when nodes of that type are first created.
    /// These indexes will be applied after the seed data migration creates the labels.
    async fn create_indexes(&self, ctx: &(dyn MigrationContext + Sync)) -> Result<(), AppError> {
        // AGE stores node properties in an 'properties' column of type agtype
        // We'll create indexes after labels exist (in a later migration or via triggers)
        //
        // For now, we create the pgvector extension for embedding support
        ctx.execute_sql("CREATE EXTENSION IF NOT EXISTS vector")
            .await?;

        // Create embeddings table for vector search
        // Separate from graph for efficient vector operations with pgvector
        ctx.execute_sql(
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

#[async_trait]
impl Migration for M001Schema {
    fn id(&self) -> &'static str {
        "m001_schema"
    }

    fn version(&self) -> u32 {
        1
    }

    fn description(&self) -> &'static str {
        "Schema setup (indexes and extensions)"
    }

    fn graph_name(&self) -> &str {
        &self.graph_name
    }

    async fn up(&self, ctx: &(dyn MigrationContext + Sync)) -> Result<(), AppError> {
        self.create_indexes(ctx).await?;
        Ok(())
    }
}
