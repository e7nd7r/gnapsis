---
title: "DES-002: Taxonomy Tree for Code and Document Classification"
date: 2025-01-15
status: draft
tags: [design, taxonomy, neo4j, classification, ontology]
---

# DES-002: Taxonomy Tree for Code and Document Classification

## Problem

We need a classification system to organize code and documents within a project's ontology graph. The system must support:

1. Hierarchical composition (how software is structured)
2. Flexible relationships (entities can relate to multiple higher-order categories)
3. Extensible vocabulary (new categories can be added at each scope)

## Conceptual Framework

### Why Not Biological Taxonomy Directly?

Biological taxonomy (Domain → Kingdom → Phylum → Class → Order → Family → Genus → Species) classifies organisms by **what they ARE** (IS-A relationships). This is an **ontological** approach based on inheritance and type hierarchy.

Software structure is different. We don't primarily care what something IS, but rather **how it's composed** and **where it belongs**. A function isn't a "child species" of a class - it's a **component contained within** a class.

### IS-A vs COMPOSED-OF

| Approach | Relationship | Example | Use Case |
|----------|--------------|---------|----------|
| **IS-A (Ontological)** | Inheritance | `Dog IS-A Animal` | OOP class hierarchies |
| **COMPOSED-OF (Compositional)** | Containment | `Module CONTAINS Functions` | Software structure |

OOP uses IS-A for class relationships, but even in OOP, the structural organization of code (files, modules, namespaces) follows COMPOSED-OF patterns.

### The Compositional Tree

We define an abstract **compositional tree** that describes how software artifacts nest:

```
Domain (business boundary)
  └── Feature (capability type)
        └── Namespace (organizational container)
              └── Component (type-level construct)
                    └── Unit (member-level construct)
```

This is a **grammar for structure**, not a taxonomy of types. It answers: "What can contain what?"

### Scopes and Categories

Borrowing from biological taxonomy terminology but adapting it:

- **Scope** (replaces "Rank"): A level in the compositional hierarchy (Domain, Feature, Namespace, Component, Unit)
- **Category** (replaces "Taxon"): A classification value at a specific scope (e.g., "orchestration" at Domain scope, "struct" at Component scope)

### Why Directed Graph, Not Strict Tree?

An entity can belong to multiple categories at the same scope:

- A `Logger` utility might belong to both `orchestration` and `infrastructure` domains
- A `Config` struct might be `functional` and `cross-cutting`

The taxonomy defines valid scopes and categories. The actual classification graph allows many-to-many relationships.

## Taxonomy Structure

### Scopes (Fixed)

The abstract tree has 5 scopes representing compositional hierarchy:

```
Domain
  └── Feature
        └── Namespace
              └── Component
                    └── Unit
```

### Categories (Extensible vocabulary at each scope)

| Scope | Description | Example Categories |
|-------|-------------|-------------------|
| **Domain** | Business/technical domain boundary | `orchestration`, `core-api`, `auth`, `payments`, `infrastructure`, `cli` |
| **Feature** | Type of capability | `functional`, `non-functional`, `technical`, `cross-cutting` |
| **Namespace** | Organizational container | `module`, `library`, `namespace`, `package`, `crate` |
| **Component** | Type-level construct | `class`, `struct`, `trait`, `interface`, `enum`, `component`, `type` |
| **Unit** | Member-level construct | `function`, `method`, `property`, `attribute`, `variable`, `constant`, `field`, `call` |

## Identifiers

All entities use **ULID** (Universally Unique Lexicographically Sortable Identifier):

- Lexicographically sortable (alphabetical order = chronological order)
- Timestamp-prefixed (first 10 chars encode millisecond timestamp)
- Generated in application (Rust `ulid` crate), passed to Neo4j

Benefits:
- **Temporal queries**: `WHERE id > '01HQ...'` = "created after X"
- **Graph reconstruction**: Replay graph state at any point in time
- **Evolution tracking**: Combined with `commit_sha`, correlate graph changes with git history

```
ULID: 01ARZ3NDEKTSV4RRFFQ69G5FAV
      |---------||-----------|
      timestamp   randomness
      (48 bits)   (80 bits)
```

