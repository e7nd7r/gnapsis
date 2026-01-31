---
name: smoke-test
description: Run a smoke test of all gnapsis MCP tools to verify the system works end-to-end.
---

# Gnapsis Smoke Test

Run through every gnapsis MCP tool to verify the system is functional. This skill
exercises the full feature set: project setup, taxonomy, entity CRUD, references,
search, subgraph queries, document analysis, validation, and multi-source support.

## Prerequisites

1. Database must be running: `just db-up`
2. gnapsis must be installed: `cargo install --path .`
3. MCP server must be connected: `/mcp`

## Test Procedure

Run each phase sequentially. If any step fails, jump to Phase 11 (Cleanup) to restore
the original config before reporting the failure.
Track entity IDs, reference IDs, and category IDs created during the test for cleanup.

### Phase 0: Configuration Validation

Run the config unit tests to verify source validation rules before touching the database.

**0.1** Run the config unit tests:
```bash
cargo test --lib config::tests -- --nocapture
```
  - Verify all 17 tests pass
  - The test suite covers: zero/one/two/three+ source validation, duplicate detection,
    `get_source()` implicit default, `effective_sources()`, and `resolve_path()` behavior

### Phase 1: Config Isolation

The smoke test uses a separate project name (`gnapsis_smoke_test`) so it gets its own
graph and does not interfere with the real project data.

**1.1** Backup the real config:
```bash
cp .gnapsis.toml .gnapsis.toml.bak
```

**1.2** Overwrite `.gnapsis.toml` with the smoke test config:
```toml
# Force local database — overrides global ~/.config/gnapsis/config.toml
[postgres]
uri = "postgresql://postgres:postgres@localhost:5432/gnapsis_dev"

[embedding]
provider = "fastembed"
model = "BAAI/bge-small-en-v1.5"
dimensions = 384

[project]
name = "smoke_test"

[[project.sources]]
id = "default"
path = "<absolute path to this repo>"

[[project.sources]]
id = "vault"
path = "<absolute path to vault source from the backup>"
```
Copy the source paths from the backup file. The key differences are:
- `[postgres]` and `[embedding]` force the local Docker database (overrides global config)
- `name = "smoke_test"` creates graph `gnapsis_smoke_test` instead of `gnapsis_gnapsis`

**1.3** Reconnect the MCP server so it picks up the new config: `/mcp`

### Phase 2: Database Reset & Init

**2.1** Drop the smoke test graph so init starts fresh:
```bash
just db-reset
just db-up
```
Wait for the database to be ready before proceeding.

**2.2** Run `init_project` (force: true) and verify:
  - Response includes `db_version` and `graph_version`
  - `applied_db_migrations` is non-empty
  - `applied_graph_migrations` is non-empty
  - `was_initialized` is false (fresh graph)

**2.3** Run `init_project` again (force: false) and verify:
  - `was_initialized` is true
  - `applied_db_migrations` is empty (no new migrations)
  - `applied_graph_migrations` is empty

### Phase 3: Project Overview (Empty State)

**3.1** Run `project_overview` with no optional params and verify:
  - `categories` contains 17 categories across 5 scopes (Domain, Feature, Namespace, Component, Unit)
  - `stats.domains` is 0
  - `stats.features` is 0
  - `stats.namespaces` is 0
  - `stats.components` is 0
  - `stats.units` is 0
  - `stats.references` is 0

**3.2** Run `project_overview` with `include_descriptions: true` and verify:
  - Response structure is the same
  - Descriptions are not truncated

**3.3** Run `project_overview` with `output_format: "toon"` and verify:
  - Response is returned in TOON format

**3.4** Note down the category IDs needed for entity creation:
  - Find the "core" category at Domain scope -> save as `DOMAIN_CAT_ID`
  - Find the "functional" category at Feature scope -> save as `FEATURE_CAT_ID`
  - Find the "struct" category at Component scope -> save as `COMPONENT_CAT_ID`
  - Find the "function" category at Unit scope -> save as `UNIT_CAT_ID`

