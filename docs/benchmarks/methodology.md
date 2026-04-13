# Benchmark Methodology

## Datasets

Open-source projects pinned at specific commits for reproducibility. Datasets are automatically cloned into `lab/` on first benchmark run.

### Speed + Static Token Benchmarks

| Label | Repo | Language | Code Files | LOC | Commit |
|---|---|---|---:|---:|---|
| small | [tiangolo/fastapi](https://github.com/tiangolo/fastapi) | Python | 1,121 | 107,493 | `eba8942c81db` |
| medium | [temporalio/temporal](https://github.com/temporalio/temporal) | Go | 2,531 | 832,991 | `29a039286526` |
| large | [n8n-io/n8n](https://github.com/n8n-io/n8n) | TypeScript | 10,454 | 2,068,515 | `f7a787aca81c` |

### Agent Eval

| Repo | Language | Code Files | LOC | Commit |
|---|---|---:|---:|---|
| [tiangolo/fastapi](https://github.com/tiangolo/fastapi) | Python | 1,121 | 107,493 | `eba8942c81db` |

FastAPI is used for the agent eval because it's universally recognized — developers can intuit the codebase size and judge whether the AI's answers are correct.

## Benchmark Categories

### 1. Speed Benchmarks

Measures wall-clock latency of codebones operations vs competitors.

#### Semantic Features
1. `lookup_query` — symbol search
2. `repo_index_build` — full index from scratch (symbols + imports)
3. `structure_outline` — file skeleton
4. `context_pack` — full repo packing with token budget
5. `skeleton_map` — `codebones map` (skeleton-only output)
6. `dependency_graph` — `codebones graph` (full import graph)
7. `blast_radius` — `codebones graph <file>` (transitive impact)
8. `incremental_reindex_single` — re-index after single file change
9. `incremental_reindex_batch` — re-index after batch change
10. `time_to_query_after_change` — query latency after incremental update

#### Execution Controls
- Single machine only. No parallel benchmark jobs.
- Warmup + measured policy: `1 warmup + 5 measured`.
- Timeouts: 10s for queries, 180s for index/pack.
- Timing and memory capture: `/usr/bin/time -l`.

#### Command Patterns

```bash
codebones index .
codebones search <query>
codebones outline <file>
codebones pack --format markdown .
codebones map --format markdown .
codebones graph --dir . --format markdown
codebones graph <file> --dir . --depth 3
```

### 2. Static Token Savings

Model-agnostic. Counts tokens in codebones output vs raw source using tiktoken (cl100k_base). No API calls required.

| Scenario | Without codebones | With codebones |
|----------|-------------------|----------------|
| **Orientation** | All source files | `codebones map` |
| **Impact analysis** | All source files | `codebones graph <file>` |
| **Symbol retrieval** | Full file content | `codebones search` + `codebones get` |
| **Budget efficiency** | Hard truncation at N tokens | `codebones pack --max-tokens N` |

Metrics: raw token count, codebones token count, reduction ratio.

### 3. Agent Eval (Sonnet on FastAPI)

The real-world benchmark. Two agents solve the same task on FastAPI — one with standard tools (grep, cat, find, ls), one with codebones tools (map, search, get, graph, outline). Both run as multi-turn agentic conversations with Claude Sonnet via the API.

**Requires:** `ANTHROPIC_API_KEY` environment variable.

#### What each agent gets

| Agent | Tools | How it works |
|-------|-------|-------------|
| **Standard** | `grep`, `cat`, `find`, `ls` | Explores iteratively — greps for patterns, reads files, follows imports manually |
| **Codebones** | `codebones_map`, `codebones_search`, `codebones_get`, `codebones_graph`, `codebones_outline` | Queries the structural index — gets the skeleton map, searches symbols, reads targeted code |

Both agents get the same system prompt (adjusted for their tool set) and the same user task. Neither is told how to approach the problem — they figure out their own exploration strategy.

#### Tasks

1. **Orientation** — "Describe the architecture of this project. What are the main modules, their responsibilities, and how they relate to each other? Be specific about file paths."
2. **Impact analysis** — "I need to refactor `fastapi/routing.py`. What other files in this project would be affected by changes to that file? List them and explain the dependency chain."
3. **Symbol retrieval** — "Find the `APIRouter` class, explain its main methods and how it's used by the rest of the codebase."

#### Metrics per task

- `total_input_tokens` — total tokens sent to the model across all turns
- `total_output_tokens` — total tokens generated across all turns
- `total_tokens` — input + output
- `tool_calls` — number of tool invocations
- `turns` — number of conversation turns
- `final_answer` — the agent's complete answer (saved for manual review)

#### What makes this credible

Every conversation is saved as a JSON log in `agent-eval-results/`. Readers can inspect:
- The exact prompts
- Every tool call and its result
- The agent's reasoning
- The final answer
- The token counts from the API `usage` field

No synthetic context, no cherry-picked examples. The agent decides what to explore and how.

### Runtime States
- `OK`: completed and passed correctness checks.
- `TIMEOUT`: exceeded timeout.
- `ERROR`: non-zero exit or correctness failed.

## Running Benchmarks

```bash
# All benchmarks auto-clone datasets into lab/ on first run.
# lab/ is gitignored.

# Static token savings
./docs/benchmarks/run_token_savings.sh

# Agent eval (requires ANTHROPIC_API_KEY)
ANTHROPIC_API_KEY=your-key python3 docs/benchmarks/run_agent_eval.py
```

## Deliverables
- `docs/benchmarks/methodology.md` — this file
- `docs/benchmarks/results.md` — speed benchmark results
- `docs/benchmarks/raw.csv` — raw speed data
- `docs/benchmarks/correctness.csv` — correctness validation
- `docs/benchmarks/normalized.csv` — normalized speed metrics
- `docs/benchmarks/token-savings.md` — static token savings scenarios
- `docs/benchmarks/token-savings.csv` — static token savings results (generated)
- `docs/benchmarks/run_token_savings.sh` — static benchmark script
- `docs/benchmarks/run_agent_eval.py` — agent eval script
- `docs/benchmarks/agent-eval.csv` — agent eval results (generated)
- `docs/benchmarks/agent-eval-results/` — full conversation logs (generated)
