---
title: "DES-003: Gnapsis - Code Intelligence Graph"
date: 2025-01-16
status: draft
iteration: 1
author: nvim-mcp Team
tags: [design, implementation, neo4j, mcp, gnapsis, embeddings]
priority: high
epic: EPIC-001
related_adrs: []
related_designs: [DES-001, DES-002]
changelog:
  - 2025-01-16 (v1): Initial draft - implementation plan
  - 2025-01-16 (v2): Added schema migration strategy (additive-only, forward-only)
  - 2025-01-16 (v3): Renamed from "ontology" to "cortex"
  - 2025-01-16 (v4): Renamed from "cortex" to "gnapsis"
  - 2025-01-17 (v5): Restructured to Epic→Features→Tasks nomenclature, fixed tool count (29), added missing tasks
---

# DES-003: Gnapsis - Code Intelligence Graph

## Overview

Implementation plan for **Gnapsis** - a code intelligence system with 29 MCP tools for managing a knowledge graph in Neo4j. Features include entity classification, semantic search via embeddings, and git-based change tracking. This document covers technical implementation details based on the conceptual framework (DES-001) and API design (DES-002).

**Epic:** [EPIC-001] Gnapsis - Code Intelligence Graph
**Features:** 9 | **Tasks:** 51 | **Tools:** 29

## Goals

1. Implement the Neo4j graph model defined in DES-001 (Taxonomy Tree)
2. Create MCP tool handlers as specified in DES-002 (MCP Tools API)
3. Integrate raggy for configurable embedding generation (fastembed local, OpenAI, etc.)
4. Support git-based change tracking via git2
5. Maintain consistency with existing codebase style and error handling

## Non-Goals

- LSP server implementation (uses existing nvim LSP via tools)
- Real-time file watching (sync triggered manually)
- Multi-database support (single Neo4j instance per project)
- Custom embedding model training

---

## Dependencies

### New Cargo Dependencies

```toml
# Neo4j async driver
neo4rs = "0.8"

# Git operations
git2 = "0.19"

# ULID generation
ulid = "1"

# Embeddings abstraction
raggy = { git = "https://github.com/e7nd7r/raggy" }

# Configuration with layered resolution
figment = { version = "0.10", features = ["toml", "env"] }

# Async utilities
futures = "0.3"

# Lazy initialization
once_cell = "1"

# Error handling (thiserror for domain errors, anyhow for context)
thiserror = "1"
anyhow = "1"
```

### Rationale

| Dependency | Purpose | Alternatives Considered |
|------------|---------|------------------------|
| `neo4rs` | Async Neo4j driver | `neo4j` (sync), `bolt-client` (low-level) |
| `git2` | Git operations | `gitoxide` (newer but less mature) |
| `raggy` | Embedder interface (provider-agnostic) | Direct provider deps (couples business logic) |
| `figment` | Layered config resolution (user/project/env) | `config-rs` (similar), manual resolution |
| `thiserror` | Domain error types (`GnapsisError`) | - |
| `anyhow` | Context wrapping at app boundary | `color_eyre` (heavier, CLI-focused) |

---

## Module Organization

### Workspace Structure

```
gnapsis/
├── Cargo.toml                   # Workspace manifest
├── mcp/                         # Main MCP server crate
│   ├── Cargo.toml               # Depends on di-macros
│   └── src/
│       ├── main.rs              # Entry point
│       ├── config.rs            # Config (Neo4j settings)
│       ├── error.rs             # GnapsisError enum (thiserror)
│       ├── git.rs               # git2 operations
│       ├── context.rs           # Context (DI root)
│       ├── models/              # Domain models
│       │   ├── mod.rs           # Re-exports
│       │   ├── entity.rs        # Entity struct
│       │   ├── category.rs      # Category struct
│       │   ├── scope.rs         # Scope enum
│       │   └── document.rs      # DocumentReference struct
│       ├── repositories/        # Neo4j data access layer
│       │   ├── mod.rs           # Re-exports + shared Neo4j helpers
│       │   ├── entity.rs        # Entity CRUD operations
│       │   ├── category.rs      # Category operations
│       │   ├── document.rs      # DocumentReference operations
│       │   └── schema.rs        # Schema/migration queries
│       ├── migrations/          # Schema migrations (additive-only)
│       │   ├── mod.rs           # Migration trait + runner
│       │   ├── m001_init.rs     # Initial schema
│       │   └── ...              # Future migrations
│       └── tools/               # MCP tool handlers
│           ├── mod.rs           # Re-exports + response helpers
│           ├── project.rs       # init_project, get_project_stats
│           ├── taxonomy.rs      # create_category, list_categories, list_scopes
│           ├── entity.rs        # CRUD, classify, relationships
│           ├── query.rs         # get_entity, find_entities, semantic_search
│           ├── sync.rs          # sync_document, validate_documents
│           └── validation.rs    # validate_graph
└── di-macros/                   # Proc-macro crate for DI
    ├── Cargo.toml               # proc-macro = true
    └── src/
        ├── lib.rs               # Macro exports
        ├── context.rs           # #[derive(Context)]
        └── from_context.rs      # #[derive(FromContext)]
```

