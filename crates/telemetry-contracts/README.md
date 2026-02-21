# racing-wheel-telemetry-contracts

Shared telemetry data contracts for OpenRacing.

This crate contains normalized telemetry domain types that are consumed by
services, adapters, and diagnostics code:

- `NormalizedTelemetry`
- `TelemetryFlags`
- `TelemetryValue`
- `TelemetryFrame`
- telemetry field coverage metadata structures

The crate is intentionally dependency-light so it can be reused across
RT-sensitive and non-RT components without importing full service internals.
