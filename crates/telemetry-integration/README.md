# racing-wheel-telemetry-integration

Matrix coverage utilities for OpenRacing telemetry integration.

## Purpose

- Validate that telemetry game IDs present in the support matrix
  are represented in runtime registries.
- Produce deterministic reports of:
  - missing implementations (matrix entry without factory/writer)
  - extra implementations (factory/writer not present in matrix)
- Compare adapter and writer coverage in one structured report.
- Expose deterministic ratio-based coverage metrics for observability.

## API

- `compare_matrix_and_registry(...)`
  - Compare two ID collections and return a `RegistryCoverage` report.
- `compare_matrix_and_registry_with_policy(..., CoveragePolicy)`
  - Apply explicit completeness/strictness policies while still returning detailed coverage.
- `compare_runtime_registries_with_policies(...)`
  - Evaluate adapter and writer registries in a single report with individual policy outcomes.
- `RegistryCoverage`
  - Query whether coverage is exact and inspect drift details.
- `RegistryCoverage::metrics()`
  - Emit deterministic BDD/observability counters and ratios for one registry comparison.
- `RegistryCoverage::bdd_metrics(CoveragePolicy)`
  - Emit policy-aware BDD counters/ratios including `parity_ok` for one registry.
- `RuntimeCoverageReport`
  - Surface matrix, adapter, and writer parity snapshots plus policy checks.
- `RuntimeCoverageReport::metrics()`
  - Emit aggregate adapter+writer matrix metrics plus overall parity status.
- `RuntimeCoverageReport::bdd_metrics()`
  - Emit policy-aware adapter+writer BDD metric snapshots and overall runtime parity.
- `CoveragePolicy::is_satisfied`
  - Evaluate whether a `RegistryCoverage` satisfies a specific policy.

## Coverage policy helpers

- `CoveragePolicy::STRICT`: exact matrix/registry parity required.
- `CoveragePolicy::MATRIX_COMPLETE`: matrix IDs must be fully covered; extras are allowed and reported.
- `CoveragePolicy::LENIENT`: complete permissive coverage (used for diagnostics and startup logs).
