//! Document and reference repository for tracking code locations.
//!
//! Supports two reference types:
//! - `CodeReference` - For source code with LSP metadata
//! - `TextReference` - For markdown/text with line ranges and anchors

use std::sync::Arc;

use neo4rs::{query, Graph, Row};

use crate::context::Context;
use crate::di::FromContext;
use crate::error::AppError;
use crate::models::{generate_ulid, CodeReference, Document, Reference, TextReference};

/// Parameters for creating a code reference.
pub struct CreateCodeReferenceParams<'a> {
    pub entity_id: &'a str,
    pub path: &'a str,
    pub language: &'a str,
    pub commit_sha: &'a str,
    pub description: &'a str,
    pub embedding: Option<&'a [f32]>,
    pub lsp_symbol: &'a str,
    pub lsp_kind: i32,
    pub lsp_range: &'a str,
}

/// Parameters for creating a text reference.
pub struct CreateTextReferenceParams<'a> {
    pub entity_id: &'a str,
    pub path: &'a str,
    pub content_type: &'a str,
    pub commit_sha: &'a str,
    pub description: &'a str,
    pub embedding: Option<&'a [f32]>,
    pub start_line: u32,
    pub end_line: u32,
    pub anchor: Option<&'a str>,
}

/// Parameters for updating a code reference.
#[derive(Default)]
pub struct UpdateCodeReferenceParams<'a> {
    pub commit_sha: Option<&'a str>,
    pub embedding: Option<&'a [f32]>,
    pub lsp_symbol: Option<&'a str>,
    pub lsp_kind: Option<i32>,
    pub lsp_range: Option<&'a str>,
}

/// Parameters for updating a text reference.
#[derive(Default)]
pub struct UpdateTextReferenceParams<'a> {
    pub commit_sha: Option<&'a str>,
    pub embedding: Option<&'a [f32]>,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    pub anchor: Option<&'a str>,
}

/// Repository for Document and Reference operations.
#[derive(FromContext, Clone)]
pub struct DocumentRepository {
    graph: Arc<Graph>,
}

impl DocumentRepository {
    /// Get access to the graph for custom queries.
    pub fn graph(&self) -> &Graph {
        &self.graph
    }
}

impl DocumentRepository {
    // ============================================
    // Document operations
    // ============================================

    /// Create or update a document.
    pub async fn upsert_document(
        &self,
        path: &str,
        content_hash: &str,
    ) -> Result<Document, AppError> {
        let id = generate_ulid();

        let mut result = self
            .graph
            .execute(
                query(
                    "MERGE (d:Document {path: $path})
                     ON CREATE SET d.id = $id, d.created_at = datetime()
                     SET d.content_hash = $content_hash, d.updated_at = datetime()
                     RETURN d",
                )
                .param("id", id)
                .param("path", path)
                .param("content_hash", content_hash),
            )
            .await?;

        if let Some(row) = result.next().await? {
            Self::row_to_document(&row)
        } else {
            Err(AppError::Query {
                message: "Failed to upsert document".to_string(),
                query: "upsert_document".to_string(),
            })
        }
    }

    /// Find a document by path.
    pub async fn find_document_by_path(&self, path: &str) -> Result<Option<Document>, AppError> {
        let mut result = self
            .graph
            .execute(query("MATCH (d:Document {path: $path}) RETURN d").param("path", path))
            .await?;

        if let Some(row) = result.next().await? {
            Ok(Some(Self::row_to_document(&row)?))
        } else {
            Ok(None)
        }
    }

    // ============================================
    // CodeReference operations
    // ============================================

    /// Create a code reference and link it to an entity.
    pub async fn create_code_reference(
        &self,
        params: CreateCodeReferenceParams<'_>,
    ) -> Result<CodeReference, AppError> {
        let id = generate_ulid();

        let embedding_param: Option<Vec<f64>> = params
            .embedding
            .map(|e| e.iter().map(|&f| f as f64).collect());

        self.graph
            .run(
                query(
                    "MATCH (e:Entity {id: $entity_id})
                     MERGE (d:Document {path: $path})
                     ON CREATE SET d.id = $doc_id, d.content_hash = '', d.created_at = datetime()
                     CREATE (ref:CodeReference {
                         id: $id,
                         path: $path,
                         language: $language,
                         commit_sha: $commit_sha,
                         description: $description,
                         embedding: $embedding,
                         lsp_symbol: $lsp_symbol,
                         lsp_kind: $lsp_kind,
                         lsp_range: $lsp_range,
                         created_at: datetime()
                     })
                     CREATE (e)-[:HAS_REFERENCE]->(ref)
                     CREATE (ref)-[:IN_DOCUMENT]->(d)",
                )
                .param("id", id.clone())
                .param("doc_id", generate_ulid())
                .param("entity_id", params.entity_id)
                .param("path", params.path)
                .param("language", params.language)
                .param("commit_sha", params.commit_sha)
                .param("description", params.description)
                .param("embedding", embedding_param)
                .param("lsp_symbol", params.lsp_symbol)
                .param("lsp_kind", params.lsp_kind as i64)
                .param("lsp_range", params.lsp_range),
            )
            .await?;

        Ok(CodeReference {
            id,
            path: params.path.to_string(),
            language: params.language.to_string(),
            commit_sha: params.commit_sha.to_string(),
            description: params.description.to_string(),
            embedding: params.embedding.map(|e| e.to_vec()),
            lsp_symbol: params.lsp_symbol.to_string(),
            lsp_kind: params.lsp_kind,
            lsp_range: params.lsp_range.to_string(),
        })
    }

