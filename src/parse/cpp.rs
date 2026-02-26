//! C++ parsing using tree-sitter.
//!
//! Extracts functions, classes, structs, namespaces, enums, templates, includes.

use std::path::Path;

use crate::types::{AcbResult, CodeUnitType, Language, Visibility};

use super::treesitter::{get_node_text, node_to_span};
use super::{LanguageParser, RawCodeUnit};

/// C++ language parser.
pub struct CppParser;

impl Default for CppParser {
    fn default() -> Self {
        Self::new()
    }
}

impl CppParser {
    /// Create a new C++ parser.
    pub fn new() -> Self {
        Self
    }

    fn extract_from_node(
        &self,
        node: tree_sitter::Node,
        source: &str,
        file_path: &Path,
        units: &mut Vec<RawCodeUnit>,
        next_id: &mut u64,
        parent_qname: &str,
    ) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "function_definition" => {
                    if let Some(unit) =
                        self.extract_function(child, source, file_path, parent_qname, next_id)
                    {
                        units.push(unit);
                    }
                }
                "declaration" => {
                    // A declaration can contain a function declarator (forward decl / prototype)
                    // or a variable. We extract function prototypes.
                    if let Some(unit) =
                        self.extract_declaration(child, source, file_path, parent_qname, next_id)
                    {
                        units.push(unit);
                    }
                }
                "class_specifier" => {
                    self.extract_class_or_struct(
                        child,
                        source,
                        file_path,
                        units,
                        next_id,
                        parent_qname,
                    );
                }
                "struct_specifier" => {
                    self.extract_class_or_struct(
                        child,
                        source,
                        file_path,
                        units,
                        next_id,
                        parent_qname,
                    );
                }
                "namespace_definition" => {
                    self.extract_namespace(child, source, file_path, units, next_id, parent_qname);
                }
                "enum_specifier" => {
                    if let Some(unit) =
                        self.extract_enum(child, source, file_path, parent_qname, next_id)
                    {
                        units.push(unit);
                    }
                }
                "template_declaration" => {
                    // Recurse into the template body to find the actual declaration
                    self.extract_from_node(child, source, file_path, units, next_id, parent_qname);
                }
                "preproc_include" => {
                    if let Some(unit) =
                        self.extract_include(child, source, file_path, parent_qname, next_id)
                    {
                        units.push(unit);
                    }
                }
                // Top-level declarations wrapped in linkage_specification (extern "C" { ... })
                "linkage_specification" => {
                    self.extract_from_node(child, source, file_path, units, next_id, parent_qname);
                }
                _ => {}
            }
        }
    }

    fn extract_function(
        &self,
        node: tree_sitter::Node,
        source: &str,
        file_path: &Path,
        parent_qname: &str,
        next_id: &mut u64,
    ) -> Option<RawCodeUnit> {
        let name = self.function_name(node, source)?;
        let qname = cpp_qname(parent_qname, &name);
        let span = node_to_span(node);

        let id = *next_id;
        *next_id += 1;

        let is_test = name.starts_with("TEST") || name.starts_with("test_");
        let unit_type = if is_test {
            CodeUnitType::Test
        } else {
            CodeUnitType::Function
        };

        let mut unit = RawCodeUnit::new(
            unit_type,
            Language::Cpp,
            name,
            file_path.to_path_buf(),
            span,
        );
        unit.temp_id = id;
        unit.qualified_name = qname;
        unit.visibility = Visibility::Public;

        Some(unit)
    }

    /// Extract a function prototype from a top-level declaration node.
    fn extract_declaration(
        &self,
        node: tree_sitter::Node,
        source: &str,
        file_path: &Path,
        parent_qname: &str,
        next_id: &mut u64,
    ) -> Option<RawCodeUnit> {
        // Only extract if the declaration contains a function_declarator
        let declarator = find_descendant_by_kind(node, "function_declarator")?;
        let name_node = declarator.child_by_field_name("declarator")?;
        let name = get_node_text(name_node, source).to_string();
        let qname = cpp_qname(parent_qname, &name);
        let span = node_to_span(node);

        let id = *next_id;
        *next_id += 1;

        let mut unit = RawCodeUnit::new(
            CodeUnitType::Function,
            Language::Cpp,
            name,
            file_path.to_path_buf(),
            span,
        );
        unit.temp_id = id;
        unit.qualified_name = qname;
        unit.visibility = Visibility::Public;

        Some(unit)
    }

    fn extract_class_or_struct(
        &self,
        node: tree_sitter::Node,
        source: &str,
        file_path: &Path,
        units: &mut Vec<RawCodeUnit>,
        next_id: &mut u64,
        parent_qname: &str,
    ) {
        let unit_type = CodeUnitType::Type;
        let name = match node.child_by_field_name("name") {
            Some(n) => get_node_text(n, source).to_string(),
            None => return, // anonymous struct/class — skip
        };
        let qname = cpp_qname(parent_qname, &name);
        let span = node_to_span(node);

        let id = *next_id;
        *next_id += 1;

        let mut unit = RawCodeUnit::new(
            unit_type,
            Language::Cpp,
            name.clone(),
            file_path.to_path_buf(),
            span,
        );
        unit.temp_id = id;
        unit.qualified_name = qname.clone();
        unit.visibility = Visibility::Public;
        units.push(unit);

        // Recurse into the body to find methods
        if let Some(body) = node.child_by_field_name("body") {
            self.extract_class_members(body, source, file_path, units, next_id, &qname);
        }
    }

    fn extract_class_members(
        &self,
        body: tree_sitter::Node,
        source: &str,
        file_path: &Path,
        units: &mut Vec<RawCodeUnit>,
        next_id: &mut u64,
        parent_qname: &str,
    ) {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            match child.kind() {
                "function_definition" => {
                    if let Some(unit) =
                        self.extract_function(child, source, file_path, parent_qname, next_id)
                    {
                        units.push(unit);
                    }
                }
                "declaration" | "field_declaration" => {
                    // Check if it's a method declaration inside a class
                    if let Some(unit) =
                        self.extract_declaration(child, source, file_path, parent_qname, next_id)
                    {
                        units.push(unit);
                    }
                }
                "template_declaration" => {
                    self.extract_class_members(
                        child,
                        source,
                        file_path,
                        units,
                        next_id,
                        parent_qname,
                    );
                }
                // Nested classes/structs
                "class_specifier" => {
                    self.extract_class_or_struct(
                        child,
                        source,
                        file_path,
                        units,
                        next_id,
                        parent_qname,
                    );
                }
                "struct_specifier" => {
                    self.extract_class_or_struct(
                        child,
                        source,
                        file_path,
                        units,
                        next_id,
                        parent_qname,
                    );
                }
                _ => {}
            }
        }
    }

    fn extract_namespace(
        &self,
        node: tree_sitter::Node,
        source: &str,
        file_path: &Path,
        units: &mut Vec<RawCodeUnit>,
        next_id: &mut u64,
        parent_qname: &str,
    ) {
        let name = node
            .child_by_field_name("name")
            .map(|n| get_node_text(n, source).to_string())
            .unwrap_or_else(|| "(anonymous)".to_string());
        let qname = cpp_qname(parent_qname, &name);
        let span = node_to_span(node);

        let id = *next_id;
        *next_id += 1;

        let mut unit = RawCodeUnit::new(
            CodeUnitType::Module,
            Language::Cpp,
            name,
            file_path.to_path_buf(),
            span,
        );
        unit.temp_id = id;
        unit.qualified_name = qname.clone();
        unit.visibility = Visibility::Public;
        units.push(unit);

        // Recurse into namespace body
        if let Some(body) = node.child_by_field_name("body") {
            self.extract_from_node(body, source, file_path, units, next_id, &qname);
        }
    }

    fn extract_enum(
        &self,
        node: tree_sitter::Node,
        source: &str,
        file_path: &Path,
        parent_qname: &str,
        next_id: &mut u64,
    ) -> Option<RawCodeUnit> {
        let name_node = node.child_by_field_name("name")?;
        let name = get_node_text(name_node, source).to_string();
        let qname = cpp_qname(parent_qname, &name);
        let span = node_to_span(node);

        let id = *next_id;
        *next_id += 1;

        let mut unit = RawCodeUnit::new(
            CodeUnitType::Type,
            Language::Cpp,
            name,
            file_path.to_path_buf(),
            span,
        );
        unit.temp_id = id;
        unit.qualified_name = qname;
        unit.visibility = Visibility::Public;

        Some(unit)
    }

    fn extract_include(
        &self,
        node: tree_sitter::Node,
        source: &str,
        file_path: &Path,
        parent_qname: &str,
        next_id: &mut u64,
    ) -> Option<RawCodeUnit> {
        let path_node = node.child_by_field_name("path")?;
        let include_path = get_node_text(path_node, source).to_string();
        let span = node_to_span(node);

        let id = *next_id;
        *next_id += 1;

        let mut unit = RawCodeUnit::new(
            CodeUnitType::Import,
            Language::Cpp,
            include_path,
            file_path.to_path_buf(),
            span,
        );
        unit.temp_id = id;
        unit.qualified_name = cpp_qname(parent_qname, "include");

        Some(unit)
    }

    /// Extract the name from a function_definition node.
    /// Handles plain functions, qualified names (Foo::bar), and destructors (~Foo).
    fn function_name(&self, node: tree_sitter::Node, source: &str) -> Option<String> {
        let declarator = node.child_by_field_name("declarator")?;
        self.declarator_name(declarator, source)
    }

    /// Extract the name from a declarator node.
    fn declarator_name(&self, node: tree_sitter::Node, source: &str) -> Option<String> {
        declarator_name_inner(node, source)
    }
}

