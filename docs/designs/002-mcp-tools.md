---
title: "DES-003: MCP Tools for Ontology Management"
date: 2025-01-15
status: draft
tags: [design, mcp, tools, api]
---

# DES-003: MCP Tools for Ontology Management

## Problem

We need an MCP interface to manage the ontology graph - creating categories, classifying entities, querying relationships, and syncing with git changes.

## Tool Categories

1. **Project Management** - Initialize projects, manage Neo4j connections
2. **Taxonomy Management** - Create/list scopes and categories
3. **Entity Management** - Create entities, classify them, manage references
4. **Query Tools** - Search and navigate the graph
5. **Sync Tools** - Update graph based on git changes

## Tools

### Project Management

#### `init_project`

Initialize a new project with Neo4j database.

```rust
#[tool(description = "Initialize a new ontology project")]
async fn init_project(
    name: String,           // Project identifier
    repo_path: String,      // Path to git repository
    description: Option<String>,
) -> Result<CallToolResult, McpError>;
```

**Actions:**
- Validate/initialize git repository
- Create Neo4j database
- Seed default scopes and categories
- Save project config

---

### Taxonomy Management

#### `create_category`

Create a new category at a scope.

```rust
#[tool(description = "Create a new category at a scope")]
async fn create_category(
    name: String,
    scope: String,          // "Domain", "Feature", "Namespace", "Component", "Unit"
    description: Option<String>,
) -> Result<CallToolResult, McpError>;
```

#### `list_categories`

List categories at a scope.

```rust
#[tool(description = "List all categories at a scope")]
async fn list_categories(
    scope: Option<String>,  // If None, list all categories grouped by scope
) -> Result<CallToolResult, McpError>;
```

#### `list_scopes`

List all scopes with their hierarchy.

```rust
#[tool(description = "List all scopes and their composition hierarchy")]
async fn list_scopes() -> Result<CallToolResult, McpError>;
```

---

### Entity Management

#### `create_entity`

Create a new entity (without document references).

```rust
#[tool(description = "Create a new entity")]
async fn create_entity(
    name: String,
    description: String,    // auto-embedded
) -> Result<CallToolResult, McpError>;
```

**Returns:** Entity ID

#### `update_entity`

Update an entity's properties.

```rust
#[tool(description = "Update an entity's name and/or description")]
async fn update_entity(
    entity_id: String,
    name: Option<String>,
    description: Option<String>,  // re-embeds if changed
) -> Result<CallToolResult, McpError>;
```

**Note:** If description changes, embedding is regenerated.

#### `delete_entity`

Delete an entity (fails if has children).

```rust
#[tool(description = "Delete an entity (must have no children)")]
async fn delete_entity(
    entity_id: String,
) -> Result<CallToolResult, McpError>;
```

**Note:** Blocked by Neo4j trigger if entity has children. Cascades delete of DocumentReferences and relationships.

#### `add_references`

Add document references to an entity.

```rust
#[tool(description = "Add document references to an entity")]
async fn add_references(
    entity_id: String,
    refs: Vec<DocumentRef>,
) -> Result<CallToolResult, McpError>;

struct DocumentRef {
    document_path: String,          // Relative path from repo root
    start_line: u32,
    end_line: u32,
    description: String,            // What this code/text does (auto-embedded)
    content_type: Option<String>,   // "code" or "text" (auto-detected if not provided)
    offset: Option<u32>,
    commit_sha: Option<String>,     // Defaults to HEAD

    // LSP fields (optional - enables lsp_refresh)
    lsp_symbol: Option<String>,     // Fully qualified symbol name
    lsp_kind: Option<i32>,          // LSP SymbolKind enum value
    lsp_range: Option<String>,      // "startLine:startChar-endLine:endChar"
}
```

**Note:** Description is auto-embedded. Content can be retrieved via `git show {commit_sha}:{path}` with line range.

**Returns:** Vec of DocumentReference IDs

#### `remove_references`

Remove document references.

```rust
#[tool(description = "Remove document references")]
async fn remove_references(
    reference_ids: Vec<String>,
) -> Result<CallToolResult, McpError>;
```

#### `classify`

Classify an entity with one or more categories.

```rust
#[tool(description = "Classify an entity with categories")]
async fn classify(
    entity_id: String,
    categories: Vec<String>,
) -> Result<CallToolResult, McpError>;
```

