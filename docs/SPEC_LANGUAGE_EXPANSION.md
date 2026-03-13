# Language Expansion Specification

This document outlines the architectural specification for expanding `codebones` to support Java, C, C++, C#, Ruby, PHP, and Swift. This specification provides the necessary `tree-sitter` configurations, crate dependencies, and TDD requirements to ensure accurate symbol extraction and body elision.

## 1. Crate Dependencies (`crates/core/Cargo.toml`)

Add the following dependencies to `Cargo.toml`. 

```toml
tree-sitter-java = "0.20"
tree-sitter-c = "0.20"
tree-sitter-cpp = "0.20"
tree-sitter-c-sharp = "0.20"
tree-sitter-ruby = "0.20"
tree-sitter-php = "0.20"
tree-sitter-swift = "0.3" # Note: check latest compatible community version if official is lagging
```

### Known Quirks & Considerations:
- **C/C++**: C++ is a superset of C in terms of tree-sitter grammars. While `tree-sitter-c` works for pure C, `tree-sitter-cpp` can parse most C code as well. We should register them separately based on the `.c` / `.cpp` / `.h` / `.hpp` file extensions.
- **PHP**: The `tree-sitter-php` crate typically bundles both HTML and PHP. You **must** specifically initialize it using `tree_sitter_php::language_php()` rather than a generic language function, to avoid parsing the file as plain HTML.
- **Swift**: The official `tree-sitter-swift` crate can sometimes be out of sync on `crates.io`. Ensure the version matches the core `tree-sitter` version used in `codebones`.
- **Ruby**: Ruby's AST often does not wrap method or class bodies in a single "block" node (unlike languages with curly braces). The `body` might just be a sequence of children. The parser might need slight adjustments if `body_node_opt` fails to find a single bounding node.

---

## 2. Parser Configurations (`crates/core/src/parser.rs`)

For each language, create a new `get_<language>_spec()` function returning a `LanguageSpec`.

### Java (`get_java_spec`)
- **`symbol_node_types`**: 
  - `"method_declaration"` -> `SymbolKind::Method`
  - `"class_declaration"` -> `SymbolKind::Class`
  - `"interface_declaration"` -> `SymbolKind::Interface`
  - `"enum_declaration"` -> `SymbolKind::Class` (or `Enum` if added to `SymbolKind`)
- **`name_fields`**: `"name"` for all the above.
- **`container_node_types`**: `"class_declaration"`, `"interface_declaration"`
- **`body_node_types`**: `"block"`, `"class_body"`, `"interface_body"`, `"enum_body"`

### C and C++ (`get_c_spec` / `get_cpp_spec`)
- **`symbol_node_types`**: 
  - `"function_definition"` -> `SymbolKind::Function`
  - `"struct_specifier"` -> `SymbolKind::Struct`
  - `"class_specifier"` -> `SymbolKind::Class`
  - `"namespace_definition"` -> `SymbolKind::Class` (or `Namespace`)
- **`name_fields`**: `"declarator"` (The existing fallback in `parser.rs` will drill down into `function_declarator` -> `identifier`).
- **`container_node_types`**: `"class_specifier"`, `"struct_specifier"`, `"namespace_definition"`
- **`body_node_types`**: `"compound_statement"`, `"field_declaration_list"`

### C# (`get_csharp_spec`)
- **`symbol_node_types`**: 
  - `"method_declaration"` -> `SymbolKind::Method`
  - `"class_declaration"` -> `SymbolKind::Class`
  - `"interface_declaration"` -> `SymbolKind::Interface`
  - `"struct_declaration"` -> `SymbolKind::Struct`
  - `"namespace_declaration"` -> `SymbolKind::Class` (or `Namespace`)
- **`name_fields`**: `"name"`
- **`container_node_types`**: `"class_declaration"`, `"interface_declaration"`, `"namespace_declaration"`, `"struct_declaration"`
- **`body_node_types`**: `"block"`, `"declaration_list"`