### Separation of Concerns

- **Domain logic** (`models/`, `repositories/`): Pure business logic, testable independently
- **MCP layer** (`tools/`, `context.rs`, `dep.rs`): Server setup, DI, and tool handlers
- **DI Macros** (`di-macros/`): `#[derive(Context)]`, `#[derive(FromContext)]`

---

## Core Types

### mcp/src/models/scope.rs

```rust
use serde::{Deserialize, Serialize};

/// Fixed scopes in the compositional hierarchy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Scope {
    Domain = 1,
    Feature = 2,
    Namespace = 3,
    Component = 4,
    Unit = 5,
}

impl Scope {
    pub fn depth(&self) -> u8 {
        *self as u8
    }

    pub fn all() -> &'static [Scope] {
        &[Scope::Domain, Scope::Feature, Scope::Namespace, Scope::Component, Scope::Unit]
    }
}
```

### mcp/src/models/entity.rs

```rust
use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// Entity in the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: String,           // ULID
    pub name: String,
    pub description: String,
    pub embedding: Option<Vec<f32>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Generate a new ULID
pub fn generate_ulid() -> String {
    Ulid::new().to_string()
}
```

### mcp/src/models/category.rs

```rust
use serde::{Deserialize, Serialize};
use crate::models::Scope;

/// Category - classification value at a scope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub id: String,           // ULID
    pub name: String,
    pub scope: Scope,
    pub description: Option<String>,
}
```

### mcp/src/models/document.rs

```rust
use serde::{Deserialize, Serialize};

/// Document reference - pointer to code/text location
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentReference {
    pub id: String,           // ULID
    pub document_path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub offset: Option<u32>,
    pub commit_sha: String,
    pub content_type: ContentType,
    pub description: String,
    pub embedding: Option<Vec<f32>>,
    pub lsp_symbol: Option<String>,
    pub lsp_kind: Option<i32>,
    pub lsp_range: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentType {
    Code(String),  // language identifier (e.g., "rust", "typescript")
    Markdown,
}
```

---

## Configuration

### mcp/src/config.rs

```rust
use figment::{Figment, providers::{Env, Toml, Format}};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub neo4j: Neo4jConfig,
    pub embedding: EmbeddingConfig,
    pub project: ProjectConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Neo4jConfig {
    pub uri: String,
    pub user: String,
    pub password: Option<String>,
    pub database: String,
    pub pool_size: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmbeddingConfig {
    pub provider: String,
    pub model: String,
    pub dimensions: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectConfig {
    pub name: Option<String>,
    pub repo_path: Option<String>,
}

impl Config {
    /// Load config with layered resolution (user → project → env)
    pub fn load() -> Result<Self, figment::Error> {
        Figment::new()
            // Layer 1: User config (lowest priority)
            .merge(Toml::file("~/.config/gnapsis/config.toml"))
            // Layer 2: Project config
            .merge(Toml::file(".gnapsis.toml"))
            // Layer 3: Environment variables (highest priority)
            .merge(Env::prefixed("GNAPSIS_").split("_"))
            .extract()
    }
}
```

### Configuration Layers

| Priority | Source | Example |
|----------|--------|---------|
| 1 (lowest) | User config | `~/.config/gnapsis/config.toml` |
| 2 | Project config | `.gnapsis.toml` |
| 3 (highest) | Environment | `GNAPSIS_NEO4J_URI` |

### Configuration File Example

`.gnapsis.toml` (project-level):

```toml
[neo4j]
uri = "bolt://localhost:7687"
user = "neo4j"
database = "gnapsis"

[embedding]
provider = "fastembed"
model = "BAAI/bge-small-en-v1.5"
dimensions = 384

[project]
name = "my-project"
```

### Environment Variables

```bash
# Override any config value via env
export GNAPSIS_NEO4J_URI="bolt://production:7687"
export GNAPSIS_NEO4J_PASSWORD="secret"
```

---

## Error Handling

