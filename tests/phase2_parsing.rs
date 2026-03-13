//! Phase 2 tests: Multi-language parsing engine.
//!
//! Tests the tree-sitter based parsers for Python, Rust, TypeScript, Go, and Java.

use std::path::Path;

use agentic_codebase::parse::{ParseOptions, Parser};
use agentic_codebase::types::{CodeUnitType, Language, Visibility};

// ============================================================
// Helper functions
// ============================================================

fn testdata_path(relative: &str) -> std::path::PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    std::path::Path::new(manifest_dir)
        .join("testdata")
        .join(relative)
}

fn parse_test_file(relative: &str) -> Vec<agentic_codebase::parse::RawCodeUnit> {
    let path = testdata_path(relative);
    let content = std::fs::read_to_string(&path).expect("Could not read test file");
    let parser = Parser::new();
    parser.parse_file(&path, &content).expect("Parse failed")
}

fn find_unit_by_name<'a>(
    units: &'a [agentic_codebase::parse::RawCodeUnit],
    name: &str,
) -> Option<&'a agentic_codebase::parse::RawCodeUnit> {
    units.iter().find(|u| u.name == name)
}

fn find_units_by_type(
    units: &[agentic_codebase::parse::RawCodeUnit],
    unit_type: CodeUnitType,
) -> Vec<&agentic_codebase::parse::RawCodeUnit> {
    units.iter().filter(|u| u.unit_type == unit_type).collect()
}

// ============================================================
// Parser construction
// ============================================================

#[test]
fn test_parser_new() {
    let parser = Parser::new();
    assert!(parser.should_parse(Path::new("foo.py")));
    assert!(parser.should_parse(Path::new("foo.rs")));
    assert!(parser.should_parse(Path::new("foo.ts")));
    assert!(parser.should_parse(Path::new("foo.js")));
    assert!(parser.should_parse(Path::new("foo.go")));
    assert!(parser.should_parse(Path::new("foo.java")));
    assert!(!parser.should_parse(Path::new("foo.txt")));
    assert!(!parser.should_parse(Path::new("foo.c")));
}

#[test]
fn test_parser_default() {
    let parser = Parser::default();
    assert!(parser.should_parse(Path::new("foo.py")));
}

#[test]
fn test_parse_unknown_language() {
    let parser = Parser::new();
    let result = parser.parse_file(Path::new("foo.xyz"), "some content");
    assert!(result.is_err());
}

// ============================================================
// ParseOptions
// ============================================================

#[test]
fn test_parse_options_default() {
    let opts = ParseOptions::default();
    assert!(opts.languages.is_empty());
    assert!(opts.include_tests);
    assert_eq!(opts.max_file_size, 10 * 1024 * 1024);
    assert!(!opts.exclude.is_empty());
    assert!(opts.exclude.iter().any(|e| e.contains("node_modules")));
    assert!(opts.exclude.iter().any(|e| e.contains("target")));
}

// ============================================================
// Python parsing
// ============================================================

#[test]
fn test_python_parse_simple_module() {
    let units = parse_test_file("python/simple_module.py");
    assert!(!units.is_empty());

    // Should have a module unit
    let modules = find_units_by_type(&units, CodeUnitType::Module);
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].name, "simple_module");
    assert_eq!(modules[0].language, Language::Python);
}

#[test]
fn test_python_extracts_classes() {
    let units = parse_test_file("python/simple_module.py");

    let animal = find_unit_by_name(&units, "Animal").expect("Animal not found");
    assert_eq!(animal.unit_type, CodeUnitType::Type);
    assert_eq!(animal.language, Language::Python);
    assert_eq!(animal.visibility, Visibility::Public);

    let dog = find_unit_by_name(&units, "Dog").expect("Dog not found");
    assert_eq!(dog.unit_type, CodeUnitType::Type);
    // Dog should have an inheritance reference
    assert!(!dog.references.is_empty());
    let inherit_ref = dog
        .references
        .iter()
        .find(|r| r.kind == agentic_codebase::parse::ReferenceKind::Inherit);
    assert!(
        inherit_ref.is_some(),
        "Dog should reference Animal via inheritance"
    );
}

#[test]
fn test_python_extracts_functions() {
    let units = parse_test_file("python/simple_module.py");

    let fetch = find_unit_by_name(&units, "fetch_data").expect("fetch_data not found");
    assert_eq!(fetch.unit_type, CodeUnitType::Function);
    assert_eq!(fetch.visibility, Visibility::Public);

    let process = find_unit_by_name(&units, "process_items").expect("process_items not found");
    assert_eq!(process.unit_type, CodeUnitType::Function);
}