### Ruby (`get_ruby_spec`)
- **`symbol_node_types`**: 
  - `"method"` -> `SymbolKind::Method`
  - `"singleton_method"` -> `SymbolKind::Method`
  - `"class"` -> `SymbolKind::Class`
  - `"module"` -> `SymbolKind::Class` (or `Module`)
- **`name_fields`**: `"name"`
- **`container_node_types`**: `"class"`, `"module"`
- **`body_node_types`**: `"body"`, `"do_block"`, `"begin_block"` 

### PHP (`get_php_spec`)
- **`symbol_node_types`**: 
  - `"function_definition"` -> `SymbolKind::Function`
  - `"method_declaration"` -> `SymbolKind::Method`
  - `"class_declaration"` -> `SymbolKind::Class`
  - `"interface_declaration"` -> `SymbolKind::Interface`
  - `"trait_declaration"` -> `SymbolKind::Class` (or `Trait`)
- **`name_fields`**: `"name"`
- **`container_node_types`**: `"class_declaration"`, `"interface_declaration"`, `"trait_declaration"`
- **`body_node_types`**: `"compound_statement"`, `"declaration_list"`

### Swift (`get_swift_spec`)
- **`symbol_node_types`**: 
  - `"function_declaration"` -> `SymbolKind::Function`
  - `"class_declaration"` -> `SymbolKind::Class`
  - `"struct_declaration"` -> `SymbolKind::Struct`
  - `"protocol_declaration"` -> `SymbolKind::Interface`
  - `"extension_declaration"` -> `SymbolKind::Impl`
- **`name_fields`**: `"name"` (Note: `extension_declaration` usually uses `"type"` field, similar to Rust's `impl_item`).
- **`container_node_types`**: `"class_declaration"`, `"struct_declaration"`, `"protocol_declaration"`, `"extension_declaration"`
- **`body_node_types`**: `"class_body"`, `"function_body"`, `"code_block"`

---

## 3. Strict TDD Unit Tests

The test-writing agent must implement the following tests in `crates/core/src/parser.rs` before or alongside the implementation. Each test must assert the correct symbol extraction (name, kind, qualified name) AND exact structural elision output (`...`).

### Java Tests
1. `test_extract_java_class_and_elide_body`: Verify class signatures including `implements` and `extends` are preserved, and the `class_body` is elided.
2. `test_extract_java_method_with_annotations`: Ensure annotations (like `@Override`) are kept as part of the method signature and the `block` is elided.

### C/C++ Tests
1. `test_extract_cpp_class_with_access_specifiers`: Ensure `public:` / `private:` modifiers and inheritance are preserved, and `field_declaration_list` is elided.
2. `test_extract_c_function_pointers_and_structs`: Ensure nested `function_declarator` correctly resolves the function name.
3. `test_extract_cpp_namespace_nesting`: Ensure symbols inside namespaces get correctly qualified names (e.g., `MyNamespace.MyClass.my_method`).

### C# Tests
1. `test_extract_csharp_async_task_method`: Ensure `public async Task<T>` signatures are fully preserved before the `block` elision.
2. `test_extract_csharp_namespace_and_interface`: Test that interfaces within namespaces are extracted and their `declaration_list` bodies are elided.

### Ruby Tests
1. `test_extract_ruby_class_and_module`: Ensure module and class definitions are identified and their bodies elided.
2. **[CRITICAL]** `test_extract_ruby_method_implicit_body`: Ruby methods often do not have a single bounding block node. This test must verify that `def foo(args) ... end` is correctly elided regardless of internal statement structure.

### PHP Tests
1. `test_extract_php_class_with_traits`: Verify `class_declaration` retains its `implements` / `extends` signature, and the `declaration_list` is elided.
2. `test_extract_php_function_with_types`: Test `function foo(int $a): string` signature preservation and `compound_statement` elision.

### Swift Tests
1. `test_extract_swift_protocol_and_extension`: Ensure `protocol_declaration` and `extension_declaration` (using the `type` name field) are extracted.
2. `test_extract_swift_func_with_labels`: Ensure function signatures with argument labels (e.g., `func build(with name: String)`) are preserved correctly, and the `code_block` is elided.
