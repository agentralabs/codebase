# Contributing to AgenticCodebase

Thank you for your interest in contributing to AgenticCodebase!

## Development Setup

### Prerequisites

- Rust 1.75+ (stable)
- Git

### Building

```bash
cargo build
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

### Running Benchmarks

```bash
cargo bench
```

### Project Structure

```
src/
  lib.rs              # Library root
  bin/
    acb.rs            # CLI binary
    agentic-codebase-mcp.rs        # MCP server binary
  types/              # Core data types
  parse/              # tree-sitter parsers (Python, Rust, TS, Go)
  semantic/           # Cross-language analysis
  graph/              # In-memory graph operations
  format/             # Binary .acb format I/O
  engine/             # Query engine
  index/              # Fast lookup indexes
  temporal/           # Time-based analysis (prophecy)
  collective/         # Pattern sharing
  cli/                # CLI commands
  mcp/                # MCP server
tests/                # Integration tests (phase-numbered)
benches/              # Criterion benchmarks
testdata/             # Test fixtures (Python, Rust, TS, Go)
```

## How to Contribute

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Make your changes
4. Run tests: `cargo test --workspace`
5. Run lints: `cargo clippy --workspace -- -D warnings`
6. Format code: `cargo fmt`
7. Commit your changes
8. Push to your fork
9. Open a Pull Request

## Code Style

- Follow Rust standard naming conventions (snake_case for functions, PascalCase for types)
- Every public item must have a `///` doc comment
- Every module must have a `//!` module-level doc comment
- Use `thiserror` for error types, `?` for propagation
- All logging via `tracing` (NOT `log`), always to stderr

## Testing

- Integration tests go in `tests/phase{N}_{area}.rs`
- Edge case tests go in `tests/edge_cases.rs`
- Unit tests use `#[cfg(test)] mod tests { ... }` inline
- Use `tempfile` for file I/O tests
- Use `testdata/` for fixture files

## Questions?

Open an issue or discussion on GitHub.
