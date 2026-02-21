# racing-wheel-telemetry-config-writers

Game-specific telemetry configuration writers for OpenRacing.

## Purpose

- Writes stable game configuration files for supported simulators.
- Provides diffs and validation helpers for service integration and testing.
- Keeps game configuration behavior aligned with the shared support matrix.

## Exposed types

- `TelemetryConfig`
- `ConfigDiff`
- `DiffOperation`
- `ConfigWriter`
- `IRacingConfigWriter`, `ACCConfigWriter`, `ACRallyConfigWriter`, `AMS2ConfigWriter`, `RFactor2ConfigWriter`, `EAWRCConfigWriter`, `F1ConfigWriter`, `Dirt5ConfigWriter`

## Registry

- `config_writer_factories()` returns the canonical `&'static` registry of supported
  config-writer ID -> constructor pairs.
- `ConfigWriterFactory` is the constructor function pointer type for writers.

Use this registry for matrix-backed game integration setup.
