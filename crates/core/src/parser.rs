use std::collections::{HashMap, HashSet};
use std::ops::Range;
use tree_sitter::{Language, Node};

#[derive(Debug, Clone, Default)]
pub struct Bone {
    pub metadata: HashMap<String, String>,
}

/// A zero-sized token used to thread parser access through [`crate::plugin::Packer`].
/// Instantiate with `Parser {}` to satisfy the `Packer::new` signature.
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
    pub imports: Vec<String>,
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
        "ts" | "js" => Some(get_typescript_spec()),
        "tsx" | "jsx" => Some(get_tsx_spec()),
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

/// Same node types as TypeScript, but the TSX grammar — required to parse JSX
/// syntax; the plain TypeScript grammar produces ERROR nodes on JSX elements.
pub fn get_tsx_spec() -> LanguageSpec {
    LanguageSpec {
        language: tree_sitter_typescript::LANGUAGE_TSX.into(),
        ..get_typescript_spec()
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

    let imports = extract_imports(source, root_node, &spec.language);

    ParsedDocument {
        file_path: String::new(),
        symbols,
        imports,
    }
}

fn node_text<'a>(node: Node, source: &'a [u8]) -> &'a str {
    std::str::from_utf8(&source[node.start_byte()..node.end_byte()]).unwrap_or("")
}

fn strip_quotes(s: &str) -> String {
    let s = s.trim();
    // len check guards a lone quote char, where start == end would underflow
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"'))
            || (s.starts_with('\'') && s.ends_with('\''))
            || (s.starts_with('`') && s.ends_with('`')))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

fn extract_imports(source: &str, root: Node, language: &Language) -> Vec<String> {
    let src = source.as_bytes();

    // Identify language by checking the language pointer value indirectly — we
    // compare against the known language objects.
    let is_typescript = *language == tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
        || *language == tree_sitter_typescript::LANGUAGE_TSX.into();
    let is_python = *language == tree_sitter_python::LANGUAGE.into();
    let is_rust = *language == tree_sitter_rust::LANGUAGE.into();
    let is_go = *language == tree_sitter_go::LANGUAGE.into();
    let is_java = *language == tree_sitter_java::LANGUAGE.into();
    let is_c = *language == tree_sitter_c::LANGUAGE.into();
    let is_cpp = *language == tree_sitter_cpp::LANGUAGE.into();
    let is_csharp = *language == tree_sitter_c_sharp::LANGUAGE.into();
    let is_ruby = *language == tree_sitter_ruby::LANGUAGE.into();
    let is_php = *language == tree_sitter_php::LANGUAGE_PHP.into();
    let is_swift = *language == tree_sitter_swift::LANGUAGE.into();

    let mut imports = Vec::new();

    let mut cursor = root.walk();
    // We do a recursive DFS over the entire tree
    walk_for_imports(
        root,
        src,
        &mut imports,
        is_typescript,
        is_python,
        is_rust,
        is_go,
        is_java,
        is_c,
        is_cpp,
        is_csharp,
        is_ruby,
        is_php,
        is_swift,
        &mut cursor,
    );

    imports
}

