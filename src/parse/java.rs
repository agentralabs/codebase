//! Java parsing using tree-sitter.
//!
//! Extracts classes, interfaces, enums, methods, constructors, imports, packages.

use std::collections::HashMap;
use std::path::{Component, Path};

use crate::types::{AcbResult, CodeUnitType, Language, Span, Visibility};

use super::treesitter::{get_node_text, node_to_span};
use super::{LanguageParser, RawCodeUnit, RawReference, ReferenceKind};

/// Java language parser.
pub struct JavaParser;

#[derive(Default)]
struct SyntheticCounters {
    lambda: u32,
    anonymous: u32,
}

#[derive(Clone)]
struct TraversalFrame<'a> {
    node: tree_sitter::Node<'a>,
    scope_qname: String,
    current_type_qname: Option<String>,
    callable_temp_id: Option<u64>,
    callable_qname: Option<String>,
}

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

    fn walk_tree_iterative<'a>(
        &self,
        root: tree_sitter::Node<'a>,
        source: &str,
        file_path: &Path,
        namespace_root: &str,
        units: &mut Vec<RawCodeUnit>,
        index_by_temp_id: &mut HashMap<u64, usize>,
        next_id: &mut u64,
        counters: &mut SyntheticCounters,
    ) {
        let mut pending_refs: HashMap<u64, Vec<RawReference>> = HashMap::new();
        let mut stack = vec![TraversalFrame {
            node: root,
            scope_qname: namespace_root.to_string(),
            current_type_qname: None,
            callable_temp_id: None,
            callable_qname: None,
        }];

        while let Some(frame) = stack.pop() {
            let node = frame.node;

            if let Some(callable_id) = frame.callable_temp_id {
                self.collect_inline_callable_refs(node, source, callable_id, &mut pending_refs);
            }

            match node.kind() {
                "class_declaration" => {
                    if let Some(unit) = self.extract_class_node(
                        node,
                        source,
                        file_path,
                        &frame.scope_qname,
                        CodeUnitType::Type,
                        next_id,
                    ) {
                        let class_qname = unit.qualified_name.clone();
                        self.push_unit(units, index_by_temp_id, unit);
                        self.push_children(
                            &mut stack,
                            node,
                            class_qname.clone(),
                            Some(class_qname),
                            None,
                            None,
                        );
                        continue;
                    }
                }
                "interface_declaration" => {
                    if let Some(unit) = self.extract_class_node(
                        node,
                        source,
                        file_path,
                        &frame.scope_qname,
                        CodeUnitType::Trait,
                        next_id,
                    ) {
                        let class_qname = unit.qualified_name.clone();
                        self.push_unit(units, index_by_temp_id, unit);
                        self.push_children(
                            &mut stack,
                            node,
                            class_qname.clone(),
                            Some(class_qname),
                            None,
                            None,
                        );
                        continue;
                    }
                }
                "enum_declaration" | "record_declaration" => {
                    if let Some(unit) = self.extract_class_node(
                        node,
                        source,
                        file_path,
                        &frame.scope_qname,
                        CodeUnitType::Type,
                        next_id,
                    ) {
                        let class_qname = unit.qualified_name.clone();
                        self.push_unit(units, index_by_temp_id, unit);
                        self.push_children(
                            &mut stack,
                            node,
                            class_qname.clone(),
                            Some(class_qname),
                            None,
                            None,
                        );
                        continue;
                    }
                }
                "method_declaration" | "constructor_declaration" => {
                    if let Some(unit) = self.extract_method_node(
                        node,
                        source,
                        file_path,
                        &frame.scope_qname,
                        next_id,
                    ) {
                        let callable_id = unit.temp_id;
                        let callable_qname = unit.qualified_name.clone();
                        self.push_unit(units, index_by_temp_id, unit);
                        self.push_children(
                            &mut stack,
                            node,
                            callable_qname.clone(),
                            frame.current_type_qname.clone(),
                            Some(callable_id),
                            Some(callable_qname),
                        );
                        continue;
                    }
                }
                "import_declaration" => {
                    if let Some(unit) = self.extract_import_node(
                        node,
                        source,
                        file_path,
                        &frame.scope_qname,
                        next_id,
                    ) {
                        self.push_unit(units, index_by_temp_id, unit);
                    }
                }
                "lambda_expression" => {
                    if let Some(unit) = self.extract_lambda_node(
                        node,
                        source,
                        file_path,
                        &frame.scope_qname,
                        next_id,
                        counters,
                    ) {
                        let callable_id = unit.temp_id;
                        let callable_qname = unit.qualified_name.clone();
                        self.push_unit(units, index_by_temp_id, unit);
                        self.push_children(
                            &mut stack,
                            node,
                            callable_qname.clone(),
                            frame.current_type_qname.clone(),
                            Some(callable_id),
                            Some(callable_qname),
                        );
                        continue;
                    }
                }
                "object_creation_expression" => {
                    if let Some(class_body) = find_anonymous_class_body(node) {
                        if let Some(unit) = self.extract_anonymous_class_node(
                            node,
                            source,
                            file_path,
                            &frame.scope_qname,
                            next_id,
                            counters,
                        ) {
                            let anon_qname = unit.qualified_name.clone();
                            self.push_unit(units, index_by_temp_id, unit);

                            let mut children = collect_direct_children(node);
                            children.reverse();
                            for child in children {
                                if child == class_body {
                                    stack.push(TraversalFrame {
                                        node: child,
                                        scope_qname: anon_qname.clone(),
                                        current_type_qname: Some(anon_qname.clone()),
                                        callable_temp_id: None,
                                        callable_qname: None,
                                    });
                                } else {
                                    stack.push(TraversalFrame {
                                        node: child,
                                        scope_qname: frame.scope_qname.clone(),
                                        current_type_qname: frame.current_type_qname.clone(),
                                        callable_temp_id: frame.callable_temp_id,
                                        callable_qname: frame.callable_qname.clone(),
                                    });
                                }
                            }
                            continue;
                        }
                    }
                }
                _ => {}
            }

            self.push_children(
                &mut stack,
                node,
                frame.scope_qname,
                frame.current_type_qname,
                frame.callable_temp_id,
                frame.callable_qname,
            );
        }

        for (temp_id, refs) in pending_refs {
            if let Some(idx) = index_by_temp_id.get(&temp_id).copied() {
                let unit = &mut units[idx];
                for r in refs {
                    push_reference(&mut unit.references, r.name, r.kind, r.span);
                }
            }
        }
    }

    fn push_children<'a>(
        &self,
        stack: &mut Vec<TraversalFrame<'a>>,
        node: tree_sitter::Node<'a>,
        scope_qname: String,
        current_type_qname: Option<String>,
        callable_temp_id: Option<u64>,
        callable_qname: Option<String>,
    ) {
        let mut children = collect_direct_children(node);
        children.reverse();
        for child in children {
            stack.push(TraversalFrame {
                node: child,
                scope_qname: scope_qname.clone(),
                current_type_qname: current_type_qname.clone(),
                callable_temp_id,
                callable_qname: callable_qname.clone(),
            });
        }
    }

    fn push_unit(
        &self,
        units: &mut Vec<RawCodeUnit>,
        index_by_temp_id: &mut HashMap<u64, usize>,
        unit: RawCodeUnit,
    ) {
        index_by_temp_id.insert(unit.temp_id, units.len());
        units.push(unit);
    }

    fn collect_inline_callable_refs(
        &self,
        node: tree_sitter::Node,
        source: &str,
        callable_temp_id: u64,
        pending_refs: &mut HashMap<u64, Vec<RawReference>>,
    ) {
        let refs = pending_refs.entry(callable_temp_id).or_default();

        match node.kind() {
            "method_invocation" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| get_node_text(n, source).trim().to_string())
                    .unwrap_or_else(|| parse_call_name(get_node_text(node, source)));
                push_reference(refs, name, ReferenceKind::Call, node_to_span(node));
            }
            "object_creation_expression" => {
                let ctor = node
                    .child_by_field_name("type")
                    .map(|n| normalize_type_text(get_node_text(n, source)))
                    .unwrap_or_else(|| parse_new_target(get_node_text(node, source)));
                push_reference(refs, ctor, ReferenceKind::Call, node_to_span(node));
            }
            "explicit_constructor_invocation" => {
                let name = parse_call_name(get_node_text(node, source));
                push_reference(refs, name, ReferenceKind::Call, node_to_span(node));
            }
            "field_access" => {
                let name = node
                    .child_by_field_name("field")
                    .or_else(|| node.child_by_field_name("name"))
                    .map(|n| get_node_text(n, source).trim().to_string())
                    .unwrap_or_else(|| parse_access_name(get_node_text(node, source)));
                push_reference(refs, name, ReferenceKind::Access, node_to_span(node));
            }
            "local_variable_declaration"
            | "formal_parameter"
            | "spread_parameter"
            | "receiver_parameter"
            | "catch_formal_parameter" => {
                if let Some(type_node) = node.child_by_field_name("type") {
                    push_type_refs_from_text(
                        get_node_text(type_node, source),
                        node_to_span(type_node),
                        refs,
                    );
                }
            }
            _ => {}
        }
    }

    fn extract_class_node(
        &self,
        node: tree_sitter::Node,
        source: &str,
        file_path: &Path,
        parent_qname: &str,
        unit_type: CodeUnitType,
        next_id: &mut u64,
    ) -> Option<RawCodeUnit> {
        let name = node
            .child_by_field_name("name")
            .map(|n| get_node_text(n, source).to_string())?;
        let qname = java_qname(parent_qname, &name, None);

        let id = *next_id;
        *next_id += 1;

        let mut unit = RawCodeUnit::new(
            unit_type,
            Language::Java,
            name,
            file_path.to_path_buf(),
            node_to_span(node),
        );
        unit.temp_id = id;
        unit.qualified_name = qname;
        unit.visibility = extract_java_visibility(node, source);
        extract_heritage_refs(node, source, &mut unit.references);
        extract_direct_class_field_type_refs(node, source, &mut unit.references);
        Some(unit)
    }

    fn extract_method_node(
        &self,
        node: tree_sitter::Node,
        source: &str,
        file_path: &Path,
        parent_qname: &str,
        next_id: &mut u64,
    ) -> Option<RawCodeUnit> {
        let name_node = node.child_by_field_name("name")?;
        let name = get_node_text(name_node, source).to_string();
        let overload = method_overload_suffix(node, source);
        let qname = java_qname(parent_qname, &name, Some(&overload));

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
            name.clone(),
            file_path.to_path_buf(),
            node_to_span(node),
        );
        unit.temp_id = id;
        unit.qualified_name = qname;
        unit.signature = Some(method_signature(node, source, &name));
        unit.visibility = extract_java_visibility(node, source);
        extract_method_header_type_refs(node, source, &mut unit.references);
        Some(unit)
    }

    fn extract_import_node(
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
        unit.qualified_name = java_qname(
            parent_qname,
            &format!("import.{}", sanitize_qname_leaf(&unit.name)),
            None,
        );
        unit.references.push(RawReference {
            name: unit.name.clone(),
            kind: ReferenceKind::Import,
            span,
        });
        Some(unit)
    }

    fn extract_lambda_node(
        &self,
        node: tree_sitter::Node,
        source: &str,
        file_path: &Path,
        parent_qname: &str,
        next_id: &mut u64,
        counters: &mut SyntheticCounters,
    ) -> Option<RawCodeUnit> {
        counters.lambda += 1;
        let span = node_to_span(node);
        let name = format!(
            "lambda${}_{}_{}",
            span.start_line, span.start_col, counters.lambda
        );
        let qname = java_qname(parent_qname, &name, None);

        let id = *next_id;
        *next_id += 1;

        let mut unit = RawCodeUnit::new(
            CodeUnitType::Function,
            Language::Java,
            name,
            file_path.to_path_buf(),
            span,
        );
        unit.temp_id = id;
        unit.qualified_name = qname;
        unit.visibility = Visibility::Private;

        if let Some(params) = node.child_by_field_name("parameters") {
            unit.signature = Some(get_node_text(params, source).trim().to_string());
            collect_type_refs_from_param_list(params, source, &mut unit.references);
        }

        Some(unit)
    }

    fn extract_anonymous_class_node(
        &self,
        node: tree_sitter::Node,
        source: &str,
        file_path: &Path,
        parent_qname: &str,
        next_id: &mut u64,
        counters: &mut SyntheticCounters,
    ) -> Option<RawCodeUnit> {
        let class_body = find_anonymous_class_body(node)?;
        counters.anonymous += 1;

        let span = node_to_span(class_body);
        let name = format!(
            "anonymous${}_{}_{}",
            span.start_line, span.start_col, counters.anonymous
        );
        let qname = java_qname(parent_qname, &name, None);

        let id = *next_id;
        *next_id += 1;

        let mut unit = RawCodeUnit::new(
            CodeUnitType::Type,
            Language::Java,
            name,
            file_path.to_path_buf(),
            span,
        );
        unit.temp_id = id;
        unit.qualified_name = qname;
        unit.visibility = Visibility::Private;

        if let Some(type_node) = node.child_by_field_name("type") {
            let type_text = normalize_type_text(get_node_text(type_node, source));
            push_reference(
                &mut unit.references,
                type_text,
                ReferenceKind::Inherit,
                node_to_span(type_node),
            );
        }

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
        let mut index_by_temp_id: HashMap<u64, usize> = HashMap::new();
        let mut next_id = 0u64;
        let mut counters = SyntheticCounters::default();

        let module_name = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        let package_name = extract_package_name(tree.root_node(), source)
            .or_else(|| fallback_package_from_path(file_path))
            .unwrap_or_default();
        let namespace_root = if package_name.is_empty() {
            module_name.clone()
        } else {
            package_name.clone()
        };

        self.walk_tree_iterative(
            tree.root_node(),
            source,
            file_path,
            &namespace_root,
            &mut units,
            &mut index_by_temp_id,
            &mut next_id,
            &mut counters,
        );

        // Emit module after traversal to keep type/function lookup precedence
        // for same-name symbols in a file.
        let mut module_unit = RawCodeUnit::new(
            CodeUnitType::Module,
            Language::Java,
            module_name.clone(),
            file_path.to_path_buf(),
            node_to_span(tree.root_node()),
        );
        module_unit.temp_id = next_id;
        let module_leaf = sanitize_qname_leaf(&module_name);
        module_unit.qualified_name = if namespace_root.is_empty() {
            format!("$module.{}", module_leaf)
        } else {
            format!("{}.$module.{}", namespace_root, module_leaf)
        };
        units.push(module_unit);

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

fn collect_direct_children<'a>(node: tree_sitter::Node<'a>) -> Vec<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

