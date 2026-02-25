# Installation Guide

## Quick Install (one-liner)

```bash
curl -fsSL https://agentralabs.tech/install/codebase | bash
```

Downloads pre-built `acb` + `agentic-codebase-mcp` binaries, installs to `~/.local/bin/`, and merges MCP server config into Claude Desktop and Claude Code. Requires `curl` and `jq`.

### Install by environment

```bash
# Desktop MCP clients (auto-merge Claude configs)
curl -fsSL https://agentralabs.tech/install/codebase/desktop | bash

# Terminal-only (no desktop config writes)
curl -fsSL https://agentralabs.tech/install/codebase/terminal | bash

# Remote/server host (no desktop config writes)
curl -fsSL https://agentralabs.tech/install/codebase/server | bash
```

### Server auth and artifact sync

Cloud/server runtime cannot read files from your laptop directly.

```bash
export AGENTIC_TOKEN="$(openssl rand -hex 32)"
```

All MCP clients must send `Authorization: Bearer <same-token>`.
If `.acb/.amem/.avis` artifacts were created elsewhere, sync them to the server first.

---

Three ways to install AgenticCodebase, depending on your use case.

---

## 1. CLI Tool (recommended for most users)

The `acb` binary compiles codebases into `.acb` graph files and queries them. Requires **Rust 1.70+**.

```bash
cargo install agentic-codebase
```

### Verify

```bash
acb --help
acb --version
```

### Compile a repository

```bash
acb compile ./my-project -o project.acb
acb info project.acb
acb query project.acb symbol --name "UserService"
```

### Available commands

| Command | Description |
|:---|:---|
| `acb compile` | Compile a source directory into an `.acb` graph file |
| `acb info` | Display summary information about an `.acb` file |
| `acb query` | Run queries against a compiled graph (9 query types) |
| `acb get` | Get detailed information about a specific code unit |

### Query types

| Query | Alias | Description |
|:---|:---|:---|
| `symbol` | `sym`, `s` | Find code units by name |
| `deps` | `dep`, `d` | Forward dependencies of a unit |
| `rdeps` | `rdep`, `r` | Reverse dependencies (who depends on this unit) |
| `impact` | `imp`, `i` | Impact analysis with risk scoring |
| `calls` | `call`, `c` | Call graph exploration |
| `similar` | `sim` | Find structurally similar code units |
| `prophecy` | `predict`, `p` | Predict which units are likely to break |
| `stability` | `stab` | Stability score for a specific unit |
| `coupling` | `couple` | Detect tightly coupled unit pairs |

All commands support `--format json` output for programmatic consumption.

---

## 2. MCP Server (for Claude Desktop, VS Code, Cursor, Windsurf)

The MCP server exposes compiled graphs as tools, resources, and prompts to any MCP-compatible LLM client.

```bash
cargo install agentic-codebase
```

This installs both the `acb` CLI and the `agentic-codebase-mcp` MCP server binary.

### Configure Claude Desktop

Add to `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "agentic-codebase": {
      "command": "agentic-codebase-mcp",
      "args": []
    }
  }
}
```

### Configure VS Code / Cursor

Add to `.vscode/settings.json`:

```json
{
  "mcp.servers": {
    "agentic-codebase": {
      "command": "agentic-codebase-mcp",
      "args": []
    }
  }
}
```

### Configure Windsurf

Add to `~/.codeium/windsurf/mcp_config.json`:

```json
{
  "mcpServers": {
    "agentic-codebase": {
      "command": "agentic-codebase-mcp",
      "args": []
    }
  }
}
```

### Verify

Once connected, the LLM gains access to tools like `acb_compile`, `acb_query`, `acb_info`, and `acb_get`. Test by asking the LLM:

> "Compile the current project and tell me about its architecture."

The LLM should call `acb_compile` to build the graph and `acb_query` to explore it.

---

## 3. Core Library (for Rust projects)

