# Quickstart Guide

Get AgenticCodebase running in under 5 minutes. This guide covers installation, compiling your first repository, querying the graph, and connecting the MCP server to your AI tool.

## Installation

### One-line installer (recommended)

```bash
curl -fsSL https://agentralabs.tech/install/codebase | bash
```

Installs `acb` and `agentic-codebase-mcp` release binaries and merges MCP config for desktop clients.

### Environment-specific installers

```bash
# Desktop MCP clients
curl -fsSL https://agentralabs.tech/install/codebase/desktop | bash

# Terminal-only (no desktop config writes)
curl -fsSL https://agentralabs.tech/install/codebase/terminal | bash

# Remote/server host (no desktop config writes)
curl -fsSL https://agentralabs.tech/install/codebase/server | bash
```

For server mode:

```bash
export AGENTIC_TOKEN="$(openssl rand -hex 32)"
```

Cloud/server runtime cannot read laptop files directly. Sync `.acb/.amem/.avis` artifacts to server storage first.

### Cargo install

```bash
cargo install agentic-codebase
```

Requires Rust 1.70 or later (tested with 1.90.0). This installs both the `acb` CLI and the `agentic-codebase-mcp` MCP server binary.

### From source

```bash
git clone https://github.com/agentralabs/codebase.git
cd agentic-codebase
cargo build --release
```

Binaries are at `target/release/acb` and `target/release/agentic-codebase-mcp`.

## Compile Your First Repository

A **compilation** scans a source directory, parses all supported files (Python, Rust, TypeScript, Go), performs semantic analysis, and writes a compact `.acb` binary.

```bash
# Compile a repository
acb compile ./my-project -o project.acb

# Check the result
acb info project.acb
```

Expected output:

```
  -> project.acb
     Version:   v1
     Units:     142
     Edges:     387
     Languages: 2
     File size: 48.3 KB

     -> Rust           98 units
     -> Python         44 units
```

### Compilation options

```bash
# Exclude test files and vendor directories
acb compile ./src --exclude="*test*" --exclude="vendor" -o project.acb

# Verbose output (shows parse progress and errors)
acb compile ./src -v -o project.acb

# JSON output (for scripting)
acb compile ./src -f json -o project.acb
```

## Query the Graph

### Find symbols by name

```bash
acb query project.acb symbol --name "UserService"
```

Output:

```
  Symbol lookup: "UserService" (2 results)

   #1. src::services::UserService (Class) src/services/user.py:15
   #2. src::tests::test_user_service (Function) src/tests/test_user.py:8
```

### Trace dependencies

```bash
# Forward dependencies: what does unit 42 depend on?
acb query project.acb deps --unit-id 42 --depth 3

# Reverse dependencies: who depends on unit 42?
acb query project.acb rdeps --unit-id 42
```

### Impact analysis

```bash
# What breaks if I change unit 42?
acb query project.acb impact --unit-id 42
```

Output includes risk scoring (LOW/MEDIUM/HIGH), test coverage status, and recommendations.

### Call graph

```bash
acb query project.acb calls --unit-id 42 --depth 3
```

### Structural similarity

```bash
# Find code that looks structurally similar to unit 42
acb query project.acb similar --unit-id 42 --limit 5
```

### Code prophecy

```bash
# Predict which units are most likely to break
acb query project.acb prophecy --limit 10
```

### Stability analysis

```bash
# Get stability score and contributing factors for a unit
acb query project.acb stability --unit-id 42
```

### Coupling detection

```bash
# Find tightly coupled unit pairs
acb query project.acb coupling
```

### Get unit details

```bash
# Full metadata for a specific unit
acb get project.acb 42
```

## JSON Output

All commands support `--format json` (or `-f json`) for machine-readable output:

```bash
acb -f json info project.acb
acb -f json query project.acb symbol --name "UserService"
acb -f json get project.acb 42
```

## MCP Server

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

Restart Claude Desktop and ask:

> "Compile the current project and tell me about its architecture."

Claude will call `acb_compile` to build the graph and then use query tools to explore it.

### What the MCP server exposes

- **Tools**: `acb_compile`, `acb_info`, `acb_query`, `acb_get`, `acb_load`, `acb_unload`
- **Resources**: Graph metadata, unit listings accessible via `acb://` URIs
- **Prompts**: Pre-built prompt templates for common code analysis tasks

---

## Next Steps

- **[Core Concepts](concepts.md)** -- Understand the graph model, code units, edge semantics, and query tiers.
- **[API Reference](api-reference.md)** -- Complete Rust library reference for all modules and types.
- **[Benchmarks](benchmarks.md)** -- Detailed performance characteristics at various scales.
- **[Integration Guide](integration-guide.md)** -- Connect to MCP clients, CI pipelines, and custom tooling.
- **[FAQ](faq.md)** -- Common questions and answers.