#### `unclassify`

Remove classifications from an entity.

```rust
#[tool(description = "Remove classifications from an entity")]
async fn unclassify(
    entity_id: String,
    categories: Vec<String>,
) -> Result<CallToolResult, McpError>;
```

#### `add_belongs`

Add composition relationships (child belongs to one or more parents).

```rust
#[tool(description = "Add composition: child BELONGS_TO parents")]
async fn add_belongs(
    from: String,           // child entity ID
    to: Vec<String>,        // parent entity IDs
    note: Option<String>,   // context for the relationship (auto-embedded)
) -> Result<CallToolResult, McpError>;
```

**Note:** Neo4j trigger enforces that child's scope depth > parent's scope depth. If `note` is provided, embedding is auto-generated.

#### `add_related`

Add a general relationship between entities.

```rust
#[tool(description = "Add RELATED_TO relationship between entities")]
async fn add_related(
    from: String,           // source entity ID
    to: Vec<String>,        // target entity IDs
    note: Option<String>,   // context for the relationship (auto-embedded)
) -> Result<CallToolResult, McpError>;
```

#### `add_link`

Add a cross-reference link between entities.

```rust
#[tool(description = "Add a code-level link between entities")]
async fn add_link(
    from_id: String,
    to_id: String,
    link_type: String,       // "CALLS", "IMPORTS", "IMPLEMENTS", "INSTANTIATES"
    note: Option<String>,    // context for the link (auto-embedded)
) -> Result<CallToolResult, McpError>;
```

---

### Query Tools

#### `get_entity`

Get entity details with classifications.

```rust
#[tool(description = "Get entity details including classifications")]
async fn get_entity(
    entity_id: String,
) -> Result<CallToolResult, McpError>;
```

**Returns:**
```json
{
  "id": "uuid",
  "name": "Scheduler",
  "description": "...",
  "classifications": [
    {"category": "struct", "scope": "Component"}
  ],
  "references": [
    {
      "id": "ref-uuid",
      "document": "src/scheduler/mod.rs",
      "start_line": 10,
      "end_line": 25,
      "commit_sha": "abc123"
    }
  ],
  "belongs_to": "parent-entity-id",
  "children": ["entity-id-1", "entity-id-2"]
}
```

#### `find_entities`

Search entities by classification.

```rust
#[tool(description = "Find entities by classification criteria")]
async fn find_entities(
    domain: Option<String>,
    feature: Option<String>,
    namespace: Option<String>,
    component: Option<String>,
    unit: Option<String>,
    in_document: Option<String>,  // Filter by document path
) -> Result<CallToolResult, McpError>;
```

#### `get_document_entities`

Get all entities defined in a document.

```rust
#[tool(description = "Get all entities in a document")]
async fn get_document_entities(
    path: String,
) -> Result<CallToolResult, McpError>;
```

#### `get_composition_graph`

Get the full composition subgraph for an entity via BELONGS_TO DAG.

```rust
#[tool(description = "Get entity's composition subgraph (ancestors, siblings, descendants)")]
async fn get_composition_graph(
    entity_id: String,
    depth_up: Option<u32>,      // Max ancestor depth (default: unlimited)
    depth_down: Option<u32>,    // Max descendant depth (default: unlimited)
    include_siblings: Option<bool>,  // Include siblings at each level (default: false)
) -> Result<CallToolResult, McpError>;
```

**Returns:**
```json
{
  "entity": {
    "id": "tick-id",
    "name": "tick",
    "category": "method"
  },
  "ancestors": [
    {"id": "scheduler-struct-id", "name": "Scheduler", "category": "struct"},
    {"id": "scheduler-mod-id", "name": "scheduler", "category": "module"}
  ],
  "descendants": [],
  "siblings": [
    {"id": "other-method-id", "name": "run", "category": "method"}
  ]
}
```

**Note:** This is bounded by the BELONGS_TO DAG structure - safe traversal without pulling entire graph.

#### `query_subgraph`

Extract a subgraph from a starting entity within N hops.

```rust
#[tool(description = "Extract subgraph around an entity")]
async fn query_subgraph(
    entity_id: String,
    hops: u32,                      // max distance
    query: Option<String>,          // optional semantic filter
) -> Result<CallToolResult, McpError>;
```

