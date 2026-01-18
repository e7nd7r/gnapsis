//! Document and DocumentReference repository for tracking code locations.

use std::sync::Arc;

use neo4rs::{query, Graph, Row};

use crate::context::Context;
use crate::di::FromContext;
use crate::error::AppError;
use crate::models::{generate_ulid, ContentType, Document, DocumentReference};

/// Parameters for creating a document reference.
pub struct CreateReferenceParams<'a> {
    pub entity_id: &'a str,
    pub document_path: &'a str,
    pub start_line: u32,
    pub end_line: u32,
    pub offset: Option<u32>,
    pub commit_sha: &'a str,
    pub content_type: &'a ContentType,
    pub description: &'a str,
    pub embedding: Option<&'a [f32]>,
    pub lsp_symbol: Option<&'a str>,
    pub lsp_kind: Option<i32>,
    pub lsp_range: Option<&'a str>,
}

/// Parameters for updating a document reference.
#[derive(Default)]
pub struct UpdateReferenceParams<'a> {
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    pub offset: Option<u32>,
    pub commit_sha: Option<&'a str>,
    pub embedding: Option<&'a [f32]>,
    pub lsp_symbol: Option<&'a str>,
    pub lsp_kind: Option<i32>,
    pub lsp_range: Option<&'a str>,
}

/// Repository for Document and DocumentReference operations.
#[derive(FromContext, Clone)]
pub struct DocumentRepository {
    graph: Arc<Graph>,
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
    // DocumentReference operations
    // ============================================

    /// Create a document reference and link it to an entity.
    pub async fn create_reference(
        &self,
        params: CreateReferenceParams<'_>,
    ) -> Result<DocumentReference, AppError> {
        let id = generate_ulid();

        let content_type_str = match params.content_type {
            ContentType::Code(lang) => format!("code:{}", lang),
            ContentType::Markdown => "markdown".to_string(),
        };

        let embedding_param: Option<Vec<f64>> = params
            .embedding
            .map(|e| e.iter().map(|&f| f as f64).collect());

        self.graph
            .run(
                query(
                    "MATCH (e:Entity {id: $entity_id})
                     MERGE (d:Document {path: $document_path})
                     ON CREATE SET d.id = $doc_id, d.content_hash = '', d.created_at = datetime()
                     CREATE (ref:DocumentReference {
                         id: $id,
                         document_path: $document_path,
                         start_line: $start_line,
                         end_line: $end_line,
                         offset: $offset,
                         commit_sha: $commit_sha,
                         content_type: $content_type,
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
                .param("document_path", params.document_path)
                .param("start_line", params.start_line as i64)
                .param("end_line", params.end_line as i64)
                .param("offset", params.offset.map(|o| o as i64))
                .param("commit_sha", params.commit_sha)
                .param("content_type", content_type_str.clone())
                .param("description", params.description)
                .param("embedding", embedding_param.clone())
                .param("lsp_symbol", params.lsp_symbol)
                .param("lsp_kind", params.lsp_kind.map(|k| k as i64))
                .param("lsp_range", params.lsp_range),
            )
            .await?;

        Ok(DocumentReference {
            id,
            document_path: params.document_path.to_string(),
            start_line: params.start_line,
            end_line: params.end_line,
            offset: params.offset,
            commit_sha: params.commit_sha.to_string(),
            content_type: params.content_type.clone(),
            description: params.description.to_string(),
            embedding: params.embedding.map(|e| e.to_vec()),
            lsp_symbol: params.lsp_symbol.map(|s| s.to_string()),
            lsp_kind: params.lsp_kind,
            lsp_range: params.lsp_range.map(|s| s.to_string()),
        })
    }

    /// Update a document reference (e.g., after syncing line numbers).
    pub async fn update_reference(
        &self,
        id: &str,
        params: UpdateReferenceParams<'_>,
    ) -> Result<(), AppError> {
        let embedding_param: Option<Vec<f64>> = params
            .embedding
            .map(|e| e.iter().map(|&f| f as f64).collect());

        self.graph
            .run(
                query(
                    "MATCH (ref:DocumentReference {id: $id})
                     SET ref.start_line = coalesce($start_line, ref.start_line),
                         ref.end_line = coalesce($end_line, ref.end_line),
                         ref.offset = coalesce($offset, ref.offset),
                         ref.commit_sha = coalesce($commit_sha, ref.commit_sha),
                         ref.embedding = coalesce($embedding, ref.embedding),
                         ref.lsp_symbol = coalesce($lsp_symbol, ref.lsp_symbol),
                         ref.lsp_kind = coalesce($lsp_kind, ref.lsp_kind),
                         ref.lsp_range = coalesce($lsp_range, ref.lsp_range),
                         ref.updated_at = datetime()",
                )
                .param("id", id)
                .param("start_line", params.start_line.map(|l| l as i64))
                .param("end_line", params.end_line.map(|l| l as i64))
                .param("offset", params.offset.map(|o| o as i64))
                .param("commit_sha", params.commit_sha)
                .param("embedding", embedding_param)
                .param("lsp_symbol", params.lsp_symbol)
                .param("lsp_kind", params.lsp_kind.map(|k| k as i64))
                .param("lsp_range", params.lsp_range),
            )
            .await?;

        Ok(())
    }

    /// Delete a document reference.
    pub async fn delete_reference(&self, id: &str) -> Result<(), AppError> {
        self.graph
            .run(
                query(
                    "MATCH (ref:DocumentReference {id: $id})
                     DETACH DELETE ref",
                )
                .param("id", id),
            )
            .await?;
        Ok(())
    }

    /// Get all references for an entity.
    pub async fn get_entity_references(
        &self,
        entity_id: &str,
    ) -> Result<Vec<DocumentReference>, AppError> {
        let mut result = self
            .graph
            .execute(
                query(
                    "MATCH (e:Entity {id: $id})-[:HAS_REFERENCE]->(ref:DocumentReference)
                     RETURN ref
                     ORDER BY ref.document_path, ref.start_line",
                )
                .param("id", entity_id),
            )
            .await?;

        let mut references = Vec::new();
        while let Some(row) = result.next().await? {
            references.push(Self::row_to_reference(&row)?);
        }
        Ok(references)
    }

    /// Get all references in a document.
    pub async fn get_document_references(
        &self,
        document_path: &str,
    ) -> Result<Vec<DocumentReference>, AppError> {
        let mut result = self
            .graph
            .execute(
                query(
                    "MATCH (ref:DocumentReference)-[:IN_DOCUMENT]->(d:Document {path: $path})
                     RETURN ref
                     ORDER BY ref.start_line",
                )
                .param("path", document_path),
            )
            .await?;

        let mut references = Vec::new();
        while let Some(row) = result.next().await? {
            references.push(Self::row_to_reference(&row)?);
        }
        Ok(references)
    }

    /// Get stale references (references with old commit SHA).
    pub async fn get_stale_references(
        &self,
        document_path: &str,
        current_commit: &str,
    ) -> Result<Vec<DocumentReference>, AppError> {
        let mut result = self
            .graph
            .execute(
                query(
                    "MATCH (ref:DocumentReference)-[:IN_DOCUMENT]->(d:Document {path: $path})
                     WHERE ref.commit_sha <> $commit
                     RETURN ref
                     ORDER BY ref.start_line",
                )
                .param("path", document_path)
                .param("commit", current_commit),
            )
            .await?;

        let mut references = Vec::new();
        while let Some(row) = result.next().await? {
            references.push(Self::row_to_reference(&row)?);
        }
        Ok(references)
    }

    /// Convert a Neo4j row to a Document.
    fn row_to_document(row: &Row) -> Result<Document, AppError> {
        let node: neo4rs::Node = row.get("d").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "parse document node".to_string(),
        })?;

        let id: String = node.get("id").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "get document id".to_string(),
        })?;

