//! Integration stress tests — grounding + workspace together, real workflows.

use std::path::PathBuf;

use agentic_codebase::graph::CodeGraph;
use agentic_codebase::grounding::{Grounded, GroundingEngine, GroundingResult};
use agentic_codebase::types::{CodeUnit, CodeUnitType, Language, Span};
use agentic_codebase::workspace::{
    ContextRole, TranslationMap, TranslationStatus, WorkspaceManager,
};

// ─────────────────────── helpers ───────────────────────

fn cpp_graph() -> CodeGraph {
    let mut graph = CodeGraph::with_default_dimension();
    let symbols = vec![
        ("parse_config", "Config parse_config(const char* path)"),
        ("process_request", "Response process_request(Request req)"),
        ("validate_input", "bool validate_input(const Input& input)"),
        ("send_response", "void send_response(Response resp)"),
        ("log_error", "void log_error(const char* msg)"),
    ];
    for (name, sig) in symbols {
        let mut unit = CodeUnit::new(
            CodeUnitType::Function,
            Language::Rust, // Using Rust since we don't have C++ Language variant
            name.to_string(),
            format!("app::{}", name),
            PathBuf::from(format!("src/{}.cpp", name)),
            Span::new(1, 0, 20, 0),
        );
        unit.signature = Some(sig.to_string());
        graph.add_unit(unit);
    }
    graph
}

fn rust_graph_partial() -> CodeGraph {
    let mut graph = CodeGraph::with_default_dimension();
    // Only 2 of 5 functions ported so far.
    let symbols = vec![
        ("parse_config", "fn parse_config(path: &str) -> Config"),
        ("validate_input", "fn validate_input(input: &Input) -> bool"),
    ];
    for (name, sig) in symbols {
        let mut unit = CodeUnit::new(
            CodeUnitType::Function,
            Language::Rust,
            name.to_string(),
            format!("crate::{}", name),
            PathBuf::from(format!("src/{}.rs", name)),
            Span::new(1, 0, 20, 0),
        );
        unit.signature = Some(sig.to_string());
        graph.add_unit(unit);
    }
    graph
}

fn rust_graph_complete() -> CodeGraph {
    let mut graph = CodeGraph::with_default_dimension();
    let symbols = vec![
        ("parse_config", "fn parse_config(path: &str) -> Config"),
        ("process_request", "fn process_request(req: Request) -> Response"),
        ("validate_input", "fn validate_input(input: &Input) -> bool"),
        ("send_response", "fn send_response(resp: Response)"),
        ("log_error", "fn log_error(msg: &str)"),
    ];
    for (name, sig) in symbols {
        let mut unit = CodeUnit::new(
            CodeUnitType::Function,
            Language::Rust,
            name.to_string(),
            format!("crate::{}", name),
            PathBuf::from(format!("src/{}.rs", name)),
            Span::new(1, 0, 20, 0),
        );
        unit.signature = Some(sig.to_string());
        graph.add_unit(unit);
    }
    graph
}

// ============================================================================
// Full migration workflow
// ============================================================================

