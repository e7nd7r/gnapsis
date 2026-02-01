//! Document and reference models for tracking code locations.
//!
//! References come in two types:
//! - `CodeReference` - For source code, uses LSP range with full position info
//! - `TextReference` - For markdown/text, uses line ranges with optional anchors

use serde::{Deserialize, Deserializer, Serialize};

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
    #[serde(default)]
    pub content_hash: String,
}

/// A reference to a location in a source code file.
///
/// Uses LSP metadata for precise symbol tracking and IDE integration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeReference {
    /// Unique identifier (ULID).
    pub id: String,
    /// Source ID indicating which source this reference belongs to.
    #[serde(default)]
    pub source_id: String,
    /// Path to the document (relative to source root).
    pub path: String,
    /// Programming language (e.g., "rust", "typescript").
    pub language: String,
    /// Git commit SHA when this reference was recorded.
    pub commit_sha: String,
    /// Description of what this reference points to.
    #[serde(default)]
    pub description: String,
    /// Vector embedding for semantic search (not serialized in responses).
    #[serde(skip_serializing, default, deserialize_with = "deserialize_embedding")]
    pub embedding: Option<Vec<f32>>,
    /// LSP symbol name (e.g., "impl Foo::bar").
    pub lsp_symbol: String,
    /// LSP symbol kind as integer (from LSP SymbolKind enum).
    #[serde(deserialize_with = "deserialize_i32")]
    pub lsp_kind: i32,
    /// LSP range as JSON string (contains start/end line and character positions).
    pub lsp_range: String,
}

/// A reference to a location in a text/markdown file.
///
/// Uses line ranges and optional anchors for documentation tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextReference {
    /// Unique identifier (ULID).
    pub id: String,
    /// Source ID indicating which source this reference belongs to.
    #[serde(default)]
    pub source_id: String,
    /// Path to the document (relative to source root).
    pub path: String,
    /// Content type (e.g., "markdown", "text").
    #[serde(default = "default_content_type")]
    pub content_type: String,
    /// Git commit SHA when this reference was recorded.
    pub commit_sha: String,
    /// Description of what this reference points to.
    #[serde(default)]
    pub description: String,
    /// Vector embedding for semantic search (not serialized in responses).
    #[serde(skip_serializing, default, deserialize_with = "deserialize_embedding")]
    pub embedding: Option<Vec<f32>>,
    /// Starting line number (1-indexed).
    #[serde(deserialize_with = "deserialize_u32")]
    pub start_line: u32,
    /// Ending line number (1-indexed).
    #[serde(deserialize_with = "deserialize_u32")]
    pub end_line: u32,
    /// Optional semantic anchor (e.g., "## Architecture", "### Overview").
    #[serde(default)]
    pub anchor: Option<String>,
}

fn default_content_type() -> String {
    "markdown".to_string()
}

/// Deserialize embedding from f64 to f32.
fn deserialize_embedding<'de, D>(deserializer: D) -> Result<Option<Vec<f32>>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<Vec<f64>> = Option::deserialize(deserializer)?;
    Ok(opt.map(|v| v.into_iter().map(|f| f as f32).collect()))
}

/// Deserialize i64 to i32.
fn deserialize_i32<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: Deserializer<'de>,
{
    let value: i64 = i64::deserialize(deserializer)?;
    Ok(value as i32)
}

/// Deserialize i64 to u32.
fn deserialize_u32<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let value: i64 = i64::deserialize(deserializer)?;
    Ok(value as u32)
}

/// Enum wrapper for both reference types.
///
/// Used when querying references without knowing the type upfront.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Reference {
    Code(CodeReference),
    Text(TextReference),
}

impl Reference {
    /// Get the reference ID.
    pub fn id(&self) -> &str {
        match self {
            Reference::Code(r) => &r.id,
            Reference::Text(r) => &r.id,
        }
    }

    /// Get the source ID.
    pub fn source_id(&self) -> &str {
        match self {
            Reference::Code(r) => &r.source_id,
            Reference::Text(r) => &r.source_id,
        }
    }

    /// Get the document path (relative to source).
    pub fn path(&self) -> &str {
        match self {
            Reference::Code(r) => &r.path,
            Reference::Text(r) => &r.path,
        }
    }

    /// Get the commit SHA.
    pub fn commit_sha(&self) -> &str {
        match self {
            Reference::Code(r) => &r.commit_sha,
            Reference::Text(r) => &r.commit_sha,
        }
    }

    /// Get the description.
    pub fn description(&self) -> &str {
        match self {
            Reference::Code(r) => &r.description,
            Reference::Text(r) => &r.description,
        }
    }

    /// Get the embedding if present.
    pub fn embedding(&self) -> Option<&Vec<f32>> {
        match self {
            Reference::Code(r) => r.embedding.as_ref(),
            Reference::Text(r) => r.embedding.as_ref(),
        }
    }

    /// Get start line (for TextReference) or extract from lsp_range (for CodeReference).
    pub fn start_line(&self) -> Option<u32> {
        match self {
            Reference::Code(_) => None, // Use lsp_range for code
            Reference::Text(r) => Some(r.start_line),
        }
    }

    /// Get end line (for TextReference) or extract from lsp_range (for CodeReference).
    pub fn end_line(&self) -> Option<u32> {
        match self {
            Reference::Code(_) => None, // Use lsp_range for code
            Reference::Text(r) => Some(r.end_line),
        }
    }

    /// Check if this is a code reference.
    pub fn is_code(&self) -> bool {
        matches!(self, Reference::Code(_))
    }

    /// Check if this is a text reference.
    pub fn is_text(&self) -> bool {
        matches!(self, Reference::Text(_))
    }

    /// Get as CodeReference if it is one.
    pub fn as_code(&self) -> Option<&CodeReference> {
        match self {
            Reference::Code(r) => Some(r),
            Reference::Text(_) => None,
        }
    }

    /// Get as TextReference if it is one.
    pub fn as_text(&self) -> Option<&TextReference> {
        match self {
            Reference::Code(_) => None,
            Reference::Text(r) => Some(r),
        }
    }
}
