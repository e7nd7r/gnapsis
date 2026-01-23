# Gnapsis

Code intelligence graph - MCP server for knowledge management with Neo4j.

## Overview

Gnapsis is an MCP (Model Context Protocol) server that helps AI assistants understand and navigate codebases through a semantic knowledge graph. It tracks entities (modules, structs, functions), their relationships, and anchors them to specific code locations.

## Features

- **Semantic Knowledge Graph**: Track entities at multiple scope levels (Domain → Feature → Namespace → Component → Unit)
- **Document References**: Anchor entities to code/documentation with line-level precision
- **Staleness Detection**: Detect when references become outdated via git hunk overlap
- **Semantic Search**: Find entities and references by meaning, not just keywords
- **Subgraph Queries**: Extract relevant context within token budgets using Best-First Search
- **LSP Integration**: Validate and refresh code references using language server data

## Requirements

- Rust 1.75+
- Neo4j 5.x with APOC plugin
- Git (for staleness detection)

## Installation

```bash
# Clone the repository
git clone https://github.com/e7nd7r/gnapsis.git
cd gnapsis

# Build
cargo build --release

# The binary is at target/release/gnapsis
```

## Configuration

Create a `.gnapsis.toml` in your project root:

```toml
[neo4j]
uri = "bolt://localhost:7687"
username = "neo4j"
password = "your-password"
database = "neo4j"
```

Or use environment variables:

```bash
export NEO4J_URI="bolt://localhost:7687"
export NEO4J_USERNAME="neo4j"
export NEO4J_PASSWORD="your-password"
```

## MCP Server Usage

Add to your Claude Code MCP configuration:

```json
{
  "mcpServers": {
    "gnapsis": {
      "command": "/path/to/gnapsis",
      "args": ["mcp"]
    }
  }
}
```

## Tools (16 total)

### Project Tools

| Tool | Description |
|------|-------------|
| `init_project` | Initialize database schema and seed data |
| `project_overview` | Get full context: taxonomy, entity hierarchy, stats. Optionally generates a skill file |

### Taxonomy Tools

| Tool | Description |
|------|-------------|
| `create_category` | Create new category at a scope |

### Entity Tools

| Tool | Description |
|------|-------------|
| `create_entity` | Create entity with commands (Add, Relate, Link) |
| `update_entity` | Update entity with commands (re-embeds on description change) |
| `delete_entity` | Delete entity (must have no children) |

### Reference Tools

| Tool | Description |
|------|-------------|
| `alter_references` | Bulk update/delete references (auto-updates commit SHA) |

### Query Tools

| Tool | Description |
|------|-------------|
| `get_entity` | Get entity with full context (classifications, references, hierarchy) |
| `find_entities` | Find entities by scope, category, or parent |
| `get_document_entities` | Get all entities in a document |
| `search` | Unified semantic search across entities and/or references |
| `query` | Semantic subgraph extraction with Best-First Search |

### Sync Tools

| Tool | Description |
|------|-------------|
| `get_changed_files` | Get list of files changed between commits |

### Analysis Tools

| Tool | Description |
|------|-------------|
| `analyze_document` | Unified document analysis: tracked refs, staleness, untracked symbols, diffs |

### Validation & LSP Tools

| Tool | Description |
|------|-------------|
| `validate_graph` | Check graph integrity (orphans, cycles, scope violations, unclassified, no references) |
| `lsp_refresh` | Refresh document references using LSP symbol locations |

## Concepts

### Scopes

Entities are organized in a hierarchy of scopes:

| Scope | Description | Example |
|-------|-------------|---------|
| **Domain** | High-level business domain | "Authentication", "Payments" |
| **Feature** | Functional capability | "JWT Validation", "OAuth Flow" |
| **Namespace** | Code organization unit | "auth::providers", "services" |
| **Component** | Structural code element | Struct, Class, Module |
| **Unit** | Atomic code element | Function, Method, Constant |

### Categories

Categories classify entities within their scope. Examples:
- Domain: `core`, `infrastructure`
- Feature: `api`, `ui`, `background`
- Namespace: `module`, `package`
- Component: `struct`, `class`, `trait`, `enum`
- Unit: `function`, `method`, `constant`

### References

References anchor entities to code locations:

- **Code References**: Point to LSP symbols with line ranges
- **Text References**: Point to documentation sections with optional anchors

### Relationships

- **BELONGS_TO**: Composition hierarchy (child belongs to parent)
- **RELATED_TO**: Semantic association with optional note
- **CALLS/IMPORTS/IMPLEMENTS/INSTANTIATES**: Code-level links

## License

MIT
