//! C# parsing using tree-sitter.
//!
//! Extracts classes, interfaces, structs, enums, methods, properties, namespaces, usings.

use std::path::Path;

use crate::types::{AcbResult, CodeUnitType, Language, Visibility};

use super::treesitter::{get_node_text, node_to_span};
use super::{LanguageParser, RawCodeUnit};

/// C# language parser.
pub struct CSharpParser;

impl Default for CSharpParser {
    fn default() -> Self {
        Self::new()
    }
}

impl CSharpParser {
    /// Create a new C# parser.
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
                "class_declaration" => {
                    self.extract_type_decl(
                        child,
                        source,
                        file_path,
                        units,
                        next_id,
                        parent_qname,
                        CodeUnitType::Type,
                    );
                }
                "struct_declaration" => {
                    self.extract_type_decl(
                        child,
                        source,
                        file_path,
                        units,
                        next_id,
                        parent_qname,
                        CodeUnitType::Type,
                    );
                }
                "interface_declaration" => {
                    self.extract_type_decl(
                        child,
                        source,
                        file_path,
                        units,
                        next_id,
                        parent_qname,
                        CodeUnitType::Trait,
                    );
                }
                "enum_declaration" => {
                    self.extract_type_decl(
                        child,
                        source,
                        file_path,
                        units,
                        next_id,
                        parent_qname,
                        CodeUnitType::Type,
                    );
                }
                "record_declaration" => {
                    self.extract_type_decl(
                        child,
                        source,
                        file_path,
                        units,
                        next_id,
                        parent_qname,
                        CodeUnitType::Type,
                    );
                }
                "namespace_declaration" | "file_scoped_namespace_declaration" => {
                    self.extract_namespace(child, source, file_path, units, next_id, parent_qname);
                }
                "method_declaration" | "constructor_declaration" => {
                    if let Some(unit) =
                        self.extract_method(child, source, file_path, parent_qname, next_id)
                    {
                        units.push(unit);
                    }
                }
                "property_declaration" => {
                    if let Some(unit) =
                        self.extract_property(child, source, file_path, parent_qname, next_id)
                    {
                        units.push(unit);
                    }
                }
                "using_directive" => {
                    if let Some(unit) =
                        self.extract_using(child, source, file_path, parent_qname, next_id)
                    {
                        units.push(unit);
                    }
                }
                // Recurse into declaration_list (body of namespace/class)
                "declaration_list" => {
                    self.extract_from_node(child, source, file_path, units, next_id, parent_qname);
                }
                _ => {}
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn extract_type_decl(
        &self,
        node: tree_sitter::Node,
        source: &str,
        file_path: &Path,
        units: &mut Vec<RawCodeUnit>,
        next_id: &mut u64,
        parent_qname: &str,
        unit_type: CodeUnitType,
    ) {
        let name = match node.child_by_field_name("name") {
            Some(n) => get_node_text(n, source).to_string(),
            None => return,
        };
        let qname = cs_qname(parent_qname, &name);
        let span = node_to_span(node);
        let vis = extract_cs_visibility(node, source);

        let id = *next_id;
        *next_id += 1;

        let mut unit = RawCodeUnit::new(
            unit_type,
            Language::CSharp,
            name,
            file_path.to_path_buf(),
            span,
        );
        unit.temp_id = id;
        unit.qualified_name = qname.clone();
        unit.visibility = vis;
        units.push(unit);

        // Recurse into body
        if let Some(body) = node.child_by_field_name("body") {
            self.extract_from_node(body, source, file_path, units, next_id, &qname);
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
        let name = match node.child_by_field_name("name") {
            Some(n) => get_node_text(n, source).to_string(),
            None => return,
        };
        let qname = cs_qname(parent_qname, &name);
        let span = node_to_span(node);

        let id = *next_id;
        *next_id += 1;

        let mut unit = RawCodeUnit::new(
            CodeUnitType::Module,
            Language::CSharp,
            name,
            file_path.to_path_buf(),
            span,
        );
        unit.temp_id = id;
        unit.qualified_name = qname.clone();
        unit.visibility = Visibility::Public;
        units.push(unit);

        // Recurse into namespace body (or file-scoped namespace children)
        if let Some(body) = node.child_by_field_name("body") {
            self.extract_from_node(body, source, file_path, units, next_id, &qname);
        } else {
            // File-scoped namespace: children are siblings
            self.extract_from_node(node, source, file_path, units, next_id, &qname);
        }
    }

    fn extract_method(
        &self,
        node: tree_sitter::Node,
        source: &str,
        file_path: &Path,
        parent_qname: &str,
        next_id: &mut u64,
    ) -> Option<RawCodeUnit> {
        let name_node = node.child_by_field_name("name")?;
        let name = get_node_text(name_node, source).to_string();
        let qname = cs_qname(parent_qname, &name);
        let span = node_to_span(node);
        let vis = extract_cs_visibility(node, source);

        let id = *next_id;
        *next_id += 1;

        let is_test = has_cs_attribute(node, source, "Test")
            || has_cs_attribute(node, source, "Fact")
            || has_cs_attribute(node, source, "Theory")
            || has_cs_attribute(node, source, "TestMethod");
        let unit_type = if is_test {
            CodeUnitType::Test
        } else {
            CodeUnitType::Function
        };

        let is_async = node_text_contains_modifier(node, source, "async");

        let mut unit = RawCodeUnit::new(
            unit_type,
            Language::CSharp,
            name,
            file_path.to_path_buf(),
            span,
        );
        unit.temp_id = id;
        unit.qualified_name = qname;
        unit.visibility = vis;
        unit.is_async = is_async;

        Some(unit)
    }

    fn extract_property(
        &self,
        node: tree_sitter::Node,
        source: &str,
        file_path: &Path,
        parent_qname: &str,
        next_id: &mut u64,
    ) -> Option<RawCodeUnit> {
        let name_node = node.child_by_field_name("name")?;
        let name = get_node_text(name_node, source).to_string();
        let qname = cs_qname(parent_qname, &name);
        let span = node_to_span(node);
        let vis = extract_cs_visibility(node, source);

        let id = *next_id;
        *next_id += 1;

        let mut unit = RawCodeUnit::new(
            CodeUnitType::Symbol,
            Language::CSharp,
            name,
            file_path.to_path_buf(),
            span,
        );
        unit.temp_id = id;
        unit.qualified_name = qname;
        unit.visibility = vis;

        Some(unit)
    }

    fn extract_using(
        &self,
        node: tree_sitter::Node,
        source: &str,
        file_path: &Path,
        parent_qname: &str,
        next_id: &mut u64,
    ) -> Option<RawCodeUnit> {
        let text = get_node_text(node, source)
            .trim_start_matches("using ")
            .trim_start_matches("global ")
            .trim_start_matches("static ")
            .trim_end_matches(';')
            .trim()
            .to_string();
        let span = node_to_span(node);

        let id = *next_id;
        *next_id += 1;

        let mut unit = RawCodeUnit::new(
            CodeUnitType::Import,
            Language::CSharp,
            text,
            file_path.to_path_buf(),
            span,
        );
        unit.temp_id = id;
        unit.qualified_name = cs_qname(parent_qname, "using");

        Some(unit)
    }
}

impl LanguageParser for CSharpParser {
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
            Language::CSharp,
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
        name.ends_with("Tests.cs") || name.ends_with("Test.cs") || name.starts_with("Test")
    }
}

fn cs_qname(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{}.{}", parent, name)
    }
}

/// Extract visibility from C# modifiers.
fn extract_cs_visibility(node: tree_sitter::Node, source: &str) -> Visibility {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "modifier" {
            let text = get_node_text(child, source);
            match text {
                "public" => return Visibility::Public,
                "private" => return Visibility::Private,
                "protected" | "internal" => return Visibility::Public,
                _ => {}
            }
        }
    }
    Visibility::Private // C# default is private for members
}

/// Check if a member has a specific C# attribute.
fn has_cs_attribute(node: tree_sitter::Node, source: &str, attribute: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "attribute_list" {
            let text = get_node_text(child, source);
            if text.contains(attribute) {
                return true;
            }
        }
    }
    false
}

/// Check if a node's modifiers contain a specific keyword.
fn node_text_contains_modifier(node: tree_sitter::Node, source: &str, keyword: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "modifier" && get_node_text(child, source) == keyword {
            return true;
        }
    }
    false
}
