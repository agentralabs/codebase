# Primary Problem Coverage (Codebase)

This page tracks direct coverage for Codebase primary problems:

- P09 whole-repo topology blindness
- P10 change-impact blindness
- P11 hidden coupling
- P12 test-gap blindness
- P13 refactor-safety uncertainty
- P14 multi-language boundary gaps
- P15 dependency/version drift
- P16 build-system variance
- P22 spec-to-code drift

## What is implemented now

Codebase already ships core graph and query primitives for these problems. This phase adds an explicit regression entrypoint:

```bash
./scripts/test-primary-problems.sh
```

The script validates:

1. Graph compile and topology extraction (`compile`, `info`)
2. Impact/coupling/test-gap signal (`query impact|coupling|test-gap`)
3. Health and drift observability (`health`, `query hotspots`)
4. Cross-language graphing (`Rust + Python + TypeScript` fixture)
5. Long-horizon storage governance (`budget`)
6. MCP-facing edge-case strictness (`edge_cases` targeted tests)

## Problem-to-capability map

| Problem | Coverage primitive |
|---|---|
| P09 | `acb compile`, `acb info` |
| P10 | `acb query impact` |
| P11 | `acb query coupling` |
| P12 | `acb query test-gap`, `acb health` |
| P13 | `acb gate`, `acb query impact` |
| P14 | Multi-language compile in regression script |
| P15 | `acb query hotspots`, `acb health` |
| P16 | CI matrix + deterministic compile/query checks |
| P22 | `acb health` plus regression checks tied to docs/spec contracts |

## Sister-assisted validation workflow

Codebase and Memory MCP can be used together during implementation validation:

- Code structure state: `graph_stats`, `list_units`, `symbol_lookup`, `impact_analysis`
- Longitudinal runtime evidence: `memory_stats`, `memory_quality`

Vision remains the visual evidence sister for runtime state and UI-level signals.

## See also

- [Initial Problem Coverage](initial-problem-coverage.md)
