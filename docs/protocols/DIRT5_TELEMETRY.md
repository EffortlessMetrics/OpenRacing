# Dirt 5 Telemetry Support (Bridge-Backed)

## Summary

Dirt 5 does not expose a native telemetry export channel that OpenRacing can consume
through existing in-game configuration settings. OpenRacing supports Dirt 5 by consuming
an external telemetry provider bridge that emits Codemasters-style UDP packets.

Current implementation treats Dirt 5 as a **telemetry-only** source:

- `speed_ms`
- `rpm` (derived from `engine_rate` in rad/s)
- `gear`
- `slip_ratio` (derived from wheel patch speeds)
- Extended values for wheels/suspension/input channels

`ffb_scalar` is intentionally left unsupported until a bridge exposes a valid force
feedback request channel.

## Expected wire format

The bridge should provide packets compatible with Codemasters custom UDP semantics:

- Little-endian payload fields
- 4-byte per channel values (`uint32`, `int32`, `float`, optional `fourcc`)
- A `speed`, `engine_rate`, `gear` base field set
- Optional mode 1/2/3 field expansion (wheels/suspension/acceleration)

This is implemented by `crates/telemetry-adapters/src/codemasters_udp.rs`.

## Runtime contract

OpenRacing writes a bridge contract here:

- `Documents/OpenRacing/dirt5_bridge_contract.json`

Fields:

- `telemetry_protocol`: `codemasters_udp`
- `mode`: default `1`
- `udp_port`: UDP listen port (default `20777`)
- `update_rate_hz`: configured target update rate
- `enabled`: whether telemetry is enabled

The adapter listens on UDP port `20777` by default and auto-detects running state
from recent packet heartbeats.

## Configuration

Set environment overrides when needed:

- `OPENRACING_DIRT5_UDP_PORT`
- `OPENRACING_DIRT5_UDP_MODE` (0-3)
- `OPENRACING_DIRT5_CUSTOM_UDP_XML` (optional custom schema path)
- `OPENRACING_DIRT5_HEARTBEAT_TIMEOUT_MS`
