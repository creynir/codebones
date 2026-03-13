use std::collections::{HashMap, HashSet};
use std::ops::Range;
use tree_sitter::{Language, Node};

#[derive(Debug, Clone, Default)]
pub struct Bone {
    pub metadata: HashMap<String, String>,
}

pub struct Parser {}

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

pub fn get_python_spec() -> LanguageSpec {
    LanguageSpec {
        language: tree_sitter_python::LANGUAGE.into(),
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
}

pub fn get_rust_spec() -> LanguageSpec {
    LanguageSpec {
        language: tree_sitter_rust::LANGUAGE.into(),
        symbol_node_types: HashMap::from([
            ("function_item", SymbolKind::Function),
            ("struct_item", SymbolKind::Struct),
            ("impl_item", SymbolKind::Impl),
        ]),
        name_fields: HashMap::from([
            ("function_item", "name"),
            ("struct_item", "name"),
            ("impl_item", "type"),
        ]),
        container_node_types: HashSet::from(["impl_item"]),
        body_node_types: HashSet::from(["block", "declaration_list"]),
    }
}

pub fn get_java_spec() -> LanguageSpec {
    LanguageSpec {
        language: tree_sitter_java::LANGUAGE.into(),
        symbol_node_types: std::collections::HashMap::from([
            ("method_declaration", SymbolKind::Method),
            ("class_declaration", SymbolKind::Class),
            ("interface_declaration", SymbolKind::Interface),
            ("enum_declaration", SymbolKind::Class),
        ]),
        name_fields: std::collections::HashMap::from([
            ("method_declaration", "name"),
            ("class_declaration", "name"),
            ("interface_declaration", "name"),
            ("enum_declaration", "name"),
        ]),
        container_node_types: std::collections::HashSet::from([
            "class_declaration",
            "interface_declaration",
        ]),
        body_node_types: std::collections::HashSet::from([
            "block",
            "class_body",
            "interface_body",
            "enum_body",
        ]),
    }
}

pub fn get_c_spec() -> LanguageSpec {
    LanguageSpec {
        language: tree_sitter_c::LANGUAGE.into(),
        symbol_node_types: std::collections::HashMap::from([
            ("function_definition", SymbolKind::Function),
            ("struct_specifier", SymbolKind::Struct),
            ("class_specifier", SymbolKind::Class),
            ("namespace_definition", SymbolKind::Class),
        ]),
        name_fields: std::collections::HashMap::from([
            ("function_definition", "declarator"),
            ("struct_specifier", "name"),
            ("class_specifier", "name"),
            ("namespace_definition", "name"),
        ]),
        container_node_types: std::collections::HashSet::from([
            "class_specifier",
            "struct_specifier",
            "namespace_definition",
        ]),
        body_node_types: std::collections::HashSet::from([
            "compound_statement",
            "field_declaration_list",
        ]),
    }
}

pub fn get_cpp_spec() -> LanguageSpec {
    LanguageSpec {
        language: tree_sitter_cpp::LANGUAGE.into(),
        symbol_node_types: std::collections::HashMap::from([
            ("function_definition", SymbolKind::Function),
            ("struct_specifier", SymbolKind::Struct),
            ("class_specifier", SymbolKind::Class),
            ("namespace_definition", SymbolKind::Class),
        ]),
        name_fields: std::collections::HashMap::from([
            ("function_definition", "declarator"),
            ("struct_specifier", "name"),
            ("class_specifier", "name"),
            ("namespace_definition", "name"),
        ]),
        container_node_types: std::collections::HashSet::from([
            "class_specifier",
            "struct_specifier",
            "namespace_definition",
        ]),
        body_node_types: std::collections::HashSet::from([
            "compound_statement",
            "field_declaration_list",
        ]),
    }
}

pub fn get_csharp_spec() -> LanguageSpec {
    LanguageSpec {
        language: tree_sitter_c_sharp::LANGUAGE.into(),
        symbol_node_types: std::collections::HashMap::from([
            ("method_declaration", SymbolKind::Method),
            ("class_declaration", SymbolKind::Class),
            ("interface_declaration", SymbolKind::Interface),
            ("struct_declaration", SymbolKind::Struct),
            ("namespace_declaration", SymbolKind::Class),
        ]),
        name_fields: std::collections::HashMap::from([
            ("method_declaration", "name"),
            ("class_declaration", "name"),
            ("interface_declaration", "name"),
            ("struct_declaration", "name"),
            ("namespace_declaration", "name"),
        ]),
        container_node_types: std::collections::HashSet::from([
            "class_declaration",
            "interface_declaration",
            "namespace_declaration",
            "struct_declaration",
        ]),
        body_node_types: std::collections::HashSet::from(["block", "declaration_list"]),
    }
}

pub fn get_ruby_spec() -> LanguageSpec {
    LanguageSpec {
        language: tree_sitter_ruby::LANGUAGE.into(),
        symbol_node_types: std::collections::HashMap::from([
            ("method", SymbolKind::Method),
            ("singleton_method", SymbolKind::Method),
            ("class", SymbolKind::Class),
            ("module", SymbolKind::Class),
        ]),
        name_fields: std::collections::HashMap::from([
            ("method", "name"),
            ("singleton_method", "name"),
            ("class", "name"),
            ("module", "name"),
        ]),
        container_node_types: std::collections::HashSet::from(["class", "module"]),
        body_node_types: std::collections::HashSet::from(["body", "do_block", "begin_block"]),
    }
}

pub fn get_php_spec() -> LanguageSpec {
    LanguageSpec {
        language: tree_sitter_php::LANGUAGE_PHP.into(),
        symbol_node_types: std::collections::HashMap::from([
            ("function_definition", SymbolKind::Function),
            ("method_declaration", SymbolKind::Method),
            ("class_declaration", SymbolKind::Class),
            ("interface_declaration", SymbolKind::Interface),
            ("trait_declaration", SymbolKind::Class),
        ]),
        name_fields: std::collections::HashMap::from([
            ("function_definition", "name"),
            ("method_declaration", "name"),
            ("class_declaration", "name"),
            ("interface_declaration", "name"),
            ("trait_declaration", "name"),
        ]),
        container_node_types: std::collections::HashSet::from([
            "class_declaration",
            "interface_declaration",
            "trait_declaration",
        ]),
        body_node_types: std::collections::HashSet::from([
            "compound_statement",
            "declaration_list",
        ]),
    }
}

pub fn get_swift_spec() -> LanguageSpec {
    LanguageSpec {
        language: tree_sitter_swift::LANGUAGE.into(),
        symbol_node_types: std::collections::HashMap::from([
            ("function_declaration", SymbolKind::Function),
            ("class_declaration", SymbolKind::Class),
            ("struct_declaration", SymbolKind::Struct),
            ("protocol_declaration", SymbolKind::Interface),
            ("extension_declaration", SymbolKind::Impl),
        ]),
        name_fields: std::collections::HashMap::from([
            ("function_declaration", "name"),
            ("class_declaration", "name"),
            ("struct_declaration", "name"),
            ("protocol_declaration", "name"),
            ("extension_declaration", "type"),
        ]),
        container_node_types: std::collections::HashSet::from([
            "class_declaration",
            "struct_declaration",
            "protocol_declaration",
            "extension_declaration",
        ]),
        body_node_types: std::collections::HashSet::from([
            "class_body",
            "function_body",
            "code_block",
        ]),
    }
}

pub fn get_spec_for_extension(ext: &str) -> Option<LanguageSpec> {
    match ext {
        "rs" => Some(get_rust_spec()),
        "py" => Some(get_python_spec()),
        "go" => Some(get_go_spec()),
        "ts" | "tsx" | "js" | "jsx" => Some(get_typescript_spec()),
        "java" => Some(get_java_spec()),
        "c" | "h" => Some(get_c_spec()),
        "cpp" | "hpp" | "cc" | "cxx" => Some(get_cpp_spec()),
        "cs" => Some(get_csharp_spec()),
        "rb" => Some(get_ruby_spec()),
        "php" => Some(get_php_spec()),
        "swift" => Some(get_swift_spec()),
        _ => None,
    }
}

pub fn get_go_spec() -> LanguageSpec {
    LanguageSpec {
        language: tree_sitter_go::LANGUAGE.into(),
        symbol_node_types: std::collections::HashMap::from([
            ("function_declaration", SymbolKind::Function),
            ("method_declaration", SymbolKind::Method),
            ("type_declaration", SymbolKind::Struct),
        ]),
        name_fields: std::collections::HashMap::from([
            ("function_declaration", "name"),
            ("method_declaration", "name"),
            ("type_declaration", "name"),
        ]),
        container_node_types: std::collections::HashSet::from(["type_declaration"]),
        body_node_types: std::collections::HashSet::from(["block", "type_spec"]),
    }
}

pub fn get_typescript_spec() -> LanguageSpec {
    LanguageSpec {
        language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        symbol_node_types: std::collections::HashMap::from([
            ("function_declaration", SymbolKind::Function),
            ("method_definition", SymbolKind::Method),
            ("class_declaration", SymbolKind::Class),
            ("interface_declaration", SymbolKind::Interface),
        ]),
        name_fields: std::collections::HashMap::from([
            ("function_declaration", "name"),
            ("method_definition", "name"),
            ("class_declaration", "name"),
            ("interface_declaration", "name"),
        ]),
        container_node_types: std::collections::HashSet::from([
            "class_declaration",
            "interface_declaration",
        ]),
        body_node_types: std::collections::HashSet::from([
            "statement_block",
            "class_body",
            "object_type",
        ]),
    }
}

pub fn parse_file(source: &str, spec: &LanguageSpec) -> ParsedDocument {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&spec.language)
        .expect("Error loading language");
    let tree = parser.parse(source, None).expect("Error parsing source");
    let root_node = tree.root_node();

    let mut symbols = Vec::new();
    walk_tree(root_node, source.as_bytes(), spec, None, &mut symbols);

    ParsedDocument {
        file_path: String::new(),
        symbols,
    }
}

