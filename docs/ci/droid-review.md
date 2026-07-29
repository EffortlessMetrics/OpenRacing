# Droid Auto Review

`Droid Auto Review` is an advisory pull request review workflow. It can provide
useful automated review comments, but it is backed by an external credit-based
service and is not a deterministic repository gate.

On 2026-05-08, the workflow failed across multiple documentation and tooling PRs
with a service-side `402 Payment Required` / usage-limit response. That failure
did not indicate a repository test failure.

The workflow therefore runs with `continue-on-error: true` for the automatic
review step. Required merge policy should rely on deterministic project checks
such as CI, schema validation, YAML sync, compatibility tracking, security and
license audit, integration tests, and coverage. Do not add `droid-review` as a
required status check for `main`.

If the team wants Droid review to become blocking later, first ensure the
external service has reliable credits, a stable model path, and an operational
runbook for service outages.

## Supported `@droid` command surface

This repository routes explicit commands only from trusted PR-linked comment paths:

- `@droid review`
- `@droid fill`
- `@droid security`

Issue events are intentionally not supported for command execution. The only
automatic path is `droid-review.yml`, which runs on non-fork pull requests
for opened/ready-for-review/reopened events.
