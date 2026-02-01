---
name: build-knowledge-graph
description: Reverse engineer codebase architecture and build a knowledge graph using gnapsis MCP tools. Use when the user wants to map domains, features, modules, components, and their relationships.
argument-hint: <optional-scope-or-path>
---

# Build Knowledge Graph

Reverse engineer the architecture of the codebase and build a comprehensive knowledge graph
using the gnapsis MCP tools, following a strict derivation order methodology.

If an argument is provided, focus the analysis on that scope or path. Otherwise, analyze
the entire codebase.

---

## CRITICAL RULES

1. **Always use gnapsis MCP tools** to register all nodes and relationships in the knowledge graph. The gnapsis graph is the primary output.
2. **Proceed through phases, summarize at each phase boundary.** Complete each phase fully, then summarize what was done before moving to the next.
3. **Use best judgment for ambiguity**: When you encounter ambiguity about business domains, feature boundaries, or architectural decisions, use your best technical judgment and document your reasoning. Add a note in the entity description when a decision was ambiguous.
4. **ALWAYS run `analyze_document` before creating entities** for a source file. This gives you the exact LSP symbol names. Never guess symbol names.
5. **Use `ref_type: "code"` with `lsp_symbol` for source files**. Only use `ref_type: "text"` for markdown, docs, and config files.
6. **Be exhaustive, not superficial**. Scan every directory, every configuration file, every module entry point. Leave no stone unturned.

---

## GNAPSIS TOOL REFERENCE

### Initialization & Discovery
- `init_project` — Initialize the database schema (run once at start)
- `project_overview` — Get current ontology: taxonomy (categories by scope), entity hierarchy, statistics

### Entity Lifecycle
- `create_entity(name, description, category_ids, parent_ids, commands)` — Create an entity with at least one reference
- `update_entity(entity_id, ...)` — Update entity, add/remove references, create relationships
- `delete_entity(entity_id)` — Delete entity (must have no children)