    /// Update a code reference.
    pub async fn update_code_reference(
        &self,
        id: &str,
        params: UpdateCodeReferenceParams<'_>,
    ) -> Result<(), AppError> {
        let embedding_param: Option<Vec<f64>> = params
            .embedding
            .map(|e| e.iter().map(|&f| f as f64).collect());

        self.graph
            .run(
                query(
                    "MATCH (ref:CodeReference {id: $id})
                     SET ref.commit_sha = coalesce($commit_sha, ref.commit_sha),
                         ref.embedding = coalesce($embedding, ref.embedding),
                         ref.lsp_symbol = coalesce($lsp_symbol, ref.lsp_symbol),
                         ref.lsp_kind = coalesce($lsp_kind, ref.lsp_kind),
                         ref.lsp_range = coalesce($lsp_range, ref.lsp_range),
                         ref.updated_at = datetime()",
                )
                .param("id", id)
                .param("commit_sha", params.commit_sha)
                .param("embedding", embedding_param)
                .param("lsp_symbol", params.lsp_symbol)
                .param("lsp_kind", params.lsp_kind.map(|k| k as i64))
                .param("lsp_range", params.lsp_range),
            )
            .await?;

        Ok(())
    }

    // ============================================
    // TextReference operations
    // ============================================

    /// Create a text reference and link it to an entity.
    pub async fn create_text_reference(
        &self,
        params: CreateTextReferenceParams<'_>,
    ) -> Result<TextReference, AppError> {
        let id = generate_ulid();

        let embedding_param: Option<Vec<f64>> = params
            .embedding
            .map(|e| e.iter().map(|&f| f as f64).collect());

        self.graph
            .run(
                query(
                    "MATCH (e:Entity {id: $entity_id})
                     MERGE (d:Document {path: $path})
                     ON CREATE SET d.id = $doc_id, d.content_hash = '', d.created_at = datetime()
                     CREATE (ref:TextReference {
                         id: $id,
                         path: $path,
                         content_type: $content_type,
                         commit_sha: $commit_sha,
                         description: $description,
                         embedding: $embedding,
                         start_line: $start_line,
                         end_line: $end_line,
                         anchor: $anchor,
                         created_at: datetime()
                     })
                     CREATE (e)-[:HAS_REFERENCE]->(ref)
                     CREATE (ref)-[:IN_DOCUMENT]->(d)",
                )
                .param("id", id.clone())
                .param("doc_id", generate_ulid())
                .param("entity_id", params.entity_id)
                .param("path", params.path)
                .param("content_type", params.content_type)
                .param("commit_sha", params.commit_sha)
                .param("description", params.description)
                .param("embedding", embedding_param)
                .param("start_line", params.start_line as i64)
                .param("end_line", params.end_line as i64)
                .param("anchor", params.anchor),
            )
            .await?;

        Ok(TextReference {
            id,
            path: params.path.to_string(),
            content_type: params.content_type.to_string(),
            commit_sha: params.commit_sha.to_string(),
            description: params.description.to_string(),
            embedding: params.embedding.map(|e| e.to_vec()),
            start_line: params.start_line,
            end_line: params.end_line,
            anchor: params.anchor.map(|s| s.to_string()),
        })
    }

