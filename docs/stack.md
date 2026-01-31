# Gnapsis - Technology Stack & Architecture Overview

## Project Identity

- **Name**: Gnapsis
- **Description**: Code intelligence graph - MCP server for semantic codebase understanding
- **Repository**: https://github.com/e7nd7r/gnapsis
- **Version**: 0.1.0

## Primary Language

- **Rust** (Edition 2021, requires 1.75+)
- Workspace with 2 members: main crate (`gnapsis`) and proc-macro crate (`di-macros`)

## Architectural Style

**Layered Architecture** with Repository Pattern, Command Pattern, and compile-time Dependency Injection:

```
CLI Layer (clap)
    |
MCP Server Layer (rmcp - stdio transport)
    |
Tool Layer (MCP tool implementations, organized by domain)
    |
Service Layer (business logic, command execution, validation)
    |
Repository Layer (data access via Cypher queries)
    |
Graph Abstraction Layer (trait-based backend abstraction)
    |
PostgreSQL + Apache AGE Backend (connection pooling, agtype serialization)
```

## Core Framework: MCP Server

- **rmcp** (v0.9) - Model Context Protocol SDK for Rust (server mode with macros, stdio transport)
- The application exposes tools to AI assistants via the MCP protocol
- Tools are organized into 8 domains: project, taxonomy, entity, query, reference, sync, validation, analysis

## Dependencies by Function

### CLI & Configuration
| Dependency | Version | Purpose |
|-----------|---------|---------|
| clap | 4 | CLI argument parsing with derive macros |
| figment | 0.10 | Layered config resolution (TOML + ENV) |
| dirs | 5 | XDG/platform config directory resolution |

### Database & Graph
| Dependency | Version | Purpose |
|-----------|---------|---------|
| deadpool-postgres | 0.14 | PostgreSQL connection pooling |
| tokio-postgres | 0.7 | Async PostgreSQL driver |
| bytes | 1 | Binary serialization for AGE agtype |
| pest / pest_derive | 2 | PEG parser for Cypher RETURN clause extraction |

### Async Runtime
| Dependency | Version | Purpose |
|-----------|---------|---------|
| tokio | 1 (full) | Async runtime |
| futures | 0.3 | Stream utilities |
| async-stream | 0.3 | Async stream construction |
| async-trait | 0.1 | Async trait support |

### Serialization & Output
| Dependency | Version | Purpose |
|-----------|---------|---------|
| serde / serde_json | 1 | JSON serialization |
| serde_toon | 0.2 | TOON format (40-60% fewer tokens than JSON) |
| rmpv | 1 | Msgpack for Neovim RPC |

### Embeddings & Search
| Dependency | Version | Purpose |
|-----------|---------|---------|
| raggy (git) | - | Embeddings abstraction with FastEmbed backend |

### Version Control
| Dependency | Version | Purpose |
|-----------|---------|---------|
| git2 | 0.19 | Git operations (HEAD SHA, diffs, changed files) |

### Error Handling & Logging
| Dependency | Version | Purpose |
|-----------|---------|---------|
| color-eyre | 0.6 | Enhanced error reporting |
| anyhow | 1.0 | Flexible error handling |
| thiserror | 1 | Derive macro for error types |
| tracing | 0.1 | Structured logging |
| tracing-subscriber | 0.3 | Log output (file + stderr) |

### Utilities
| Dependency | Version | Purpose |
|-----------|---------|---------|
| ulid | 1 | Unique ID generation |
| chrono | 0.4 | Date/time with serde |
| once_cell | 1 | Lazy static initialization |
| tempfile | 3 | Temporary files |

### Visualization
| Dependency | Version | Purpose |
|-----------|---------|---------|
| bevy | 0.15 | 3D ECS engine for graph visualization |

### Proc-Macro Crate (di-macros)
| Dependency | Version | Purpose |
|-----------|---------|---------|
| syn | 2 | Rust syntax parsing |
| quote | 1 | Rust code generation |
| proc-macro2 | 1 | Proc macro utilities |

### Dev Dependencies
| Dependency | Version | Purpose |
|-----------|---------|---------|
| serial_test | 3.3.1 | Serial test execution |

## Persistence Layer

- **PostgreSQL** (via Docker, PG17) with:
  - **Apache AGE 1.6.0** - Graph database extension providing Cypher query language
  - **pgvector 0.8.0** - Vector similarity search for semantic embeddings
- Two-tier migration system:
  - **DB migrations** (global, per-database) - extensions, tables, tracked in `db_schema_version`
  - **Graph migrations** (per-graph) - seed data, graph-specific schema, tracked as `:SchemaVersion` node
