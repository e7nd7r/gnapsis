//! Validation and LSP integration tools.

use rmcp::{
    handler::server::wrapper::Parameters,
    model::CallToolResult,
    schemars::{self, JsonSchema},
    tool, tool_router, ErrorData as McpError,
};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::mcp::protocol::Response;
use crate::mcp::server::McpServer;
use crate::models::Reference;
use crate::repositories::{DocumentRepository, UpdateCodeReferenceParams};
use crate::services::{ValidationIssue, ValidationService};

// ============================================================================
// Parameter Types
// ============================================================================

/// Parameters for validate_graph tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ValidateGraphParams {
    /// Check for orphan entities (no parent at non-Domain scope).
    #[serde(default = "default_true")]
    pub check_orphans: Option<bool>,
    /// Check for cycles in BELONGS_TO relationships.
    #[serde(default = "default_true")]
    pub check_cycles: Option<bool>,
    /// Check for scope violations (child scope not deeper than parent).
    #[serde(default = "default_true")]
    pub check_scope_violations: Option<bool>,
    /// Check for entities without any classification.
    #[serde(default = "default_true")]
    pub check_unclassified: Option<bool>,
    /// Check for entities without any document references.
    #[serde(default = "default_true")]
    pub check_no_references: Option<bool>,
}

fn default_true() -> Option<bool> {
    Some(true)
}

/// An LSP symbol from the language server.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct LspSymbol {
    /// Symbol name (e.g., "McpServer", "resolve").
    pub name: String,
    /// LSP SymbolKind as integer (1=File, 5=Class, 6=Method, 12=Function, 23=Struct, etc.).
    pub kind: i32,
    /// Starting line (1-indexed).
    pub start_line: u32,
    /// Ending line (1-indexed).
    pub end_line: u32,
    /// Container name (e.g., "impl McpServer" for methods).
    #[serde(default)]
    pub container_name: Option<String>,
}

/// Parameters for lsp_refresh tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct LspRefreshParams {
    /// Path to the document being refreshed.
    pub document_path: String,
    /// Source ID from project config. Defaults to "default" if not specified.
    #[serde(default = "crate::config::default_source_id")]
    pub source_id: String,
    /// LSP symbols from the language server.
    pub lsp_symbols: Vec<LspSymbol>,
}

// ============================================================================
// Response Types
// ============================================================================

/// Result of graph validation.
#[derive(Debug, Serialize)]
pub struct ValidateGraphResult {
    /// Whether the graph passed all checks.
    pub valid: bool,
    /// Total number of issues found.
    pub issue_count: usize,
    /// Orphan entities (no parent at non-Domain scope).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub orphans: Vec<ValidationIssue>,
    /// Entities involved in BELONGS_TO cycles.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cycles: Vec<ValidationIssue>,
    /// Scope violations (child scope not deeper than parent).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub scope_violations: Vec<ValidationIssue>,
    /// Entities without any classification.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unclassified: Vec<ValidationIssue>,
    /// Entities without any document references.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub no_references: Vec<ValidationIssue>,
}

/// Result of LSP refresh.
#[derive(Debug, Serialize)]
pub struct LspRefreshResult {
    /// Document path refreshed.
    pub document_path: String,
    /// Number of references updated.
    pub updated_count: usize,
    /// References that were updated.
    pub updated: Vec<UpdatedReference>,
    /// Symbols that couldn't be matched.
    pub unmatched_count: usize,
}

/// A reference that was updated.
#[derive(Debug, Serialize)]
pub struct UpdatedReference {
    /// Reference ID.
    pub id: String,
    /// Symbol name.
    pub symbol_name: String,
    /// Previous start line.
    pub old_start_line: u32,
    /// New start line.
    pub new_start_line: u32,
    /// Previous end line.
    pub old_end_line: u32,
    /// New end line.
    pub new_end_line: u32,
}

// ============================================================================
// Tool Router
// ============================================================================

