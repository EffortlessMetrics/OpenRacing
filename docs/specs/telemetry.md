# Telemetry Integration Specification

This document defines:
- The **contract** between game-specific telemetry adapters and the OpenRacing service.
- The **normalized schema** we emit internally.
- The **wire-level requirements** we follow for each integration (UDP/shared memory), including configuration.

Where a vendor's primary documentation is not publicly accessible, this spec is written to:
1) match what we implement today, and
2) make the remaining unknowns explicit so they can be verified against the vendor-shipped SDK/header.

---

## 1) Architecture and contracts

### 1.1 Components

- **TelemetryService (service/daemon)** orchestrates polling/receiving and exposes telemetry to the rest of the system.
- **TelemetryAdapter (per game)** owns:
  - transport (UDP, shared memory),
  - parsing,
  - mapping into the normalized model,
  - detection ("is game running?").

Implementation touchpoints:
- `crates/service/src/telemetry/mod.rs` (adapter registration)
- `crates/service/src/telemetry/adapters/*` (per-game)
- `crates/service/src/telemetry/normalized.rs` (schema)

### 1.2 Adapter runtime contract (MUST)

An adapter MUST:
- Emit `TelemetryFrame`s containing:
  - a `NormalizedTelemetry` payload,
  - a monotonically increasing `sequence` (local to adapter task),
  - a timestamp (ns), and
  - raw input length (bytes) where applicable.
- Never panic on malformed input.
- Treat parse failures as **drop + log**, not process termination.
- Be resilient to "game present but idle" (timeouts are expected).

An adapter SHOULD:
- Avoid unbounded allocations per packet.
- Emit updates at its `expected_update_rate()` (best effort).

---

## 2) Normalized telemetry model

### 2.1 Core fields

| Field | Type | Units | Notes |
|---|---:|---|---|
| `speed_ms` | `f32` | m/s | MUST be meters per second. |
| `rpm` | `f32` | rpm | Engine RPM where available. |
| `gear` | `i8` | n/a | Conventional: -1 reverse, 0 neutral, 1..n forward. |
| `ffb_scalar` | `f32` | -1..1 | Best-effort normalized steering/FFB proxy. Not safety-critical. |
| `slip_ratio` | `f32` | 0..1 | Best-effort "how much wheel vs ground speed diverges." |
| `car_id` | `String` | n/a | Stable-ish identifier if available. |
| `track_id` | `String` | n/a | Stable-ish identifier if available. |
| `flags` | struct | n/a | In pits, flags, etc. |

### 2.2 Extended fields

Adapters MAY attach additional fields via `extended: HashMap<String, TelemetryValue>`.

Rules:
- Extended keys MUST be stable strings.
- Values MUST be typed (`Integer`, `Float`, `Bool`, `String` where supported).
- Extended fields MUST NOT be required for core functionality.

---

## 3) Game integrations

## 3.1 Assetto Corsa Competizione (ACC)

### 3.1.1 Transport and config

Transport: **UDP** "broadcasting" protocol.

Config file (Windows default):
- `Documents/Assetto Corsa Competizione/Config/broadcasting.json`

Keys OpenRacing expects to manage:
- `updListenerPort` (typo preserved by ACC)
- `broadcastingPort`
- `connectionId`
- `connectionPassword`
- `commandPassword`
- `updateRateHz`

OpenRacing default:
- `output_target = "127.0.0.1:9000"` for ACC unless overridden by configuration.

> NOTE: Some ACC clients configure separate "listener" and "broadcasting" ports. OpenRacing's default stance is to keep this simple: use a single port unless you have a known reason not to.

Implementation touchpoints:
- `crates/service/src/telemetry/adapters/acc.rs`
- `crates/service/src/config_writers.rs` (writes `broadcasting.json`)
- Fixture conformance: `crates/service/tests/fixtures/acc/*.bin`

### 3.1.2 Wire format (MUST)

Strings are encoded as:
- `u16` little-endian length (byte count),
- followed by UTF-8 bytes.

Registration request (client -> game) is encoded as:
- `u8` message type = REGISTER
- `u8` protocol version
- `string` display name
- `string` connection password
- `i32` update interval (ms), little-endian
- `string` command password

Inbound messages begin with:
- `u8` message type, followed by a type-specific payload.

OpenRacing parses (at minimum):
- Registration result
- Realtime update
- Realtime car update
- Track data
- Entry list / events (best-effort; primarily for enrichment)

### 3.1.3 Normalization mapping (SHOULD)

For realtime car update:
- `speed_ms = speed_kmh / 3.6`
- `gear = (gear_raw - 2)` (ACC commonly encodes as gear+2)
- Flags:
  - `in_pits` and `pit_limiter` derived from location enum where available.
- Extend:
  - positions, lap count, delta, temps/wetness where available

### 3.1.4 Conformance tests (MUST)

OpenRacing MUST maintain:
- Unit test for registration packet layout.
- Fixture-backed decode tests for a realistic message sequence:
  - track data -> realtime update -> realtime car update -> normalized output.
- Truncation test: truncated packets must fail cleanly (error, not panic).

(These are already represented by `include_bytes!` fixture tests in `acc.rs`.)

---

## 3.2 iRacing

