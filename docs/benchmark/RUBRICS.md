# Benchmark Rubrics v2: Tiered Scoring

Updated rubrics that graduate scoring from "correct" (4-5) to "comprehensive engineering analysis" (10). Higher tiers require identifying architectural patterns, engineering fundamentals (error handling, observability, standardization), and cross-cutting concerns.

---

## T1 — Architecture Layers
**Category**: architecture | **Difficulty**: medium

**Prompt**: Describe the layered architecture of the gnapsis codebase. What are the main layers, what module does each layer correspond to, and what is the direction of dependencies between layers? Be specific about module names and file paths.

| Score | Criteria |
|-------|----------|
| 4-5 | Identifies 3+ layers with correct dependency direction and specific module paths |
| 6-7 | 4+ layers, mentions DI mechanism (Context/FromRef/FromContext), identifies PostgreSQL + Apache AGE backend |
| 8 | Names specific components per layer (repositories, services by name), identifies GraphClient as key abstraction trait, recognizes architectural style (Clean Architecture, Hexagonal, or Onion) |
| 9 | Discusses cross-cutting concerns (error handling via AppError, config management, models as shared layer), explains compile-time DI via derive macros, identifies SOLID principles (Dependency Inversion via traits, Single Responsibility per layer), mentions Repository Pattern |
| 10 | 5+ layers with all key components named, notes observability patterns or their absence, mentions standardization/coding conventions, identifies architectural trade-offs or improvements. Comprehensive engineering analysis, not just structural listing |

---

## T2 — Dependency Trace
**Category**: dependency_tracing | **Difficulty**: hard

**Prompt**: Trace the complete data flow when the MCP tool 'search' is called with a query string. Starting from the MCP tool handler, trace through every layer until the embedding is generated and the database query is executed. List each function call in order with its file path.

| Score | Criteria |
|-------|----------|
| 4-5 | Gets 3+ hops correct in the call chain with file paths |
| 6-7 | Complete trace from MCP handler through GraphService to QueryRepository, mentions embedding vector generation, reaches database layer |
| 8 | Identifies data transformations at layer boundaries (query string -> embedding vector -> Cypher query -> parsed results), notes DI wiring (FromContext/Context), identifies Repository Pattern |
| 9 | Explains error propagation along the chain (Result<T, AppError> threading, ? operator), identifies async/await patterns, notes how the embedding model is injected and abstracted |
| 10 | Discusses potential failure modes at each hop (network errors, embedding failures, query timeouts), notes observability presence/absence (logging, tracing spans), identifies input validation points, assesses separation of concerns quality |

---

## T3 — Error Propagation
**Category**: error_analysis | **Difficulty**: medium

**Prompt**: How does error handling work in this codebase? What error type does src/error.rs define, what variants does it have, and how are errors converted for MCP protocol responses? Show specific mappings from error variants to MCP error codes.

| Score | Criteria |
|-------|----------|
| 4-5 | Identifies AppError enum and thiserror derive macro, lists some variants |
| 6-7 | Lists 6+ variants, explains From<AppError> for ErrorData conversion, provides 3+ specific variant-to-error-code mappings |
| 8 | Identifies error boundary pattern — internal errors (Connection, Query) vs domain errors (EntityNotFound, Validation) vs user-facing MCP errors (ErrorData). Explains how conversion sanitizes internal details |
| 9 | Evaluates against best practices — error context for debugging, error chain preservation, structured error logging. Identifies Result<T, AppError> convention across layers |
| 10 | Assesses completeness — unhandled edge cases, silently swallowed errors. Notes recovery strategies (retries, fallbacks). Discusses observability support (correlation IDs, error metrics). Compares to standard patterns (RFC 7807, error envelopes) |

---

## T4 — Impact Analysis
**Category**: impact_analysis | **Difficulty**: hard

**Prompt**: If I wanted to add a new graph backend (say SQLite instead of PostgreSQL), what traits would I need to implement, what files would need to change, and what would NOT need to change? Analyze the abstraction boundaries in the codebase.

| Score | Criteria |
|-------|----------|
| 4-5 | Identifies key traits (CypherExecutor, GraphClient, Transaction) |
| 6-7 | Full trait list including SqlExecutor, new module under graph/backends/ needed, services/repositories/MCP tools would NOT change |
| 8 | Mentions AppGraph type alias and Graph<C: GraphClient> generic, identifies Dependency Inversion Principle in practice, recognizes Strategy Pattern enabling backend swapping |
| 9 | Discusses config changes (connection strings, backend selection), migration implications (schema compatibility, Cypher dialect), testing strategy (trait-based testing, integration tests) |
| 10 | Identifies backend parity issues (SQLite lacking AGE graph extensions), transaction semantics differences, abstraction leaks (backend-specific Cypher, performance). Evaluates overall abstraction quality — what it enables and where it breaks down |

