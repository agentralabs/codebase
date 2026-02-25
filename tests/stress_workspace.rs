//! Workspace stress tests — multi-context operations, cross-queries, scale.

use std::path::PathBuf;

use agentic_codebase::graph::CodeGraph;
use agentic_codebase::types::{CodeUnit, CodeUnitType, Language, Span};
use agentic_codebase::workspace::{ContextRole, WorkspaceManager};

// ─────────────────────── helpers ───────────────────────

fn make_graph(symbols: &[(&str, CodeUnitType)]) -> CodeGraph {
    let mut graph = CodeGraph::with_default_dimension();
    for (name, utype) in symbols {
        let mut unit = CodeUnit::new(
            *utype,
            Language::Rust,
            name.to_string(),
            format!("crate::{}", name),
            PathBuf::from(format!("src/{}.rs", name)),
            Span::new(1, 0, 10, 0),
        );
        unit.signature = Some(format!("fn {}()", name));
        graph.add_unit(unit);
    }
    graph
}

fn make_named_graph(name: &str, sig: Option<&str>, lang: Language) -> CodeGraph {
    let mut graph = CodeGraph::with_default_dimension();
    let mut unit = CodeUnit::new(
        CodeUnitType::Function,
        lang,
        name.to_string(),
        format!("crate::{}", name),
        PathBuf::from(format!("src/{}.rs", name)),
        Span::new(1, 0, 10, 0),
    );
    if let Some(s) = sig {
        unit.signature = Some(s.to_string());
    }
    graph.add_unit(unit);
    graph
}

fn scale_graph(n: usize, prefix: &str) -> CodeGraph {
    let mut graph = CodeGraph::with_default_dimension();
    for i in 0..n {
        graph.add_unit(CodeUnit::new(
            CodeUnitType::Function,
            Language::Rust,
            format!("{}_{}", prefix, i),
            format!("crate::{}_{}", prefix, i),
            PathBuf::from(format!("src/{}_{}.rs", prefix, i)),
            Span::new(1, 0, 10, 0),
        ));
    }
    graph
}

// ============================================================================
// Create / Add / List
// ============================================================================

#[test]
fn test_workspace_create() {
    let mut mgr = WorkspaceManager::new();
    let id = mgr.create("test-ws");
    assert!(id.starts_with("ws-"));
    assert_eq!(mgr.get_active(), Some(id.as_str()));
}

#[test]
fn test_workspace_create_multiple() {
    let mut mgr = WorkspaceManager::new();
    let id1 = mgr.create("ws-1");
    let id2 = mgr.create("ws-2");
    assert_ne!(id1, id2);
    // Last created is active.
    assert_eq!(mgr.get_active(), Some(id2.as_str()));
}

#[test]
fn test_workspace_add_multiple_contexts() {
    let mut mgr = WorkspaceManager::new();
    let ws = mgr.create("migration");

    let g1 = make_graph(&[("foo", CodeUnitType::Function)]);
    let g2 = make_graph(&[("bar", CodeUnitType::Function)]);
    let g3 = make_graph(&[("baz", CodeUnitType::Function)]);

    let ctx1 = mgr.add_context(&ws, "/cpp", ContextRole::Source, Some("C++".into()), g1).unwrap();
    let ctx2 = mgr.add_context(&ws, "/rust", ContextRole::Target, Some("Rust".into()), g2).unwrap();
    let ctx3 = mgr.add_context(&ws, "/ref", ContextRole::Reference, None, g3).unwrap();

    assert_ne!(ctx1, ctx2);
    assert_ne!(ctx2, ctx3);

    let workspace = mgr.list(&ws).unwrap();
    assert_eq!(workspace.contexts.len(), 3);
    assert_eq!(workspace.contexts[0].role, ContextRole::Source);
    assert_eq!(workspace.contexts[1].role, ContextRole::Target);
    assert_eq!(workspace.contexts[2].role, ContextRole::Reference);
}

// ============================================================================
// Query single context
// ============================================================================