impl LanguageParser for CppParser {
    fn extract_units(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        file_path: &Path,
    ) -> AcbResult<Vec<RawCodeUnit>> {
        let mut units = Vec::new();
        let mut next_id = 0u64;

        let module_name = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let root_span = node_to_span(tree.root_node());
        let mut module_unit = RawCodeUnit::new(
            CodeUnitType::Module,
            Language::Cpp,
            module_name.clone(),
            file_path.to_path_buf(),
            root_span,
        );
        module_unit.temp_id = next_id;
        module_unit.qualified_name = module_name.clone();
        next_id += 1;
        units.push(module_unit);

        self.extract_from_node(
            tree.root_node(),
            source,
            file_path,
            &mut units,
            &mut next_id,
            &module_name,
        );

        Ok(units)
    }

    fn is_test_file(&self, path: &Path, _source: &str) -> bool {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        name.ends_with("_test.cpp")
            || name.ends_with("_test.cc")
            || name.starts_with("test_")
            || name.ends_with("_unittest.cpp")
            || name.ends_with("_unittest.cc")
    }
}

/// Recursively drill into declarator nodes to find the identifier.
fn declarator_name_inner(node: tree_sitter::Node, source: &str) -> Option<String> {
    match node.kind() {
        "function_declarator" => {
            let inner = node.child_by_field_name("declarator")?;
            declarator_name_inner(inner, source)
        }
        "qualified_identifier" | "scoped_identifier" => {
            // e.g. Foo::bar — return full qualified text
            Some(get_node_text(node, source).to_string())
        }
        "destructor_name" => Some(get_node_text(node, source).to_string()),
        "identifier" | "field_identifier" | "operator_name" | "template_function" => {
            Some(get_node_text(node, source).to_string())
        }
        "pointer_declarator" | "reference_declarator" => {
            // *foo or &foo — drill into child
            let inner = node.child_by_field_name("declarator")?;
            declarator_name_inner(inner, source)
        }
        _ => {
            // Fallback: try "declarator" field
            if let Some(inner) = node.child_by_field_name("declarator") {
                return declarator_name_inner(inner, source);
            }
            None
        }
    }
}

fn cpp_qname(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{}::{}", parent, name)
    }
}

/// Find the first descendant with a given kind (DFS).
fn find_descendant_by_kind<'a>(
    node: tree_sitter::Node<'a>,
    kind: &str,
) -> Option<tree_sitter::Node<'a>> {
    if node.kind() == kind {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_descendant_by_kind(child, kind) {
            return Some(found);
        }
    }
    None
}
