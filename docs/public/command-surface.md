---
status: stable
---

# Command Surface

Install commands are documented in [Installation](installation.md).

## Binaries

- `acb` (CLI compiler and query engine)
- `agentic-codebase-mcp` (MCP server)

## `acb` top-level

```bash
acb compile
acb info
acb query
acb get
acb completions
acb health
acb gate
acb budget
```

## `acb compile`

```bash
acb compile <repo-path> -o graph.acb
acb compile <repo-path> --exclude "target" --exclude "node_modules"
acb compile <repo-path> --coverage-report coverage.json
```

Common options:

- `--output <file.acb>`
- `--exclude <glob>` (repeatable)
- `--include-tests`
- `--coverage-report <path>`

## `acb query` types

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
acb query <file.acb> test-gap
acb query <file.acb> hotspots
acb query <file.acb> dead-code
```

## `acb health`

```bash
acb health <file.acb>
acb health <file.acb> --limit 20 --format json
```

Returns graph-wide risk, test gaps, hotspots, and dead-code summary.

## `acb gate`

```bash
acb gate <file.acb> --unit-id 42
acb gate <file.acb> --unit-id 42 --max-risk 0.55 --depth 4 --require-tests
```

Fails with non-zero exit if risk/test criteria are not met (CI-friendly).

## `acb budget`

```bash
acb budget <file.acb>
acb budget <file.acb> --horizon-years 20 --max-bytes 2147483648
acb budget <file.acb> --format json
```

Estimates long-horizon growth and reports whether the graph is on track for a fixed storage budget.

Runtime policy env:

```bash
export ACB_STORAGE_BUDGET_MODE=auto-rollup
export ACB_STORAGE_BUDGET_BYTES=2147483648
export ACB_STORAGE_BUDGET_HORIZON_YEARS=20
export ACB_STORAGE_BUDGET_TARGET_FRACTION=0.85
```

## `agentic-codebase-mcp`

```bash
agentic-codebase-mcp serve
```

Common options:

- `--config <toml>`
- `--graph <file.acb>`
- `--name <graph-name>`

## MCP Tools

All tools exposed by the `agentic-codebase-mcp` MCP server:

### Core Tools

| Tool | Purpose |
|------|---------|
| `symbol_lookup` | Look up symbols by name in the code graph |
| `impact_analysis` | Analyse the impact of changing a code unit |
| `graph_stats` | Get summary statistics about a loaded code graph |
| `list_units` | List code units in a graph, optionally filtered by type |

### Context Capture Tools

| Tool | Purpose |
|------|---------|
| `analysis_log` | Log the intent and context behind a code analysis |

### Grounding Tools (v0.2)

| Tool | Purpose |
|------|---------|
| `codebase_ground` | Verify a code claim has graph evidence — zero hallucination |
| `codebase_evidence` | Get graph evidence for a symbol name |
| `codebase_suggest` | Find symbols similar to a name (for corrections) |

### Workspace Tools (v0.2)

| Tool | Purpose |
|------|---------|
| `workspace_create` | Create a workspace to load multiple codebases |
| `workspace_add` | Add a codebase to an existing workspace |
| `workspace_list` | List all contexts in a workspace |
| `workspace_query` | Search across all codebases in workspace |
| `workspace_compare` | Compare a symbol between source and target |
| `workspace_xref` | Find where symbol exists/doesn't exist across contexts |

### Translation Tools (v0.2)

| Tool | Purpose |
|------|---------|
| `translation_record` | Record source-to-target symbol mapping |
| `translation_progress` | Get migration progress statistics |
| `translation_remaining` | List symbols not yet ported |

## Universal MCP entry

```json
{
  "mcpServers": {
    "agentic-codebase": {
      "command": "$HOME/.local/bin/agentic-codebase-mcp",
      "args": []
    }
  }
}
```
