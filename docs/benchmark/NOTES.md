# Benchmark Notes

Notes and observations from benchmark iterations. To be incorporated into a full report later.

---

## Benchmark 01: Baseline vs Gnapsis (v1)

- **Setup**: 7 tasks x 2 conditions x 3 runs = 42 runs, model: sonnet
- **Graph state**: 102 entities (2 domains, 21 features, 26 namespaces, 53 components, 0 units)
- **System prompt**: Identical for both conditions (no gnapsis-specific guidance)
- **Results**: See `01/REPORT.md` and `01/summary.csv`

Key finding: Gnapsis helped design tasks (T1, T4, T7) but added overhead for implementation tasks (T2, T5, T6). Quality was comparable. Cost increased due to graph schema in context.

---

## Prompt Engineering Experiments (v2, v3)

### v2: Aggressive prompt
- Added to gnapsis condition: "Use the gnapsis knowledge graph tools as your primary source... Only read source files directly when you need exact code that the graph doesn't provide."
- **T1**: Turns dropped (11.7 → 6.3) but quality dropped too (9.7 → 8.7)
- **T4**: Regressed badly (9.3 → 18.0 turns) — model made extra graph queries it wouldn't otherwise make
- **T7**: Neutral

### v3: Softer prompt
- Changed to: "Start by using gnapsis knowledge graph tools to orient yourself, then read source files to fill in details."
- **T4 only**: Got even worse (20.7 turns, $0.353)

### Conclusion
Prompt engineering hurt more than it helped. The model's own judgment about when to use graph tools was better than forced guidance. **Reverted to identical prompts for both conditions.**

---

## Graph Depth Experiment: Adding Unit Entities

### Hypothesis
Graph completeness (adding method-level units) matters more than prompt engineering.

### Batch 1: 22 units (graph traits + services)

Added 11 units for graph abstraction trait methods:
- CypherExecutor: execute_cypher, run_cypher
- SqlExecutor: execute_sql
- Transaction: commit, rollback
- GraphClient: begin
- Graph: query, transaction
- PostgresClient: connect, build_age_query, parse_pg_row

Added 11 units for service layer methods:
- EntityService: create, update
- GraphService: unified_search, semantic_query, best_first_search, cosine_similarity
- CommandService: execute, execute_add
- ValidationService: find_orphan_entities, find_cycles, find_scope_violations

**T4 results with 22 units (graph state: 124 entities)**:
| Metric | Baseline | Gnapsis v1 (0 units) | Gnapsis + 22 units |
|--------|----------|---------------------|-------------------|
| Turns  | 15.0     | 9.3                 | **2.0**           |
| Cost   | $0.220   | $0.335              | $0.275            |
| Quality| 9.7/10   | 10/10               | 10/10             |

**This was the breakthrough.** The model could answer "what traits would you need to implement for a new backend" directly from graph data in 2 turns instead of 15.

### Batch 2: +40 entities (repositories + MCP handlers)

Added 6 components (repository structs):
- EntityRepository, QueryRepository, DocumentRepository, CategoryRepository, SchemaRepository, SubgraphNode

Added 34 units across the full stack:
- EntityRepository (9): create, find_by_id, update, delete, validate_belongs_to, classify, add_belongs, add_related, add_link
- QueryRepository (7): get_entity_with_context, find_entities, get_document_entities, query_subgraph, search_entities_by_embedding, search_documents_by_embedding, get_entity_summaries_by_scope
- DocumentRepository (7): upsert_document, create_code_reference, create_text_reference, attach_reference, get_stale_references, get_entity_references, delete_reference
- SchemaRepository (1): get_project_stats
- McpServer (10): resolve, search, query, analyze_document, project_overview, get_entity, find_entities, create_entity, update_entity, init_project

**T4 results with 56 units (graph state: 164 entities)**:
| Metric | Baseline | Gnapsis + 22 units | Gnapsis + 56 units |
|--------|----------|--------------------|-------------------|
| Turns  | 15.0     | 2.0                | **2.7**           |
| Cost   | $0.220   | $0.275             | $0.270            |
| Quality| 9.7/10   | 10/10              | 10/10 (est.)      |