#[test]
fn test_migration_workflow_cpp_to_rust() {
    let mut mgr = WorkspaceManager::new();
    let ws = mgr.create("cpp-to-rust");

    // Step 1: Add source (C++) and partial target (Rust).
    let cpp = cpp_graph();
    let rust_partial = rust_graph_partial();

    let src_ctx = mgr
        .add_context(&ws, "/src/cpp", ContextRole::Source, Some("C++".into()), cpp)
        .unwrap();
    let tgt_ctx = mgr
        .add_context(&ws, "/src/rust", ContextRole::Target, Some("Rust".into()), rust_partial)
        .unwrap();

    // Step 2: Query source for all symbols.
    let source_results = mgr.query_context(&ws, &src_ctx, "").unwrap();
    assert_eq!(source_results.len(), 5, "Source should have 5 symbols");

    // Step 3: Check which are already in target.
    let xref_parse = mgr.cross_reference(&ws, "parse_config").unwrap();
    assert_eq!(xref_parse.found_in.len(), 2, "parse_config exists in both");

    let xref_send = mgr.cross_reference(&ws, "send_response").unwrap();
    assert_eq!(xref_send.found_in.len(), 1, "send_response only in source");
    assert_eq!(xref_send.missing_from.len(), 1, "send_response missing from target");

    // Step 4: Record translation progress.
    let mut tmap = TranslationMap::new(src_ctx.clone(), tgt_ctx.clone());
    tmap.record("parse_config", Some("parse_config"), TranslationStatus::Verified, None);
    tmap.record("validate_input", Some("validate_input"), TranslationStatus::Ported, None);
    tmap.record("process_request", None, TranslationStatus::InProgress, Some("Complex logic".into()));
    tmap.record("send_response", None, TranslationStatus::NotStarted, None);
    tmap.record("log_error", None, TranslationStatus::Skipped, Some("Using tracing instead".into()));

    // Step 5: Check progress.
    let progress = tmap.progress();
    assert_eq!(progress.total, 5);
    assert_eq!(progress.verified, 1);
    assert_eq!(progress.ported, 1);
    assert_eq!(progress.in_progress, 1);
    assert_eq!(progress.not_started, 1);
    assert_eq!(progress.skipped, 1);
    // (1 verified + 1 ported + 1 skipped) / 5 = 60%
    assert!((progress.percent_complete - 60.0).abs() < 0.01);

    // Step 6: Check remaining.
    let remaining = tmap.remaining();
    assert_eq!(remaining.len(), 2);
    let remaining_names: Vec<_> = remaining.iter().map(|m| m.source_symbol.as_str()).collect();
    assert!(remaining_names.contains(&"process_request"));
    assert!(remaining_names.contains(&"send_response"));

    // Step 7: Compare a ported symbol.
    let cmp = mgr.compare(&ws, "parse_config").unwrap();
    assert!(cmp.contexts[0].found);
    assert!(cmp.contexts[1].found);
    assert!(!cmp.structural_diff.is_empty(), "C++ and Rust signatures differ");
}

// ============================================================================
// Anti-hallucination workflow
// ============================================================================

#[test]
fn test_anti_hallucination_workflow() {
    let graph = cpp_graph();
    let engine = GroundingEngine::new(&graph);

    // Claims about existing symbols → Verified.
    match engine.ground_claim("parse_config reads the configuration file") {
        GroundingResult::Verified { evidence, .. } => {
            assert!(evidence.iter().any(|e| e.name == "parse_config"));
        }
        other => panic!("Expected Verified, got {:?}", other),
    }

    match engine.ground_claim("validate_input checks the input before processing") {
        GroundingResult::Verified { evidence, .. } => {
            assert!(evidence.iter().any(|e| e.name == "validate_input"));
        }
        other => panic!("Expected Verified, got {:?}", other),
    }

    // Claims about non-existing symbols → Ungrounded.
    match engine.ground_claim("The encrypt_data function encrypts all user data") {
        GroundingResult::Ungrounded { suggestions, .. } => {
            // Suggestions should be somewhat helpful.
            assert!(suggestions.len() <= 10);
        }
        other => panic!("Expected Ungrounded for fabricated symbol, got {:?}", other),
    }

    // Mixed claim → Partial.
    match engine.ground_claim("process_request calls authenticate_user before handling") {
        GroundingResult::Partial { supported, unsupported, .. } => {
            assert!(supported.contains(&"process_request".to_string()));
            assert!(unsupported.contains(&"authenticate_user".to_string()));
        }
        other => panic!("Expected Partial, got {:?}", other),
    }
}

// ============================================================================
// Grounding within workspace contexts
// ============================================================================

