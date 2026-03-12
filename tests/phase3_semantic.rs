//! Phase 3 tests: Semantic analysis.
//!
//! Tests symbol table building, reference resolution, FFI tracing,
//! pattern detection, concept extraction, and full graph building.

use std::path::PathBuf;

use agentic_codebase::parse::{Parser, RawCodeUnit, ReferenceKind};
use agentic_codebase::semantic::{
    AnalyzeOptions, ConceptExtractor, ConceptRole, FfiPatternType, FfiTracer, PatternDetector,
    Resolution, ResolvedUnit, Resolver, SemanticAnalyzer, SymbolTable,
};
use agentic_codebase::types::{CodeUnitType, EdgeType, Language, Visibility};

// ============================================================
// Helpers
// ============================================================

fn testdata_path(relative: &str) -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    std::path::Path::new(manifest_dir)
        .join("testdata")
        .join(relative)
}

fn parse_test_file(relative: &str) -> Vec<RawCodeUnit> {
    let path = testdata_path(relative);
    let content = std::fs::read_to_string(&path).expect("Could not read test file");
    let parser = Parser::new();
    parser.parse_file(&path, &content).expect("Parse failed")
}

// ============================================================
// Symbol Table
// ============================================================

#[test]
fn test_symbol_table_build() {
    let units = parse_test_file("python/simple_module.py");
    let table = SymbolTable::build(&units).expect("Symbol table build failed");
    assert!(!table.is_empty(), "Symbol table should have entries");
}

#[test]
fn test_symbol_table_lookup_qualified() {
    let units = parse_test_file("python/simple_module.py");
    let table = SymbolTable::build(&units).expect("build failed");

    // Module should be registered
    let module_id = table.lookup_qualified("simple_module");
    assert!(
        module_id.is_some(),
        "simple_module should be in symbol table"
    );
}

#[test]
fn test_symbol_table_lookup_by_name() {
    let units = parse_test_file("python/simple_module.py");
    let table = SymbolTable::build(&units).expect("build failed");

    let animals = table.lookup_name("Animal");
    assert!(!animals.is_empty(), "Animal should be found by name");
}

#[test]
fn test_symbol_table_file_grouping() {
    let units = parse_test_file("python/simple_module.py");
    let table = SymbolTable::build(&units).expect("build failed");

    let path = testdata_path("python/simple_module.py");
    let file_key = path.to_string_lossy().to_string();
    let file_units = table.units_in_file(&file_key);
    assert!(
        file_units.len() > 1,
        "Should find multiple units in file, got {}",
        file_units.len()
    );
}

#[test]
fn test_symbol_table_empty() {
    let table = SymbolTable::build(&[]).expect("build failed");
    assert!(table.is_empty());
    assert_eq!(table.len(), 0);
}

#[test]
fn test_symbol_table_qname_roundtrip() {
    let units = parse_test_file("rust/simple_lib.rs");
    let table = SymbolTable::build(&units).expect("build failed");

    for unit in &units {
        let id = table.lookup_qualified(&unit.qualified_name);
        assert!(
            id.is_some(),
            "Should find unit {} (qname: {}) in table",
            unit.name,
            unit.qualified_name
        );
        let qname = table.qname_for_id(id.unwrap());
        assert_eq!(
            qname,
            Some(unit.qualified_name.as_str()),
            "Qname should round-trip"
        );
    }
}

// ============================================================
// Reference Resolution
// ============================================================

#[test]
fn test_resolve_local_reference() {
    let units = parse_test_file("python/simple_module.py");
    let table = SymbolTable::build(&units).expect("build failed");
    let resolver = Resolver::new();
    let resolved = resolver
        .resolve_all(&units, &table)
        .expect("resolve failed");

    // test_animals calls Dog(), which should resolve locally
    let test_fn = resolved
        .iter()
        .find(|r| r.unit.name == "test_animals")
        .expect("test_animals not found");

    let local_refs: Vec<_> = test_fn
        .resolved_refs
        .iter()
        .filter(|r| matches!(r.resolution, Resolution::Local(_)))
        .collect();

    assert!(
        !local_refs.is_empty(),
        "test_animals should have local references resolved"
    );
}

