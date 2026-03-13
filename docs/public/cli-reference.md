---
status: stable
---

# CLI Reference

The `acb` CLI provides command-line access to AgenticCodebase semantic code graphs.

## Global Options

| Option | Description |
|--------|-------------|
| `--format <fmt>`, `-f` | Output format: `text`, `json` (default: `text`) |
| `--verbose`, `-v` | Show detailed progress and diagnostic messages |
| `--quiet`, `-q` | Suppress all non-error output |
| `-h, --help` | Print help information |
| `-V, --version` | Print version |

## Commands

### `acb` (no subcommand)

Launch the interactive REPL for exploring code graphs.

```bash
acb
```

### `acb init`

Create a new empty `.acb` graph file.

```bash
acb init project.acb
```

### `acb compile`

Compile a repository into an `.acb` graph file. Alias: `build`.

```bash
# Compile a directory
acb compile ./src

# Compile with explicit output path
acb compile ./src -o myapp.acb

# Exclude patterns
acb compile ./src --exclude="*test*" --exclude="vendor"

# Write coverage report
acb compile ./src --coverage-report coverage.json

# Parse files even if matched by .gitignore
acb compile ./src --no-gitignore
```

| Option | Description |
|--------|-------------|
| `-o, --output <path>` | Output file path (default: `<dirname>.acb` in current dir) |
| `-e, --exclude <glob>` | Glob patterns to exclude (may be repeated) |
| `--include-tests` | Include test files in compilation (default: true) |
| `--no-gitignore` | Disable `.gitignore` filtering during file discovery |
| `--coverage-report <path>` | Write ingestion coverage report JSON |

### `acb info`

Display summary information about an `.acb` graph file. Alias: `stat`.

```bash
acb info project.acb
acb info project.acb --format json
```

### `acb query`

Run a query against a compiled `.acb` graph. Alias: `q`.

Available query types: `symbol`, `deps`, `rdeps`, `impact`, `calls`, `similar`, `prophecy`, `stability`, `coupling`, `test-gap`, `hotspots`, `dead-code`.

```bash
# Look up symbols by name
acb query project.acb symbol --name "UserService"

# Forward dependencies
acb query project.acb deps --unit-id 42 --depth 5

# Reverse dependencies (who depends on this unit)
acb query project.acb rdeps --unit-id 42

# Impact analysis with risk scoring
acb query project.acb impact --unit-id 42

# Call graph exploration
acb query project.acb calls --unit-id 42

# Find structurally similar code units
acb query project.acb similar --unit-id 42

# Predict which units are likely to break
acb query project.acb prophecy --limit 10

# Stability score for a specific unit
acb query project.acb stability --unit-id 42

# Detect tightly coupled unit pairs
acb query project.acb coupling --limit 20

# High-risk units without adequate tests
acb query project.acb test-gap --limit 10

# High-change concentration units
acb query project.acb hotspots --limit 10

# Unreachable or orphaned units
acb query project.acb dead-code --limit 20
```

| Option | Description |
|--------|-------------|
| `-n, --name <str>` | Search string for symbol queries |
| `-u, --unit-id <id>` | Unit ID for unit-centric queries |
| `-d, --depth <n>` | Maximum traversal depth (default: 3) |
| `-l, --limit <n>` | Maximum results to return (default: 20) |

### `acb get`

Get detailed information about a specific code unit by ID.

```bash
acb get project.acb 42
acb get project.acb 42 --format json
```

### `acb health`

Summarize graph health (risk, test gaps, hotspots, dead code).

```bash
acb health project.acb
acb health project.acb --limit 20
```

| Option | Description |
|--------|-------------|
| `-l, --limit <n>` | Maximum items to show per section (default: 10) |

### `acb gate`

Enforce a CI risk gate for a proposed unit change.

```bash
acb gate project.acb --unit-id 42
acb gate project.acb --unit-id 42 --max-risk 0.40 --depth 5
```

| Option | Description |
|--------|-------------|
| `-u, --unit-id <id>` | Unit ID being changed (required) |
| `--max-risk <f32>` | Max allowed overall risk score, 0.0-1.0 (default: 0.60) |
| `-d, --depth <n>` | Traversal depth for impact analysis (default: 3) |
| `--require-tests` | Fail if impacted units without tests are present (default: true) |

### `acb budget`

Estimate long-horizon storage usage against a fixed budget.

```bash
acb budget project.acb
acb budget project.acb --max-bytes 4294967296 --horizon-years 10
```

| Option | Description |
|--------|-------------|
| `--max-bytes <n>` | Max allowed bytes over the horizon (default: 2 GiB) |
| `--horizon-years <n>` | Projection horizon in years (default: 20) |

### `acb export`

Export an `.acb` file into JSON.

```bash
acb export project.acb
acb export project.acb -o graph.json
```

| Option | Description |
|--------|-------------|
| `-o, --output <path>` | Output path (defaults to stdout) |

### `acb ground`

Verify a natural-language claim against code graph evidence.

```bash
acb ground project.acb "function validate_token exists"
```

### `acb evidence`

Return evidence nodes for a symbol-like query.

```bash
acb evidence project.acb "UserService" --limit 10
```

| Option | Description |
|--------|-------------|
| `-l, --limit <n>` | Maximum results (default: 20) |

### `acb suggest`

Suggest likely symbol corrections for a typo or partial name.

```bash
acb suggest project.acb "UserServce" --limit 5
```

| Option | Description |
|--------|-------------|
| `-l, --limit <n>` | Maximum suggestions (default: 10) |

### `acb workspace`

Workspace operations across multiple `.acb` files.

```bash
# Create a workspace
acb workspace create my-migration

# Add a context
acb workspace add my-migration old-api.acb --role source --language rust

# List contexts
acb workspace list my-migration

# Query across contexts
acb workspace query my-migration "authenticate"

# Compare a symbol across contexts
acb workspace compare my-migration "UserService"

# Cross-reference a symbol
acb workspace xref my-migration "authenticate"
```

### `acb completions`

Generate shell completion scripts.

```bash
acb completions bash > ~/.local/share/bash-completion/completions/acb
acb completions zsh > ~/.zfunc/_acb
acb completions fish > ~/.config/fish/completions/acb.fish
```