fn find_anonymous_class_body(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    if let Some(class_body) = node.child_by_field_name("class_body") {
        return Some(class_body);
    }

    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .find(|child| child.kind() == "class_body");
    found
}

fn java_qname(parent: &str, name: &str, overload_suffix: Option<&str>) -> String {
    let leaf = if let Some(suffix) = overload_suffix {
        format!("{}${}", name, sanitize_qname_leaf(suffix))
    } else {
        name.to_string()
    };

    if parent.is_empty() {
        leaf
    } else {
        format!("{}.{}", parent, leaf)
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
            }
            if text.contains("private") {
                return Visibility::Private;
            }
            if text.contains("protected") {
                return Visibility::Protected;
            }
        }
    }

    // Java default (package-private): treat as public for graph
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

fn extract_package_name(root: tree_sitter::Node, source: &str) -> Option<String> {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "package_declaration" {
            let package_text = get_node_text(child, source)
                .trim_start_matches("package")
                .trim_end_matches(';')
                .trim();
            if is_valid_package(package_text) {
                return Some(package_text.to_string());
            }
        }
    }
    None
}

fn fallback_package_from_path(file_path: &Path) -> Option<String> {
    let parent = file_path.parent()?;
    let parts: Vec<String> = parent
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => s.to_str().map(|x| x.to_string()),
            _ => None,
        })
        .collect();

    if parts.is_empty() {
        return None;
    }

    let markers = ["sources", "source", "src", "java", "kotlin"];
    if let Some(idx) = parts.iter().rposition(|p| {
        let lower = p.to_ascii_lowercase();
        markers.contains(&lower.as_str())
    }) {
        let candidate: Vec<String> = parts[idx + 1..]
            .iter()
            .filter(|seg| is_valid_package_segment(seg))
            .cloned()
            .collect();
        if !candidate.is_empty() {
            return Some(candidate.join("."));
        }
    }

    let mut tail: Vec<String> = parts
        .iter()
        .rev()
        .filter(|seg| is_valid_package_segment(seg))
        .take(4)
        .cloned()
        .collect();
    tail.reverse();

    if tail.is_empty() {
        None
    } else {
        Some(tail.join("."))
    }
}