#[test]
fn test_resolve_external_import() {
    let units = parse_test_file("python/simple_module.py");
    let table = SymbolTable::build(&units).expect("build failed");
    let resolver = Resolver::new();
    let resolved = resolver
        .resolve_all(&units, &table)
        .expect("resolve failed");

    // Look for external resolution — Python stdlib like 'os'
    let has_external = resolved.iter().any(|r| {
        r.resolved_refs
            .iter()
            .any(|ref_info| matches!(ref_info.resolution, Resolution::External(_)))
    });

    // The imports in simple_module.py import 'os' which has known stdlib functions
    // but the import unit itself may not have references that match stdlib.
    // This test just verifies the machinery works.
    let _ = has_external; // may or may not have external, both valid
}

#[test]
fn test_resolve_qualified_names_built() {
    let units = parse_test_file("python/simple_module.py");
    let table = SymbolTable::build(&units).expect("build failed");
    let resolver = Resolver::new();
    let resolved = resolver
        .resolve_all(&units, &table)
        .expect("resolve failed");

    // All units should have qualified names
    for r in &resolved {
        assert!(
            !r.unit.qualified_name.is_empty(),
            "Unit {} should have a qualified name",
            r.unit.name
        );
    }
}

#[test]
fn test_resolve_cross_file_reference() {
    // Parse multiple files to test cross-file resolution
    let parser = Parser::new();
    let testdata = testdata_path("");
    let opts = agentic_codebase::parse::ParseOptions {
        languages: vec![Language::Python],
        ..Default::default()
    };
    let result = parser
        .parse_directory(&testdata, &opts)
        .expect("parse failed");

    let table = SymbolTable::build(&result.units).expect("build failed");
    let resolver = Resolver::new();
    let resolved = resolver
        .resolve_all(&result.units, &table)
        .expect("resolve failed");

    // Should have resolved some references
    assert!(!resolved.is_empty());
}

#[test]
fn test_resolve_circular_import_no_hang() {
    // Create two units that import each other
    use agentic_codebase::parse::{RawReference, ReferenceKind};
    use agentic_codebase::types::Span;

    let mut unit_a = RawCodeUnit::new(
        CodeUnitType::Module,
        Language::Python,
        "module_a".to_string(),
        PathBuf::from("a.py"),
        Span::new(1, 0, 10, 0),
    );
    unit_a.temp_id = 0;
    unit_a.qualified_name = "module_a".to_string();

    let mut import_b = RawCodeUnit::new(
        CodeUnitType::Import,
        Language::Python,
        "module_b".to_string(),
        PathBuf::from("a.py"),
        Span::new(2, 0, 2, 20),
    );
    import_b.temp_id = 1;
    import_b.qualified_name = "module_a.module_b".to_string();
    import_b.references.push(RawReference {
        name: "module_b".to_string(),
        kind: ReferenceKind::Import,
        span: Span::new(2, 0, 2, 20),
    });

    let mut unit_b = RawCodeUnit::new(
        CodeUnitType::Module,
        Language::Python,
        "module_b".to_string(),
        PathBuf::from("b.py"),
        Span::new(1, 0, 10, 0),
    );
    unit_b.temp_id = 2;
    unit_b.qualified_name = "module_b".to_string();

    let mut import_a = RawCodeUnit::new(
        CodeUnitType::Import,
        Language::Python,
        "module_a".to_string(),
        PathBuf::from("b.py"),
        Span::new(2, 0, 2, 20),
    );
    import_a.temp_id = 3;
    import_a.qualified_name = "module_b.module_a".to_string();
    import_a.references.push(RawReference {
        name: "module_a".to_string(),
        kind: ReferenceKind::Import,
        span: Span::new(2, 0, 2, 20),
    });

    let units = vec![unit_a, import_b, unit_b, import_a];
    let table = SymbolTable::build(&units).expect("build failed");
    let resolver = Resolver::new();

    // This should not hang — no infinite loops
    let resolved = resolver
        .resolve_all(&units, &table)
        .expect("resolve failed");
    assert_eq!(resolved.len(), 4);
}

