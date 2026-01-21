---
title: "DES-005: Ontology Improvements"
date: 2025-01-20
status: approved
iteration: 9
author: Team
tags: [design, ontology, references, lsp, validation, commands, semantic-search]
priority: high
trackId: 696f979f200e2fc758037587
related_adrs: []
related_designs: [DES-001, DES-002]
changelog:
  - 2025-01-20 (v1): Initial draft
  - 2025-01-20 (v2): Redesigned entity/reference model with command pattern
  - 2025-01-20 (v3): Consolidated tools from 29 to 20, added get_taxonomy, mandatory categories
  - 2025-01-20 (v4): Resolved all open questions
  - 2025-01-20 (v5): Merged get_taxonomy and get_project_stats into project_overview (29 → 19 tools, 34%)
  - 2025-01-20 (v6): Removed sync_references, alter_references auto-updates SHA (29 → 18 tools, 38%)
  - 2025-01-20 (v7): Unified search tools, semantic subgraph with `query`, RELATED_TO note embeddings (29 → 16 tools, 45%)
  - 2025-01-20 (v8): Best-First Search algorithm with configurable scoring (GlobalOnly vs BranchPenalty)
  - 2025-01-20 (v9): LSP internal integration - Gnapsis queries LSP directly, removed lsp_symbols from all tool params
---

# DES-005: Ontology Improvements

## Overview

EPIC-001 delivered 29 MCP tools for managing a knowledge graph, but lacks constraints to ensure the graph remains useful. Entities can exist without code/document anchors, references become stale without detection, and LSP symbols aren't validated against actual language server data.

This design introduces:
- **Command-based entity operations** - Attach, add, relate, link references/relationships in one call
- **Independent reference lifecycle** - References exist independently, attached to entities
- **Mandatory references** - All entities must have at least one reference
- **LSP-first code references** - Use LSP range directly; user range only when no LSP available
- **Line-level staleness** - Git hunk overlap detection
- **Unified document analysis** - Single tool for complete document state
- **Semantic subgraph optimization** - Best-First Search with budget-aware scoring for context-efficient graph queries

## Goals

1. **Simplify entity management** - Consolidate 5+ tools into command-based create/update
2. **Enforce mandatory references** - All entities MUST have at least one reference
3. **Line-level staleness detection** - Detect via git hunk overlap, not file-level
4. **LSP-first references** - Code references use LSP range; validation required
5. **Unified document analysis** - Single tool replaces lsp_analyze, validate_documents, get_document_references
6. **Partial execution safety** - Commands execute sequentially with clear failure reporting
7. **Context-efficient subgraph queries** - Best-First Search with semantic scoring to return optimal context within token budget

## Non-Goals

- Automatic entity creation from LSP symbols (LLM decides what to track)
- Transactional rollback (partial execution is acceptable with clear reporting)

---

## Problem Analysis

### Current State

The existing tools have several issues:

1. **Too many granular tools** - 5+ separate tools for entity relationships:
   - `add_belongs`, `add_related`, `add_link`, `add_references`, `remove_references`

2. **No reference enforcement** - Entities can exist without any references

3. **Coarse staleness** - File-level (commit SHA differs) instead of line-level

4. **No LSP validation** - References can claim any `lsp_symbol` without verification

5. **Multiple analysis tools** - `lsp_analyze`, `validate_documents`, `get_document_references`

### Problems

1. **Tool sprawl** - LLM must orchestrate many calls for simple operations. Creating an entity with a reference and parent requires 3+ tool calls.

2. **Orphan entities** - Entities without references drift from codebase reality. They become "floating knowledge" that can't be verified.

3. **Coarse staleness** - A file might have 100 commits but a specific function unchanged. We mark it stale unnecessarily.

4. **Invalid references** - `lsp_symbol` can be anything. If symbol doesn't exist or was renamed, the reference is silently broken.

5. **Inefficient analysis** - Understanding document state requires 3 tool calls.

### Desired State