## Neo4j Graph Model

### Schema

```cypher
// Scope - a level in the compositional hierarchy (fixed set of 5)
(:Scope {
    name: String,           // "Domain", "Feature", "Namespace", "Component", "Unit"
    depth: Int,             // 1-5 (hierarchy depth)
    description: String
})

// Category - a classification value at a specific scope
(:Category {
    id: UUID,
    name: String,           // e.g., "orchestration", "struct", "method"
    description: String?,   // human-readable explanation of this category
    created_at: DateTime
})

// Category belongs to a Scope
(:Category)-[:IN_SCOPE]->(:Scope)

// Scope hierarchy (compositional structure)
(:Scope)-[:COMPOSES]->(:Scope)
// Domain -[:COMPOSES]-> Feature -[:COMPOSES]-> Namespace -[:COMPOSES]-> Component -[:COMPOSES]-> Unit

// Document - a file in the repository (source of truth)
(:Document {
    id: UUID,
    path: String,           // relative path from repo root
    content_hash: String,   // for change detection
    created_at: DateTime,
    updated_at: DateTime
})

// DocumentReference - a reference to a location in a document
(:DocumentReference {
    id: UUID,
    offset: Int,            // byte offset in file
    start_line: Int,
    end_line: Int,
    commit_sha: String,     // git commit - enables: git show {sha}:{path}
    content_type: String,   // "code" or "text"
    description: String,    // what this code/text does (used for embedding)
    embedding: [Float]?,    // semantic embedding of the description

    // LSP integration (optional - present if tracked by LSP)
    lsp_symbol: String?,    // fully qualified symbol name (e.g., "scheduler::Scheduler::tick")
    lsp_kind: Int?,         // LSP SymbolKind enum value (6=method, 12=function, 5=class, etc.)
    lsp_range: String?,     // LSP range as "startLine:startChar-endLine:endChar"

    created_at: DateTime,
    updated_at: DateTime
})

// Note: All embeddings are generated from descriptions (natural language),
// so a single text embedding model can be used consistently.

// Entity - a classified code/doc element
(:Entity {
    id: UUID,
    name: String,           // e.g., "Scheduler", "tick"
    description: String,    // text used for embedding generation
    embedding: [Float]?,    // vector embedding for semantic search
    created_at: DateTime
})
```

### Relationships

```cypher
// Entity has 0..n document references
(:Entity)-[:HAS_REFERENCE]->(:DocumentReference)
(:DocumentReference)-[:IN_DOCUMENT]->(:Document)

// Entity classified by Category (determines Scope)
(:Entity)-[:CLASSIFIED_AS]->(:Category)

// Composition - child entity belongs to parent entity
(:Entity)-[:BELONGS_TO {
    note: String?,          // context for the relationship
    embedding: [Float]?     // semantic embedding of the note
}]->(:Entity)

// General relation between entities
(:Entity)-[:RELATED_TO {
    note: String?,          // context for the relationship
    embedding: [Float]?     // semantic embedding of the note
}]->(:Entity)

// Category hierarchy within a scope (subtypes)
(:Category)-[:SUBTYPE_OF {
    note: String?,          // why this is a subtype (e.g., "async variant of function")
    embedding: [Float]?
}]->(:Category)

// Cross-references between entities
(:Entity)-[:CALLS {
    note: String?,          // context (e.g., "calls on error", "delegates to")
    embedding: [Float]?
}]->(:Entity)

(:Entity)-[:IMPORTS {
    note: String?,
    embedding: [Float]?
}]->(:Entity)

(:Entity)-[:IMPLEMENTS {
    note: String?,          // context (e.g., "partial implementation", "via delegation")
    embedding: [Float]?
}]->(:Entity)

(:Entity)-[:INSTANTIATES {
    note: String?,
    embedding: [Float]?
}]->(:Entity)
```

### Constraints

Neo4j built-in constraints only support property validation, not cross-node relationships. We use APOC triggers to enforce the `BELONGS_TO` scope constraint.

**Requirement:** `apoc.trigger.enabled=true` in `neo4j.conf`