        let path: String = node.get("path").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "get document path".to_string(),
        })?;

        let content_hash: String = node.get("content_hash").unwrap_or_default();

        Ok(Document {
            id,
            path,
            content_hash,
        })
    }

    /// Convert a Neo4j row to a DocumentReference.
    fn row_to_reference(row: &Row) -> Result<DocumentReference, AppError> {
        let node: neo4rs::Node = row.get("ref").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "parse reference node".to_string(),
        })?;

        let id: String = node.get("id").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "get reference id".to_string(),
        })?;

        let document_path: String = node.get("document_path").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "get document_path".to_string(),
        })?;

        let start_line: i64 = node.get("start_line").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "get start_line".to_string(),
        })?;

        let end_line: i64 = node.get("end_line").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "get end_line".to_string(),
        })?;

        let offset: Option<i64> = node.get("offset").ok();

        let commit_sha: String = node.get("commit_sha").map_err(|e| AppError::Query {
            message: e.to_string(),
            query: "get commit_sha".to_string(),
        })?;

        let content_type_str: String = node
            .get("content_type")
            .unwrap_or_else(|_| "markdown".to_string());
        let content_type = if content_type_str.starts_with("code:") {
            ContentType::Code(content_type_str.trim_start_matches("code:").to_string())
        } else {
            ContentType::Markdown
        };

        let description: String = node.get("description").unwrap_or_default();

        let embedding: Option<Vec<f64>> = node.get("embedding").ok();
        let embedding = embedding.map(|e| e.iter().map(|&f| f as f32).collect());

        let lsp_symbol: Option<String> = node.get("lsp_symbol").ok();
        let lsp_kind: Option<i64> = node.get("lsp_kind").ok();
        let lsp_range: Option<String> = node.get("lsp_range").ok();

        Ok(DocumentReference {
            id,
            document_path,
            start_line: start_line as u32,
            end_line: end_line as u32,
            offset: offset.map(|o| o as u32),
            commit_sha,
            content_type,
            description,
            embedding,
            lsp_symbol,
            lsp_kind: lsp_kind.map(|k| k as i32),
            lsp_range,
        })
    }
}