#[test]
fn test_workspace_query_single() {
    let mut mgr = WorkspaceManager::new();
    let ws = mgr.create("q");
    let ctx = mgr
        .add_context(&ws, "/a", ContextRole::Source, None, make_graph(&[("alpha", CodeUnitType::Function)]))
        .unwrap();

    let matches = mgr.query_context(&ws, &ctx, "alph").unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].name, "alpha");
}

#[test]
fn test_workspace_query_single_no_match() {
    let mut mgr = WorkspaceManager::new();
    let ws = mgr.create("q");
    let ctx = mgr
        .add_context(&ws, "/a", ContextRole::Source, None, make_graph(&[("alpha", CodeUnitType::Function)]))
        .unwrap();

    let matches = mgr.query_context(&ws, &ctx, "omega").unwrap();
    assert!(matches.is_empty());
}

// ============================================================================
// Query across contexts
// ============================================================================

#[test]
fn test_workspace_query_all() {
    let mut mgr = WorkspaceManager::new();
    let ws = mgr.create("multi");

    let g1 = make_graph(&[("process", CodeUnitType::Function), ("handle", CodeUnitType::Function)]);
    let g2 = make_graph(&[("process", CodeUnitType::Function), ("other", CodeUnitType::Function)]);

    mgr.add_context(&ws, "/a", ContextRole::Source, None, g1).unwrap();
    mgr.add_context(&ws, "/b", ContextRole::Target, None, g2).unwrap();

    let results = mgr.query_all(&ws, "process").unwrap();
    assert_eq!(results.len(), 2, "Both contexts should have 'process'");
}

#[test]
fn test_workspace_query_all_partial() {
    let mut mgr = WorkspaceManager::new();
    let ws = mgr.create("partial");

    let g1 = make_graph(&[("unique_source_fn", CodeUnitType::Function)]);
    let g2 = make_graph(&[("common", CodeUnitType::Function)]);

    mgr.add_context(&ws, "/a", ContextRole::Source, None, g1).unwrap();
    mgr.add_context(&ws, "/b", ContextRole::Target, None, g2).unwrap();

    let results = mgr.query_all(&ws, "unique_source").unwrap();
    assert_eq!(results.len(), 1, "Only source context should match");
}

// ============================================================================
// Cross-reference symbols
// ============================================================================

#[test]
fn test_workspace_xref_all_contexts() {
    let mut mgr = WorkspaceManager::new();
    let ws = mgr.create("xref");

    let g1 = make_graph(&[("shared_fn", CodeUnitType::Function)]);
    let g2 = make_graph(&[("shared_fn", CodeUnitType::Function)]);
    let g3 = make_graph(&[("shared_fn", CodeUnitType::Function)]);

    mgr.add_context(&ws, "/a", ContextRole::Source, None, g1).unwrap();
    mgr.add_context(&ws, "/b", ContextRole::Target, None, g2).unwrap();
    mgr.add_context(&ws, "/c", ContextRole::Reference, None, g3).unwrap();

    let xref = mgr.cross_reference(&ws, "shared_fn").unwrap();
    assert_eq!(xref.found_in.len(), 3);
    assert!(xref.missing_from.is_empty());
}

#[test]
fn test_workspace_xref_missing_from_target() {
    let mut mgr = WorkspaceManager::new();
    let ws = mgr.create("xref2");

    let g1 = make_graph(&[("source_only", CodeUnitType::Function)]);
    let g2 = make_graph(&[("other", CodeUnitType::Function)]);

    mgr.add_context(&ws, "/a", ContextRole::Source, None, g1).unwrap();
    mgr.add_context(&ws, "/b", ContextRole::Target, None, g2).unwrap();

    let xref = mgr.cross_reference(&ws, "source_only").unwrap();
    assert_eq!(xref.found_in.len(), 1);
    assert_eq!(xref.missing_from.len(), 1);
    assert_eq!(xref.missing_from[0].1, ContextRole::Target);
}

// ============================================================================
// Compare symbols
// ============================================================================