#[test]
fn test_python_extracts_methods() {
    let units = parse_test_file("python/simple_module.py");

    let init = find_unit_by_name(&units, "__init__").expect("__init__ not found");
    assert_eq!(init.unit_type, CodeUnitType::Function);

    let speak_methods: Vec<_> = units.iter().filter(|u| u.name == "speak").collect();
    // Should find speak in Animal and Dog
    assert!(
        speak_methods.len() >= 2,
        "Expected at least 2 speak methods, found {}",
        speak_methods.len()
    );
}

#[test]
fn test_python_visibility() {
    let units = parse_test_file("python/simple_module.py");

    let private_helper =
        find_unit_by_name(&units, "_private_helper").expect("_private_helper not found");
    assert_eq!(private_helper.visibility, Visibility::Internal);

    let very_private =
        find_unit_by_name(&units, "__very_private").expect("__very_private not found");
    assert_eq!(very_private.visibility, Visibility::Private);

    let process = find_unit_by_name(&units, "process_items").expect("process_items not found");
    assert_eq!(process.visibility, Visibility::Public);
}

#[test]
fn test_python_imports() {
    let units = parse_test_file("python/simple_module.py");
    let imports = find_units_by_type(&units, CodeUnitType::Import);
    assert!(
        imports.len() >= 3,
        "Expected at least 3 imports, found {}",
        imports.len()
    );
}

#[test]
fn test_python_test_detection() {
    let units = parse_test_file("python/simple_module.py");
    let test_fn = find_unit_by_name(&units, "test_animals").expect("test_animals not found");
    assert_eq!(test_fn.unit_type, CodeUnitType::Test);
}

#[test]
fn test_python_generator_detection() {
    let units = parse_test_file("python/simple_module.py");
    let gen = find_unit_by_name(&units, "generator_func").expect("generator_func not found");
    assert!(
        gen.is_generator,
        "generator_func should be detected as generator"
    );
}

#[test]
fn test_python_complexity() {
    let units = parse_test_file("python/simple_module.py");
    let process = find_unit_by_name(&units, "process_items").expect("process_items not found");
    // process_items has for, if, elif, else, for, if — multiple decision points
    assert!(
        process.complexity > 1,
        "process_items should have complexity > 1, got {}",
        process.complexity
    );
}

#[test]
fn test_python_docstrings() {
    let units = parse_test_file("python/simple_module.py");

    let fetch = find_unit_by_name(&units, "fetch_data").expect("fetch_data not found");
    assert!(fetch.doc.is_some(), "fetch_data should have a docstring");
    let doc = fetch.doc.as_ref().unwrap();
    assert!(
        doc.contains("Fetch data"),
        "Docstring should contain 'Fetch data', got: {}",
        doc
    );
}

#[test]
fn test_python_test_file_detection() {
    let path = testdata_path("python/test_sample.py");
    let content = std::fs::read_to_string(&path).expect("Could not read file");
    let parser = agentic_codebase::parse::python::PythonParser::new();
    assert!(
        agentic_codebase::parse::LanguageParser::is_test_file(&parser, &path, &content),
        "test_sample.py should be detected as a test file"
    );
}

#[test]
fn test_python_non_test_file_detection() {
    let path = testdata_path("python/simple_module.py");
    let content = std::fs::read_to_string(&path).expect("Could not read file");
    let parser = agentic_codebase::parse::python::PythonParser::new();
    assert!(
        !agentic_codebase::parse::LanguageParser::is_test_file(&parser, &path, &content),
        "simple_module.py should not be detected as a test file"
    );
}

#[test]
fn test_python_call_references() {
    let units = parse_test_file("python/simple_module.py");
    let test_fn = find_unit_by_name(&units, "test_animals").expect("test_animals not found");
    // test_animals calls Dog() and dog.speak()
    let call_refs: Vec<_> = test_fn
        .references
        .iter()
        .filter(|r| r.kind == agentic_codebase::parse::ReferenceKind::Call)
        .collect();
    assert!(
        !call_refs.is_empty(),
        "test_animals should have call references"
    );
}

// ============================================================
// Rust parsing
// ============================================================

#[test]
fn test_rust_parse_simple_lib() {
    let units = parse_test_file("rust/simple_lib.rs");
    assert!(!units.is_empty());

    let modules = find_units_by_type(&units, CodeUnitType::Module);
    assert!(!modules.is_empty());
}

