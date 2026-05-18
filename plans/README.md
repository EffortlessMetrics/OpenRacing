# OpenRacing Implementation Plans

Implementation plans are the source of truth for **how** accepted work lands.
They sequence PRs, define work items, name dependencies, list proof commands,
and describe rollback. They do not own product motivation, durable architecture
decisions, or generated status truth.

## Naming

Use one directory per lane:

```text
plans/<lane>/README.md
plans/<lane>/implementation-plan.md
plans/<lane>/closeout.md
```

## Implementation plan template

````md
# Lane implementation plan

Status: active
Owner:
Created:
Linked proposal:
Linked specs:
Linked ADRs:
Linked issues:
Linked PRs:
Support-tier impact:
Policy impact:
Active goal:

## Current state

Short factual baseline. Link to status docs and receipts.

## Work item: short-id

Status: ready | active | blocked | completed | superseded
Linked proposal:
Linked spec:
Linked ADR:
Blocks:
Blocked by:

### Goal

One paragraph.

### Production delta

What files, commands, APIs, workflows, or behavior change?

### Non-goals

What is explicitly out of scope?

### Acceptance

What must be true for the PR to merge?

### Proof commands

```bash
cargo test ...
git diff --check
```

### Rollback

How to undo this PR safely.

### Notes

Optional.
````

## Plan rules

- Plans are queues, not product strategy.
- Keep why in `docs/proposals/`.
- Keep behavior contracts in `docs/specs/`.
- Keep durable decisions in `docs/adr/`.
- Every ready work item must include proof commands and rollback guidance.
- Active goal work items should point at plan anchors like
  `plans/<lane>/implementation-plan.md#work-item-short-id`.
