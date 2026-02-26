//! Java parsing using tree-sitter.
//!
//! Extracts classes, interfaces, enums, methods, constructors, imports, packages.

use std::path::Path;

use crate::types::{AcbResult, CodeUnitType, Language, Visibility};

use super::treesitter::{get_node_text, node_to_span};
use super::{LanguageParser, RawCodeUnit};

/// Java language parser.
pub struct JavaParser;

impl Default for JavaParser {
    fn default() -> Self {
        Self::new()
    }
}

impl JavaParser {
    /// Create a new Java parser.
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
                    self.extract_class(
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
                    self.extract_class(
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
                    self.extract_class(
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
                    self.extract_class(
                        child,
                        source,
                        file_path,
                        units,
                        next_id,
                        parent_qname,
                        CodeUnitType::Type,
                    );
                }
                "method_declaration" => {
                    if let Some(unit) =
                        self.extract_method(child, source, file_path, parent_qname, next_id)
                    {
                        units.push(unit);
                    }
                }
                "constructor_declaration" => {
                    if let Some(unit) =
                        self.extract_method(child, source, file_path, parent_qname, next_id)
                    {
                        units.push(unit);
                    }
                }
                "import_declaration" => {
                    if let Some(unit) =
                        self.extract_import(child, source, file_path, parent_qname, next_id)
                    {
                        units.push(unit);
                    }
                }
                "package_declaration" => {
                    // Captured as module-level metadata, skip as unit
                }
                _ => {}
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn extract_class(
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
        let qname = java_qname(parent_qname, &name);
        let span = node_to_span(node);
        let vis = extract_java_visibility(node, source);

        let id = *next_id;
        *next_id += 1;

        let mut unit = RawCodeUnit::new(
            unit_type,
            Language::Java,
            name,
            file_path.to_path_buf(),
            span,
        );
        unit.temp_id = id;
        unit.qualified_name = qname.clone();
        unit.visibility = vis;
        units.push(unit);

        // Recurse into the class body
        if let Some(body) = node.child_by_field_name("body") {
            self.extract_from_node(body, source, file_path, units, next_id, &qname);
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
        let qname = java_qname(parent_qname, &name);
        let span = node_to_span(node);
        let vis = extract_java_visibility(node, source);

        let id = *next_id;
        *next_id += 1;

        let is_test = has_annotation(node, source, "Test")
            || has_annotation(node, source, "ParameterizedTest")
            || name.starts_with("test");
        let unit_type = if is_test {
            CodeUnitType::Test
        } else {
            CodeUnitType::Function
        };

        let mut unit = RawCodeUnit::new(
            unit_type,
            Language::Java,
            name,
            file_path.to_path_buf(),
            span,
        );
        unit.temp_id = id;
        unit.qualified_name = qname;
        unit.visibility = vis;

        Some(unit)
    }

    fn extract_import(
        &self,
        node: tree_sitter::Node,
        source: &str,
        file_path: &Path,
        parent_qname: &str,
        next_id: &mut u64,
    ) -> Option<RawCodeUnit> {
        let text = get_node_text(node, source)
            .trim_start_matches("import ")
            .trim_start_matches("static ")
            .trim_end_matches(';')
            .trim()
            .to_string();
        let span = node_to_span(node);

        let id = *next_id;
        *next_id += 1;

        let mut unit = RawCodeUnit::new(
            CodeUnitType::Import,
            Language::Java,
            text,
            file_path.to_path_buf(),
            span,
        );
        unit.temp_id = id;
        unit.qualified_name = java_qname(parent_qname, "import");

        Some(unit)
    }
}

impl LanguageParser for JavaParser {
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
            Language::Java,
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
        name.ends_with("Test.java")
            || name.ends_with("Tests.java")
            || name.starts_with("Test")
            || name.ends_with("IT.java")
    }
}

fn java_qname(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{}.{}", parent, name)
    }
}

/// Extract visibility from Java modifiers.
fn extract_java_visibility(node: tree_sitter::Node, source: &str) -> Visibility {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "modifiers" {
            let text = get_node_text(child, source);
            if text.contains("public") {
                return Visibility::Public;
            } else if text.contains("private") {
                return Visibility::Private;
            } else if text.contains("protected") {
                return Visibility::Public; // close enough for graph purposes
            }
        }
    }
    // Java default (package-private) — treat as public for graph
    Visibility::Public
}

/// Check if a method/class has a specific annotation.
fn has_annotation(node: tree_sitter::Node, source: &str, annotation: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "modifiers" {
            let mut inner_cursor = child.walk();
            for modifier in child.children(&mut inner_cursor) {
                if modifier.kind() == "marker_annotation" || modifier.kind() == "annotation" {
                    let text = get_node_text(modifier, source);
                    if text.contains(annotation) {
                        return true;
                    }
                }
            }
        }
    }
    false
}
