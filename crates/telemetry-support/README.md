# racing-wheel-telemetry-support

Shared telemetry metadata crate for OpenRacing.

## Purpose

- Owns game support matrix schema and canonical data source (`game_support_matrix.yaml`).
- Supplies normalized game metadata for service integration and process auto-detection.

## Provided types

- `GameSupportMatrix`
- `GameSupport`
- `GameVersion`
- `TelemetrySupport`
- `TelemetryFieldMapping`
- `AutoDetectConfig`
- `load_default_matrix`
- `normalize_game_id`