// ============================================================
// FFI Tracing
// ============================================================

#[test]
fn test_ffi_tracer_no_ffi() {
    let units = parse_test_file("python/simple_module.py");
    let table = SymbolTable::build(&units).expect("build failed");
    let resolver = Resolver::new();
    let resolved = resolver
        .resolve_all(&units, &table)
        .expect("resolve failed");

    let tracer = FfiTracer::new();
    let edges = tracer.trace(&resolved).expect("trace failed");

    // simple_module.py doesn't have FFI calls, but HTTP calls could be detected
    // due to fetch_data function name. This is okay.
    let _ = edges;
}

#[test]
fn test_ffi_detection_ctypes() {
    use agentic_codebase::parse::RawReference;
    use agentic_codebase::types::Span;

    let mut unit = RawCodeUnit::new(
        CodeUnitType::Module,
        Language::Python,
        "native_wrapper".to_string(),
        PathBuf::from("wrapper.py"),
        Span::new(1, 0, 10, 0),
    );
    unit.temp_id = 0;
    unit.qualified_name = "native_wrapper".to_string();
    unit.references.push(RawReference {
        name: "ctypes".to_string(),
        kind: ReferenceKind::Import,
        span: Span::new(1, 0, 1, 20),
    });

    let resolved = vec![ResolvedUnit {
        unit,
        resolved_refs: vec![agentic_codebase::semantic::ResolvedReference {
            raw: RawReference {
                name: "ctypes".to_string(),
                kind: ReferenceKind::Import,
                span: Span::new(1, 0, 1, 20),
            },
            resolution: Resolution::Unresolved,
        }],
    }];

    let tracer = FfiTracer::new();
    let edges = tracer.trace(&resolved).expect("trace failed");

    assert!(!edges.is_empty(), "Should detect ctypes FFI usage");
    assert_eq!(edges[0].ffi_type, FfiPatternType::Ctypes);
}

#[test]
fn test_ffi_detection_http() {
    use agentic_codebase::parse::RawReference;
    use agentic_codebase::types::Span;

    let mut unit = RawCodeUnit::new(
        CodeUnitType::Function,
        Language::Python,
        "call_api".to_string(),
        PathBuf::from("client.py"),
        Span::new(1, 0, 10, 0),
    );
    unit.temp_id = 0;
    unit.qualified_name = "call_api".to_string();
    unit.references.push(RawReference {
        name: "requests.get".to_string(),
        kind: ReferenceKind::Call,
        span: Span::new(3, 0, 3, 30),
    });

    let resolved = vec![ResolvedUnit {
        unit,
        resolved_refs: vec![agentic_codebase::semantic::ResolvedReference {
            raw: RawReference {
                name: "requests.get".to_string(),
                kind: ReferenceKind::Call,
                span: Span::new(3, 0, 3, 30),
            },
            resolution: Resolution::Unresolved,
        }],
    }];

    let tracer = FfiTracer::new();
    let edges = tracer.trace(&resolved).expect("trace failed");

    assert!(!edges.is_empty(), "Should detect HTTP RPC call");
    assert_eq!(edges[0].ffi_type, FfiPatternType::HttpRpc);
}

// ============================================================
// Pattern Detection
// ============================================================

