//! MCP server implementation for Gnapsis.

use std::sync::Arc;

use rmcp::{
    handler::server::{router::tool::ToolRouter, ServerHandler},
    model::{Implementation, ProtocolVersion, ServerCapabilities, ServerInfo},
    tool_handler,
};

use crate::context::Context;
use crate::di::FromRef;

/// Gnapsis MCP Server for code intelligence graph management.
///
/// This server provides AI assistants with tools to:
/// - Initialize and manage the graph database schema
/// - Create and query entities in the knowledge graph
/// - Manage classifications and relationships
/// - Sync with git changes
#[derive(Clone)]
pub struct McpServer {
    pub(crate) ctx: Arc<Context>,
    tool_router: ToolRouter<McpServer>,
}

impl McpServer {
    /// Create a new Gnapsis MCP server with the given context.
    pub fn new(ctx: Context) -> Self {
        tracing::info!("Initializing Gnapsis MCP server");

        Self {
            ctx: Arc::new(ctx),
            tool_router: Self::tool_router(),
        }
    }

    /// Build the combined tool router from all tool modules.
    fn tool_router() -> ToolRouter<Self> {
        Self::project_tools()
            + Self::taxonomy_tools()
            + Self::entity_tools()
            + Self::query_tools()
            + Self::reference_tools()
            + Self::sync_tools()
            + Self::validation_tools()
            + Self::analysis_tools()
    }

    /// Resolve a dependency from the context.
    ///
    /// This enables compile-time dependency injection for repositories
    /// and other services that implement `FromRef<Context>`.
    pub fn resolve<T: FromRef<Context>>(&self) -> T {
        T::from_ref(&self.ctx)
    }

    /// Get direct access to the context.
    pub fn context(&self) -> &Context {
        &self.ctx
    }
}

// ============================================================================
// Server Handler
// ============================================================================

#[tool_handler]
impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                r#"Gnapsis - Code Intelligence Graph MCP Server

A knowledge graph for understanding codebases through semantic relationships.

## Quick Start

1. **init_project** - Initialize schema (run once)
2. **project_overview** - Get current ontology state
3. **analyze_document** - Discover symbols in a file
4. **create_entity** - Add entities with code references

## Key Workflows

### Building the Ontology
```
analyze_document(path) → see untracked LSP symbols
create_entity(name, description, category_ids, parent_ids, commands=[Add...])
```

For code files, use `ref_type: "code"` with `lsp_symbol` (e.g., "McpServer", "impl Foo::bar").
For docs/markdown, use `ref_type: "text"` with `start_line`, `end_line`, optional `anchor`.

### Querying Knowledge
```
search(query) → find entities/references by meaning
query(entity_id) → extract relevant subgraph with context budget
get_entity(id) → full entity details with references
```

### Maintenance
```
get_changed_files() → find modified files
analyze_document(path) → check staleness (is_stale: true)
alter_references([Update...]) → fix line numbers (auto-updates commit SHA)
validate_graph() → check integrity (orphans, cycles, missing refs)
```

## Scopes (hierarchy)

Domain(1) → Feature(2) → Namespace(3) → Component(4) → Unit(5)

- **Domain**: Business domains (e.g., "Authentication")
- **Feature**: Capabilities (e.g., "JWT Validation")
- **Namespace**: Code modules (e.g., "services", "mcp::tools")
- **Component**: Structs, classes, traits
- **Unit**: Functions, methods, constants

**BELONGS_TO rules** (enforced at creation and update time):
- Child scope must be **deeper** than parent (e.g., Feature under Domain, Component under Namespace)
- **Namespace** and **Component** allow same-scope nesting (e.g., namespace under namespace)
- Domain, Feature, and Unit do **not** allow same-scope nesting
- Non-Domain entities **require** at least one parent

## Entity Commands

Use in create_entity/update_entity `commands` array:
- `Add` - Create and attach a new reference
- `Attach` - Attach existing reference by ID
- `Relate` - Create RELATED_TO with optional note
- `Link` - Code links (CALLS, IMPORTS, IMPLEMENTS, INSTANTIATES)

## IMPORTANT: Do NOT

- **Do NOT use text references for code files** - Use `ref_type: "code"` with `lsp_symbol` (unless no LSP available)
- **Do NOT guess lsp_symbol names** - Run `analyze_document` first to see exact symbol names
- **Do NOT create entities without running analyze_document first** - Get the exact symbols available
- **Do NOT manually specify line numbers for code refs** - Lines are auto-populated from LSP

## Reference Types

**Code references** (for source files):
```json
{"type": "add", "ref_type": "code", "document_path": "src/foo.rs", "lsp_symbol": "McpServer", "description": "..."}
```
The `lsp_symbol` must match exactly what `analyze_document` returns in `untracked[].name`.

**Text references** (for markdown, docs, config):
```json
{"type": "add", "ref_type": "text", "document_path": "README.md", "start_line": 1, "end_line": 30, "description": "..."}
```

## Tools Reference

**Project**: init_project, project_overview
**Taxonomy**: create_category
**Entity**: create_entity, update_entity, delete_entity
**Reference**: alter_references
**Query**: get_entity, find_entities, get_document_entities, search, query
**Sync**: get_changed_files
**Analysis**: analyze_document
**Validation**: validate_graph, lsp_refresh
"#
                .to_string(),
            ),
        }
    }
}