fn is_valid_package(package_name: &str) -> bool {
    !package_name.is_empty() && package_name.split('.').all(is_valid_package_segment)
}

fn is_valid_package_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

fn method_signature(node: tree_sitter::Node, source: &str, method_name: &str) -> String {
    let param_types = collect_parameter_type_texts(node, source);
    let params = if param_types.is_empty() {
        "()".to_string()
    } else {
        format!("({})", param_types.join(", "))
    };

    let return_ty = node
        .child_by_field_name("type")
        .map(|n| format!(" -> {}", normalize_type_text(get_node_text(n, source))))
        .unwrap_or_default();

    format!("{}{}{}", method_name, params, return_ty)
}

fn method_overload_suffix(node: tree_sitter::Node, source: &str) -> String {
    let param_types = collect_parameter_type_texts(node, source);
    if !param_types.is_empty() {
        return format!("sig_{}", sanitize_qname_leaf(&param_types.join("_")));
    }
    format!("arity_{}", parameter_arity(node))
}

fn parameter_arity(node: tree_sitter::Node) -> usize {
    let Some(params) = node.child_by_field_name("parameters") else {
        return 0;
    };

    let mut count = 0usize;
    let mut cursor = params.walk();
    for child in params.children(&mut cursor) {
        match child.kind() {
            "formal_parameter" | "spread_parameter" | "receiver_parameter" => {
                count += 1;
            }
            _ => {}
        }
    }
    count
}

