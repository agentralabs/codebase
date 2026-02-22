# Command Surface

Install commands are documented in [Installation](installation.md).

## Binaries

- `acb` (CLI compiler and query engine)
- `acb-mcp` (MCP server)

## `acb` top-level

```bash
acb compile
acb info
acb query
acb get
acb completions
```

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
```

## `acb-mcp`

```bash
acb-mcp serve
```

Common options:

- `--config <toml>`
- `--graph <file.acb>`
- `--name <graph-name>`

## Universal MCP entry

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
