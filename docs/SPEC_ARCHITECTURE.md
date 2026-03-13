# Codebones <🦴> : Architecture Specification

## 1. Core Philosophy
The architecture is designed around three principles:
1.  **Speed:** Agents cannot wait for full-file AST parsing on every request.
2.  **Safety:** Rust provides memory safety and fearless concurrency.
3.  **Testability (TDD):** The core logic must be decoupled from the CLI/Python bindings to allow for extensive, fixture-based unit testing and Test-Driven Development (TDD) from Day 1.

## 2. Tech Stack
*   **Language:** Rust (Edition 2021)
*   **Parser:** `tree-sitter` (Rust native bindings) + static language grammars.
*   **Caching/Storage:** `rusqlite` (SQLite) for persistent indexing and O(1) blob retrieval.
*   **Python Bindings:** `pyo3` and `maturin`.
*   **CLI Framework:** `clap`.
*   **Error Handling:** `anyhow` or `thiserror`.

## 3. System Components (Cargo Workspace)

The project is structured as a 4-crate Cargo workspace to enforce strict boundaries.

### A. The Core Library (`crates/core`)
This is the pure Rust library containing all business logic. It has no knowledge of the CLI, MCP, or Python.
*   **`indexer` module:** Handles traversing directories, respecting `.gitignore` (using the `ignore` crate), and hashing files to detect changes.
*   **`cache` module:** Wraps `rusqlite`. Manages the `.codebones.db` SQLite database. Handles storing raw file BLOBs and retrieving symbol byte-offsets.
*   **`parser` module:** Wraps `tree-sitter`. Defines `LanguageSpec` structs that query the AST to extract "bones".
*   **`plugin` module:** Defines the `ContextPlugin` trait, allowing extensibility (e.g., extracting `dbt` columns or OpenAPI routes) without bloating the core parser.

### B. The CLI (`crates/cli`)
A thin wrapper around `crates/core` using `clap`.
*   Parses command-line arguments (`index`, `outline`, `get`, `search`, `pack`).
*   Handles stdout, stderr, and exit codes.

### C. The MCP Server (`crates/mcp`)
A standalone binary providing the Model Context Protocol interface.
*   Exposes `core` functions as MCP tools over stdio.

### D. The Python Bindings (`crates/python-ext`)
A thin wrapper around `crates/core` using `pyo3`.
*   Exposes Rust structs as Python classes.
*   Handles conversion between Rust `Result` and Python `Exception`.

## 4. Data Flow

### Flow 1: Indexing (`codebones index`)
1.  **Discover:** `indexer` finds all valid source files.
2.  **Hash Check:** `cache` checks if the file's SHA-256 hash has changed.
3.  **Parse:** `parser` uses `tree-sitter` to generate the AST.
4.  **Extract:** `parser` runs language-specific queries to find classes, functions, and methods.
5.  **Enrich (Plugins):** Any active `ContextPlugin`s inspect the file and add metadata to the extracted symbols.
6.  **Cache (Atomic):** `cache` begins an SQLite transaction, writes the raw file content as a BLOB, writes the symbol byte offsets, and commits.

### Flow 2: Surgical Retrieval (`codebones get <symbol>`)
1.  **Query:** `cache` queries SQLite for `<symbol>`.
2.  **Locate:** SQLite returns the `file_id`, `start_byte`, and `length`.
3.  **Extract (O(1)):** SQLite uses the `substr(content, start, length)` function to extract the exact string from the BLOB.
4.  **Return:** The raw string is returned to the user/agent. No AST parsing occurs during retrieval.

## 5. Plugin Architecture
To support ecosystem-specific metadata (like `dbt`), `core` defines a `ContextPlugin` trait:

```rust
pub trait ContextPlugin {
    fn name(&self) -> &str;
    fn detect(&self, directory: &Path) -> bool;
    fn enrich(&self, file_path: &Path, base_bones: &mut Vec<Bone>) -> Result<()>;
}
```
Plugins can inject JSON metadata into a `metadata` column in the SQLite `symbols` table, which is indexed by FTS5 for fast searching.