- Entity operations consolidated into command-based tools
- Every entity anchored to at least one document location
- References are independent objects with their own lifecycle
- Staleness detected at line-range granularity
- LSP symbols validated; LSP range used directly for code
- Single tool call to understand complete document state

---

## Reference Model

### Reference Independence

References are **independent objects** that:
- Exist separately from entities
- Can be attached to multiple entities (shared)
- Have their own lifecycle (create, update, delete)
- Cannot be deleted while attached to any entity

### Scope-Based Requirements

| Scope | Reference Type | Parent Requirement | Link Commands |
|-------|---------------|-------------------|---------------|
| **Domain** | Document (text) | None (top level) | No |
| **Feature** | Document (text) | ≥1 parent required | No |
| **Namespace** | Code OR Document | ≥1 parent required | No |
| **Component** | Code | ≥1 parent required | Yes (CALLS, etc.) |
| **Unit** | Code | ≥1 parent required | Yes (CALLS, etc.) |

### Code Reference Structure

For code files, we use **LSP range directly** (not user-specified range):

```rust
struct CodeReference {
    id: String,
    document_path: String,

    // LSP-derived (required when LSP available)
    lsp_symbol: String,        // e.g., "impl Foo::bar"
    lsp_kind: i32,             // LSP SymbolKind
    start_line: u32,           // From LSP
    end_line: u32,             // From LSP

    // Fallback (only when no LSP for language)
    // start_line, end_line set manually
    // lsp_symbol is LLM's best-effort name

    // Git tracking
    commit_sha: String,
    content_type: String,      // "code:rust", "code:typescript", etc.
}
```

### Text Reference Structure

```rust
struct TextReference {
    id: String,
    document_path: String,
    start_line: u32,
    end_line: u32,

    // Optional semantic anchor
    anchor: Option<String>,    // e.g., "## Architecture"

    // Git tracking
    commit_sha: String,
    content_type: String,      // "markdown", "text", etc.
}
```

### Embeddings

The following items are embedded for semantic search and subgraph optimization:

| Item | Field Embedded | Used For |
|------|----------------|----------|
| Entity | `description` | `search`, `query` relevance |
| Reference | `description` | `search`, `query` relevance |
| RELATED_TO | `note` | `query` edge relevance |

**Not embedded:**
- BELONGS_TO relationships - use parent entity's embedding instead
- Code links (CALLS, IMPORTS, etc.) - structural, not semantic

**Subgraph pruning heuristic:**
- BELONGS_TO: if parent irrelevant, skip grandparents
- RELATED_TO: score = note embedding similarity (if note exists) + target entity similarity

---

## Entity Command Model

### Commands

Entities are managed through commands that execute sequentially:

```rust
enum EntityCommand {
    // Reference operations
    Attach { reference_id: String },
    Unattach { reference_id: String },
    Add(NewReference),

    // Relationships (any scope)
    Relate {
        entity_id: String,
        note: Option<String>,  // Embedded for semantic subgraph queries
    },
    Unrelate { entity_id: String },

    // Code links (Component/Unit only)
    Link { entity_id: String, link_type: LinkType },
    Unlink { entity_id: String, link_type: LinkType },
}

enum LinkType {
    Calls,
    Imports,
    Implements,
    Instantiates,
}

enum NewReference {
    Code {
        document_path: String,
        lsp_symbol: String,
        // Range from LSP, or manual if no LSP
        start_line: Option<u32>,
        end_line: Option<u32>,
    },
    Text {
        document_path: String,
        start_line: u32,
        end_line: u32,
        anchor: Option<String>,
    },
}
```

### Execution Model

Commands execute **sequentially** with **partial execution on failure**:

```rust
struct CommandResult {
    // Commands that succeeded
    executed: Vec<ExecutedCommand>,

    // Command that failed (if any)
    failed: Option<FailedCommand>,

    // Commands not attempted after failure
    skipped: Vec<Command>,
}

struct FailedCommand {
    index: usize,
    command: Command,
    error: String,
    // For context (e.g., entities blocking delete)
    context: Option<serde_json::Value>,
}
```

