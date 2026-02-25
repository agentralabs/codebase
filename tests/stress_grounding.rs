//! Grounding stress tests — verifies anti-hallucination across many scenarios.

use std::path::PathBuf;

use agentic_codebase::graph::CodeGraph;
use agentic_codebase::grounding::{Grounded, GroundingEngine, GroundingResult};
use agentic_codebase::types::{CodeUnit, CodeUnitType, Language, Span};

// ─────────────────────── helpers ───────────────────────

fn test_graph() -> CodeGraph {
    let mut graph = CodeGraph::with_default_dimension();

    let units = vec![
        ("process_payment", CodeUnitType::Function, Language::Python, "payments.stripe.process_payment", "src/payments/stripe.py"),
        ("CodeGraph", CodeUnitType::Type, Language::Rust, "crate::graph::CodeGraph", "src/graph/code_graph.rs"),
        ("add_unit", CodeUnitType::Function, Language::Rust, "crate::graph::CodeGraph::add_unit", "src/graph/code_graph.rs"),
        ("MAX_EDGES_PER_UNIT", CodeUnitType::Config, Language::Rust, "crate::types::MAX_EDGES_PER_UNIT", "src/types/mod.rs"),
        ("validate_amount", CodeUnitType::Function, Language::Python, "payments.utils.validate_amount", "src/payments/utils.py"),
        ("UserProfile", CodeUnitType::Type, Language::TypeScript, "models.UserProfile", "src/models/user.ts"),
        ("parse_config", CodeUnitType::Function, Language::Rust, "crate::config::parse_config", "src/config/loader.rs"),
        ("DatabaseConnection", CodeUnitType::Type, Language::Rust, "crate::db::DatabaseConnection", "src/db/connection.rs"),
        ("run_migration", CodeUnitType::Function, Language::Rust, "crate::db::run_migration", "src/db/migration.rs"),
        ("API_VERSION", CodeUnitType::Config, Language::Rust, "crate::API_VERSION", "src/lib.rs"),
    ];

    for (name, utype, lang, qname, fpath) in units {
        graph.add_unit(CodeUnit::new(
            utype,
            lang,
            name.to_string(),
            qname.to_string(),
            PathBuf::from(fpath),
            Span::new(1, 0, 50, 0),
        ));
    }

    graph
}

/// Build a large graph with `n` symbols for scale testing.
fn scale_graph(n: usize) -> CodeGraph {
    let mut graph = CodeGraph::with_default_dimension();
    for i in 0..n {
        graph.add_unit(CodeUnit::new(
            CodeUnitType::Function,
            Language::Rust,
            format!("function_{}", i),
            format!("crate::mod_{0}::function_{0}", i),
            PathBuf::from(format!("src/mod_{}.rs", i)),
            Span::new(1, 0, 10, 0),
        ));
    }
    graph
}

// ============================================================================
// Verified claims
// ============================================================================

#[test]
fn test_grounding_verified_function() {
    let graph = test_graph();
    let engine = GroundingEngine::new(&graph);
    match engine.ground_claim("The process_payment function handles Stripe payments") {
        GroundingResult::Verified { evidence, confidence } => {
            assert!(!evidence.is_empty());
            assert!(confidence > 0.0);
            assert_eq!(evidence[0].name, "process_payment");
        }
        other => panic!("Expected Verified, got {:?}", other),
    }
}

#[test]
fn test_grounding_verified_struct() {
    let graph = test_graph();
    let engine = GroundingEngine::new(&graph);
    match engine.ground_claim("CodeGraph stores all the parsed code units") {
        GroundingResult::Verified { evidence, .. } => {
            assert!(evidence.iter().any(|e| e.name == "CodeGraph"));
        }
        other => panic!("Expected Verified, got {:?}", other),
    }
}

#[test]
fn test_grounding_verified_module() {
    let graph = test_graph();
    let engine = GroundingEngine::new(&graph);
    match engine.ground_claim("parse_config loads configuration from disk") {
        GroundingResult::Verified { evidence, .. } => {
            assert!(evidence.iter().any(|e| e.name == "parse_config"));
        }
        other => panic!("Expected Verified, got {:?}", other),
    }
}

