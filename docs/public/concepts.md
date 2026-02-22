# Core Concepts

AgenticCodebase models source code as a **typed concept graph** -- a directed, weighted graph where nodes represent discrete code units and edges capture the semantic relationships between them. This document explains the foundational ideas behind the system.

## Why a Graph?

Most code analysis tools provide flat search: find files matching a pattern, find text matching a regex. This works for simple lookups but fails to capture how code actually relates: functions call other functions, classes inherit from base classes, modules import other modules, tests cover specific implementations.

A concept graph preserves these relationships explicitly. When an agent finds a function, it can also traverse to the functions it calls, the modules that import it, the tests that cover it, and the classes that contain it. This makes code navigation structured, predictive, and auditable.

Compared to embedding-based RAG, which answers "what text is similar?", a concept graph answers richer questions: "what depends on this?", "what breaks if I change this?", "what calls this?", and "what is the full inheritance chain?"

## Code Units (Nodes)

Every node in the graph is a **CodeUnit** -- a discrete, named element of source code. There are 13 unit types, each serving a distinct purpose.

### Function

A standalone callable unit of code. Functions are the most common unit type in most codebases.

```rust
// Rust
pub fn process_payment(amount: f64) -> Result<Receipt> { ... }
```

```python
# Python
def process_payment(amount: float) -> Receipt: ...
```

### Method

A function attached to a class, struct, or trait. Methods have an implicit receiver (`self`, `this`) and belong to a containing type.

### Class

An object-oriented type definition. Classes may contain methods, fields, and nested types.

### Struct

A data structure type. In Rust, structs are the primary composite type. In other languages, this maps to value types or data classes.

### Enum

An enumeration type with named variants.

### Interface / Trait

A contract that types can implement. In Rust, this is a `trait`. In TypeScript, an `interface`. In Python, an abstract base class.

### Module

A file-level or namespace-level container. Every source file creates at least one module unit.

### Import

A dependency declaration. Imports create edges between the importing module and the imported unit.

### Variable / Constant

Named bindings at module scope. Constants are immutable; variables may be reassigned.

### TypeAlias

A named alias for another type.

### Macro

A metaprogramming construct (Rust macros, Python decorators with code generation).

## Edges (Relationships)

Every edge in the graph connects two code units with a typed, weighted relationship. There are 18 edge types organized in symmetric pairs.

### Call Edges

- **Calls** / **CalledBy** -- Function A calls Function B.

### Import Edges

- **Imports** / **ImportedBy** -- Module A imports Unit B.

### Containment Edges

- **Contains** / **ContainedBy** -- Class A contains Method B, Module A contains Function B.

### Inheritance Edges

- **Inherits** / **InheritedBy** -- Class A extends Class B.

### Implementation Edges

- **Implements** / **ImplementedBy** -- Struct A implements Trait B.

### Usage Edges

- **Uses** / **UsedBy** -- Function A references Type B in its signature or body.

### Return / Accept Edges

- **Returns** -- Function A returns Type B.
- **Accepts** -- Function A accepts Type B as a parameter.

### Override Edges

- **Overrides** / **OverriddenBy** -- Method A overrides Method B from a parent class.

### Test Edges

- **Tests** / **TestedBy** -- Test function A tests Implementation B.

## Edge Weights

Every edge carries a floating-point weight (0.0 to 1.0) representing confidence or strength. Weights are assigned during semantic analysis based on heuristics:

- Direct function calls: 1.0 (definite)
- Type usage in signatures: 0.9 (strong)
- Type usage in function bodies: 0.7 (moderate)
- Inferred relationships: 0.5 (speculative)

## The Compilation Pipeline

AgenticCodebase transforms source files into a queryable graph through four stages:

### 1. Parsing (tree-sitter)

Each source file is parsed using a language-specific tree-sitter grammar. The parser extracts structural elements: function definitions, class declarations, import statements, etc. Parsing is language-aware but produces a uniform `CodeUnit` representation.

**Supported languages:** Python, Rust, TypeScript, Go.

### 2. Semantic Analysis

The semantic analyzer processes raw parse results to:

- Resolve cross-file references (imports, qualified names)
- Infer call relationships from function bodies
- Build containment hierarchies (module > class > method)
- Detect inheritance and implementation chains
- Compute feature vectors for structural similarity

### 3. Graph Building

Analyzed units and their relationships are assembled into a `CodeGraph` -- an in-memory directed graph with adjacency lists for fast traversal.

### 4. Binary Serialization (.acb)

The graph is written to a compact binary `.acb` file:

| Section | Record Size | Access Pattern |
|:---|:---|:---|
| Header | 128 bytes (fixed) | Direct read |
| Unit Table | 96 bytes per unit | O(1) by unit ID |
| Edge Table | 40 bytes per edge | Sequential scan or indexed |
| String Pool | Variable (LZ4) | Decompressed on read |
| Feature Vectors | Variable (f32) | Memory-mapped |

The binary format supports O(1) random access to any unit by ID, making queries extremely fast.

## Query Tiers

Queries are organized into three tiers based on complexity:

### Core (8 queries)

Direct graph operations. Constant or linear time. Sub-microsecond latency.

Symbol lookup, Dependency graph, Reverse dependency, Call graph, Similarity search, Type hierarchy, Containment, Pattern matching.

### Built (5 queries)

Composed from core operations. Linear to quadratic time. Microsecond latency.

Impact analysis, Coverage mapping, Execution trace, Shortest path, Reverse chains.

### Novel (11 queries)

Advanced analysis combining graph structure with heuristics. Millisecond latency.

Collective intelligence, Temporal evolution, Stability scoring, Coupling detection, Dead code, Code prophecy, Concept clustering, Migration planning, Test gap analysis, Drift detection, Hotspot analysis.

## Feature Vectors

Every code unit has an associated feature vector (default: 64 dimensions, f32). These vectors encode structural properties:

- Complexity metrics (cyclomatic, nesting depth)
- Connectivity metrics (in-degree, out-degree, fan-out)
- Size metrics (line count, parameter count)
- Type distribution (what kinds of units it relates to)

Feature vectors enable the **similarity** query: find code units that are structurally similar even if they have different names and live in different files.

## Stability Scoring

Each unit receives a stability score (0.0 to 1.0) based on multiple factors:

- **Complexity factor** -- High cyclomatic complexity reduces stability
- **Coupling factor** -- High fan-in or fan-out reduces stability
- **Test factor** -- Having tests increases stability
- **Size factor** -- Very large units are less stable
- **Depth factor** -- Deeply nested units are less stable

The `prophecy` query uses stability scores combined with graph topology to predict which units are most likely to cause issues during development.

---

## Next Steps

- **[Quickstart Guide](quickstart.md)** -- Hands-on tutorial in 5 minutes.
- **[API Reference](api-reference.md)** -- Complete Rust library reference.
- **[Benchmarks](benchmarks.md)** -- Performance at various scales.