---

## T5 — Command Pattern
**Category**: pattern_recognition | **Difficulty**: medium

**Prompt**: The file src/services/commands.rs implements a specific design pattern. Identify the pattern, explain how it works in this codebase, list all command variants, and describe the failure handling semantics.

| Score | Criteria |
|-------|----------|
| 4-5 | Identifies Command Pattern by name, lists some variants |
| 6-7 | Lists all 7 variants (Attach, Unattach, Add, Relate, Unrelate, Link, Unlink), explains sequential execution with stop-on-first-failure |
| 8 | Explains CommandResult (executed/failed/skipped), identifies Rust enums as algebraic data types vs traditional OOP Command interface, notes each variant carries its own data |
| 9 | Evaluates pattern fitness — benefits (encapsulation, batch execution, audit trail) and limitations. Identifies related patterns (Builder, Undo/Redo potential). Notes transaction boundary behavior |
| 10 | Assesses engineering quality — error handling within commands, idempotency, extensibility, observability (logging, metrics). Compares to alternatives (event sourcing, CQRS) with trade-offs |

---

## T6 — BFS Algorithm
**Category**: algorithm_understanding | **Difficulty**: hard

**Prompt**: Explain the Best-First Search algorithm implemented in src/services/graph.rs for semantic subgraph extraction. What data structures does it use, how does it score nodes, what are the two scoring strategies and how do they differ, and what are the budget constraints?

| Score | Criteria |
|-------|----------|
| 4-5 | Identifies BinaryHeap/priority queue with PQNode entries, explains basic greedy traversal |
| 6-7 | Scoring via cosine similarity between embeddings, names both strategies (Global, BranchPenalty), mentions 3 budget constraints (max_tokens, max_nodes, min_relevance) |
| 8 | Contrasts strategies in detail (Global: cumulative token decay only; BranchPenalty: adds per-branch token penalty to encourage breadth), explains token estimation via TOKENS_PER_CHAR |
| 9 | Discusses entity caching (CacheEntry), analyzes complexity (time/space), identifies greedy implications (local optima, no backtracking), notes cycle handling |
| 10 | Evaluates design — strategy trade-offs for different query types, potential improvements (beam search, A*), token budget accuracy. Notes observability (traversal logging) and testability |

---

## T7 — Find Duplication
**Category**: code_quality | **Difficulty**: hard

**Prompt**: Find duplicated code in the gnapsis codebase. Focus on the src/repositories/ directory. Identify functions or patterns that are implemented more than once across different files, and explain what they do.

Known duplications: (1) row_to_entity in entity.rs vs query.rs, (2) row_to_code_reference in document.rs vs query.rs, (3) row_to_text_reference in document.rs vs query.rs, (4) parse_scope in category.rs vs schema.rs, (5) cosine_similarity in query.rs vs graph.rs, (6) embedding serialization patterns.

| Score | Criteria |
|-------|----------|
| 4-5 | Identifies 2-3 duplications with file locations |
| 6-7 | 3-4 duplications with explanations of what each function does |
| 8 | 5+ duplications, explains DRY violation and maintenance risk, suggests concrete refactoring (shared trait, common module) |
| 9 | Analyzes root cause — architectural reasons (repositories lacking common conversion layer, query.rs aggregating across types). Discusses maintainability and bug risk |
| 10 | Provides refactoring plan with implementation details (RowConversion trait, shared module). Evaluates trade-offs (complexity vs DRY). Notes whether copies have diverged. Identifies test coverage gaps |

---

## Scoring Dimensions (cross-cutting)

Higher-tier answers (8+) should demonstrate awareness of these engineering fundamentals, identifying their presence or noting their absence:

| Dimension | What to look for |
|-----------|-----------------|
| **Architectural Patterns** | Named patterns (Clean Architecture, Repository, Command, Strategy, DI), SOLID principles, separation of concerns |
| **Error Handling** | Error types, propagation strategy, boundary handling, recovery patterns, error context |
| **Observability** | Logging, tracing, metrics, debug tooling, correlation IDs |
| **Standardization** | Naming conventions, module organization, code style consistency, derive macros |
| **Configuration** | Config layering, environment handling, secrets management |
| **Security** | Input validation at boundaries, data sanitization, auth patterns |
| **Testing** | Test patterns, coverage strategy, testability of components |
| **Localization** | i18n support or its absence |
