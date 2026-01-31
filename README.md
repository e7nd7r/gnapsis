# Gnapsis

Code intelligence graph - MCP server for semantic codebase understanding with PostgreSQL + Apache AGE.

## What is Gnapsis?

Gnapsis is an MCP (Model Context Protocol) server that helps AI assistants understand codebases through a semantic knowledge graph. Instead of just searching text, it tracks **entities** (domains, features, modules, structs, functions), their **relationships**, and anchors them to specific **code locations**.

Think of it as a structured memory layer for AI coding assistants.

## Features

- **Semantic Knowledge Graph**: Organize entities in a hierarchy (Domain → Feature → Namespace → Component → Unit)
- **Document References**: Anchor entities to code with line-level precision
- **Semantic Search**: Find entities by meaning, not just keywords
- **Subgraph Queries**: Extract relevant context within token budgets
- **Staleness Detection**: Know when references become outdated via git
- **TOON Output**: Token-efficient output format (40-60% fewer tokens than JSON)
- **Multi-Project Support**: Each project gets its own graph in a shared database

## Quick Start

### 1. Prerequisites

**PostgreSQL with Apache AGE and pgvector**:

```bash
# Using Docker Compose (recommended)
# Builds a custom image with AGE 1.6.0 (PG17) + pgvector 0.8.0
docker compose up -d
```

This starts a PostgreSQL instance with Apache AGE and pgvector extensions pre-configured, including the initial graph creation.

**Rust 1.75+** for building from source.

### 2. Installation

```bash
git clone https://github.com/e7nd7r/gnapsis.git
cd gnapsis

# Install the binary
cargo install --path .
```

### 3. Configuration

Gnapsis uses a layered configuration approach (highest priority last):

1. **Global config** (`~/.config/gnapsis/config.toml`) - database and embedding settings
2. **Project config** (`.gnapsis.toml` in your project root) - project name and sources
3. **Environment variables** (`GNAPSIS_*`) - overrides for any setting

**Global config** (`~/.config/gnapsis/config.toml`):

```toml
[postgres]
uri = "postgresql://postgres:postgres@localhost:5432/gnapsis_dev"

[embedding]
provider = "fastembed"
model = "BAAI/bge-small-en-v1.5"
dimensions = 384
```

**Project config** (`.gnapsis.toml` in your project root):

```toml
[project]
name = "my-project"
# Graph name is always gnapsis_<name> (e.g., gnapsis_my_project)
```

**Multi-source projects** - span multiple directories (e.g., code + docs):

```toml
[project]
name = "my-project"

[[project.sources]]
id = "default"
path = "/path/to/code-repo"

[[project.sources]]
id = "docs"
path = "/path/to/documentation-vault"
```

See `.gnapsis.toml.example` for a complete reference.

### 4. MCP Setup

Add to your project's `.mcp.json`:

```json
{
  "mcpServers": {
    "gnapsis": {
      "command": "gnapsis",
      "args": ["mcp"]
    }
  }
}
```

### 5. Initialize the Project

Once connected via MCP, initialize the database:

```
Use init_project to set up the database schema
```

This creates the schema, scopes, and default categories.

## Workflow

### Understanding the Hierarchy

Gnapsis organizes knowledge in **scopes** (levels of abstraction):

```
Domain          (e.g., "Authentication", "Payments")
  └── Feature   (e.g., "JWT Validation", "OAuth Flow")
      └── Namespace   (e.g., "auth::providers", "services")
          └── Component   (e.g., UserService struct, AuthTrait)
              └── Unit   (e.g., validate() method, MAX_RETRIES constant)
```

Each entity has a **category** that classifies it within its scope:
- Domain: `core`, `infrastructure`
- Feature: `functional`, `technical`, `non-functional`
- Namespace: `module`, `library`
- Component: `struct`, `trait`, `enum`, `class`
- Unit: `function`, `method`, `constant`, `field`

### Creating Entities

Entities need: **name**, **description**, **category**, and at least one **reference** to code.

Example - creating a domain:

```
Use create_entity with:
- name: "Authentication"
- description: "Domain for user authentication and authorization"
- category_ids: [<core-domain-category-id>]
- commands: [{ type: "add", ref_type: "text", document_path: "README.md", description: "Auth section", start_line: 10, end_line: 20 }]
```

Example - creating a component under a namespace:

```
Use create_entity with:
- name: "UserService"
- description: "Service for user management operations"
- category_ids: [<struct-category-id>]
- parent_ids: [<services-namespace-id>]
- commands: [{ type: "add", ref_type: "code", document_path: "src/services/user.rs", lsp_symbol: "UserService", description: "UserService struct" }]
```

### Querying the Graph

**Get project overview** - See all domains, features, namespaces, and categories:
```
Use project_overview
```

**Semantic search** - Find entities by meaning:
```
Use search with query: "user authentication validation"
```

**Subgraph query** - Get related entities within a token budget:
```
Use query with semantic_query: "how does authentication work"
```

**Find entities** - Filter by scope, category, or parent:
```
Use find_entities with scope: "Component", category: "struct"
```

### Analyzing Documents

Check what's tracked in a file and detect stale references:
```
Use analyze_document with document_path: "src/services/user.rs"
```

### Validating the Graph

Check for integrity issues (orphans, cycles, missing references):
```
Use validate_graph
```

## Output Formats

Tools that return large results support **TOON** (Token-Oriented Object Notation) for 40-60% token savings:

```
Use query with semantic_query: "...", output_format: "toon"
```

Supported on: `query`, `search`, `analyze_document`, `project_overview`

## Tools Reference

| Tool | Description |
|------|-------------|
| `init_project` | Initialize database schema |
| `project_overview` | Get taxonomy, entities, and stats |
| `create_category` | Create new category at a scope |
| `create_entity` | Create entity with references |
| `update_entity` | Update entity (re-embeds on description change) |
| `delete_entity` | Delete entity (must have no children) |
| `get_entity` | Get entity with full context |
| `find_entities` | Find entities by scope/category/parent |
| `search` | Semantic search across entities and references |
| `query` | Semantic subgraph extraction |
| `get_document_entities` | Get entities referenced in a document |
| `analyze_document` | Document analysis with staleness detection |
| `alter_references` | Bulk update/delete references |
| `validate_graph` | Check graph integrity |
| `lsp_refresh` | Refresh references using LSP data |
| `get_changed_files` | Get files changed between commits |

## Development

Gnapsis uses [just](https://github.com/casey/just) for development tasks:

```bash
just db-up          # Start the database
just build          # Build the project
just test           # Run unit tests
just test-integration  # Run integration tests (requires db-up)
just lint           # Run clippy
just fmt            # Format code
just db-shell       # Connect to psql
just cypher "MATCH (n) RETURN n"  # Run a Cypher query
```

Run `just` with no arguments to see all available commands.

## License

MIT - See [LICENSE](LICENSE) for details.
