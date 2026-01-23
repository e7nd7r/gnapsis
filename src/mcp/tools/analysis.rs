//! Analysis tools for unified document inspection.
//!
//! Provides the analyze_document tool that replaces lsp_analyze,
//! validate_documents, and get_document_references with line-level
//! staleness detection via git hunk overlap.

use rmcp::{
    handler::server::wrapper::Parameters,
    model::CallToolResult,
    schemars::{self, JsonSchema},
    tool, tool_router, ErrorData as McpError,
};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::git::{DiffHunk, GitOps};
use crate::mcp::protocol::Response;
use crate::mcp::server::McpServer;
use crate::mcp::tools::validation::lsp_kind_to_suggestions;
use crate::models::Reference;
use crate::repositories::DocumentRepository;

// ============================================================================
// Parameter Types
// ============================================================================

/// Parameters for analyze_document tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AnalyzeDocumentParams {
    /// Path to the document to analyze.
    pub document_path: String,
    /// Include tracked references (default: true).
    #[serde(default = "default_true")]
    pub include_tracked: Option<bool>,
    /// Include untracked LSP symbols (default: true).
    /// Note: Requires external LSP symbols to be provided.
    #[serde(default = "default_true")]
    pub include_untracked: Option<bool>,
    /// Include git diff hunks (default: true).
    #[serde(default = "default_true")]
    pub include_diffs: Option<bool>,
    /// LSP symbols for untracked detection (optional).
    /// If not provided, untracked detection is skipped.
    #[serde(default)]
    pub lsp_symbols: Option<Vec<LspSymbolInput>>,
}

fn default_true() -> Option<bool> {
    Some(true)
}

/// LSP symbol input for untracked detection.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct LspSymbolInput {
    /// Symbol name (e.g., "McpServer", "resolve").
    pub name: String,
    /// LSP SymbolKind as integer.
    pub kind: i32,
    /// Starting line (1-indexed).
    pub start_line: u32,
    /// Ending line (1-indexed).
    pub end_line: u32,
    /// Container name (e.g., "impl McpServer" for methods).
    #[serde(default)]
    pub container_name: Option<String>,
}

// ============================================================================
// Response Types
// ============================================================================

/// Result of document analysis.
#[derive(Debug, Serialize)]
pub struct AnalyzeDocumentResult {
    /// Path to the analyzed document.
    pub document_path: String,
    /// Document type: "code" or "text".
    pub document_type: String,
    /// Current HEAD commit SHA.
    pub current_commit: String,

    /// Tracked references in this document.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tracked: Vec<TrackedReference>,

    /// Untracked LSP symbols (code files only).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub untracked: Vec<UntrackedSymbol>,

    /// Entities that have references in this document.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entities: Vec<EntitySummary>,

    /// Git diff hunks for this file.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub diff_hunks: Vec<HunkInfo>,

    /// Analysis summary.
    pub summary: AnalysisSummary,
}

/// A tracked reference with staleness information.
#[derive(Debug, Serialize)]
pub struct TrackedReference {
    /// Reference ID.
    pub id: String,
    /// Entity ID this reference belongs to.
    pub entity_id: String,
    /// Entity name this reference belongs to.
    pub entity_name: String,
    /// Starting line (1-indexed).
    pub start_line: u32,
    /// Ending line (1-indexed).
    pub end_line: u32,
    /// LSP symbol name (for code references).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lsp_symbol: Option<String>,
    /// Anchor (for text references).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    /// Reference type: "code" or "text".
    pub reference_type: String,
    /// Commit SHA when this reference was last updated.
    pub reference_commit: String,
    /// Whether this reference is stale.
    pub is_stale: bool,
    /// Reason for staleness.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_reason: Option<String>,
    /// Hunks that affect this reference.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub affected_hunks: Vec<HunkInfo>,
}

/// An untracked LSP symbol.
#[derive(Debug, Serialize)]
pub struct UntrackedSymbol {
    /// Symbol name.
    pub name: String,
    /// LSP symbol kind.
    pub kind: i32,
    /// Human-readable kind name.
    pub kind_name: String,
    /// Starting line (1-indexed).
    pub start_line: u32,
    /// Ending line (1-indexed).
    pub end_line: u32,
    /// Container name if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,
    /// Suggested scope for entity creation.
    pub suggested_scope: String,
    /// Suggested category for classification.
    pub suggested_category: String,
}

