//! Schema migration - extensions and global tables.

use futures::future::BoxFuture;
use futures::FutureExt;

use crate::error::AppError;
use crate::graph::SqlExecutor;
use crate::migrations::Migration;

/// Database schema setup (extensions and global tables).
pub struct M001Schema;

impl Migration for M001Schema {
    type Context = dyn SqlExecutor + Sync;

    fn id(&self) -> &'static str {
        "db001_schema"
    }

    fn version(&self) -> u32 {
        1
    }

    fn description(&self) -> &'static str {
        "Database setup (pgvector extension, embeddings table)"
    }

    fn up<'a>(&'a self, ctx: &'a Self::Context) -> BoxFuture<'a, Result<(), AppError>> {
        async move {
            // pgvector extension
            ctx.execute_sql("CREATE EXTENSION IF NOT EXISTS vector")
                .await?;

            // Embeddings table for vector search
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
        .boxed()
    }
}