### Phase 4: Taxonomy

**4.1** Run `create_category` with name "smoke-test", scope "Feature", description "Temporary category for smoke testing"
  - Verify the category is created with an ID -> save as `SMOKE_CAT_ID`

**4.2** Run `project_overview` and verify:
  - The "smoke-test" category appears in the Feature scope
  - Total category count is now 18

### Phase 5: Entity CRUD

#### 5A: Analyze Document (discover symbols)

**5A.1** Run `analyze_document` on `src/config.rs` with `source_id: "default"` to discover symbols
  - Verify `untracked` list contains symbols (e.g., "Config", "ProjectConfig", "Source")
  - Save the untracked symbol names for use in entity creation

#### 5B: Create Entity - Success Paths

**5B.1** Create a Domain entity:
  - `name`: "Smoke Test Domain"
  - `description`: "Temporary domain for smoke testing gnapsis"
  - `category_ids`: [`DOMAIN_CAT_ID`]
  - `commands`: [Add code reference to `src/config.rs`, lsp_symbol "Config", source_id "default"]
  - Verify: entity created with ID (save as `DOMAIN_ID`), `has_embedding` is true
  - Verify: `executed` contains the Add command result with reference ID (save as `DOMAIN_REF_ID`)
  - Verify: `failed` is null, `skipped` is empty

**5B.2** Create a Feature entity with parent:
  - `name`: "Smoke Test Feature"
  - `description`: "Temporary feature for smoke testing"
  - `category_ids`: [`FEATURE_CAT_ID`]
  - `parent_ids`: [`DOMAIN_ID`]
  - `commands`: [Add code reference to `src/config.rs`, lsp_symbol "ProjectConfig", source_id "default"]
  - Verify: entity created with ID (save as `FEATURE_ID`), parent relationship established
  - Verify: `executed` contains Add result with reference ID (save as `FEATURE_REF_ID`)

**5B.3** Create a Component entity (child of Feature):
  - `name`: "Smoke Test Component"
  - `description`: "Temporary component for smoke testing"
  - `category_ids`: [`COMPONENT_CAT_ID`]
  - `parent_ids`: [`FEATURE_ID`]
  - `commands`: [Add code reference to `src/config.rs`, lsp_symbol "Source", source_id "default"]
  - Verify: entity created with ID (save as `COMPONENT_ID`)

#### 5C: Create Entity - Error Paths

**5C.1** Attempt to create entity with empty `category_ids`:
  - Expect error about missing classification

**5C.2** Attempt to create entity with a non-existent category ID:
  - Expect error about invalid category

**5C.3** Attempt to create entity with a non-existent parent ID:
  - Expect error about invalid parent

#### 5D: Update Entity - Success Paths

**5D.1** Update the Feature entity description:
  - `entity_id`: `FEATURE_ID`
  - `description`: "Updated smoke test feature description"
  - Verify: `embedding_updated` is true
  - Verify: description changed in response

**5D.2** Update entity with Relate command:
  - `entity_id`: `COMPONENT_ID`
  - `commands`: [Relate to `DOMAIN_ID` with note "smoke test relationship"]
  - Verify: `executed` contains the Relate result

**5D.3** Update entity with Unrelate command:
  - `entity_id`: `COMPONENT_ID`
  - `commands`: [Unrelate from `DOMAIN_ID`]
  - Verify: relationship removed

**5D.4** Update entity name only:
  - `entity_id`: `COMPONENT_ID`
  - `name`: "Renamed Smoke Component"
  - Verify: name changed, `embedding_updated` is false (description unchanged)

**5D.5** Update entity categories (replace semantics):
  - `entity_id`: `COMPONENT_ID`
  - `category_ids`: [`COMPONENT_CAT_ID`] (same category, verifies replace works)
  - Verify: categories replaced

#### 5E: Get Entity

