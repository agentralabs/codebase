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

## Workspace auto-indexing behavior

- Installer writes `acb-mcp-agentra` launcher as MCP entrypoint.
- On every start, launcher resolves active workspace and graph in this order:
1. Explicit override: `AGENTRA_ACB_PATH` / `AGENTRA_GRAPH_PATH`.
2. Active workspace root (`AGENTRA_WORKSPACE_ROOT` / `AGENTRA_PROJECT_ROOT` / current project dir).
3. Cached per-workspace graph: `${CODEX_HOME:-~/.codex}/graphs/<workspace-slug>.acb`.
4. Latest cached fallback graph if workspace resolution is unavailable.
- If the per-workspace graph is missing or stale, launcher rebuilds it automatically with a lock to avoid parallel rebuild races.
- If your MCP client starts outside the project directory, set:

```bash
export AGENTRA_WORKSPACE_ROOT="/absolute/path/to/project"
```

## Server auth + sync

```bash
export AGENTIC_TOKEN="$(openssl rand -hex 32)"
```

Server deployments must sync `.acb/.amem/.avis` artifacts to server storage before runtime.

## Long-horizon storage budget policy

To target ~1-2 GB over long horizons (for example 20 years), configure:

```bash
export ACB_STORAGE_BUDGET_MODE=auto-rollup
export ACB_STORAGE_BUDGET_BYTES=2147483648
export ACB_STORAGE_BUDGET_HORIZON_YEARS=20
export ACB_STORAGE_BUDGET_TARGET_FRACTION=0.85
```

Modes:

- `auto-rollup`: trims oldest `.acb` backup lineage when budget pressure is detected.
- `warn`: emit warnings only.
- `off`: disable policy.