fn collect_parameter_type_texts(node: tree_sitter::Node, source: &str) -> Vec<String> {
    let mut result = Vec::new();
    let Some(params) = node.child_by_field_name("parameters") else {
        return result;
    };

    let mut cursor = params.walk();
    for child in params.children(&mut cursor) {
        match child.kind() {
            "formal_parameter" | "spread_parameter" | "receiver_parameter" => {
                if let Some(type_node) = child.child_by_field_name("type") {
                    let text = normalize_type_text(get_node_text(type_node, source));
                    if !text.is_empty() {
                        result.push(text);
                    }
                }
            }
            _ => {}
        }
    }

    result
}

fn collect_type_refs_from_param_list(
    params: tree_sitter::Node,
    source: &str,
    refs: &mut Vec<RawReference>,
) {
    let mut cursor = params.walk();
    for child in params.children(&mut cursor) {
        match child.kind() {
            "formal_parameter" | "spread_parameter" | "receiver_parameter" => {
                if let Some(type_node) = child.child_by_field_name("type") {
                    push_type_refs_from_text(
                        get_node_text(type_node, source),
                        node_to_span(type_node),
                        refs,
                    );
                }
            }
            _ => {}
        }
    }
}

fn extract_heritage_refs(node: tree_sitter::Node, source: &str, refs: &mut Vec<RawReference>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "superclass" => {
                let text = get_node_text(child, source).trim();
                for ty in split_type_candidates(text.trim_start_matches("extends ")) {
                    push_reference(refs, ty, ReferenceKind::Inherit, node_to_span(child));
                }
            }
            "super_interfaces" | "interfaces" => {
                let text = get_node_text(child, source).trim();
                for ty in split_type_candidates(
                    text.trim_start_matches("implements ")
                        .trim_start_matches("extends "),
                ) {
                    push_reference(refs, ty, ReferenceKind::Implement, node_to_span(child));
                }
            }
            _ => {}
        }
    }
}

