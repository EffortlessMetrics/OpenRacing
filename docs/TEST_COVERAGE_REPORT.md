# Test Coverage Report

**Branch:** `feat/wave15-rc-hardening`
**Generated:** 2026-03-04
**Waves completed:** 15–46

---

## Summary

| Metric | Count |
|--------|------:|
| **Total tests** | **22,326+** |
| Failed | 0 |
| Ignored | 44 |
| Test binaries | 640+ |
| Test files (`crates/**/tests/**/*.rs`) | 662 |
| Crates with test files | 79 / 82 |

---

## Tests by Category

| Category | Count | Notes |
|----------|------:|-------|
| Unit tests | 15,800+ | Standard `#[test]` functions across all crates |
| Property tests (proptest) | 2,500+ | 360+ `proptest!` blocks generating multiple test cases |
| Snapshot tests (insta) | 1,327 files | Across 52 snapshot directories; ~1,100+ running test cases |
| Doc-tests | 490+ | `/// ```rust` examples in public API documentation |
| Integration tests | 890+ | `crates/integration-tests/tests/*.rs` (cross-crate validation) |
| Protocol verification | 400+ | Cross-verified against community sources (waves 31-33) |
| Golden-packet tests | 72+ | End-to-end adapter validation against known-good captures |
| BDD / acceptance tests | 73+ | 24 `.feature` files; Given/When/Then behavior scenarios |
| Concurrency stress tests | 23 | Multi-threaded scenarios with barrier sync (wave 34) |
| Compile-fail (trybuild) | 20 | Type-safety invariants enforced at API boundaries |
| Performance validation | 12 | RT timing checks — 1kHz sustained throughput (wave 34) |
| Safety soak tests | 5+ | 10K+ tick sustained operation under fault injection |
| Benchmarks | 1 | `benches/` — RT timing benchmarks |

---

## Coverage by Crate

| Crate / Category | Tests | Key crates |
|------------------|------:|------------|
| Telemetry | 4,650+ | `telemetry-adapters` (2,270+), `telemetry-core` (492+), `telemetry-config` (293+), `telemetry-orchestrator` (127), `telemetry-contracts` (125), `telemetry-config-writers` (48+), `telemetry-streams` (52+) — extended verification for 9 adapters (wave 34), core/integration/rate-limiter deep (wave 37), adapter re-verification (wave 40), config/streams deep (wave 40), 61 adapters verified (wave 43), adapter validation (wave 45) |
| HID Protocols | 4,231+ | `hid-thrustmaster-protocol` (480+), `hid-fanatec-protocol` (370+), `hid-simagic-protocol` (292+), `hid-logitech-protocol` (270+), `hid-moza-protocol` (314+), `hid-vrs-protocol` (217+), `hid-cammus-protocol` (188+), `hid-ffbeast-protocol` (136+), `hid-openffboard-protocol` (153+), `simucube-protocol` (310+), `simplemotion-v2` (79+), `hbp` (43+), `moza-wheelbase-report` (59+), and more — ALL 14 crates cross-verified, Simagic deep (wave 38), roundtrip proptests across 9 crates (wave 44) |
| Engine | 1,676+ | `racing-wheel-engine` — RT pipeline, filters, HID dispatch, safety, device/game, hot-swap, FFB pipeline E2E, HID common deep (wave 36), safety + device management deep (wave 41), RT no-allocation enforcement (wave 44) |
| Integration tests | 888+ | `integration-tests` — E2E device pipelines, RC validation, golden packets, full-stack E2E, concurrency stress (23), performance validation (12), plugin + telemetry E2E + device protocol (wave 40) |
| Service + CLI | 1,056+ | `racing-wheel-service` (587+), `wheelctl` (508+) — daemon, IPC, lifecycle, CLI E2E, diagnostics deep (wave 35), service lifecycle + IPC deep (wave 41), service lifecycle (wave 45), CLI deep (wave 46) |
| Schemas | 721+ | `racing-wheel-schemas` — JSON schema validation, migration, profile inheritance, evolution, domain type proptests (wave 36), schema validation deep (wave 41), IPC schema compat (wave 44) |
| Plugins | 976+ | `racing-wheel-plugins` (394+), `openracing-wasm-runtime` (232+), `openracing-native-plugin` (217+), `openracing-plugin-abi` (263+) — WASM deep (wave 38), native plugin + ABI deep (wave 39), example plugin tests (51) (wave 43) |
| Errors | 425 | `openracing-errors` — exhaustive error variant coverage, error handling deep (wave 45) |
| Compat + Config | 312+ | `compat` — deep migration + validation tests (wave 33), migration deep (wave 41) |
| Profile | 475+ | `openracing-profile` (269+), `openracing-profile-repository` (94+) — inheritance, validation, comprehensive system tests (wave 35), profile + repo deep (wave 40) |
| Filters | 334+ | `openracing-filters` — snapshot + property tests, SM-V2 deep, filters deep (wave 39) |
| Safety | 674+ | `openracing-fmea` (371+), `openracing-watchdog` (58+), `openracing-hardware-watchdog` (286+) — fault injection, property tests, 23 safety invariants (wave 30), crypto + FMEA deep (wave 39), watchdog deep (wave 39), fault injection expansion (wave 44) |
| IPC | 261+ | `openracing-ipc` — message serialization, snapshot round-trips, codec proptests (wave 36), IPC protocol deep (wave 41) |
| FFB + Calibration | 656+ | `openracing-ffb` (366+), `openracing-calibration` (290+) — force output, profile application, workflows, migration, serde proptests (wave 36), FFB + calibration deep (wave 41), calibration + FFB precision (wave 46) |
| Curves | 169+ | `openracing-curves` — LUT fidelity, interpolation, bezier, fitting, monotonicity, stability (wave 35) |
| Firmware | 230+ | `openracing-firmware-update` — state machine, rollback, validation, firmware update deep (wave 41) |
| Capture | 379+ | `hid-capture` — device capture tooling, fingerprinting, classification (wave 34), diagnostic + SRP + capture deep (wave 38), capture IDs (wave 41) |
| Pipeline | 179+ | `openracing-pipeline` — filter chains, edge cases, proptests (wave 36), pipeline deep (wave 39) |
| Tracing | 120+ | `openracing-tracing` — drop rate, emission verification, spans, formats, snapshots (wave 35) |
| Scheduler | 189+ | `openracing-scheduler` — priority inversion, deadline handling, RT setup, PLL, jitter metrics, adaptive scheduling (wave 36) |
| Diagnostic | 404+ | `openracing-diagnostic` — insta snapshots, diagnostics deep (wave 35), diagnostic + SRP deep (wave 38) |
| Atomic | 198+ | `openracing-atomic` — concurrent stress, ordering guarantees, counters, snapshots, streaming stats (wave 36) |
| Crypto | 178+ | `openracing-crypto` — signing property tests, crypto deep (wave 39), crypto + signing verification (wave 46) |
| Other utilities | 1,240+ | `openracing-shifter` (178+), `openracing-handbrake` (73+), `openracing-device-types` (75+), `input-maps` (67+), `openracing-ks` (83+), `openracing-capture-ids` (45+), `openracing-test-helpers` (149+), `openracing-support` (25+), `compat`, `changelog`, etc. — peripherals deep (wave 37), test helpers (wave 41), device discovery (wave 45), replay + diagnostics (wave 46) |

---

## Fuzz Targets

| Metric | Count |
|--------|------:|
| **Total fuzz targets** | **113** |
| Location | `fuzz/fuzz_targets/*.rs` |
| Coverage | All 17 HID protocol crates + all game telemetry adapters + replay, diagnostics, calibration, FFB, crypto, CLI |
| New in wave 31 | telemetry packet, profile, calibration, filter pipeline |
| New in wave 46 | replay parsing, diagnostic export, calibration input, FFB commands, crypto payloads, CLI argument parsing |

---

## Snapshot Tests

| Metric | Count |
|--------|------:|
| **Snapshot files (`.snap`)** | **1,327** |
| **Snapshot directories** | **52** |
| Coverage | Protocol encoding, telemetry normalization, diagnostic output, filter state, tracing formats |

---

## Excluded from Test Run

| Crate | Reason |
|-------|--------|
| `racing-wheel-ui` | GUI crate — excluded via `--exclude`; needs separate GUI test strategy |

---

## Test Infrastructure

- **Test helpers:** `openracing-test-helpers` crate with shared utilities
- **Proptest regressions:** tracked via `proptest-regressions/` directories
- **Insta snapshots:** `cargo insta review` workflow for snapshot management
- **Trybuild:** compile-fail tests in `crates/*/tests/compile_fail/`
- **BDD features:** `.feature` files in `crates/telemetry-bdd-metrics/`
- **Fuzz corpus:** seed corpus in `fuzz/corpus/` for all 113 targets

---

## Protocol Verification Status (Waves 31-33)

ALL 14 HID crates cross-verified against community sources:

| HID Crate | Tests | Verified Against | Wave |
|-----------|------:|------------------|------|
| Thrustmaster | 59 | kernel `hid-thrustmaster` | 31 |
| Moza | 49 | boxflat, Linux kernel | 31 |
| Logitech | 45 | kernel `hid-lg4ff` | 31 |
| Fanatec | 45 | `hid-fanatecff`, Wine | 31 |
| SimuCube | — | Official docs, pid.codes | 31 |
| OpenFFBoard | — | pid.codes, firmware source | 31 |
| AccuForce | — | Community databases | 33 |
| Asetek | — | Community databases | 33 |
| Button Box | — | pid.codes, Arduino community | 33 |
| Cammus | — | Community sources | 33 |
| Cube Controls | — | Community databases | 33 |
| FFBeast | — | Community databases | 33 |
| Leo Bodnar | — | Vendor documentation | 33 |
| VRS | — | Kernel mainline | 33 |

---

## Wave 34-35 Test Additions

| Commit | Tests | Description |
|--------|------:|-------------|
| Concurrency stress | 23 | Multi-threaded scenarios — device state, telemetry, safety, IPC, atomics, watchdog |
| Performance validation | 12 | RT timing checks — filter processing, pipeline latency, 1kHz throughput |
| Device capture tooling | 83 | HID descriptor parsing, USB enumeration, fingerprinting, classification |
| Extended telemetry verification | 110 | 9 adapters verified against authoritative SDK sources |
| Service diagnostics | 40 | Diagnostic types, health scoring, export, error rate tracking |
| Profile system | 64 | Creation, inheritance, validation, import/export, merge, versioning |
| Tracing deep | 21 | Spans, events, formats, async, rate limiting, snapshots |
| Curves deep | 45 | Interpolation, bezier, fitting, monotonicity, stability |
| Calibration deep | 24 | Workflows, recalibration, validation, migration, pedal curves |

## Wave 36-37 Test Additions

| Commit | Tests | Description |
|--------|------:|-------------|
| FFB/pipeline/schemas/IPC proptests | 72 | Serde roundtrips, torque sign, gain monotonicity, domain types, codec validation |
| HID common deep | 72 | Device info, report parser/builder, mock devices, error handling |
| Scheduler deep | 79 | RT setup, PLL, jitter metrics, adaptive scheduling |
| Atomic deep | 100 | Counters, snapshots, streaming stats, concurrent queues |
| Input maps + KS | 150 | Button/axis/rotary binding, compilation, KS axis/bit/byte sources, report layout |
| SimpleMotion V2 verification | 79 | Command encoding, CRC polynomial, status/fault registers, VID/PID |
| Doc-tests (5 crates) | ~58 | openracing-ffb, filters, pipeline, calibration, ipc — compilable API examples |
| Telemetry core/integration/rate-limiter | 152 | GameTelemetry, NormalizedTelemetry, RegistryCoverage, drop-rate, burst patterns |
| HBP + Moza wheelbase report | 102 | Layout inference, byte order, axis decoding, report ID validation, endianness |
| Peripherals deep | ~80 | Handbrake position/calibration, shifter gear/multi-gate, device-types identification |
| BDD scenarios | 13 | 8 device scenarios (Moza/Fanatec/Logitech/Thrustmaster/SimuCube/OpenFFBoard), 5 game scenarios |

## Wave 38-41 Test Additions

| Commit | Tests | Description |
|--------|------:|-------------|
| Simagic protocol verification + deep | 106 | Protocol verification (38) + comprehensive deep tests (68) |
| WASM runtime deep | 54 | Plugin loading, execution, sandboxing, error recovery |
| Diagnostic + SRP + capture deep | 251 | Diagnostic infrastructure, SRP protocol, capture tooling |
| Forza + support deep | 115 | Forza adapter verification (90) + support utilities (25) |
| Native plugin + plugin ABI deep | 171 | Native plugin loading/isolation (90) + ABI compatibility (81) |
| Crypto + FMEA deep | 102 | Cryptographic verification (52) + FMEA fault injection (50) |
| Filters + pipeline deep | 163 | Filter processing chains (101) + pipeline orchestration (62) |
| Watchdog deep | 139 | Software watchdog (58) + hardware watchdog (81) |
| Integration E2E expansion | 67 | Plugin (23) + telemetry E2E (22) + device protocol E2E (22) |
| Telemetry adapter re-verification | 374 | AMS2, F1, Rennsport, SimHub, RaceRoom, LFS, KartKraft, MudRunner, WRC |
| Profile + repo + config writers deep | 239 | Profile system (97) + profile repository (94) + config writers (48) |
| Telemetry config + streams deep | 125 | Telemetry config (73) + telemetry streams (52) |
| FFB + calibration deep | 191 | FFB force output (107) + calibration workflows (84) |
| Service lifecycle + IPC deep | 74 | Service lifecycle (37) + IPC channel management (37) |
| Engine safety + device management deep | 129 | Safety subsystem (76) + device management (53) |
| Schemas + IPC protocol deep | 173 | Schema validation (97) + IPC protocol (76) |
| Compat + firmware update deep | 111 | Migration compatibility (40) + firmware update process (71) |
| Capture IDs + test helpers deep | 194 | Capture ID lookup (45) + test helper utilities (149) |

## Wave 43 Test Additions

| Commit | Tests | Description |
|--------|------:|-------------|
| CI gate verification | — | `cargo fmt`, `cargo deny`, ADR validation all verified passing |
| Workspace-hack sync | — | Workspace-hack crate verified in sync |
| Game support matrix | ~70 | 61 telemetry adapters verified with test coverage (5 new adapters) |
| Udev rules expansion | — | +75 udev rules for new device support |
| Example plugin tests | 51 | Plugin lifecycle, loading, sandboxing, error recovery |
| Docs alignment fixes | — | ADR and developer guide alignment |

## Wave 44-46 Test Additions

| Commit | Tests | Description |
|--------|------:|-------------|
| RT no-allocation enforcement | 36 | Dedicated tests verifying zero heap allocations in RT code paths |
| Safety fault injection | 74 | Expanded interlock, watchdog, and FMEA fault injection scenarios |
| Protocol roundtrip proptests | 104 | Property-based roundtrip verification across 9 protocol crates |
| IPC schema compat | 64 | Backward/forward compatibility validation for IPC schema evolution |
| Service lifecycle | 87 | Comprehensive start/stop/restart/recovery/state-machine coverage |
| Cross-platform | 60 | Platform-specific behavior validation across Windows, Linux, macOS |
| Telemetry adapter validation | 119 | Extended adapter verification with edge-case and error-path coverage |
| Error handling | 86 | Exhaustive error propagation and recovery path validation |
| Device discovery | 84 | Enumeration, hot-plug detection, multi-vendor discovery scenarios |
| Replay + diagnostics | 73 | Session replay, diagnostic export, health scoring, timeline reconstruction |
| Calibration + FFB | 91 | Calibration workflow edge cases, FFB force output precision, profile application |
| Crypto + signing | 47 | Ed25519 signing verification, key management, signature validation |
| CLI deep | 68 | Extended subcommand coverage, argument parsing, output formatting, error reporting |
| Fuzz targets | +9 | Replay parsing, diagnostic export, calibration input, FFB commands, crypto payloads, CLI argument parsing (113 total) |

---

*Source: `cargo test --workspace --all-features --exclude racing-wheel-ui` · waves 15–46 complete*
