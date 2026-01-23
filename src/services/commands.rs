//! Command execution engine for entity operations.
//!
//! Entities are managed through commands that execute sequentially with
//! partial execution on failure. This allows clear feedback about what
//! succeeded and what failed.
//!
//! # Architecture
//!
//! This module implements the **Command Pattern** where each operation is
//! encapsulated as a command object. The `CommandService` processes these
//! commands sequentially, stopping on first failure.
//!
//! # Example
//!
//! ```ignore
//! let commands = vec![
//!     EntityCommand::Add(NewReference::Code { ... }),
//!     EntityCommand::Relate { entity_id: "ent-123", note: Some("Related to auth") },
//! ];
//!
//! let result = command_service.execute(entity_id, commands).await?;
//! if result.is_success() {
//!     println!("All {} commands executed", result.executed.len());
//! } else {
//!     println!("Failed at command {}: {}", result.failed.unwrap().index, ...);
//! }
//! ```

use serde::{Deserialize, Serialize};

// ============================================================================
// Command Types
// ============================================================================

/// Commands for entity operations.
///
/// Commands execute sequentially. If one fails, subsequent commands are skipped
/// but previously executed commands remain applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EntityCommand {
    /// Attach an existing reference to this entity.
    Attach {
        /// ID of the reference to attach.
        reference_id: String,
    },

    /// Detach a reference from this entity.
    ///
    /// The reference continues to exist but is no longer associated with this entity.
    Unattach {
        /// ID of the reference to detach.
        reference_id: String,
    },

    /// Create and attach a new reference to this entity.
    Add(NewReference),

    /// Create a RELATED_TO relationship to another entity.
    Relate {
        /// Target entity ID.
        entity_id: String,
        /// Optional note describing the relationship (embedded for semantic search).
        #[serde(default)]
        note: Option<String>,
    },

    /// Remove a RELATED_TO relationship.
    Unrelate {
        /// Target entity ID.
        entity_id: String,
    },

    /// Create a code-level link (Component/Unit scope only).
    Link {
        /// Target entity ID.
        entity_id: String,
        /// Type of link.
        link_type: LinkType,
    },

    /// Remove a code-level link.
    Unlink {
        /// Target entity ID.
        entity_id: String,
        /// Type of link.
        link_type: LinkType,
    },
}

/// Types of code-level links between Component/Unit entities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkType {
    /// Function/method calls another.
    Calls,
    /// Module imports another.
    Imports,
    /// Type implements a trait/interface.
    Implements,
    /// Code instantiates a type.
    Instantiates,
}

impl LinkType {
    /// Get the relationship type string for Neo4j.
    pub fn as_relationship(&self) -> &'static str {
        match self {
            LinkType::Calls => "CALLS",
            LinkType::Imports => "IMPORTS",
            LinkType::Implements => "IMPLEMENTS",
            LinkType::Instantiates => "INSTANTIATES",
        }
    }
}

/// A new reference to create and attach.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "ref_type", rename_all = "snake_case")]
pub enum NewReference {
    /// Code reference with LSP metadata.
    Code {
        /// Path to the source file.
        document_path: String,
        /// LSP symbol name (e.g., "impl Foo::bar").
        lsp_symbol: String,
        /// Description of what this reference points to.
        description: String,
        /// Start line (from LSP, or manual if no LSP available).
        #[serde(default)]
        start_line: Option<u32>,
        /// End line (from LSP, or manual if no LSP available).
        #[serde(default)]
        end_line: Option<u32>,
    },

    /// Text/documentation reference with line range.
    Text {
        /// Path to the document.
        document_path: String,
        /// Description of what this reference points to.
        description: String,
        /// Starting line number.
        start_line: u32,
        /// Ending line number.
        end_line: u32,
        /// Optional semantic anchor (e.g., "## Architecture").
        #[serde(default)]
        anchor: Option<String>,
    },
}

impl NewReference {
    /// Get the document path for this reference.
    pub fn document_path(&self) -> &str {
        match self {
            NewReference::Code { document_path, .. } => document_path,
            NewReference::Text { document_path, .. } => document_path,
        }
    }

    /// Get the description for this reference.
    pub fn description(&self) -> &str {
        match self {
            NewReference::Code { description, .. } => description,
            NewReference::Text { description, .. } => description,
        }
    }
}

// ============================================================================
// Execution Results
// ============================================================================

/// Result of executing a sequence of commands.
#[derive(Debug, Clone, Serialize)]
pub struct CommandResult {
    /// Commands that executed successfully.
    pub executed: Vec<ExecutedCommand>,

    /// The command that failed, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed: Option<FailedCommand>,

