# Token Savings Benchmark

Measures how many tokens an AI agent needs to consume for common tasks — with codebones vs reading raw source files.

## Datasets

Same pinned repos as the speed benchmarks:

| Label | Repo | LOC | Language | Commit |
|---|---|---:|---|---|
| small | [hadywalied/agenthelm](https://github.com/hadywalied/agenthelm) | 6,250 | Python | `9ec76caae764` |
| medium | [temporalio/temporal](https://github.com/temporalio/temporal) | 832,991 | Go | `29a039286526` |
| large | [n8n-io/n8n](https://github.com/n8n-io/n8n) | 2,068,515 | TypeScript | `f7a787aca81c` |

## Scenarios

### 1. Project Orientation

**Task:** "Understand the architecture of this project before making changes."

| Metric | Without codebones | With codebones |
|--------|-------------------|----------------|
| Method | Read all source files | `codebones map --format markdown` |
| What the AI sees | Every line of every file | File paths + symbol signatures |

**Measured:** total tokens (raw source) vs tokens (map output). Reduction ratio = map / raw.

### 2. Impact Analysis

**Task:** "What breaks if I change this file?"

For each dataset, pick the 3 most-imported files (via `codebones graph --top 3`). For each:

| Metric | Without codebones | With codebones |
|--------|-------------------|----------------|
| Method | Read all source files (AI traces imports manually) | `codebones graph <file>` |
| What the AI sees | Entire codebase | Affected file list only |

**Measured:** total tokens (all source) vs tokens (graph output). The "without" cost is the same for every file because the AI has no way to know which files are relevant without reading them all.

### 3. Symbol Retrieval

**Task:** "Find and read the implementation of function X."

For each dataset, pick 3 known symbols. For each:

| Metric | Without codebones | With codebones |
|--------|-------------------|----------------|
| Method | Read files until found (grep + cat) | `codebones search X` + `codebones get X` |
| What the AI sees | Full content of files containing the symbol name | Search results + targeted symbol source |

**Measured:** tokens (files containing the symbol name) vs tokens (search + get output).

### 4. Budget Efficiency

**Task:** "Fit this project into a fixed token budget."

At budgets of 8K, 16K, 32K, and 64K tokens:

| Metric | Without codebones | With codebones |
|--------|-------------------|----------------|
| Method | Alphabetical file dump, hard truncation at budget | `codebones pack --max-tokens N` |
| What the AI sees | First N tokens of concatenated files | Skeleton map + as many file bodies as fit |

**Measured:** number of symbols visible to the AI at each budget level. Codebones preserves the skeleton map (all symbols) even when file bodies are dropped; raw truncation loses everything after the cutoff.

## Running the benchmark

```bash
# Prerequisites: codebones built and in PATH, Python 3 with tiktoken
cargo install --path crates/cli
pip install tiktoken

# Run all scenarios (clones datasets into lab/ on first run)
./docs/benchmarks/run_token_savings.sh

# Output: docs/benchmarks/token-savings.csv
```

The script is self-contained — it clones each dataset into `lab/` at the pinned commit on first run. Subsequent runs reuse the existing clones. The `lab/` directory is gitignored.

## Token counting

Tokens are counted using `tiktoken` with the `cl100k_base` encoding (same tokenizer used by codebones internally for `--max-tokens`). Character-based approximations are not used.