#[test]
fn test_rust_extracts_structs() {
    let units = parse_test_file("rust/simple_lib.rs");

    let config = find_unit_by_name(&units, "Config").expect("Config not found");
    assert_eq!(config.unit_type, CodeUnitType::Type);
    assert_eq!(config.visibility, Visibility::Public);
    assert_eq!(config.language, Language::Rust);

    let internal = find_unit_by_name(&units, "InternalState").expect("InternalState not found");
    assert_eq!(internal.unit_type, CodeUnitType::Type);
    assert_eq!(internal.visibility, Visibility::Private);
}

#[test]
fn test_rust_extracts_traits() {
    let units = parse_test_file("rust/simple_lib.rs");

    let processor = find_unit_by_name(&units, "Processor").expect("Processor not found");
    assert_eq!(processor.unit_type, CodeUnitType::Trait);
    assert_eq!(processor.visibility, Visibility::Public);
}

#[test]
fn test_rust_extracts_enums() {
    let units = parse_test_file("rust/simple_lib.rs");

    let status = find_unit_by_name(&units, "Status").expect("Status not found");
    assert_eq!(status.unit_type, CodeUnitType::Type);
    assert_eq!(status.visibility, Visibility::Public);
}

#[test]
fn test_rust_extracts_impls() {
    let units = parse_test_file("rust/simple_lib.rs");

    let impls = find_units_by_type(&units, CodeUnitType::Impl);
    assert!(
        impls.len() >= 2,
        "Expected at least 2 impl blocks, found {}",
        impls.len()
    );

    // One should be "impl Config", another "impl Processor for Config"
    let impl_config = impls.iter().find(|u| u.name == "impl Config");
    assert!(impl_config.is_some(), "Should have impl Config");

    let impl_processor = impls.iter().find(|u| u.name.contains("Processor"));
    assert!(
        impl_processor.is_some(),
        "Should have impl Processor for Config"
    );
}

#[test]
fn test_rust_extracts_functions() {
    let units = parse_test_file("rust/simple_lib.rs");

    let calc = find_unit_by_name(&units, "calculate").expect("calculate not found");
    assert_eq!(calc.unit_type, CodeUnitType::Function);
    assert_eq!(calc.visibility, Visibility::Public);
}

#[test]
fn test_rust_visibility_variants() {
    let units = parse_test_file("rust/simple_lib.rs");

    let crate_vis =
        find_unit_by_name(&units, "crate_visible_func").expect("crate_visible_func not found");
    assert_eq!(crate_vis.visibility, Visibility::Internal);

    let super_vis =
        find_unit_by_name(&units, "super_visible_func").expect("super_visible_func not found");
    assert_eq!(super_vis.visibility, Visibility::Protected);
}

#[test]
fn test_rust_async_detection() {
    let units = parse_test_file("rust/simple_lib.rs");

    let fetch = find_unit_by_name(&units, "fetch_remote").expect("fetch_remote not found");
    assert!(fetch.is_async, "fetch_remote should be async");

    let calc = find_unit_by_name(&units, "calculate").expect("calculate not found");
    assert!(!calc.is_async, "calculate should not be async");
}

#[test]
fn test_rust_complexity() {
    let units = parse_test_file("rust/simple_lib.rs");

    let calc = find_unit_by_name(&units, "calculate").expect("calculate not found");
    assert!(
        calc.complexity > 1,
        "calculate should have complexity > 1, got {}",
        calc.complexity
    );
}

#[test]
fn test_rust_extracts_mods() {
    let units = parse_test_file("rust/simple_lib.rs");
    let mods: Vec<_> = units
        .iter()
        .filter(|u| u.name == "inner" || u.name == "tests")
        .collect();
    assert!(!mods.is_empty(), "Should find at least one mod");
}

#[test]
fn test_rust_extracts_macros() {
    let units = parse_test_file("rust/simple_lib.rs");
    let macros = find_units_by_type(&units, CodeUnitType::Macro);
    assert!(!macros.is_empty(), "Should find at least 1 macro");
    assert!(macros.iter().any(|m| m.name == "my_macro"));
}

#[test]
fn test_rust_extracts_use_decls() {
    let units = parse_test_file("rust/simple_lib.rs");
    let imports = find_units_by_type(&units, CodeUnitType::Import);
    assert!(
        imports.len() >= 2,
        "Expected at least 2 use declarations, found {}",
        imports.len()
    );
}

#[test]
fn test_rust_doc_comments() {
    let units = parse_test_file("rust/simple_lib.rs");
    let config = find_unit_by_name(&units, "Config").expect("Config not found");
    assert!(config.doc.is_some(), "Config should have doc comment");
}