    /// Commands that were skipped due to earlier failure.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<EntityCommand>,
}

impl CommandResult {
    /// Create a successful result with all commands executed.
    pub fn success(executed: Vec<ExecutedCommand>) -> Self {
        Self {
            executed,
            failed: None,
            skipped: Vec::new(),
        }
    }

    /// Create a result with a failure.
    pub fn with_failure(
        executed: Vec<ExecutedCommand>,
        failed: FailedCommand,
        skipped: Vec<EntityCommand>,
    ) -> Self {
        Self {
            executed,
            failed: Some(failed),
            skipped,
        }
    }

    /// Check if all commands executed successfully.
    pub fn is_success(&self) -> bool {
        self.failed.is_none()
    }

    /// Get the total number of commands (executed + failed + skipped).
    pub fn total_commands(&self) -> usize {
        self.executed.len() + if self.failed.is_some() { 1 } else { 0 } + self.skipped.len()
    }
}

/// A successfully executed command with its outcome.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutedCommand {
    /// Index of the command in the original sequence.
    pub index: usize,

    /// The command that was executed.
    pub command: EntityCommand,

    /// Outcome of the execution.
    pub outcome: CommandOutcome,
}

/// Outcome of a successfully executed command.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome_type", rename_all = "snake_case")]
pub enum CommandOutcome {
    /// Reference was attached.
    Attached { reference_id: String },

    /// Reference was detached.
    Unattached { reference_id: String },

    /// New reference was created and attached.
    Added { reference_id: String },

    /// Relationship was created.
    Related { entity_id: String },

    /// Relationship was removed.
    Unrelated { entity_id: String },

    /// Code link was created.
    Linked {
        entity_id: String,
        link_type: LinkType,
    },

    /// Code link was removed.
    Unlinked {
        entity_id: String,
        link_type: LinkType,
    },
}

/// A command that failed during execution.
#[derive(Debug, Clone, Serialize)]
pub struct FailedCommand {
    /// Index of the command in the original sequence.
    pub index: usize,

    /// The command that failed.
    pub command: EntityCommand,

    /// Error message describing the failure.
    pub error: String,

    /// Additional context about the failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<FailureContext>,
}

impl FailedCommand {
    /// Create a new failed command without context.
    pub fn new(index: usize, command: EntityCommand, error: impl Into<String>) -> Self {
        Self {
            index,
            command,
            error: error.into(),
            context: None,
        }
    }

    /// Create a new failed command with context.
    pub fn with_context(
        index: usize,
        command: EntityCommand,
        error: impl Into<String>,
        context: FailureContext,
    ) -> Self {
        Self {
            index,
            command,
            error: error.into(),
            context: Some(context),
        }
    }
}

/// Additional context for command failures.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "context_type", rename_all = "snake_case")]
pub enum FailureContext {
    /// Reference cannot be deleted because entities are attached.
    AttachedEntities {
        /// Entities that have this reference attached.
        entities: Vec<AttachedEntityInfo>,
    },

    /// Entity not found.
    EntityNotFound {
        /// The entity ID that was not found.
        entity_id: String,
    },

    /// Reference not found.
    ReferenceNotFound {
        /// The reference ID that was not found.
        reference_id: String,
    },

    /// Scope violation (e.g., Link command on non-Component/Unit entity).
    ScopeViolation {
        /// The entity's actual scope.
        actual_scope: String,
        /// The required scope(s) for this operation.
        required_scopes: Vec<String>,
    },

    /// LSP symbol not found in document.
    SymbolNotFound {
        /// The symbol that was not found.
        symbol: String,
        /// The document path searched.
        document_path: String,
    },

    /// References must be in the same document.
    DocumentMismatch {
        /// Expected document path.
        expected: String,
        /// Actual document path.
        actual: String,
    },
}

/// Information about an entity attached to a reference.
#[derive(Debug, Clone, Serialize)]
pub struct AttachedEntityInfo {
    /// Entity ID.
    pub id: String,
    /// Entity name.
    pub name: String,
}

// ============================================================================
// Command Service
// ============================================================================

use crate::context::{AppEmbedder, Context};
use crate::di::FromContext;
use crate::error::AppError;
use crate::repositories::{DocumentRepository, EntityRepository};

use super::LspService;

/// Service for executing entity commands.
///
/// Processes commands sequentially, stopping on first failure.
/// Previously executed commands remain applied (no rollback).
#[derive(FromContext, Clone)]
pub struct CommandService {
    entity_repo: EntityRepository,
    doc_repo: DocumentRepository,
    embedder: AppEmbedder,
    lsp: LspService,
}

