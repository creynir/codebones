# Phase 3: Tree-sitter Parser Design Document

## 1. Overview and Analysis

As part of the `codebones` project, Phase 3 focuses on implementing a Tree-sitter based parser to extract code skeletons. This allows us to index codebases efficiently by retaining the structural signatures (classes, functions, imports) while eliding the implementation details (bodies) to save LLM tokens.

### Analysis of Prior Art (7 Reference Repositories)

We analyzed 7 reference implementations to determine the best architectural approach for extracting signatures and handling nested symbols:

1.  **`jcodemunch-mcp` (Python):** Uses **manual AST walking** driven by a declarative `LanguageSpec`. It maps node types to symbol kinds, defines `container_node_types` for nesting, and dynamically finds the `"body"` field to extract the signature and elide the rest.
2.  **`grep-ast` (Python):** Uses Tree-sitter to parse code into an AST and applies heuristics to determine "important" context nodes, collapsing bodies into `...`.
3.  **`repomix` (TypeScript):** Uses S-expression queries to identify exports, classes, and functions to build a structural map.
4.  **`ast-grep` (Rust):** A highly optimized tool that uses a custom pattern-matching language (YAML rules) and S-expressions to search and rewrite ASTs. While extremely powerful, embedding its full engine might be overkill for simple signature extraction, but its rigorous node-matching logic is industry-leading.
5.  **`code-chunk` (TypeScript):** Employs a **hybrid approach**. It primarily uses Tree-sitter S-expression queries (`.scm` files) for accuracy but falls back to manual AST walking and node-type heuristics if queries fail or are unavailable.
6.  **`code-indexer-loop` (Python):** Focuses on **token-aware chunking**. It walks the AST and recursively groups nodes, using a token counter (like `tiktoken`) to ensure chunks don't exceed a maximum token limit.
7.  **`tree-sitter-mcp` (TypeScript):** Uses manual AST walking with explicit language configurations (e.g., `functionTypes`, `classTypes`). It features robust fallback logic for name extraction (e.g., if `node.childForFieldName('name')` fails, it iterates children looking for `identifier` types).

**Synthesis & Best Approach for `codebones` (Rust):**
The absolute best approach for our Rust CLI is a **Hybrid AST Walker with Robust Field Extraction and Token Awareness**, combining the strengths of `jcodemunch-mcp`, `tree-sitter-mcp`, and `code-indexer-loop`:

*   **Core Traversal:** Manual AST walking with a `LanguageSpec` (from `jcodemunch-mcp`). This avoids the overhead of maintaining complex `.scm` query files for every language while providing exact byte-range control for body elision.
*   **Robust Name Extraction:** Incorporate `tree-sitter-mcp`'s fallback logic. If a node lacks a named `"name"` field, fallback to searching for `identifier` children.
*   **Token/Size-Aware Elision:** Inspired by `code-indexer-loop`, we shouldn't blindly elide *all* bodies. If a function body is extremely short (e.g., a one-liner getter), eliding it might save negligible tokens while losing valuable context. We will track body byte-size (or token count) and only elide if it exceeds a configurable threshold.

## 2. Rust Architecture

We will use the `tree-sitter` Rust crate. The architecture relies on a core `LanguageSpec` struct and a recursive AST walker.

### Core Structs and Traits

```rust
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use tree_sitter::{Language, Node};

/// Represents the type of symbol extracted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Impl,
    Interface,
    // Add more as needed
}

/// Represents an extracted symbol from the AST.
#[derive(Debug, Clone)]
pub struct Symbol {
    /// The local name of the symbol
    pub name: String,
    /// The qualified name (e.g., "MyClass.my_method")
    pub qualified_name: String,
    /// The kind of the symbol
    pub kind: SymbolKind,
    /// The byte range of the entire definition (signature + body)
    pub full_range: Range<usize>,
    /// The byte range of the body. This is the part that will be elided with `...`
    pub body_range: Option<Range<usize>>,
    /// Whether the body was actually elided (based on size/token thresholds)
    pub is_elided: bool,
}

/// Represents a fully parsed document with all its symbols.
#[derive(Debug, Clone)]
pub struct ParsedDocument {
    pub file_path: String,
    pub symbols: Vec<Symbol>,
}

/// Configuration for extracting symbols from a specific language.
pub struct LanguageSpec {
    /// The Tree-sitter language object.
    pub language: Language,
    
    /// Maps a node type to a SymbolKind (e.g., "function_definition" -> SymbolKind::Function)
    pub symbol_node_types: HashMap<&'static str, SymbolKind>,
    
    /// Maps a node type to the field name containing its identifier (e.g., "function_definition" -> "name")
    pub name_fields: HashMap<&'static str, &'static str>,
    
    /// Node types that establish a new scope/container (e.g., "class_definition", "impl_item")
    pub container_node_types: HashSet<&'static str>,
    
    /// Node types that represent the "body" of a symbol, if not accessible via a "body" field
    pub body_node_types: HashSet<&'static str>,
}
```

## 3. Extraction Logic (AST Walking)

