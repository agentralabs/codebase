# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## v0.2.0 — V2: Grounding & Multi-Context Workspaces

### Added
- **Grounding (anti-hallucination)**: Verify code claims have graph backing before an agent asserts them.
  - `codebase_ground`: Verify a claim about code against the graph. Returns verified/partial/ungrounded with evidence.
  - `codebase_evidence`: Find graph evidence for a symbol name.
  - `codebase_suggest`: Suggest similar symbols for typos/hallucinations (Levenshtein distance).
- **Multi-context workspaces**: Load and query multiple codebases simultaneously.
  - `workspace_create`: Create a multi-codebase workspace.
  - `workspace_add`: Add a codebase with role (source/target/reference/comparison).
  - `workspace_list`: List all loaded codebases.
  - `workspace_query`: Search across all graphs.
  - `workspace_compare`: Compare a symbol between source and target.
  - `workspace_xref`: Cross-reference where a symbol exists/doesn't.
- **Translation mapping**: Track code migration progress (source->target).
  - `translation_record`: Record a source->target symbol mapping.
  - `translation_progress`: Get migration progress statistics.
  - `translation_remaining`: List symbols not yet ported.
- 69 new V2 stress tests (grounding, workspace, translation, MCP integration).

### Changed
- MCP tool count increased from 5 to 17.

## [0.1.5] - 2026-02-23

### Fixed
- Enforced strict MCP parameter validation for `symbol_lookup.mode` and `impact_analysis.max_depth` to prevent silent fallbacks.
- Switched per-project graph identity to canonical-path hashing to eliminate graph collisions for same-named folders.
- Removed unsafe cached-graph fallback that could bind the wrong project graph in multi-project sessions.
- Added runtime compile locking in `agentic-codebase-mcp` and hardened launcher lock acquisition for concurrent startup reliability.
- Added regression tests for deterministic/unique project identity keys.

## [0.1.4] - 2026-02-23

### Fixed
- Hardened MCP graph lock handling to recover from stale lockfiles and avoid deadlock under concurrent launches.
- Ensured repo graph resolution falls back safely when common root detection does not yield a graph path.
- Improved per-repo auto-indexing reliability so `graph_stats` no longer fails with empty graph state during normal startup races.

## [0.1.2] - 2026-02-23

### Fixed
- MCP `list_units` now enforces and validates `unit_type` filters consistently.
- MCP `impact_analysis` now includes full dependency coverage across containment/semantic edges.
- Added regression tests to lock both MCP fixes for future releases.

## [0.1.1] - 2026-02-22

### Fixed
- Hardened MCP stdio framing to correctly handle Content-Length protocol messages.
- Improved interoperability with desktop clients that send framed MCP requests.

### Changed
- Documentation updates for workspace orchestration and install profiles.

## [0.1.0] - 2026-02-19

### Added
- Semantic code compiler with tree-sitter parsing for Python, Rust, TypeScript, and Go
- 13 code unit types and 18 edge types for typed concept graphs
- Binary file format (.acb) with 128-byte header, fixed-size records, LZ4-compressed string pools
- Memory-mapped file access via memmap2
- Query engine with 24 query types across three tiers (Core, Built, Novel)
- Five index types: SymbolIndex, TypeIndex, LanguageIndex, PathIndex, EmbeddingIndex
- Semantic analysis: cross-language resolution, pattern detection, visibility inference, FFI tracing
- Temporal analysis: change history, stability scoring, coupling detection, failure prophecy
- Collective intelligence: delta compression, pattern extraction, privacy filtering
- CLI tool (`acb`) with compile, info, query, and get commands
- MCP server (`agentic-codebase-mcp`) with JSON-RPC 2.0 over stdio
- 386 tests (38 unit + 348 integration), 21 Criterion benchmarks
- Research paper: "AgenticCodebase: A Semantic Code Compiler for Navigable, Predictive, and Collective Code Intelligence"