The LLM receives clear information about what succeeded and what failed, allowing it to fix and retry.

---

## Validation Rules

### Entity Creation

1. `create_entity` MUST include at least one `Add` command (initial reference)
2. `create_entity` MUST include at least one `category_ids` (entities cannot be unclassified)
3. `parent_ids` can be empty ONLY for Domain scope entities
4. Reference type must match scope rules (see table above)
5. `Link`/`Unlink` commands only valid for Component/Unit scope
6. All `Add` commands must target the same document

### Code Reference Validation

When adding a code reference:

1. **Symbol validation** - Gnapsis queries LSP directly to validate symbol exists
   - Fail: `Symbol 'foo' not found in {path}`

2. **Range auto-populated** - `start_line`/`end_line` fetched from LSP automatically
   - If no LSP available for language, manual range required in the `Add` command

4. **Single document** - All `Add` commands in one entity operation must target the same file
   - Fail: `All references must be in same document. Got {path1} and {path2}`

### Reference Deletion

References cannot be deleted while attached:

```rust
// In alter_references
Delete { id: "ref-123" }
// Fails if attached:
// {
//   "error": "Reference attached to entities",
//   "context": {
//     "attached_entities": [
//       { "id": "ent-1", "name": "MyComponent" }
//     ]
//   }
// }
```

### Staleness Detection

A reference is stale when its line range overlaps with git diff hunks:

```rust
fn is_stale(ref: &Reference, hunks: &[DiffHunk]) -> bool {
    for hunk in hunks {
        let hunk_end = hunk.old_start + hunk.old_lines.saturating_sub(1);
        if ref.start_line <= hunk_end && ref.end_line >= hunk.old_start {
            return true;
        }
    }
    false
}
```

This is already implemented in `GitOps::is_in_changed_region`.

---

## Tool Design

### Entity Tools

#### `create_entity`

```rust
struct CreateEntityParams {
    name: String,
    description: String,
    category_ids: Vec<String>,       // Classification
    parent_ids: Vec<String>,         // BELONGS_TO (empty only for Domain)
    commands: Vec<EntityCommand>,    // Must include at least one Add
}

struct CreateEntityResult {
    entity: Entity,
    executed: Vec<ExecutedCommand>,
    failed: Option<FailedCommand>,
    skipped: Vec<EntityCommand>,
}
```

#### `update_entity`

```rust
struct UpdateEntityParams {
    entity_id: String,
    name: Option<String>,
    description: Option<String>,
    category_ids: Option<Vec<String>>,  // Replace classifications
    parent_ids: Option<Vec<String>>,    // Replace parents
    commands: Vec<EntityCommand>,
}

struct UpdateEntityResult {
    entity: Entity,
    executed: Vec<ExecutedCommand>,
    failed: Option<FailedCommand>,
    skipped: Vec<EntityCommand>,
}
```

#### `delete_entity`

Unchanged - fails if entity has children.

### Reference Tools

#### `alter_references`

Bulk update/delete references. **Auto-updates `commit_sha` to HEAD** on any Update command.

```rust
struct AlterReferencesParams {
    commands: Vec<ReferenceCommand>,
}

enum ReferenceCommand {
    Update {
        id: String,
        start_line: Option<u32>,
        end_line: Option<u32>,
        anchor: Option<String>,
        lsp_symbol: Option<String>,
    },
    Delete { id: String },
}

struct AlterReferencesResult {
    executed: Vec<ExecutedCommand>,
    failed: Option<FailedCommand>,
    skipped: Vec<ReferenceCommand>,
    commit_sha: String,  // Current HEAD after updates
}
```

**Behavior**:
- **Update**: Modifies reference fields, auto-sets `commit_sha = HEAD`
- **Delete**: Fails if reference is attached to any entity. Returns attached entities in error context.

### Analysis Tools

#### `analyze_document`