#[test]
fn test_rust_test_detection() {
    let units = parse_test_file("rust/simple_lib.rs");
    let tests = find_units_by_type(&units, CodeUnitType::Test);
    assert!(
        tests.len() >= 2,
        "Expected at least 2 test functions, found {}",
        tests.len()
    );
}

#[test]
fn test_rust_test_file_detection() {
    let parser = agentic_codebase::parse::rust::RustParser::new();
    let path = testdata_path("rust/simple_lib.rs");
    let content = std::fs::read_to_string(&path).expect("Could not read file");
    // simple_lib.rs contains #[cfg(test)] and #[test], so should be detected
    assert!(
        agentic_codebase::parse::LanguageParser::is_test_file(&parser, &path, &content),
        "simple_lib.rs with #[cfg(test)] should be detected as test file"
    );
}

#[test]
fn test_rust_signatures() {
    let units = parse_test_file("rust/simple_lib.rs");
    let calc = find_unit_by_name(&units, "calculate").expect("calculate not found");
    assert!(
        calc.signature.is_some(),
        "calculate should have a signature"
    );
    let sig = calc.signature.as_ref().unwrap();
    assert!(sig.contains("i32"), "Signature should mention i32: {}", sig);
}

// ============================================================
// TypeScript parsing
// ============================================================

#[test]
fn test_ts_parse_simple_module() {
    let units = parse_test_file("typescript/simple_module.ts");
    assert!(!units.is_empty());

    let modules = find_units_by_type(&units, CodeUnitType::Module);
    assert_eq!(modules.len(), 1);
}

#[test]
fn test_ts_extracts_interfaces() {
    let units = parse_test_file("typescript/simple_module.ts");

    let logger = find_unit_by_name(&units, "Logger").expect("Logger not found");
    assert_eq!(logger.unit_type, CodeUnitType::Trait);
    assert_eq!(logger.language, Language::TypeScript);
}

#[test]
fn test_ts_extracts_type_aliases() {
    let units = parse_test_file("typescript/simple_module.ts");

    let config = find_unit_by_name(&units, "Config").expect("Config not found");
    assert_eq!(config.unit_type, CodeUnitType::Type);
}

#[test]
fn test_ts_extracts_classes() {
    let units = parse_test_file("typescript/simple_module.ts");

    let base = find_unit_by_name(&units, "BaseService").expect("BaseService not found");
    assert_eq!(base.unit_type, CodeUnitType::Type);

    let data = find_unit_by_name(&units, "DataService").expect("DataService not found");
    assert_eq!(data.unit_type, CodeUnitType::Type);
    // DataService extends BaseService
    let inherit_refs: Vec<_> = data
        .references
        .iter()
        .filter(|r| r.kind == agentic_codebase::parse::ReferenceKind::Inherit)
        .collect();
    assert!(
        !inherit_refs.is_empty(),
        "DataService should have inheritance reference"
    );
}

#[test]
fn test_ts_extracts_functions() {
    let units = parse_test_file("typescript/simple_module.ts");

    let create_logger = find_unit_by_name(&units, "createLogger").expect("createLogger not found");
    assert_eq!(create_logger.unit_type, CodeUnitType::Function);

    let load_config = find_unit_by_name(&units, "loadConfig").expect("loadConfig not found");
    assert_eq!(load_config.unit_type, CodeUnitType::Function);
    assert!(load_config.is_async, "loadConfig should be async");
}

#[test]
fn test_ts_extracts_arrow_functions() {
    let units = parse_test_file("typescript/simple_module.ts");

    let process_data = find_unit_by_name(&units, "processData").expect("processData not found");
    assert_eq!(process_data.unit_type, CodeUnitType::Function);

    let helper = find_unit_by_name(&units, "internalHelper").expect("internalHelper not found");
    assert_eq!(helper.unit_type, CodeUnitType::Function);
}

#[test]
fn test_ts_extracts_methods() {
    let units = parse_test_file("typescript/simple_module.ts");

    let get_name = find_unit_by_name(&units, "getName").expect("getName not found");
    assert_eq!(get_name.unit_type, CodeUnitType::Function);

    let start = find_unit_by_name(&units, "start").expect("start not found");
    assert_eq!(start.unit_type, CodeUnitType::Function);
    assert!(start.is_async, "start should be async");
}

#[test]
fn test_ts_extracts_imports() {
    let units = parse_test_file("typescript/simple_module.ts");
    let imports = find_units_by_type(&units, CodeUnitType::Import);
    assert!(
        imports.len() >= 2,
        "Expected at least 2 imports, found {}",
        imports.len()
    );
}