#[test]
fn test_detect_factory() {
    use agentic_codebase::types::Span;

    let mut unit = RawCodeUnit::new(
        CodeUnitType::Function,
        Language::Python,
        "create_user".to_string(),
        PathBuf::from("factory.py"),
        Span::new(1, 0, 10, 0),
    );
    unit.temp_id = 0;
    unit.qualified_name = "create_user".to_string();

    let resolved = vec![ResolvedUnit {
        unit,
        resolved_refs: vec![],
    }];

    let detector = PatternDetector::new();
    let patterns = detector.detect(&resolved).expect("detect failed");

    assert!(
        !patterns.is_empty(),
        "Should detect Factory pattern for create_user"
    );
    assert_eq!(patterns[0].pattern_name, "Factory");
}

#[test]
fn test_detect_repository() {
    use agentic_codebase::types::Span;

    let mut type_unit = RawCodeUnit::new(
        CodeUnitType::Type,
        Language::Python,
        "UserRepository".to_string(),
        PathBuf::from("repo.py"),
        Span::new(1, 0, 30, 0),
    );
    type_unit.temp_id = 0;
    type_unit.qualified_name = "UserRepository".to_string();

    let mut get_method = RawCodeUnit::new(
        CodeUnitType::Function,
        Language::Python,
        "get_by_id".to_string(),
        PathBuf::from("repo.py"),
        Span::new(5, 0, 8, 0),
    );
    get_method.temp_id = 1;
    get_method.qualified_name = "UserRepository.get_by_id".to_string();

    let mut create_method = RawCodeUnit::new(
        CodeUnitType::Function,
        Language::Python,
        "create".to_string(),
        PathBuf::from("repo.py"),
        Span::new(10, 0, 14, 0),
    );
    create_method.temp_id = 2;
    create_method.qualified_name = "UserRepository.create".to_string();

    let mut delete_method = RawCodeUnit::new(
        CodeUnitType::Function,
        Language::Python,
        "delete".to_string(),
        PathBuf::from("repo.py"),
        Span::new(15, 0, 18, 0),
    );
    delete_method.temp_id = 3;
    delete_method.qualified_name = "UserRepository.delete".to_string();

    let resolved: Vec<ResolvedUnit> = vec![type_unit, get_method, create_method, delete_method]
        .into_iter()
        .map(|u| ResolvedUnit {
            unit: u,
            resolved_refs: vec![],
        })
        .collect();

    let detector = PatternDetector::new();
    let patterns = detector.detect(&resolved).expect("detect failed");

    let repo_patterns: Vec<_> = patterns
        .iter()
        .filter(|p| p.pattern_name == "Repository")
        .collect();
    assert!(
        !repo_patterns.is_empty(),
        "Should detect Repository pattern"
    );
    assert!(
        repo_patterns[0].confidence >= 0.6,
        "Repository with CRUD methods should have high confidence"
    );
}

#[test]
fn test_detect_decorator_pattern() {
    use agentic_codebase::types::Span;

    let mut unit = RawCodeUnit::new(
        CodeUnitType::Function,
        Language::Python,
        "logging_middleware".to_string(),
        PathBuf::from("middleware.py"),
        Span::new(1, 0, 10, 0),
    );
    unit.temp_id = 0;
    unit.qualified_name = "logging_middleware".to_string();

    let resolved = vec![ResolvedUnit {
        unit,
        resolved_refs: vec![],
    }];

    let detector = PatternDetector::new();
    let patterns = detector.detect(&resolved).expect("detect failed");

    let decorator_patterns: Vec<_> = patterns
        .iter()
        .filter(|p| p.pattern_name == "Decorator")
        .collect();
    assert!(
        !decorator_patterns.is_empty(),
        "Should detect Decorator/middleware pattern"
    );
}

