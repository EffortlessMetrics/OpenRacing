# RC Readiness Report

**Branch:** `feat/wave15-rc-hardening`
**Generated:** 2025-07-15
**Commit:** HEAD (wave 34)

## Test Summary

| Metric | Count |
|--------|------:|
| **Total tests** | **8,344+** |
| Unit tests | 6,200+ |
| Snapshot tests | 850+ |
| Property tests (proptest) | 780+ |
| End-to-end (E2E) tests | 460+ |
| BDD / acceptance tests | 27 |
| Fuzz targets | 85+ |
| Integration test files | 34 |

## Test Types Present

| Type | Files | Notes |
|------|------:|-------|
| Proptest files | 100+ | Property-based testing across all 17 protocol & engine crates |
| Snapshot test files | 47+ | `insta` snapshots for protocol encoding & telemetry |
| Integration test files | 34 | `crates/integration-tests/tests/*.rs` |
| Fuzz targets | 85+ | `fuzz/fuzz_targets/` — covers all protocols & telemetry parsers |
| Benchmark suites | 1 | `benches/` — RT timing benchmarks |

## Coverage by Crate Category

| Category | Tests | Key crates |
|----------|------:|------------|
| Telemetry | 1,050 | `telemetry-adapters`, `telemetry-core`, `telemetry-config`, `telemetry-orchestrator` |
| Engine | 795 | `engine` (RT pipeline, filters, HID, safety) |
| Protocols | 593 | `hid-*-protocol`, `simplemotion-v2`, `hbp`, `moza-wheelbase-report` |
| Plugins | 403 | `plugins`, `openracing-wasm-runtime`, `openracing-native-plugin`, `openracing-plugin-abi` |
| Service | 299 | `service` (daemon, IPC, crypto, firmware updates) |
| Schemas | 249 | `schemas` (JSON schema validation, migration, profile inheritance) |
| Integration tests | 241 | `integration-tests` (E2E device pipelines, RC validation) |
| Safety | 193 | `openracing-fmea`, `openracing-watchdog`, `openracing-hardware-watchdog` |
| Filters | 158 | `openracing-filters` (snapshot + property tests) |
| Other / utilities | 3,611 | Curves, crypto, calibration, errors, scheduler, IPC, tracing, etc. |

## Strengths

- **All 17 vendor protocol crates wired into engine dispatch**: Thrustmaster, Logitech,
  Fanatec, Simucube (1 & 2), Simagic, Moza, Asetek, VRS, Heusinkveld, AccuForce,
  OpenFFBoard, FFBeast, Leo Bodnar, Cube Controls, Cammus, and PXN — each with unit,
  snapshot, property, and E2E tests plus a dedicated fuzz target.
- **PXN protocol crate** (`hid-pxn-protocol`): VID `0x11FF`, 5 devices (V10, V12, GT987,
  and 2 additional models) — web-verified against Linux kernel `hid-ids.h`.
- **GT7 extended packet support**: 316/344-byte PacketType2 and PacketType3 implemented,
  adding wheel rotation, sway/heave/surge, energy recovery, and filtered throttle/brake.
- **Comprehensive proptest coverage**: all 17 protocol crates have property-based testing
  with 780+ proptest cases exercising encoding round-trips, ID mappings, and safety invariants.
- **Telemetry adapters** cover 40+ games with snapshot regression tests across multiple
  schema versions (v2–v9).
- **CLI, schemas, plugins, and engine** all have dedicated test suites.
- **Fuzz testing** covers 85+ targets spanning all protocol parsers and telemetry decoders.
- **Safety-critical paths** (FMEA, watchdog, hardware watchdog) have dedicated test suites
  including fault-injection and property tests.
- **RC-specific integration tests** exist (`rc_integration_tests.rs`, 48 tests).

## PID Verification Status

| Device | PID | Status | Notes |
|--------|-----|--------|-------|
| Fanatec GT DD Pro / ClubSport DD | `0x0020` | Confirmed | GT DD Pro and ClubSport DD share PID `0x0020` with CSL DD in PC mode |
| OpenFFBoard (alt) | `0xFFB1` | **SPECULATIVE** | Zero evidence across 5 sources; `0xFFB0` confirmed via pid.codes + firmware |
| Cube Controls | `0x0C73`–`0x0C75` | **UNVERIFIED** | Zero external evidence exists; OpenFlight uses different estimates |
| VRS DFP V2 | `0xA356` | **UNVERIFIED** | DFP uses `0xA355` (kernel mainline); Pedals use `0xA3BE`; V2 PID not in any source |

## Known Gaps

| Gap | Severity | Notes |
|-----|----------|-------|
| No line-level code coverage (e.g., `llvm-cov`) | Medium | Test count is high but uncovered branches are unknown |
| UI crate excluded from test run | Low | `racing-wheel-ui` excluded via `--exclude`; needs separate GUI test strategy |
| Benchmark suite is minimal | Low | Single bench file; RT timing validation relies on CI perf gates |
| `compat` crate has 0 unit tests | ~~Medium~~ Low | Compat crate now has tests added in waves 31-32 |
| Doc-tests not counted | Low | `cargo test` doc-tests run but are not enumerated in `--list` output |
| No mutation testing in CI | Low | `mutants.toml` exists but results are stale (`mutants.out.old/`) |