### Taxonomy
- `create_category(name, scope)` — Create a new category at a scope (if the defaults don't fit)

### Querying
- `get_entity(entity_id)` — Full entity details with references and relationships
- `find_entities(scope, category, parent_id)` — Filter entities by scope/category/parent
- `search(query)` — Semantic search across entities and references
- `query(entity_id, semantic_query)` — Extract relevant subgraph within token budget
- `get_document_entities(document_path)` — Get all entities referenced in a file

### Document Analysis
- `analyze_document(document_path)` — **CRITICAL**: Discover tracked refs, untracked LSP symbols, and git diffs. Run this BEFORE creating entities for any source file.

### Maintenance
- `alter_references(commands)` — Bulk update/delete references
- `validate_graph()` — Check for orphans, cycles, scope violations, missing references
- `get_changed_files()` — Find files modified since last sync

### Entity Commands (used in create_entity/update_entity `commands` array)
- `{ type: "add", ref_type: "code", document_path: "...", lsp_symbol: "...", description: "..." }` — For source files
- `{ type: "add", ref_type: "text", document_path: "...", start_line: N, end_line: M, description: "..." }` — For docs/config
- `{ type: "relate", entity_id: "...", note: "..." }` — Create RELATED_TO relationship
- `{ type: "link", entity_id: "...", link_type: "calls|imports|implements|instantiates" }` — Code links (Component/Unit only)

### Default Categories (from `project_overview`)

| Scope | Categories |
|-------|-----------|
| Domain | `core`, `infrastructure` |
| Feature | `functional`, `technical`, `non-functional` |
| Namespace | `module`, `library` |
| Component | `struct`, `trait`, `enum`, `class` |
| Unit | `function`, `method`, `constant`, `field` |

---

## MANDATORY WORKFLOW: Creating Entities from Source Files

```
1. analyze_document(document_path: "src/foo.rs")
   → Returns untracked[] with exact LSP symbol names

2. create_entity(
     name: "FooService",
     description: "Service for foo operations",
     category_ids: ["<struct-category-id>"],
     parent_ids: ["<parent-namespace-id>"],
     commands: [{
       type: "add",
       ref_type: "code",
       document_path: "src/foo.rs",
       lsp_symbol: "FooService",        ← MUST match analyze_document output
       description: "FooService struct"
     }]
   )
```

**NEVER skip `analyze_document`. NEVER guess `lsp_symbol` names.**

---

## PHASE 1: STACK IDENTIFICATION

Before anything else, build a complete understanding of the project's technology stack.

### Steps:
1. **Scan the root directory** for configuration files: package.json, Cargo.toml, go.mod, pom.xml, build.gradle, requirements.txt, pyproject.toml, Gemfile, composer.json, Makefile, Dockerfile, docker-compose.yml, CI/CD configs, etc.
2. **Identify the primary language(s)** and their versions.
3. **Catalog all libraries and frameworks** with their roles (web framework, ORM, testing, logging, auth, etc.).
4. **Map the persistence layer**: databases, migration tools, ORM configurations.
5. **Identify service dependencies**: external APIs, microservice connections, message brokers, cloud services.
6. **Identify infrastructure patterns**: containerization, orchestration, CI/CD pipelines, deployment targets.
7. **Identify architectural style**: monolith, microservices, modular monolith, serverless, event-driven, hexagonal, clean architecture, MVC, CQRS, etc.

### Gnapsis Setup:
1. Run `project_overview` to get the current ontology state and available category IDs.
2. Note all category IDs — you will need them for every `create_entity` call.

### Output:
Summarize findings to the user. Optionally write a `docs/stack.md` if the user requests documentation.

After summarizing, proceed to Phase 2.

---

## PHASE 2: KNOWLEDGE GRAPH CONSTRUCTION (Gnapsis Derivation Order)

Build the knowledge graph layer by layer following the strict gnapsis scope hierarchy:

```
Domain → Feature → Namespace → Component → Unit
```

### Scope Definitions:
- **Domain**: A bounded context representing a major business or technical concern (e.g., Authentication, Graph Abstraction, MCP Server).
- **Feature**: A cohesive capability within a domain (e.g., within Auth: Login, Registration, Password Reset).
- **Namespace**: A code module or package that implements part of a feature (e.g., `services`, `repositories`, `mcp::tools`).
- **Component**: A concrete code artifact — struct, trait, class, enum (e.g., UserService, AuthTrait).
- **Unit**: An atomic functional element — function, method, constant, field (e.g., `validate()`, `MAX_RETRIES`).

### Process for EACH Domain:

#### Step 2.1: Domain Discovery
- Analyze directory structure, namespace patterns, and module boundaries.
- Look for domain indicators: directory names, namespace prefixes, bounded context markers, configuration sections.
- Cross-reference with the stack analysis to understand framework-specific conventions.
- Register each domain using `create_entity` with a Domain-scope category.

#### Step 2.2: Domain Summary
For each identified domain, briefly log:
- Domain name and description
- Constituent features (enumerated)
- Key entry points and interfaces
- Dependencies on other domains (preliminary)

Then proceed immediately to build the subgraph.

#### Step 2.3: Full Domain Subgraph Construction
Build the complete subgraph for each domain:

1. **Register Features** under the domain. For each feature:
   - Use `create_entity` with Feature-scope category and `parent_ids: [<domain-id>]`.
   - Identify all code paths that implement this feature.

2. **Register Namespaces** under each feature. For each namespace:
   - Use `create_entity` with Namespace-scope category and `parent_ids: [<feature-id>]`.
   - Map module boundaries and imports/exports.

3. **Register Components** under each namespace. For each component:
   - **First** run `analyze_document(document_path)` to discover LSP symbols.
   - Use `create_entity` with Component-scope category, `parent_ids: [<namespace-id>]`, and a code reference using the exact `lsp_symbol` from `analyze_document`.

4. **Register Units** under each component. For each unit:
   - Use `create_entity` with Unit-scope category and `parent_ids: [<component-id>]`.
   - Reference the exact LSP symbol from `analyze_document`.

5. **Register relationships** using `update_entity` commands:
   - `{ type: "relate", entity_id: "...", note: "..." }` for semantic relationships.
   - `{ type: "link", entity_id: "...", link_type: "calls" }` for code-level links.

#### Step 2.4: Repeat for All Domains
Proceed domain by domain. After completing all domains, summarize progress and proceed to Phase 3.

---

## PHASE 3: INTER-DOMAIN RELATIONSHIP ANALYSIS AND OPTIMIZATION

After all domain subgraphs are built:

### Step 3.1: Cross-Domain Dependency Scan
- Trace all imports, calls, events, and data flows that cross domain boundaries.
- Identify shared models, common utilities, and cross-cutting concerns.
- Map integration points: API calls between domains, shared database tables, event bus topics.

### Step 3.2: Register Inter-Domain Relationships
For each cross-domain relationship, use `update_entity` with:
- `{ type: "relate", entity_id: "<target>", note: "description of relationship" }` for semantic links.
- `{ type: "link", entity_id: "<target>", link_type: "calls|imports|implements|instantiates" }` for code links.

### Step 3.3: Coupling and Cohesion Analysis
- **Cohesion check**: Are all elements within each domain/feature/namespace closely related? Flag low-cohesion areas.
- **Coupling check**: Are inter-domain dependencies minimal and well-defined? Flag high-coupling areas.
- **Identify architectural smells**: circular dependencies, god modules, feature envy, shotgun surgery.

### Step 3.4: Graph Validation
Run `validate_graph()` to check for:
- Orphan entities (no parent at non-Domain scope)
- Cycles in BELONGS_TO relationships
- Scope violations (child scope not deeper than parent)
- Entities without references
- Entities without classification

Fix any issues found.

---

## PROGRESS TRACKING

- At the start of each phase, announce what you're about to do.
- After completing each domain, summarize what was registered in the graph (entity counts by scope).
- After completing all phases, provide a final summary.

---

## QUALITY ASSURANCE

Before declaring completion:
1. Run `validate_graph()` and fix all reported issues.
2. Verify every source file has been analyzed via `analyze_document`.
3. Cross-check the graph against the directory structure for completeness using `find_entities` at each scope.
4. Ensure all relationship types are consistent and accurately labeled.
5. Confirm the derivation order (Domain → Feature → Namespace → Component → Unit) is respected throughout.
6. Present a final summary with graph statistics from `project_overview`.

The final deliverable is the complete knowledge graph in gnapsis. Every node, every edge, every relationship must be registered through the gnapsis MCP tools.
