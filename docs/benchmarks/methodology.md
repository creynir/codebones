# Benchmark Methodology
Status: Complete (2026-03-15 baseline)

## Scope Freeze
Scope is frozen by semantic feature eligibility in:
- `docs/benchmarks/feature-matrix.csv`

Rules:
- Eligible pairs are benchmarked and must end in `OK`, `TIMEOUT`, or `ERROR`.
- Non-eligible pairs are `OUT_OF_SCOPE` (never `NA`).
- No silent drops.

## Semantic Features
1. `lookup_query`
2. `repo_index_build`
3. `structure_outline`
4. `context_pack`
5. `incremental_reindex_single`
6. `incremental_reindex_batch`
7. `time_to_query_after_change`
8. `changed_files_per_sec`

## Runtime States
- `OK`: command completed and passed correctness checks.
- `TIMEOUT`: command exceeded timeout, report exact `timeout_ms`.
- `ERROR`: command exited non-zero or correctness failed.
- `OUT_OF_SCOPE`: non-eligible pair from matrix.

## Datasets
Primary datasets from `../competitor-repos`:
- `small`: `agenthelm`
- `medium`: `temporal`
- `large`: `n8n`

Language/accountability fields are required in final report:
- code files
- LOC
- language distribution

Dataset profile and pinned revisions for this baseline:

| Dataset Label | Repo | Code Files | LOC | Commit |
|---|---|---:|---:|---|
| small | `agenthelm` | 53 | 6250 | `9ec76caae764` |
| medium | `temporal` | 2531 | 832991 | `29a039286526` |
| large | `n8n` | 10454 | 2068515 | `f7a787aca81c` |

## Execution Controls
- Single machine only.
- No intentional parallel benchmark jobs.
- Warmup + measured policy: `1 warmup + 5 measured`.
- Timeouts:
- `lookup_query`: 10000 ms
- `repo_index_build`: 180000 ms
- `structure_outline`: 10000 ms
- `context_pack`: 180000 ms
- `incremental_*`: 180000 ms
- Timing and memory capture: `/usr/bin/time -l`.
- Timeout value for each `TIMEOUT` row is recorded in output as `timeout_ms`.

Machine baseline for this run:
- OS: macOS 15.7.1 (`24G231`)
- CPU: Apple M4
- Memory: 16 GB (`17179869184` bytes)
- Kernel: Darwin 24.6.0 (`arm64`)

## Correctness Validation
### Lookup Golden Suite
For each dataset:
- Deterministic marker function is injected in source.
- Query must return marker hit.
- Metrics reported:
- `hit@k`
- `precision@k`
- `recall@k`

### Feature-specific checks
- `repo_index_build`: index exists and `lookup_query` succeeds.
- `structure_outline`: output non-empty and parses as valid structure payload.
- `context_pack`: deterministic constraints (non-empty output, stable format invocation).

## Incremental Update Benchmarks
Single-file change set:
- modify existing function body
- add new marker function
- delete marker function (cleanup verification)

Batch change set:
- controlled changes on up to 10 files.

Per dataset/tool record:
- `incremental_reindex_ms`
- `incremental_batch_reindex_ms`
- `time_to_query_after_change_ms`
- `changed_files_per_sec`
- `correctness_after_change`

## Normalized Metrics
Reported alongside raw values:
- `lookup_ms_per_kloc`
- `reindex_ms_per_kloc`
- `index_files_per_sec`
- `index_kloc_per_sec`
- `rss_mb_per_kloc` (when meaningful)

## Compared Tool Revisions
Pinned revisions used for this baseline:

| Tool | Version / Commit |
|---|---|
| `codebones` | `codebones 0.2.0`, commit `a429bad35b42` |
| `ast-grep` | `ast-grep 0.41.1`, commit `133d85647524` |
| `grep-ast` | commit `9a2c49b00852` |
| `tree-sitter-mcp` | commit `d2b82eb8db63` |
| `jcodemunch-mcp` | commit `6e616930e0e4` |
| `repomix` | commit `d10807523237` |
| `node` runtime | `v25.8.1` |
| Python runtime (`jcodemunch` venv) | `3.14.3` |

## Reproducibility
Reproduction is documented in methodology even when the orchestration script is not committed.

Protocol:
1. Build/prepare each eligible tool revision above.
2. Use datasets and commits from the Dataset section.
3. For each eligible `(tool, feature, dataset)` pair:
4. Run `1 warmup + 5 measured` iterations.
5. Measure with `/usr/bin/time -l`.
6. Record `OK`, `TIMEOUT`, or `ERROR` per run and carry `timeout_ms` for timeouts.

Exact command patterns used in this baseline:

```bash
# lookup_query
codebones search <query>
ast-grep run --pattern <query> .
grep-ast <query> .
node tree-sitter-mcp/dist/cli.js search <query> --output json -d .
python /tmp/jcm_bench_tool.py search --path <dataset_path> --storage <jcm_store> --query <query>

# repo_index_build
codebones index .
python /tmp/jcm_bench_tool.py index --path <dataset_path> --storage <jcm_store>

# structure_outline
codebones outline <relative_file_path>
python /tmp/jcm_bench_tool.py outline --path <dataset_path> --storage <jcm_store> --file <relative_file_path>

# context_pack
codebones pack --format markdown .
node repomix/bin/repomix.cjs --stdout
```

Per-run command strings are also preserved verbatim in `raw-YYYY-MM-DD.csv` (`command` column).

## Deliverables
- `docs/benchmarks/methodology.md`
- `docs/benchmarks/results-YYYY-MM-DD.md`
- `docs/benchmarks/raw-YYYY-MM-DD.csv`
- `docs/benchmarks/correctness-YYYY-MM-DD.csv`
- `docs/benchmarks/normalized-YYYY-MM-DD.csv`