Replaces `lsp_analyze`, `validate_documents`, `get_document_references`. Returns detailed diff information for affected references.

```rust
struct AnalyzeDocumentParams {
    document_path: String,
    include_tracked: Option<bool>,    // Default: true
    include_untracked: Option<bool>,  // Default: true
    include_diffs: Option<bool>,      // Default: true - include git diff hunks
}

struct AnalyzeDocumentResult {
    document_path: String,
    document_type: String,  // "code" | "text"
    current_commit: String,

    // References in this document
    tracked: Vec<TrackedReference>,

    // LSP symbols not yet tracked (code files only)
    untracked: Vec<UntrackedSymbol>,

    // Entities that have references in this document
    entities: Vec<EntitySummary>,

    // Git diff hunks for this file (if changed since refs were recorded)
    diff_hunks: Vec<HunkInfo>,

    summary: AnalysisSummary,
}

struct TrackedReference {
    id: String,
    entity_id: String,
    entity_name: String,
    start_line: u32,
    end_line: u32,
    lsp_symbol: Option<String>,
    anchor: Option<String>,
    reference_commit: String,  // When this ref was last updated

    // Staleness info
    is_stale: bool,
    stale_reason: Option<String>,  // "lines_changed" | "symbol_missing" | "anchor_missing"
    affected_hunks: Vec<HunkInfo>, // Which hunks overlap this reference
}

struct HunkInfo {
    old_start: u32,
    old_lines: u32,
    new_start: u32,
    new_lines: u32,
    // Optionally include actual diff content for context
    context: Option<String>,
}

struct AnalysisSummary {
    tracked_count: usize,
    stale_count: usize,
    untracked_count: usize,
}
```

**Workflow**:
1. `get_changed_files()` → lightweight list of changed documents
2. `analyze_document(path)` → detailed analysis with diffs (queries LSP internally)
3. `alter_references(updates)` → fix stale refs, auto-update SHA

### Workflow

```
1. analyze_document(path)
   → Complete picture: tracked, untracked, stale, entities
   → Gnapsis queries LSP internally for symbol data

2. query(feature_id) or query(entity_id, relationship_types: ["BELONGS_TO"])
   → What entities exist for this area

3. LLM decides and executes:
   - create_entity with Add commands for new symbols
   - update_entity with Attach/Unattach for reorganization
   - alter_references with Update for stale refs
   - delete_entity for removed code
```

---

## Tool Consolidation

### Removed Tools (17)

| Old Tool | Replacement |
|----------|-------------|
| `add_belongs` | `parent_ids` argument on create/update_entity |
| `add_related` | `Relate` command |
| `add_link` | `Link` command |
| `add_references` | `Add` command |
| `remove_references` | `Unattach` command + `alter_references` Delete |
| `classify` | `category_ids` argument on create/update_entity |
| `unclassify` | N/A (entities must have categories) |
| `list_scopes` | `project_overview` (scopes are static, documented in instructions) |
| `list_categories` | `project_overview` |
| `get_project_stats` | `project_overview` |
| `lsp_analyze` | `analyze_document` |
| `validate_documents` | `analyze_document` |
| `get_document_references` | `analyze_document` |
| `lsp_refresh` | `alter_references` |
| `sync_references` | `alter_references` (auto-updates SHA on Update) |
| `get_composition_graph` | `query` with `relationship_types: ["BELONGS_TO"]` |
| `query_subgraph` | `query` (renamed, with semantic optimization) |
| `semantic_search` | `search` (unified) |
| `search_documents` | `search` (unified) |

### New Tools (5)

| Tool | Purpose |
|------|---------|
| `analyze_document` | Document analysis: tracked refs, untracked symbols, diffs, affected refs |
| `alter_references` | Bulk update/delete references (auto-updates SHA to HEAD) |
| `project_overview` | Full project context: taxonomy, entity hierarchy, stats |
| `search` | Unified semantic search across entities and references |
| `query` | Semantic subgraph extraction with relevance-based pruning |

### Final Tool List (16 tools)

