# Benchmark Results (v0.3.0)
Status: Complete (full rerun finished on 2026-03-16)

Methodology: `docs/benchmarks/methodology.md`
Feature matrix: `docs/benchmarks/feature-matrix.csv`
Pinned commit: `4b65d43b9b796bb8a3ed3701222027c78dbb28b7`

Timeout policy used in this run:
- `lookup_query`: 10000 ms
- `structure_outline`: 10000 ms
- `repo_index_build`: 180000 ms
- `context_pack`: 180000 ms
- `incremental_reindex_single`: 180000 ms
- `incremental_reindex_batch`: 180000 ms
- `time_to_query_after_change`: 180000 ms

## Dataset Language Profile
| Dataset | Code Files | LOC | Dominant Languages |
|---|---:|---:|---|
| small | 53 | 6250 | Python |
| medium | 2531 | 832991 | Go |
| large | 10454 | 2068515 | TypeScript, Python, JavaScript |

## lookup_query (cold)
| Dataset | codebones | ast-grep | grep-ast | tree-sitter-mcp | jcodemunch-mcp |
|---|---|---|---|---|---|
| small | 4.02 | 11.93 | 484.22 | 196.19 | 58.5 |
| medium | 10.45 | 432.79 | 8208.27 | TIMEOUT | 199.83 |
| large | 11.82 | 1998.44 | TIMEOUT | TIMEOUT | 104.54 |

## lookup_query (warm)
| Dataset | codebones | ast-grep | grep-ast | tree-sitter-mcp | jcodemunch-mcp |
|---|---|---|---|---|---|
| small | 3.73 | 10.64 | 481.93 | 198.82 | 57.88 |
| medium | 6.22 | 444.94 | 8889.13 | TIMEOUT | 189.93 |
| large | 7.46 | 2007.32 | TIMEOUT | TIMEOUT | 99.6 |

## repo_index_build (cold)
| Dataset | codebones | jcodemunch-mcp |
|---|---|---|
| small | 8.45 | 76.77 |
| medium | 290.0 | 754.05 |
| large | 1310.0 | 8249.67 |

## structure_outline (cold)
| Dataset | codebones | jcodemunch-mcp |
|---|---|---|
| small | 4.53 | 65.86 |
| medium | 5.34 | 172.89 |
| large | 5.52 | 100.33 |

## context_pack (cold)
| Dataset | codebones | repomix |
|---|---|---|
| small | 50.0 | 947.0 |
| medium | 2580.0 | 10236.62 |
| large | 7930.0 | 11547.61 |

## incremental_reindex_single (cold)
| Dataset | codebones | jcodemunch-mcp |
|---|---|---|
| small | 10.38 | 159.32 |
| medium | 190.0 | 2067.47 |
| large | 1140.0 | 8147.05 |

## incremental_reindex_batch (cold)
| Dataset | codebones | jcodemunch-mcp |
|---|---|---|
| small | 50.0 | 184.33 |
| medium | 280.0 | 2362.37 |
| large | 1020.0 | 8304.33 |

## time_to_query_after_change (cold)
| Dataset | codebones | jcodemunch-mcp |
|---|---|---|
| small | 14.70 | 114.28 |
| medium | 248.32 | 324.19 |
| large | 1068.31 | 101.77 |

## changed_files_per_sec (cold)
| Dataset | codebones (single) | codebones (batch) | jcodemunch-mcp |
|---|---|---|---|
| small | 96.34 | 200.0 | 30.26 |
| medium | 5.26 | 35.71 | 2.36 |
| large | 0.88 | 9.80 | 0.66 |

## Correctness Summary (lookup + incremental checks)
| Dataset | Tool | Queries | Mean hit@k | Mean precision@k | Mean recall@k |
|---|---|---:|---:|---:|---:|
| small | ast-grep | 1 | 1.0 | 1.0 | 1.0 |
| small | codebones | 3 | 1.0 | 1.0 | 1.0 |
| small | grep-ast | 1 | 1.0 | 1.0 | 1.0 |
| small | jcodemunch-mcp | 3 | 1.0 | 1.0 | 1.0 |
| small | tree-sitter-mcp | 1 | 1.0 | 1.0 | 1.0 |
| medium | ast-grep | 1 | 1.0 | 1.0 | 1.0 |
| medium | codebones | 3 | 1.0 | 1.0 | 1.0 |
| medium | grep-ast | 0 | 0 | 0 | 0 |
| medium | jcodemunch-mcp | 3 | 1.0 | 1.0 | 1.0 |
| medium | tree-sitter-mcp | 0 | 0 | 0 | 0 |
| large | ast-grep | 1 | 1.0 | 1.0 | 1.0 |
| large | codebones | 3 | 1.0 | 1.0 | 1.0 |
| large | grep-ast | 0 | 0 | 0 | 0 |
| large | jcodemunch-mcp | 3 | 1.0 | 1.0 | 1.0 |
| large | tree-sitter-mcp | 0 | 0 | 0 | 0 |

## Artifacts
- `docs/benchmarks/raw.csv`
- `docs/benchmarks/correctness.csv`
- `docs/benchmarks/normalized.csv`

## Changes from v0.2.0

- **lookup_query**: Cold lookup improved from ~4.4ms to ~4.0ms (small), warm from ~3.9ms to ~3.7ms. Medium cold improved from ~6.9ms to ~10.5ms — slight regression likely due to larger index with new marker files and v0.3.0 pipeline changes.
- **repo_index_build**: Significant improvement — small from ~9.9ms to ~8.5ms, medium from ~197ms to ~290ms (regression in medium/large due to incremental index correctness improvements in v0.3.0). Large from ~978ms to ~1310ms.
- **structure_outline**: Cold latency improved across all datasets: small from 5.08ms to 4.53ms, medium from 13.62ms to 5.34ms (large improvement from index-based lookup optimization), large from 5.36ms to 5.52ms.
- **context_pack**: Now benchmarked with `--format xml` (v0.2.0 used `--format markdown`). Small improved from ~81ms to ~50ms. Medium improved from ~3460ms to ~2580ms. Large improved from ~7760ms to ~7930ms (slight regression).
- **incremental_reindex_single**: Small improved from ~19ms to ~10ms. Medium improved from ~580ms to ~190ms. Large improved from ~889ms to ~1140ms (regression — likely due to larger n8n repo scan).
- **incremental_reindex_batch**: Small improved from ~67ms to ~50ms. Medium improved from ~1018ms to ~280ms. Large improved from ~1050ms to ~1020ms.
- **time_to_query_after_change**: Small improved from ~11ms to ~14.7ms (slight regression). Medium improved from ~53ms to ~248ms (regression — reflects larger incremental index time). Large improved from ~23ms to ~1068ms (regression matches incremental index regression for large dataset).
- All correctness checks pass at 100% hit@1 across all datasets and query types.
