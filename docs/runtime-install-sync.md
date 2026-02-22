# Runtime, Install Output, and Sync Contract (Canonical Sync Source)

This page documents runtime behavior that should remain consistent across installer output, CLI behavior, and web documentation.

## Installer Behavior by Profile

- `desktop`
  - Installs `acb` and `acb-mcp`
  - Merges MCP configs for detected clients
- `terminal`
  - Installs `acb` and `acb-mcp`
  - Also merges MCP configs
  - Native terminal workflow remains available
- `server`
  - Installs `acb` and `acb-mcp`
  - Skips desktop config writes
  - Intended for remote/server hosts

## Post-Install Output Contract

The installer emits a completion section with:

1. Installed MCP server command
2. Restart instruction for MCP clients (desktop/terminal profiles)
3. Server auth + artifact sync instruction (server profile)
4. Optional feedback link

Expected completion marker:

```text
Install complete: AgenticCodebase (<profile>)
```

## Universal MCP Detection Goal

Any MCP client can consume the same MCP entry:

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

If auto-merge does not detect a client, add the entry manually and restart that client.

## Server Auth Pattern

```bash
TOKEN=$(openssl rand -hex 32)
export AGENTIC_TOKEN="$TOKEN"
# Clients send: Authorization: Bearer $TOKEN
```

## Artifact Sync Rule (Server)

Server runtimes cannot read laptop-local files directly.

Before using Codebase + sister data on a server, sync artifacts to server storage:

- `.acb`
- `.amem`
- `.avis`

## Smoke Test Matrix

```bash
# Install simulation
bash scripts/install.sh --dry-run
bash scripts/install.sh --profile=desktop --dry-run
bash scripts/install.sh --profile=terminal --dry-run
bash scripts/install.sh --profile=server --dry-run

# Guardrails
bash scripts/check-install-commands.sh
bash scripts/check-canonical-sister.sh
```

## Release Preflight

```bash
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo publish --dry-run
```

## Support

- Install and runtime issues: https://github.com/agentralabs/codebase/issues