| Category | Tools |
|----------|-------|
| **Project (2)** | `init_project`, `project_overview` |
| **Taxonomy (1)** | `create_category` |
| **Entity (3)** | `create_entity`, `update_entity`, `delete_entity` |
| **Reference (1)** | `alter_references` |
| **Query (5)** | `get_entity`, `find_entities`, `get_document_entities`, `query`, `search` |
| **Sync (1)** | `get_changed_files` |
| **Validation (1)** | `validate_graph` |
| **Analysis (1)** | `analyze_document` |

**Reduction: 29 → 16 tools (45%)**

---

## New Tool Definitions

### `project_overview`

Returns complete project context: taxonomy, entity hierarchy, and stats. Optionally generates a markdown skill file.

```rust
struct ProjectOverviewParams {
    /// Generate markdown skill file at this path (optional)
    output_path: Option<String>,
    /// Include full entity descriptions (default: false for summaries only)
    include_descriptions: Option<bool>,
}

struct ProjectOverviewResult {
    // Taxonomy
    categories: Vec<CategoryInfo>,

    // Entity hierarchy (summaries)
    domains: Vec<EntitySummary>,
    features: Vec<EntitySummary>,
    namespaces: Vec<EntitySummary>,

    // Stats
    stats: ProjectStats,

    // Generated skill file path (if output_path provided)
    skill_path: Option<String>,
}

struct CategoryInfo {
    id: String,
    name: String,
    scope: String,
    description: Option<String>,
}

struct EntitySummary {
    id: String,
    name: String,
    description: String,  // Truncated if include_descriptions=false
    parent_id: Option<String>,
    category: String,
}

struct ProjectStats {
    domains: usize,
    features: usize,
    namespaces: usize,
    components: usize,
    units: usize,
    references: usize,
}
```

**Note**: Scopes are static (Domain, Feature, Namespace, Component, Unit) and documented in MCP server instructions. No need to return them dynamically.

When `output_path` is provided, generates markdown:
```markdown
# Project Overview: {project_name}

## Purpose
{Domain entity description}

## Features
{List of Feature entities with descriptions}

## Architecture
{Key Component entities grouped by Namespace}

## Entry Points
{Unit entities marked as entry points}

## Key Files
{Most-referenced documents}
```

This gives new sessions instant context without expensive graph traversal.

### `search`

Unified semantic search across entities and references. Exhaustive and deterministic.

```rust
struct SearchParams {
    /// Natural language search query
    query: String,
    /// What to search: "entities", "references", or "all" (default)
    target: Option<SearchTarget>,
    /// Maximum results (default: 20)
    limit: Option<u32>,
    /// Minimum similarity score (default: 0.3)
    min_score: Option<f32>,
    /// Filter by scope (entities only)
    scope: Option<String>,
}

enum SearchTarget {
    Entities,
    References,
    All,
}

struct SearchResult {
    entities: Vec<EntityMatch>,
    references: Vec<ReferenceMatch>,
}

struct EntityMatch {
    id: String,
    name: String,
    description: String,
    score: f32,
    scope: String,
    categories: Vec<String>,
}

struct ReferenceMatch {
    id: String,
    entity_id: String,
    entity_name: String,
    document_path: String,
    start_line: u32,
    end_line: u32,
    description: String,
    score: f32,
}
```

### `query`

Semantic subgraph extraction with relevance-based pruning. Returns an optimized graph within budget constraints.