- Graph naming convention: `gnapsis_<project_name>`
- Embedding model: BAAI/bge-small-en-v1.5 (384 dimensions)

## External Service Dependencies

- **Neovim** (optional) - LSP integration for document symbol resolution via Unix socket + msgpack-RPC
- **Git** (required) - Repository state tracking for staleness detection

## Infrastructure & Deployment

- **Docker Compose** - PostgreSQL+AGE container with healthcheck
- **GitHub Actions** - CI/CD workflows
- **just** (task runner) - Development commands (db-up, build, test, lint, etc.)
- **Cargo features**: `integration` flag for database-dependent tests

## Key Architectural Patterns

1. **Repository Pattern** - Data access abstracted behind repository structs (EntityRepository, DocumentRepository, QueryRepository, etc.)
2. **Command Pattern** - Entity operations encapsulated as EntityCommand variants, executed sequentially by CommandService
3. **Compile-time Dependency Injection** - `FromRef` trait + `#[derive(Context)]` + `#[derive(FromContext)]` macros for zero-cost DI
4. **Graph Abstraction** - Trait hierarchy (CypherExecutor, SqlExecutor, Transaction, GraphClient) enabling backend-agnostic code
5. **Query Builder** - Fluent API for Cypher query construction with type-safe parameter binding
6. **Layered Configuration** - User config -> Project config -> Environment variables (figment)
7. **Multi-source Projects** - A single graph spans multiple source directories
8. **Two-tier Migrations** - Separate database-level and graph-level migration tracks
9. **ECS Pattern** (visualization) - Bevy's Entity-Component-System for 3D graph rendering

## Module Structure

```
src/
  main.rs          - Entry point, logging setup
  lib.rs           - Module declarations
  config.rs        - Layered configuration (figment)
  context.rs       - DI root (AppGraph, AppEmbedder, Config, Nvim)
  di.rs            - FromRef trait + re-exports of derive macros
  error.rs         - AppError enum with MCP protocol conversion
  git.rs           - Git operations (HEAD, diffs, changed files)
  cli/             - CLI subcommands (init, mcp, visualize)
  mcp/             - MCP server + tool implementations
    server.rs      - McpServer struct, tool router composition
    protocol.rs    - Response helpers (JSON/TOON, pagination)
    tools/         - Tool modules (analysis, entity, project, query, reference, sync, taxonomy, validation)
  services/        - Business logic layer
    entity.rs      - Entity CRUD with validation
    commands.rs    - Command pattern execution engine
    graph.rs       - Semantic search, subgraph extraction (BFS)
    lsp.rs         - LSP integration via Neovim
    validation.rs  - Graph integrity checks
  repositories/    - Data access layer
    entity.rs      - Entity CRUD via Cypher
    document.rs    - Reference management (code + text)
    query.rs       - Graph traversal, search, subgraph extraction
    schema.rs      - Schema version, stats
    category.rs    - Category CRUD
  models/          - Domain models
    entity.rs      - Entity struct
    document.rs    - CodeReference, TextReference, Reference enum
    graph.rs       - Query result models (EntityWithContext, QueryGraph, etc.)
    scope.rs       - Scope enum (Domain->Feature->Namespace->Component->Unit)
    category.rs    - Category struct
  graph/           - Graph abstraction layer
    traits.rs      - CypherExecutor, SqlExecutor, Transaction, GraphClient
    mod.rs         - Graph<C> wrapper struct
    query.rs       - Query builder + QueryExt trait
    row.rs         - Row, Node, Relation, Path, Params types
    macros.rs      - cypher! macro
    cypher.rs      - PEG parser for Cypher RETURN clauses
    cypher.pest    - PEG grammar
    backends/
      postgres.rs  - PostgreSQL+AGE implementation
  migrations/      - Two-tier migration system
    traits.rs      - Migration traits
    runner.rs      - Migration runner
    db/            - Database-level migrations
    graph/         - Graph-level migrations
  nvim/            - Neovim IPC client
    client.rs      - Low-level msgpack-RPC
    lazy.rs        - Lazy connection wrapper
  visualization/   - 3D graph visualization (Bevy)
    graph.rs       - Force-directed layout
    plugin.rs      - Bevy plugin
    setup.rs       - Scene initialization
    systems/       - ECS systems (camera, interaction, physics, UI, nvim)
    components.rs  - ECS components
    resources.rs   - ECS resources
    constants.rs   - Visual constants
    nvim.rs        - Nvim file navigation from visualizer
di-macros/         - Proc-macro crate for DI (Context, FromContext derives)
docker/            - Docker build for PostgreSQL+AGE+pgvector
tests/             - Integration tests (postgres_integration.rs)
```