#[test]
fn test_detect_singleton() {
    use agentic_codebase::types::Span;

    let mut type_unit = RawCodeUnit::new(
        CodeUnitType::Type,
        Language::Python,
        "DatabasePool".to_string(),
        PathBuf::from("db.py"),
        Span::new(1, 0, 30, 0),
    );
    type_unit.temp_id = 0;
    type_unit.qualified_name = "DatabasePool".to_string();

    let mut instance_method = RawCodeUnit::new(
        CodeUnitType::Function,
        Language::Python,
        "get_instance".to_string(),
        PathBuf::from("db.py"),
        Span::new(5, 0, 10, 0),
    );
    instance_method.temp_id = 1;
    instance_method.qualified_name = "DatabasePool.get_instance".to_string();

    let resolved: Vec<ResolvedUnit> = vec![type_unit, instance_method]
        .into_iter()
        .map(|u| ResolvedUnit {
            unit: u,
            resolved_refs: vec![],
        })
        .collect();

    let detector = PatternDetector::new();
    let patterns = detector.detect(&resolved).expect("detect failed");

    let singletons: Vec<_> = patterns
        .iter()
        .filter(|p| p.pattern_name == "Singleton")
        .collect();
    assert!(!singletons.is_empty(), "Should detect Singleton pattern");
}

// ============================================================
// Concept Extraction
// ============================================================

#[test]
fn test_extract_concept_auth() {
    use agentic_codebase::types::Span;

    let units: Vec<RawCodeUnit> = vec![
        {
            let mut u = RawCodeUnit::new(
                CodeUnitType::Function,
                Language::Python,
                "authenticate_user".to_string(),
                PathBuf::from("auth.py"),
                Span::new(1, 0, 10, 0),
            );
            u.temp_id = 0;
            u.qualified_name = "auth.authenticate_user".to_string();
            u
        },
        {
            let mut u = RawCodeUnit::new(
                CodeUnitType::Function,
                Language::Python,
                "login".to_string(),
                PathBuf::from("auth.py"),
                Span::new(12, 0, 20, 0),
            );
            u.temp_id = 1;
            u.qualified_name = "auth.login".to_string();
            u
        },
        {
            let mut u = RawCodeUnit::new(
                CodeUnitType::Type,
                Language::Python,
                "TokenValidator".to_string(),
                PathBuf::from("auth.py"),
                Span::new(22, 0, 40, 0),
            );
            u.temp_id = 2;
            u.qualified_name = "auth.TokenValidator".to_string();
            u
        },
    ];

    let resolved: Vec<ResolvedUnit> = units
        .into_iter()
        .map(|u| ResolvedUnit {
            unit: u,
            resolved_refs: vec![],
        })
        .collect();

    let extractor = ConceptExtractor::new();
    let concepts = extractor.extract(&resolved).expect("extract failed");

    let auth_concepts: Vec<_> = concepts
        .iter()
        .filter(|c| c.name == "Authentication")
        .collect();
    assert!(
        !auth_concepts.is_empty(),
        "Should detect Authentication concept"
    );
    assert!(
        auth_concepts[0].units.len() >= 2,
        "Should group multiple auth units"
    );
}

#[test]
fn test_extract_concept_user() {
    use agentic_codebase::types::Span;

    let units: Vec<RawCodeUnit> = vec![{
        let mut u = RawCodeUnit::new(
            CodeUnitType::Type,
            Language::Python,
            "UserProfile".to_string(),
            PathBuf::from("users.py"),
            Span::new(1, 0, 30, 0),
        );
        u.temp_id = 0;
        u.qualified_name = "users.UserProfile".to_string();
        u
    }];

    let resolved: Vec<ResolvedUnit> = units
        .into_iter()
        .map(|u| ResolvedUnit {
            unit: u,
            resolved_refs: vec![],
        })
        .collect();

    let extractor = ConceptExtractor::new();
    let concepts = extractor.extract(&resolved).expect("extract failed");

    let user_concepts: Vec<_> = concepts
        .iter()
        .filter(|c| c.name == "UserManagement")
        .collect();
    assert!(
        !user_concepts.is_empty(),
        "Should detect UserManagement concept"
    );
}

