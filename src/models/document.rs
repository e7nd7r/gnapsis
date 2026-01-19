//! Document and DocumentReference models for tracking code locations.

use serde::{Deserialize, Serialize};

/// A tracked document (file) in the repository.
///
/// Used to track file state for sync operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// Unique identifier (ULID).
    pub id: String,
    /// Path relative to repository root.
    pub path: String,
    /// Hash of file content for change detection.
    pub content_hash: String,
}

/// A reference to a specific location within a document.
///
/// DocumentReferences link entities to their source code locations
/// and include LSP metadata for IDE integration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentReference {
    /// Unique identifier (ULID).
    pub id: String,
    /// Path to the document.
    pub document_path: String,
    /// Starting line number (1-indexed).
    pub start_line: u32,
    /// Ending line number (1-indexed).
    pub end_line: u32,
    /// Character offset within the file (optional).
    pub offset: Option<u32>,
    /// Git commit SHA when this reference was recorded.
    pub commit_sha: String,
    /// Type of content at this location.
    pub content_type: ContentType,
    /// Description of what this reference points to.
    pub description: String,
    /// Vector embedding for semantic search (internal, not serialized).
    #[serde(skip_serializing)]
    pub embedding: Option<Vec<f32>>,
    /// LSP symbol name (optional).
    pub lsp_symbol: Option<String>,
    /// LSP symbol kind as integer (optional).
    pub lsp_kind: Option<i32>,
    /// LSP range as JSON string (optional).
    pub lsp_range: Option<String>,
}

/// The type of content at a document reference location.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContentType {
    /// Source code with language identifier (e.g., "rust", "typescript").
    Code(String),
    /// Markdown documentation.
    Markdown,
}

impl ContentType {
    /// Returns the language identifier for Code, or "markdown" for Markdown.
    pub fn language(&self) -> &str {
        match self {
            ContentType::Code(lang) => lang,
            ContentType::Markdown => "markdown",
        }
    }
}