fn extract_direct_class_field_type_refs(
    class_node: tree_sitter::Node,
    source: &str,
    refs: &mut Vec<RawReference>,
) {
    let Some(body) = class_node.child_by_field_name("body") else {
        return;
    };

    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "field_declaration" {
            if let Some(type_node) = child.child_by_field_name("type") {
                push_type_refs_from_text(
                    get_node_text(type_node, source),
                    node_to_span(type_node),
                    refs,
                );
            }
        }
    }
}

fn extract_method_header_type_refs(
    method_node: tree_sitter::Node,
    source: &str,
    refs: &mut Vec<RawReference>,
) {
    if let Some(return_type) = method_node.child_by_field_name("type") {
        push_type_refs_from_text(
            get_node_text(return_type, source),
            node_to_span(return_type),
            refs,
        );
    }

    if let Some(parameters) = method_node.child_by_field_name("parameters") {
        collect_type_refs_from_param_list(parameters, source, refs);
    }

    let mut cursor = method_node.walk();
    for child in method_node.children(&mut cursor) {
        if child.kind() == "throws" {
            let text = get_node_text(child, source)
                .trim_start_matches("throws ")
                .trim();
            for ty in split_type_candidates(text) {
                push_reference(refs, ty, ReferenceKind::TypeUse, node_to_span(child));
            }
        }
    }
}