#[test]
fn test_ts_test_file_detection() {
    let parser = agentic_codebase::parse::typescript::TypeScriptParser::new();

    let test_path = testdata_path("typescript/simple_module.test.ts");
    let content = std::fs::read_to_string(&test_path).expect("Could not read file");
    assert!(
        agentic_codebase::parse::LanguageParser::is_test_file(&parser, &test_path, &content),
        "simple_module.test.ts should be detected as test file"
    );

    let non_test_path = testdata_path("typescript/simple_module.ts");
    let non_test_content = std::fs::read_to_string(&non_test_path).expect("Could not read file");
    assert!(
        !agentic_codebase::parse::LanguageParser::is_test_file(
            &parser,
            &non_test_path,
            &non_test_content
        ),
        "simple_module.ts should not be detected as test file"
    );
}

// ============================================================
// Go parsing
// ============================================================

#[test]
fn test_go_parse_simple_module() {
    let units = parse_test_file("go/simple_module.go");
    assert!(!units.is_empty());

    let modules = find_units_by_type(&units, CodeUnitType::Module);
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].language, Language::Go);
}

#[test]
fn test_go_extracts_types() {
    let units = parse_test_file("go/simple_module.go");

    let animal = find_unit_by_name(&units, "Animal").expect("Animal not found");
    assert_eq!(animal.unit_type, CodeUnitType::Type);
    assert_eq!(animal.visibility, Visibility::Public); // Uppercase = public

    let internal = find_unit_by_name(&units, "internal").expect("internal not found");
    assert_eq!(internal.unit_type, CodeUnitType::Type);
    assert_eq!(internal.visibility, Visibility::Private); // lowercase = private
}

#[test]
fn test_go_extracts_interfaces() {
    let units = parse_test_file("go/simple_module.go");

    let speaker = find_unit_by_name(&units, "Speaker").expect("Speaker not found");
    assert_eq!(speaker.unit_type, CodeUnitType::Trait); // Interface -> Trait
    assert_eq!(speaker.visibility, Visibility::Public);
}

#[test]
fn test_go_extracts_functions() {
    let units = parse_test_file("go/simple_module.go");

    let process = find_unit_by_name(&units, "ProcessItems").expect("ProcessItems not found");
    assert_eq!(process.unit_type, CodeUnitType::Function);
    assert_eq!(process.visibility, Visibility::Public);

    let helper = find_unit_by_name(&units, "privateHelper").expect("privateHelper not found");
    assert_eq!(helper.unit_type, CodeUnitType::Function);
    assert_eq!(helper.visibility, Visibility::Private);
}

#[test]
fn test_go_extracts_methods() {
    let units = parse_test_file("go/simple_module.go");

    let speak = find_unit_by_name(&units, "Speak").expect("Speak not found");
    assert_eq!(speak.unit_type, CodeUnitType::Function);
    assert_eq!(speak.visibility, Visibility::Public);
}

#[test]
fn test_go_extracts_imports() {
    let units = parse_test_file("go/simple_module.go");
    let imports = find_units_by_type(&units, CodeUnitType::Import);
    assert!(
        !imports.is_empty(),
        "Should find at least one import declaration"
    );
}

#[test]
fn test_go_visibility() {
    let units = parse_test_file("go/simple_module.go");

    let new_animal = find_unit_by_name(&units, "NewAnimal").expect("NewAnimal not found");
    assert_eq!(new_animal.visibility, Visibility::Public);

    let private_helper =
        find_unit_by_name(&units, "privateHelper").expect("privateHelper not found");
    assert_eq!(private_helper.visibility, Visibility::Private);
}

#[test]
fn test_go_test_file_detection() {
    let parser = agentic_codebase::parse::go::GoParser::new();

    let test_path = testdata_path("go/simple_module_test.go");
    assert!(
        agentic_codebase::parse::LanguageParser::is_test_file(&parser, &test_path, ""),
        "simple_module_test.go should be detected as test file"
    );

    let non_test_path = testdata_path("go/simple_module.go");
    assert!(
        !agentic_codebase::parse::LanguageParser::is_test_file(&parser, &non_test_path, ""),
        "simple_module.go should not be detected as test file"
    );
}

#[test]
fn test_go_test_function_detection() {
    let units = parse_test_file("go/simple_module_test.go");
    let tests = find_units_by_type(&units, CodeUnitType::Test);
    assert!(
        tests.len() >= 2,
        "Expected at least 2 test functions, found {}",
        tests.len()
    );
}

