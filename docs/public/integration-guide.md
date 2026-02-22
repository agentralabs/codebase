# Integration Guide

This guide covers how to integrate AgenticCodebase into various environments: MCP-compatible AI tools, CI/CD pipelines, custom Rust applications, and the broader Agentic ecosystem.

## MCP Server Integration

The MCP server (`acb-mcp`) exposes AgenticCodebase to any LLM client that supports the [Model Context Protocol](https://modelcontextprotocol.io/).

### Claude Desktop

Add to `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "agentic-codebase": {
      "command": "acb-mcp",
      "args": []
    }
  }
}
```

Restart Claude Desktop. The LLM now has access to code analysis tools.

### VS Code / Cursor

Add to `.vscode/settings.json`:

```json
{
  "mcp.servers": {
    "agentic-codebase": {
      "command": "acb-mcp",
      "args": []
    }
  }
}
```

### Windsurf

Add to `~/.codeium/windsurf/mcp_config.json`:

```json
{
  "mcpServers": {
    "agentic-codebase": {
      "command": "acb-mcp",
      "args": []
    }
  }
}
```

### MCP Tools Reference

The MCP server exposes these tools:

| Tool | Description |
|:---|:---|
| `acb_compile` | Compile a source directory into a graph |
| `acb_load` | Load a pre-compiled `.acb` file into memory |
| `acb_unload` | Unload a graph from memory |
| `acb_info` | Get graph metadata (units, edges, languages) |
| `acb_query` | Run any of the 24 query types |
| `acb_get` | Get detailed info about a specific code unit |

### MCP Resources

Loaded graphs expose resources via `acb://` URIs:

| URI Pattern | Description |
|:---|:---|
| `acb://graphs` | List all loaded graphs |
| `acb://graphs/{name}/info` | Graph metadata |
| `acb://graphs/{name}/units` | Unit listing |
| `acb://graphs/{name}/units/{id}` | Specific unit details |

---

## CI/CD Integration

AgenticCodebase is designed for automated pipelines. All commands support `--format json` for machine-readable output.

### Impact Analysis on Pull Requests

```bash
#!/bin/bash
# ci/impact-check.sh

# Compile the current state
acb compile ./src -o current.acb -f json -q

# Get changed files from git
changed_files=$(git diff --name-only origin/main...HEAD)

# For each changed file, find affected units and run impact analysis
acb -f json query current.acb symbol --name "$changed_function" | \
  jq -r '.results[].id' | \
  while read unit_id; do
    acb -f json query current.acb impact --unit-id "$unit_id"
  done
```

### Stability Monitoring

```bash
#!/bin/bash
# ci/stability-check.sh

acb compile ./src -o project.acb -q
prophecy=$(acb -f json query project.acb prophecy --limit 5)

# Check if any high-risk predictions
high_risk=$(echo "$prophecy" | jq '[.results[] | select(.risk_score >= 0.7)] | length')
if [ "$high_risk" -gt 0 ]; then
  echo "WARNING: $high_risk high-risk predictions detected"
  echo "$prophecy" | jq '.results[] | select(.risk_score >= 0.7)'
  exit 1
fi
```

### Coupling Gate

```bash
#!/bin/bash
# ci/coupling-check.sh

acb compile ./src -o project.acb -q
coupling=$(acb -f json query project.acb coupling)

# Fail if any coupling strength exceeds threshold
violations=$(echo "$coupling" | jq '[.results[] | select(.strength >= 0.9)] | length')
if [ "$violations" -gt 0 ]; then
  echo "ERROR: $violations coupling violations (strength >= 0.9)"
  exit 1
fi
```

---

## Rust Library Integration

Use AgenticCodebase as a dependency in your Rust project:

```toml
[dependencies]
agentic-codebase = "0.1"
```

### Basic workflow

```rust
use agentic_codebase::parse::parser::{Parser, ParseOptions};
use agentic_codebase::semantic::analyzer::{SemanticAnalyzer, AnalyzeOptions};
use agentic_codebase::format::{AcbWriter, AcbReader};
use agentic_codebase::engine::query::{QueryEngine, SymbolLookupParams, MatchMode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Parse
    let parser = Parser::new();
    let result = parser.parse_directory("./src", &ParseOptions::default())?;
    println!("Parsed {} files, {} units", result.stats.files_parsed, result.units.len());

    // 2. Analyze
    let analyzer = SemanticAnalyzer::new();
    let graph = analyzer.analyze(result.units, &AnalyzeOptions::default())?;
    println!("Graph: {} units, {} edges", graph.unit_count(), graph.edge_count());

    // 3. Write
    let writer = AcbWriter::with_default_dimension();
    writer.write_to_file(&graph, "project.acb")?;

    // 4. Read back and query
    let graph = AcbReader::read_from_file("project.acb")?;
    let engine = QueryEngine::new();

    let params = SymbolLookupParams {
        name: "main".to_string(),
        mode: MatchMode::Contains,
        limit: 10,
        ..Default::default()
    };
    let results = engine.symbol_lookup(&graph, params)?;
    for unit in results {
        println!("  {} ({}) at {}:{}",
            unit.qualified_name, unit.unit_type,
            unit.file_path.display(), unit.span.start_line);
    }

    Ok(())
}
```

### Custom query pipelines

```rust
use agentic_codebase::engine::query::{ImpactParams, ProphecyParams};

// Impact analysis for a specific unit
let impact = engine.impact_analysis(&graph, ImpactParams {
    unit_id: 42,
    max_depth: 5,
    edge_types: vec![],
})?;

println!("Risk: {:.2}, {} units impacted", impact.overall_risk, impact.impacted.len());

// Code prophecy across the entire graph
let prophecy = engine.prophecy(&graph, ProphecyParams {
    top_k: 10,
    min_risk: 0.3,
})?;

for pred in &prophecy.predictions {
    println!("  Unit {}: risk={:.2} - {}", pred.unit_id, pred.risk_score, pred.reason);
}
```

---

## Agentic Ecosystem Integration

AgenticCodebase works alongside [AgenticMemory](https://github.com/agentralabs/agentic-memory) and [AgenticVision](https://github.com/agentralabs/agentic-vision). Run all three MCP servers for an agent with full cognitive, visual, and code capabilities:

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
      "command": "acb-mcp",
      "args": []
    }
  }
}
```

### Cross-system workflows

An agent with all three systems can:

1. **Remember decisions** (AgenticMemory) -- "We chose PostgreSQL for the backend."
2. **See UI changes** (AgenticVision) -- "The login page layout changed since yesterday."
3. **Understand code structure** (AgenticCodebase) -- "The `AuthService` depends on `DatabasePool` and is tested by `test_auth_flow`."

The MCP protocol enables the LLM to seamlessly combine tools from all three servers in a single conversation.

---

## Next Steps

- **[Quickstart Guide](quickstart.md)** -- Get started in 5 minutes.
- **[API Reference](api-reference.md)** -- Complete Rust library reference.
- **[Core Concepts](concepts.md)** -- Understand the graph model.
- **[FAQ](faq.md)** -- Common questions and answers.
