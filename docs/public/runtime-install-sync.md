# Runtime, Install Output, and Sync Contract

This page defines expected runtime behavior across installer output, CLI behavior, and web documentation.

## Installer profiles

- `desktop`: installs `acb` and `acb-mcp`, then merges detected desktop MCP config.
- `terminal`: installs `acb` and `acb-mcp` without desktop-specific UX assumptions.
- `server`: installs `acb` and `acb-mcp` without desktop config writes.

## Completion output contract

Installer must print:

1. Installed binary summary.
2. MCP restart instruction.
3. Server auth + artifact sync guidance when relevant.
4. Optional feedback instruction.

Expected completion marker:

```text
Install complete: AgenticCodebase (<profile>)
```

## Universal MCP config

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

## Server auth + sync

```bash
export AGENTIC_TOKEN="$(openssl rand -hex 32)"
```

Server deployments must sync `.acb/.amem/.avis` artifacts to server storage before runtime.
