# Codebones <🦴> : Objective and UX Specification

## 1. Objective
`codebones` is a blazing-fast, language-agnostic tool designed to strip codebases down to their structural skeleton (signatures, classes, imports, constants) using Abstract Syntax Tree (AST) parsing. 

**The Goal:** Save up to 80% of LLM token context while maintaining perfect semantic understanding of the codebase structure.

**Target Audience:**
1. **AI Agents (Interactive):** Tools like Cursor, Aider, and Claude Code that need surgical, instant O(1) symbol lookups in a terminal environment.
2. **Humans (One-Shot):** Developers who need to pack an entire repository into a single, token-optimized prompt for web-based LLMs.

## 2. Branding
*   **Name:** `codebones`
*   **Metaphor:** Strip the meat (implementation details). Feed your LLM the bones (structural AST skeleton).
*   **Logo/Icon:** `<🦴>`

## 3. Packaging & Distribution
To maximize reach across both systems-level developers and the AI/Python ecosystem, `codebones` uses a Hybrid Core distribution model:

1.  **Rust Binary (Crates.io):** For maximum speed, memory safety, and standalone usage (`cargo install codebones`).
2.  **Python Package (PyPI):** Using `PyO3` and `Maturin`, the Rust core is compiled into a native Python module. Running `pip install codebones` installs the CLI *and* provides a Python API (`import codebones`).
3.  **MCP Server:** A dedicated Rust binary (`codebones-mcp`) that exposes the core API over stdio for Claude Desktop and other MCP clients.

## 4. CLI Interface & Commands (The 80/20 API)

The CLI is built using `clap` (Rust) and provides 6 core commands that cover 90% of the value needed by AI agents.

### 1. `codebones index [dir]`
*   **What it does:** Walks the directory, hashes files, parses them with Tree-sitter, and builds the SQLite `.codebones.db` cache. Performs incremental updates on subsequent runs.
*   **Output:** Indexing statistics (files parsed, symbols extracted, time taken).

### 2. `codebones outline [path]`
*   **What it does:** 
    *   If `path` is a directory: Prints the file tree.
    *   If `path` is a file: Prints the "skeleton" (imports, classes, function signatures).
*   **Output:** Formatted text representation of the structure.

### 3. `codebones get <symbol_name>`
*   **What it does:** Instantly retrieves the full source code for a specific symbol (e.g., `src/main.rs::Database.connect`) from the SQLite cache. If a file path is passed, returns the whole file.
*   **Output:** The exact lines of code for that symbol, extracted via O(1) byte-offset seeking.

### 4. `codebones search <query>`
*   **What it does:** Uses SQLite's built-in Full-Text Search (FTS5) to instantly find symbols or text across the repo.
*   **Output:** List of matching `symbol_id`s and their file paths.

### 5. `codebones pack [dir]`
*   **What it does:** Runs `index` in memory, extracts all skeletons, and prints a single XML/Markdown string containing the whole repo's structure. Designed for humans to paste into web LLMs.
*   **Output:** A single string containing the directory structure and the skeleton of all supported files.

### 6. `codebones-mcp` (Separate Binary)
*   **What it does:** Starts an MCP server over stdio, exposing the core commands (`index`, `outline`, `get`, `search`) as MCP tools.

## 5. Python API Interface

The Python API mirrors the CLI but returns structured data (Pydantic models / Dicts) instead of strings.

```python
import codebones

# Build/update the index
codebones.index("./src")

# Get a specific symbol's full code
symbol_code = codebones.get("DatabaseConnection")

# Get the outline of a file
outline = codebones.outline("src/main.rs")

# Pack a directory
context = codebones.pack("./src", format="xml")
```