**Returns:**
```json
{
  "nodes": [
    {"id": "uuid-1", "name": "Scheduler", "category": "struct", "distance": 0},
    {"id": "uuid-2", "name": "Backend", "category": "trait", "distance": 1},
    {"id": "uuid-3", "name": "Config", "category": "struct", "distance": 2}
  ],
  "edges": [
    {"from": "uuid-1", "to": "uuid-2", "type": "CALLS", "note": null},
    {"from": "uuid-2", "to": "uuid-3", "type": "BELONGS_TO", "note": "config dependency"}
  ]
}
```

**Note:** Traverses BELONGS_TO, RELATED_TO, CALLS, IMPORTS, etc. If query provided, nodes/edges are filtered by semantic similarity.

#### `search_documents`

Search document references with optional filters.

```rust
#[tool(description = "Search document references by semantic similarity")]
async fn search_documents(
    query: String,                  // semantic search query
    entity_id: Option<String>,      // filter by entity
    category: Option<String>,       // filter by category
    limit: Option<u32>,             // default 10
) -> Result<CallToolResult, McpError>;
```

**Returns:**
```json
{
  "results": [
    {
      "reference_id": "ref-uuid",
      "entity_id": "entity-uuid",
      "entity_name": "Scheduler",
      "document": "src/scheduler/mod.rs",
      "start_line": 10,
      "end_line": 25,
      "score": 0.92
    }
  ]
}
```

**Note:** Searches against DocumentReference embeddings.

#### `semantic_search`

Search entities by semantic similarity.

```rust
#[tool(description = "Search entities by semantic similarity to a query")]
async fn semantic_search(
    query: String,
    limit: Option<u32>,         // Default 10
    scope: Option<String>,      // Filter by scope
    category: Option<String>,   // Filter by category
) -> Result<CallToolResult, McpError>;
```

---

### Sync Tools

#### `sync_document`

Parse and sync a document's entities.

```rust
#[tool(description = "Parse a document and sync its entities to the graph")]
async fn sync_document(
    path: String,
    commit_sha: Option<String>,  // Defaults to HEAD
) -> Result<CallToolResult, McpError>;
```

**Actions:**
- Parse document for entities (functions, structs, etc.)
- Create/update entities in graph
- Update document reference with commit

#### `sync_changes`

Sync all changes between commits.

```rust
#[tool(description = "Sync graph with git changes between commits")]
async fn sync_changes(
    from_commit: Option<String>,  // Defaults to last synced commit
    to_commit: Option<String>,    // Defaults to HEAD
) -> Result<CallToolResult, McpError>;
```

**Actions:**
- Get changed files from `git diff`
- Re-parse changed documents
- Update affected entities
- Track new sync commit

#### `validate_documents`

Find documents with stale references and return detailed report for LLM review.

```rust
#[tool(description = "Find stale document references with diffs for LLM review")]
async fn validate_documents(
    since_commit: Option<String>,  // Defaults to last synced commit
    path: Option<String>,          // Filter to specific document
    limit: Option<u32>,            // Max stale references returned (default: 50)
    offset: Option<u32>,           // Skip first N stale references (default: 0)
) -> Result<CallToolResult, McpError>;
```

**Returns:**
```json
{
  "stale_references": [
    {
      "reference_id": "ref-uuid-1",
      "entity_id": "entity-uuid-1",
      "entity_name": "Scheduler",
      "document": "src/scheduler/mod.rs",
      "old_commit": "abc123",
      "current_commit": "def456",
      "old_lines": [10, 25],
      "old_content": "pub struct Scheduler { ... }",
      "current_content_at_lines": "impl Scheduler { ... }",
      "status": "content_mismatch"
    },
    {
      "reference_id": "ref-uuid-2",
      "entity_id": "entity-uuid-2",
      "entity_name": "tick",
      "document": "src/scheduler/mod.rs",
      "old_commit": "abc123",
      "current_commit": "def456",
      "old_lines": [30, 45],
      "old_content": "pub async fn tick(&self) -> Result<()> { ... }",
      "current_content_at_lines": null,
      "status": "lines_out_of_range"
    }
  ],
  "pagination": {
    "offset": 0,
    "limit": 50,
    "returned": 50,
    "has_more": true
  },
  "summary": {
    "documents_checked": 89,
    "total_stale_documents": 12,
    "total_stale_references": 127,
    "by_status": {
      "content_mismatch": 89,
      "lines_out_of_range": 31,
      "document_deleted": 7
    }
  }
}
```

