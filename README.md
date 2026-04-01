# 🦴 codebones

**AST-aware code indexing for LLMs.** Token-budget packing with graceful degradation — full files when there's room, structural skeletons when there isn't.

[![Crates.io](https://img.shields.io/crates/v/codebones)](https://crates.io/crates/codebones)
[![PyPI](https://img.shields.io/pypi/v/codebones)](https://pypi.org/project/codebones/)
[![License: MIT](https://img.shields.io/crates/l/codebones)](https://github.com/creynir/codebones/blob/main/LICENSE)
[![CI](https://github.com/creynir/codebones/actions/workflows/test.yml/badge.svg)](https://github.com/creynir/codebones/actions/workflows/test.yml)

<p align="center">
  <img src="assets/demo.gif" alt="codebones demo" width="800" />
</p>

```xml
<repository>
  <skeleton_map>
    <file path="./src/api.rs">
      <signature>Function index</signature>
      <signature>Function pack</signature>
      <signature>Function search</signature>
      <signature>Function get</signature>
    </file>
    <file path="./src/parser.rs">
      <signature>Struct LanguageSpec</signature>
      <signature>Function parse_document</signature>
      <signature>Function extract_symbols</signature>
    </file>
  </skeleton_map>
  <file path="./src/api.rs">
    <content><![CDATA[
pub fn index(dir: &Path) -> Result<()> ...

pub fn pack(dir: &Path, format: Format, max_tokens: Option<usize>) -> Result<String> ...

pub fn search(dir: &Path, query: &str) -> Result<Vec<String>> ...

pub fn get(dir: &Path, symbol: &str) -> Result<String> ...
]]></content>
  </file>
</repository>
```

codebones parses your codebase with tree-sitter, caches the AST in SQLite, and packs everything into a single LLM-ready payload. When the token budget runs out, it drops function bodies and keeps signatures — so the model always sees the full structure.
Symbol lookup takes 4ms on a 2M LOC codebase. Competitors time out.

## Install

```bash
cargo install codebones
```

```bash
pip install codebones
```

The Python package installs the `codebones` and `codebones-mcp` binaries. It does not currently expose a separate Python API.

## Quick start

```bash
# Index the current repo (creates codebones.db)
codebones index .

# Pack into a single AI-ready payload within a token budget
codebones pack . --format markdown --max-tokens 120000 > context.md

# Search for symbols across the codebase
codebones search "Authentication"

# Retrieve a specific symbol's full source
codebones get "MyClass.my_method"

# View a file's structural skeleton
codebones outline src/main.rs

# Query an indexed repo without changing cwd
codebones search --dir /path/to/repo "Authentication"
```

## What it does

| Feature | What you get |
|---|---|
| **AST-aware parsing** | Function signatures, class hierarchies, and impl blocks extracted via tree-sitter across 11 languages |
| **Token-budget packing** | Full files until the budget fills, then automatic degradation to structural skeletons — no manual trimming |
| **Skeleton map** | Aider-style hierarchical repo map at the top of every payload so the LLM orients instantly |
| **O(1) symbol retrieval** | SQLite cache with byte-offset indexing — `substr()` reads, no re-parsing |
| **Secret filtering** | `.env`, private keys, credentials, and PEM files automatically excluded from output |
| **Incremental indexing** | SHA-256 file hashing — only re-parses changed files on subsequent runs |

**Supported languages:** Rust, Python, Go, TypeScript, JavaScript, Java, C, C++, C#, Ruby, PHP, Swift.

## Output

### `codebones pack --format markdown`

```markdown
## Skeleton Map

- ./main.py
  - Function add
  - Class Calculator
  - Function Calculator.__init__
  - Function Calculator.multiply
- ./test.rs
  - Function greet
  - Impl User
  - Function User.new
  - Function User.display

## ./main.py

def add(a, b):...

class Calculator:...

## ./test.rs

/// A greeting function
pub fn greet(name: &str) -> String ...

pub struct User ...

impl User ...
```

### `codebones outline src/main.rs`

```
/// A greeting function
pub fn greet(name: &str) -> String ...

pub struct User ...

impl User ...
```

Bodies are replaced with `...`. Doc comments and signatures are preserved.

## How it works

1. **Index** — Walks the directory, filters out binaries and secrets, hashes each file with SHA-256. Only changed files are re-parsed.
2. **Parse** — Tree-sitter extracts symbols (functions, classes, structs, impls) with byte ranges and qualified names (`MyClass.my_method`).
3. **Cache** — Symbols and file contents are stored in a SQLite database (`.codebones`). Byte offsets enable O(1) retrieval via `substr()`.
4. **Pack** — Assembles a Markdown or XML payload. Counts tokens with `tiktoken` (cl100k_base). When the budget is exceeded, drops file contents and keeps the skeleton map.

## Benchmarks

All numbers are cold-start medians in milliseconds. Full methodology and raw data in [docs/benchmarks/](docs/benchmarks/README.md).

### Symbol lookup

| Dataset | codebones | ast-grep | grep-ast | tree-sitter-mcp | jcodemunch-mcp |
|---|---:|---:|---:|---:|---:|
| 6,250 LOC | **4.02** | 11.93 | 484.22 | 196.19 | 58.50 |
| 832,991 LOC | **10.45** | 432.79 | 8,208.27 | TIMEOUT | 199.83 |
| 2,068,515 LOC | **11.82** | 1,998.44 | TIMEOUT | TIMEOUT | 104.54 |

### Indexing

| Dataset | codebones | jcodemunch-mcp |
|---|---:|---:|
| 6,250 LOC | **8.45** | 76.77 |
| 832,991 LOC | **290** | 754.05 |
| 2,068,515 LOC | **1,310** | 8,249.67 |

### Context packing

| Dataset | codebones | repomix |
|---|---:|---:|
| 6,250 LOC | **50** | 947 |
| 832,991 LOC | **2,580** | 10,237 |
| 2,068,515 LOC | **7,930** | 11,548 |

100% correctness (hit@1, precision, recall) across all datasets. Benchmark machine: macOS 15.7.1, Apple M4, 16 GB RAM.

## Limitations

- **Language coverage** — Only 11 languages have AST support. Unsupported files are indexed as plain text (no symbol extraction or body elision).
- **File size cap** — Files over 500 KB are skipped. Large generated files and vendored code won't appear in output.
- **Scope tracking** — Qualified names are built from AST container nodes (class, impl, namespace). Some scope types aren't tracked: Go packages, Python module-level groupings, Rust trait bounds.
- **Inline functions** — Single-expression bodies (Python lambdas, Rust closures, JS arrow functions in class fields) may not be elided correctly.
- **Symlinks** — Skipped by default. When enabled, symlinks pointing outside the workspace root are rejected to prevent path traversal.

## MCP server

codebones includes a Model Context Protocol server for real-time codebase queries from AI agents (Claude Desktop, Cursor).

```bash
codebones-mcp
```

Exposes `index`, `outline`, `get`, and `search` as MCP tools.

## CLI reference

```
codebones index <dir>                        Build/update the SQLite cache
codebones pack <dir> [options]               Pack repo into LLM-ready payload
codebones search [--dir <repo>] <query>      Substring search across symbol names
codebones get [--dir <repo>] <symbol>        Retrieve full source by symbol ID or file path
codebones outline [--dir <repo>] <path>      Skeleton view of an indexed file
```

### `pack` options

| Flag | Description |
|---|---|
| `--format markdown\|xml` | Output format (default: xml) |
| `--max-tokens N` | Token budget — triggers degradation when exceeded |
| `--no-files` | Skeleton map only, no file contents |
| `--no-file-summary` | File contents only, no skeleton map |
| `--remove-comments` | Strip comments from output |

## Works with Phalanx

[Phalanx](https://github.com/creynir/phalanx) is a multi-agent orchestration CLI. When phalanx agents work on a real codebase, codebones gives them a structural map upfront so each agent goes straight to work instead of spending tokens on code discovery.

```bash
codebones pack . --format markdown --max-tokens 40000 > context.md
```

Reference the output in your agent prompts so every agent arrives oriented.

## Plugins

Domain-specific metadata can be injected via the `ContextPlugin` trait. See the [Plugin Authoring Guide](docs/PLUGIN_AUTHORING_GUIDE.md) for examples (dbt, OpenAPI, GraphQL).

## Contributing

Issues and pull requests are welcome. For questions and ideas, start a thread in [Discussions](https://github.com/creynir/codebones/discussions).

## License

[MIT](LICENSE)
