# OpenRacing Specs

This directory is "how it should work" documentation: concrete enough to review against code, and strict enough to write tests against.

## Specs

- Telemetry integrations: `telemetry.md`
- Safety-critical FFB control loop: `ffb-safety.md`

## Conventions

- **MUST / SHOULD / MAY** language is intentional.
- Specs link to **implementation touchpoints** in `crates/...` so reviewers can trace behavior.
- Where vendors do not publish docs publicly, the spec points to the **authoritative shipped header/config** on a developer machine, and calls out any assumptions.

## Role in the source-of-truth stack

Specs are the source of truth for **what must be true**. They define required
behavior, acceptance examples, proof requirements, test mapping, implementation
mapping, CI proof, support-tier impact, and claim boundaries.

Specs should not own product motivation or exact PR order. Put why in
`docs/proposals/` and sequencing in `plans/<lane>/implementation-plan.md`. See
`docs/reference/SPEC_SYSTEM.md` for the repo-wide source-of-truth rules.

## New spec naming

Use stable IDs for new source-of-truth specs:

```text
docs/specs/OPENRACING-SPEC-0001-<behavior-contract>.md
```

Existing specs may keep their current filenames unless a plan explicitly calls
for migration.