#[test]
fn test_grounding_verified_multiple_refs() {
    let graph = test_graph();
    let engine = GroundingEngine::new(&graph);
    match engine.ground_claim("add_unit works on the CodeGraph struct") {
        GroundingResult::Verified { evidence, .. } => {
            assert!(evidence.len() >= 2, "Should find evidence for both add_unit and CodeGraph");
        }
        other => panic!("Expected Verified, got {:?}", other),
    }
}

// ============================================================================
// Ungrounded claims (hallucinations)
// ============================================================================

#[test]
fn test_grounding_ungrounded_typo() {
    let graph = test_graph();
    let engine = GroundingEngine::new(&graph);
    match engine.ground_claim("The send_invoice function emails invoices") {
        GroundingResult::Ungrounded { .. } => {}
        other => panic!("Expected Ungrounded, got {:?}", other),
    }
}

#[test]
fn test_grounding_ungrounded_hallucination() {
    let graph = test_graph();
    let engine = GroundingEngine::new(&graph);
    match engine.ground_claim("The deploy_kubernetes function orchestrates containers") {
        GroundingResult::Ungrounded { suggestions, .. } => {
            // Should not crash even with no matching suggestions
            assert!(suggestions.len() <= 10);
        }
        other => panic!("Expected Ungrounded, got {:?}", other),
    }
}

#[test]
fn test_grounding_ungrounded_complete_fabrication() {
    let graph = test_graph();
    let engine = GroundingEngine::new(&graph);
    match engine.ground_claim("quantum_entangle teleports data via SpookyAction") {
        GroundingResult::Ungrounded { .. } => {}
        other => panic!("Expected Ungrounded, got {:?}", other),
    }
}

// ============================================================================
// Partial claims (some exist, some don't)
// ============================================================================

#[test]
fn test_grounding_partial_mixed() {
    let graph = test_graph();
    let engine = GroundingEngine::new(&graph);
    match engine.ground_claim("process_payment calls send_notification after success") {
        GroundingResult::Partial { supported, unsupported, .. } => {
            assert!(supported.contains(&"process_payment".to_string()));
            assert!(unsupported.contains(&"send_notification".to_string()));
        }
        other => panic!("Expected Partial, got {:?}", other),
    }
}

#[test]
fn test_grounding_partial_two_real_one_fake() {
    let graph = test_graph();
    let engine = GroundingEngine::new(&graph);
    match engine.ground_claim("CodeGraph uses add_unit and remove_unit methods") {
        GroundingResult::Partial { supported, unsupported, .. } => {
            assert!(supported.contains(&"CodeGraph".to_string()));
            assert!(supported.contains(&"add_unit".to_string()));
            assert!(unsupported.contains(&"remove_unit".to_string()));
        }
        other => panic!("Expected Partial, got {:?}", other),
    }
}

// ============================================================================
// Fuzzy suggestions quality
// ============================================================================

#[test]
fn test_grounding_suggestions_quality() {
    let graph = test_graph();
    let engine = GroundingEngine::new(&graph);
    let suggestions = engine.suggest_similar("process_paymnt", 5);
    assert!(
        suggestions.contains(&"process_payment".to_string()),
        "Expected 'process_payment' in suggestions for typo 'process_paymnt': {:?}",
        suggestions
    );
}

#[test]
fn test_grounding_suggestions_for_close_typo() {
    let graph = test_graph();
    let engine = GroundingEngine::new(&graph);
    let suggestions = engine.suggest_similar("validate_amout", 5);
    assert!(
        suggestions.contains(&"validate_amount".to_string()),
        "Expected 'validate_amount' in suggestions: {:?}",
        suggestions
    );
}

#[test]
fn test_grounding_suggestions_prefix_match() {
    let graph = test_graph();
    let engine = GroundingEngine::new(&graph);
    let suggestions = engine.suggest_similar("parse", 5);
    assert!(
        suggestions.contains(&"parse_config".to_string()),
        "Expected 'parse_config' for prefix 'parse': {:?}",
        suggestions
    );
}

