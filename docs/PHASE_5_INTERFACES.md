# Phase 5: Interfaces (CLI, MCP, Python)

This specification defines the three primary interfaces for the `codebones` core library: the CLI, the MCP server, and the Python bindings.

## 1. CLI Structure (`crates/cli/src/main.rs`)

The CLI is built using `clap` (derive API) and exposes the 5 core commands.

```rust
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "codebones", version, about = "Strip codebases down to their structural skeleton", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Builds or updates the SQLite cache for the given directory
    Index {
        /// The directory to index (defaults to current directory)
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
    /// Prints the file tree or the skeleton of a specific file
    Outline {
        /// The path to a file or directory
        path: PathBuf,
    },
    /// Retrieves the full source code for a specific symbol or file
    Get {
        /// The symbol name (e.g., `src/main.rs::Database.connect`) or file path
        symbol_or_path: String,
    },
    /// Searches for symbols or text across the repository using FTS5
    Search {
        /// The search query
        query: String,
    },
    /// Packs the repository's skeleton into a single string for LLM context
    Pack {
        /// The directory to pack (defaults to current directory)
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Output format (e.g., xml, markdown)
        #[arg(short, long, default_value = "markdown")]
        format: String,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    // Match on cli.command and dispatch to crates/core
    Ok(())
}
```

## 2. MCP Server Setup (`crates/mcp/src/main.rs`)

The MCP server exposes the core capabilities as tools over stdio, allowing AI agents (like Claude Desktop) to interact with the codebase.

```rust
use mcp_sdk::{Server, Tool};
// ... imports ...

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut server = Server::new("codebones-mcp", "0.1.0");

    server.register_tool(
        Tool::new("index", "Builds or updates the codebones index for a directory")
            .with_arg("dir", "string", "Directory to index"),
        |args| async move {
            // Call core::indexer
            Ok("Indexing complete".into())
        }
    );

    server.register_tool(
        Tool::new("outline", "Gets the skeleton outline of a file or directory")
            .with_arg("path", "string", "Path to file or directory"),
        |args| async move {
            // Call core::outline
            Ok("...".into())
        }
    );

    server.register_tool(
        Tool::new("get", "Retrieves the full source code for a specific symbol")
            .with_arg("symbol", "string", "Symbol name to retrieve"),
        |args| async move {
            // Call core::get
            Ok("...".into())
        }
    );

    server.register_tool(
        Tool::new("search", "Searches for symbols across the repository")
            .with_arg("query", "string", "Search query"),
        |args| async move {
            // Call core::search
            Ok("...".into())
        }
    );

    // Start listening on stdio
    server.serve_stdio().await?;
    Ok(())
}
```

## 3. Python Bindings (`crates/python-ext/src/lib.rs`)

Using `pyo3`, we expose a `Codebones` module/class that Python developers can use directly.

```rust
use pyo3::prelude::*;
use std::path::PathBuf;

#[pyclass]
struct Codebones {
    // Internal state, e.g., cache connection or config
}

#[pymethods]
impl Codebones {
    #[new]
    fn new() -> Self {
        Codebones {}
    }

    #[staticmethod]
    fn index(dir: String) -> PyResult<()> {
        // Call core::indexer
        Ok(())
    }

    #[staticmethod]
    fn outline(path: String) -> PyResult<String> {
        // Call core::outline
        Ok("...".to_string())
    }

    #[staticmethod]
    fn get(symbol_name: String) -> PyResult<String> {
        // Call core::get
        Ok("...".to_string())
    }

    #[staticmethod]
    fn search(query: String) -> PyResult<Vec<String>> {
        // Call core::search
        Ok(vec![])
    }

    #[staticmethod]
    fn pack(dir: String, format: Option<String>) -> PyResult<String> {
        // Call core::pack
        Ok("...".to_string())
    }
}

#[pymodule]
fn codebones(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<Codebones>()?;
    
    // Also expose static methods directly on the module for ease of use
    // e.g. codebones.index("./src")
    
    Ok(())
}
```

## 4. TDD Integration/E2E Tests

The test-writing agent must implement the following strict integration tests to verify these interfaces:

1. **CLI `index` and `get` E2E**: Run `cargo run --bin codebones index .` on a fixture directory, then run `cargo run --bin codebones get MyClass.my_method` and assert the stdout matches the exact method source code.
2. **CLI `pack` Format Test**: Run `codebones pack --format xml` on a fixture directory and assert the stdout is valid XML containing the expected file skeletons.
3. **MCP Tool Execution**: Instantiate the MCP server in-memory (or via stdio pipes) and send a JSON-RPC request to execute the `outline` tool on a fixture file. Assert the JSON-RPC response contains the correct skeleton.
4. **Python API `get` Exception Handling**: In a Python test environment (using `pytest` or inline `pyo3` testing), call `codebones.get("NonExistentSymbol")` and assert it raises a specific, catchable Python exception (e.g., `ValueError` or a custom `SymbolNotFoundError`).
5. **Python API End-to-End**: In Python, call `codebones.index()`, then `codebones.search()`, and assert the returned list of dictionaries/strings matches the expected symbols from the fixture.
6. **CLI `search` FTS5 Verification**: Run `codebones search "database connection"` and verify that the SQLite FTS5 engine correctly returns symbols that match the query fuzzily or exactly, asserting the exit code is 0.