Use AgenticCodebase as a library in your own Rust project. Requires **Rust 1.70+**.

Add to your `Cargo.toml`:

```toml
[dependencies]
agentic-codebase = "0.1"
```

### Example

```rust
use agentic_codebase::parse::parser::{Parser, ParseOptions};
use agentic_codebase::semantic::analyzer::{SemanticAnalyzer, AnalyzeOptions};
use agentic_codebase::format::{AcbWriter, AcbReader};
use agentic_codebase::engine::query::QueryEngine;

// Parse source files
let parser = Parser::new();
let result = parser.parse_directory("./my-project", &ParseOptions::default())?;

// Build the semantic graph
let analyzer = SemanticAnalyzer::new();
let graph = analyzer.analyze(result.units, &AnalyzeOptions::default())?;

// Write to disk
let writer = AcbWriter::with_default_dimension();
writer.write_to_file(&graph, "project.acb")?;

// Read back and query
let graph = AcbReader::read_from_file("project.acb")?;
let engine = QueryEngine::new();
// ... run queries
```

---

## 4. Combined with AgenticMemory and AgenticVision

AgenticCodebase is part of the Agentic ecosystem. Run all three MCP servers for full cognitive + visual + code understanding:

```json
{
  "mcpServers": {
    "memory": {
      "command": "agentic-memory-mcp",
      "args": ["serve"]
    },
    "vision": {
      "command": "agentic-vision-mcp",
      "args": ["serve"]
    },
    "codebase": {
      "command": "agentic-codebase-mcp",
      "args": []
    }
  }
}
```

An agent can associate what it *knows* (memory), what it *sees* (vision), and what it *understands about code* (codebase) in a unified workflow.

---

## Build from Source

```bash
git clone https://github.com/agentralabs/codebase.git
cd agentic-codebase

# Build both binaries (acb + agentic-codebase-mcp)
cargo build --release

# Install CLI
cargo install --path .

# Or copy binaries directly
cp target/release/acb /usr/local/bin/
cp target/release/agentic-codebase-mcp /usr/local/bin/
```

### Run tests

```bash
# All tests (432 tests)
cargo test

# Unit tests only
cargo test --lib

# Integration tests only
cargo test --tests

# Benchmarks
cargo bench
```

---

## Package Registry Links

| Component | Distribution | Install |
|:---|:---|:---|
| **agentic-codebase** (core crate) | [crates.io](https://crates.io/crates/agentic-codebase) | `cargo install agentic-codebase` |
| **acb CLI binary** | Bundled in `agentic-codebase` crate | `cargo install agentic-codebase` |
| **agentic-codebase-mcp MCP binary** | Bundled in `agentic-codebase` crate | `cargo install agentic-codebase` |
| **One-line installer** | GitHub release artifacts | `curl -fsSL https://agentralabs.tech/install/codebase \| bash` |

---

## Requirements

| Component | Minimum version |
|:---|:---|
| Rust | 1.70+ (for building from source or `cargo install`) |
| OS | macOS, Linux |
| C compiler | Required for tree-sitter grammars (Xcode CLT on macOS) |

---

## Troubleshooting

### `acb: command not found` after `cargo install`

Make sure `~/.cargo/bin` is in your PATH:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

Add this line to your `~/.zshrc` or `~/.bashrc` to make it permanent.

### Build fails with tree-sitter errors

The tree-sitter grammars require a C compiler. On macOS, ensure Xcode Command Line Tools are installed:

```bash
xcode-select --install
```

### macOS: "can't be opened because Apple cannot check it for malicious software"

```bash
xattr -d com.apple.quarantine $(which acb)
xattr -d com.apple.quarantine $(which agentic-codebase-mcp)
```

### MCP server doesn't respond

Check that the binary is accessible:

```bash
which agentic-codebase-mcp
```

The server communicates via stdin/stdout (MCP stdio transport). If running manually, send a JSON-RPC initialize request to verify:

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}' | agentic-codebase-mcp
```
