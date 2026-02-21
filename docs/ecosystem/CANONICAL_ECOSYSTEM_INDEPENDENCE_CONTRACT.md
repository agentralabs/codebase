# Canonical Ecosystem Independence Contract

Date ratified: 2026-02-21  
Scope: AgenticMemory, AgenticVision, AgenticCodebase  
Status: Canonical (normative)

## Purpose

Preserve modular adoption while autonomic operations are introduced.

Users must be able to install and run any single sister project without
requiring the others.

## Relationship to Sister Kit

This independence contract is one part of the broader canonical sister standard.
For all current and future sisters, the full operational baseline lives in:

- `docs/ecosystem/CANONICAL_SISTER_KIT.md`

That file defines the cross-repo requirements for:

- release artifact naming and composition
- install contract and fallback behavior
- reusable CI guardrails
- README canonical layout
- MCP canonical profile
- packaging readiness policy
- versioning and release policy
- design asset contract
- environment variable namespace contract
- new-sister bootstrap requirements

## Canonical Rules (MUST)

1. Each sister MUST remain independently installable and operable.
2. No sister MAY require another sister as a hard runtime dependency.
3. Cross-sister integration MUST be optional adapter behavior.
4. Default autonomic ops MUST run locally within each repo boundary.
5. Feature parity MAY differ, but lifecycle ownership MUST be local-first.
6. If a sister is missing, the running sister MUST degrade gracefully, not fail.
7. Packaging and install UX MUST keep single-project setup as first-class.
8. Migration, backup, and maintenance policies MUST be repo-local by default.
9. Any future unified OS layer MUST NOT break standalone mode.
10. Public docs MUST state standalone support explicitly.

## Canonical Rules (SHOULD)

1. Shared policy vocabulary should be reused across sisters (`tier`, `sleep-cycle`, `backup`, `migrate`).
2. Health reporting should expose the same concepts in all sisters.
3. Optional adapters should be capability-detected, not config-fragile.

## User Promise

Users can:

- Install only AgenticMemory, only AgenticVision, or only AgenticCodebase.
- Get autonomic lifecycle benefits in that single project.
- Add other sisters later without re-architecting existing usage.

## Design Implication for Autonomic Ops

Autonomic ops is implemented as:

- Local policy engine per sister
- Local scheduler/sleep-cycle per sister
- Local backup and migration strategy per sister

Optional cross-sister coordination can be added later through adapters,
but it is never required for baseline reliability.

## Change Control

Any proposal that violates independent installability requires explicit written
approval and a migration plan that preserves standalone behavior.

## Conformance Checklist (for PRs and future models)

- Does this introduce a hard dependency on another sister at runtime?
- Can a user still install and run this project alone?
- Does failure of another sister break this one?
- Are lifecycle tasks still executable in standalone mode?
- Is the README still clear about standalone support?

If any answer is negative, the change is non-conformant.