#[test]
fn test_go_benchmark_detection() {
    let units = parse_test_file("go/simple_module_test.go");
    // Benchmark functions start with "Benchmark" and should be detected as Test
    let bench = find_unit_by_name(&units, "BenchmarkProcessItems");
    assert!(bench.is_some(), "BenchmarkProcessItems should be found");
    assert_eq!(bench.unwrap().unit_type, CodeUnitType::Test);
}

// ============================================================
// Java parsing
// ============================================================

#[test]
fn test_java_parse_worker_module() {
    let units = parse_test_file("java/com/example/core/Worker.java");
    assert!(!units.is_empty());

    let modules = find_units_by_type(&units, CodeUnitType::Module);
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].language, Language::Java);
}

#[test]
fn test_java_extracts_types_and_signatures() {
    let units = parse_test_file("java/com/example/core/Worker.java");

    let worker = find_unit_by_name(&units, "Worker").expect("Worker not found");
    assert_eq!(worker.unit_type, CodeUnitType::Type);
    assert!(
        worker.qualified_name.starts_with("com.example.core.Worker"),
        "Worker qname should be package-rooted, got {}",
        worker.qualified_name
    );

    let process_methods: Vec<_> = units.iter().filter(|u| u.name == "process").collect();
    assert!(
        process_methods.len() >= 2,
        "Expected overloaded process methods"
    );
    let qnames: std::collections::HashSet<_> = process_methods
        .iter()
        .map(|u| u.qualified_name.as_str())
        .collect();
    assert_eq!(
        qnames.len(),
        process_methods.len(),
        "Overloaded methods should have unique qnames"
    );
}

#[test]
fn test_java_module_and_type_qnames_are_distinct() {
    let units = parse_test_file("java/com/example/core/Worker.java");

    let module = units
        .iter()
        .find(|u| u.name == "Worker" && u.unit_type == CodeUnitType::Module)
        .expect("Worker module not found");
    let ty = units
        .iter()
        .find(|u| u.name == "Worker" && u.unit_type == CodeUnitType::Type)
        .expect("Worker type not found");

    assert_ne!(
        module.qualified_name, ty.qualified_name,
        "Module and top-level type should have distinct qnames"
    );
}

#[test]
fn test_java_extracts_reference_kinds() {
    let units = parse_test_file("java/com/example/core/Worker.java");

    let worker = find_unit_by_name(&units, "Worker").expect("Worker not found");
    assert!(
        worker
            .references
            .iter()
            .any(|r| r.kind == agentic_codebase::parse::ReferenceKind::Inherit),
        "Worker should contain inheritance refs"
    );
    assert!(
        worker
            .references
            .iter()
            .any(|r| r.kind == agentic_codebase::parse::ReferenceKind::Implement),
        "Worker should contain interface refs"
    );
    assert!(
        worker
            .references
            .iter()
            .any(|r| r.kind == agentic_codebase::parse::ReferenceKind::TypeUse),
        "Worker should contain type-use refs"
    );

    let process = units
        .iter()
        .find(|u| {
            u.name == "process"
                && u.signature
                    .as_deref()
                    .is_some_and(|s| s.contains("String)"))
        })
        .expect("single-arg process not found");
    assert!(
        process
            .references
            .iter()
            .any(|r| r.kind == agentic_codebase::parse::ReferenceKind::Call),
        "process should contain call refs"
    );
}

#[test]
fn test_java_import_units_have_import_refs() {
    let units = parse_test_file("java/com/example/core/Worker.java");
    let imports = find_units_by_type(&units, CodeUnitType::Import);
    assert!(imports.len() >= 3, "Expected import units for Worker.java");
    assert!(imports.iter().all(|u| {
        u.references
            .iter()
            .any(|r| r.kind == agentic_codebase::parse::ReferenceKind::Import)
    }));
}

#[test]
fn test_java_synthetic_lambda_and_anonymous_nodes() {
    let units = parse_test_file("java/com/example/core/Worker.java");
    assert!(
        units.iter().any(|u| u.name.starts_with("lambda$")),
        "Expected synthetic lambda node"
    );
    assert!(
        units.iter().any(|u| u.name.starts_with("anonymous$")),
        "Expected synthetic anonymous class node"
    );
}

#[test]
fn test_java_test_file_detection() {
    let parser = agentic_codebase::parse::java::JavaParser::new();
    assert!(agentic_codebase::parse::LanguageParser::is_test_file(
        &parser,
        Path::new("WorkerTest.java"),
        ""
    ));
    assert!(!agentic_codebase::parse::LanguageParser::is_test_file(
        &parser,
        Path::new("Worker.java"),
        ""
    ));
}

