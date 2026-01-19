//! Sync tools for keeping the graph in sync with git changes.

use rmcp::{
    handler::server::wrapper::Parameters,
    model::CallToolResult,
    schemars::{self, JsonSchema},
    tool, tool_router, ErrorData as McpError,
};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::git::{ChangeType, ChangedFile, DiffHunk, FileDiff, GitOps};
use crate::mcp::protocol::Response;
use crate::mcp::server::McpServer;
use crate::models::DocumentReference;
use crate::repositories::{DocumentRepository, UpdateReferenceParams};

// ============================================================================
// Parameter Types
// ============================================================================

/// Parameters for sync_references tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SyncReferencesParams {
    /// List of reference updates to apply.
    pub updates: Vec<ReferenceUpdate>,
}

/// A single reference update.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReferenceUpdate {
    /// Reference ID to update.
    pub id: String,
    /// New starting line number.
    pub start_line: u32,
    /// New ending line number.
    pub end_line: u32,
}

/// Parameters for validate_documents tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ValidateDocumentsParams {
    /// Document path to validate. If not provided, validates all documents with stale refs.
    #[serde(default)]
    pub document_path: Option<String>,
}

/// Parameters for get_changed_files tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetChangedFilesParams {
    /// Starting commit SHA. If not provided, uses the initial commit.
    #[serde(default)]
    pub from_sha: Option<String>,
    /// Ending commit SHA. If not provided, uses HEAD.
    #[serde(default)]
    pub to_sha: Option<String>,
}

/// Parameters for get_document_references tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetDocumentReferencesParams {
    /// Document path to get references for.
    pub document_path: String,
}

// ============================================================================
// Response Types
// ============================================================================

/// Result of sync_references operation.
#[derive(Debug, Serialize)]
pub struct SyncReferencesResult {
    /// Number of references updated.
    pub updated_count: usize,
    /// Current HEAD commit SHA.
    pub commit_sha: String,
    /// IDs of updated references.
    pub updated_ids: Vec<String>,
}

/// Result of validate_documents operation.
#[derive(Debug, Serialize)]
pub struct ValidateDocumentsResult {
    /// Current HEAD commit SHA.
    pub current_commit: String,
    /// Stale references that need review.
    pub stale_references: Vec<StaleReference>,
    /// Total count of stale references.
    pub total_stale: usize,
}

/// A stale reference with diff context.
#[derive(Debug, Serialize)]
pub struct StaleReference {
    /// Reference ID.
    pub id: String,
    /// Document path.
    pub document_path: String,
    /// Current start line in the reference.
    pub start_line: u32,
    /// Current end line in the reference.
    pub end_line: u32,
    /// Commit SHA when reference was last updated.
    pub reference_commit: String,
    /// Description of the reference.
    pub description: String,
    /// Whether the reference is in a changed region of the file.
    pub in_changed_region: bool,
    /// Diff hunks affecting this file (if in changed region).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub affected_hunks: Vec<HunkInfo>,
}

/// Simplified hunk info for response.
#[derive(Debug, Serialize)]
pub struct HunkInfo {
    /// Old file start line.
    pub old_start: u32,
    /// Old file line count.
    pub old_lines: u32,
    /// New file start line.
    pub new_start: u32,
    /// New file line count.
    pub new_lines: u32,
}

/// Result of get_changed_files operation.
#[derive(Debug, Serialize)]
pub struct GetChangedFilesResult {
    /// Starting commit SHA.
    pub from_sha: Option<String>,
    /// Ending commit SHA.
    pub to_sha: String,
    /// List of changed files.
    pub changed_files: Vec<ChangedFileInfo>,
    /// Total count of changed files.
    pub total_count: usize,
}

/// Information about a changed file.
#[derive(Debug, Serialize)]
pub struct ChangedFileInfo {
    /// Path to the file.
    pub path: String,
    /// Type of change.
    pub change_type: String,
}

impl From<&ChangedFile> for ChangedFileInfo {
    fn from(f: &ChangedFile) -> Self {
        let change_type = match f.change_type {
            ChangeType::Added => "added",
            ChangeType::Modified => "modified",
            ChangeType::Deleted => "deleted",
            ChangeType::Renamed => "renamed",
        };
        Self {
            path: f.path.clone(),
            change_type: change_type.to_string(),
        }
    }
}

/// Result of get_document_references operation.
#[derive(Debug, Serialize)]
pub struct GetDocumentReferencesResult {
    /// Document path.
    pub document_path: String,
    /// References in the document.
    pub references: Vec<DocumentReferenceInfo>,
    /// Total count.
    pub total_count: usize,
    /// Current HEAD commit.
    pub current_commit: String,
}