**5E.1** Run `get_entity` on `DOMAIN_ID`:
  - Verify: returns full entity details with classifications, references, and hierarchy
  - Verify: references include the code reference from 5B.1
  - Verify: entity has parent/child relationships as expected

#### 5F: Delete Entity - Error Paths

**5F.1** Attempt to delete Domain entity (has children):
  - `entity_id`: `DOMAIN_ID`
  - Expect error: "has N children and cannot be deleted"

#### 5G: Delete Entity - Success Path

**5G.1** Delete the Component entity (leaf node):
  - `entity_id`: `COMPONENT_ID`
  - Verify: `deleted` is true

### Phase 6: Multi-Source References

**6.1** Create a Feature entity with a vault text reference:
  - `name`: "Smoke Vault Feature"
  - `description`: "Feature referencing vault documentation"
  - `category_ids`: [`FEATURE_CAT_ID`]
  - `parent_ids`: [`DOMAIN_ID`]
  - `commands`: [Add text reference: source_id "vault", document_path "designs/005-ontology-improvements.md", start_line 1, end_line 30, anchor "# DES-005: Ontology Improvements"]
  - Verify: entity created (save as `VAULT_FEATURE_ID`)
  - Verify: `executed` contains Add with reference (save as `VAULT_REF_ID`)

**6.2** Run `get_entity` on `VAULT_FEATURE_ID`:
  - Verify: reference details include `source_id: "vault"`
  - Verify: reference type is "text"
  - Verify: document_path, start_line, end_line, anchor are correct

**6.3** Create another entity with a code reference using default source_id:
  - `commands`: [Add code reference without specifying source_id, document_path "src/config.rs", lsp_symbol "default_source_id"]
  - Verify: reference created with source_id "default" (the default)

### Phase 7: Search & Query

#### 7A: Unified Search

**7A.1** Search with target "entities":
  - `query`: "smoke test domain"
  - `target`: "entities"
  - Verify: results contain entity matches
  - Verify: `references` array is empty (target is entities-only)

**7A.2** Search with target "references":
  - `query`: "configuration"
  - `target`: "references"
  - Verify: results contain reference matches with document paths
  - Verify: `entities` array is empty (target is references-only)

**7A.3** Search with target "all" (default):
  - `query`: "smoke testing"
  - Verify: results may contain both entities and references

**7A.4** Search with scope filter:
  - `query`: "smoke"
  - `scope`: "Domain"
  - Verify: only Domain-scope entities returned

**7A.5** Search with min_score filter:
  - `query`: "smoke"
  - `min_score`: 0.9
  - Verify: only high-similarity results returned (may be empty)

**7A.6** Search with limit:
  - `query`: "smoke"
  - `limit`: 1
  - Verify: at most 1 result returned

**7A.7** Search with output_format "toon":
  - `query`: "smoke"
  - `output_format`: "toon"
  - Verify: response is in TOON format

#### 7B: Semantic Subgraph Query

**7B.1** Query with entity_id:
  - `entity_id`: `DOMAIN_ID`
  - Verify: `nodes` array contains the domain entity and connected entities
  - Verify: `edges` array shows BELONGS_TO and HAS_REFERENCE relationships
  - Verify: `stats` includes `estimated_tokens` and `nodes_pruned`

**7B.2** Query with semantic_query only:
  - `semantic_query`: "smoke test configuration"
  - Verify: returns a subgraph relevant to the query

**7B.3** Query with both entity_id and semantic_query:
  - `entity_id`: `DOMAIN_ID`
  - `semantic_query`: "configuration management"
  - Verify: subgraph extracted with semantic relevance scoring

**7B.4** Query with neither entity_id nor semantic_query:
  - Expect error: "Either entity_id or semantic_query must be provided"

**7B.5** Query with max_tokens limit:
  - `entity_id`: `DOMAIN_ID`
  - `max_tokens`: 500
  - Verify: `stats.estimated_tokens` is within budget

