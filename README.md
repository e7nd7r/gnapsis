# Gnapsis

Code intelligence graph - MCP server for semantic codebase understanding with Neo4j.

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

## Quick Start

### 1. Prerequisites

**Neo4j 5.x** with GDS plugin:

```bash
# Using Docker
docker run -d \
  --name neo4j \
  -p 7474:7474 -p 7687:7687 \
  -e NEO4J_AUTH=neo4j/your-password \
  -e NEO4J_PLUGINS='["graph-data-science"]' \
  neo4j:5
```

**Rust 1.75+** for building from source.

### 2. Installation

```bash
git clone https://github.com/e7nd7r/gnapsis.git
cd gnapsis

# Install the binary
cargo install --path .
```

### 3. Configuration

Create `.gnapsis.toml` in your project root (see `.gnapsis.toml.example`):

```toml
[project]
name = "my-project"

[neo4j]
uri = "bolt://localhost:7687"
username = "neo4j"
password = "your-password"
database = "neo4j"
```

Or use environment variables: `NEO4J_URI`, `NEO4J_USERNAME`, `NEO4J_PASSWORD`.

### 4. MCP Setup

Add to your Claude Code configuration (`~/.claude/claude_desktop_config.json`):

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

## License

MIT - See [LICENSE](LICENSE) for details.