Instead of queries, we implement a recursive function `walk_tree`:

1.  **Node Identification:** Check if `node.kind()` is in `spec.symbol_node_types`.
2.  **Robust Name Extraction:** 
    *   Try `node.child_by_field_name(spec.name_fields[node.kind()])`.
    *   *Fallback (from `tree-sitter-mcp`):* Iterate over `node.children()`. If a child's kind is `"identifier"` or `"type_identifier"`, use its text.
3.  **Qualified Names & Nesting:** If `parent_symbol` is provided, construct `qualified_name = format!("{}.{}", parent_symbol.name, name)`. If the node is inside a container, its kind might be adjusted (e.g., `Function` becomes `Method`).
4.  **Signature vs Body & Elision Threshold:** 
    *   Look for a child node via `node.child_by_field_name("body")` or by checking if a child's kind is in `spec.body_node_types` (e.g., `"block"`).
    *   The `body_range` is `body.start_byte()..body.end_byte()`.
    *   The signature is everything from `node.start_byte()` up to `body.start_byte()`.
    *   *Size-Aware Elision (from `code-indexer-loop`):* Calculate `body.end_byte() - body.start_byte()`. If it's below `MIN_ELISION_BYTES` (e.g., 50 bytes), set `is_elided = false` and keep the body.
5.  **Recursion:** Iterate over `node.children()`. If the current node's kind is in `spec.container_node_types`, pass the newly created `Symbol` down as the `parent_symbol` for the recursive calls.

### Example Specs

**Python (`PythonSpec`)**
```rust
LanguageSpec {
    language: tree_sitter_python::language(),
    symbol_node_types: HashMap::from([
        ("function_definition", SymbolKind::Function),
        ("class_definition", SymbolKind::Class),
    ]),
    name_fields: HashMap::from([
        ("function_definition", "name"),
        ("class_definition", "name"),
    ]),
    container_node_types: HashSet::from(["class_definition"]),
    body_node_types: HashSet::from(["block"]),
}
```

**Rust (`RustSpec`)**
```rust
LanguageSpec {
    language: tree_sitter_rust::language(),
    symbol_node_types: HashMap::from([
        ("function_item", SymbolKind::Function),
        ("struct_item", SymbolKind::Struct),
        ("impl_item", SymbolKind::Impl),
    ]),
    name_fields: HashMap::from([
        ("function_item", "name"),
        ("struct_item", "name"),
        ("impl_item", "type"), // Impl blocks use the 'type' field for their name
    ]),
    container_node_types: HashSet::from(["impl_item"]),
    body_node_types: HashSet::from(["block", "declaration_list"]),
}
```

## 4. TDD Unit Tests (Strict Requirements)

The test-writing agent MUST implement the following tests before the implementation agent writes the core logic. These tests ensure the AST walking, fallback heuristics, and byte-range math are correct.

1.  **Test: Should extract Python class signature and elide body**
    *   *Input:* `class MyClass:\n    def __init__(self):\n        pass`
    *   *Assertion:* `name` is `"MyClass"`. `body_range` covers `\n    def __init__(self):\n        pass`. Replacing `body_range` with `...` yields `class MyClass:...`.
2.  **Test: Should extract Python function signature and elide body**
    *   *Input:* `def calculate_total(a: int, b: int) -> int:\n    return a + b`
    *   *Assertion:* `name` is `"calculate_total"`. Replacing `body_range` with `...` yields `def calculate_total(a: int, b: int) -> int:...`.
3.  **Test: Should extract Rust struct signature and elide body**
    *   *Input:* `pub struct User {\n    pub id: i32,\n    pub name: String,\n}`
    *   *Assertion:* `name` is `"User"`. Replacing `body_range` with `...` yields `pub struct User ...`.
4.  **Test: Should extract Rust function signature and elide body**
    *   *Input:* `pub fn process_data(data: &[u8]) -> Result<(), Error> {\n    // do work\n    Ok(())\n}`
    *   *Assertion:* `name` is `"process_data"`. Replacing `body_range` with `...` yields `pub fn process_data(data: &[u8]) -> Result<(), Error> ...`.
5.  **Test: Should handle nested functions/classes correctly**
    *   *Input:* Python code with a class containing a method, containing a nested function.
    *   *Assertion:* The parser should extract all three symbols. The method should have `qualified_name` like `"MyClass.my_method"`, and the nested function should be tracked correctly.
6.  **Test: Should use fallback name extraction if field is missing**
    *   *Input:* A node where `child_by_field_name("name")` returns None, but it has an `identifier` child.
    *   *Assertion:* The parser successfully extracts the name using the fallback logic.
7.  **Test: Should NOT elide very short bodies (Size-Aware Elision)**
    *   *Input:* `def get_id(): return 1`
    *   *Assertion:* `is_elided` is `false` because the body is below the byte threshold.
8.  **Test: Should ignore empty files or files with no symbols**
    *   *Input:* An empty string or a file with only comments.
    *   *Assertion:* `ParsedDocument.symbols` is empty.