/// Summary of an entity with references in the document.
#[derive(Debug, Serialize)]
pub struct EntitySummary {
    /// Entity ID.
    pub id: String,
    /// Entity name.
    pub name: String,
    /// Number of references in this document.
    pub reference_count: usize,
}

/// Information about a git diff hunk.
#[derive(Debug, Clone, Serialize)]
pub struct HunkInfo {
    /// Starting line in old file (1-indexed).
    pub old_start: u32,
    /// Number of lines in old file.
    pub old_lines: u32,
    /// Starting line in new file (1-indexed).
    pub new_start: u32,
    /// Number of lines in new file.
    pub new_lines: u32,
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

/// Summary statistics for the analysis.
#[derive(Debug, Serialize)]
pub struct AnalysisSummary {
    /// Number of tracked references.
    pub tracked_count: usize,
    /// Number of stale references.
    pub stale_count: usize,
    /// Number of untracked LSP symbols.
    pub untracked_count: usize,
    /// Number of entities with references.
    pub entity_count: usize,
}

// ============================================================================
// Tool Router
// ============================================================================

#[tool_router(router = analysis_tools, vis = "pub(crate)")]
impl McpServer {
    /// Analyze a document for tracked references, staleness, and untracked symbols.
    ///
    /// Replaces lsp_analyze, validate_documents, and get_document_references
    /// with line-level staleness detection via git hunk overlap.
    #[tool(
        description = "Unified document analysis: tracked refs with staleness, untracked symbols, git diffs. Pass lsp_symbols for untracked detection."
    )]
    pub async fn analyze_document(
        &self,
        Parameters(params): Parameters<AnalyzeDocumentParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(path = %params.document_path, "Running analyze_document tool");

        let doc_repo = self.resolve::<DocumentRepository>();

        // Get current HEAD commit
        let git = GitOps::open_current().map_err(McpError::from)?;
        let head_sha = git.get_head_sha().map_err(McpError::from)?;

        // Determine document type from extension
        let document_type = detect_document_type(&params.document_path);

        // Get all references in this document
        let references = doc_repo
            .get_document_references(&params.document_path)
            .await
            .map_err(|e: AppError| McpError::from(e))?;

        // Get entity info for each reference
        let entity_refs = get_entity_references(&doc_repo, &params.document_path).await?;

        // Get diff hunks if requested
        let diff_hunks = if params.include_diffs.unwrap_or(true) {
            get_diff_hunks(&git, &params.document_path, &references, &head_sha)?
        } else {
            Vec::new()
        };

        // Build tracked references with staleness info
        let tracked = if params.include_tracked.unwrap_or(true) {
            build_tracked_references(&references, &entity_refs, &diff_hunks)?
        } else {
            Vec::new()
        };

        // Find untracked symbols if LSP symbols provided
        let untracked = if params.include_untracked.unwrap_or(true) {
            if let Some(lsp_symbols) = &params.lsp_symbols {
                find_untracked_symbols(lsp_symbols, &references)
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // Build entity summary
        let entities = build_entity_summary(&entity_refs);

        // Calculate summary
        let stale_count = tracked.iter().filter(|r| r.is_stale).count();
        let summary = AnalysisSummary {
            tracked_count: tracked.len(),
            stale_count,
            untracked_count: untracked.len(),
            entity_count: entities.len(),
        };

        let result = AnalyzeDocumentResult {
            document_path: params.document_path,
            document_type,
            current_commit: head_sha,
            tracked,
            untracked,
            entities,
            diff_hunks: diff_hunks.iter().map(HunkInfo::from).collect(),
            summary,
        };

        tracing::info!(
            tracked = result.summary.tracked_count,
            stale = result.summary.stale_count,
            untracked = result.summary.untracked_count,
            "Document analysis complete"
        );

        Response(result).into()
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Detect document type from file extension.
fn detect_document_type(path: &str) -> String {
    let code_extensions = [
        "rs", "ts", "tsx", "js", "jsx", "py", "go", "java", "c", "cpp", "h", "hpp", "rb", "php",
        "swift", "kt", "scala", "cs", "fs", "lua", "sh", "bash", "zsh",
    ];

    if let Some(ext) = path.rsplit('.').next() {
        if code_extensions.contains(&ext.to_lowercase().as_str()) {
            return "code".to_string();
        }
    }

    "text".to_string()
}

/// Entity reference info from Neo4j query.
#[derive(Debug)]
struct EntityRefInfo {
    entity_id: String,
    entity_name: String,
    reference_id: String,
}

/// Get entity information for all references in a document.
async fn get_entity_references(
    doc_repo: &DocumentRepository,
    document_path: &str,
) -> Result<Vec<EntityRefInfo>, McpError> {
    use neo4rs::query;

    // Query entities and their references in this document
    let graph = doc_repo.graph();
    let mut result = graph
        .execute(
            query(
                "MATCH (e:Entity)-[:HAS_REFERENCE]->(ref)-[:IN_DOCUMENT]->(d:Document {path: $path})
                 RETURN e.id AS entity_id, e.name AS entity_name, ref.id AS reference_id",
            )
            .param("path", document_path),
        )
        .await
        .map_err(|e| McpError::internal_error(format!("Query failed: {}", e), None))?;

    let mut refs = Vec::new();
    while let Some(row) = result
        .next()
        .await
        .map_err(|e| McpError::internal_error(format!("Row fetch failed: {}", e), None))?
    {
        let entity_id: String = row
            .get("entity_id")
            .map_err(|e| McpError::internal_error(format!("Parse error: {}", e), None))?;
        let entity_name: String = row
            .get("entity_name")
            .map_err(|e| McpError::internal_error(format!("Parse error: {}", e), None))?;
        let reference_id: String = row
            .get("reference_id")
            .map_err(|e| McpError::internal_error(format!("Parse error: {}", e), None))?;

        refs.push(EntityRefInfo {
            entity_id,
            entity_name,
            reference_id,
        });
    }

    Ok(refs)
}

/// Get diff hunks for the document.
fn get_diff_hunks(
    git: &GitOps,
    document_path: &str,
    references: &[Reference],
    head_sha: &str,
) -> Result<Vec<DiffHunk>, McpError> {
    // Find the oldest commit SHA among all references
    let oldest_commit = references
        .iter()
        .filter_map(|r| match r {
            Reference::Code(c) => Some(c.commit_sha.as_str()),
            Reference::Text(t) => Some(t.commit_sha.as_str()),
        })
        .next();

    let Some(from_sha) = oldest_commit else {
        return Ok(Vec::new());
    };

    // If all references are at HEAD, no diffs needed
    if from_sha == head_sha {
        return Ok(Vec::new());
    }

    // Get diff from oldest reference commit to HEAD
    let file_diff = git
        .get_file_diff(document_path, from_sha, Some(head_sha))
        .map_err(McpError::from)?;

    Ok(file_diff.map(|fd| fd.hunks).unwrap_or_default())
}

/// Build tracked references with staleness information.
fn build_tracked_references(
    references: &[Reference],
    entity_refs: &[EntityRefInfo],
    hunks: &[DiffHunk],
) -> Result<Vec<TrackedReference>, McpError> {
    let mut tracked = Vec::new();

    for reference in references {
        // Find entity info for this reference
        let entity_info = entity_refs
            .iter()
            .find(|e| e.reference_id == reference.id())
            .map(|e| (e.entity_id.clone(), e.entity_name.clone()))
            .unwrap_or_else(|| ("unknown".to_string(), "Unknown".to_string()));

        let (start_line, end_line, lsp_symbol, anchor, ref_type, commit_sha) = match reference {
            Reference::Code(code) => {
                // Parse LSP range to get line numbers
                let (start, end) = parse_lsp_range(&code.lsp_range).unwrap_or((1, 1));
                (
                    start,
                    end,
                    Some(code.lsp_symbol.clone()),
                    None,
                    "code",
                    &code.commit_sha,
                )
            }
            Reference::Text(text) => (
                text.start_line,
                text.end_line,
                None,
                text.anchor.clone(),
                "text",
                &text.commit_sha,
            ),
        };

        // Check for staleness via hunk overlap
        let is_stale = GitOps::is_in_changed_region(hunks, start_line, end_line);

        // Find affected hunks
        let affected_hunks: Vec<HunkInfo> = hunks
            .iter()
            .filter(|h| {
                let hunk_end = h.old_start + h.old_lines.saturating_sub(1);
                start_line <= hunk_end && end_line >= h.old_start
            })
            .map(HunkInfo::from)
            .collect();

        let stale_reason = if is_stale {
            Some("lines_changed".to_string())
        } else {
            None
        };

        tracked.push(TrackedReference {
            id: reference.id().to_string(),
            entity_id: entity_info.0,
            entity_name: entity_info.1,
            start_line,
            end_line,
            lsp_symbol,
            anchor,
            reference_type: ref_type.to_string(),
            reference_commit: commit_sha.to_string(),
            is_stale,
            stale_reason,
            affected_hunks,
        });
    }

    Ok(tracked)
}

/// Parse LSP range JSON to extract start and end lines.
fn parse_lsp_range(lsp_range: &str) -> Option<(u32, u32)> {
    // Try JSON format: {"start":{"line":X,"character":Y},"end":{"line":Z,"character":W}}
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(lsp_range) {
        let start_line = value.get("start")?.get("line")?.as_u64()? as u32 + 1; // LSP is 0-indexed
        let end_line = value.get("end")?.get("line")?.as_u64()? as u32 + 1;
        return Some((start_line, end_line));
    }

    // Try simple format: "start_line:start_char-end_line:end_char"
    let parts: Vec<&str> = lsp_range.split('-').collect();
    if parts.len() == 2 {
        let start = parts[0].split(':').next()?.parse().ok()?;
        let end = parts[1].split(':').next()?.parse().ok()?;
        return Some((start, end));
    }

    None
}

/// Find untracked LSP symbols (symbols not yet in the knowledge graph).
fn find_untracked_symbols(
    lsp_symbols: &[LspSymbolInput],
    references: &[Reference],
) -> Vec<UntrackedSymbol> {
    // Build set of tracked symbol names
    let tracked_symbols: std::collections::HashSet<String> = references
        .iter()
        .filter_map(|r| {
            if let Reference::Code(code_ref) = r {
                Some(code_ref.lsp_symbol.clone())
            } else {
                None
            }
        })
        .collect();

    let mut untracked = Vec::new();

    for symbol in lsp_symbols {
        let full_name = if let Some(container) = &symbol.container_name {
            format!("{}::{}", container, symbol.name)
        } else {
            symbol.name.clone()
        };

        // Check if symbol is tracked
        if !tracked_symbols.contains(&full_name) && !tracked_symbols.contains(&symbol.name) {
            let (kind_name, suggested_scope, suggested_category) =
                lsp_kind_to_suggestions(symbol.kind);

            untracked.push(UntrackedSymbol {
                name: full_name,
                kind: symbol.kind,
                kind_name: kind_name.to_string(),
                start_line: symbol.start_line,
                end_line: symbol.end_line,
                container_name: symbol.container_name.clone(),
                suggested_scope: suggested_scope.to_string(),
                suggested_category: suggested_category.to_string(),
            });
        }
    }

    untracked
}

/// Build entity summary from reference info.
fn build_entity_summary(entity_refs: &[EntityRefInfo]) -> Vec<EntitySummary> {
    use std::collections::HashMap;

    let mut entity_map: HashMap<String, (String, usize)> = HashMap::new();

    for info in entity_refs {
        entity_map
            .entry(info.entity_id.clone())
            .and_modify(|(_, count)| *count += 1)
            .or_insert((info.entity_name.clone(), 1));
    }

    entity_map
        .into_iter()
        .map(|(id, (name, count))| EntitySummary {
            id,
            name,
            reference_count: count,
        })
        .collect()
}