fn walk_tree(
    node: Node,
    source: &[u8],
    spec: &LanguageSpec,
    parent_symbol: Option<&Symbol>,
    symbols: &mut Vec<Symbol>,
) {
    let kind = node.kind();
    let mut current_symbol = None;

    if let Some(symbol_kind) = spec.symbol_node_types.get(kind) {
        let mut name = None;

        if let Some(name_field) = spec.name_fields.get(kind) {
            if let Some(mut child) = node.child_by_field_name(name_field) {
                while child.kind() == "function_declarator"
                    || child.kind() == "pointer_declarator"
                    || child.kind() == "reference_declarator"
                {
                    if let Some(inner) = child.child_by_field_name("declarator") {
                        child = inner;
                    } else {
                        break;
                    }
                }
                if let Ok(text) = std::str::from_utf8(&source[child.start_byte()..child.end_byte()])
                {
                    name = Some(text.to_string());
                }
            }
        }

        if name.is_none() {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                let child_kind = child.kind();
                if child_kind == "identifier" || child_kind == "type_identifier" {
                    if let Ok(text) =
                        std::str::from_utf8(&source[child.start_byte()..child.end_byte()])
                    {
                        name = Some(text.to_string());
                        break;
                    }
                }
            }
        }

        if let Some(name) = name {
            let qualified_name = if let Some(parent) = parent_symbol {
                format!("{}.{}", parent.qualified_name, name)
            } else {
                name.clone()
            };

            let mut body_range = None;
            let mut body_node_opt = node.child_by_field_name("body");

            if body_node_opt.is_none() {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if spec.body_node_types.contains(child.kind()) {
                        body_node_opt = Some(child);
                        break;
                    }
                }
            }

            if let Some(body_node) = body_node_opt {
                let mut start = body_node.start_byte();
                if let Some(prev) = body_node.prev_sibling() {
                    if prev.kind() == ":" {
                        start = prev.end_byte();
                    } else {
                        let mut has_newline = false;
                        for i in prev.end_byte()..start {
                            if i < source.len() && (source[i] == b'\n' || source[i] == b'\r') {
                                has_newline = true;
                                break;
                            }
                        }
                        if has_newline {
                            start = prev.end_byte();
                        }
                    }
                }
                body_range = Some(start..body_node.end_byte());
            }

            let symbol = Symbol {
                name,
                qualified_name,
                kind: symbol_kind.clone(),
                full_range: node.start_byte()..node.end_byte(),
                body_range,
            };

            symbols.push(symbol.clone());
            current_symbol = Some(symbol);
        }
    }

    let next_parent = current_symbol.as_ref().or(parent_symbol);

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree(child, source, spec, next_parent, symbols);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn elide_document(source: &str, doc: &ParsedDocument) -> String {
        let mut result = String::new();
        let mut last_end = 0;

        let mut sorted_symbols = doc.symbols.clone();
        sorted_symbols.sort_by_key(|s| s.full_range.start);

        for sym in sorted_symbols {
            if let Some(body_range) = &sym.body_range {
                if body_range.start >= last_end {
                    result.push_str(&source[last_end..body_range.start]);
                    result.push_str("...");
                    last_end = body_range.end;
                }
            }
        }
        result.push_str(&source[last_end..]);
        result
    }

    #[test]
    fn test_extract_python_class_signature_and_elide_body() {
        let source = "class MyClass:\n    def __init__(self):\n        pass";
        let spec = get_python_spec();
        let doc = parse_file(source, &spec);

        assert_eq!(doc.symbols.len(), 2);

        let class_sym = doc.symbols.iter().find(|s| s.name == "MyClass").unwrap();
        assert_eq!(class_sym.kind, SymbolKind::Class);

        let elided = elide_document(source, &doc);
        assert_eq!(elided, "class MyClass:...");
    }

    #[test]
    fn test_extract_python_function_signature_and_elide_body() {
        let source = "def calculate_total(a: int, b: int) -> int:\n    return a + b";
        let spec = get_python_spec();
        let doc = parse_file(source, &spec);

        assert_eq!(doc.symbols.len(), 1);
        let sym = &doc.symbols[0];
        assert_eq!(sym.name, "calculate_total");

        let elided = elide_document(source, &doc);
        assert_eq!(elided, "def calculate_total(a: int, b: int) -> int:...");
    }

    #[test]
    fn test_extract_rust_struct_signature_and_elide_body() {
        let source = "pub struct User {\n    pub id: i32,\n    pub name: String,\n}";
        let spec = get_rust_spec();
        let doc = parse_file(source, &spec);

        assert_eq!(doc.symbols.len(), 1);
        let sym = &doc.symbols[0];
        assert_eq!(sym.name, "User");

        let elided = elide_document(source, &doc);
        assert_eq!(elided, "pub struct User ...");
    }

    #[test]
    fn test_extract_rust_function_signature_and_elide_body() {
        let source = "pub fn process_data(data: &[u8]) -> Result<(), Error> {\n    // do work\n    Ok(())\n}";
        let spec = get_rust_spec();
        let doc = parse_file(source, &spec);

        assert_eq!(doc.symbols.len(), 1);
        let sym = &doc.symbols[0];
        assert_eq!(sym.name, "process_data");

        let elided = elide_document(source, &doc);
        assert_eq!(
            elided,
            "pub fn process_data(data: &[u8]) -> Result<(), Error> ..."
        );
    }

    #[test]
    fn test_handle_nested_functions_classes() {
        let source =
            "class MyClass:\n    def my_method(self):\n        def nested():\n            pass";
        let spec = get_python_spec();
        let doc = parse_file(source, &spec);

        assert_eq!(doc.symbols.len(), 3);

        let method_sym = doc.symbols.iter().find(|s| s.name == "my_method").unwrap();
        assert_eq!(method_sym.qualified_name, "MyClass.my_method");

        let nested_sym = doc.symbols.iter().find(|s| s.name == "nested").unwrap();
        assert_eq!(nested_sym.qualified_name, "MyClass.my_method.nested");
    }

    #[test]
    fn test_use_fallback_name_extraction() {
        // A test that ensures fallback logic works when "name" field is missing
        // This is an implementation detail, but we can verify it parses correctly
        let source = "def calculate_total(a: int, b: int) -> int:\n    return a + b";
        let mut spec = get_python_spec();
        // Remove the name field mapping to force fallback
        spec.name_fields.remove("function_definition");

        let doc = parse_file(source, &spec);

        assert_eq!(doc.symbols.len(), 1);
        let sym = &doc.symbols[0];
        assert_eq!(sym.name, "calculate_total");
    }

    #[test]
    fn test_ignore_empty_files_or_no_symbols() {
        let source = "# just a comment\n\n";
        let spec = get_python_spec();
        let doc = parse_file(source, &spec);

        assert!(doc.symbols.is_empty());
    }

    #[test]
    fn test_extract_java_class_and_method_elide_body() {
        let source = "public class MyClass {\n    public void doWork() {\n        System.out.println(\"work\");\n    }\n}";
        let spec = get_java_spec();
        let doc = parse_file(source, &spec);

        assert_eq!(doc.symbols.len(), 2);

        let class_sym = doc.symbols.iter().find(|s| s.name == "MyClass").unwrap();
        assert_eq!(class_sym.kind, SymbolKind::Class);
        assert!(class_sym.body_range.is_some());

        let method_sym = doc.symbols.iter().find(|s| s.name == "doWork").unwrap();
        assert_eq!(method_sym.kind, SymbolKind::Method);
        assert_eq!(method_sym.qualified_name, "MyClass.doWork");
        assert!(method_sym.body_range.is_some());

        let elided = elide_document(source, &doc);
        assert!(elided.starts_with("public class MyClass ..."));
    }

    #[test]
    fn test_extract_c_cpp_function_elide_body() {
        let source = "int calculate(int a, int b) {\n    return a + b;\n}";
        let spec = get_c_spec();
        let doc = parse_file(source, &spec);

        assert_eq!(doc.symbols.len(), 1);

        let func_sym = doc.symbols.iter().find(|s| s.name == "calculate").unwrap();
        assert_eq!(func_sym.kind, SymbolKind::Function);
        assert!(func_sym.body_range.is_some());

        let elided = elide_document(source, &doc);
        assert!(elided.starts_with("int calculate(int a, int b) ..."));
    }

    #[test]
    fn test_extract_csharp_class_and_method_elide_body() {
        let source = "public class Server {\n    public async Task StartAsync() {\n        await Task.Delay(10);\n    }\n}";
        let spec = get_csharp_spec();
        let doc = parse_file(source, &spec);

        assert_eq!(doc.symbols.len(), 2);

        let class_sym = doc.symbols.iter().find(|s| s.name == "Server").unwrap();
        assert_eq!(class_sym.kind, SymbolKind::Class);

        let method_sym = doc.symbols.iter().find(|s| s.name == "StartAsync").unwrap();
        assert_eq!(method_sym.kind, SymbolKind::Method);
        assert_eq!(method_sym.qualified_name, "Server.StartAsync");

        let elided = elide_document(source, &doc);
        assert!(elided.starts_with("public class Server ..."));
    }

    #[test]
    fn test_extract_ruby_class_and_method_elide_body() {
        let source = "class User\n  def login(email)\n    puts 'login'\n  end\nend";
        let spec = get_ruby_spec();
        let doc = parse_file(source, &spec);

        assert_eq!(doc.symbols.len(), 2);

        let class_sym = doc.symbols.iter().find(|s| s.name == "User").unwrap();
        assert_eq!(class_sym.kind, SymbolKind::Class);

        let method_sym = doc.symbols.iter().find(|s| s.name == "login").unwrap();
        assert_eq!(method_sym.kind, SymbolKind::Method);
        assert_eq!(method_sym.qualified_name, "User.login");

        let elided = elide_document(source, &doc);
        assert!(elided.starts_with("class User..."));
    }

    #[test]
    fn test_extract_php_class_and_method_elide_body() {
        let source = "<?php\nclass Controller {\n    public function handle($req) {\n        return true;\n    }\n}";
        let spec = get_php_spec();
        let doc = parse_file(source, &spec);

        assert_eq!(doc.symbols.len(), 2);

        let class_sym = doc.symbols.iter().find(|s| s.name == "Controller").unwrap();
        assert_eq!(class_sym.kind, SymbolKind::Class);

        let method_sym = doc.symbols.iter().find(|s| s.name == "handle").unwrap();
        assert_eq!(method_sym.kind, SymbolKind::Method);
        assert_eq!(method_sym.qualified_name, "Controller.handle");

        let elided = elide_document(source, &doc);
        assert!(elided.contains("class Controller ..."));
    }

    #[test]
    fn test_extract_swift_class_and_function_elide_body() {
        let source =
            "class ViewModel {\n    func loadData(with id: String) {\n        print(id)\n    }\n}";
        let spec = get_swift_spec();
        let doc = parse_file(source, &spec);

        assert_eq!(doc.symbols.len(), 2);

        let class_sym = doc.symbols.iter().find(|s| s.name == "ViewModel").unwrap();
        assert_eq!(class_sym.kind, SymbolKind::Class);

        let method_sym = doc.symbols.iter().find(|s| s.name == "loadData").unwrap();
        assert_eq!(method_sym.kind, SymbolKind::Function);
        assert_eq!(method_sym.qualified_name, "ViewModel.loadData");

        let elided = elide_document(source, &doc);
        assert!(elided.starts_with("class ViewModel ..."));
    }
}