T4 is saturated — the trait-level units from batch 1 were the key factor. The new repository/MCP units should help more on T1 (architecture), T2 (dependency trace), and T7 (duplication) where the model needs to understand the full service→repository→graph call chain.

---

## Key Insight

**Graph depth > prompt engineering.** Adding the right unit-level entities (methods and functions) to the knowledge graph was dramatically more effective than any system prompt modification. The model naturally uses the graph when it contains the answers, and falls back to file reading when it doesn't.

---

## Benchmark 02: Full Results (56 units)

- **Setup**: 7 tasks x 2 conditions x 3 runs = 42 runs, model: sonnet
- **Graph state**: 164 entities (2 domains, 21 features, 26 namespaces, 59 components, 56 units)
- **Results**: See `02/` charts and `02/summary.csv`

### Overall
- Quality: Gnapsis 10.0/10 vs Baseline 9.7/10 (+3.4%)
- Turns: -35.8% overall, design tasks -49%
- Cost: +19.8% overhead (driven by simple tasks loading graph context)

### Standout results
- **T4 Impact Analysis**: 15 → 2 turns (-87%), $0.42 → $0.27 (-37%)
- **T2 Dependency Trace**: 18.7 → 11.7 turns (-38%)
- **T7 Find Duplication**: 10 → 7 turns (-30%)
- **T1 Architecture**: quality 8 → 10 (+25%), turns stable

### Cost overhead on simple tasks
T5 Command and T6 BFS take 2 turns regardless of condition. Gnapsis adds ~$0.06 overhead per task due to graph schema/context loading. This is the fixed cost of having the MCP tools available.

---

## TOON Projection: -40% Input Tokens

TOON (Token-Oriented Object Notation) reduces MCP tool response sizes by ~40-60%. Since input tokens are ~85% of total cost, a 40% TOON reduction yields ~34% cost reduction on gnapsis rows.

| Task | Baseline | Gnapsis (actual) | Gnapsis+TOON (est.) | Delta vs Baseline |
|------|----------|------------------|---------------------|-------------------|
| T1 Architecture | $0.184 | $0.260 (+41%) | ~$0.172 | **-7%** |
| T2 Dep. Trace | $0.284 | $0.375 (+32%) | ~$0.248 | **-13%** |
| T3 Error Prop. | $0.041 | $0.130 (+218%) | ~$0.086 | +110% |
| T4 Impact | $0.423 | $0.267 (-37%) | ~$0.176 | **-58%** |
| T5 Command | $0.066 | $0.123 (+86%) | ~$0.081 | +23% |
| T6 BFS | $0.064 | $0.120 (+90%) | ~$0.079 | +24% |
| T7 Duplication | $0.174 | $0.203 (+17%) | ~$0.134 | **-23%** |
| **Total** | **$1.236** | **$1.478 (+20%)** | **~$0.976** | **-21%** |

Overall cost flips from **+20% overhead to -21% savings** with TOON.
- Design tasks: +29% → **-38%** (cheaper than baseline)
- Implementation tasks: +58% → **+9%** (small remaining overhead)
- T3 remains an outlier — baseline solves it in 3.7 turns for $0.04, any MCP overhead is proportionally large

**Next step**: Run benchmark 03 with TOON enabled to validate these projections.

---

## Key Insight

**Graph depth > prompt engineering.** Adding the right unit-level entities (methods and functions) to the knowledge graph was dramatically more effective than any system prompt modification. The model naturally uses the graph when it contains the answers, and falls back to file reading when it doesn't.

---

## TODO for Full Report

- [x] Run full 7-task benchmark with 56 units (no prompt changes)
- [x] Compare against benchmark 01 baseline data
- [x] Score with LLM judge
- [x] Generate updated visualizations
- [x] Analyze which tasks benefit most from deeper graph
- [ ] Calculate ROI: graph construction cost vs per-query savings
- [ ] Run benchmark 03 with TOON format enabled
- [ ] Write full report with side-by-side benchmark 01 vs 02 comparison