```cypher
// Constraint: child scope depth must be greater than parent scope depth
// A Unit can belong to a Component, but not vice versa
CALL apoc.trigger.add('validate_belongs_to',
  "UNWIND $createdRelationships AS r
   WITH r
   WHERE type(r) = 'BELONGS_TO'
   MATCH (child)-[r]->(parent)
   MATCH (child)-[:CLASSIFIED_AS]->(:Category)-[:IN_SCOPE]->(cs:Scope)
   MATCH (parent)-[:CLASSIFIED_AS]->(:Category)-[:IN_SCOPE]->(ps:Scope)
   CALL apoc.util.validate(
     cs.depth <= ps.depth,
     'Invalid BELONGS_TO: %s (depth %d) cannot belong to %s (depth %d)',
     [child.name, cs.depth, parent.name, ps.depth]
   )
   RETURN null",
  {phase: 'before'})
```

This fails the transaction with a descriptive error if violated.

```cypher
// Constraint: Entity can have at most ONE category per scope
CALL apoc.trigger.add('validate_single_category_per_scope',
  "UNWIND $createdRelationships AS r
   WITH r
   WHERE type(r) = 'CLASSIFIED_AS'
   MATCH (e:Entity)-[r]->(newCat:Category)-[:IN_SCOPE]->(s:Scope)
   MATCH (e)-[:CLASSIFIED_AS]->(existingCat:Category)-[:IN_SCOPE]->(s)
   WHERE existingCat <> newCat
   CALL apoc.util.validate(
     true,
     'Entity %s already has category %s at scope %s, cannot add %s',
     [e.name, existingCat.name, s.name, newCat.name]
   )
   RETURN null",
  {phase: 'before'})
```

```cypher
// Constraint: Cannot delete entity that has children
CALL apoc.trigger.add('prevent_delete_with_children',
  "UNWIND $deletedNodes AS n
   WITH n
   WHERE n:Entity
   MATCH (child:Entity)-[:BELONGS_TO]->(n)
   CALL apoc.util.validate(
     true,
     'Cannot delete entity %s: has children',
     [n.name]
   )
   RETURN null",
  {phase: 'before'})
```

### Deletion Behavior

When an entity is deleted (after passing the children check):

| Relationship | Behavior |
|--------------|----------|
| `HAS_REFERENCE` → DocumentReference | Cascade delete (orphan cleanup) |
| `CLASSIFIED_AS` | Cascade delete relationship |
| `BELONGS_TO` (as child) | Cascade delete relationship |
| `RELATED_TO`, `CALLS`, `IMPORTS`, etc. | Cascade delete relationships |

```cypher
// Cascade cleanup on entity deletion
CALL apoc.trigger.add('cascade_delete_entity',
  "UNWIND $deletedNodes AS n
   WITH n
   WHERE n:Entity
   OPTIONAL MATCH (n)-[:HAS_REFERENCE]->(ref:DocumentReference)
   DETACH DELETE ref",
  {phase: 'after'})
```

### Change Detection

Using git commits to track what needs updating:

```cypher
// Find entities with references in changed documents
MATCH (e:Entity)-[:HAS_REFERENCE]->(ref:DocumentReference)-[:IN_DOCUMENT]->(d:Document {path: $changedPath})
WHERE ref.commit_sha = $oldCommit
RETURN e.id, e.name, ref.start_line, ref.end_line

// Retrieve exact content from git (externally)
// git show {ref.commit_sha}:{d.path} | sed -n '{ref.start_line},{ref.end_line}p'

// After re-parsing, update document reference
MATCH (ref:DocumentReference {id: $refId})
SET ref.offset = $newOffset,
    ref.start_line = $newStartLine,
    ref.end_line = $newEndLine,
    ref.commit_sha = $newCommit,
    ref.updated_at = datetime()
```

Workflow:
1. `git diff oldCommit..newCommit` → get changed files
2. Query entities with references pointing to those documents
3. Re-parse changed files, update DocumentReference nodes with new commit

## Classification Example

Given this Rust code:

```rust
// src/scheduler/mod.rs
pub struct Scheduler {
    backend: Box<dyn StorageBackend>,
}

impl Scheduler {
    pub async fn tick(&self) -> Result<()> {
        // ...
    }
}
```

