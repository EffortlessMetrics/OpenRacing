# Test Coverage Report

**Branch:** `feat/wave15-rc-hardening`
**Generated:** 2025-07-24
**Waves completed:** 15–35

---

## Summary

| Metric | Count |
|--------|------:|
| **Total tests** | **17,696+** |
| Failed | 0 |
| Ignored | 44 |
| Test binaries | 580+ |
| Test files (`crates/**/tests/**/*.rs`) | 556 |
| Crates with test files | 79 / 82 |

---

## Tests by Category

| Category | Count | Notes |
|----------|------:|-------|
| Unit tests | 12,200+ | Standard `#[test]` functions across all crates |
| Property tests (proptest) | 2,000+ | 348+ `proptest!` blocks generating multiple test cases |
| Snapshot tests (insta) | 1,327 files | Across 52 snapshot directories; ~1,100+ running test cases |
| Doc-tests | 432+ | `/// ```rust` examples in public API documentation |
| Integration tests | 786+ | `crates/integration-tests/tests/*.rs` (cross-crate validation) |
| Protocol verification | 400+ | Cross-verified against community sources (waves 31-33) |
| Golden-packet tests | 72+ | End-to-end adapter validation against known-good captures |
| BDD / acceptance tests | 60+ | 24 `.feature` files; Given/When/Then behavior scenarios |
| Concurrency stress tests | 23 | Multi-threaded scenarios with barrier sync (wave 34) |
| Compile-fail (trybuild) | 20 | Type-safety invariants enforced at API boundaries |
| Performance validation | 12 | RT timing checks — 1kHz sustained throughput (wave 34) |
| Safety soak tests | 5+ | 10K+ tick sustained operation under fault injection |
| Benchmarks | 1 | `benches/` — RT timing benchmarks |

---

## Coverage by Crate

| Crate / Category | Tests | Key crates |
|------------------|------:|------------|
| Telemetry | 3,685+ | `telemetry-adapters` (1,710+), `telemetry-core` (376), `telemetry-config` (220), `telemetry-orchestrator` (127), `telemetry-contracts` (125), `telemetry-config-writers`, `telemetry-streams` — extended verification for 9 adapters (wave 34) |
| HID Protocols | 3,840+ | `hid-thrustmaster-protocol` (480+), `hid-fanatec-protocol` (370+), `hid-simagic-protocol` (224), `hid-logitech-protocol` (270+), `hid-moza-protocol` (314+), `hid-vrs-protocol` (217+), `hid-cammus-protocol` (188+), `hid-ffbeast-protocol` (136+), `hid-openffboard-protocol` (153+), `simucube-protocol` (310+), and 7 more — ALL 14 crates cross-verified |
| Engine | 1,386+ | `racing-wheel-engine` — RT pipeline, filters, HID dispatch, safety, device/game, hot-swap, FFB pipeline E2E |
| Integration tests | 821+ | `integration-tests` — E2E device pipelines, RC validation, golden packets, full-stack E2E, concurrency stress (23), performance validation (12) |
| Service + CLI | 790+ | `racing-wheel-service` (426+), `wheelctl` (440+) — daemon, IPC, lifecycle, CLI E2E, diagnostics deep (wave 35) |
| Schemas | 531+ | `racing-wheel-schemas` — JSON schema validation, migration, profile inheritance, evolution |
| Plugins | 518+ | `racing-wheel-plugins` (394+), `openracing-wasm-runtime` (178), `openracing-native-plugin` (127), `openracing-plugin-abi` (182) |
| Errors | 339 | `openracing-errors` — exhaustive error variant coverage |
| Compat + Config | 272+ | `compat` — deep migration + validation tests (wave 33) |
| Profile | 236+ | `openracing-profile` (172), `openracing-profile-repository` — inheritance, validation, comprehensive system tests (wave 35) |
| Filters | 233 | `openracing-filters` — snapshot + property tests, SM-V2 deep |
| Safety | 400+ | `openracing-fmea` (271), `openracing-watchdog`, `openracing-hardware-watchdog` (205) — fault injection, property tests, 23 safety invariants (wave 30) |
| IPC | 170 | `openracing-ipc` — message serialization, snapshot round-trips |
| FFB + Calibration | 266+ | `openracing-ffb` (152), `openracing-calibration` (114+) — force output, profile application, workflows, migration (wave 35) |
| Curves | 169+ | `openracing-curves` — LUT fidelity, interpolation, bezier, fitting, monotonicity, stability (wave 35) |
| Firmware | 159 | `openracing-firmware-update` — state machine, rollback, validation |
| Capture | 83+ | `hid-capture` — device capture tooling, fingerprinting, classification (wave 34) |
| Pipeline | 106 | `openracing-pipeline` — filter chains, edge cases |
| Tracing | 120+ | `openracing-tracing` — drop rate, emission verification, spans, formats, snapshots (wave 35) |
| Scheduler | 110 | `openracing-scheduler` — priority inversion, deadline handling |
| Diagnostic | 153+ | `openracing-diagnostic` — insta snapshots, diagnostics deep (wave 35) |
| Atomic | 98 | `openracing-atomic` — concurrent stress, ordering guarantees |
| Crypto | 79 | `openracing-crypto` — signing property tests |
| Other utilities | 600+ | `openracing-shifter` (148), `openracing-handbrake` (48), `openracing-device-types`, `openracing-capture-ids`, `input-maps`, `compat`, `changelog`, etc. |

---

## Fuzz Targets

| Metric | Count |
|--------|------:|
| **Total fuzz targets** | **104** |
| Location | `fuzz/fuzz_targets/*.rs` |
| Coverage | All 17 HID protocol crates + all game telemetry adapters |
| New in wave 31 | telemetry packet, profile, calibration, filter pipeline |

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
- **Fuzz corpus:** seed corpus in `fuzz/corpus/` for all 104 targets

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

---

*Source: `cargo test --workspace --all-features --exclude racing-wheel-ui` · waves 15–35 complete*
