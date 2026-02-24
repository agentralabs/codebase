# Initial Problem Coverage (Codebase)

This page records the **foundational problems AgenticCodebase already solved** before the newer primary-problem expansion.

## Reference set

| Ref | Initial problem solved | Shipped capability |
|---|---|---|
| ICB-I01 | No repository-wide structural map | `acb compile` builds a typed `.acb` concept graph |
| ICB-I02 | Slow, text-only code navigation | `acb query symbol|deps|rdeps|calls` |
| ICB-I03 | Unknown change blast radius | `acb query impact` with risk scoring |
| ICB-I04 | Hidden coupling and fragility | `acb query coupling`, `acb query prophecy` |
| ICB-I05 | Test blind spots before merge | `acb query test-gap`, `acb health` |
| ICB-I06 | No enforceable pre-merge risk gate | `acb gate --max-risk --require-tests` |
| ICB-I07 | No long-horizon artifact governance | `acb budget` + storage budget policy env vars |
| ICB-I08 | No universal MCP code-intelligence surface | `acb-mcp` tool/resource surface |

## AgenticCodebase verification snapshot

Verification method used: AgenticCodebase scanning AgenticCodebase itself.

```bash
acb -f json compile . -o /tmp/acb_codebase_repo.acb --exclude target --exclude .git --include-tests
acb -f json info /tmp/acb_codebase_repo.acb
acb -f json query /tmp/acb_codebase_repo.acb symbol --name gate
acb -f json query /tmp/acb_codebase_repo.acb symbol --name budget
acb -f json query /tmp/acb_codebase_repo.acb symbol --name health
```

Observed snapshot (2026-02-24):

- Units: `2118`
- Edges: `1830`
- Languages: `5`
- Compile status: `ok`
- Symbol evidence:
  - `commands::cmd_gate`
  - `commands::cmd_budget`
  - `commands::cmd_health`

## Status

All initial references `ICB-I01` to `ICB-I08` are implemented and actively testable from CLI/MCP surfaces.

## See also

- [Primary Problem Coverage](primary-problem-coverage.md)
- [Quickstart](quickstart.md)