The graph structure:

```
Document: {path: "src/scheduler/mod.rs", content_hash: "abc123"}
    ▲
    │ IN_DOCUMENT
    │
DocumentReference: {offset: 0, start_line: 1, end_line: 12, commit_sha: "abc123"}
    ▲
    │ HAS_REFERENCE
    │
Entity: "scheduler"
├── CLASSIFIED_AS → Category: "module" (Namespace)
│
├── Entity: "Scheduler" ─[BELONGS_TO]→ "scheduler"
│   ├── HAS_REFERENCE → DocumentReference {start_line: 2, end_line: 4} → Document
│   ├── CLASSIFIED_AS → Category: "struct" (Component)
│   │
│   └── Entity: "tick" ─[BELONGS_TO]→ "Scheduler"
│       ├── HAS_REFERENCE → DocumentReference {start_line: 7, end_line: 10} → Document
│       └── CLASSIFIED_AS → Category: "method" (Unit)
```

## Multi-Classification

An entity can be classified by multiple categories, even at the same scope:

```cypher
// A logging utility might belong to multiple domains
(e:Entity {name: "Logger"})
  -[:CLASSIFIED_AS]->(:Category {name: "orchestration"})
  -[:CLASSIFIED_AS]->(:Category {name: "infrastructure"})
  -[:CLASSIFIED_AS]->(:Category {name: "cross-cutting"})
```

## Queries

### Find all methods in orchestration domain

```cypher
MATCH (e:Entity)-[:CLASSIFIED_AS]->(:Category {name: "orchestration"})
MATCH (e)-[:CLASSIFIED_AS]->(:Category {name: "method"})
MATCH (e)-[:HAS_REFERENCE]->(ref:DocumentReference)-[:IN_DOCUMENT]->(doc:Document)
RETURN e.name, doc.path, ref.start_line, ref.end_line
```

### Navigate composition hierarchy

```cypher
// Get ancestry path for an entity (upward)
MATCH path = (e:Entity {name: "tick"})-[:BELONGS_TO*]->(ancestor:Entity)
RETURN [n IN nodes(path) | n.name] AS hierarchy

// Get descendants of an entity (downward)
MATCH path = (child:Entity)-[:BELONGS_TO*]->(e:Entity {name: "scheduler"})
RETURN [n IN nodes(path) | n.name] AS descendants
```

### Find entities with same classification pattern

```cypher
// Find all structs in the orchestration domain
MATCH (e:Entity)-[:CLASSIFIED_AS]->(:Category {name: "orchestration"})
MATCH (e)-[:CLASSIFIED_AS]->(:Category {name: "struct"})
MATCH (e)-[:HAS_REFERENCE]->(:DocumentReference)-[:IN_DOCUMENT]->(doc:Document)
RETURN e.name, doc.path
```

### Find all entities in a document

```cypher
MATCH (e:Entity)-[:HAS_REFERENCE]->(ref:DocumentReference)-[:IN_DOCUMENT]->(d:Document {path: $path})
RETURN e.name, ref.start_line, ref.end_line
ORDER BY ref.start_line
```

## API Design

### MCP Tools

```rust
/// Create a new category at a scope
#[tool(description = "Create a new category (classification value) at a scope")]
async fn create_category(
    name: String,
    scope: String,          // "Domain", "Feature", "Namespace", "Component", "Unit"
    description: Option<String>,
) -> Result<CallToolResult, McpError>;

/// Classify an entity with a category
#[tool(description = "Classify an entity with a category")]
async fn classify(
    entity_id: String,
    category_name: String,
) -> Result<CallToolResult, McpError>;

/// List categories at a scope
#[tool(description = "List all categories at a specific scope")]
async fn list_categories(
    scope: String,
) -> Result<CallToolResult, McpError>;

/// Get classification for an entity
#[tool(description = "Get all classifications for an entity")]
async fn get_classifications(
    entity_id: String,
) -> Result<CallToolResult, McpError>;
```

## Initialization

When a project is created, seed the default scopes and categories:

```cypher
// Create scopes
CREATE (domain:Scope {name: "Domain", depth: 1, description: "Business/technical domain boundary"})
CREATE (feature:Scope {name: "Feature", depth: 2, description: "Type of capability"})
CREATE (namespace:Scope {name: "Namespace", depth: 3, description: "Organizational container"})
CREATE (component:Scope {name: "Component", depth: 4, description: "Type-level construct"})
CREATE (unit:Scope {name: "Unit", depth: 5, description: "Member-level construct"})

// Wire up scope hierarchy
CREATE (domain)-[:COMPOSES]->(feature)
CREATE (feature)-[:COMPOSES]->(namespace)
CREATE (namespace)-[:COMPOSES]->(component)
CREATE (component)-[:COMPOSES]->(unit)

// Seed common categories
// Domain
MATCH (s:Scope {name: "Domain"})
CREATE (:Category {name: "core"})-[:IN_SCOPE]->(s)
CREATE (:Category {name: "infrastructure"})-[:IN_SCOPE]->(s)

// Feature
MATCH (s:Scope {name: "Feature"})
CREATE (:Category {name: "functional"})-[:IN_SCOPE]->(s)
CREATE (:Category {name: "non-functional"})-[:IN_SCOPE]->(s)
CREATE (:Category {name: "technical"})-[:IN_SCOPE]->(s)

// Namespace
MATCH (s:Scope {name: "Namespace"})
CREATE (:Category {name: "module"})-[:IN_SCOPE]->(s)
CREATE (:Category {name: "library"})-[:IN_SCOPE]->(s)

// Component
MATCH (s:Scope {name: "Component"})
CREATE (:Category {name: "class"})-[:IN_SCOPE]->(s)
CREATE (:Category {name: "struct"})-[:IN_SCOPE]->(s)
CREATE (:Category {name: "trait"})-[:IN_SCOPE]->(s)
CREATE (:Category {name: "interface"})-[:IN_SCOPE]->(s)
CREATE (:Category {name: "enum"})-[:IN_SCOPE]->(s)

// Unit
MATCH (s:Scope {name: "Unit"})
CREATE (:Category {name: "function"})-[:IN_SCOPE]->(s)
CREATE (:Category {name: "method"})-[:IN_SCOPE]->(s)
CREATE (:Category {name: "property"})-[:IN_SCOPE]->(s)
CREATE (:Category {name: "field"})-[:IN_SCOPE]->(s)
CREATE (:Category {name: "constant"})-[:IN_SCOPE]->(s)
```

## Libraries

### Git Integration

**`git2`** - Rust bindings to libgit2 for repository operations.

```toml
git2 = "0.19"
```

```rust
use git2::Repository;

// Open repo
let repo = Repository::open(&project.repo_path)?;

// Get file content at specific commit
let commit = repo.find_commit(oid)?;
let tree = commit.tree()?;
let entry = tree.get_path(Path::new("src/main.rs"))?;
let blob = repo.find_blob(entry.id())?;
let content = std::str::from_utf8(blob.content())?;

// Diff between commits (for change detection)
let diff = repo.diff_tree_to_tree(Some(&old_tree), Some(&new_tree), None)?;
diff.foreach(&mut |delta, _| {
    println!("Changed: {:?}", delta.new_file().path());
    true
}, None, None, None)?;

// Get HEAD sha
let head_sha = repo.head()?.peel_to_commit()?.id().to_string();
```

### Neo4j

**`neo4rs`** - Async Neo4j driver.

```toml
neo4rs = "0.8"
```

### Embeddings

**`raggy`** - Embedding abstraction layer supporting multiple providers.

```toml
raggy = { git = "https://github.com/e7nd7r/raggy", features = ["fastembed"] }
```

Enables configurable embedding backends (fastembed for local, OpenAI, etc.) without code changes.

### Identifiers

**`ulid`** - ULID generation.

```toml
ulid = "1"
```

### MCP Server

**`rmcp`** - Rust MCP SDK.

```toml
rmcp = { version = "0.1", features = ["server", "macros", "transport-io"] }
```

## Open Questions

None - all constraints defined.