**Status values:**
- `content_mismatch`: Lines exist but content differs
- `lines_out_of_range`: Line numbers exceed document length
- `document_deleted`: Source document no longer exists

#### `sync_references`

Update document references after LLM determines correct new positions.

```rust
#[tool(description = "Sync document references with corrected line numbers")]
async fn sync_references(
    refs: Vec<ReferenceUpdate>,
) -> Result<CallToolResult, McpError>;

struct ReferenceUpdate {
    reference_id: String,
    start_line: u32,
    end_line: u32,
    offset: Option<u32>,
    commit_sha: Option<String>,  // Defaults to HEAD
    regenerate_embedding: Option<bool>,  // Default: true
}
```

**Actions:**
- Updates line numbers and offset
- Updates commit_sha to current HEAD (or specified)
- Re-extracts content from document
- Regenerates embedding if content changed

**Returns:**
```json
{
  "updated": [
    {"reference_id": "ref-uuid-1", "status": "ok"},
    {"reference_id": "ref-uuid-2", "status": "ok"}
  ],
  "failed": [
    {"reference_id": "ref-uuid-3", "error": "Reference not found"}
  ]
}
```

---

### LSP Integration

#### `lsp_analyze`

Find code symbols via LSP that aren't tracked in the graph.

```rust
#[tool(description = "Find untracked LSP symbols for manual entity/reference creation")]
async fn lsp_analyze(
    path: Option<String>,             // Filter to specific file/directory
    include_kinds: Option<Vec<i32>>,  // LSP SymbolKind filter
    limit: Option<u32>,               // Max results (default: 50)
    offset: Option<u32>,              // Skip first N results (default: 0)
) -> Result<CallToolResult, McpError>;
```

**Requires:** LSP server running for the project language

**Returns:**
```json
{
  "untracked_symbols": [
    {
      "symbol": "scheduler::Scheduler::new",
      "kind": 6,
      "kind_name": "method",
      "document": "src/scheduler/mod.rs",
      "start_line": 15,
      "end_line": 22,
      "lsp_range": "15:4-22:5",
      "container": "scheduler::Scheduler",
      "suggested_category": "method"
    }
  ],
  "pagination": {
    "offset": 0,
    "limit": 50,
    "returned": 50,
    "has_more": true
  },
  "summary": {
    "symbols_found": 1650,
    "tracked": 1523,
    "total_untracked": 127
  }
}
```

**Workflow:**
1. Run `lsp_analyze` to find untracked symbols
2. LLM reviews and creates entities with descriptions via `create_entity`
3. LLM adds references via `add_references` with LSP fields populated

**Note:** When calling `add_references`, include LSP fields to enable future `lsp_refresh`:

```rust
struct DocumentRef {
    // ... existing fields ...
    lsp_symbol: Option<String>,   // "scheduler::Scheduler::new"
    lsp_kind: Option<i32>,        // 6 (method)
    lsp_range: Option<String>,    // "15:4-22:5"
}
```

#### `lsp_refresh`

Update references using LSP to find current symbol positions.

```rust
#[tool(description = "Refresh stale references using LSP symbol lookup")]
async fn lsp_refresh(
    path: Option<String>,       // Filter to specific document
    dry_run: Option<bool>,      // Report changes without applying (default: false)
    limit: Option<u32>,         // Max references to process (default: 50)
    offset: Option<u32>,        // Skip first N references (default: 0)
) -> Result<CallToolResult, McpError>;
```

**Actions:**
1. Find DocumentReferences with `lsp_symbol` set
2. Query LSP for current symbol positions
3. Update line numbers and `lsp_range`
4. Regenerate embeddings if content changed

**Returns:**
```json
{
  "refreshed": [
    {
      "reference_id": "ref-uuid",
      "entity_name": "new",
      "symbol": "scheduler::Scheduler::new",
      "old_lines": [15, 22],
      "new_lines": [18, 25],
      "content_changed": true
    }
  ],
  "not_found": [
    {
      "reference_id": "ref-uuid-2",
      "symbol": "scheduler::deprecated",
      "reason": "symbol_removed"
    }
  ],
  "pagination": {
    "offset": 0,
    "limit": 50,
    "returned": 50,
    "has_more": true
  },
  "summary": {
    "total_lsp_tracked": 1523,
    "refreshed": 45,
    "unchanged": 1470,
    "not_found": 8
  }
}
```