fn push_type_refs_from_text(raw: &str, span: Span, refs: &mut Vec<RawReference>) {
    for ty in split_type_candidates(raw) {
        push_reference(refs, ty, ReferenceKind::TypeUse, span);
    }
}

fn split_type_candidates(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in
        raw.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '.'))
    {
        if token.is_empty() || is_java_primitive(token) {
            continue;
        }
        let Some(first) = token.chars().next() else {
            continue;
        };
        if first.is_ascii_uppercase() || token.contains('.') || token.contains('$') {
            let normalized = normalize_type_text(token);
            if !normalized.is_empty() && !out.contains(&normalized) {
                out.push(normalized);
            }
        }
    }
    out
}

fn is_java_primitive(token: &str) -> bool {
    matches!(
        token,
        "byte" | "short" | "int" | "long" | "float" | "double" | "char" | "boolean" | "void"
    )
}

fn normalize_type_text(raw: &str) -> String {
    raw.split_whitespace().collect::<String>()
}

fn parse_call_name(raw: &str) -> String {
    let head = raw.split('(').next().unwrap_or("").trim();
    head.rsplit('.').next().unwrap_or(head).trim().to_string()
}

fn parse_new_target(raw: &str) -> String {
    let tail = raw.trim_start_matches("new ").trim();
    tail.split('(').next().unwrap_or("").trim().to_string()
}

fn parse_access_name(raw: &str) -> String {
    raw.rsplit('.').next().unwrap_or(raw).trim().to_string()
}

fn push_reference(
    refs: &mut Vec<RawReference>,
    name: impl Into<String>,
    kind: ReferenceKind,
    span: Span,
) {
    let name = name.into();
    if name.is_empty() {
        return;
    }
    if refs.iter().any(|r| r.kind == kind && r.name == name) {
        return;
    }
    refs.push(RawReference { name, kind, span });
}

fn sanitize_qname_leaf(raw: &str) -> String {
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '$' | '.' | '-') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}
