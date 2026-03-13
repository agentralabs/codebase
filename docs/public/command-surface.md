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
acb compile <repo-path> --no-gitignore
```

Common options:

- `--output <file.acb>`
- `--exclude <glob>` (repeatable)
- `--include-tests`
- `--no-gitignore`
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
| `impact_analyze` | Analyze the full impact of a proposed code change with blast radius and risk assessment |
| `impact_path` | Find the impact path between two code units |
| `graph_stats` | Get summary statistics about a loaded code graph |
| `list_units` | List code units in a graph, optionally filtered by type |

### Context Capture Tools

| Tool | Purpose |
|------|---------|
| `analysis_log` | Log the intent and context behind a code analysis |

### Search Tools

| Tool | Purpose |
|------|---------|
| `search_semantic` | Natural-language semantic search across the codebase |
| `search_similar` | Find code units similar to a given unit |
| `search_explain` | Explain why a unit matched a search query |

### Concept Tools

| Tool | Purpose |
|------|---------|
| `concept_find` | Find code implementing a concept (e.g., authentication, payment) |
| `concept_explain` | Explain how a concept is implemented with details |
| `concept_map` | Map all detected concepts in the codebase |

### Archaeology Tools

| Tool | Purpose |
|------|---------|
| `archaeology_node` | Investigate the full history and evolution of a code unit |
| `archaeology_when` | Get the timeline of changes for a code unit |
| `archaeology_why` | Explain why code looks the way it does based on its history |

### Architecture Tools

| Tool | Purpose |
|------|---------|
| `architecture_infer` | Infer the architecture pattern of the codebase |
| `architecture_validate` | Validate the codebase against its inferred architecture |

### Pattern Tools

| Tool | Purpose |
|------|---------|
| `pattern_extract` | Extract all detected patterns from the codebase |
| `pattern_check` | Check a code unit against detected patterns for violations |
| `pattern_suggest` | Suggest patterns for new code based on file location |

### Prophecy Tools

| Tool | Purpose |
|------|---------|
| `prophecy` | Predict the future of a code unit based on history, complexity, and dependencies |
| `prophecy_if` | What-if scenario: predict impact of a hypothetical change |

### Regression Tools

| Tool | Purpose |
|------|---------|
| `regression_predict` | Predict which tests are most likely affected by a change |
| `regression_minimal` | Get the minimal test set needed for a change |

### Grounding Tools (v0.2)

| Tool | Purpose |
|------|---------|
| `codebase_ground` | Verify a code claim has graph evidence — zero hallucination |
| `codebase_ground_claim` | Ground a claim with full citations including file locations and code snippets |
| `codebase_evidence` | Get graph evidence for a symbol name |
| `codebase_suggest` | Find symbols similar to a name (for corrections) |
| `codebase_cite` | Get a citation for a specific code unit |
| `hallucination_check` | Check AI-generated output for hallucinations about code |
| `truth_register` | Register a truth claim for ongoing maintenance |
| `truth_check` | Check if a registered truth is still valid |

### Workspace Tools (v0.2)

| Tool | Purpose |
|------|---------|
| `workspace_create` | Create a workspace to load multiple codebases |
| `workspace_add` | Add a codebase to an existing workspace |
| `workspace_list` | List all contexts in a workspace |
| `workspace_query` | Search across all codebases in workspace |
| `workspace_compare` | Compare a symbol between source and target |
| `workspace_xref` | Find where symbol exists/doesn't exist across contexts |
| `compare_codebases` | Full structural, conceptual, and pattern comparison between two codebases |
| `compare_concept` | Compare how a concept is implemented across two codebases |
| `compare_migrate` | Generate a migration plan from source to target codebase |

### Translation Tools (v0.2)

| Tool | Purpose |
|------|---------|
| `translation_record` | Record source-to-target symbol mapping |
| `translation_progress` | Get migration progress statistics |
| `translation_remaining` | List symbols not yet ported |

### Compact Facade Tools (v0.3+)

Use these to keep MCP tool surfaces small while preserving backward compatibility:

| Tool | Purpose |
|------|---------|
| `codebase_core` | Unified core analysis/impact operations via `operation` |
| `codebase_grounding` | Unified grounding operations via `operation` |
| `codebase_workspace` | Unified workspace/compare operations via `operation` |
| `codebase_session` | Unified session operations via `operation` |
| `codebase_conceptual` | Unified concept/architecture/search operations via `operation` |
| `codebase_translation` | Unified translation operations via `operation` |
| `codebase_archaeology` | Unified archaeology/resurrection operations via `operation` |
| `codebase_patterns` | Unified pattern/genetics operations via `operation` |
| `codebase_collective` | Unified telepathy/soul operations via `operation` |
| `codebase_intelligence` | Unified prophecy/regression/omniscience operations via `operation` |

Compact list mode:

```bash
export ACB_MCP_TOOL_SURFACE=compact
```

In compact mode, `tools/list` returns only the 10 facade tools above, while all legacy tool names remain callable.

### Advanced Tools

#### Resurrection Tools

| Tool | Purpose |
|------|---------|
| `resurrect_search` | Search for traces of deleted code |
| `resurrect_attempt` | Attempt to reconstruct deleted code from traces |
| `resurrect_verify` | Verify a resurrection attempt is accurate |
| `resurrect_history` | Get resurrection history for the codebase |

#### Genetics Tools

| Tool | Purpose |
|------|---------|
| `genetics_dna` | Extract the DNA (core patterns) of a code unit |
| `genetics_lineage` | Trace the lineage of a code unit through evolution |
| `genetics_mutations` | Detect mutations (unexpected changes) in code patterns |
| `genetics_diseases` | Diagnose inherited code diseases (anti-patterns passed through lineage) |

#### Telepathy Tools

| Tool | Purpose |
|------|---------|
| `telepathy_connect` | Establish telepathic connection between codebases |
| `telepathy_broadcast` | Broadcast a code insight to connected codebases |
| `telepathy_listen` | Listen for insights from connected codebases |
| `telepathy_consensus` | Find consensus patterns across connected codebases |

#### Soul Tools

| Tool | Purpose |
|------|---------|
| `soul_extract` | Extract the soul (essential purpose and values) of code |
| `soul_compare` | Compare souls across code reincarnations |
| `soul_preserve` | Preserve a code soul during rewrite |
| `soul_reincarnate` | Guide a soul to a new code manifestation |
| `soul_karma` | Analyze the karma (positive/negative impact history) of code |

#### Omniscience Tools

| Tool | Purpose |
|------|---------|
| `omniscience_search` | Search across global code knowledge |
| `omniscience_best` | Find the best implementation of a concept globally |
| `omniscience_census` | Global code census for a concept |
| `omniscience_vuln` | Scan for known vulnerability patterns |
| `omniscience_trend` | Find emerging or declining code patterns |
| `omniscience_compare` | Compare your code to global best practices |
| `omniscience_api_usage` | Find all usages of an API globally |
| `omniscience_solve` | Find code that solves a specific problem |

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