    /// Update a text reference.
    pub async fn update_text_reference(
        &self,
        id: &str,
        params: UpdateTextReferenceParams<'_>,
    ) -> Result<(), AppError> {
        let embedding_param: Option<Vec<f64>> = params
            .embedding
            .map(|e| e.iter().map(|&f| f as f64).collect());

        self.graph
            .run(
                query(
                    "MATCH (ref:TextReference {id: $id})
                     SET ref.commit_sha = coalesce($commit_sha, ref.commit_sha),
                         ref.embedding = coalesce($embedding, ref.embedding),
                         ref.start_line = coalesce($start_line, ref.start_line),
                         ref.end_line = coalesce($end_line, ref.end_line),
                         ref.anchor = coalesce($anchor, ref.anchor),
                         ref.updated_at = datetime()",
                )
                .param("id", id)
                .param("commit_sha", params.commit_sha)
                .param("embedding", embedding_param)
                .param("start_line", params.start_line.map(|l| l as i64))
                .param("end_line", params.end_line.map(|l| l as i64))
                .param("anchor", params.anchor),
            )
            .await?;

        Ok(())
    }

    // ============================================
    // Common Reference operations
    // ============================================

    /// Find a reference by ID (either CodeReference or TextReference).
    pub async fn find_reference_by_id(&self, id: &str) -> Result<Option<Reference>, AppError> {
        // Try CodeReference first
        let mut code_result = self
            .graph
            .execute(query("MATCH (ref:CodeReference {id: $id}) RETURN ref").param("id", id))
            .await?;

        if let Some(row) = code_result.next().await? {
            return Ok(Some(Reference::Code(Self::row_to_code_reference(&row)?)));
        }

        // Try TextReference
        let mut text_result = self
            .graph
            .execute(query("MATCH (ref:TextReference {id: $id}) RETURN ref").param("id", id))
            .await?;

        if let Some(row) = text_result.next().await? {
            return Ok(Some(Reference::Text(Self::row_to_text_reference(&row)?)));
        }

        Ok(None)
    }

    /// Attach a reference to an entity (creates HAS_REFERENCE relationship).
    pub async fn attach_reference(
        &self,
        entity_id: &str,
        reference_id: &str,
    ) -> Result<(), AppError> {
        self.graph
            .run(
                query(
                    "MATCH (e:Entity {id: $entity_id})
                     OPTIONAL MATCH (code:CodeReference {id: $ref_id})
                     OPTIONAL MATCH (text:TextReference {id: $ref_id})
                     WITH e, coalesce(code, text) AS ref
                     WHERE ref IS NOT NULL
                     MERGE (e)-[:HAS_REFERENCE]->(ref)",
                )
                .param("entity_id", entity_id)
                .param("ref_id", reference_id),
            )
            .await?;
        Ok(())
    }

    /// Detach a reference from an entity (removes HAS_REFERENCE relationship).
    pub async fn detach_reference(
        &self,
        entity_id: &str,
        reference_id: &str,
    ) -> Result<(), AppError> {
        self.graph
            .run(
                query(
                    "MATCH (e:Entity {id: $entity_id})-[r:HAS_REFERENCE]->(ref)
                     WHERE ref.id = $ref_id
                     DELETE r",
                )
                .param("entity_id", entity_id)
                .param("ref_id", reference_id),
            )
            .await?;
        Ok(())
    }

    /// Delete a reference (works for both CodeReference and TextReference).
    pub async fn delete_reference(&self, id: &str) -> Result<(), AppError> {
        self.graph
            .run(
                query(
                    "OPTIONAL MATCH (ref:CodeReference {id: $id})
                     OPTIONAL MATCH (ref2:TextReference {id: $id})
                     WITH coalesce(ref, ref2) AS r
                     WHERE r IS NOT NULL
                     DETACH DELETE r",
                )
                .param("id", id),
            )
            .await?;
        Ok(())
    }

    /// Get all references for an entity (both code and text).
    pub async fn get_entity_references(&self, entity_id: &str) -> Result<Vec<Reference>, AppError> {
        let mut references = Vec::new();

        // Get CodeReferences
        let mut code_result = self
            .graph
            .execute(
                query(
                    "MATCH (e:Entity {id: $id})-[:HAS_REFERENCE]->(ref:CodeReference)
                     RETURN ref
                     ORDER BY ref.path, ref.lsp_symbol",
                )
                .param("id", entity_id),
            )
            .await?;

        while let Some(row) = code_result.next().await? {
            references.push(Reference::Code(Self::row_to_code_reference(&row)?));
        }

        // Get TextReferences
        let mut text_result = self
            .graph
            .execute(
                query(
                    "MATCH (e:Entity {id: $id})-[:HAS_REFERENCE]->(ref:TextReference)
                     RETURN ref
                     ORDER BY ref.path, ref.start_line",
                )
                .param("id", entity_id),
            )
            .await?;

        while let Some(row) = text_result.next().await? {
            references.push(Reference::Text(Self::row_to_text_reference(&row)?));
        }

        Ok(references)
    }