---

### Validation Tools

#### `validate_graph`

Validate graph integrity and find issues.

```rust
#[tool(description = "Validate graph integrity and report issues")]
async fn validate_graph(
    fix: Option<bool>,          // Auto-fix fixable issues (default: false)
    issue_type: Option<String>, // Filter by issue type
    limit: Option<u32>,         // Max issues returned (default: 50)
    offset: Option<u32>,        // Skip first N issues (default: 0)
) -> Result<CallToolResult, McpError>;
```

**Checks:**
- **Orphaned DocumentReferences**: References not linked to any Entity
- **Orphaned Documents**: Documents with no references pointing to them
- **Dangling references**: References pointing to non-existent Documents
- **Missing classifications**: Entities without any category classification
- **Scope depth violations**: BELONGS_TO where child depth ≤ parent depth
- **Multiple categories per scope**: Entities with >1 category at same scope
- **BELONGS_TO cycles**: Circular references in composition hierarchy
- **Invalid category-scope links**: Categories not properly linked to Scopes
- **Stale commit references**: DocumentReferences with commit_sha not in git history

**Returns:**
```json
{
  "valid": false,
  "issues": [
    {
      "type": "orphaned_reference",
      "severity": "warning",
      "entity_id": null,
      "reference_id": "ref-uuid",
      "message": "DocumentReference not linked to any Entity",
      "fixable": true
    },
    {
      "type": "missing_classification",
      "severity": "error",
      "entity_id": "entity-uuid",
      "reference_id": null,
      "message": "Entity 'Scheduler' has no classifications",
      "fixable": false
    }
  ],
  "pagination": {
    "offset": 0,
    "limit": 50,
    "returned": 50,
    "has_more": true
  },
  "summary": {
    "total_issues": 142,
    "by_type": {
      "orphaned_reference": 45,
      "missing_classification": 12,
      "scope_depth_violation": 3
    },
    "errors": 15,
    "warnings": 127,
    "fixed": 0
  }
}
```

**Fix behavior** (when `fix: true`):
- Orphaned DocumentReferences → deleted
- Orphaned Documents → deleted
- Dangling references → deleted
- Other issues require manual intervention

#### `get_project_stats`

Get statistics about the project graph.

```rust
#[tool(description = "Get project statistics: entity counts, documents, relationships")]
async fn get_project_stats() -> Result<CallToolResult, McpError>;
```

**Returns:**
```json
{
  "entities": {
    "total": 1523,
    "by_scope": {
      "Domain": 3,
      "Feature": 12,
      "Namespace": 45,
      "Component": 312,
      "Unit": 1151
    },
    "by_category": {
      "module": 45,
      "struct": 187,
      "trait": 23,
      "enum": 57,
      "function": 412,
      "method": 739
    },
    "unclassified": 0
  },
  "documents": {
    "total": 89,
    "by_extension": {
      ".rs": 72,
      ".md": 12,
      ".toml": 5
    }
  },
  "references": {
    "total": 1847,
    "by_content_type": {
      "code": 1623,
      "text": 224
    },
    "with_embeddings": 1847
  },
  "relationships": {
    "BELONGS_TO": 1520,
    "RELATED_TO": 89,
    "CALLS": 2341,
    "IMPORTS": 456,
    "IMPLEMENTS": 67,
    "INSTANTIATES": 234
  },
  "graph": {
    "max_depth": 5,
    "avg_children_per_entity": 3.2,
    "entities_with_multiple_parents": 12
  }
}
```

---

## Response Format

All tools return structured JSON:

```json
{
  "success": true,
  "data": { ... },
  "message": "Optional human-readable message"
}
```

Or on error:

```json
{
  "success": false,
  "error": "Error description",
  "code": "ERROR_CODE"
}
```

## Open Questions

1. **Batch operations** - Should we have batch versions of create/classify for efficiency?
2. **Embedding generation** - Should `create_entity` auto-generate embeddings, or separate tool?
3. **Parser selection** - How to specify which parser to use for `sync_document` (tree-sitter, regex, LLM)?
