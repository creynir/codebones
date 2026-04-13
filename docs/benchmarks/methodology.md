# Benchmark Methodology

## Datasets

Three open-source projects of increasing scale, each pinned at a specific commit for reproducibility. Datasets are automatically cloned into `lab/` at the repo root on first benchmark run.

| Label | Repo | Language | Code Files | LOC | Commit |
|---|---|---|---:|---:|---|
| small | [hadywalied/agenthelm](https://github.com/hadywalied/agenthelm) | Python | 53 | 6,250 | `9ec76caae764` |
| medium | [temporalio/temporal](https://github.com/temporalio/temporal) | Go | 2,531 | 832,991 | `29a039286526` |
| large | [n8n-io/n8n](https://github.com/n8n-io/n8n) | TypeScript | 10,454 | 2,068,515 | `f7a787aca81c` |

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
# Speed benchmarks (codebones)
codebones index .
codebones search <query>
codebones outline <file>
codebones pack --format markdown .
codebones map --format markdown .
codebones graph --dir . --format markdown
codebones graph <file> --dir . --depth 3
```

### 2. Token Savings Benchmarks

Measures how many tokens an AI agent needs to consume for common tasks — with codebones vs reading raw source files. Two tiers:

#### Tier 1: Static Token Counting

Model-agnostic. Counts tokens in codebones output vs raw source using tiktoken (cl100k_base). No API calls required.

| Scenario | Without codebones | With codebones |
|----------|-------------------|----------------|
| **Orientation** | All source files | `codebones map` |
| **Impact analysis** | All source files (manual import tracing) | `codebones graph <file>` |
| **Symbol retrieval** | Full file content | `codebones search` + `codebones get` |
| **Budget efficiency** | Hard truncation at N tokens | `codebones pack --max-tokens N` |

Metrics: raw token count, codebones token count, reduction ratio.

#### Tier 2: LLM Evaluation (Sonnet + Opus)

Sends identical tasks to Claude Sonnet and Opus, once with raw file context and once with codebones context. Measures actual API token usage and answer quality.

**Requires:** `ANTHROPIC_API_KEY` environment variable.

**Tasks per dataset:**
1. "Describe the architecture of this project." (orientation)
2. "What files would be affected if I changed `<hot_file>`?" (impact analysis)
3. "Find and explain the function `<symbol>`." (symbol retrieval)

**Metrics per task:**
- `input_tokens` — tokens sent to the model
- `output_tokens` — tokens in the response
- `total_tokens` — input + output
- `correctness` — manual or automated check against ground truth
- `model` — `claude-sonnet-4-6` or `claude-opus-4-6`

**Protocol:**
1. For each (dataset, task, model) triple, run with raw context and codebones context
2. Raw context: concatenate all source files up to model's context limit
3. Codebones context: appropriate command output (map, graph, search+get)
4. Record all token counts from the API response `usage` field
5. Each run is a single API call (no multi-turn)

### Runtime States
- `OK`: completed and passed correctness checks.
- `TIMEOUT`: exceeded timeout, report exact `timeout_ms`.
- `ERROR`: non-zero exit or correctness failed.
- `OUT_OF_SCOPE`: non-eligible pair.

## Correctness Validation

### Lookup Golden Suite
For each dataset, a deterministic marker function is injected. Query must return marker hit. Metrics: `hit@k`, `precision@k`, `recall@k`.

### Feature-specific checks
- `repo_index_build`: index exists and `lookup_query` succeeds.
- `structure_outline`: output non-empty and parses as valid structure payload.
- `context_pack`: non-empty output, stable format.
- `skeleton_map`: output contains skeleton entries, no `<content>` blocks.
- `dependency_graph`: output contains file entries with import counts.
- `blast_radius`: output contains affected file list.

## Running Benchmarks

```bash
# All benchmarks auto-clone datasets into lab/ on first run.
# lab/ is gitignored.

# Token savings (static + LLM)
./docs/benchmarks/run_token_savings.sh

# Speed benchmarks
# (orchestration script — run manually per the command patterns above)
```

## Deliverables
- `docs/benchmarks/methodology.md` — this file
- `docs/benchmarks/results.md` — speed benchmark results
- `docs/benchmarks/raw.csv` — raw speed data
- `docs/benchmarks/correctness.csv` — correctness validation
- `docs/benchmarks/normalized.csv` — normalized speed metrics
- `docs/benchmarks/token-savings.md` — token savings scenarios
- `docs/benchmarks/token-savings.csv` — token savings results (generated)
- `docs/benchmarks/run_token_savings.sh` — self-contained benchmark script