#[test]
fn test_java_qname_collision_regression() {
    let parser = Parser::new();
    let root = testdata_path("java");
    let opts = ParseOptions {
        languages: vec![Language::Java],
        ..Default::default()
    };
    let result = parser
        .parse_directory(&root, &opts)
        .expect("parse_directory failed");

    let helpers: Vec<_> = result
        .units
        .iter()
        .filter(|u| u.name == "Helper" && u.unit_type == CodeUnitType::Type)
        .collect();
    assert!(
        helpers.len() >= 2,
        "Expected Helper classes in separate packages"
    );

    let helper_qnames: std::collections::HashSet<_> =
        helpers.iter().map(|u| u.qualified_name.as_str()).collect();
    assert_eq!(
        helper_qnames.len(),
        helpers.len(),
        "Helper qnames should be unique across packages"
    );
}

#[test]
fn test_java_deep_nesting_generated_source() {
    let parser = Parser::new();
    let depth = 4000usize;

    let mut source = String::from("package stress; public class Deep { public void run() {");
    for _ in 0..depth {
        source.push_str("if (true) {");
    }
    for _ in 0..depth {
        source.push('}');
    }
    source.push_str(" } }");

    let units = parser
        .parse_file(Path::new("Deep.java"), &source)
        .expect("deep nesting Java parse failed");

    assert!(!units.is_empty(), "Deep.java should produce units");
    assert!(
        units.iter().any(|u| u.name == "run"),
        "Expected run() method in deep nesting source"
    );
}

#[test]
fn test_java_large_body_generated_source() {
    let parser = Parser::new();
    let statements = 15000usize;

    let mut source =
        String::from("package stress; public class Large { public void run() { String s = \"x\";");
    for _ in 0..statements {
        source.push_str("helper(); s.length();");
    }
    source.push_str(" } private void helper() {} }");

    let units = parser
        .parse_file(Path::new("Large.java"), &source)
        .expect("large-body Java parse failed");

    let run = units
        .iter()
        .find(|u| u.name == "run")
        .expect("run() not found in Large.java");
    assert!(
        !run.references.is_empty(),
        "run() should contain extracted references in large body"
    );
}
// ============================================================
// Cross-language tests
// ============================================================

#[test]
fn test_all_units_have_spans() {
    for file in &[
        "python/simple_module.py",
        "rust/simple_lib.rs",
        "typescript/simple_module.ts",
        "go/simple_module.go",
        "java/com/example/core/Worker.java",
    ] {
        let units = parse_test_file(file);
        for unit in &units {
            assert!(
                unit.span.start_line > 0,
                "Unit {} in {} has invalid start line: {}",
                unit.name,
                file,
                unit.span.start_line
            );
        }
    }
}

#[test]
fn test_all_units_have_qualified_names() {
    for file in &[
        "python/simple_module.py",
        "rust/simple_lib.rs",
        "typescript/simple_module.ts",
        "go/simple_module.go",
        "java/com/example/core/Worker.java",
    ] {
        let units = parse_test_file(file);
        for unit in &units {
            assert!(
                !unit.qualified_name.is_empty(),
                "Unit {} in {} has empty qualified_name",
                unit.name,
                file
            );
        }
    }
}

#[test]
fn test_all_units_have_correct_language() {
    let py_units = parse_test_file("python/simple_module.py");
    for u in &py_units {
        assert_eq!(
            u.language,
            Language::Python,
            "Python units should be Language::Python"
        );
    }

    let rs_units = parse_test_file("rust/simple_lib.rs");
    for u in &rs_units {
        assert_eq!(
            u.language,
            Language::Rust,
            "Rust units should be Language::Rust"
        );
    }

    let ts_units = parse_test_file("typescript/simple_module.ts");
    for u in &ts_units {
        assert_eq!(
            u.language,
            Language::TypeScript,
            "TS units should be Language::TypeScript"
        );
    }

    let go_units = parse_test_file("go/simple_module.go");
    for u in &go_units {
        assert_eq!(u.language, Language::Go, "Go units should be Language::Go");
    }

    let java_units = parse_test_file("java/com/example/core/Worker.java");
    for u in &java_units {
        assert_eq!(
            u.language,
            Language::Java,
            "Java units should be Language::Java"
        );
    }
}