```rust
struct QueryParams {
    /// Starting entity (optional - if omitted, searches for best match)
    entity_id: Option<String>,
    /// Semantic query for relevance scoring (uses entity.description if omitted)
    semantic_query: Option<String>,
    /// Maximum nodes in result (default: 50)
    max_nodes: Option<u32>,
    /// Maximum estimated tokens (default: 4000)
    max_tokens: Option<u32>,
    /// Minimum relevance to include node (default: 0.3)
    min_relevance: Option<f32>,
    /// Scoring strategy: "global" or "branch_penalty" (default: "global")
    scoring_strategy: Option<ScoringStrategy>,
    /// Filter relationship types (e.g., ["BELONGS_TO", "CALLS"])
    relationship_types: Option<Vec<String>>,
    /// Open 3D visualization
    visualize: Option<bool>,
}

enum ScoringStrategy {
    /// Only global token accumulation affects scoring (simpler, may go deep)
    Global,
    /// Also penalize deep branches to encourage breadth
    BranchPenalty,
}

struct QueryResult {
    /// Starting entity used
    root_entity: EntitySummary,
    /// Graph nodes (entities)
    nodes: Vec<GraphNode>,
    /// Graph edges (relationships)
    edges: Vec<GraphEdge>,
    /// Statistics
    stats: QueryStats,
}

struct GraphNode {
    id: String,
    name: String,
    description: String,
    scope: String,
    relevance: f32,
    references: Vec<ReferenceSummary>,
}

struct GraphEdge {
    from_id: String,
    to_id: String,
    relationship: String,  // "BELONGS_TO", "RELATED_TO", "CALLS", etc.
    note: Option<String>,  // For RELATED_TO
    relevance: f32,
}

struct QueryStats {
    nodes_visited: usize,
    nodes_pruned: usize,
    estimated_tokens: usize,
}
```

**Behavior:**

| entity_id | semantic_query | Behavior |
|-----------|----------------|----------|
| ✅ | ❌ | Use entity.description as query, expand |
| ✅ | ✅ | Expand from entity, filter by query |
| ❌ | ✅ | Search best match, expand from there |
| ❌ | ❌ | Error |

**Algorithm: Best-First Search with budget-aware scoring**

```rust
fn semantic_subgraph(start: Entity, query: String, params: QueryParams) -> Subgraph {
    let mut pq = PriorityQueue::new();
    let mut visited = HashSet::new();
    let mut total_tokens = 0;

    pq.push(start, initial_score);

    while let Some((node, _)) = pq.pop() {
        if visited.contains(&node.id) { continue; }
        if total_tokens + node.tokens > params.max_tokens { continue; }

        visited.insert(node.id);
        total_tokens += node.tokens;

        for neighbor in node.neighbors() {
            if visited.contains(&neighbor.id) { continue; }
            let score = score_node(neighbor, query, total_tokens, params);
            if score > params.min_relevance {
                pq.push(neighbor, score);
            }
        }
    }

    build_subgraph(visited)
}
```

**Scoring strategies (configurable):**

```rust
enum ScoringStrategy {
    /// Simple: only global accumulation affects threshold
    GlobalOnly,
    /// Branch-aware: penalize deep branches to encourage breadth
    BranchPenalty,
}

fn score_node(node, query, state, params) -> f32 {
    let relevance = embedding_similarity(node, query);
    let node_cost = estimate_tokens(node);

    // Global factor: bar rises as context fills
    let global_factor = 1.0 / (1.0 + state.total_tokens / params.max_tokens);

    match params.scoring_strategy {
        GlobalOnly => {
            relevance * global_factor / node_cost
        }
        BranchPenalty => {
            // Additional penalty for deep branches
            let branch_factor = 1.0 / (1.0 + state.branch_tokens / BRANCH_BUDGET);
            relevance * global_factor * branch_factor / node_cost
        }
    }
}
```

**Trade-offs:**
- `GlobalOnly`: simpler, lets relevance drive selection, may go deep on one branch
- `BranchPenalty`: forces breadth, more diverse context, prevents tunnel vision

Both respect the global budget. Evaluate empirically which produces better context.

---

## Implementation Plan

### Phase 1: Schema Updates

1. Update DocumentReference model
   - Remove separate `lsp_range_start`/`lsp_range_end` (use `start_line`/`end_line` from LSP)
   - Add `anchor` field for text references
2. Neo4j migration for schema changes
3. Update DocumentRepository

### Phase 2: Command Execution Engine