    /// Get all references in a document (both code and text).
    pub async fn get_document_references(
        &self,
        document_path: &str,
    ) -> Result<Vec<Reference>, AppError> {
        let mut references = Vec::new();

        // Get CodeReferences
        let mut code_result = self
            .graph
            .execute(
                query(
                    "MATCH (ref:CodeReference)-[:IN_DOCUMENT]->(d:Document {path: $path})
                     RETURN ref
                     ORDER BY ref.lsp_symbol",
                )
                .param("path", document_path),
            )
            .await?;

        while let Some(row) = code_result.next().await? {
            references.push(Reference::Code(Self::row_to_code_reference(&row)?));
        }

        // Get TextReferences
        let mut text_result = self
            .graph
            .execute(
                query(
                    "MATCH (ref:TextReference)-[:IN_DOCUMENT]->(d:Document {path: $path})
                     RETURN ref
                     ORDER BY ref.start_line",
                )
                .param("path", document_path),
            )
            .await?;

        while let Some(row) = text_result.next().await? {
            references.push(Reference::Text(Self::row_to_text_reference(&row)?));
        }

        Ok(references)
    }

    /// Get all entities attached to a reference.
    ///
    /// Returns a list of (entity_id, entity_name) tuples for all entities
    /// that have HAS_REFERENCE relationship to this reference.
    pub async fn get_attached_entities(
        &self,
        reference_id: &str,
    ) -> Result<Vec<(String, String)>, AppError> {
        let mut entities = Vec::new();

        // Query entities attached to either CodeReference or TextReference
        let mut result = self
            .graph
            .execute(
                query(
                    "OPTIONAL MATCH (e:Entity)-[:HAS_REFERENCE]->(code:CodeReference {id: $id})
                     OPTIONAL MATCH (e2:Entity)-[:HAS_REFERENCE]->(text:TextReference {id: $id})
                     WITH coalesce(e, e2) AS entity
                     WHERE entity IS NOT NULL
                     RETURN entity.id AS id, entity.name AS name",
                )
                .param("id", reference_id),
            )
            .await?;

        while let Some(row) = result.next().await? {
            let id: String = row.get("id").map_err(|e| AppError::Query {
                message: e.to_string(),
                query: "get_attached_entities".to_string(),
            })?;
            let name: String = row.get("name").map_err(|e| AppError::Query {
                message: e.to_string(),
                query: "get_attached_entities".to_string(),
            })?;
            entities.push((id, name));
        }

        Ok(entities)
    }

    /// Get stale references (references with old commit SHA).
    pub async fn get_stale_references(
        &self,
        document_path: &str,
        current_commit: &str,
    ) -> Result<Vec<Reference>, AppError> {
        let mut references = Vec::new();

        // Get stale CodeReferences
        let mut code_result = self
            .graph
            .execute(
                query(
                    "MATCH (ref:CodeReference)-[:IN_DOCUMENT]->(d:Document {path: $path})
                     WHERE ref.commit_sha <> $commit
                     RETURN ref
                     ORDER BY ref.lsp_symbol",
                )
                .param("path", document_path)
                .param("commit", current_commit),
            )
            .await?;

        while let Some(row) = code_result.next().await? {
            references.push(Reference::Code(Self::row_to_code_reference(&row)?));
        }

        // Get stale TextReferences
        let mut text_result = self
            .graph
            .execute(
                query(
                    "MATCH (ref:TextReference)-[:IN_DOCUMENT]->(d:Document {path: $path})
                     WHERE ref.commit_sha <> $commit
                     RETURN ref
                     ORDER BY ref.start_line",
                )
                .param("path", document_path)
                .param("commit", current_commit),
            )
            .await?;

        while let Some(row) = text_result.next().await? {
            references.push(Reference::Text(Self::row_to_text_reference(&row)?));
        }

        Ok(references)
    }

    // ============================================
    // Row conversion helpers (using serde)
    // ============================================

    fn row_to_document(row: &Row) -> Result<Document, AppError> {
        let node: neo4rs::Node = row.get("d").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "parse document node".to_string(),
        })?;

        node.to::<Document>().map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "deserialize document".to_string(),
        })
    }

    fn row_to_code_reference(row: &Row) -> Result<CodeReference, AppError> {
        let node: neo4rs::Node = row.get("ref").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "parse code reference node".to_string(),
        })?;

        node.to::<CodeReference>().map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "deserialize code reference".to_string(),
        })
    }

    fn row_to_text_reference(row: &Row) -> Result<TextReference, AppError> {
        let node: neo4rs::Node = row.get("ref").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "parse text reference node".to_string(),
        })?;

        node.to::<TextReference>().map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "deserialize text reference".to_string(),
        })
    }
}