#[test]
fn test_grounding_across_workspace_lifecycle() {
    let mut mgr = WorkspaceManager::new();
    let ws = mgr.create("grounding-ws");

    // Start with partial Rust codebase.
    let rust_partial = rust_graph_partial();
    let _ctx = mgr
        .add_context(&ws, "/src/rust", ContextRole::Target, Some("Rust".into()), rust_partial.clone())
        .unwrap();

    // Ground against the partial target — parse_config exists.
    let engine = GroundingEngine::new(&rust_partial);

    match engine.ground_claim("parse_config is implemented in Rust") {
        GroundingResult::Verified { .. } => {}
        other => panic!("Expected Verified, got {:?}", other),
    }

    // send_response does NOT exist yet → Ungrounded.
    match engine.ground_claim("send_response handles HTTP responses") {
        GroundingResult::Ungrounded { .. } => {}
        other => panic!("Expected Ungrounded for not-yet-ported function, got {:?}", other),
    }

    // After completing migration, ground against complete graph.
    let rust_complete = rust_graph_complete();
    let engine_complete = GroundingEngine::new(&rust_complete);

    // Now send_response exists → Verified.
    match engine_complete.ground_claim("send_response handles HTTP responses") {
        GroundingResult::Verified { evidence, .. } => {
            assert!(evidence.iter().any(|e| e.name == "send_response"));
        }
        other => panic!("Expected Verified after migration, got {:?}", other),
    }
}

// ============================================================================
// Translation map edge cases
// ============================================================================

#[test]
fn test_translation_update_existing() {
    let mut tmap = TranslationMap::new("src".into(), "tgt".into());
    tmap.record("foo", None, TranslationStatus::NotStarted, None);
    tmap.record("foo", Some("foo_rs"), TranslationStatus::Ported, Some("Done".into()));

    let progress = tmap.progress();
    assert_eq!(progress.total, 1, "Should update, not duplicate");
    assert_eq!(progress.ported, 1);

    let m = tmap.status("foo").unwrap();
    assert_eq!(m.target_symbol.as_deref(), Some("foo_rs"));
    assert_eq!(m.notes.as_deref(), Some("Done"));
}

#[test]
fn test_translation_progress_empty() {
    let tmap = TranslationMap::new("a".into(), "b".into());
    let p = tmap.progress();
    assert_eq!(p.total, 0);
    assert!((p.percent_complete - 0.0).abs() < 0.01);
}

#[test]
fn test_translation_all_statuses() {
    let mut tmap = TranslationMap::new("a".into(), "b".into());
    tmap.record("s1", None, TranslationStatus::NotStarted, None);
    tmap.record("s2", None, TranslationStatus::InProgress, None);
    tmap.record("s3", Some("t3"), TranslationStatus::Ported, None);
    tmap.record("s4", Some("t4"), TranslationStatus::Verified, None);
    tmap.record("s5", None, TranslationStatus::Skipped, Some("Not needed".into()));

    let p = tmap.progress();
    assert_eq!(p.total, 5);
    assert_eq!(p.not_started, 1);
    assert_eq!(p.in_progress, 1);
    assert_eq!(p.ported, 1);
    assert_eq!(p.verified, 1);
    assert_eq!(p.skipped, 1);
    assert!((p.percent_complete - 60.0).abs() < 0.01);

    let remaining = tmap.remaining();
    assert_eq!(remaining.len(), 2);
    let completed = tmap.completed();
    assert_eq!(completed.len(), 3);
}

#[test]
fn test_translation_status_roundtrip() {
    for label in &["not_started", "in_progress", "ported", "verified", "skipped"] {
        let status = TranslationStatus::from_str(label).unwrap();
        assert_eq!(status.label(), *label);
    }
    // Hyphenated forms.
    assert_eq!(TranslationStatus::from_str("not-started"), Some(TranslationStatus::NotStarted));
    assert_eq!(TranslationStatus::from_str("in-progress"), Some(TranslationStatus::InProgress));
    assert!(TranslationStatus::from_str("invalid").is_none());
}
