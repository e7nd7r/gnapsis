//! Ontology V2 schema migration - constraints and indexes for CodeReference and TextReference.

use async_trait::async_trait;
use neo4rs::{query, Txn};

use crate::error::AppError;

use super::Migration;

/// Ontology V2 schema migration - constraints and indexes.
pub struct M004OntologyV2;

#[async_trait]
impl Migration for M004OntologyV2 {
    fn id(&self) -> &'static str {
        "m004_ontology_v2"
    }

    fn version(&self) -> u32 {
        4
    }

    fn description(&self) -> &'static str {
        "Ontology V2 schema (CodeReference and TextReference constraints/indexes)"
    }

    async fn up(&self, txn: &mut Txn) -> Result<(), AppError> {
        self.create_constraints(txn).await?;
        self.create_vector_indexes(txn).await?;
        Ok(())
    }
}

impl M004OntologyV2 {
    /// Create uniqueness constraints for new node types.
    async fn create_constraints(&self, txn: &mut Txn) -> Result<(), AppError> {
        let constraints = [
            "CREATE CONSTRAINT index_codereference_id IF NOT EXISTS FOR (r:CodeReference) REQUIRE r.id IS UNIQUE",
            "CREATE CONSTRAINT index_textreference_id IF NOT EXISTS FOR (r:TextReference) REQUIRE r.id IS UNIQUE",
        ];

        for constraint in constraints {
            txn.run(query(constraint)).await?;
        }
        Ok(())
    }

    /// Create vector indexes for semantic search on new reference types.
    async fn create_vector_indexes(&self, txn: &mut Txn) -> Result<(), AppError> {
        let indexes = [
            "CREATE VECTOR INDEX index_codereference_embedding IF NOT EXISTS
             FOR (r:CodeReference) ON r.embedding
             OPTIONS {indexConfig: {`vector.dimensions`: 384, `vector.similarity_function`: 'cosine'}}",
            "CREATE VECTOR INDEX index_textreference_embedding IF NOT EXISTS
             FOR (r:TextReference) ON r.embedding
             OPTIONS {indexConfig: {`vector.dimensions`: 384, `vector.similarity_function`: 'cosine'}}",
        ];

        for index in indexes {
            if let Err(e) = txn.run(query(index)).await {
                tracing::warn!("Could not create vector index: {}", e);
            }
        }

        // Note: Neo4j doesn't support vector indexes on relationship properties,
        // so RELATED_TO.embedding will be searched using gds.similarity.cosine() function.

        Ok(())
    }
}
