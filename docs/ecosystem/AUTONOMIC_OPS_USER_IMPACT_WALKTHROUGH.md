# Autonomic Ops User Impact Walkthrough

Date: 2026-02-21  
Scope: AgenticMemory, AgenticVision, AgenticCodebase  
Objective: Keep daily operation hands-off so users do not manually manage lifecycle tasks.
Canonical references:
- `CANONICAL_ECOSYSTEM_INDEPENDENCE_CONTRACT.md`
- `CANONICAL_SISTER_KIT.md`

## 1) What Is Implemented Now (Phase 4)

### AgenticMemory

- Autonomous maintenance loop is wired into MCP runtime and tenant sessions.
- Loop runs:
- Auto-save checks
- Rolling backup + retention pruning
- Sleep-cycle maintenance (decay refresh, tier balancing, completed-session auto-archive)
- Profile-driven posture via `AMEM_AUTONOMIC_PROFILE` (`desktop|cloud|aggressive`).
- Storage migration is policy-gated via `AMEM_STORAGE_MIGRATION_POLICY` (`auto-safe|strict|off`).
- Legacy `.amem` checkpoint + auto-migration occurs in `auto-safe` mode.
- SLA-aware throttling for heavy maintenance under sustained mutation pressure (`AMEM_SLA_MAX_MUTATIONS_PER_MIN`).
- Health-ledger snapshot output for operations visibility (`AMEM_HEALTH_LEDGER_DIR` or shared `AGENTRA_HEALTH_LEDGER_DIR`, default `~/.agentra/health-ledger`).

### AgenticVision

- Daemon starts a background maintenance loop automatically.
- Loop performs map cache expiry cleanup and registry delta GC.
- Profile-driven posture via `CORTEX_AUTONOMIC_PROFILE` (`desktop|cloud|aggressive`).
- Cache migration policy via `CORTEX_STORAGE_MIGRATION_POLICY` (`auto-safe|strict|off`).
- Legacy cached `.ctx` maps are rewritten forward in `auto-safe` mode.
- SLA-aware throttling for registry GC under sustained cache pressure (`CORTEX_SLA_MAX_CACHE_ENTRIES_BEFORE_GC_THROTTLE`).
- Health-ledger snapshot output (`CORTEX_HEALTH_LEDGER_DIR` or shared `AGENTRA_HEALTH_LEDGER_DIR`, default `~/.agentra/health-ledger`).

### AgenticCodebase

- Compile performs automatic pre-write rolling backup on existing `.acb` outputs.
- Collective cache self-maintains by periodic expiry eviction.
- Profile-driven posture via `ACB_AUTONOMIC_PROFILE` (`desktop|cloud|aggressive`).
- Storage migration policy via `ACB_STORAGE_MIGRATION_POLICY` (`auto-safe|strict|off`).
- Legacy `.acb` files checkpoint + auto-migrate on read in `auto-safe` mode.
- SLA-aware throttling for collective cache maintenance under sustained registry load (`ACB_SLA_MAX_REGISTRY_OPS_PER_MIN`).
- Health-ledger snapshot output for registry and CLI operations (`ACB_HEALTH_LEDGER_DIR` or shared `AGENTRA_HEALTH_LEDGER_DIR`, default `~/.agentra/health-ledger`).

## 2) Day-to-Day Effect for Users

- Default is still zero-config and conservative.
- Teams can now choose operational posture by profile instead of low-level tuning.
- Strict environments can block migration automatically.
- Upgrade-friendly environments can auto-migrate with checkpoints.

## 3) User Experience by Environment

### Local desktop

- `desktop` profile gives low-noise maintenance cadence and safer background behavior.

### MCP desktop agents (Codex/Claude/Desktop clients)

- Connected sessions continuously self-maintain with low operational overhead.
- Sleep-cycle behavior improves long-running graph health with minimal user involvement.

### Cloud/fleet

- `cloud` profile increases maintenance cadence and retention posture for high-churn runtimes.
- Policy gates allow stricter migration controls when needed.

## 4) What Is Still Pending (Phase 5+)

- Full physical tiered storage classes (separate on-disk hot/warm/cold backends).
- Major-version migration policy matrix with explicit compatibility windows.
- Unified ledger aggregation endpoint (optional adapter) while preserving standalone execution.

## 5) Canonical Constraint (Must Remain True)

All autonomic operations must preserve sister independence:

- Each repo remains independently installable and operable.
- No hard runtime dependency between sisters.
- Cross-sister integration remains optional adapter behavior.
- Missing sister components must degrade gracefully, never block core operation.

This constraint is canonical for all future implementation work.
