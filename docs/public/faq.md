# Frequently Asked Questions

## How is this different from a language server (LSP)?

Language servers (rust-analyzer, pylsp, tsserver) provide real-time IDE features: completions, go-to-definition, diagnostics. They are designed for interactive use and keep the entire project in memory.

AgenticCodebase is a **compilation step** that produces a persistent, portable artifact. The key differences:

- **Persistence.** An `.acb` file survives across sessions. An LSP server starts fresh every time.
- **Portability.** An `.acb` file is a single binary. Copy it, store it, share it. LSP state is ephemeral.
- **Multi-language.** One graph covers Python, Rust, TypeScript, and Go in a single file. LSP servers are typically language-specific.
- **AI-native queries.** Impact analysis, code prophecy, coupling detection, and structural similarity are not LSP operations.
- **Sub-microsecond.** Graph queries take nanoseconds to microseconds. LSP responses take milliseconds to seconds.

You can use both: LSP for real-time editing, AgenticCodebase for agent memory and batch analysis.

## How is this different from RAG over source code?

RAG (Retrieval-Augmented Generation) over source code typically embeds code chunks as text vectors and retrieves the most similar chunks for a query.

AgenticCodebase is fundamentally different:

- **Structure, not text.** AgenticCodebase understands that `UserService.save()` calls `Database.insert()`. RAG sees them as two similar text blobs.
- **Typed relationships.** The graph has 18 edge types (calls, imports, inherits, tests, etc.). RAG has no concept of relationships.
- **Deterministic queries.** "What depends on module X?" returns an exact, complete answer. RAG returns approximate, possibly incomplete text matches.
- **Impact analysis.** "What breaks if I change function Y?" requires traversing dependency chains. RAG cannot answer this.
- **No embeddings needed.** AgenticCodebase uses structural analysis, not language model embeddings. No API calls, no token costs, no hallucinated connections.

You might use both: RAG for natural-language code search ("find the function that handles user authentication"), AgenticCodebase for structural queries ("what calls that function, and what tests cover it").

## What languages are supported?

Python, Rust, TypeScript, and Go. Each language is parsed using a tree-sitter grammar.

JavaScript files are parsed using the TypeScript grammar (since TypeScript is a superset of JavaScript).

Adding a new language requires implementing a tree-sitter extractor (typically 200-400 lines of Rust). The architecture is designed for this -- see the `parse/extractors/` module.

## What happens with large repositories?

AgenticCodebase handles large repositories efficiently:

- **Tree-sitter parsing** runs in microseconds per file. A 10K-file repository parses in under a second.
- **Memory-mapped I/O.** The `.acb` reader uses `mmap()` to access files without loading them entirely into memory.
- **LZ4 compression.** String content is compressed at ~2.5x ratio while decompressing at memory bandwidth speeds.
- **Fixed-size records.** Unit and edge records are fixed-size, so accessing unit N is a direct offset calculation -- O(1) regardless of graph size.

Practical limits:

- 1K units: sub-millisecond compile, ~100 KB file.
- 10K units: ~4 ms compile, ~1 MB file. All queries under 15 us.
- 50K units: ~20 ms compile, ~5 MB file. All queries under 50 us.

## Does it work with monorepos?

Yes. Compile the monorepo root (or specific subdirectories) and the graph captures cross-package relationships:

```bash
# Entire monorepo
acb compile ./monorepo -o full.acb

# Specific packages
acb compile ./monorepo/packages/api -o api.acb
acb compile ./monorepo/packages/web -o web.acb

# Exclude specific directories
acb compile ./monorepo --exclude="**/node_modules/**" --exclude="**/vendor/**" -o project.acb
```

## Can I use this in CI/CD?

Yes. AgenticCodebase is designed for automation:

```bash
# In your CI pipeline
acb compile ./src -o project.acb -f json   # JSON output for parsing
acb query project.acb prophecy -f json     # Machine-readable predictions
```

Common CI use cases:

- **Impact analysis on PRs.** Compile before/after, compare impact of changed files.
- **Stability monitoring.** Track prophecy scores over time. Alert when stability drops.
- **Coupling detection.** Flag tightly coupled modules in code review.
- **Dead code detection.** Identify unreachable code after refactoring.

## How does the MCP server work?

The `acb-mcp` binary implements the [Model Context Protocol](https://modelcontextprotocol.io/) over stdio. When an MCP-compatible client (Claude Desktop, VS Code, Cursor, Windsurf) connects, it receives:

- **Tools**: Functions the LLM can call (`acb_compile`, `acb_query`, `acb_info`, etc.)
- **Resources**: Data the LLM can read (`acb://graphs/project/units`, etc.)
- **Prompts**: Pre-built prompt templates for common analysis tasks

The MCP server manages graph state in-memory. Load a graph, query it multiple times, unload when done. All communication is JSON-RPC 2.0 over stdin/stdout.

## Can I query the graph programmatically from Rust?

Yes. AgenticCodebase is a library, not just a CLI:

```rust
use agentic_codebase::format::AcbReader;
use agentic_codebase::engine::query::{QueryEngine, SymbolLookupParams, MatchMode};

let graph = AcbReader::read_from_file("project.acb")?;
let engine = QueryEngine::new();

let params = SymbolLookupParams {
    name: "UserService".to_string(),
    mode: MatchMode::Contains,
    limit: 20,
    ..Default::default()
};
let results = engine.symbol_lookup(&graph, params)?;
```

See the [API Reference](api-reference.md) for the complete library API.

## Is the .acb format stable?

The `.acb` format is at version 1. The header includes a version field, so future versions can maintain backward compatibility. The format is designed to be forward-compatible -- new fields are appended, never reordered.

## How does code prophecy work?

Code prophecy predicts which units are most likely to cause issues based on:

1. **Stability score** -- Low-stability units (high complexity, high coupling, no tests) are higher risk.
2. **Graph topology** -- Units at the center of dependency chains have higher blast radius.
3. **Test coverage** -- Untested units in critical paths are flagged.
4. **Coupling strength** -- Tightly coupled pairs where one unit changes frequently.

Prophecy is a heuristic, not a guarantee. It is most useful for prioritizing code review and test writing.

## Does it support incremental compilation?

Not yet. Currently, `acb compile` rebuilds the entire graph from scratch. For most repositories (under 50K units), full compilation takes under 20 ms, so incremental compilation is rarely needed.

Incremental compilation is planned for a future version to support very large repositories and watch-mode workflows.
