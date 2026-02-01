# Architecture

## Overview

Gnapsis is an MCP server that provides AI assistants with semantic understanding of codebases through a knowledge graph. It uses PostgreSQL with Apache AGE for graph storage and pgvector for embedding-based semantic search.

```
┌─────────────────────────────────────────┐
│            AI Assistant (MCP Client)     │
└───────────────────┬─────────────────────┘
                    │ stdio (JSON-RPC)
┌───────────────────▼─────────────────────┐
│              MCP Server                  │
│         (tool routing, protocol)         │
├──────────────────────────────────────────┤
│              Services                    │
│    (business logic, commands, validation)│
├──────────────────────────────────────────┤
│            Repositories                  │
│     (data access, query construction)    │
├──────────────────────────────────────────┤
│          Graph Abstraction Layer         │
│   (traits, query builder, row parsing)   │
├──────────────────────────────────────────┤
│     PostgreSQL + Apache AGE + pgvector   │
└──────────────────────────────────────────┘
```

## Module Structure

```
src/
├── cli/           CLI commands (init, mcp, visualize)
├── config.rs      Layered configuration (global → project → env)
├── context.rs     Dependency injection context
├── di.rs          DI trait definitions
├── error.rs       Error types
├── git.rs         Git operations for staleness detection
├── graph/         Graph abstraction layer
│   ├── traits.rs  Core traits (CypherExecutor, GraphClient, Transaction)
│   ├── query.rs   Fluent query builder
│   ├── row.rs     Result row, Node, Relation, Path types
│   ├── cypher.rs  Cypher parser (RETURN clause extraction)
│   ├── macros.rs  cypher!() macro for query construction
│   └── backends/
│       └── postgres.rs  PostgreSQL + Apache AGE implementation
├── mcp/           MCP server and tool implementations
│   ├── server.rs  Tool router and handler
│   ├── protocol.rs Response helpers
│   └── tools/     15 tools organized by domain
├── migrations/    Database schema migrations
├── models/        Data models (Entity, Document, Category, Scope)
├── repositories/  Data access layer (entity, document, category, query, schema)
├── services/      Business logic (entity, graph, commands, validation, lsp)
├── nvim/          Neovim integration (RPC client)
└── visualization/ 3D graph visualization (Bevy)
```

## Graph Abstraction Layer

The graph module provides a backend-agnostic interface for graph database operations. This allows the application logic to remain independent of the underlying database engine.

### Trait Hierarchy

```
CypherExecutor          (required - execute Cypher queries)
    │
    ├── SqlExecutor     (optional - execute SQL for DDL/backend-specific ops)
    │
    └── GraphClient     (connection management + transaction creation)
            │
            └── Transaction  (commit/rollback lifecycle)
```

**`CypherExecutor`** is the core trait. All graph operations go through Cypher queries, making the application code database-agnostic. Backends only need to translate Cypher into their native query format.

**`SqlExecutor`** is optional and used for operations that can't be expressed in Cypher (DDL, extension-specific features). Only backends with SQL support (like PostgreSQL) implement this.

**`GraphClient`** extends `CypherExecutor` with transaction support. It manages connection pooling and creates transactions.

### Graph Wrapper

The `Graph<C: GraphClient>` struct wraps any client and provides:
- **Direct queries** - auto-commit per query via `graph.query("...")`
- **Transactions** - closure-based with explicit commit/rollback

```rust
// Direct query
let rows = graph.query("MATCH (n:Entity) WHERE n.id = $id RETURN n")
    .param("id", entity_id)
    .fetch_all()
    .await?;

// Transaction
graph.transaction(|txn| async move {
    txn.query("CREATE (n:Entity {id: $id})").param("id", id).run().await?;
    txn.commit().await?;
    Ok(())
}).await?;
```

### Query Builder

The `Query` struct provides a fluent API for constructing Cypher queries with typed parameters. Parameters use `$name` syntax and are bound safely (never interpolated into the query string).

Methods: `param()`, `fetch_all()`, `fetch_one()`, `execute()` (streaming), `run()` (no results).

### PostgreSQL + Apache AGE Backend

The current backend uses:
- **Apache AGE** extension for Cypher query execution within PostgreSQL
- **pgvector** extension for embedding storage and similarity search
- **deadpool-postgres** for connection pooling
- **Agtype** binary format for safe parameter binding (no SQL injection)

AGE wraps Cypher queries in SQL function calls (`SELECT * FROM cypher('graph', $$ ... $$)`). The backend handles this translation transparently, including parameter binding through AGE's `agtype` type system.

## Knowledge Graph Model

### Entity Hierarchy (Scopes)

Entities are organized in a five-level hierarchy of increasing specificity:

| Scope | Purpose | Examples |
|-------|---------|----------|
| Domain | Business domains | Authentication, Payments |
| Feature | Capabilities within a domain | JWT Validation, OAuth Flow |
| Namespace | Code modules / packages | `auth::providers`, `services` |
| Component | Types and interfaces | UserService struct, AuthTrait |
| Unit | Individual members | `validate()` method, `MAX_RETRIES` constant |

Each scope has **categories** that classify entities within it (e.g., Component scope has `struct`, `trait`, `enum` categories).

### Relationships

- **BELONGS_TO** - parent-child hierarchy (child scope must be deeper than parent)
- **CLASSIFIED_AS** - entity to category mapping
- **RELATED_TO** - semantic relationships with optional notes
- **Code links** - CALLS, IMPORTS, IMPLEMENTS, INSTANTIATES (Component/Unit only)

### Document References

Entities are anchored to code through references:

- **Code references** - linked via LSP symbol names, with automatic line tracking
- **Text references** - linked via line ranges, for documentation and markdown

References track git commit SHAs for **staleness detection** - when the referenced file changes, the reference is marked stale.

## Multi-Project Support

A single PostgreSQL database can host multiple project graphs. Each project:
- Has a unique name and graph (`gnapsis_<name>`)
- Can span multiple source directories via **sources**
- Shares the same database connection but operates on its own graph

## Configuration

Three-layer resolution (highest priority last):

1. **Global config** (`~/.config/gnapsis/config.toml`) - database URI, embedding settings
2. **Project config** (`.gnapsis.toml`) - project name, source directories
3. **Environment variables** (`GNAPSIS_*`) - overrides for any setting

## Dependency Injection

Gnapsis uses compile-time DI via the `di-macros` crate:

- `Context` struct holds shared state (graph client, embedding model, config)
- Repositories derive `FromRef<Context>` to extract their dependencies
- No runtime reflection or dynamic dispatch for DI resolution