// ============================================================================
// Scale tests
// ============================================================================

#[test]
fn test_grounding_scale_10k_symbols() {
    let graph = scale_graph(10_000);
    let engine = GroundingEngine::new(&graph);

    let start = std::time::Instant::now();
    let result = engine.ground_claim("The function_5000 processes data efficiently");
    let elapsed = start.elapsed();

    match result {
        GroundingResult::Verified { evidence, .. } => {
            assert!(evidence.iter().any(|e| e.name == "function_5000"));
        }
        other => panic!("Expected Verified for function_5000 in 10K graph, got {:?}", other),
    }

    // Should be fast — under 100ms even for 10K symbols.
    assert!(
        elapsed.as_millis() < 100,
        "Grounding 10K symbols took {}ms, expected < 100ms",
        elapsed.as_millis()
    );
}

#[test]
fn test_grounding_scale_suggestions_10k() {
    let graph = scale_graph(10_000);
    let engine = GroundingEngine::new(&graph);

    let start = std::time::Instant::now();
    let suggestions = engine.suggest_similar("function_999", 5);
    let elapsed = start.elapsed();

    assert!(!suggestions.is_empty());
    // In debug mode, levenshtein over 10K symbols can be slower.
    assert!(
        elapsed.as_millis() < 2000,
        "Suggestions in 10K graph took {}ms, expected < 2000ms",
        elapsed.as_millis()
    );
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn test_grounding_empty_claim() {
    let graph = test_graph();
    let engine = GroundingEngine::new(&graph);
    let result = engine.ground_claim("");
    assert!(matches!(result, GroundingResult::Ungrounded { .. }));
}

#[test]
fn test_grounding_long_claim() {
    let graph = test_graph();
    let engine = GroundingEngine::new(&graph);
    let long_claim = "word ".repeat(1000) + "process_payment";
    let result = engine.ground_claim(&long_claim);
    match result {
        GroundingResult::Verified { evidence, .. } => {
            assert!(evidence.iter().any(|e| e.name == "process_payment"));
        }
        other => panic!("Expected Verified even in long claim, got {:?}", other),
    }
}

#[test]
fn test_grounding_special_chars() {
    let graph = test_graph();
    let engine = GroundingEngine::new(&graph);
    let result = engine.ground_claim("!@#$%^&*() process_payment ()!@#$");
    match result {
        GroundingResult::Verified { evidence, .. } => {
            assert!(evidence.iter().any(|e| e.name == "process_payment"));
        }
        other => panic!("Expected Verified through special chars, got {:?}", other),
    }
}

#[test]
fn test_grounding_unicode_symbols() {
    let graph = test_graph();
    let engine = GroundingEngine::new(&graph);
    // Unicode surrounding real code refs should still work.
    let result = engine.ground_claim("处理 process_payment 函数 validates amounts");
    match result {
        GroundingResult::Verified { evidence, .. } => {
            assert!(evidence.iter().any(|e| e.name == "process_payment"));
        }
        other => panic!("Expected Verified with unicode context, got {:?}", other),
    }
}

#[test]
fn test_grounding_backtick_extraction() {
    let graph = test_graph();
    let engine = GroundingEngine::new(&graph);
    match engine.ground_claim("Use `add_unit` to insert nodes into `CodeGraph`") {
        GroundingResult::Verified { evidence, .. } => {
            assert!(evidence.len() >= 2);
        }
        other => panic!("Expected Verified with backtick refs, got {:?}", other),
    }
}

#[test]
fn test_grounding_no_refs_is_ungrounded() {
    let graph = test_graph();
    let engine = GroundingEngine::new(&graph);
    let result = engine.ground_claim("This is a normal English sentence about nothing.");
    assert!(matches!(result, GroundingResult::Ungrounded { .. }));
}

#[test]
fn test_grounding_case_insensitive_evidence() {
    let graph = test_graph();
    let engine = GroundingEngine::new(&graph);
    let evidence = engine.find_evidence("codegraph");
    assert!(!evidence.is_empty());
    assert_eq!(evidence[0].name, "CodeGraph");
}
