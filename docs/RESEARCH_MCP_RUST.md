# Rust MCP SDK Research & Architecture Recommendation

## 1. State of Rust MCP SDKs
**Recommendation:** Use the official `rmcp` crate instead of a raw JSON-RPC loop.

- **Current Status:** The official Rust SDK for the Model Context Protocol is available on crates.io as `rmcp` (currently v1.2.0 as of March 2026). 
- **Why use it:** It provides robust, out-of-the-box features including async/await support via `tokio`, standard transports (stdio, HTTP), OAuth support, and procedural macros (`rmcp-macros`) to easily generate schemas and route tool implementations. Writing a raw JSON-RPC loop from scratch with `tokio` and `serde_json` would require manually handling protocol edge cases, JSON-RPC parsing, schema generation, and error formatting, which `rmcp` already handles reliably.

## 2. Architecture: Thin CLI Wrapper vs. Direct Core Import
**Recommendation:** Import `codebones-core` directly into the MCP server rather than executing the CLI as a subprocess.

### Thin Wrapper (Subprocess `codebones search ...`)
- **Pros:** 
  - Complete decoupling from core logic.
  - Reuses CLI formatting and error handling without adding dependencies to the MCP server.
- **Cons:** 
  - **Performance overhead:** Spawning a new child process for every tool call is slow, which degrades the LLM's perceived responsiveness.
  - **State & Connection initialization:** Every invocation requires spinning up a new SQLite connection to the cache instead of reusing an active connection pool.
  - **Parsing friction:** The MCP server needs structured JSON to return to the LLM. If the CLI outputs raw text/tables, you'll have to parse it or force the CLI to output JSON (`--json`), adding serialization/deserialization overhead.

### Direct Import (`codebones-core`)
- **Pros:**
  - **O(1) Connections:** You can initialize the SQLite cache connection pool *once* when the MCP server starts and hold it in memory. This provides massive performance benefits for high-frequency LLM tool calls.
  - **Zero process overhead:** Function calls are direct, native Rust invocations.
  - **Strong Typing:** Core functions return native Rust structs (`Result<T, E>`), which map perfectly into `rmcp` macros and `serde_json` for seamless, strictly-typed tool responses.
- **Cons:**
  - Slightly increased binary size for the MCP server.
  - Tighter coupling to the core library's API changes.

**Conclusion:** 
For an optimal LLM experience where speed and structured data are paramount, the O(1) in-memory SQLite connections and strongly-typed zero-overhead direct calls make importing `codebones-core` the clear winner. Use `rmcp` to expose the core as an MCP server.