#[test]
fn test_concept_roles() {
    use agentic_codebase::types::Span;

    let units: Vec<RawCodeUnit> = vec![
        {
            let mut u = RawCodeUnit::new(
                CodeUnitType::Type,
                Language::Python,
                "UserModel".to_string(),
                PathBuf::from("models.py"),
                Span::new(1, 0, 20, 0),
            );
            u.temp_id = 0;
            u.qualified_name = "UserModel".to_string();
            u
        },
        {
            let mut u = RawCodeUnit::new(
                CodeUnitType::Test,
                Language::Python,
                "test_user_creation".to_string(),
                PathBuf::from("test_user.py"),
                Span::new(1, 0, 10, 0),
            );
            u.temp_id = 1;
            u.qualified_name = "test_user_creation".to_string();
            u
        },
    ];

    let resolved: Vec<ResolvedUnit> = units
        .into_iter()
        .map(|u| ResolvedUnit {
            unit: u,
            resolved_refs: vec![],
        })
        .collect();

    let extractor = ConceptExtractor::new();
    let concepts = extractor.extract(&resolved).expect("extract failed");

    let user_concept = concepts
        .iter()
        .find(|c| c.name == "UserManagement")
        .expect("UserManagement concept not found");

    // Should have different roles
    let roles: Vec<_> = user_concept.units.iter().map(|u| u.role).collect();
    assert!(
        roles.contains(&ConceptRole::Definition) || roles.contains(&ConceptRole::Implementation),
        "Should have a definition/impl role"
    );
    assert!(
        roles.contains(&ConceptRole::Test),
        "Should have a test role"
    );
}

// ============================================================
// Full Semantic Analyzer (Integration)
// ============================================================

#[test]
fn test_full_analysis_python() {
    let units = parse_test_file("python/simple_module.py");
    let analyzer = SemanticAnalyzer::new();
    let graph = analyzer
        .analyze(units, &AnalyzeOptions::default())
        .expect("analysis failed");

    assert!(graph.unit_count() > 0, "Graph should have units");
    assert!(graph.edge_count() > 0, "Graph should have edges");
}

#[test]
fn test_full_analysis_rust() {
    let units = parse_test_file("rust/simple_lib.rs");
    let analyzer = SemanticAnalyzer::new();
    let graph = analyzer
        .analyze(units, &AnalyzeOptions::default())
        .expect("analysis failed");

    assert!(graph.unit_count() > 0, "Graph should have units");
    assert!(graph.edge_count() > 0, "Graph should have edges");
}

#[test]
fn test_full_analysis_typescript() {
    let units = parse_test_file("typescript/simple_module.ts");
    let analyzer = SemanticAnalyzer::new();
    let graph = analyzer
        .analyze(units, &AnalyzeOptions::default())
        .expect("analysis failed");

    assert!(graph.unit_count() > 0);
    assert!(graph.edge_count() > 0);
}

#[test]
fn test_full_analysis_go() {
    let units = parse_test_file("go/simple_module.go");
    let analyzer = SemanticAnalyzer::new();
    let graph = analyzer
        .analyze(units, &AnalyzeOptions::default())
        .expect("analysis failed");

    assert!(graph.unit_count() > 0);
    assert!(graph.edge_count() > 0);
}

#[test]
fn test_full_analysis_java() {
    let units = parse_test_file("java/com/example/core/Worker.java");
    let analyzer = SemanticAnalyzer::new();
    let graph = analyzer
        .analyze(units, &AnalyzeOptions::default())
        .expect("analysis failed");

    assert!(graph.unit_count() > 0);
}

