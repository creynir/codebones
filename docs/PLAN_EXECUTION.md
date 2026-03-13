# Codebones <🦴> : Execution & Testing Plan

## 1. Development Methodology
We will use a **Test-Driven Development (TDD)** approach orchestrated by AI agents. 
*   **The Architect (Me):** I will define the interfaces, write the failing tests (the contract), and oversee the architecture.
*   **The Builders (Sub-agents):** I will dispatch sub-agents to write the implementation code that makes the tests pass.
*   **The Reviewer (You):** You will review the code, run the tests, and approve the progression to the next phase.

## 2. Testing Strategy

### A. Unit & Integration Tests (TDD)
*   **Fixture-Based:** We will maintain a `tests/fixtures/` directory containing sample code in various languages (Python, Rust, JS).
*   **Parser Tests:** Assert that Tree-sitter correctly extracts the expected "bones" from the fixtures.
*   **Cache Tests:** Assert that SQLite correctly stores BLOBs, handles incremental updates (hashing), and retrieves substrings accurately.

### B. Security Tests (Inspired by jcodemunch)
Security is critical when an LLM has access to the filesystem. We must write tests to ensure:
*   **Path Traversal Prevention:** `codebones get ../../../etc/passwd` must fail.
*   **Symlink Escapes:** Malicious symlinks pointing outside the workspace must be ignored.
*   **Secret Exclusion:** `.env`, `*.pem`, and other secret files must be excluded from the index by default.
*   **Binary Detection:** The indexer must skip binary files to prevent database bloat and encoding panics.

### C. End-to-End (E2E) Tests
*   We will use a tool like `assert_cmd` in Rust to test the compiled CLI binaries.
*   We will test the full pipeline: `cargo run -- index fixtures/` -> `cargo run -- get MyClass` -> Assert stdout matches expected output.

## 3. Phase-by-Phase Execution Plan

We will build the project in 5 distinct phases. Each phase will have its own dedicated specification document (`docs/PHASE_X.md`) detailing the exact Rust traits, structs, and tests required.

### Phase 1: Workspace & Core Foundations
*   **Goal:** Set up the 4-crate Cargo workspace and define the core domain models (`Bone`, `FileHash`).
*   **TDD Focus:** Write tests for the `indexer` module (file walking, `.gitignore` respecting, hashing, and security filters).

### Phase 2: The SQLite Cache Layer
*   **Goal:** Implement the `cache` module using `rusqlite`.
*   **TDD Focus:** Write tests for atomic transactions, storing file BLOBs, inserting symbols, and retrieving O(1) substrings.

### Phase 3: The Tree-sitter Parser
*   **Goal:** Implement the `parser` module and statically link the core languages.
*   **TDD Focus:** Write fixture tests asserting that the S-expression queries correctly identify classes, functions, and bodies to elide.

### Phase 4: The Plugin System & Pack Command
*   **Goal:** Implement the `ContextPlugin` trait and the `formatter` module for the `pack` command.
*   **TDD Focus:** Write tests for a dummy plugin injecting metadata, and snapshot tests for the XML/Markdown `pack` output.

### Phase 5: Interfaces (CLI, MCP, Python)
*   **Goal:** Wire the `core` library to `crates/cli` (`clap`), `crates/mcp` (stdio), and `crates/python-ext` (`pyo3`).
*   **TDD Focus:** E2E CLI tests using `assert_cmd` and Python unit tests using `pytest`.