### 3.2.1 Transport and platform constraints

Transport: **Windows shared memory (memory-mapped file)**.

Non-Windows behavior:
- Adapter SHOULD return "not running" and SHOULD NOT crash.

### 3.2.2 Win32 mapping rules (MUST)

The shared memory mapping MUST be opened read-only using:
- `OpenFileMappingW(dwDesiredAccess = FILE_MAP_READ, ...)`
- `MapViewOfFile(..., dwDesiredAccess = FILE_MAP_READ, ...)`

Do not confuse:
- FILE mapping access flags (`FILE_MAP_READ`) with
- file share flags (`FILE_SHARE_READ`).

### 3.2.3 IRSDK buffer semantics (MUST)

The mapping contains:
- a header at base,
- rotating telemetry buffers described by header metadata.

OpenRacing MUST:
- read the header,
- select the newest buffer by tick count,
- perform a "stable read" check (header/buffer consistent across two reads),
- decode variables using the variable header table rather than overlaying an invented struct.

OpenRacing SHOULD:
- tolerate missing variables (treat as 0 / empty) to avoid breaking across IRSDK revisions.

Implementation touchpoints:
- `crates/service/src/telemetry/adapters/iracing.rs`

### 3.2.4 Normalization mapping (SHOULD)

Variables of interest (names subject to IRSDK header verification on a dev machine):
- SessionTime, SessionFlags
- Speed, RPM, Gear
- Throttle, Brake
- SteeringWheelAngle, SteeringWheelTorque
- OnPitRoad, FuelLevel, Lap, LapBestLapTime
- CarPath, TrackName (when exposed)

Mapping:
- `speed_ms` MUST be m/s in normalized form.
- `ffb_scalar` is best-effort (not safety-critical). If torque is in Nm, normalize consistently (e.g., divide by a configured max).

### 3.2.5 Conformance tests (MUST)

OpenRacing MUST keep deterministic tests for:
- buffer selection (highest tick wins),
- rotated-buffer read behavior using a synthetic memory image.
- decoding resilience for variable iRacing payload sizes, including minimum legacy payload and full payload layouts.

---

## 3.3 Automobilista 2 (AMS2)

Status: **experimental / best-effort**.

Transport: PCARS-style shared memory (commonly referenced as `$pcars2$`).

Authoritative schema:
- The shipped `SharedMemory.h` in the AMS2 install directory is the source of truth.

Spec requirement:
- The Rust struct/layout MUST be generated from or manually matched to that header.
- Torn-read avoidance MUST follow the header's published sequencing/version fields (if present).

Implementation touchpoints:
- `crates/service/src/telemetry/adapters/ams2.rs`

---

## 3.4 rFactor 2

Status: **experimental / best-effort**.

Transport: shared memory, typically exposed by a plugin.

Spec requirements:
- Adapter MUST clearly distinguish:
  - "game running but plugin missing" vs
  - "plugin present but no frames."

Naming:
- Shared memory names may be fixed or PID-suffixed depending on plugin/environment.
- Prefer dedicated force feedback maps when available (do not infer torque from unrelated signals).

Implementation touchpoints:
- `crates/service/src/telemetry/adapters/rfactor2.rs`

---

## 4) Config writers

Config writers MUST:
- Be idempotent.
- Preserve unknown fields where practical.
- Write the actual files (not just "diffs"), while still returning a diff summary.

### 4.1 ACC

- Write `broadcasting.json` with:
  - `updListenerPort` derived from `TelemetryConfig.output_target` (port),
  - `broadcastingPort` defaulted sensibly,
  - credentials fields present (possibly empty),
  - `updateRateHz` from config.

### 4.2 iRacing

- Edit `Documents/iRacing/app.ini`, ensuring:
  - `[Telemetry]` section exists
  - `telemetryDiskFile=1` when enabled.

Implementation touchpoint:
- `crates/service/src/config_writers.rs`

---

## 5) Troubleshooting checklist

### ACC: no frames
- Verify `broadcasting.json` exists and has `updListenerPort`.
- Ensure the port matches the service configuration (default 9000).
- Restart ACC after edits (some setups only read config at startup).

### iRacing: not detected / no frames
- Windows only: confirm you're running on Windows.
- Confirm the mapping can be opened read-only with FILE_MAP_READ.
- Verify variable names against the installed IRSDK header if values stay zero.

---

## References

R1 (Win32): File mapping access rights and `FILE_MAP_READ`.
- https://learn.microsoft.com/en-us/windows/win32/memory/file-mapping-security-and-access-rights

R2 (Win32): `MapViewOfFileEx` parameters (`dwDesiredAccess` uses `FILE_MAP_*`).
- https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-mapviewoffileex

R3 (Win32): `OpenFileMappingW` (`dwDesiredAccess` uses `FILE_MAP_*`).
- https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-openfilemappingw

R4 (ACC): Broadcasting protocol sample code (message layouts, string encoding).
- https://raw.githubusercontent.com/angel-git/acc-broadcasting/master/BroadcastingNetworkProtocol.cs

R5 (ACC): Broadcasting enums (message type enums).
- https://raw.githubusercontent.com/angel-git/acc-broadcasting/master/BroadcastingEnums.cs