#[tool_router(router = validation_tools, vis = "pub(crate)")]
impl McpServer {
    /// Validate graph integrity.
    ///
    /// Checks for common issues like orphan entities, cycles in composition,
    /// scope violations, and missing classifications.
    #[tool(
        description = "Validate graph integrity. Checks for orphans, cycles, scope violations, and missing classifications."
    )]
    pub async fn validate_graph(
        &self,
        Parameters(params): Parameters<ValidateGraphParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!("Running validate_graph tool");

        let service = self.resolve::<ValidationService>();

        let mut result = ValidateGraphResult {
            valid: true,
            issue_count: 0,
            orphans: Vec::new(),
            cycles: Vec::new(),
            scope_violations: Vec::new(),
            unclassified: Vec::new(),
            no_references: Vec::new(),
        };

        // Check for orphans
        if params.check_orphans.unwrap_or(true) {
            let orphans = service.find_orphan_entities().await?;
            result.orphans = orphans;
        }

        // Check for cycles
        if params.check_cycles.unwrap_or(true) {
            let cycles = service.find_cycles().await?;
            result.cycles = cycles;
        }

        // Check for scope violations
        if params.check_scope_violations.unwrap_or(true) {
            let violations = service.find_scope_violations().await?;
            result.scope_violations = violations;
        }

        // Check for unclassified
        if params.check_unclassified.unwrap_or(true) {
            let unclassified = service.find_unclassified_entities().await?;
            result.unclassified = unclassified;
        }

        // Check for entities without references
        if params.check_no_references.unwrap_or(true) {
            let no_refs = service.find_entities_without_references().await?;
            result.no_references = no_refs;
        }

        result.issue_count = result.orphans.len()
            + result.cycles.len()
            + result.scope_violations.len()
            + result.unclassified.len()
            + result.no_references.len();
        result.valid = result.issue_count == 0;

        tracing::info!(
            valid = result.valid,
            issues = result.issue_count,
            "Graph validation complete"
        );

        Response(result, None).into()
    }

    /// Refresh document references using LSP symbol locations.
    ///
    /// Updates line numbers for existing references by matching them
    /// with current LSP symbols.
    #[tool(
        description = "Refresh document references using LSP symbol locations. Updates line numbers from LSP data."
    )]
    pub async fn lsp_refresh(
        &self,
        Parameters(params): Parameters<LspRefreshParams>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(
            path = %params.document_path,
            symbols = params.lsp_symbols.len(),
            "Running lsp_refresh tool"
        );

        let doc_repo = self.resolve::<DocumentRepository>();

        // Get existing references for this document
        let existing_refs = doc_repo
            .get_document_references(&params.document_path)
            .await
            .map_err(|e: AppError| McpError::from(e))?;

        // Build map of LSP symbols by name for quick lookup
        let symbol_map: std::collections::HashMap<String, &LspSymbol> = params
            .lsp_symbols
            .iter()
            .flat_map(|s| {
                let mut entries = vec![(s.name.clone(), s)];
                if let Some(container) = &s.container_name {
                    entries.push((format!("{}::{}", container, s.name), s));
                }
                entries
            })
            .collect();

        let mut updated = Vec::new();
        let mut unmatched_count = 0;

        // Only process CodeReferences since they have lsp_symbol
        for doc_ref in &existing_refs {
            if let Reference::Code(code_ref) = doc_ref {
                let lsp_symbol_name = &code_ref.lsp_symbol;
                if let Some(symbol) = symbol_map.get(lsp_symbol_name) {
                    // Parse current line numbers from lsp_range
                    let (current_start, current_end) =
                        parse_lsp_range_lines(&code_ref.lsp_range).unwrap_or((0, 0));

                    // Check if lines changed
                    if current_start != symbol.start_line || current_end != symbol.end_line {
                        // Build new LSP range
                        let new_lsp_range = format!(
                            r#"{{"start":{{"line":{},"character":0}},"end":{{"line":{},"character":0}}}}"#,
                            symbol.start_line.saturating_sub(1), // LSP is 0-indexed
                            symbol.end_line.saturating_sub(1)
                        );

                        // Update the reference
                        let update_params = UpdateCodeReferenceParams {
                            lsp_range: Some(&new_lsp_range),
                            ..Default::default()
                        };

                        doc_repo
                            .update_code_reference(&code_ref.id, update_params)
                            .await
                            .map_err(|e: AppError| McpError::from(e))?;

                        updated.push(UpdatedReference {
                            id: code_ref.id.clone(),
                            symbol_name: lsp_symbol_name.clone(),
                            old_start_line: current_start,
                            new_start_line: symbol.start_line,
                            old_end_line: current_end,
                            new_end_line: symbol.end_line,
                        });
                    }
                } else {
                    unmatched_count += 1;
                }
            }
        }

        let result = LspRefreshResult {
            document_path: params.document_path,
            updated_count: updated.len(),
            updated,
            unmatched_count,
        };

        tracing::info!(
            updated = result.updated_count,
            unmatched = result.unmatched_count,
            "LSP refresh complete"
        );

        Response(result, None).into()
    }
}

/// Parse LSP range string to extract start and end lines.
fn parse_lsp_range_lines(lsp_range: &str) -> Option<(u32, u32)> {
    // Try JSON format first: {"start":{"line":X,"character":Y},"end":{"line":Z,"character":W}}
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

/// Map LSP SymbolKind to scope and category suggestions.
pub fn lsp_kind_to_suggestions(kind: i32) -> (&'static str, &'static str, &'static str) {
    // LSP SymbolKind values: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#symbolKind
    match kind {
        1 => ("File", "Namespace", "module"),      // File
        2 => ("Module", "Namespace", "module"),    // Module
        3 => ("Namespace", "Namespace", "module"), // Namespace
        4 => ("Package", "Namespace", "module"),   // Package
        5 => ("Class", "Component", "class"),      // Class
        6 => ("Method", "Unit", "method"),         // Method
        7 => ("Property", "Unit", "property"),     // Property
        8 => ("Field", "Unit", "field"),           // Field
        9 => ("Constructor", "Unit", "method"),    // Constructor
        10 => ("Enum", "Component", "enum"),       // Enum
        11 => ("Interface", "Component", "trait"), // Interface
        12 => ("Function", "Unit", "function"),    // Function
        13 => ("Variable", "Unit", "field"),       // Variable
        14 => ("Constant", "Unit", "constant"),    // Constant
        15 => ("String", "Unit", "constant"),      // String
        16 => ("Number", "Unit", "constant"),      // Number
        17 => ("Boolean", "Unit", "constant"),     // Boolean
        18 => ("Array", "Unit", "field"),          // Array
        19 => ("Object", "Component", "struct"),   // Object
        20 => ("Key", "Unit", "field"),            // Key
        21 => ("Null", "Unit", "constant"),        // Null
        22 => ("EnumMember", "Unit", "constant"),  // EnumMember
        23 => ("Struct", "Component", "struct"),   // Struct
        24 => ("Event", "Unit", "method"),         // Event
        25 => ("Operator", "Unit", "function"),    // Operator
        26 => ("TypeParameter", "Unit", "field"),  // TypeParameter
        _ => ("Unknown", "Component", "struct"),   // Default
    }
}
