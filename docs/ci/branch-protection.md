# Main Branch Protection

This document records the required merge policy for `main`. It exists because
repository rulesets are configured in GitHub, not in this repository.

## Current Audit

Audited on 2026-05-08 with:

```powershell
gh api repos/EffortlessMetrics/OpenRacing/branches/main/protection
gh api repos/EffortlessMetrics/OpenRacing/rulesets
gh api repos/EffortlessMetrics/OpenRacing/rulesets/12099933
```

Findings:

- Classic branch protection for `main` is not enabled.
- Repository ruleset `main` (`12099933`) is active for the default branch.
- The ruleset blocks branch deletion.
- The ruleset blocks non-fast-forward updates.
- The ruleset requires pull requests.
- The ruleset does not currently require status checks to pass before merge.

That last point is the operational gap: a pull request can merge while long CI
jobs are still pending if a user or tool runs a merge command.

## Required Policy

`main` must not accept a pull request until required checks have completed and
passed. This is especially important for hardware receipt PRs, where a premature
merge can make unvalidated evidence look accepted by the project history.

The `main` ruleset should include a required status check rule with stale-check
protection enabled. In the GitHub UI, configure:

- Rulesets -> `main` -> Rules -> Require status checks to pass.
- Enable "Require branches to be up to date before merging" if available.
- Add each required check by its exact status-check name.
- Keep pull requests required.
- Keep deletion and non-fast-forward protection enabled.
- Do not grant bypass actors for routine project work.

## Required Checks

At minimum, the required checks for `main` should include:

- `CI`
- `Code Coverage`
- `Regression Prevention`
- `Integration Tests`
- `Schema Validation`
- `YAML Sync Check`
- `Compatibility Layer Usage Tracking`
- `Security & License Audit`

If GitHub exposes only job-level contexts rather than workflow-level contexts,
require the corresponding blocking jobs from those workflows. Do not require
informational, skipped, or advisory bot checks unless the repository explicitly
depends on them for merge safety.

## Hardware Receipt PRs

Hardware receipt PRs must follow the same merge policy as code PRs. A Moza R5
receipt PR is not merge-ready while any required check is pending, even if the
receipt verifier artifacts are present.

For hardware PR review, also confirm:

- The PR claim ceiling matches the receipt stage.
- No staged receipt is missing from the lane manifest.
- No hardware validation boolean is promoted without matching receipts.
- No high-torque, serial configuration, firmware, or DFU claim is introduced by
  passive or zero-output receipt PRs.

## Verification Commands

Before merging a PR, use:

```powershell
gh pr checks <pr-number>
gh pr view <pr-number> --json mergeStateStatus,state,isDraft,headRefOid
```

The PR is merge-ready only when required checks are passing and GitHub reports a
mergeable state. Do not use `gh pr merge --auto` as a substitute for enforced
required checks; if the ruleset is incomplete, it can merge immediately.