#[test]
fn test_workspace_compare_found_both() {
    let mut mgr = WorkspaceManager::new();
    let ws = mgr.create("cmp");

    mgr.add_context(&ws, "/a", ContextRole::Source, None,
        make_named_graph("process", Some("void process(int x)"), Language::Rust)).unwrap();
    mgr.add_context(&ws, "/b", ContextRole::Target, None,
        make_named_graph("process", Some("fn process(x: i32)"), Language::Rust)).unwrap();

    let cmp = mgr.compare(&ws, "process").unwrap();
    assert_eq!(cmp.contexts.len(), 2);
    assert!(cmp.contexts[0].found);
    assert!(cmp.contexts[1].found);
    assert!(!cmp.structural_diff.is_empty(), "Different signatures should produce structural diff");
}

#[test]
fn test_workspace_compare_found_source_only() {
    let mut mgr = WorkspaceManager::new();
    let ws = mgr.create("cmp2");

    mgr.add_context(&ws, "/a", ContextRole::Source, None,
        make_named_graph("legacy_fn", Some("int legacy_fn()"), Language::Rust)).unwrap();
    mgr.add_context(&ws, "/b", ContextRole::Target, None,
        make_graph(&[("other_fn", CodeUnitType::Function)])).unwrap();

    let cmp = mgr.compare(&ws, "legacy_fn").unwrap();
    assert!(cmp.contexts[0].found);
    assert!(!cmp.contexts[1].found);
}

// ============================================================================
// Scale tests
// ============================================================================

#[test]
fn test_workspace_scale_5_contexts() {
    let mut mgr = WorkspaceManager::new();
    let ws = mgr.create("scale");

    for i in 0..5 {
        let g = scale_graph(2_000, &format!("ctx{}", i));
        let role = match i {
            0 => ContextRole::Source,
            1 => ContextRole::Target,
            _ => ContextRole::Reference,
        };
        mgr.add_context(&ws, &format!("/ctx{}", i), role, None, g).unwrap();
    }

    let workspace = mgr.list(&ws).unwrap();
    assert_eq!(workspace.contexts.len(), 5);

    // Cross-query should work across all contexts.
    let start = std::time::Instant::now();
    let results = mgr.query_all(&ws, "ctx0_500").unwrap();
    let elapsed = start.elapsed();

    assert_eq!(results.len(), 1, "Only first context should match ctx0_500");
    assert!(
        elapsed.as_millis() < 500,
        "Cross-query on 5x2K graphs took {}ms, expected < 500ms",
        elapsed.as_millis()
    );
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn test_workspace_empty() {
    let mut mgr = WorkspaceManager::new();
    let ws = mgr.create("empty");

    let workspace = mgr.list(&ws).unwrap();
    assert!(workspace.contexts.is_empty());

    let results = mgr.query_all(&ws, "anything").unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_workspace_missing_context() {
    let mut mgr = WorkspaceManager::new();
    let ws = mgr.create("missing");
    mgr.add_context(&ws, "/a", ContextRole::Source, None, make_graph(&[("x", CodeUnitType::Function)])).unwrap();

    let err = mgr.query_context(&ws, "ctx-999", "x");
    assert!(err.is_err());
}

#[test]
fn test_workspace_not_found_error() {
    let mgr = WorkspaceManager::new();
    assert!(mgr.list("ws-999").is_err());
    assert!(mgr.query_all("ws-999", "x").is_err());
    assert!(mgr.compare("ws-999", "x").is_err());
    assert!(mgr.cross_reference("ws-999", "x").is_err());
}

#[test]
fn test_context_role_roundtrip() {
    for label in &["source", "target", "reference", "comparison"] {
        let role = ContextRole::from_str(label).unwrap();
        assert_eq!(role.label(), *label);
    }
    assert!(ContextRole::from_str("invalid").is_none());
    // Case-insensitive.
    assert_eq!(ContextRole::from_str("SOURCE"), Some(ContextRole::Source));
    assert_eq!(ContextRole::from_str("Target"), Some(ContextRole::Target));
}