/// Information about a document reference.
#[derive(Debug, Serialize)]
pub struct DocumentReferenceInfo {
    /// Reference ID.
    pub id: String,
    /// Start line.
    pub start_line: u32,
    /// End line.
    pub end_line: u32,
    /// Description.
    pub description: String,
    /// Commit SHA when reference was recorded.
    pub commit_sha: String,
    /// Whether the reference is stale (commit differs from HEAD).
    pub is_stale: bool,
}

impl From<&DiffHunk> for HunkInfo {
    fn from(h: &DiffHunk) -> Self {
        Self {
            old_start: h.old_start,
            old_lines: h.old_lines,
            new_start: h.new_start,
            new_lines: h.new_lines,
        }
    }
}

// ============================================================================
// Tool Router
// ============================================================================

#[tool_router(router = sync_tools, vis = "pub(crate)")]
impl McpServer {
    /// Update line numbers for document references after code changes.
    ///
    /// Use this after reviewing stale references to update their line numbers
    /// to match the current code. Updates are applied in batch and the commit
    /// SHA is updated to current HEAD.
    #[tool(
        description = "Update line numbers for document references after code changes. Updates commit SHA to HEAD."
    )]
    pub async fn sync_references(
        &self,
        Parameters(params): Parameters<SyncReferencesParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(count = params.updates.len(), "Running sync_references tool");

        let doc_repo = self.resolve::<DocumentRepository>();

        // Get current HEAD
        let git = GitOps::open_current().map_err(McpError::from)?;
        let head_sha = git.get_head_sha().map_err(McpError::from)?;

        let mut updated_ids = Vec::new();

        for update in &params.updates {
            let update_params = UpdateReferenceParams {
                start_line: Some(update.start_line),
                end_line: Some(update.end_line),
                commit_sha: Some(&head_sha),
                ..Default::default()
            };

            doc_repo
                .update_reference(&update.id, update_params)
                .await
                .map_err(|e: AppError| McpError::from(e))?;

            updated_ids.push(update.id.clone());
        }

        let response = SyncReferencesResult {
            updated_count: updated_ids.len(),
            commit_sha: head_sha,
            updated_ids,
        };

        tracing::info!(
            updated = response.updated_count,
            commit = %response.commit_sha,
            "References synced"
        );

        Response(response).into()
    }

    /// Find stale document references that may need line number updates.
    ///
    /// Compares stored commit SHAs with current HEAD to find references
    /// that haven't been updated since the code changed. Returns diff
    /// context to help identify which references need updating.
    #[tool(
        description = "Find stale document references with diff context. Shows which refs may need line number updates."
    )]
    pub async fn validate_documents(
        &self,
        Parameters(params): Parameters<ValidateDocumentsParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(path = ?params.document_path, "Running validate_documents tool");

        let doc_repo = self.resolve::<DocumentRepository>();

        // Get current HEAD
        let git = GitOps::open_current().map_err(McpError::from)?;
        let head_sha = git.get_head_sha().map_err(McpError::from)?;

        let mut stale_references = Vec::new();

        if let Some(path) = &params.document_path {
            // Get references with different commit SHA
            let refs = doc_repo
                .get_stale_references(path, &head_sha)
                .await
                .map_err(|e: AppError| McpError::from(e))?;

            // Only process if the file actually has changes
            for doc_ref in refs {
                // Check if the file changed between reference commit and HEAD
                let file_diff = git
                    .get_file_diff(&doc_ref.document_path, &doc_ref.commit_sha, Some(&head_sha))
                    .map_err(McpError::from)?;

                // Only include if file actually changed
                if file_diff.is_some() {
                    let stale = build_stale_reference(&git, &doc_ref, &head_sha, file_diff)?;
                    stale_references.push(stale);
                }
            }
        } else {
            // Get all documents and check each for stale refs
            // For now, we'll return an error asking for a specific path
            // A full implementation would iterate all documents
            return Err(McpError::invalid_params(
                "document_path is required. Full scan not yet implemented.",
                None,
            ));
        }

        let total_stale = stale_references.len();

        let response = ValidateDocumentsResult {
            current_commit: head_sha,
            stale_references,
            total_stale,
        };

        tracing::info!(
            stale = response.total_stale,
            commit = %response.current_commit,
            "Document validation complete"
        );

        Response(response).into()
    }

    /// Get list of files changed between two commits.
    ///
    /// Returns all files that were added, modified, deleted, or renamed
    /// between the specified commits. Useful for identifying which documents
    /// may need their references updated.
    #[tool(
        description = "Get list of files changed between commits. Use to find documents needing sync."
    )]
    pub async fn get_changed_files(
        &self,
        Parameters(params): Parameters<GetChangedFilesParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(
            from = ?params.from_sha,
            to = ?params.to_sha,
            "Running get_changed_files tool"
        );

        let git = GitOps::open_current().map_err(McpError::from)?;
        let head_sha = git.get_head_sha().map_err(McpError::from)?;

        let to_sha = params.to_sha.as_deref();
        let from_sha = params.from_sha.as_deref();

        let changed = git
            .get_changed_files(from_sha, to_sha)
            .map_err(McpError::from)?;

        let changed_files: Vec<ChangedFileInfo> =
            changed.iter().map(ChangedFileInfo::from).collect();
        let total_count = changed_files.len();

        let response = GetChangedFilesResult {
            from_sha: params.from_sha,
            to_sha: to_sha.unwrap_or(&head_sha).to_string(),
            changed_files,
            total_count,
        };

        tracing::info!(
            count = response.total_count,
            from = ?response.from_sha,
            to = %response.to_sha,
            "Changed files retrieved"
        );

        Response(response).into()
    }

    /// Get all document references for a specific file.
    ///
    /// Returns all references in a document along with their current state,
    /// including whether they are stale (file changed since reference was recorded).
    #[tool(
        description = "Get all document references for a file. Shows current state and staleness."
    )]
    pub async fn get_document_references(
        &self,
        Parameters(params): Parameters<GetDocumentReferencesParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(path = %params.document_path, "Running get_document_references tool");

        let doc_repo = self.resolve::<DocumentRepository>();

        let git = GitOps::open_current().map_err(McpError::from)?;
        let head_sha = git.get_head_sha().map_err(McpError::from)?;

        let refs = doc_repo
            .get_document_references(&params.document_path)
            .await
            .map_err(|e: AppError| McpError::from(e))?;

        let references: Vec<DocumentReferenceInfo> = refs
            .iter()
            .map(|r| {
                // Only stale if the file actually changed between commits
                let is_stale = if r.commit_sha == head_sha {
                    false
                } else {
                    // Check if file has changes between reference commit and HEAD
                    git.get_file_diff(&r.document_path, &r.commit_sha, Some(&head_sha))
                        .map(|diff| diff.is_some()) // Some means file changed
                        .unwrap_or(false)
                };

                DocumentReferenceInfo {
                    id: r.id.clone(),
                    start_line: r.start_line,
                    end_line: r.end_line,
                    description: r.description.clone(),
                    commit_sha: r.commit_sha.clone(),
                    is_stale,
                }
            })
            .collect();

        let total_count = references.len();

        let response = GetDocumentReferencesResult {
            document_path: params.document_path,
            references,
            total_count,
            current_commit: head_sha,
        };

        tracing::info!(
            count = response.total_count,
            path = %response.document_path,
            "Document references retrieved"
        );

        Response(response).into()
    }
}