### mcp/src/error.rs

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    // Neo4j errors
    #[error("Neo4j connection error: {0}")]
    Connection(#[from] neo4rs::Error),

    #[error("Neo4j query error: {message}")]
    Query { message: String, query: String },

    // Domain errors
    #[error("Entity not found: {0}")]
    EntityNotFound(String),

    #[error("Category not found: {0}")]
    CategoryNotFound(String),

    #[error("Scope not found: {0}")]
    ScopeNotFound(String),

    #[error("Invalid BELONGS_TO: {child} cannot belong to {parent} - {reason}")]
    InvalidBelongsTo {
        child: String,
        parent: String,
        reason: String,
    },

    #[error("Entity has children and cannot be deleted: {0}")]
    HasChildren(String),

    // Git errors
    #[error("Git error: {0}")]
    Git(#[from] git2::Error),

    #[error("Repository not found at: {0}")]
    RepoNotFound(String),

    // Embedding errors
    #[error("Embedding generation failed: {0}")]
    Embedding(String),

    // Config errors
    #[error("Configuration error: {0}")]
    Config(#[from] figment::Error),

    #[error("Project not initialized. Run init_project first.")]
    NotInitialized,
}
```

### MCP Error Conversion

```rust
use rmcp::model::ErrorCode;

impl From<AppError> for rmcp::ErrorData {
    fn from(err: AppError) -> Self {
        let (code, app_code) = match &err {
            AppError::EntityNotFound(_) => (ErrorCode::RESOURCE_NOT_FOUND, "ENTITY_NOT_FOUND"),
            AppError::CategoryNotFound(_) => (ErrorCode::RESOURCE_NOT_FOUND, "CATEGORY_NOT_FOUND"),
            AppError::ScopeNotFound(_) => (ErrorCode::RESOURCE_NOT_FOUND, "SCOPE_NOT_FOUND"),
            AppError::InvalidBelongsTo { .. } => (ErrorCode::INVALID_PARAMS, "INVALID_BELONGS_TO"),
            AppError::HasChildren(_) => (ErrorCode::INVALID_PARAMS, "HAS_CHILDREN"),
            AppError::NotInitialized => (ErrorCode::INVALID_REQUEST, "NOT_INITIALIZED"),
            AppError::Config(_) => (ErrorCode::INTERNAL_ERROR, "CONFIG_ERROR"),
            AppError::Connection(_) => (ErrorCode::INTERNAL_ERROR, "CONNECTION_ERROR"),
            _ => (ErrorCode::INTERNAL_ERROR, "INTERNAL_ERROR"),
        };

        rmcp::ErrorData::new(code, format!("[{}] {}", app_code, err), None)
    }
}
```

---

## Dependency Injection

### Overview

Gnapsis uses a compile-time DI pattern inspired by axum's `FromRef`. Dependencies are resolved recursively from a root `Context` struct using the `FromRef` trait and `Dep<T>` extractor.

### FromRef Trait

```rust
// di-macros re-exports this trait
pub trait FromRef<T> {
    fn from_ref(input: &T) -> Self;
}

// Blanket impl: anything Clone can be extracted from itself
impl<T: Clone> FromRef<T> for T {
    fn from_ref(input: &T) -> Self {
        input.clone()
    }
}
```

### Context (DI Root)

```rust
// src/context.rs
use di_macros::Context;
use neo4rs::Graph;
use raggy::Embedder;

/// Root context - #[derive(Context)] generates FromRef for each field
#[derive(Context, Clone)]
pub struct Context {
    pub graph: Graph,
    pub config: Config,
    pub embedder: Embedder,  // abstract interface, provider-agnostic
}

// Generated by #[derive(Context)]:
// impl FromRef<Context> for Graph { ... }
// impl FromRef<Context> for Config { ... }
// impl FromRef<Context> for Embedder { ... }
```

### FromContext Repositories

```rust
// src/repositories/entity.rs
use di_macros::FromContext;
use neo4rs::Graph;

/// #[derive(FromContext)] generates FromRef<Context> by resolving fields
#[derive(FromContext, Clone)]
pub struct EntityRepository {
    graph: Graph,  // resolved via Graph::from_ref(ctx)
}

impl EntityRepository {
    pub async fn create(&self, entity: &Entity) -> Result<Entity, GnapsisError> {
        // use self.graph
    }

    pub async fn find_by_id(&self, id: &str) -> Result<Option<Entity>, GnapsisError> {
        // use self.graph
    }
}

// Generated by #[derive(FromContext)]:
// impl FromRef<Context> for EntityRepository {
//     fn from_ref(ctx: &Context) -> Self {
//         Self { graph: Graph::from_ref(ctx) }
//     }
// }
```

### MCP Server

```rust
// mcp/src/mcp/mod.rs
pub struct McpServer {
    ctx: Arc<Context>,
    tool_router: ToolRouter<McpServer>,
}

impl McpServer {
    pub fn new(ctx: Context) -> Self {
        Self {
            ctx: Arc::new(ctx),
            tool_router: Self::tool_router(),
        }
    }

    /// Resolve a dependency from the context
    pub fn resolve<T: FromRef<Context>>(&self) -> T {
        T::from_ref(&self.ctx)
    }
}
```

### Tool Handler Example

```rust
// mcp/src/tools/entity.rs
use crate::tools::{Response, PaginatedResponse, Pagination};

#[tool_router]
impl McpServer {
    #[tool(description = "Create a new entity in the knowledge graph")]
    pub async fn create_entity(
        &self,
        params: Params<CreateEntityParams>,
    ) -> Result<CallToolResult, McpError> {
        let repo = self.resolve::<EntityRepository>();

        let entity = Entity {
            id: generate_ulid(),
            name: params.name.clone(),
            description: params.description.clone(),
        };

        let created = repo.create(&entity).await?;

        Response(created).into()
    }

    #[tool(description = "Get an entity by ID")]
    pub async fn get_entity(
        &self,
        params: Params<GetEntityParams>,
    ) -> Result<CallToolResult, McpError> {
        let repo = self.resolve::<EntityRepository>();

        let entity = repo.find_by_id(&params.id).await?
            .ok_or_else(|| AppError::EntityNotFound(params.id.clone()))?;

        Response(entity).into()
    }
}
```

### DI Resolution Chain

```
Context (root)
├── Graph (field, direct FromRef)
│   └── EntityRepository (FromContext, needs Graph)
│   └── CategoryRepository (FromContext, needs Graph)
│   └── DocumentRepository (FromContext, needs Graph)
├── Config (field, direct FromRef)
└── Embedder (field, direct FromRef - provider-agnostic interface)
```

---

## Schema Migrations

Schema changes are managed through versioned migrations. All schema setup, including initial creation, goes through the migration system.

### Principles

1. **Additive-only**: Never delete properties, nodes, relationships, or constraints
2. **Forward-only**: No rollback support - create compensating migrations if needed
3. **Idempotent**: All migrations use `IF NOT EXISTS`, `MERGE`, `COALESCE`
4. **Version-tracked**: Schema version stored in graph metadata node
5. **Auto-applied**: Migrations run automatically on `init_project`

### Why Additive-Only?

| Approach | Pros | Cons |
|----------|------|------|
| **Additive-only** | Safe, simple, no data loss | Accumulates unused fields |
| **With deletions** | Clean schema | Risk of data loss, complex rollbacks |

Unused properties in Neo4j have minimal cost - they're only stored on nodes that have them.

### Migration Module Structure

```
mcp/src/migrations/
├── mod.rs              # Migration trait, runner, version tracking
├── m001_init.rs        # Initial schema (constraints, scopes, categories, indexes)
├── m002_lsp_fields.rs  # Add LSP tracking fields
└── ...                 # Future migrations
```

### Migration Trait

```rust
// mcp/src/migrations/mod.rs

/// A schema migration
pub trait Migration: Send + Sync {
    fn id(&self) -> &'static str;
    fn version(&self) -> u32;
    fn description(&self) -> &'static str;
    async fn up(&self, graph: &Graph) -> Result<(), AppError>;
}

/// Run all pending migrations
pub async fn run_migrations(graph: &Graph) -> Result<MigrationResult, AppError> {
    let current_version = get_schema_version(graph).await?;
    let migrations = all_migrations();

    let mut applied = vec![];
    for migration in migrations {
        if migration.version() > current_version {
            tracing::info!("Applying migration: {}", migration.description());
            migration.up(graph).await?;
            applied.push(migration.id().to_string());
        }
    }

    if !applied.is_empty() {
        update_schema_version(graph, &applied).await?;
    }

    Ok(MigrationResult {
        previous_version: current_version,
        current_version: migrations.last().map(|m| m.version()).unwrap_or(0),
        applied_migrations: applied,
    })
}
```

### Version Tracking

```cypher
(:SchemaVersion {
    version: 2,
    applied_migrations: ["m001_init", "m002_lsp_fields"],
    last_applied_at: datetime()
})
```

### m001_init - Initial Schema

```rust
// mcp/src/migrations/m001_init.rs

pub struct M001Init;

impl Migration for M001Init {
    fn id(&self) -> &'static str { "m001_init" }
    fn version(&self) -> u32 { 1 }
    fn description(&self) -> &'static str { "Initial schema setup" }

    async fn up(&self, graph: &Graph) -> Result<(), AppError> {
        self.create_constraints(graph).await?;
        self.create_scopes(graph).await?;
        self.create_default_categories(graph).await?;
        self.create_vector_indexes(graph).await?;
        Ok(())
    }
}

impl M001Init {
    async fn create_constraints(&self, graph: &Graph) -> Result<(), AppError> {
        let constraints = [
            "CREATE CONSTRAINT entity_id IF NOT EXISTS FOR (e:Entity) REQUIRE e.id IS UNIQUE",
            "CREATE CONSTRAINT category_id IF NOT EXISTS FOR (c:Category) REQUIRE c.id IS UNIQUE",
            "CREATE CONSTRAINT document_id IF NOT EXISTS FOR (d:Document) REQUIRE d.id IS UNIQUE",
            "CREATE CONSTRAINT docref_id IF NOT EXISTS FOR (r:DocumentReference) REQUIRE r.id IS UNIQUE",
            "CREATE CONSTRAINT scope_name IF NOT EXISTS FOR (s:Scope) REQUIRE s.name IS UNIQUE",
        ];

        for constraint in constraints {
            graph.run(query(constraint)).await?;
        }
        Ok(())
    }

    async fn create_scopes(&self, graph: &Graph) -> Result<(), AppError> {
        for scope in Scope::all() {
            graph.run(query(
                "MERGE (s:Scope {name: $name})
                 SET s.depth = $depth, s.description = $description"
            )
            .param("name", format!("{:?}", scope))
            .param("depth", scope.depth() as i64)
            .param("description", scope.description()))
            .await?;
        }

        // Create hierarchy: Domain -> Feature -> Namespace -> Component -> Unit
        graph.run(query(
            "MATCH (domain:Scope {name: 'Domain'})
             MATCH (feature:Scope {name: 'Feature'})
             MATCH (namespace:Scope {name: 'Namespace'})
             MATCH (component:Scope {name: 'Component'})
             MATCH (unit:Scope {name: 'Unit'})
             MERGE (domain)-[:COMPOSES]->(feature)
             MERGE (feature)-[:COMPOSES]->(namespace)
             MERGE (namespace)-[:COMPOSES]->(component)
             MERGE (component)-[:COMPOSES]->(unit)"
        )).await?;

        Ok(())
    }

    async fn create_default_categories(&self, graph: &Graph) -> Result<(), AppError> {
        let categories = [
            // Domain
            ("core", "Domain", "Core business logic"),
            ("infrastructure", "Domain", "Infrastructure and utilities"),
            // Feature
            ("functional", "Feature", "Functional capabilities"),
            ("non-functional", "Feature", "Cross-cutting concerns"),
            ("technical", "Feature", "Technical implementation details"),
            // Namespace
            ("module", "Namespace", "Code module"),
            ("library", "Namespace", "External library"),
            // Component
            ("class", "Component", "Object-oriented class"),
            ("struct", "Component", "Data structure"),
            ("trait", "Component", "Trait/interface definition"),
            ("interface", "Component", "Interface definition"),
            ("enum", "Component", "Enumeration type"),
            // Unit
            ("function", "Unit", "Standalone function"),
            ("method", "Unit", "Class/struct method"),
            ("property", "Unit", "Property accessor"),
            ("field", "Unit", "Data field"),
            ("constant", "Unit", "Constant value"),
        ];

        for (name, scope, description) in categories {
            graph.run(query(
                "MATCH (s:Scope {name: $scope})
                 MERGE (c:Category {name: $name})-[:IN_SCOPE]->(s)
                 SET c.id = coalesce(c.id, $id),
                     c.description = $description,
                     c.created_at = coalesce(c.created_at, datetime())"
            )
            .param("name", name)
            .param("scope", scope)
            .param("description", description)
            .param("id", generate_ulid()))
            .await?;
        }
        Ok(())
    }

    async fn create_vector_indexes(&self, graph: &Graph) -> Result<(), AppError> {
        // These may fail on older Neo4j versions - log warning but don't fail
        let indexes = [
            "CREATE VECTOR INDEX entity_embedding IF NOT EXISTS
             FOR (e:Entity) ON e.embedding
             OPTIONS {indexConfig: {`vector.dimensions`: 384, `vector.similarity_function`: 'cosine'}}",
            "CREATE VECTOR INDEX docref_embedding IF NOT EXISTS
             FOR (r:DocumentReference) ON r.embedding
             OPTIONS {indexConfig: {`vector.dimensions`: 384, `vector.similarity_function`: 'cosine'}}",
        ];

        for index in indexes {
            if let Err(e) = graph.run(query(index)).await {
                tracing::warn!("Could not create vector index: {}", e);
            }
        }
        Ok(())
    }
}
```

### m002_lsp_fields - Example Follow-up Migration

```rust
// mcp/src/migrations/m002_lsp_fields.rs

pub struct M002LspFields;

impl Migration for M002LspFields {
    fn id(&self) -> &'static str { "m002_lsp_fields" }
    fn version(&self) -> u32 { 2 }
    fn description(&self) -> &'static str { "Add LSP tracking fields" }

    async fn up(&self, graph: &Graph) -> Result<(), AppError> {
        graph.run(query(
            "CREATE INDEX docref_lsp_symbol IF NOT EXISTS
             FOR (r:DocumentReference) ON (r.lsp_symbol)"
        )).await?;
        Ok(())
    }
}
```

### Integration with init_project

```rust
pub async fn init_project(graph: &Graph) -> Result<InitResult, AppError> {
    let migration_result = run_migrations(graph).await?;

    Ok(InitResult {
        schema_version: migration_result.current_version,
        migrations_applied: migration_result.applied_migrations,
    })
}
```

### Backup Recommendation

```bash
# Before running migrations in production
neo4j-admin database dump neo4j --to-path=/backup/before-migration.dump
```

---

## Tool Response Helpers

### Response Types

Two response types provide consistent tool output formatting:

- `Response<T>` - Serializes as the raw inner value (no wrapper object)
- `PaginatedResponse<T>` - Serializes with `data` array and `pagination` metadata

### mcp/src/tools/mod.rs

```rust
use rmcp::model::CallToolResult;
use serde::Serialize;
use serde_json::to_value;

pub mod project;
pub mod taxonomy;
pub mod entity;
pub mod query;
pub mod sync;
pub mod validation;

/// Single-item response - serializes as raw inner value
pub struct Response<T>(pub T);

impl<T: Serialize> Serialize for Response<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<T: Serialize> From<Response<T>> for Result<CallToolResult, McpError> {
    fn from(response: Response<T>) -> Self {
        Ok(CallToolResult::structured(to_value(response).unwrap()))
    }
}

/// Paginated response with metadata
#[derive(Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub data: Vec<T>,
    pub pagination: Pagination,
}

#[derive(Serialize)]
pub struct Pagination {
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub has_more: bool,
}

impl<T: Serialize> From<PaginatedResponse<T>> for Result<CallToolResult, McpError> {
    fn from(response: PaginatedResponse<T>) -> Self {
        Ok(CallToolResult::structured(to_value(response).unwrap()))
    }
}
```

### Usage Examples

```rust
// Single item - serializes as raw object: { "id": "...", "name": "..." }
Response(entity).into()

// Paginated - serializes as: { "data": [...], "pagination": {...} }
PaginatedResponse {
    data: entities,
    pagination: Pagination {
        total: 100,
        offset: 0,
        limit: 20,
        has_more: true,
    },
}.into()
```

Errors are returned via `Err(AppError.into())` which converts to MCP protocol errors (see Error Handling section).

---

## Epic Structure

Following the [Card Nomenclature](../../pergamini/docs/platform/project/card-nomenclature.md), this implementation is organized as:

```
[EPIC-001] Gnapsis - Code Intelligence Graph
    │
    ├── [EPIC001-F1] DI Macros Crate
    ├── [EPIC001-F2] Core Infrastructure
    ├── [EPIC001-F3] Neo4j Schema & Repositories
    ├── [EPIC001-F4] MCP Server & Project Tools
    ├── [EPIC001-F5] Taxonomy Tools
    ├── [EPIC001-F6] Entity Management Tools
    ├── [EPIC001-F7] Query & Search Tools
    ├── [EPIC001-F8] Sync Tools
    └── [EPIC001-F9] Validation & LSP Tools

Related designs:
[DESIGN] DES-001: Taxonomy Tree → docs/designs/001-taxonomy-tree.md
[DESIGN] DES-002: MCP Tools API → docs/designs/002-mcp-tools.md
[DESIGN] DES-003: Implementation Plan → docs/designs/003-implementation-plan.md
```

---

## Features & Tasks

### [EPIC001-F1] DI Macros Crate

Compile-time dependency injection infrastructure using proc-macros.

| Task | File | Description |
|------|------|-------------|
| EPIC001-F1-01 | di-macros/Cargo.toml | Create di-macros crate manifest with proc-macro = true |
| EPIC001-F1-02 | di-macros/src/lib.rs | Implement FromRef trait and re-exports |
| EPIC001-F1-03 | di-macros/src/context.rs | Implement #[derive(Context)] proc-macro |
| EPIC001-F1-04 | di-macros/src/from_context.rs | Implement #[derive(FromContext)] proc-macro |

### [EPIC001-F2] Core Infrastructure

Domain models, configuration, and error handling.

| Task | File | Description |
|------|------|-------------|
| EPIC001-F2-01 | mcp/src/config.rs | Implement Config with layered resolution (figment) |
| EPIC001-F2-02 | mcp/src/error.rs | Implement AppError enum with MCP ErrorData conversion |
| EPIC001-F2-03 | mcp/src/models/scope.rs | Implement Scope enum (Domain→Feature→Namespace→Component→Unit) |
| EPIC001-F2-04 | mcp/src/models/entity.rs | Implement Entity model with ULID generation |
| EPIC001-F2-05 | mcp/src/models/category.rs | Implement Category model |
| EPIC001-F2-06 | mcp/src/models/document.rs | Implement Document model (path, content_hash) |
| EPIC001-F2-07 | mcp/src/models/document.rs | Implement DocumentReference model (with LSP fields) |
| EPIC001-F2-08 | mcp/src/context.rs | Implement Context struct (DI root with Graph, Config, Embedder) |
| EPIC001-F2-09 | mcp/src/tools/mod.rs | Implement Response and PaginatedResponse helpers |

### [EPIC001-F3] Neo4j Schema & Repositories

Database schema, migrations, APOC triggers, and data access layer.

| Task | File | Description |
|------|------|-------------|
| EPIC001-F3-01 | mcp/src/migrations/mod.rs | Implement Migration trait, runner, and version tracking |
| EPIC001-F3-02 | mcp/src/migrations/m001_init.rs | Implement m001_init (constraints, scopes, categories, vector indexes) |
| EPIC001-F3-03 | mcp/src/migrations/m001_init.rs | Implement APOC triggers (validate_belongs_to, single_category_per_scope, prevent_delete_with_children, cascade_delete) |
| EPIC001-F3-04 | mcp/src/repositories/entity.rs | Implement EntityRepository (CRUD, relationships) |
| EPIC001-F3-05 | mcp/src/repositories/category.rs | Implement CategoryRepository (create, list, by scope) |
| EPIC001-F3-06 | mcp/src/repositories/document.rs | Implement DocumentRepository (Document + DocumentReference operations) |
| EPIC001-F3-07 | mcp/src/repositories/schema.rs | Implement SchemaRepository (migration queries, schema version) |

### [EPIC001-F4] MCP Server & Project Tools

MCP server setup and project initialization tools.

| Task | File | Description |
|------|------|-------------|
| EPIC001-F4-01 | mcp/src/main.rs | Implement McpServer with tool router and DI resolution |
| EPIC001-F4-02 | mcp/src/tools/project.rs | Implement init_project tool (migrations, seed data) |
| EPIC001-F4-03 | mcp/src/tools/project.rs | Implement get_project_stats tool (counts, relationships) |

### [EPIC001-F5] Taxonomy Tools

Tools for managing scopes and categories.

| Task | File | Description |
|------|------|-------------|
| EPIC001-F5-01 | mcp/src/tools/taxonomy.rs | Implement list_scopes tool (hierarchy with COMPOSES) |
| EPIC001-F5-02 | mcp/src/tools/taxonomy.rs | Implement list_categories tool (by scope, all) |
| EPIC001-F5-03 | mcp/src/tools/taxonomy.rs | Implement create_category tool |

### [EPIC001-F6] Entity Management Tools

Tools for creating, updating, and managing entity relationships.

| Task | File | Description |
|------|------|-------------|
| EPIC001-F6-01 | mcp/src/tools/entity.rs | Implement create_entity tool (with auto-embedding) |
| EPIC001-F6-02 | mcp/src/tools/entity.rs | Implement update_entity tool (re-embed on description change) |
| EPIC001-F6-03 | mcp/src/tools/entity.rs | Implement delete_entity tool (with children check) |
| EPIC001-F6-04 | mcp/src/tools/entity.rs | Implement classify tool (entity → categories) |
| EPIC001-F6-05 | mcp/src/tools/entity.rs | Implement unclassify tool |
| EPIC001-F6-06 | mcp/src/tools/entity.rs | Implement add_belongs tool (composition with scope validation) |
| EPIC001-F6-07 | mcp/src/tools/entity.rs | Implement add_related tool (RELATED_TO relationships) |
| EPIC001-F6-08 | mcp/src/tools/entity.rs | Implement add_link tool (CALLS, IMPORTS, IMPLEMENTS, INSTANTIATES) |
| EPIC001-F6-09 | mcp/src/tools/entity.rs | Implement add_references tool (DocumentReference with LSP fields) |
| EPIC001-F6-10 | mcp/src/tools/entity.rs | Implement remove_references tool |

### [EPIC001-F7] Query & Search Tools

Tools for querying and searching the knowledge graph.

| Task | File | Description |
|------|------|-------------|
| EPIC001-F7-01 | mcp/src/tools/query.rs | Implement get_entity tool (with classifications, references, hierarchy) |
| EPIC001-F7-02 | mcp/src/tools/query.rs | Implement find_entities tool (by classification criteria) |
| EPIC001-F7-03 | mcp/src/tools/query.rs | Implement get_document_entities tool (entities in a document) |
| EPIC001-F7-04 | mcp/src/tools/query.rs | Implement get_composition_graph tool (ancestors, descendants, siblings) |
| EPIC001-F7-05 | mcp/src/tools/query.rs | Implement query_subgraph tool (N-hop traversal with semantic filter) |
| EPIC001-F7-06 | mcp/src/tools/query.rs | Implement search_documents tool (semantic search on DocumentReference) |
| EPIC001-F7-07 | mcp/src/tools/query.rs | Implement semantic_search tool (search entities by embedding) |

### [EPIC001-F8] Sync Tools

Tools for synchronizing the graph with git changes.

| Task | File | Description |
|------|------|-------------|
| EPIC001-F8-01 | mcp/src/git.rs | Implement git operations module (diff, content at commit, HEAD sha) |
| EPIC001-F8-02 | mcp/src/tools/sync.rs | Implement sync_document tool (parse and sync entities) |
| EPIC001-F8-03 | mcp/src/tools/sync.rs | Implement sync_changes tool (sync between commits) |
| EPIC001-F8-04 | mcp/src/tools/sync.rs | Implement sync_references tool (update line numbers after LLM review) |
| EPIC001-F8-05 | mcp/src/tools/sync.rs | Implement validate_documents tool (find stale references with diffs) |

### [EPIC001-F9] Validation & LSP Tools

Tools for graph validation and LSP integration.

| Task | File | Description |
|------|------|-------------|
| EPIC001-F9-01 | mcp/src/tools/validation.rs | Implement validate_graph tool (orphans, cycles, scope violations) |
| EPIC001-F9-02 | mcp/src/tools/validation.rs | Implement lsp_analyze tool (find untracked LSP symbols) |
| EPIC001-F9-03 | mcp/src/tools/validation.rs | Implement lsp_refresh tool (update references via LSP lookup) |

---

## Task Summary

| Feature | Tasks | Tools |
|---------|-------|-------|
| [EPIC001-F1] DI Macros Crate | 4 | - |
| [EPIC001-F2] Core Infrastructure | 9 | - |
| [EPIC001-F3] Neo4j Schema & Repositories | 7 | - |
| [EPIC001-F4] MCP Server & Project Tools | 3 | 2 |
| [EPIC001-F5] Taxonomy Tools | 3 | 3 |
| [EPIC001-F6] Entity Management Tools | 10 | 10 |
| [EPIC001-F7] Query & Search Tools | 7 | 7 |
| [EPIC001-F8] Sync Tools | 5 | 4 |
| [EPIC001-F9] Validation & LSP Tools | 3 | 3 |
| **Total** | **51** | **29** |

---

## Verification

### Phase 1 Testing

1. **Build**: `cargo build` - verify no compilation errors
2. **Unit tests**: `cargo test` - run cortex module tests
3. **Manual test**:
   - Start Neo4j: `docker run -p 7687:7687 -e NEO4J_AUTH=neo4j/password neo4j:5-community`
   - Set `NEO4J_PASSWORD=password`
   - Run MCP server
   - Call `init_project` tool
   - Call `list_scopes` - verify 5 scopes returned
   - Call `create_entity` - verify entity created with embedding
   - Call `get_entity` - verify entity retrieved

### Integration Testing

```rust
#[cfg(test)]
mod tests {
    use testcontainers::{clients::Cli, GenericImage};

    #[tokio::test]
    async fn test_schema_initialization() {
        let docker = Cli::default();
        let neo4j = docker.run(
            GenericImage::new("neo4j", "5.15-community")
                .with_env_var("NEO4J_AUTH", "neo4j/test")
                .with_exposed_port(7687),
        );

        let port = neo4j.get_host_port_ipv4(7687);
        // ... test initialization
    }
}
```

---

## References

- [DES-001: Taxonomy Tree](./001-taxonomy-tree.md) - Conceptual framework
- [DES-002: MCP Tools API](./002-mcp-tools.md) - Tool specifications
- [neo4rs documentation](https://docs.rs/neo4rs)
- [raggy documentation](https://github.com/e7nd7r/raggy) - Embedding abstraction layer
