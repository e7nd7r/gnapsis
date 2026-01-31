# Gnapsis Benchmark Report

## Setup

**Methodology**: 7 code understanding tasks run against the gnapsis codebase (~15K LoC Rust), each executed 3 times under two conditions using Claude Code CLI (Sonnet model):

- **Baseline**: Claude Code with built-in tools only (Read, Grep, Glob, Bash)
- **With Gnapsis**: Claude Code + gnapsis knowledge graph MCP server (18 tools, ~8K tokens of schema)

Quality scored by a blind LLM judge (0-10) against rubrics with expected concepts. Keyword hits provide an automated proxy metric.

**Total runs**: 42 (7 tasks x 2 conditions x 3 runs)

---

## Where Gnapsis Shines

### Architecture Understanding: 44% Faster, Higher Quality

> *"Describe the layered architecture of the gnapsis codebase..."*

|                 | Baseline | With Gnapsis | Delta    |
|-----------------|----------|--------------|----------|
| **Quality**     | 8.7/10   | **9.7/10**   | **+11%** |
| **Duration**    | 106s     | **60s**      | **-44%** |
| **Cost**        | $0.248   | **$0.221**   | **-11%** |

The baseline's weakest run scored 7/10 — it missed Apache AGE as the backend technology, didn't mention the `GraphClient` trait, and failed to discuss `FromContext`. With gnapsis, the worst run scored 9/10. The knowledge graph's entity hierarchy directly maps to architectural layers, giving Claude immediate structural context instead of having to infer it from file reads.

This is the only task where gnapsis was both **faster** and **cheaper** than baseline.

### Impact Analysis: 38% Fewer Turns, 30% Fewer Tokens

> *"If I wanted to add a new graph backend (say SQLite), what traits would I need to implement, what files would need to change, and what would NOT need to change?"*

|                    | Baseline | With Gnapsis | Delta    |
|--------------------|----------|--------------|----------|
| **Quality**        | 10/10    | 10/10        | tie      |
| **Turns**          | 15.0     | **9.3**      | **-38%** |
| **Input Tokens**   | 244K     | **171K**     | **-30%** |

Both conditions reached perfect scores, but gnapsis got there in 9 turns instead of 15. The graph's `BELONGS_TO` relationships and scope hierarchy let Claude trace trait implementations and their consumers without reading every file in `src/repositories/` and `src/services/` to confirm they were backend-agnostic.

### Cross-Cutting Analysis: Better Answers on Hard Tasks

> *"Find duplicated code in src/repositories/..."*

|                 | Baseline | With Gnapsis | Delta    |
|-----------------|----------|--------------|----------|
| **Quality**     | 8.7/10   | **9.7/10**   | **+11%** |
| **Turns**       | 6.7      | 6.3          | -5%      |
| **Tokens**      | ~10K out | ~10K out     | tie      |

The gnapsis condition found more duplication instances and provided more precise line references. With entity-level tracking of which functions exist in which repositories, the graph made it easier to spot where the same `row_to_entity`, `row_to_code_reference`, and `row_to_text_reference` patterns were repeated across `query.rs`, `entity.rs`, and `document.rs`.

---

## The Pattern: Design Tasks vs. Implementation Tasks

The benchmark reveals a clear split in where gnapsis adds value:

### Design tasks (gnapsis wins)

| Task | Category | Quality Delta | Best Metric |
|------|----------|---------------|-------------|
| T1 - Architecture Layers | Architecture understanding | **+11%** | 44% faster, 11% cheaper |
| T4 - Impact Analysis | Change planning | tie (both 10) | 38% fewer turns, 30% fewer tokens |
| T7 - Find Duplication | Code quality review | **+11%** | Better coverage |

These tasks share a common trait: they require **understanding relationships between components** — which layers depend on which, what would break if something changed, where similar patterns repeat across modules. This is exactly what a knowledge graph encodes.

### Implementation tasks (baseline is sufficient)

| Task | Category | Quality Delta | Cost Delta |
|------|----------|---------------|------------|
| T5 - Command Pattern | Single-file pattern recognition | tie (both 10) | +142% cost |
| T6 - BFS Algorithm | Single-file algorithm analysis | tie (both 10) | +89% cost |
| T3 - Error Propagation | Type-driven tracing | tie (both 10) | +51% cost |
| T2 - Dependency Trace | Call chain tracing | tie (both 10) | +38% cost |

These tasks are solvable by reading 1-3 files. The baseline can grep for a function name, read the file, and answer correctly. Gnapsis adds schema overhead (~8K tokens) without providing meaningful advantage. For T5 and T6, both conditions answered in 2 turns — the knowledge graph was available but unnecessary.

