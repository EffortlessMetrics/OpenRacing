# Repo source-of-truth system

OpenRacing uses a linked source-of-truth stack. The goal is to make the repo
readable by humans and machines without relying on chat history.

## Stack

```text
Roadmap
  -> Proposal
    -> Spec
      -> ADR
        -> Implementation plan
          -> Active goal
            -> PR
              -> Proof
```

## Artifact roles

| Artifact | Owns | Does not own |
|---|---|---|
| Roadmap | Release direction, milestone framing, lane list | PR queue, live proof state |
| Proposal | Why, users, alternatives, success criteria | Behavior contract, PR sequence |
| Spec | Required behavior, acceptance, proof | Product rationale, PR sequence |
| ADR | Durable architecture or operating decision | Task list, current metric state |
| Plan | PR order, work items, proof commands, rollback | Product rationale, durable decisions |
| Active goal | Current machine-readable work item set | Generated status, long prose |
| Support tiers | Public claim proof and promotion requirements | Feature design |
| Policy ledgers | Exceptions, CI/policy intent, coverage, review dates | Broad architecture |

## Rules

1. One kind of truth belongs in one artifact.
2. One semantic artifact belongs in one PR unless the linked plan says otherwise.
3. Specs define behavior; plans define sequencing.
4. Proposals explain why; ADRs record durable decisions.
5. Active goals tell agents what to do now.
6. Generated status is updated by tools, not by hand.
7. Public claims require support-tier proof or an equivalent receipt pointer.
8. Policy exceptions require owner, reason, coverage, and review date.
9. Runtime/code PRs must link to the spec and plan item they implement.

## Required headers

Every proposal, spec, ADR, and implementation plan should declare source links
near the top of the file. Use `n/a` when a field does not apply.

```text
Status:
Owner:
Created:
Linked proposal:
Linked specs:
Linked ADRs:
Linked plan:
Linked issues:
Linked PRs:
Support-tier impact:
Policy impact:
```

Existing legacy documents may use older formats, but new source-of-truth
artifacts should follow this shape unless an ADR records a different convention.

## Agent workflow

Agents must:

1. Read `AGENTS.md` and any applicable nested agent instructions.
2. Read this file.
3. Read `.openracing/goals/active.toml` when it exists.
4. Read the linked implementation plan.
5. Read the linked proposal only for why.
6. Read the linked spec for acceptance.
7. Read linked ADRs for constraints.
8. Inspect git status before editing.
9. Pick exactly one ready work item.
10. Implement only that item.
11. Run the proof commands listed for that item.
12. Update receipts, status, or policy ledgers only when the work item requires it.
13. Stop instead of guessing when required source-of-truth artifacts are missing.

## Stop conditions

Stop and report instead of improvising if:

- the active goal is missing when the task requires one;
- linked files do not exist;
- linked specs or plan anchors are missing;
- generated status differs from committed status;
- proof commands cannot run;
- unrelated staged files exist;
- requested work conflicts with an ADR;
- a public claim lacks support-tier proof.

## Active goal lifecycle

The active goal manifest lives at:

```text
.openracing/goals/active.toml
```

Set `status = "active"` when a lane is selected. Set `status = "paused"` with a
reason when no lane is selected.

Archive completed or superseded active goals under:

```text
.openracing/goals/archive/YYYY-MM-DD-<lane>.toml
```

Do not leave multiple active goals.

## Closeout format

At the end of a lane, write:

```text
plans/<lane>/closeout.md
```

Use this shape:

````md
# Lane closeout: <lane>

Status: completed
Date:
Owner:
Linked proposal:
Linked specs:
Linked ADRs:
Linked plan:
Active goal archive:

## What shipped

## Proof

```bash
commands
```

## Receipts

PRs, CI runs, generated status, support-tier updates, and policy updates.

## What did not ship

## Deferred work

## Claim boundary

## Next lane recommendation
````

## Common failure modes

### Spec becomes a task list

Move PR order to `plans/<lane>/implementation-plan.md`; keep the spec to
behavior, examples, acceptance, and proof.

### Plan becomes product rationale

Move why/user pain to `docs/proposals/`; keep the plan focused on work items,
dependencies, proof, and rollback.

### Active goal becomes prose

Keep active goals in TOML, link out to docs, and avoid generated tables.

### Agent hand-edits generated status

Run the named generator/checker instead and record the command as proof.

### Support claims drift

Require support-tier impact on source artifacts and support-tier proof for public
claims.

### Policy exceptions become silent debt

Every exception should include owner, reason, `covered_by`, `review_after`, and
an expiry when temporary.

### Mega PR

Split by semantic artifact or by one implementation work item.

## What good looks like

A new contributor or agent can arrive cold and answer:

```text
What are we doing?
Why?
What must be true?
What decision constrains it?
What PR lands next?
What command proves it?
What may we claim?
What must we not claim?
```

If the repo answers those questions without chat history, the source-of-truth
system is working.