impl CommandService {
    /// Execute a sequence of commands for an entity.
    ///
    /// Commands are executed in order. If a command fails:
    /// - Previously executed commands remain applied
    /// - The failed command is reported with context
    /// - Remaining commands are skipped
    pub async fn execute(
        &self,
        entity_id: &str,
        commands: Vec<EntityCommand>,
    ) -> Result<CommandResult, AppError> {
        let mut executed = Vec::new();

        for (index, command) in commands.iter().enumerate() {
            match self.execute_single(entity_id, command).await {
                Ok(outcome) => {
                    executed.push(ExecutedCommand {
                        index,
                        command: command.clone(),
                        outcome,
                    });
                }
                Err((error, context)) => {
                    let failed = match context {
                        Some(ctx) => {
                            FailedCommand::with_context(index, command.clone(), error, ctx)
                        }
                        None => FailedCommand::new(index, command.clone(), error),
                    };
                    let skipped = commands.into_iter().skip(index + 1).collect();
                    return Ok(CommandResult::with_failure(executed, failed, skipped));
                }
            }
        }

        Ok(CommandResult::success(executed))
    }

    /// Execute a single command.
    ///
    /// Returns Ok(outcome) on success, or Err((message, context)) on failure.
    async fn execute_single(
        &self,
        entity_id: &str,
        command: &EntityCommand,
    ) -> Result<CommandOutcome, (String, Option<FailureContext>)> {
        match command {
            EntityCommand::Attach { reference_id } => {
                self.execute_attach(entity_id, reference_id).await
            }
            EntityCommand::Unattach { reference_id } => {
                self.execute_unattach(entity_id, reference_id).await
            }
            EntityCommand::Add(new_ref) => self.execute_add(entity_id, new_ref).await,
            EntityCommand::Relate {
                entity_id: target_id,
                note,
            } => {
                self.execute_relate(entity_id, target_id, note.as_deref())
                    .await
            }
            EntityCommand::Unrelate {
                entity_id: target_id,
            } => self.execute_unrelate(entity_id, target_id).await,
            EntityCommand::Link {
                entity_id: target_id,
                link_type,
            } => self.execute_link(entity_id, target_id, *link_type).await,
            EntityCommand::Unlink {
                entity_id: target_id,
                link_type,
            } => self.execute_unlink(entity_id, target_id, *link_type).await,
        }
    }

    async fn execute_attach(
        &self,
        entity_id: &str,
        reference_id: &str,
    ) -> Result<CommandOutcome, (String, Option<FailureContext>)> {
        // Verify reference exists
        let reference = self
            .doc_repo
            .find_reference_by_id(reference_id)
            .await
            .map_err(|e| (e.to_string(), None))?;

        if reference.is_none() {
            return Err((
                format!("Reference '{}' not found", reference_id),
                Some(FailureContext::ReferenceNotFound {
                    reference_id: reference_id.to_string(),
                }),
            ));
        }

        // Attach reference to entity
        self.doc_repo
            .attach_reference(entity_id, reference_id)
            .await
            .map_err(|e| (e.to_string(), None))?;

        Ok(CommandOutcome::Attached {
            reference_id: reference_id.to_string(),
        })
    }

    async fn execute_unattach(
        &self,
        entity_id: &str,
        reference_id: &str,
    ) -> Result<CommandOutcome, (String, Option<FailureContext>)> {
        // Detach reference from entity
        self.doc_repo
            .detach_reference(entity_id, reference_id)
            .await
            .map_err(|e| (e.to_string(), None))?;

        Ok(CommandOutcome::Unattached {
            reference_id: reference_id.to_string(),
        })
    }

