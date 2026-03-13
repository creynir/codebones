# 🦴 codebones

`codebones` is an ultra-fast, AST-aware code indexing and context extraction tool. It allows you to extract full files or gracefully degraded "skeletons" of your codebase to feed into Large Language Models (LLMs), minimizing token usage while maximizing context.

It can be used as:
1. A standalone **CLI tool**
2. A fast **Model Context Protocol (MCP) server**
3. A **Python library** (`pip install codebones`)
4. A **Rust crate** (`cargo install codebones`)

## Features

- **AST-Aware Parsing:** Uses `tree-sitter` to parse code structure (supports Rust, Python, Go, TypeScript/JavaScript, Java, C/C++, C#, Ruby, PHP, and Swift).
- **Graceful Degradation:** When packing context for an LLM, `codebones` respects a token budget (e.g., `--max-tokens 100000`). If the budget is exceeded, it gracefully degrades from full file contents to purely structural "bones" (class names, function signatures, etc.).
- **Skeleton Map:** Generates an Aider-style hierarchical repository map at the top of context payloads to give LLMs an immediate understanding of the codebase structure.
- **O(1) Symbol Retrieval:** Backed by an atomic SQLite cache, allowing immediate O(1) reads of exact symbol contents via `substr()` caching.

## Installation

### As a CLI Tool (Rust)
```bash
cargo install codebones
```

### As a Python Library
```bash
pip install codebones
```

## CLI Usage

### 1. Index a repository
Index the current directory (caches file hashes, ASTs, and symbols in a `.codebones` SQLite database).
```bash
codebones index .
```

### 2. Pack context for an LLM
Pack the repository into a single, AI-friendly markdown/XML payload.
```bash
codebones pack . --format markdown --max-tokens 120000 > context.md
```

### 3. Search for symbols
```bash
codebones search "Authentication"
```

### 4. Get specific symbol content
```bash
codebones get "MyClass.my_method"
```

### 5. View file outline (Skeleton)
```bash
codebones outline src/main.rs
```

## MCP Server

`codebones` includes a built-in Model Context Protocol (MCP) server, allowing AI agents (like Claude Desktop or Cursor) to dynamically query the AST and search your codebase in real-time.

```bash
codebones-mcp
```

## Documentation

For AI agents and developers looking to write custom context plugins, please refer to the [Plugin Authoring Guide](docs/PLUGIN_AUTHORING_GUIDE.md).
