//! Schema migration - constraints and vector indexes.

use async_trait::async_trait;
use neo4rs::{query, Txn};

use crate::error::AppError;

use super::Migration;

/// Schema setup migration (DDL only - constraints and indexes).
pub struct M001Schema;

#[async_trait(?Send)]
impl Migration for M001Schema {
    fn id(&self) -> &'static str {
        "m001_schema"
    }

    fn version(&self) -> u32 {
        1
    }

    fn description(&self) -> &'static str {
        "Schema setup (constraints, vector indexes)"
    }

    async fn up(&self, txn: &mut Txn) -> Result<(), AppError> {
        self.create_constraints(txn).await?;
        self.create_vector_indexes(txn).await?;
        Ok(())
    }
}

impl M001Schema {
    /// Create uniqueness constraints for all node types.
    async fn create_constraints(&self, txn: &mut Txn) -> Result<(), AppError> {
        let constraints = [
            "CREATE CONSTRAINT index_entity_id IF NOT EXISTS FOR (e:Entity) REQUIRE e.id IS UNIQUE",
            "CREATE CONSTRAINT index_category_id IF NOT EXISTS FOR (c:Category) REQUIRE c.id IS UNIQUE",
            "CREATE CONSTRAINT index_document_id IF NOT EXISTS FOR (d:Document) REQUIRE d.id IS UNIQUE",
            "CREATE CONSTRAINT index_documentreference_id IF NOT EXISTS FOR (r:DocumentReference) REQUIRE r.id IS UNIQUE",
            "CREATE CONSTRAINT index_scope_name IF NOT EXISTS FOR (s:Scope) REQUIRE s.name IS UNIQUE",
        ];

        for constraint in constraints {
            txn.run(query(constraint)).await?;
        }
        Ok(())
    }

    /// Create vector indexes for semantic search.
    ///
    /// These may fail on older Neo4j versions - log warning but don't fail.
    async fn create_vector_indexes(&self, txn: &mut Txn) -> Result<(), AppError> {
        let indexes = [
            "CREATE VECTOR INDEX index_entity_embedding IF NOT EXISTS
             FOR (e:Entity) ON e.embedding
             OPTIONS {indexConfig: {`vector.dimensions`: 384, `vector.similarity_function`: 'cosine'}}",
            "CREATE VECTOR INDEX index_documentreference_embedding IF NOT EXISTS
             FOR (r:DocumentReference) ON r.embedding
             OPTIONS {indexConfig: {`vector.dimensions`: 384, `vector.similarity_function`: 'cosine'}}",
        ];

        for index in indexes {
            if let Err(e) = txn.run(query(index)).await {
                tracing::warn!("Could not create vector index: {}", e);
            }
        }
        Ok(())
    }
}
