# Now · Next · Later

One-screen execution plan for OpenRacing. Updated after PR #53.

---

## NOW (actively in flight)

- **PIDFF deduplication complete**: 5 device crates refactored to re-export from shared `openracing-pidff-common` library (PR #52)
- **All vendor slot/effect encoders exposed**: Fanatec 5-slot, Logitech 4-slot, all PIDFF devices (PRs #46-51)
- **CI green maintenance**: Continuous fix-forward on any regressions
- **Progressive PR strategy**: Small, focused PRs (≤50 files, ≤5K LOC) landed incrementally

**Merged recently (PRs #24-53):**
- PR #52: PIDFF deduplication — 5 device crates use pidff-common, -1,717 lines
- PR #51: `openracing-pidff-common` shared PIDFF encoder library (37 tests + 8 proptest suites)
- PR #50: Fanatec + Logitech slot encoder public API exposure
- PR #49: PIDFF effects for Asetek, FFBeast, Leo Bodnar, PXN
- PR #48: PIDFF effects for AccuForce, Cammus
- PR #47: Simucube PIDFF effects (complete effect lifecycle)
- PR #46: VRS PIDFF effects (vendor-specific report IDs)
- PRs #24-45: CI fixes, unused deps cleanup, telemetry enrichment, protocol improvements
- PR #23: 253K LOC, 85 crates, 24,800+ tests — complete device + game + safety + CI suite

## NEXT (queued, ready to start)

- **Simucube PIDFF refactoring**: Align `SetEffect` struct-based API with pidff-common
- **Docs accuracy pass**: Fix vendor count inconsistencies, verify all CLI commands work
- **macOS IOKit HID support**: Native macOS device communication (F-053)
- **macOS CI runner**: Add macOS to GitHub Actions matrix
- **Plugin security hardening**: Replace Ed25519 stubs with real verification
- **Unverified PID resolution**: VRS DFP V2 `0xA356`, OpenFFBoard `0xFFB1` — need hardware captures
- **Device capture tooling**: USB sniffer integration for protocol discovery
- **Packaging hardening**: deb/rpm/flatpak improvements, macOS DMG with notarization
- **Line-level code coverage**: Integrate llvm-cov or cargo-tarpaulin into CI

## LATER (roadmap, not yet scoped)

- **Plugin marketplace**: Community plugin distribution, versioning, and discovery
- **Cloud telemetry**: Profile sharing, backup, and analytics via OpenRacing Hub
- **ML-based calibration**: Machine-learning-driven auto-calibration for wheel/pedal profiles
- **Adaptive RT scheduling**: CPU governor integration, load-aware deadline adjustment
- **Physical hardware capture tooling**: openracing-capture protocol sniffer/mapper
- **Niche vendor support**: MMOS, Oddor, SHH, SimGrade, Turtle Beach, Simucube 3, SIMTAG, Gomez
- **Full mutation testing coverage**: Expand beyond current safety/engine/protocol scope
- **Performance benchmarking automation**: CI-integrated bench_results.json comparison

---

## Metrics

| Metric | Value |
|--------|-------|
| Supported devices | ~90+ VID/PID pairs across 15+ vendors |
| Supported games | 61 telemetry adapter modules |
| Test count | 24,800+ (unit, integration, proptest, snapshot, E2E, compile-fail, golden-packet, doc-tests, trybuild, BDD, protocol-verification, concurrency-stress, performance-validation, soak-stress, mutation-testing) |
| Fuzz targets | 113+ across all HID protocols, game adapters, replay, diagnostics, calibration, FFB, crypto, CLI |
| Protocol crates | 17 HID vendor protocol microcrates + 1 shared PIDFF library |
| Snapshot tests | 1,400+ snapshot files across 52+ snapshot directories |
| Crate coverage | 80/87 crates have dedicated test files |
| PRs merged | 53 total (PRs #1-53) |

---

*Source: [ROADMAP.md](../ROADMAP.md) · [CHANGELOG.md](../CHANGELOG.md) · [FRICTION_LOG.md](FRICTION_LOG.md)*