# Command Surface (Canonical Sync Source)

This page is an authoritative command catalog for AgenticCodebase and is intended as a source file for web-doc synchronization.

## Install Commands

```bash
# Recommended one-liner
curl -fsSL https://agentralabs.tech/install/codebase | bash

# Explicit profiles
curl -fsSL https://agentralabs.tech/install/codebase/desktop | bash
curl -fsSL https://agentralabs.tech/install/codebase/terminal | bash
curl -fsSL https://agentralabs.tech/install/codebase/server | bash

# Cargo install (installs both binaries)
cargo install agentic-codebase
```

## Binaries

- `acb` (CLI compiler + query engine)
- `acb-mcp` (MCP server)

## `acb` Top-Level Commands

```bash
acb compile
acb info
acb query
acb get
acb completions
```

## `acb query` Types

```bash
acb query <file.acb> symbol
acb query <file.acb> deps
acb query <file.acb> rdeps
acb query <file.acb> impact
acb query <file.acb> calls
acb query <file.acb> similar
acb query <file.acb> prophecy
acb query <file.acb> stability
acb query <file.acb> coupling
```

Core flags:

- `--name` for symbol lookup
- `--unit-id` for unit-scoped queries
- `--depth` traversal depth
- `--limit` result bound
- `--format text|json`

## `acb-mcp` Commands

```bash
acb-mcp serve
```

Common options:

- `--config <toml>`
- `--graph <file.acb>`
- `--name <graph-name>`

## Universal MCP Entry (Any MCP Client)

```json
{
  "mcpServers": {
    "agentic-codebase": {
      "command": "$HOME/.local/bin/acb-mcp",
      "args": []
    }
  }
}
```

## Verification Commands

```bash
# CLI checks
acb --version
acb --help
acb-mcp --version

# Build graph + inspect
acb compile ./my-project -o project.acb
acb info project.acb
acb query project.acb symbol --name "main"

# MCP startup check (Ctrl+C after startup)
$HOME/.local/bin/acb-mcp
```

## Artifact Contract

- Primary artifact: `.acb`
- For cross-sister server workflows, sync all required artifacts to server storage: `.acb`, `.amem`, `.avis`

## Publish Commands

```bash
# In repo root
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo publish --dry-run

# Release
cargo publish
```

## Operator Notes

- Desktop/terminal profiles merge MCP config for detected clients.
- Server profile does not write desktop MCP config files.
- After install, restart MCP clients so new config is loaded.
- Optional feedback: https://github.com/agentralabs/codebase/issues
