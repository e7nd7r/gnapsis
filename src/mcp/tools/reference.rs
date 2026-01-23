//! Reference tools for bulk update/delete operations.
//!
//! Implements the `alter_references` tool from DES-005 for managing
//! document references independently of entities.

use rmcp::{
    handler::server::wrapper::Parameters,
    model::CallToolResult,
    schemars::{self, JsonSchema},
    tool, tool_router, ErrorData as McpError,
};
use serde::{Deserialize, Serialize};

use crate::git::GitOps;
use crate::mcp::protocol::Response;
use crate::mcp::server::McpServer;
use crate::repositories::DocumentRepository;
use crate::services::{AttachedEntityInfo, FailureContext};

// ============================================================================
// Parameter Types
// ============================================================================

/// Parameters for alter_references tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AlterReferencesParams {
    /// Commands to execute on references.
    pub commands: Vec<ReferenceCommand>,
}

/// Commands for reference operations.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReferenceCommand {
    /// Update a reference's line numbers. Auto-sets commit_sha to HEAD.
    Update {
        /// Reference ID to update.
        id: String,
        /// New starting line number.
        #[serde(default)]
        start_line: Option<u32>,
        /// New ending line number.
        #[serde(default)]
        end_line: Option<u32>,
        /// New anchor (for text references).
        #[serde(default)]
        anchor: Option<String>,
        /// New LSP symbol (for code references).
        #[serde(default)]
        lsp_symbol: Option<String>,
    },
    /// Delete a reference. Fails if attached to any entity.
    Delete {
        /// Reference ID to delete.
        id: String,
    },
}

// ============================================================================
// Response Types
// ============================================================================

/// Result of alter_references operation.
#[derive(Debug, Serialize)]
pub struct AlterReferencesResult {
    /// Commands that executed successfully.
    pub executed: Vec<ExecutedRefCommand>,
    /// Command that failed (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed: Option<FailedRefCommand>,
    /// Commands skipped due to earlier failure.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<ReferenceCommand>,
    /// Current HEAD commit SHA (after updates).
    pub commit_sha: String,
}

/// A successfully executed reference command.
#[derive(Debug, Serialize)]
pub struct ExecutedRefCommand {
    /// Index of the command in the original sequence.
    pub index: usize,
    /// The command that was executed.
    pub command: ReferenceCommand,
    /// Outcome of the execution.
    pub outcome: RefCommandOutcome,
}

/// Outcome of a successfully executed reference command.
#[derive(Debug, Serialize)]
#[serde(tag = "outcome_type", rename_all = "snake_case")]
pub enum RefCommandOutcome {
    /// Reference was updated.
    Updated { reference_id: String },
    /// Reference was deleted.
    Deleted { reference_id: String },
}

/// A reference command that failed during execution.
#[derive(Debug, Serialize)]
pub struct FailedRefCommand {
    /// Index of the command in the original sequence.
    pub index: usize,
    /// The command that failed.
    pub command: ReferenceCommand,
    /// Error message describing the failure.
    pub error: String,
    /// Additional context about the failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<FailureContext>,
}

// ============================================================================
// Tool Router
// ============================================================================

#[tool_router(router = reference_tools, vis = "pub(crate)")]
impl McpServer {
    /// Bulk update/delete references with automatic commit SHA updates.
    ///
    /// Commands execute sequentially. Update commands auto-set commit_sha to HEAD.
    /// Delete commands fail if the reference is attached to any entity.
    #[tool(
        description = "Bulk update/delete references. Update auto-sets commit_sha to HEAD. Delete fails if attached."
    )]
    pub async fn alter_references(
        &self,
        Parameters(params): Parameters<AlterReferencesParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(
            count = params.commands.len(),
            "Running alter_references tool"
        );

        let doc_repo = self.resolve::<DocumentRepository>();

        // Get current HEAD
        let git = GitOps::open_current().map_err(McpError::from)?;
        let head_sha = git.get_head_sha().map_err(McpError::from)?;

        let mut executed = Vec::new();

        for (index, command) in params.commands.iter().enumerate() {
            match execute_ref_command(&doc_repo, command, &head_sha).await {
                Ok(outcome) => {
                    executed.push(ExecutedRefCommand {
                        index,
                        command: command.clone(),
                        outcome,
                    });
                }
                Err((error, context)) => {
                    let failed = FailedRefCommand {
                        index,
                        command: command.clone(),
                        error,
                        context,
                    };
                    let skipped: Vec<ReferenceCommand> =
                        params.commands.into_iter().skip(index + 1).collect();

                    let response = AlterReferencesResult {
                        executed,
                        failed: Some(failed),
                        skipped,
                        commit_sha: head_sha,
                    };

                    tracing::warn!(
                        executed = response.executed.len(),
                        "alter_references failed at command {}",
                        index
                    );

                    return Response(response).into();
                }
            }
        }

        let response = AlterReferencesResult {
            executed,
            failed: None,
            skipped: Vec::new(),
            commit_sha: head_sha,
        };

        tracing::info!(
            updated = response.executed.len(),
            commit = %response.commit_sha,
            "References altered successfully"
        );

        Response(response).into()
    }
}