**7B.6** Query with max_nodes limit:
  - `entity_id`: `DOMAIN_ID`
  - `max_nodes`: 2
  - Verify: at most 2 nodes returned

**7B.7** Query with scoring_strategy "branch_penalty":
  - `entity_id`: `DOMAIN_ID`
  - `scoring_strategy`: "branch_penalty"
  - Verify: response is valid (may differ from global strategy)

**7B.8** Query with relationship_types filter:
  - `entity_id`: `DOMAIN_ID`
  - `relationship_types`: ["BELONGS_TO"]
  - Verify: edges only contain BELONGS_TO relationships

**7B.9** Query with output_format "toon":
  - `entity_id`: `DOMAIN_ID`
  - `output_format`: "toon"
  - Verify: response is in TOON format

#### 7C: Find Entities

**7C.1** Find by scope:
  - `scope`: "Domain"
  - Verify: the smoke test domain entity appears in results

**7C.2** Find by category:
  - `category`: "core"
  - Verify: domain entity appears

**7C.3** Find by parent_id:
  - `parent_id`: `DOMAIN_ID`
  - Verify: the Feature and Vault Feature entities appear

**7C.4** Find with limit:
  - `scope`: "Feature"
  - `limit`: 1
  - Verify: at most 1 result

**7C.5** Find with no filters:
  - No scope, category, or parent_id
  - Verify: returns all entities (up to default limit of 50)

#### 7D: Get Document Entities

**7D.1** Get entities for "src/config.rs" with source_id "default":
  - Verify: entities with references to config.rs are listed
  - Verify: reference details are included

**7D.2** Get entities for a vault document:
  - `document_path`: "designs/005-ontology-improvements.md"
  - `source_id`: "vault"
  - Verify: vault feature entity appears

### Phase 8: Document Analysis

**8.1** Run `analyze_document` on `src/config.rs` with source_id "default", all options enabled:
  - `include_tracked`: true
  - `include_untracked`: true
  - `include_diffs`: true
  - Verify: `tracked` references appear (from Phase 5)
  - Verify: `untracked` symbols are listed
  - Verify: `source_id` is "default" in response
  - Verify: `document_type` is "code"
  - Verify: `summary` has correct counts

**8.2** Run `analyze_document` with only tracked references:
  - `include_tracked`: true
  - `include_untracked`: false
  - `include_diffs`: false
  - Verify: `untracked` is empty, `diff_hunks` is empty
  - Verify: `tracked` is populated

**8.3** Run `analyze_document` with only untracked symbols:
  - `include_tracked`: false
  - `include_untracked`: true
  - `include_diffs`: false
  - Verify: `tracked` is empty
  - Verify: `untracked` is populated

**8.4** Run `analyze_document` with explicit LSP symbols:
  - Provide `lsp_symbols` array with at least one symbol (e.g., name "Config", kind 23, start_line and end_line from Phase 5A)
  - Verify: untracked detection uses provided symbols

**8.5** Run `analyze_document` with output_format "toon":
  - Verify: response is in TOON format

**8.6** Run `analyze_document` on a vault document:
  - `document_path`: "designs/005-ontology-improvements.md"
  - `source_id`: "vault"
  - Verify: responds without error
  - Verify: `document_type` is "text"
  - Verify: tracked references from Phase 6 appear (if any)

### Phase 9: Reference Management

#### 9A: Get Changed Files

**9A.1** Run `get_changed_files` with no params:
  - Verify: returns a list of changed files with change types
  - Verify: response includes file paths

**9A.2** Run `get_changed_files` with from_sha and to_sha (use HEAD~1 and HEAD):
  - First get HEAD SHA via git
  - Verify: returns files changed in the last commit

#### 9B: Alter References - Update

**9B.1** Run `alter_references` with an Update command on `FEATURE_REF_ID`:
  - Update `start_line` to a new value (e.g., 10)
  - Update `end_line` to a new value (e.g., 50)
  - Verify: update succeeds
  - Verify: response shows the updated reference