    async fn execute_add(
        &self,
        entity_id: &str,
        new_ref: &NewReference,
    ) -> Result<CommandOutcome, (String, Option<FailureContext>)> {
        // Generate embedding for description
        let embedding = self
            .embedder
            .embed(new_ref.description())
            .map_err(|e| (format!("Embedding error: {}", e), None))?;

        // Get current commit SHA for the reference
        use crate::git::GitOps;
        let commit_sha = GitOps::open_current()
            .and_then(|git| git.get_head_sha())
            .unwrap_or_default();

        let reference_id = match new_ref {
            NewReference::Code {
                document_path,
                lsp_symbol,
                description,
                start_line,
                end_line,
            } => {
                use crate::repositories::CreateCodeReferenceParams;

                // Try to validate via LSP
                let lsp_info = self.validate_lsp_symbol(document_path, lsp_symbol)?;

                // Use LSP data if available, otherwise fall back to provided values
                let (final_start, final_end, final_kind) = match lsp_info {
                    Some(sym) => (sym.start_line, sym.end_line, sym.kind),
                    None => (start_line.unwrap_or(1), end_line.unwrap_or(1), 0),
                };

                let lsp_range = format!("{}:0-{}:0", final_start, final_end);

                let params = CreateCodeReferenceParams {
                    entity_id,
                    path: document_path,
                    language: "unknown", // Will be determined by file extension
                    commit_sha: &commit_sha,
                    description,
                    embedding: Some(&embedding),
                    lsp_symbol,
                    lsp_kind: final_kind,
                    lsp_range: &lsp_range,
                };

                let code_ref = self
                    .doc_repo
                    .create_code_reference(params)
                    .await
                    .map_err(|e| (e.to_string(), None))?;

                code_ref.id
            }
            NewReference::Text {
                document_path,
                description,
                start_line,
                end_line,
                anchor,
            } => {
                use crate::repositories::CreateTextReferenceParams;

                let params = CreateTextReferenceParams {
                    entity_id,
                    path: document_path,
                    content_type: "markdown",
                    commit_sha: &commit_sha,
                    description,
                    embedding: Some(&embedding),
                    start_line: *start_line,
                    end_line: *end_line,
                    anchor: anchor.as_deref(),
                };

                let text_ref = self
                    .doc_repo
                    .create_text_reference(params)
                    .await
                    .map_err(|e| (e.to_string(), None))?;

                text_ref.id
            }
        };

        Ok(CommandOutcome::Added { reference_id })
    }

    /// Validate LSP symbol and get its metadata.
    ///
    /// Validate a code reference symbol via LSP.
    ///
    /// Returns:
    /// - `Ok(Some(symbol))` if LSP found it
    /// - `Ok(None)` if LSP unavailable (caller uses fallback)
    /// - `Err` if symbol not found (validation failure)
    fn validate_lsp_symbol(
        &self,
        document_path: &str,
        lsp_symbol: &str,
    ) -> Result<Option<super::LspSymbol>, (String, Option<FailureContext>)> {
        tracing::debug!(path = %document_path, symbol = %lsp_symbol, "Validating LSP symbol");

        match self.lsp.find_symbol(document_path, lsp_symbol) {
            Ok(symbol) => {
                tracing::info!(
                    symbol = %lsp_symbol,
                    start = symbol.start_line,
                    end = symbol.end_line,
                    kind = symbol.kind,
                    "LSP symbol found"
                );
                Ok(Some(symbol))
            }
            Err(ref err @ super::LspError::Unavailable(_)) => {
                tracing::warn!(error = %err, "LSP unavailable, using fallback");
                Ok(None)
            }
            Err(ref err @ super::LspError::SymbolNotFound { .. }) => {
                tracing::warn!(error = %err, "LSP symbol not found");
                Err((err.to_string(), Option::<FailureContext>::from(err)))
            }
        }
    }

    async fn execute_relate(
        &self,
        entity_id: &str,
        target_id: &str,
        note: Option<&str>,
    ) -> Result<CommandOutcome, (String, Option<FailureContext>)> {
        // Verify target entity exists
        let target = self
            .entity_repo
            .find_by_id(target_id)
            .await
            .map_err(|e| (e.to_string(), None))?;

        if target.is_none() {
            return Err((
                format!("Entity '{}' not found", target_id),
                Some(FailureContext::EntityNotFound {
                    entity_id: target_id.to_string(),
                }),
            ));
        }

        // Generate embedding for note if provided
        let note_embedding = if let Some(note_text) = note {
            Some(
                self.embedder
                    .embed(note_text)
                    .map_err(|e| (format!("Embedding error: {}", e), None))?,
            )
        } else {
            None
        };

        // Create RELATED_TO relationship (relation_type is None for standard RELATED_TO)
        self.entity_repo
            .add_related(entity_id, target_id, None, note, note_embedding.as_deref())
            .await
            .map_err(|e| (e.to_string(), None))?;

        Ok(CommandOutcome::Related {
            entity_id: target_id.to_string(),
        })
    }

    async fn execute_unrelate(
        &self,
        entity_id: &str,
        target_id: &str,
    ) -> Result<CommandOutcome, (String, Option<FailureContext>)> {
        self.entity_repo
            .remove_related(entity_id, target_id)
            .await
            .map_err(|e| (e.to_string(), None))?;

        Ok(CommandOutcome::Unrelated {
            entity_id: target_id.to_string(),
        })
    }

