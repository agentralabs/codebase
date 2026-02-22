# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
- MCP server (`acb-mcp`) with JSON-RPC 2.0 over stdio
- 386 tests (38 unit + 348 integration), 21 Criterion benchmarks
- Research paper: "AgenticCodebase: A Semantic Code Compiler for Navigable, Predictive, and Collective Code Intelligence"