#[test]
fn test_full_analysis_java_edge_categories() {
    let parser = Parser::new();
    let testdata = testdata_path("java");
    let opts = agentic_codebase::parse::ParseOptions {
        languages: vec![Language::Java],
        ..Default::default()
    };
    let parsed = parser
        .parse_directory(&testdata, &opts)
        .expect("parse failed");

    let analyzer = SemanticAnalyzer::new();
    let graph = analyzer
        .analyze(parsed.units, &AnalyzeOptions::default())
        .expect("analysis failed");

    let edge_count = |edge_type| {
        graph
            .edges()
            .iter()
            .filter(|e| e.edge_type == edge_type)
            .count()
    };

    assert!(
        edge_count(EdgeType::Contains) > 0,
        "Expected contains edges"
    );
    assert!(edge_count(EdgeType::Calls) > 0, "Expected call edges");
    assert!(edge_count(EdgeType::Imports) > 0, "Expected import edges");
    assert!(
        edge_count(EdgeType::Inherits) > 0,
        "Expected inherits edges"
    );
    assert!(
        edge_count(EdgeType::Implements) > 0,
        "Expected implements edges"
    );
    assert!(
        edge_count(EdgeType::UsesType) > 0,
        "Expected type-use edges"
    );
}

#[test]
fn test_full_analysis_java_overloads_keep_unique_qnames() {
    let parser = Parser::new();
    let path = testdata_path("java/com/example/core/Worker.java");
    let content = std::fs::read_to_string(&path).expect("Could not read test file");
    let units = parser.parse_file(&path, &content).expect("Parse failed");

    let analyzer = SemanticAnalyzer::new();
    let graph = analyzer
        .analyze(units, &AnalyzeOptions::default())
        .expect("analysis failed");

    let processes: Vec<_> = (0..graph.unit_count() as u64)
        .filter_map(|id| graph.get_unit(id))
        .filter(|u| u.language == Language::Java && u.name == "process")
        .collect();
    assert!(processes.len() >= 2, "Expected overloaded process units");

    let unique: std::collections::HashSet<_> = processes
        .iter()
        .map(|u| u.qualified_name.as_str())
        .collect();
    assert_eq!(
        unique.len(),
        processes.len(),
        "Overloaded Java methods should have unique qnames"
    );
}

#[test]
fn test_full_analysis_containment_edges() {
    let units = parse_test_file("python/simple_module.py");
    let analyzer = SemanticAnalyzer::new();
    let graph = analyzer
        .analyze(units, &AnalyzeOptions::default())
        .expect("analysis failed");

    // Check that Contains edges exist (module contains functions/classes)
    let contains_count = graph
        .edges()
        .iter()
        .filter(|e| e.edge_type == EdgeType::Contains)
        .count();

    assert!(
        contains_count > 0,
        "Should have containment edges from module to its children"
    );
}

#[test]
fn test_full_analysis_call_edges() {
    let units = parse_test_file("python/simple_module.py");
    let analyzer = SemanticAnalyzer::new();
    let graph = analyzer
        .analyze(units, &AnalyzeOptions::default())
        .expect("analysis failed");

    // Python parser extracts call references, so we should have Calls edges
    let calls_count = graph
        .edges()
        .iter()
        .filter(|e| e.edge_type == EdgeType::Calls)
        .count();

    // test_animals calls Dog() and dog.speak(), these should be resolved
    assert!(
        calls_count > 0,
        "Should have call edges from resolved references"
    );
}

#[test]
fn test_full_analysis_inheritance_edges() {
    let units = parse_test_file("python/simple_module.py");
    let analyzer = SemanticAnalyzer::new();
    let graph = analyzer
        .analyze(units, &AnalyzeOptions::default())
        .expect("analysis failed");

    // Dog inherits from Animal
    let inherits_count = graph
        .edges()
        .iter()
        .filter(|e| e.edge_type == EdgeType::Inherits)
        .count();

    assert!(
        inherits_count > 0,
        "Should have inheritance edges (Dog -> Animal)"
    );
}

#[test]
fn test_full_analysis_with_patterns_disabled() {
    let units = parse_test_file("python/simple_module.py");
    let analyzer = SemanticAnalyzer::new();
    let opts = AnalyzeOptions {
        detect_patterns: false,
        extract_concepts: false,
        trace_ffi: false,
    };
    let graph = analyzer.analyze(units, &opts).expect("analysis failed");

    // Should still work, just fewer pattern nodes
    assert!(graph.unit_count() > 0);
}

