# Why Teams Adopt AgenticCodebase

Simulation date: 2026-02-23

## Why this matters

If a team ships code weekly, the biggest hidden cost is not writing code. It is shipping unknown risk.

AgenticCodebase exists to turn "I think this is safe" into "Here is the ranked risk, with reasons, before merge."

## Core capabilities (simple language)

1. **Map your codebase into a graph**
   - Instead of raw files only, you get a structural map of functions, modules, and dependencies.
2. **Predict fragile areas before they break**
   - `prophecy`, `test-gap`, `hotspots`, and `coupling` highlight likely failure zones.
3. **Gate risky changes in CI**
   - `acb gate` can fail builds when risk/test thresholds are not met.
4. **Keep long-term graph storage under control**
   - Budget policies target ~1-2 GB over long horizons with automatic rollup behavior.

## Compelling scenario

A single engineer is about to merge a "small" patch before release.

Without AgenticCodebase:
- they run grep, scan manually, and hope no hidden blast radius exists.

With AgenticCodebase:
- they run three commands and get a ranked risk list, reasons, and the top units to harden first.

That is the difference between a guess-based release and an evidence-based release.

## With vs without (real simulation)

### Without

```bash
rg -n "fn inspect_json_target|inspect_json_target\(" src/main.rs
```

This finds locations, but not ranked risk or objective priority.

### With

```bash
acb compile <repo> -o /tmp/agentra_ui.acb
acb query /tmp/agentra_ui.acb prophecy --limit 5 --format json
acb get /tmp/agentra_ui.acb 105 --format json
```

Observed simulation output included:
- reasons like `High complexity (32); No test coverage`
- exact unit metadata for rapid triage

## Numbers that make it real

From current docs/benchmarks:
- 10K units: ~4 ms compile, ~1 MB graph, queries under ~15 us
- 50K units: ~20 ms compile, ~5 MB graph, queries under ~50 us
- LZ4 decompression runs at memory-bandwidth speed (3-5 GB/s class)

## Tradeoffs (honest)

- **No incremental compile yet**: full compile is used today.
- **Monorepo strategy matters**: large monorepos should use root or per-package graphs intentionally.
- **Prediction quality depends on graph quality**: stale graphs produce stale risk signals.

## What this means for technical readers

- Faster, repeatable risk triage.
- Less reviewer variance.
- Better CI policy enforcement.

## What this means for non-technical readers

- Clearer release risk communication.
- Better prioritization of engineering time.
- Fewer surprise regressions from "looks safe" merges.

## Multi-LLM fit

Claude, Gemini, OpenAI/Codex, Cursor, VS Code, and Windsurf teams can all consume the same risk outputs through MCP-compatible workflows.

## Start in 5 minutes

```bash
acb compile <repo> -o repo.acb
acb query repo.acb prophecy --limit 5
acb query repo.acb test-gap --limit 10
```

Success signal:
- your team can name top risk units and explain why they were prioritized.