1. Implement `EntityCommand` enum and execution
2. Implement partial execution with failure reporting
3. Add `FailedCommand` context (attached entities, etc.)
4. Unit tests for execution engine

### Phase 3: Entity Tools Redesign

1. Rewrite `create_entity` with command model
   - `parent_ids` argument (empty only for Domain)
   - Command validation (Link only for Component/Unit)
   - Must have at least one `Add` command
2. Rewrite `update_entity` with command model
3. Add LSP validation for code references

### Phase 4: Reference Tools

1. Implement `alter_references` bulk tool
   - Update and Delete commands
   - Auto-update `commit_sha` to HEAD on Update
   - Delete validation (fail if attached)
2. Remove old tools: `add_references`, `remove_references`, `sync_references`

### Phase 5: Analysis Tool

1. Implement `analyze_document` unified tool
   - Tracked references with staleness info
   - Untracked LSP symbols
   - Git diff hunks with affected references
   - Line-level staleness via hunk overlap
2. Remove old tools: `lsp_analyze`, `validate_documents`, `get_document_references`

### Phase 6: Relationship Consolidation

1. Remove `add_belongs` (now `parent_ids`)
2. Remove `add_related` (now `Relate` command)
3. Remove `add_link` (now `Link` command)
4. Add embedding for RELATED_TO notes
5. Update MCP server instructions

### Phase 7: Search & Query Tools

1. Implement unified `search` tool
   - Search entities by description embedding
   - Search references by description embedding
   - Target filter: entities, references, or all
2. Implement `query` tool with semantic subgraph
   - BFS expansion with relevance scoring
   - Pruning based on min_relevance threshold
   - Budget constraints (max_nodes, max_tokens)
   - Use entity.description if no semantic_query
   - Find best starting entity if no entity_id
3. Remove old tools: `semantic_search`, `search_documents`, `query_subgraph`, `get_composition_graph`

### Phase 8: Project Overview

1. Implement `project_overview` tool
2. Create skill template
3. Test with current graph

### Phase 9: Migration & Cleanup

1. Migration for existing entities (add warning for entities without refs)
2. Update `validate_graph` to flag entities without references
3. Update all documentation

---

## Open Questions (Resolved)

1. **Anchor format**: Should anchors be markdown-heading specific (`## Heading`) or support other formats?
   - **RESOLVED**: Markdown-heading format (`## Heading`, `### Subheading`). This covers 95% of documentation use cases. The anchor is optional - line ranges work without anchors. Future extensions (HTML anchors, comment markers) can be added if needed.

2. **Code file detection**: How to reliably detect if a file is code vs text?
   - **RESOLVED**: Use `content_type` field prefix. If starts with `code:` (e.g., `code:rust`, `code:typescript`), it's code. Otherwise it's text (`markdown`, `text`, etc.). This is already enforced when creating references.

3. **Backward compatibility**: Should we migrate existing entities without references, or leave them and flag in `validate_graph`?
   - **RESOLVED**: Flag in `validate_graph` as a new validation type (`find_entities_without_references`). LLM can fix incrementally. This is safer than automatic migration and gives visibility into the issue.

4. **LSP symbol matching**: Exact match or fuzzy?
   - **RESOLVED**: Exact match on `lsp_symbol` name. The LLM receives exact symbols from nvim, so matching should be exact. If symbol renamed, reference becomes stale and `analyze_document` reports `symbol_missing`.

5. **Classification handling**: Should `category_ids` in create/update replace or merge?
   - **RESOLVED**: Replace semantics. `category_ids` in `update_entity` replaces all existing classifications. To keep existing categories, omit the field (it's `Option<Vec<String>>`). This is simpler and matches how `parent_ids` works.

---

## References

- [DES-001: Taxonomy Tree](001-taxonomy-tree.md)
- [DES-002: MCP Tools API](002-mcp-tools.md)
- [LSP SymbolKind Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#symbolKind)

## Related Documents

- EPIC-001: Gnapsis - Code Intelligence Graph (completed)
- EPIC-002: Ontology Improvements (this design)