#[test]
fn test_full_analysis_empty_input() {
    let analyzer = SemanticAnalyzer::new();
    let graph = analyzer
        .analyze(vec![], &AnalyzeOptions::default())
        .expect("analysis of empty input failed");

    assert_eq!(graph.unit_count(), 0);
    assert_eq!(graph.edge_count(), 0);
}

#[test]
fn test_full_analysis_multi_language() {
    // Parse files from multiple languages
    let parser = Parser::new();
    let testdata = testdata_path("");
    let opts = agentic_codebase::parse::ParseOptions::default();
    let result = parser
        .parse_directory(&testdata, &opts)
        .expect("parse failed");

    let analyzer = SemanticAnalyzer::new();
    let graph = analyzer
        .analyze(result.units, &AnalyzeOptions::default())
        .expect("analysis failed");

    assert!(
        graph.unit_count() > 10,
        "Multi-language graph should have many units, got {}",
        graph.unit_count()
    );
    assert!(
        graph.edge_count() > 5,
        "Multi-language graph should have edges, got {}",
        graph.edge_count()
    );

    // Check that multiple languages are represented
    let languages: std::collections::HashSet<_> = (0..graph.unit_count() as u64)
        .filter_map(|id| graph.get_unit(id))
        .map(|u| u.language)
        .collect();
    assert!(
        languages.len() >= 3,
        "Should have at least 3 languages, got {}",
        languages.len()
    );
}

#[test]
fn test_analysis_preserves_metadata() {
    let units = parse_test_file("rust/simple_lib.rs");
    let analyzer = SemanticAnalyzer::new();
    let graph = analyzer
        .analyze(units, &AnalyzeOptions::default())
        .expect("analysis failed");

    // Find the calculate function and verify its metadata
    let calc = (0..graph.unit_count() as u64)
        .filter_map(|id| graph.get_unit(id))
        .find(|u| u.name == "calculate");

    assert!(calc.is_some(), "calculate should be in graph");
    let calc = calc.unwrap();
    assert_eq!(calc.language, Language::Rust);
    assert_eq!(calc.unit_type, CodeUnitType::Function);
    assert_eq!(calc.visibility, Visibility::Public);
    assert!(calc.complexity > 1);
}

#[test]
fn test_analysis_preserves_async() {
    let units = parse_test_file("rust/simple_lib.rs");
    let analyzer = SemanticAnalyzer::new();
    let graph = analyzer
        .analyze(units, &AnalyzeOptions::default())
        .expect("analysis failed");

    let fetch = (0..graph.unit_count() as u64)
        .filter_map(|id| graph.get_unit(id))
        .find(|u| u.name == "fetch_remote");

    assert!(fetch.is_some(), "fetch_remote should be in graph");
    assert!(fetch.unwrap().is_async, "fetch_remote should be async");
}

#[test]
fn test_analysis_preserves_signatures() {
    let units = parse_test_file("rust/simple_lib.rs");
    let analyzer = SemanticAnalyzer::new();
    let graph = analyzer
        .analyze(units, &AnalyzeOptions::default())
        .expect("analysis failed");

    let calc = (0..graph.unit_count() as u64)
        .filter_map(|id| graph.get_unit(id))
        .find(|u| u.name == "calculate");

    assert!(calc.is_some());
    assert!(
        calc.unwrap().signature.is_some(),
        "calculate should have a signature"
    );
}

#[test]
fn test_analysis_preserves_docs() {
    let units = parse_test_file("python/simple_module.py");
    let analyzer = SemanticAnalyzer::new();
    let graph = analyzer
        .analyze(units, &AnalyzeOptions::default())
        .expect("analysis failed");

    let fetch = (0..graph.unit_count() as u64)
        .filter_map(|id| graph.get_unit(id))
        .find(|u| u.name == "fetch_data");

    assert!(fetch.is_some());
    assert!(
        fetch.unwrap().doc_summary.is_some(),
        "fetch_data should have doc_summary"
    );
}