    async fn execute_link(
        &self,
        entity_id: &str,
        target_id: &str,
        link_type: LinkType,
    ) -> Result<CommandOutcome, (String, Option<FailureContext>)> {
        // Note: Scope validation is deferred to Phase 3 (Entity Tools Redesign)
        // when we have proper scope tracking on entities.
        // For now, just verify entities exist.

        // Verify source entity exists
        let entity = self
            .entity_repo
            .find_by_id(entity_id)
            .await
            .map_err(|e| (e.to_string(), None))?;

        if entity.is_none() {
            return Err((
                format!("Entity '{}' not found", entity_id),
                Some(FailureContext::EntityNotFound {
                    entity_id: entity_id.to_string(),
                }),
            ));
        }

        // Verify target entity exists
        let target = self
            .entity_repo
            .find_by_id(target_id)
            .await
            .map_err(|e| (e.to_string(), None))?;

        if target.is_none() {
            return Err((
                format!("Entity '{}' not found", target_id),
                Some(FailureContext::EntityNotFound {
                    entity_id: target_id.to_string(),
                }),
            ));
        }

        // Create link
        self.entity_repo
            .add_link(entity_id, target_id, link_type.as_relationship())
            .await
            .map_err(|e| (e.to_string(), None))?;

        Ok(CommandOutcome::Linked {
            entity_id: target_id.to_string(),
            link_type,
        })
    }

    async fn execute_unlink(
        &self,
        entity_id: &str,
        target_id: &str,
        link_type: LinkType,
    ) -> Result<CommandOutcome, (String, Option<FailureContext>)> {
        self.entity_repo
            .remove_link(entity_id, target_id, link_type.as_relationship())
            .await
            .map_err(|e| (e.to_string(), None))?;

        Ok(CommandOutcome::Unlinked {
            entity_id: target_id.to_string(),
            link_type,
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_command_serialization() {
        let cmd = EntityCommand::Attach {
            reference_id: "ref-123".to_string(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"attach\""));
        assert!(json.contains("\"reference_id\":\"ref-123\""));
    }

    #[test]
    fn test_new_reference_serialization() {
        let code_ref = NewReference::Code {
            document_path: "src/main.rs".to_string(),
            lsp_symbol: "fn main".to_string(),
            description: "Main entry point".to_string(),
            start_line: Some(1),
            end_line: Some(10),
        };
        let json = serde_json::to_string(&code_ref).unwrap();
        assert!(json.contains("\"ref_type\":\"code\""));

        let text_ref = NewReference::Text {
            document_path: "README.md".to_string(),
            description: "Project overview".to_string(),
            start_line: 1,
            end_line: 50,
            anchor: Some("## Overview".to_string()),
        };
        let json = serde_json::to_string(&text_ref).unwrap();
        assert!(json.contains("\"ref_type\":\"text\""));
    }

    #[test]
    fn test_link_type_as_relationship() {
        assert_eq!(LinkType::Calls.as_relationship(), "CALLS");
        assert_eq!(LinkType::Imports.as_relationship(), "IMPORTS");
        assert_eq!(LinkType::Implements.as_relationship(), "IMPLEMENTS");
        assert_eq!(LinkType::Instantiates.as_relationship(), "INSTANTIATES");
    }

    #[test]
    fn test_command_result_success() {
        let result = CommandResult::success(vec![]);
        assert!(result.is_success());
        assert_eq!(result.total_commands(), 0);
    }

    #[test]
    fn test_command_result_with_failure() {
        let executed = vec![ExecutedCommand {
            index: 0,
            command: EntityCommand::Attach {
                reference_id: "ref-1".to_string(),
            },
            outcome: CommandOutcome::Attached {
                reference_id: "ref-1".to_string(),
            },
        }];

        let failed = FailedCommand::new(
            1,
            EntityCommand::Attach {
                reference_id: "ref-2".to_string(),
            },
            "Reference not found",
        );

        let skipped = vec![EntityCommand::Attach {
            reference_id: "ref-3".to_string(),
        }];

        let result = CommandResult::with_failure(executed, failed, skipped);
        assert!(!result.is_success());
        assert_eq!(result.total_commands(), 3);
    }

    #[test]
    fn test_failed_command_with_context() {
        let failed = FailedCommand::with_context(
            0,
            EntityCommand::Unattach {
                reference_id: "ref-123".to_string(),
            },
            "Reference is attached to entities",
            FailureContext::AttachedEntities {
                entities: vec![AttachedEntityInfo {
                    id: "ent-1".to_string(),
                    name: "MyComponent".to_string(),
                }],
            },
        );

        let json = serde_json::to_string(&failed).unwrap();
        assert!(json.contains("attached_entities"));
        assert!(json.contains("MyComponent"));
    }
}
