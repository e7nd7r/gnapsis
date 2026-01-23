//! LSP service for querying language server information via Neovim.
//!
//! Provides access to LSP features like document symbols, diagnostics,
//! and symbol validation through Neovim's built-in LSP client.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::context::Context;
use crate::di::FromContext;
use crate::nvim::{LazyNvimClient, NvimClient};

/// Errors from LSP operations.
#[derive(Error, Debug)]
pub enum LspError {
    /// Neovim/LSP is not available.
    #[error("LSP unavailable: {0}")]
    Unavailable(String),

    /// Symbol was not found in the document.
    #[error("symbol '{symbol}' not found in '{path}'")]
    SymbolNotFound { symbol: String, path: String },
}

impl From<LspError> for crate::error::AppError {
    fn from(err: LspError) -> Self {
        match err {
            LspError::Unavailable(msg) => Self::LspUnavailable(msg),
            LspError::SymbolNotFound { symbol, path } => Self::SymbolNotFound { symbol, path },
        }
    }
}

impl From<&LspError> for Option<super::FailureContext> {
    fn from(err: &LspError) -> Self {
        match err {
            LspError::Unavailable(_) => None,
            LspError::SymbolNotFound { symbol, path } => {
                Some(super::FailureContext::SymbolNotFound {
                    symbol: symbol.clone(),
                    document_path: path.clone(),
                })
            }
        }
    }
}

/// A symbol from LSP documentSymbol response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspSymbol {
    /// Symbol name (e.g., "McpServer", "resolve").
    pub name: String,
    /// LSP SymbolKind as integer.
    pub kind: i32,
    /// Starting line (1-indexed).
    pub start_line: u32,
    /// Ending line (1-indexed).
    pub end_line: u32,
    /// Starting column (0-indexed).
    pub start_col: u32,
    /// Ending column (0-indexed).
    pub end_col: u32,
    /// Container name (e.g., "impl McpServer" for methods).
    pub container: Option<String>,
    /// Child symbols (for nested structures).
    #[serde(default)]
    pub children: Vec<LspSymbol>,
}

/// LSP service for querying language server information.
///
/// Uses Neovim's built-in LSP client via lazy connection.
/// Operations gracefully fail if Neovim is not available.
#[derive(FromContext, Clone)]
pub struct LspService {
    nvim: LazyNvimClient,
}

impl LspService {
    /// Check if LSP is available (Neovim connected).
    pub fn is_available(&self) -> bool {
        self.nvim.is_available().unwrap_or(false)
    }

    /// Get document symbols for a file.
    ///
    /// Returns all symbols (functions, structs, methods, etc.) in the file.
    /// The file must be open in a buffer with an active LSP client.
    pub fn get_document_symbols(&self, path: &str) -> Result<Vec<LspSymbol>, String> {
        self.nvim
            .with_client(|client| get_document_symbols_impl(client, path))
    }

    /// Find a symbol by name in a file.
    ///
    /// Returns the symbol if found, or an error if unavailable or not found.
    pub fn find_symbol(&self, path: &str, symbol_name: &str) -> Result<LspSymbol, LspError> {
        tracing::debug!(path = %path, symbol = %symbol_name, "LspService::find_symbol");
        let symbols = self
            .get_document_symbols(path)
            .map_err(LspError::Unavailable)?;
        tracing::debug!(symbol_count = symbols.len(), "Got document symbols");

        find_symbol_recursive(&symbols, symbol_name).ok_or_else(|| LspError::SymbolNotFound {
            symbol: symbol_name.to_string(),
            path: path.to_string(),
        })
    }

    /// Validate that a symbol exists at the specified location.
    ///
    /// Returns true if a symbol with the given name exists and overlaps
    /// with the specified line range.
    pub fn validate_symbol(
        &self,
        path: &str,
        symbol_name: &str,
        start_line: u32,
        end_line: u32,
    ) -> Result<bool, String> {
        let symbols = self.get_document_symbols(path)?;

        fn check_symbols(symbols: &[LspSymbol], name: &str, start: u32, end: u32) -> bool {
            for sym in symbols {
                // Check if name matches (could be exact or partial match)
                let name_matches =
                    sym.name == name || sym.name.contains(name) || name.contains(&sym.name);

                // Check if lines overlap
                let lines_overlap = sym.start_line <= end && sym.end_line >= start;

                if name_matches && lines_overlap {
                    return true;
                }

                // Check children
                if check_symbols(&sym.children, name, start, end) {
                    return true;
                }
            }
            false
        }

        Ok(check_symbols(&symbols, symbol_name, start_line, end_line))
    }

    /// Get all symbols as a flat list (no nesting).
    pub fn get_flat_symbols(&self, path: &str) -> Result<Vec<LspSymbol>, String> {
        let symbols = self.get_document_symbols(path)?;
        let mut flat = Vec::new();
        flatten_symbols(&symbols, &mut flat);
        Ok(flat)
    }
}

