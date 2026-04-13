# codebones

**AST-aware code indexing for LLMs.** Token-budget packing with graceful degradation — full files when there's room, structural skeletons when there isn't.

[![Crates.io](https://img.shields.io/crates/v/codebones)](https://crates.io/crates/codebones)
[![Downloads](https://img.shields.io/crates/d/codebones)](https://crates.io/crates/codebones)
[![PyPI](https://img.shields.io/pypi/v/codebones)](https://pypi.org/project/codebones/)
[![License: MIT](https://img.shields.io/crates/l/codebones)](https://github.com/creynir/codebones/blob/main/LICENSE)

<p align="center">
  <img src="assets/demo.gif" alt="codebones demo" width="800" />
</p>

codebones parses your codebase with tree-sitter, caches the AST in SQLite, and packs everything into a single LLM-ready payload. When the token budget runs out, it drops function bodies and keeps signatures — so the model always sees the full structure.

A `codebones map` of the n8n codebase (2M LOC) is **22x smaller** than the raw source — 691K tokens instead of 14.9M. Symbol lookup takes 4ms on a repo that size. Competitors time out.

## Token savings

### Static: codebones map vs raw source

| Project | Raw source | codebones map | Reduction |
|---|---:|---:|---:|
| [FastAPI](https://github.com/tiangolo/fastapi) (107K LOC, Python) | 689,433 | 83,751 | **8x** |
| [temporal](https://github.com/temporalio/temporal) (833K LOC, Go) | 7,337,966 | 298,330 | **25x** |
| [n8n](https://github.com/n8n-io/n8n) (2.07M LOC, TypeScript) | 14,945,989 | 690,544 | **22x** |

### Real-world: agent eval on FastAPI

Two Claude Sonnet agents solve the same task on [FastAPI](https://github.com/tiangolo/fastapi) (107K LOC). One has only standard tools (grep, cat, find, ls). The other has standard tools plus codebones. No turn limit — agents work until done.

| Task | Standard only | Standard + codebones | Tokens saved | Turns saved |
|------|---:|---:|---:|---:|
| Add CORS middleware | 65K tokens, 27 calls, 15 turns | 55K tokens, 23 calls, 9 turns | **1.2x** | **40%** |
| Trace dependency bug | 144K tokens, 34 calls, 20 turns | 114K tokens, 16 calls, 10 turns | **1.3x** | **53%** |

The codebones agent finishes in half the turns. `codebones search` jumps directly to symbols instead of browsing directories, and `codebones get` reads one function instead of the whole file. Full conversation logs in [docs/benchmarks/agent-eval-results/](docs/benchmarks/agent-eval-results/).

## Install

```bash
cargo install codebones
```

```bash
pip install codebones
```

The Python package installs the `codebones` and `codebones-mcp` binaries.

## Quick start

```bash
# Index the current repo
codebones index .

# Skeleton map — structural overview without file contents
codebones map

# Pack into a single AI-ready payload within a token budget
codebones pack . --format markdown --max-tokens 120000 > context.md

# Import graph — see which files are most imported
codebones graph

# Blast radius — what breaks if you change this file?
codebones graph src/api.rs

# Search for symbols across the codebase
codebones search "Authentication"

# Retrieve a specific symbol's full source
codebones get "MyClass.my_method"

# View a file's structural skeleton
codebones outline src/main.rs

# Register codebones with Claude Code, Cursor, etc.
codebones init
```

## What it does

| Feature | What you get |
|---|---|
| **AST-aware parsing** | Function signatures, class hierarchies, and impl blocks extracted via tree-sitter across 12 languages |
| **Import graph** | Dependency tracking across all 12 languages — see which files import what, find hot files, and compute blast radius for any change |
| **Token-budget packing** | Full files until the budget fills, then automatic degradation to structural skeletons — no manual trimming |
| **Skeleton map** | Hierarchical repo map at the top of every payload so the LLM orients instantly |
| **O(1) symbol retrieval** | SQLite cache with byte-offset indexing — `substr()` reads, no re-parsing |
| **Secret filtering** | `.env`, private keys, credentials, and PEM files automatically excluded from output |
| **Incremental indexing** | SHA-256 file hashing — only re-parses changed files on subsequent runs |
| **First-run setup** | `index` auto-creates `.codebones/`, adds to `.gitignore`, and appends hints to `CLAUDE.md`/`AGENTS.md` |

**Supported languages:** Rust, Python, Go, TypeScript, JavaScript, Java, C, C++, C#, Ruby, PHP, Swift.

## Output

### `codebones map --format markdown`

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
```

### `codebones outline src/main.rs`

```
/// A greeting function
pub fn greet(name: &str) -> String ...

pub struct User ...

impl User ...
```

Bodies are replaced with `...`. Doc comments and signatures are preserved.

### `codebones graph --format markdown`

```markdown
# Import Graph

## Most Imported Files
- `src/db.ts` — imported by **3** files
- `src/utils.ts` — imported by **2** files

## Import Map
- `src/main.ts` -> src/utils.ts, src/db.ts
- `src/utils.ts` -> src/db.ts
```

### `codebones graph src/db.ts`

```markdown
# Blast Radius: src/db.ts

## Affected Files (2)
- src/utils.ts
- src/main.ts
```

## How it works

1. **Index** — Walks the directory, filters out binaries and secrets, hashes each file with SHA-256. Only changed files are re-parsed. Extracts import statements across all 12 languages and builds the dependency graph.
2. **Parse** — Tree-sitter extracts symbols (functions, classes, structs, impls) with byte ranges and qualified names (`MyClass.my_method`), plus import/dependency edges.
3. **Cache** — Symbols, imports, and file contents are stored in a SQLite database (`.codebones/codebones.db`). Byte offsets enable O(1) retrieval via `substr()`.
4. **Pack** — Assembles a Markdown or XML payload. Counts tokens with `tiktoken` (cl100k_base). When the budget is exceeded, drops file contents and keeps the skeleton map.

## Query performance

All numbers are cold-start medians in milliseconds. Full methodology and raw data in [docs/benchmarks/](docs/benchmarks/).

### Symbol lookup

| Dataset | codebones | ast-grep | grep-ast | tree-sitter-mcp | jcodemunch-mcp |
|---|---:|---:|---:|---:|---:|
| 6.25K LOC (Python) | **4.02** | 11.93 | 484.22 | 196.19 | 58.50 |
| temporal (833K LOC, Go) | **10.45** | 432.79 | 8,208.27 | TIMEOUT | 199.83 |
| n8n (2.07M LOC, TypeScript) | **11.82** | 1,998.44 | TIMEOUT | TIMEOUT | 104.54 |

### Context packing

| Dataset | codebones | repomix |
|---|---:|---:|
| 6.25K LOC (Python) | **101** | 947 |
| temporal (833K LOC, Go) | **4,025** | 10,237 |
| n8n (2.07M LOC, TypeScript) | **8,511** | 11,548 |

### Import graph + blast radius

| Dataset | graph | map |
|---|---:|---:|
| 6.25K LOC (Python) | **26ms** | **32ms** |
| temporal (833K LOC, Go) | **39ms** | **303ms** |
| n8n (2.07M LOC, TypeScript) | **56ms** | **1,369ms** |

100% correctness (hit@1, precision, recall) across all datasets. Benchmark machine: macOS 15.7.1, Apple M4, 16 GB RAM.

## Limitations

- **Language coverage** — 12 languages have AST support. Unsupported files are indexed as plain text (no symbol extraction or body elision).
- **File size cap** — Files over 500 KB are skipped. Large generated files and vendored code won't appear in output.
- **Scope tracking** — Qualified names are built from AST container nodes (class, impl, namespace). Some scope types aren't tracked: Go packages, Python module-level groupings, Rust trait bounds.
- **Import resolution** — Supports file-path imports (`./utils`, `../db`), Python dotted modules (`from app.core.event import Event`), and Python relative imports (`from .utils import helper`). External/stdlib imports (e.g., `import os`) are stored but don't resolve to local files.
- **Inline functions** — Single-expression bodies (Python lambdas, Rust closures, JS arrow functions in class fields) may not be elided correctly.
- **Symlinks** — Skipped by default. When enabled, symlinks pointing outside the workspace root are rejected to prevent path traversal.

## MCP server

codebones includes a Model Context Protocol server for real-time codebase queries from AI agents (Claude Desktop, Cursor).

```bash
codebones-mcp
```

Exposes `index`, `outline`, `get`, `search`, `map`, `graph`, and `graph_file` as MCP tools.

Register it globally with one command:

```bash
codebones init
```

This detects Claude Code (`~/.claude/`) and Cursor (`~/.cursor/`) and adds the MCP server to their settings — without overriding existing configs.

## CLI reference

```
codebones init                               Register codebones-mcp with AI tools
codebones index <dir>                        Build/update the cache and import graph
codebones map [dir] [options]                Skeleton map only (shorthand for pack --no-files)
codebones pack <dir> [options]               Pack repo into LLM-ready payload
codebones graph [file] [options]             Import graph, hot files, or blast radius
codebones search [--dir <repo>] <query>      Substring search across symbol names
codebones get [--dir <repo>] <symbol>        Retrieve full source by symbol ID or file path
codebones outline [--dir <repo>] <path>      Skeleton view of an indexed file
```

### `map` options

| Flag | Description |
|---|---|
| `--format markdown\|xml` | Output format (default: xml) |
| `--max-tokens N` | Token budget |
| `--include <glob>` | Only include matching files |
| `--ignore <glob>` | Exclude matching files |

### `pack` options

| Flag | Description |
|---|---|
| `--format markdown\|xml` | Output format (default: xml) |
| `--max-tokens N` | Token budget — triggers degradation when exceeded |
| `--no-files` | Skeleton map only, no file contents |
| `--no-file-summary` | File contents only, no skeleton map |
| `--remove-comments` | Strip comments from output |

### `graph` options

| Flag | Description |
|---|---|
| `<file>` | Show blast radius for this file (omit for full graph) |
| `--format markdown\|xml\|json` | Output format (default: markdown) |
| `--top N` | Show only the N most-imported files |
| `--depth N` | Blast radius BFS depth (default: 3) |

## Plugins

Domain-specific metadata can be injected via the `ContextPlugin` trait. See the [Plugin Authoring Guide](docs/PLUGIN_AUTHORING_GUIDE.md) for examples (dbt, OpenAPI, GraphQL).

## Contributing

Issues and pull requests are welcome. For questions and ideas, start a thread in [Discussions](https://github.com/creynir/codebones/discussions).

## License

[MIT](LICENSE)