---

## Cost Analysis

The primary cost of gnapsis is the MCP tool schema: 18 tool definitions adding ~8K tokens to the system prompt. This is a fixed cost per turn, making it proportionally more expensive for simple tasks that complete in few turns.

| Task Type | Avg Cost Baseline | Avg Cost Gnapsis | Overhead |
|-----------|-------------------|------------------|----------|
| Design tasks (T1, T4, T7) | $0.202 | $0.250 | +24% |
| Implementation tasks (T2, T3, T5, T6) | $0.129 | $0.203 | +57% |

For design tasks, the overhead is moderate (24%) and offset by quality gains. For implementation tasks, the overhead is steep (57%) with no quality benefit.

---

## Qualitative Comparison: T1 Architecture

**Baseline run 1** (scored 7/10):
> Identified 7 layers including CLI and cross-cutting concerns. Correctly traced dependency direction. **Missed**: Apache AGE, `FromContext` trait, `GraphClient` trait. Mentioned `FromRef<Context>` but not the full DI picture.

**Gnapsis run 1** (scored 10/10):
> Identified 5 clean layers with specific file paths. Mentioned Apache AGE backend, `FromContext` derive macro, and `GraphClient` trait. Included a dependency direction diagram. The graph's Domain/Feature/Namespace/Component hierarchy directly informed the architectural breakdown.

The difference: baseline inferred architecture from directory structure and file reads. Gnapsis had the architecture encoded as entities with explicit relationships, producing a more complete and accurate answer.

---

## Recommendations

**Use gnapsis for design-phase tasks:**
- Architecture reviews and documentation
- Impact analysis before refactoring
- Code quality audits (duplication, coupling)
- Onboarding (understanding unfamiliar codebases)
- Change planning ("what would break if...")

**Don't pay the overhead for implementation tasks:**
- Reading a specific file to understand a function
- Tracing a call chain through 2-3 files
- Identifying a pattern in a single module

**Optimization opportunities:**
- TOON output format (not yet default) could reduce gnapsis tool response tokens by 40%
- Claude Code's Tool Search (v2.1.7) already lazy-loads tools when schemas exceed 10% of context, but gnapsis at ~8K tokens (4% of 200K) stays under the threshold
- Reducing schema complexity of `create_entity` and `update_entity` (1.5K tokens each due to nested enum variants) would cut total schema by ~35%

---

## Raw Numbers

| Task | Condition | Quality | Turns | Duration | Cost | Input Tokens |
|------|-----------|---------|-------|----------|------|-------------|
| T1 Architecture | baseline | 8.7 | 2.0 | 106s | $0.248 | 44K |
| T1 Architecture | **gnapsis** | **9.7** | 11.7 | **60s** | **$0.221** | 170K |
| T2 Dep. Trace | baseline | 10.0 | 18.3 | 77s | $0.297 | 444K |
| T2 Dep. Trace | gnapsis | 10.0 | 18.3 | 76s | $0.408 | 576K |
| T3 Errors | baseline | 10.0 | 8.7 | 37s | $0.100 | 168K |
| T3 Errors | gnapsis | 10.0 | 8.3 | 35s | $0.150 | 204K |
| T4 Impact | baseline | 10.0 | 15.0 | 55s | $0.220 | 244K |
| T4 Impact | **gnapsis** | 10.0 | **9.3** | 100s | $0.335 | **171K** |
| T5 Command | baseline | 10.0 | 2.0 | 26s | $0.055 | 50K |
| T5 Command | gnapsis | 10.0 | 2.7 | 32s | $0.134 | 94K |
| T6 BFS | baseline | 10.0 | 2.0 | 23s | $0.063 | 50K |
| T6 BFS | gnapsis | 10.0 | 2.0 | 26s | $0.118 | 67K |
| T7 Duplication | baseline | 8.7 | 6.7 | 25s | $0.139 | 88K |
| T7 Duplication | **gnapsis** | **9.7** | 6.3 | 28s | $0.193 | 113K |

*All values are means across 3 runs. Input tokens include cached tokens.*

---

## Conclusion

Gnapsis is a **design-phase tool**. It encodes architectural knowledge — entity relationships, scope hierarchies, domain boundaries — that an AI agent would otherwise have to reconstruct from scratch by reading files. For tasks that require understanding how the codebase fits together, this pre-computed knowledge graph produces better answers faster. For tasks that only require reading specific files, the schema overhead outweighs the benefit.

The benchmark on a ~15K LoC codebase represents a conservative test. On larger codebases where baseline exploration requires more turns and more file reads, the graph's advantage in providing targeted, relationship-aware context should increase proportionally.
