# OpenRacing Active Goals

Active goal manifests tell agents what to do **now**. They are small,
machine-readable TOML files that point to the proposal, specs, ADRs, plan,
status docs, claim boundaries, and proof commands for the current lane.

## Files

```text
.openracing/goals/active.toml
.openracing/goals/archive/YYYY-MM-DD-<lane>.toml
```

`active.toml` is intentionally not created by the scaffold. Add it when a lane is
selected and the linked proposal/spec/plan artifacts exist.

## Template

```toml
id = "openracing-lane-id"
title = "Human readable lane title"
status = "active"
owner = "codex-claude"
created = "2026-05-17"

proposal = "docs/proposals/OPENRACING-PROP-0001-lane.md"
plan = "plans/lane/implementation-plan.md"

specs = [
  "docs/specs/OPENRACING-SPEC-0001-contract.md",
]

adrs = [
  "docs/adr/0001-existing-decision.md",
]

objective = """
State the current lane objective in one paragraph.
"""

end_state = [
  "Checkable end-state outcome.",
]

claim_boundaries = [
  "Do not claim behavior beyond the linked spec until proof exists.",
]

status_docs = [
  "docs/PROJECT_STATUS.md",
]

[[work_item]]
id = "work-item-id"
status = "ready"
spec = "docs/specs/OPENRACING-SPEC-0001-contract.md"
adr = "docs/adr/0001-existing-decision.md"
plan = "plans/lane/implementation-plan.md#work-item-work-item-id"
current_pointer = "docs/PROJECT_STATUS.md"
claim_boundary = "What this work item may and may not claim."
commands = [
  "cargo test --workspace",
  "git diff --check",
]
```

## Agent rules

- Read `docs/reference/SPEC_SYSTEM.md` before using an active goal.
- Pick exactly one ready work item.
- Do not invent missing lanes or missing source-of-truth artifacts.
- Run the listed proof commands or record why proof is unavailable.
- Archive the prior active goal before activating a new lane.
