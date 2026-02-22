# Rust API Reference

Complete reference for the `agentic_codebase` Rust library (v0.1.0). Install with `cargo add agentic-codebase`.

## Module Structure

```
agentic_codebase
  ├── parse::parser        # Source file parsing (tree-sitter)
  ├── semantic::analyzer    # Semantic analysis and graph building
  ├── graph                 # CodeGraph, CodeUnit, CodeEdge types
  ├── types                 # Shared types (UnitType, EdgeType, Language, etc.)
  ├── format                # Binary .acb reader/writer
  ├── engine::query         # Query engine (24 query types)
  ├── temporal              # Git history analysis
  ├── collective            # Cross-repository intelligence
  └── cli                   # CLI commands and output formatting
```

---

## parse::parser

### Parser

The main entry point for parsing source files.

```rust
use agentic_codebase::parse::parser::{Parser, ParseOptions, ParseResult};

let parser = Parser::new();
let result = parser.parse_directory("./src", &ParseOptions::default())?;
```

#### `Parser::new() -> Self`

Create a new parser instance. Initializes tree-sitter grammars for all supported languages.

#### `Parser::parse_directory(path, options) -> Result<ParseResult>`

Recursively scan a directory and parse all supported source files.

**Parameters:**

| Parameter | Type | Description |
|:---|:---|:---|
| `path` | `&Path` | Root directory to scan |
| `options` | `&ParseOptions` | Parsing configuration |

**Returns:** `ParseResult` containing extracted units, parse errors, and statistics.

### ParseOptions

```rust
pub struct ParseOptions {
    pub include_tests: bool,      // Include test files (default: true)
    pub exclude: Vec<String>,     // Glob patterns to exclude
    pub max_file_size: usize,     // Maximum file size in bytes (default: 10 MB)
}
```

### ParseResult

```rust
pub struct ParseResult {
    pub units: Vec<CodeUnit>,           // Extracted code units
    pub errors: Vec<ParseFileError>,    // Non-fatal parse errors
    pub stats: ParseStats,              // File and unit counts
}
```

---

## semantic::analyzer

### SemanticAnalyzer

Performs semantic analysis on parsed units to build a fully-connected graph.

```rust
use agentic_codebase::semantic::analyzer::{SemanticAnalyzer, AnalyzeOptions};

let analyzer = SemanticAnalyzer::new();
let graph = analyzer.analyze(parse_result.units, &AnalyzeOptions::default())?;
```

#### `SemanticAnalyzer::analyze(units, options) -> Result<CodeGraph>`

Analyze a collection of code units, resolve cross-references, infer relationships, and build the concept graph.

### AnalyzeOptions

```rust
pub struct AnalyzeOptions {
    pub resolve_imports: bool,      // Resolve cross-file imports (default: true)
    pub infer_calls: bool,          // Infer call relationships (default: true)
    pub compute_features: bool,     // Compute feature vectors (default: true)
}
```

---

## graph

### CodeGraph

The central data structure. A directed graph of code units connected by typed edges.

```rust
use agentic_codebase::graph::CodeGraph;

let graph: CodeGraph = /* from analyzer or reader */;
println!("Units: {}, Edges: {}", graph.unit_count(), graph.edge_count());
```

#### Key methods

| Method | Returns | Description |
|:---|:---|:---|
| `unit_count()` | `usize` | Total number of code units |
| `edge_count()` | `usize` | Total number of edges |
| `units()` | `&[CodeUnit]` | Slice of all units |
| `edges()` | `&[CodeEdge]` | Slice of all edges |
| `get_unit(id)` | `Option<&CodeUnit>` | Look up unit by ID |
| `edges_from(id)` | `Vec<&CodeEdge>` | Outgoing edges from unit |
| `edges_to(id)` | `Vec<&CodeEdge>` | Incoming edges to unit |
| `languages()` | `Vec<Language>` | Distinct languages in graph |

### CodeUnit

A single code element (function, class, module, etc.).

```rust
pub struct CodeUnit {
    pub id: u64,
    pub name: String,
    pub qualified_name: String,
    pub unit_type: UnitType,
    pub language: Language,
    pub file_path: PathBuf,
    pub span: Span,
    pub visibility: Visibility,
    pub complexity: u32,
    pub is_async: bool,
    pub is_generator: bool,
    pub stability_score: f64,
    pub signature: Option<String>,
    pub doc_summary: Option<String>,
    pub feature_vector: Vec<f32>,
}
```

### CodeEdge

A directed relationship between two units.

```rust
pub struct CodeEdge {
    pub source_id: u64,
    pub target_id: u64,
    pub edge_type: EdgeType,
    pub weight: f64,
}
```

---

