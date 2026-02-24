---
status: stable
---

# AgenticCodebase Overview

AgenticCodebase compiles source code into a portable `.acb` graph for fast structural queries.

## What you can do

- Compile Python, Rust, TypeScript, and Go repositories.
- Query symbols, dependencies, impact, calls, similarity, stability, and coupling.
- Expose the graph through MCP with `acb-mcp`.

## Why teams adopt AgenticCodebase

Teams adopt AgenticCodebase because it closes both the original and current code-intelligence gaps:

- Foundational problems already solved: no repository-wide map, weak structural navigation, unknown blast radius, hidden coupling, test blind spots, missing risk gates, no durable artifact governance, and no universal MCP surface.
- New high-scale problems now solved: topology blindness across large repos, safer impact analysis before change, coupling/test-gap risk visibility in CI, multi-language boundary awareness, dependency/build drift detection, and ongoing spec-to-code drift checks.
- Practical outcome for teams: faster review cycles, safer refactors, fewer production surprises, and repeatable decisions across local, desktop, and server runtimes.

For a detailed before-and-after view, see [Experience With vs Without](experience-with-vs-without.md).

## Artifact

- Primary artifact: `.acb`
- Cross-sister server workflows can pair `.acb` with `.amem` and `.avis`

## Start here

- [Installation](installation.md)
- [Quickstart](quickstart.md)
- [Command Surface](command-surface.md)
- [Runtime and Sync](runtime-install-sync.md)
- [Integration Guide](integration-guide.md)
- [Experience With vs Without](experience-with-vs-without.md)

## Works with

- **AgenticMemory** — link code-graph nodes to memory decisions for traceable reasoning across refactors.
- **AgenticVision** — pair `.acb` code graphs with `.avis` screenshots to connect code changes to UI regressions.