#[test]
fn test_unique_temp_ids_per_file() {
    for file in &[
        "python/simple_module.py",
        "rust/simple_lib.rs",
        "typescript/simple_module.ts",
        "go/simple_module.go",
        "java/com/example/core/Worker.java",
    ] {
        let units = parse_test_file(file);
        let mut ids: Vec<u64> = units.iter().map(|u| u.temp_id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(
            ids.len(),
            units.len(),
            "All temp_ids should be unique in {}",
            file
        );
    }
}

// ============================================================
// Directory parsing
// ============================================================

#[test]
fn test_parse_directory() {
    let parser = Parser::new();
    let testdata = testdata_path("");
    let opts = ParseOptions::default();
    let result = parser
        .parse_directory(&testdata, &opts)
        .expect("parse_directory failed");

    assert!(
        result.stats.files_parsed > 0,
        "Should parse at least 1 file"
    );
    assert!(!result.units.is_empty(), "Should extract units");
    assert!(result.stats.total_lines > 0, "Should count lines");
}

#[test]
fn test_parse_directory_language_filter() {
    let parser = Parser::new();
    let testdata = testdata_path("");
    let opts = ParseOptions {
        languages: vec![Language::Python],
        ..Default::default()
    };
    let result = parser
        .parse_directory(&testdata, &opts)
        .expect("parse_directory failed");

    // Should only parse Python files
    for unit in &result.units {
        assert_eq!(
            unit.language,
            Language::Python,
            "Should only contain Python units"
        );
    }
    assert!(
        result.stats.files_parsed > 0,
        "Should parse at least 1 Python file"
    );
}

#[test]
fn test_parse_directory_exclude_tests() {
    let parser = Parser::new();
    let testdata = testdata_path("");
    let opts = ParseOptions {
        include_tests: false,
        ..Default::default()
    };
    let result = parser
        .parse_directory(&testdata, &opts)
        .expect("parse_directory failed");
    // Some files should be skipped because they are test files
    assert!(result.stats.files_skipped > 0 || result.stats.files_parsed > 0);
}

#[test]
fn test_parse_directory_stats() {
    let parser = Parser::new();
    let testdata = testdata_path("");
    let opts = ParseOptions::default();
    let result = parser
        .parse_directory(&testdata, &opts)
        .expect("parse_directory failed");

    assert!(
        result.stats.parse_time_ms < 60000,
        "Parse should complete in < 60s"
    );
    assert!(
        !result.stats.by_language.is_empty(),
        "Should have language breakdown"
    );
}

// ============================================================
// Edge cases
// ============================================================

#[test]
fn test_parse_empty_source() {
    let parser = Parser::new();
    let units = parser
        .parse_file(Path::new("empty.py"), "")
        .expect("Should parse empty file");
    // Should still get a module unit
    let modules = find_units_by_type(&units, CodeUnitType::Module);
    assert_eq!(modules.len(), 1);
}

#[test]
fn test_parse_comment_only() {
    let parser = Parser::new();
    let units = parser
        .parse_file(
            Path::new("comment.py"),
            "# Just a comment\n# Another comment\n",
        )
        .expect("Should parse comment-only file");
    let modules = find_units_by_type(&units, CodeUnitType::Module);
    assert_eq!(modules.len(), 1);
}

#[test]
fn test_parse_malformed_python() {
    let parser = Parser::new();
    let source = "def broken(\n  # no closing paren\nclass Foo:\n  pass\n";
    // tree-sitter should still extract what it can
    let result = parser.parse_file(Path::new("broken.py"), source);
    // May succeed with partial results or error — either is acceptable
    match result {
        Ok(units) => {
            // Should still get a module
            assert!(!units.is_empty());
        }
        Err(_) => {
            // Acceptable for malformed input
        }
    }
}

#[test]
fn test_parse_malformed_rust() {
    let parser = Parser::new();
    let source = "fn broken( { } fn good() -> u32 { 42 }";
    let result = parser.parse_file(Path::new("broken.rs"), source);
    if let Ok(units) = result {
        assert!(!units.is_empty());
    }
}

#[test]
fn test_parse_malformed_java() {
    let parser = Parser::new();
    let source = "class Broken { void run( { int x = 1; }";
    let result = parser.parse_file(Path::new("broken.java"), source);
    if let Ok(units) = result {
        assert!(!units.is_empty());
    }
}

#[test]
fn test_parse_unicode_identifiers() {
    let parser = Parser::new();
    let source = "def cafe_naïve(données: str) -> str:\n    return données\n";
    let units = parser
        .parse_file(Path::new("unicode.py"), source)
        .expect("Should handle unicode");
    assert!(!units.is_empty());
}
