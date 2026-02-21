# F1 Telemetry Support (Bridge-Backed)

## Summary

OpenRacing currently supports F1 telemetry through a bridge-backed Codemasters-style
UDP source. This path is designed to normalize core driving channels and expose
F1-specific status data (DRS/ERS/fuel/session fields) when present.

Current implementation provides:

- `speed_ms`
- `rpm`
- `gear`
- `slip_ratio`
- Flags where present: `drs_available`, `drs_active`, `ers_available`, `pit_limiter`, `in_pits`
- Extended values for all decoded channels

## Expected wire format

The bridge should emit packets compatible with the Codemasters custom UDP decoder:

- Little-endian payload fields
- 4-byte channel values (`uint32`, `int32`, `float`, optional `fourcc`)
- Core channels for speed/RPM/gear
- Optional F1 channels (DRS, ERS, fuel, tyres, session metadata)

Decoder implementation:

- `crates/telemetry-adapters/src/codemasters_udp.rs`
- `crates/telemetry-adapters/src/f1.rs`

## Runtime contract

OpenRacing writes a bridge contract at:

- `Documents/OpenRacing/f1_bridge_contract.json`

Contract fields:

- `game_id`: `f1`
- `telemetry_protocol`: `codemasters_udp`
- `mode`: default `3`
- `udp_port`: UDP listen port (default `20777`)
- `update_rate_hz`: configured target rate
- `enabled`: telemetry enabled flag

## Configuration

Optional environment overrides:

- `OPENRACING_F1_UDP_PORT`
- `OPENRACING_F1_UDP_MODE` (0-3)
- `OPENRACING_F1_CUSTOM_UDP_XML` (optional custom schema path)
- `OPENRACING_F1_HEARTBEAT_TIMEOUT_MS`