#[allow(clippy::too_many_arguments)]
fn walk_for_imports(
    node: Node,
    src: &[u8],
    imports: &mut Vec<String>,
    is_typescript: bool,
    is_python: bool,
    is_rust: bool,
    is_go: bool,
    is_java: bool,
    is_c: bool,
    is_cpp: bool,
    is_csharp: bool,
    is_ruby: bool,
    is_php: bool,
    is_swift: bool,
    _cursor: &mut tree_sitter::TreeCursor,
) {
    let kind = node.kind();

    if is_typescript {
        // import_statement / re-exports (`export * from`, `export { x } from`):
        // both carry the module in a `source` string child
        if kind == "import_statement" || kind == "export_statement" {
            if let Some(source_node) = node.child_by_field_name("source") {
                let text = node_text(source_node, src);
                imports.push(strip_quotes(text));
            }
        }
        // call_expression where function is `require` or dynamic `import`
        if kind == "call_expression" {
            if let Some(func_node) = node.child_by_field_name("function") {
                let func_text = node_text(func_node, src);
                if func_text == "require" || func_text == "import" {
                    if let Some(args_node) = node.child_by_field_name("arguments") {
                        let mut c = args_node.walk();
                        for arg in args_node.children(&mut c) {
                            let arg_kind = arg.kind();
                            if arg_kind == "string" {
                                let text = node_text(arg, src);
                                imports.push(strip_quotes(text));
                                break;
                            }
                        }
                    }
                }
            }
        }
    } else if is_python {
        if kind == "import_statement" {
            // get all `dotted_name` or `aliased_import` children
            // (`import a, b` has one child per module — capture every one)
            let mut c = node.walk();
            let mut found = false;
            for child in node.children(&mut c) {
                let ck = child.kind();
                if ck == "dotted_name" || ck == "aliased_import" {
                    let text = node_text(child, src);
                    // get just the module name (before " as ")
                    let module = text.split(" as ").next().unwrap_or(text).trim();
                    imports.push(module.to_string());
                    found = true;
                }
            }
            if !found {
                // fallback: get full text
                let text = node_text(node, src);
                let trimmed = text.trim_start_matches("import ").trim();
                imports.push(trimmed.to_string());
            }
        } else if kind == "import_from_statement" {
            // get module_name child
            if let Some(module_node) = node.child_by_field_name("module_name") {
                let text = node_text(module_node, src);
                imports.push(text.to_string());
            } else {
                // relative import: look for relative_import or dotted_name children
                let mut c = node.walk();
                for child in node.children(&mut c) {
                    let ck = child.kind();
                    if ck == "relative_import" || ck == "dotted_name" {
                        let text = node_text(child, src);
                        imports.push(text.to_string());
                        break;
                    }
                }
            }
        }
    } else if is_rust {
        if kind == "use_declaration" {
            if let Some(arg_node) = node.child_by_field_name("argument") {
                let text = node_text(arg_node, src);
                imports.push(text.to_string());
            } else {
                // fallback: get full text minus "use " and ";"
                let text = node_text(node, src);
                let trimmed = text.trim_start_matches("use ").trim_end_matches(';').trim();
                imports.push(trimmed.to_string());
            }
        } else if kind == "mod_item" {
            // Only unnamed mods (declarations without a body)
            let has_body = {
                let mut c = node.walk();
                let result = node
                    .children(&mut c)
                    .any(|child| child.kind() == "declaration_list");
                result
            };
            if !has_body {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let text = node_text(name_node, src);
                    imports.push(text.to_string());
                }
            }
        }
    } else if is_go {
        if kind == "import_declaration" {
            let mut c = node.walk();
            for child in node.children(&mut c) {
                if child.kind() == "import_spec" {
                    if let Some(path_node) = child.child_by_field_name("path") {
                        let text = node_text(path_node, src);
                        imports.push(strip_quotes(text));
                    }
                } else if child.kind() == "import_spec_list" {
                    let mut c2 = child.walk();
                    for spec in child.children(&mut c2) {
                        if spec.kind() == "import_spec" {
                            if let Some(path_node) = spec.child_by_field_name("path") {
                                let text = node_text(path_node, src);
                                imports.push(strip_quotes(text));
                            }
                        }
                    }
                }
            }
        }
    } else if is_java {
        if kind == "import_declaration" {
            // Get full text of scoped identifier, strip leading "import " and trailing ";"
            let text = node_text(node, src);
            let trimmed = text
                .trim_start_matches("import ")
                .trim_end_matches(';')
                .trim();
            imports.push(trimmed.to_string());
        }
    } else if is_c || is_cpp {
        if kind == "preproc_include" {
            // Only extract string_literal (quoted includes), skip system_lib_string (angle brackets)
            let mut c = node.walk();
            for child in node.children(&mut c) {
                let ck = child.kind();
                if ck == "string_literal" {
                    let text = node_text(child, src);
                    imports.push(strip_quotes(text));
                    break;
                }
                // system_lib_string = angle bracket includes — skip
            }
        }
    } else if is_csharp {
        if kind == "using_directive" {
            // get name child text
            if let Some(name_node) = node.child_by_field_name("name") {
                let text = node_text(name_node, src);
                imports.push(text.to_string());
            } else {
                // fallback: walk children for qualified_name or identifier
                let mut c = node.walk();
                for child in node.children(&mut c) {
                    let ck = child.kind();
                    if ck == "qualified_name"
                        || ck == "identifier"
                        || ck == "member_access_expression"
                    {
                        let text = node_text(child, src);
                        imports.push(text.to_string());
                        break;
                    }
                }
            }
        }
    } else if is_ruby {
        if kind == "call" {
            // Check method name
            let method_text = node
                .child_by_field_name("method")
                .map(|n| node_text(n, src))
                .unwrap_or("");
            if method_text == "require" || method_text == "require_relative" {
                if let Some(args_node) = node.child_by_field_name("arguments") {
                    let mut c = args_node.walk();
                    for arg in args_node.children(&mut c) {
                        let ak = arg.kind();
                        if ak == "string" {
                            let text = node_text(arg, src);
                            imports.push(strip_quotes(text));
                            break;
                        } else if ak == "argument_list" {
                            let mut c2 = arg.walk();
                            for inner in arg.children(&mut c2) {
                                if inner.kind() == "string" {
                                    let text = node_text(inner, src);
                                    imports.push(strip_quotes(text));
                                    break;
                                }
                            }
                            break;
                        }
                    }
                }
            }
        }
    } else if is_php {
        if kind == "namespace_use_declaration" || kind == "use_declaration" {
            // Get the namespace path
            let mut c = node.walk();
            for child in node.children(&mut c) {
                let ck = child.kind();
                if ck == "namespace_use_clause" || ck == "qualified_name" || ck == "name" {
                    let text = node_text(child, src);
                    imports.push(text.to_string());
                    break;
                }
            }
        } else if kind == "require_expression"
            || kind == "include_expression"
            || kind == "require_once_expression"
            || kind == "include_once_expression"
        {
            // get the string argument
            let mut c = node.walk();
            for child in node.children(&mut c) {
                let ck = child.kind();
                if ck == "string" || ck == "encapsed_string" {
                    let text = node_text(child, src);
                    imports.push(strip_quotes(text));
                    break;
                }
            }
        }
    } else if is_swift && kind == "import_declaration" {
        // Collect identifier children to form the module path
        let mut c = node.walk();
        let mut parts = Vec::new();
        for child in node.children(&mut c) {
            let ck = child.kind();
            if ck == "identifier" || ck == "simple_identifier" {
                parts.push(node_text(child, src).to_string());
            }
        }
        if !parts.is_empty() {
            imports.push(parts.join("."));
        }
    }

    // Recurse into children
    let mut c = node.walk();
    for child in node.children(&mut c) {
        walk_for_imports(
            child,
            src,
            imports,
            is_typescript,
            is_python,
            is_rust,
            is_go,
            is_java,
            is_c,
            is_cpp,
            is_csharp,
            is_ruby,
            is_php,
            is_swift,
            &mut child.walk(),
        );
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

    // ===========================================================================
    // Import extraction — failing tests (ParsedDocument.imports not yet implemented)
    // ===========================================================================

    #[test]
    fn test_extract_typescript_imports() {
        let source = r#"import { useState } from 'react';
import type { FC } from 'react';
import './styles.css';
const fs = require('fs');

export const App: FC = () => null;
"#;
        let spec = get_typescript_spec();
        let doc = parse_file(source, &spec);

        assert!(
            doc.imports.contains(&"'react'".to_string())
                || doc.imports.iter().any(|i| i.contains("react")),
            "should extract 'react' import; got: {:?}",
            doc.imports
        );
        assert!(
            doc.imports.iter().any(|i| i.contains("styles.css")),
            "should extract side-effect import; got: {:?}",
            doc.imports
        );
        assert!(
            doc.imports.iter().any(|i| i.contains("fs")),
            "should extract require('fs'); got: {:?}",
            doc.imports
        );
    }

    #[test]
    fn test_tsx_file_with_jsx_parses_symbols_and_imports() {
        // The plain TypeScript grammar produces ERROR nodes on JSX; the tsx spec must not.
        let source = r#"import { EngineApiClient } from "../api/engine-api-client";

export function AppShell() {
    return <div className="shell"><span>hello</span></div>;
}

export class ShellState {
    reset() {}
}
"#;
        let spec = get_tsx_spec();
        let doc = parse_file(source, &spec);

        assert!(
            doc.imports
                .iter()
                .any(|i| i.contains("../api/engine-api-client")),
            "should extract import from JSX file; got: {:?}",
            doc.imports
        );
        assert!(
            doc.symbols.iter().any(|s| s.name == "AppShell"),
            "should extract function containing JSX; got: {:?}",
            doc.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        assert!(
            doc.symbols.iter().any(|s| s.name == "ShellState"),
            "should extract class declared after JSX; got: {:?}",
            doc.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_extract_typescript_reexports_and_dynamic_imports() {
        let source = r#"export * from './barrel-a';
export { thing } from './barrel-b';

async function load() {
    const mod = await import('./lazy');
    return mod;
}
"#;
        let spec = get_typescript_spec();
        let doc = parse_file(source, &spec);

        for expected in ["./barrel-a", "./barrel-b", "./lazy"] {
            assert!(
                doc.imports.iter().any(|i| i.contains(expected)),
                "should extract {}; got: {:?}",
                expected,
                doc.imports
            );
        }
    }

    #[test]
    fn test_extract_python_multi_module_import_line() {
        let source = "import os, sys, json\n";
        let spec = get_python_spec();
        let doc = parse_file(source, &spec);

        for expected in ["os", "sys", "json"] {
            assert!(
                doc.imports.iter().any(|i| i == expected),
                "import os, sys, json should yield '{}'; got: {:?}",
                expected,
                doc.imports
            );
        }
    }

    #[test]
    fn test_strip_quotes_single_quote_char_does_not_panic() {
        assert_eq!(strip_quotes("\""), "\"");
        assert_eq!(strip_quotes("'"), "'");
        assert_eq!(strip_quotes(""), "");
        assert_eq!(strip_quotes("\"x\""), "x");
    }

    #[test]
    fn test_extract_python_imports() {
        let source = r#"import os
import sys
from pathlib import Path
from .utils import helper
from collections import OrderedDict

def main():
    pass
"#;
        let spec = get_python_spec();
        let doc = parse_file(source, &spec);

        assert!(
            doc.imports.iter().any(|i| i.contains("os")),
            "should extract 'import os'; got: {:?}",
            doc.imports
        );
        assert!(
            doc.imports.iter().any(|i| i.contains("pathlib")),
            "should extract 'from pathlib import Path'; got: {:?}",
            doc.imports
        );
        assert!(
            doc.imports.iter().any(|i| i.contains(".utils")),
            "should extract relative import '.utils'; got: {:?}",
            doc.imports
        );
    }

    #[test]
    fn test_extract_rust_imports() {
        let source = r#"use std::collections::HashMap;
use crate::parser::parse_file;
mod utils;

pub fn run() {}
"#;
        let spec = get_rust_spec();
        let doc = parse_file(source, &spec);

        assert!(
            doc.imports.iter().any(|i| i.contains("std::collections")),
            "should extract use std::collections::HashMap; got: {:?}",
            doc.imports
        );
        assert!(
            doc.imports.iter().any(|i| i.contains("crate::parser")),
            "should extract use crate::parser::parse_file; got: {:?}",
            doc.imports
        );
        assert!(
            doc.imports.iter().any(|i| i.contains("utils")),
            "should extract mod utils; got: {:?}",
            doc.imports
        );
    }

    #[test]
    fn test_extract_go_imports() {
        let source = r#"package main

import (
    "fmt"
    "net/http"
)

func main() {}
"#;
        let spec = get_go_spec();
        let doc = parse_file(source, &spec);

        assert!(
            doc.imports.iter().any(|i| i.contains("fmt")),
            "should extract import \"fmt\"; got: {:?}",
            doc.imports
        );
        assert!(
            doc.imports.iter().any(|i| i.contains("net/http")),
            "should extract import \"net/http\"; got: {:?}",
            doc.imports
        );
    }

    #[test]
    fn test_extract_java_imports() {
        let source = r#"import java.util.List;
import java.util.ArrayList;
import com.example.service.UserService;

public class Main {}
"#;
        let spec = get_java_spec();
        let doc = parse_file(source, &spec);

        assert!(
            doc.imports.iter().any(|i| i.contains("java.util.List")),
            "should extract import java.util.List; got: {:?}",
            doc.imports
        );
        assert!(
            doc.imports
                .iter()
                .any(|i| i.contains("com.example.service.UserService")),
            "should extract import com.example.service.UserService; got: {:?}",
            doc.imports
        );
    }

    #[test]
    fn test_extract_c_local_includes_only() {
        let source = r#"#include "myheader.h"
#include "utils/helper.h"
#include <stdio.h>
#include <stdlib.h>

int main() { return 0; }
"#;
        let spec = get_c_spec();
        let doc = parse_file(source, &spec);

        assert!(
            doc.imports.iter().any(|i| i.contains("myheader.h")),
            "should extract #include \"myheader.h\"; got: {:?}",
            doc.imports
        );
        assert!(
            doc.imports.iter().any(|i| i.contains("utils/helper.h")),
            "should extract #include \"utils/helper.h\"; got: {:?}",
            doc.imports
        );
        assert!(
            !doc.imports.iter().any(|i| i.contains("stdio.h")),
            "should NOT extract system include <stdio.h>; got: {:?}",
            doc.imports
        );
    }

    #[test]
    fn test_extract_csharp_using_directives() {
        let source = r#"using System;
using System.Collections.Generic;
using MyApp.Services;

public class Program {}
"#;
        let spec = get_csharp_spec();
        let doc = parse_file(source, &spec);

        assert!(
            doc.imports.iter().any(|i| i.contains("System")),
            "should extract using System; got: {:?}",
            doc.imports
        );
        assert!(
            doc.imports.iter().any(|i| i.contains("MyApp.Services")),
            "should extract using MyApp.Services; got: {:?}",
            doc.imports
        );
    }

    #[test]
    fn test_extract_ruby_require_statements() {
        let source = r#"require 'json'
require_relative './utils/helper'
require 'net/http'

class MyClass
  def run; end
end
"#;
        let spec = get_ruby_spec();
        let doc = parse_file(source, &spec);

        assert!(
            doc.imports.iter().any(|i| i.contains("json")),
            "should extract require 'json'; got: {:?}",
            doc.imports
        );
        assert!(
            doc.imports.iter().any(|i| i.contains("./utils/helper")),
            "should extract require_relative './utils/helper'; got: {:?}",
            doc.imports
        );
    }

    #[test]
    fn test_extract_php_use_and_require() {
        let source = r#"<?php
use App\Http\Controllers\UserController;
use Illuminate\Support\Facades\DB;
require 'config.php';
include 'helpers.php';

class Handler {}
"#;
        let spec = get_php_spec();
        let doc = parse_file(source, &spec);

        assert!(
            doc.imports
                .iter()
                .any(|i| i.contains("App\\Http\\Controllers\\UserController")
                    || i.contains("App/Http/Controllers/UserController")
                    || i.contains("UserController")),
            "should extract use App\\Http\\Controllers\\UserController; got: {:?}",
            doc.imports
        );
        assert!(
            doc.imports.iter().any(|i| i.contains("config.php")),
            "should extract require 'config.php'; got: {:?}",
            doc.imports
        );
    }

    #[test]
    fn test_extract_swift_import_declarations() {
        let source = r#"import Foundation
import UIKit
import SwiftUI

struct ContentView: View {
    var body: some View { Text("Hello") }
}
"#;
        let spec = get_swift_spec();
        let doc = parse_file(source, &spec);

        assert!(
            doc.imports.iter().any(|i| i.contains("Foundation")),
            "should extract import Foundation; got: {:?}",
            doc.imports
        );
        assert!(
            doc.imports.iter().any(|i| i.contains("UIKit")),
            "should extract import UIKit; got: {:?}",
            doc.imports
        );
        assert!(
            doc.imports.iter().any(|i| i.contains("SwiftUI")),
            "should extract import SwiftUI; got: {:?}",
            doc.imports
        );
    }

    #[test]
    fn test_extract_cpp_local_includes_only() {
        let source = r#"#include "engine/renderer.h"
#include "core/math.hpp"
#include <vector>
#include <string>

void render() {}
"#;
        let spec = get_cpp_spec();
        let doc = parse_file(source, &spec);

        assert!(
            doc.imports.iter().any(|i| i.contains("engine/renderer.h")),
            "should extract #include \"engine/renderer.h\"; got: {:?}",
            doc.imports
        );
        assert!(
            !doc.imports.iter().any(|i| i.contains("vector")),
            "should NOT extract system include <vector>; got: {:?}",
            doc.imports
        );
    }
}