/// Build a StaleReference with diff context.
fn build_stale_reference(
    _git: &GitOps,
    doc_ref: &DocumentReference,
    _head_sha: &str,
    file_diff: Option<FileDiff>,
) -> Result<StaleReference, McpError> {
    let (in_changed_region, affected_hunks) = match file_diff {
        Some(diff) => {
            let in_region =
                GitOps::is_in_changed_region(&diff.hunks, doc_ref.start_line, doc_ref.end_line);

            // Only include hunks that affect this reference
            let affected: Vec<HunkInfo> = if in_region {
                diff.hunks
                    .iter()
                    .filter(|h| {
                        let hunk_end = h.old_start + h.old_lines.saturating_sub(1);
                        doc_ref.start_line <= hunk_end && doc_ref.end_line >= h.old_start
                    })
                    .map(HunkInfo::from)
                    .collect()
            } else {
                Vec::new()
            };

            (in_region, affected)
        }
        None => (false, Vec::new()),
    };

    Ok(StaleReference {
        id: doc_ref.id.clone(),
        document_path: doc_ref.document_path.clone(),
        start_line: doc_ref.start_line,
        end_line: doc_ref.end_line,
        reference_commit: doc_ref.commit_sha.clone(),
        description: doc_ref.description.clone(),
        in_changed_region,
        affected_hunks,
    })
}
