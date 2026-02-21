# Racing Wheel Telemetry Core

Core domain types for OpenRacing telemetry integration:

- `GameTelemetry` and `GameTelemetrySnapshot` for raw gameplay telemetry
- Disconnection detection and connection lifecycle primitives
- `TelemetryError`, `ConnectionState`, and `ConnectionStateEvent`
- Legacy adapter trait definitions (`GameTelemetryAdapter`) kept for compatibility

This crate is intentionally narrow in scope so higher-level services and adapters
can depend on it without pulling in service-level orchestration concerns.