**9B.2** Run `alter_references` with Update on text reference `VAULT_REF_ID`:
  - Update `anchor` to "# Updated Anchor"
  - Verify: anchor updated

#### 9C: Alter References - Delete

**9C.1** Attempt to delete a reference that is attached to an entity:
  - Run `alter_references` with Delete on `FEATURE_REF_ID`
  - Expect error: reference is attached to an entity

**9C.2** To test delete success: first create a standalone reference, detach it, then delete:
  - Create a temporary entity with a reference
  - Use `update_entity` with Unattach command to detach the reference
  - Run `alter_references` with Delete on the detached reference
  - Verify: deletion succeeds
  - Clean up the temporary entity

#### 9D: LSP Refresh

**9D.1** Run `lsp_refresh` on `src/config.rs` with source_id "default":
  - Provide LSP symbols from analyze_document (Phase 8.1) untracked list
  - Include at least the symbols that have existing references (e.g., "Config", "ProjectConfig")
  - Verify: `updated_count` shows how many references were refreshed
  - Verify: `updated` array shows old and new line numbers for changed refs
  - Verify: `unmatched_count` shows symbols with no matching reference

### Phase 10: Graph Validation

**10.1** Run `validate_graph` with all checks enabled:
  - `check_orphans`: true
  - `check_cycles`: true
  - `check_scope_violations`: true
  - `check_unclassified`: true
  - `check_no_references`: true
  - Verify: `valid` is true or only has expected issues from test data
  - Verify: `issue_count` matches the sum of all issue arrays

**10.2** Run `validate_graph` with only orphan check:
  - `check_orphans`: true
  - `check_cycles`: false
  - `check_scope_violations`: false
  - `check_unclassified`: false
  - `check_no_references`: false
  - Verify: only `orphans` array may have entries, all others are empty

**10.3** Run `validate_graph` with only cycle check:
  - `check_cycles`: true, all others false
  - Verify: `cycles` is empty (no cycles in test data)

**10.4** Run `validate_graph` with only scope violations check:
  - `check_scope_violations`: true, all others false
  - Verify: no scope violations in test data

### Phase 11: Cleanup

This phase MUST run even if a previous phase failed — it restores the original config.

**11.1** Delete entities in reverse dependency order:
  - Delete `VAULT_FEATURE_ID` (and any temp entities from Phase 9C)
  - Delete `FEATURE_ID`
  - Delete `DOMAIN_ID`
  - Verify: each deletion succeeds

**11.2** Run `project_overview` and verify:
  - `stats.domains` is 0
  - `stats.features` is 0
  - `stats.components` is 0
  - `stats.references` is 0

**11.3** Drop the smoke test graph to leave no residue:
```bash
# The graph gnapsis_smoke_test can be dropped via psql or left for next run
# init_project with force:true will recreate it anyway
```

**11.4** Restore the original config:
```bash
mv .gnapsis.toml.bak .gnapsis.toml
```

**11.5** Reconnect the MCP server to restore the real project config: `/mcp`

**11.6** Report: "Smoke test PASSED - all 11 phases completed successfully"

## Failure Reporting

If any phase fails:
1. **Always run Phase 11.4 and 11.5 first** to restore the original config
2. Then report:
   - Phase number and subtest that failed (e.g., "Phase 5, Test 5C.1")
   - Tool name and parameters used
   - Error message received
   - "Smoke test FAILED at Phase N, Test N.X"

## Notes

- The vault source path is configured in `.gnapsis.toml` under `[[project.sources]]`
- All sources share the same graph: `gnapsis_<project_name>`
- References store `source_id` to track which source they belong to
- If vault source is not configured, skip Phase 6 and vault-related steps in Phases 7-8
- Entity IDs, reference IDs, and category IDs should be tracked throughout the test for assertions and cleanup
- Error path tests (e.g., 5C, 7B.4, 9C.1) expect specific errors - verify the error message matches
