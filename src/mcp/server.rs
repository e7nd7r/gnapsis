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
/// - Initialize and manage the Neo4j graph schema
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
        Self::project_tools() + Self::taxonomy_tools() + Self::entity_tools() + Self::query_tools()
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

## Getting Started

1. **init_project** - Initialize the database schema (run once)
2. **get_project_stats** - View entity counts and schema version

## Concepts

- **Entities**: Named concepts in your codebase (modules, structs, functions)
- **Categories**: Classification values at each scope level
- **Scopes**: Hierarchy levels (Domain → Feature → Namespace → Component → Unit)
- **DocumentReferences**: Pointers to code locations with embeddings

## Taxonomy Tools

- **list_scopes** - List scope hierarchy (Domain, Feature, Namespace, Component, Unit)
- **list_categories** - List categories by scope
- **create_category** - Create new category at a scope

## Entity Tools

- **create_entity** - Create entity with auto-embedding
- **update_entity** - Update entity (re-embeds on description change)
- **delete_entity** - Delete entity (must have no children)
- **classify** - Classify entity with categories
- **unclassify** - Remove classifications from entity
- **add_belongs** - Add composition (child BELONGS_TO parents)
- **add_related** - Add RELATED_TO relationships
- **add_link** - Add code links (CALLS, IMPORTS, IMPLEMENTS, INSTANTIATES)
- **add_references** - Add document references with auto-embedding
- **remove_references** - Remove document references

## Query Tools

- **get_entity** - Get entity with full context (classifications, references, hierarchy)
- **find_entities** - Find entities by scope, category, or parent
- **get_document_entities** - Get all entities in a document
- **get_composition_graph** - Get ancestors and descendants via BELONGS_TO
- **query_subgraph** - Extract subgraph within N hops
- **search_documents** - Semantic search on document references
- **semantic_search** - Semantic search on entities

## Coming Soon

- Git sync tools
"#
                .to_string(),
            ),
        }
    }
}
