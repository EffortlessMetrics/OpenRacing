# OpenRacing Specs

This directory is "how it should work" documentation: concrete enough to review against code, and strict enough to write tests against.

## Specs

- Telemetry integrations: `telemetry.md`
- Safety-critical FFB control loop: `ffb-safety.md`

## Conventions

- **MUST / SHOULD / MAY** language is intentional.
- Specs link to **implementation touchpoints** in `crates/...` so reviewers can trace behavior.
- Where vendors do not publish docs publicly, the spec points to the **authoritative shipped header/config** on a developer machine, and calls out any assumptions.