## types

### UnitType

```rust
pub enum UnitType {
    Function, Method, Class, Struct, Enum,
    Interface, Trait, Module, Import,
    Variable, Constant, TypeAlias, Macro,
}
```

### EdgeType

```rust
pub enum EdgeType {
    Calls, CalledBy,
    Imports, ImportedBy,
    Contains, ContainedBy,
    Inherits, InheritedBy,
    Implements, ImplementedBy,
    Uses, UsedBy,
    Returns, Accepts,
    Overrides, OverriddenBy,
    Tests, TestedBy,
}
```

### Language

```rust
pub enum Language {
    Python, Rust, TypeScript, Go, JavaScript, Unknown,
}
```

---

## format

### AcbWriter

Serialize a `CodeGraph` to the binary `.acb` format.

```rust
use agentic_codebase::format::AcbWriter;

let writer = AcbWriter::with_default_dimension();
writer.write_to_file(&graph, "project.acb")?;
```

### AcbReader

Deserialize a `CodeGraph` from an `.acb` file.

```rust
use agentic_codebase::format::AcbReader;

let graph = AcbReader::read_from_file("project.acb")?;
```

---

## engine::query

### QueryEngine

The query engine provides 24 query types over a `CodeGraph`.

```rust
use agentic_codebase::engine::query::QueryEngine;

let engine = QueryEngine::new();
```

### Symbol Lookup

```rust
use agentic_codebase::engine::query::{SymbolLookupParams, MatchMode};

let params = SymbolLookupParams {
    name: "UserService".to_string(),
    mode: MatchMode::Contains,
    limit: 20,
    ..Default::default()
};
let results: Vec<&CodeUnit> = engine.symbol_lookup(&graph, params)?;
```

### Dependency Graph

```rust
use agentic_codebase::engine::query::DependencyParams;

let params = DependencyParams {
    unit_id: 42,
    max_depth: 5,
    edge_types: vec![],          // Empty = all types
    include_transitive: true,
};
let result = engine.dependency_graph(&graph, params)?;
```

### Impact Analysis

```rust
use agentic_codebase::engine::query::ImpactParams;

let params = ImpactParams {
    unit_id: 42,
    max_depth: 5,
    edge_types: vec![],
};
let result = engine.impact_analysis(&graph, params)?;
println!("Risk: {:.2}, Impacted: {}", result.overall_risk, result.impacted.len());
```

### Call Graph

```rust
use agentic_codebase::engine::query::{CallGraphParams, CallDirection};

let params = CallGraphParams {
    unit_id: 42,
    direction: CallDirection::Both,
    max_depth: 3,
};
let result = engine.call_graph(&graph, params)?;
```

### Similarity

```rust
use agentic_codebase::engine::query::SimilarityParams;

let params = SimilarityParams {
    unit_id: 42,
    top_k: 10,
    min_similarity: 0.5,
};
let results = engine.similarity(&graph, params)?;
```

### Prophecy

```rust
use agentic_codebase::engine::query::ProphecyParams;

let params = ProphecyParams {
    top_k: 10,
    min_risk: 0.3,
};
let result = engine.prophecy(&graph, params)?;
for pred in &result.predictions {
    println!("{}: risk={:.2} reason={}", pred.unit_id, pred.risk_score, pred.reason);
}
```

### Stability Analysis

```rust
let result = engine.stability_analysis(&graph, 42)?;
println!("Score: {:.2}", result.overall_score);
for factor in &result.factors {
    println!("  {} = {:.2}: {}", factor.name, factor.value, factor.description);
}
```

### Coupling Detection

```rust
use agentic_codebase::engine::query::CouplingParams;

let params = CouplingParams {
    unit_id: Some(42),   // Or None for global detection
    min_strength: 0.5,
};
let results = engine.coupling_detection(&graph, params)?;
```

---

## temporal

Git history analysis for temporal queries. Requires the repository to be a git repository.

```rust
use agentic_codebase::temporal::TemporalAnalyzer;

let analyzer = TemporalAnalyzer::new("./my-project")?;
let history = analyzer.analyze_unit_history(unit_id)?;
```

---

## collective

Cross-repository intelligence: delta compression, pattern extraction, and privacy-preserving sharing between graphs.

```rust
use agentic_codebase::collective::CollectiveEngine;

let engine = CollectiveEngine::new();
let delta = engine.compute_delta(&graph_old, &graph_new)?;
```

---

## Next Steps

- **[Quickstart Guide](quickstart.md)** -- Get started in 5 minutes.
- **[Core Concepts](concepts.md)** -- Understand the graph model and query tiers.
- **[Benchmarks](benchmarks.md)** -- Performance characteristics at various scales.
