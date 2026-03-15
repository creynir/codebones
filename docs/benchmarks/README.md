# Benchmarks

This directory contains reproducible benchmark methodology and result artifacts for `codebones`.

## Contents

- `methodology.md`: Scope, eligibility matrix rules, runtime states, dataset policy, correctness checks, normalization policy, pinned tool revisions, machine baseline, timeout policy, and exact command patterns for reproduction.
- `feature-matrix.csv`: Semantic feature eligibility per tool (`ELIGIBLE` or `OUT_OF_SCOPE`).
- `results-2026-03-15.md`: Human-readable benchmark report with per-feature tables.
- `raw-2026-03-15.csv`: Raw per-run performance measurements and statuses.
- `correctness-2026-03-15.csv`: Golden-query and incremental correctness checks.
- `normalized-2026-03-15.csv`: Normalized metrics (`ms/KLOC`, throughput, memory/ KLOC).

## Reporting Principles

- Semantic comparability first (not command-name matching).
- No ambiguous statuses (`OK`, `TIMEOUT`, `ERROR`, `OUT_OF_SCOPE`).
- Timeouts are explicit and carry `timeout_ms`.
- Correctness is reported alongside performance.
- Raw data is published with summary tables.
