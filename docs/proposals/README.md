# OpenRacing Proposals

Proposals are the source of truth for **why** a lane exists.

Use this directory for PRDs and lane proposals that explain user pain,
affected surfaces, success criteria, risks, alternatives considered, and the
evidence plan for a body of work. Proposals do not own detailed PR ordering,
current generated status, or implementation minutiae.

## Naming

Use stable, boring IDs:

```text
docs/proposals/OPENRACING-PROP-0001-<lane>.md
```

## Required shape

Every proposal should include these headers. Use `n/a` when a field does not
apply.

```md
# OPENRACING-PROP-0001: Lane title

Status: proposed
Owner:
Created:
Target milestone:
Linked specs:
Linked ADRs:
Linked plan:
Support/status impact:
Policy impact:

## Problem

## Users and surfaces

## Success criteria

## Proposed shape

## Alternatives considered

## Specs to create or update

## ADRs needed

## Implementation campaign shape

## Evidence plan

## Risks

## Non-goals

## Exit criteria

## Claim boundary
```

## Role in the source-of-truth stack

A proposal may link to specs, ADRs, plans, and active goals. It should not turn
into a task queue. If text describes exact PR sequencing, move it to
`plans/<lane>/implementation-plan.md`.