/// Execute a single reference command.
async fn execute_ref_command(
    doc_repo: &DocumentRepository,
    command: &ReferenceCommand,
    head_sha: &str,
) -> Result<RefCommandOutcome, (String, Option<FailureContext>)> {
    match command {
        ReferenceCommand::Update {
            id,
            start_line,
            end_line,
            anchor,
            lsp_symbol,
        } => {
            execute_update(
                doc_repo,
                id,
                *start_line,
                *end_line,
                anchor,
                lsp_symbol,
                head_sha,
            )
            .await
        }
        ReferenceCommand::Delete { id } => execute_delete(doc_repo, id).await,
    }
}

/// Execute an Update command.
async fn execute_update(
    doc_repo: &DocumentRepository,
    id: &str,
    start_line: Option<u32>,
    end_line: Option<u32>,
    anchor: &Option<String>,
    lsp_symbol: &Option<String>,
    head_sha: &str,
) -> Result<RefCommandOutcome, (String, Option<FailureContext>)> {
    // Find the reference to determine its type
    let reference = doc_repo
        .find_reference_by_id(id)
        .await
        .map_err(|e| (e.to_string(), None))?;

    let reference = reference.ok_or_else(|| {
        (
            format!("Reference '{}' not found", id),
            Some(FailureContext::ReferenceNotFound {
                reference_id: id.to_string(),
            }),
        )
    })?;

    use crate::models::Reference;
    use crate::repositories::{UpdateCodeReferenceParams, UpdateTextReferenceParams};

    match reference {
        Reference::Code(_) => {
            // Update code reference
            // Build new lsp_range if lines provided
            let lsp_range = match (start_line, end_line) {
                (Some(start), Some(end)) => Some(format!("{}:0-{}:0", start, end)),
                _ => None,
            };

            let params = UpdateCodeReferenceParams {
                commit_sha: Some(head_sha),
                lsp_symbol: lsp_symbol.as_deref(),
                lsp_range: lsp_range.as_deref(),
                ..Default::default()
            };

            doc_repo
                .update_code_reference(id, params)
                .await
                .map_err(|e| (e.to_string(), None))?;
        }
        Reference::Text(_) => {
            // Update text reference
            let params = UpdateTextReferenceParams {
                commit_sha: Some(head_sha),
                start_line,
                end_line,
                anchor: anchor.as_deref(),
                ..Default::default()
            };

            doc_repo
                .update_text_reference(id, params)
                .await
                .map_err(|e| (e.to_string(), None))?;
        }
    }

    Ok(RefCommandOutcome::Updated {
        reference_id: id.to_string(),
    })
}

/// Execute a Delete command.
async fn execute_delete(
    doc_repo: &DocumentRepository,
    id: &str,
) -> Result<RefCommandOutcome, (String, Option<FailureContext>)> {
    // Check if reference exists
    let reference = doc_repo
        .find_reference_by_id(id)
        .await
        .map_err(|e| (e.to_string(), None))?;

    if reference.is_none() {
        return Err((
            format!("Reference '{}' not found", id),
            Some(FailureContext::ReferenceNotFound {
                reference_id: id.to_string(),
            }),
        ));
    }

    // Check if reference is attached to any entities
    let attached = doc_repo
        .get_attached_entities(id)
        .await
        .map_err(|e| (e.to_string(), None))?;

    if !attached.is_empty() {
        let entities: Vec<AttachedEntityInfo> = attached
            .into_iter()
            .map(|(entity_id, entity_name)| AttachedEntityInfo {
                id: entity_id,
                name: entity_name,
            })
            .collect();

        return Err((
            format!(
                "Reference '{}' is attached to {} entities",
                id,
                entities.len()
            ),
            Some(FailureContext::AttachedEntities { entities }),
        ));
    }

    // Delete the reference
    doc_repo
        .delete_reference(id)
        .await
        .map_err(|e| (e.to_string(), None))?;

    Ok(RefCommandOutcome::Deleted {
        reference_id: id.to_string(),
    })
}