/// Implementation of get_document_symbols using raw NvimClient.
fn get_document_symbols_impl(
    client: &mut NvimClient,
    path: &str,
) -> Result<Vec<LspSymbol>, String> {
    let escaped_path = path.replace('\\', "\\\\").replace('"', "\\\"");

    let lua_code = format!(
        r#"
        local path = "{}"

        -- Make path absolute if relative
        if not vim.startswith(path, '/') then
            path = vim.fn.getcwd() .. '/' .. path
        end

        -- Find or create the buffer
        local bufnr = vim.fn.bufnr(path)
        if bufnr == -1 then
            bufnr = vim.fn.bufadd(path)
        end

        if bufnr == -1 then
            return vim.json.encode({{ error = "Could not open buffer for: " .. path }})
        end

        -- Load the buffer content (triggers filetype detection and LSP attachment)
        if not vim.api.nvim_buf_is_loaded(bufnr) then
            vim.fn.bufload(bufnr)
            -- Trigger filetype detection for LSP
            vim.api.nvim_buf_call(bufnr, function()
                vim.cmd('filetype detect')
            end)
            -- Give LSP a moment to attach
            vim.wait(100, function()
                return #vim.lsp.get_clients({{ bufnr = bufnr }}) > 0
            end, 10)
        end

        -- Get LSP clients for this buffer
        local clients = vim.lsp.get_clients({{ bufnr = bufnr }})
        if #clients == 0 then
            return vim.json.encode({{ error = "No LSP client attached to buffer" }})
        end

        -- Request document symbols synchronously
        local params = {{ textDocument = vim.lsp.util.make_text_document_params(bufnr) }}
        local result = vim.lsp.buf_request_sync(bufnr, 'textDocument/documentSymbol', params, 5000)

        if not result then
            return vim.json.encode({{ error = "LSP request timed out" }})
        end

        -- Flatten and convert symbols
        local function convert_symbol(sym)
            local range = sym.range or sym.location and sym.location.range
            if not range then return nil end

            local symbol = {{
                name = sym.name,
                kind = sym.kind,
                start_line = range.start.line + 1,
                end_line = range["end"].line + 1,
                start_col = range.start.character,
                end_col = range["end"].character,
                container = sym.containerName,
                children = {{}}
            }}

            if sym.children then
                for _, child in ipairs(sym.children) do
                    local converted = convert_symbol(child)
                    if converted then
                        table.insert(symbol.children, converted)
                    end
                end
            end

            return symbol
        end

        local symbols = {{}}
        for _, client_result in pairs(result) do
            if client_result.result then
                for _, sym in ipairs(client_result.result) do
                    local converted = convert_symbol(sym)
                    if converted then
                        table.insert(symbols, converted)
                    end
                end
                break  -- Use first client's result
            end
        end

        return vim.json.encode({{ symbols = symbols }})
        "#,
        escaped_path
    );

    let result = client.execute_lua(&lua_code)?;

    // Parse JSON response
    let json_str = match result {
        rmpv::Value::String(s) => s.into_str().unwrap_or_default(),
        _ => return Err("Unexpected response type".to_string()),
    };

    let response: serde_json::Value =
        serde_json::from_str(&json_str).map_err(|e| format!("Failed to parse JSON: {}", e))?;

    if let Some(error) = response.get("error") {
        return Err(error.as_str().unwrap_or("Unknown error").to_string());
    }

    let symbols: Vec<LspSymbol> = response
        .get("symbols")
        .and_then(|s| serde_json::from_value(s.clone()).ok())
        .unwrap_or_default();

    Ok(symbols)
}

/// Recursively find a symbol by name.
fn find_symbol_recursive(symbols: &[LspSymbol], name: &str) -> Option<LspSymbol> {
    for sym in symbols {
        if sym.name == name {
            return Some(sym.clone());
        }
        if let Some(found) = find_symbol_recursive(&sym.children, name) {
            return Some(found);
        }
    }
    None
}

/// Flatten nested symbols into a single list.
fn flatten_symbols(symbols: &[LspSymbol], out: &mut Vec<LspSymbol>) {
    for sym in symbols {
        let mut flat_sym = sym.clone();
        flat_sym.children = Vec::new();
        out.push(flat_sym);
        flatten_symbols(&sym.children, out);
    }
}

/// LSP Symbol kinds (from LSP spec).
#[allow(dead_code)]
pub mod symbol_kind {
    pub const FILE: i32 = 1;
    pub const MODULE: i32 = 2;
    pub const NAMESPACE: i32 = 3;
    pub const PACKAGE: i32 = 4;
    pub const CLASS: i32 = 5;
    pub const METHOD: i32 = 6;
    pub const PROPERTY: i32 = 7;
    pub const FIELD: i32 = 8;
    pub const CONSTRUCTOR: i32 = 9;
    pub const ENUM: i32 = 10;
    pub const INTERFACE: i32 = 11;
    pub const FUNCTION: i32 = 12;
    pub const VARIABLE: i32 = 13;
    pub const CONSTANT: i32 = 14;
    pub const STRING: i32 = 15;
    pub const NUMBER: i32 = 16;
    pub const BOOLEAN: i32 = 17;
    pub const ARRAY: i32 = 18;
    pub const OBJECT: i32 = 19;
    pub const KEY: i32 = 20;
    pub const NULL: i32 = 21;
    pub const ENUM_MEMBER: i32 = 22;
    pub const STRUCT: i32 = 23;
    pub const EVENT: i32 = 24;
    pub const OPERATOR: i32 = 25;
    pub const TYPE_PARAMETER: i32 = 26;
}
