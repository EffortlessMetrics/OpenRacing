# racing-wheel-telemetry-bdd-metrics

Deterministic, policy-aware matrix parity metrics for OpenRacing telemetry BDD scenarios.

## Purpose

- Encode matrix-vs-registry parity counters used in telemetry acceptance criteria.
- Provide deterministic coverage ratios for dashboards and CI assertions.
- Evaluate parity outcomes with explicit policy controls (`STRICT`, `MATRIX_COMPLETE`, `LENIENT`).

## Core types

- `MatrixParityPolicy`:
  controls whether missing matrix IDs and extra registry IDs are allowed.
- `BddMatrixMetrics`:
  single-registry parity snapshot with spec-aligned counters and ratios.
- `RuntimeBddMatrixMetrics`:
  adapter + config-writer parity snapshots and combined runtime parity status.

