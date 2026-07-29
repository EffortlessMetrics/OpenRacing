# Droid Auto Review

## Command surfaces

- **Droid Auto Review** (`.github/workflows/droid-review.yml`) runs automatically on PR open/reopen events and keeps the action non-blocking.
- **Droid Tag** (`.github/workflows/droid.yml`) reacts to PR review comments and PR review body comments only.

Supported `@droid` commands on PRs are those handled by the underlying Droid action:
`@droid review`, `@droid fill`, and configured security variants.
Issue events are not used for execution.

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
