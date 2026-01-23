//! Sync tools for keeping the graph in sync with git changes.

use rmcp::{
    handler::server::wrapper::Parameters,
    model::CallToolResult,
    schemars::{self, JsonSchema},
    tool, tool_router, ErrorData as McpError,
};
use serde::{Deserialize, Serialize};

use crate::git::{ChangeType, ChangedFile, GitOps};
use crate::mcp::protocol::Response;
use crate::mcp::server::McpServer;

// ============================================================================
// Parameter Types
// ============================================================================

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

// ============================================================================
// Response Types
// ============================================================================

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

// ============================================================================
// Tool Router
// ============================================================================

#[tool_router(router = sync_tools, vis = "pub(crate)")]
impl McpServer {
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

        Response(response, None).into()
    }
}
