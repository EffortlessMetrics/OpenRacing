# Now · Next · Later

One-screen execution plan for OpenRacing. Updated after PR #70.

---

## NOW (actively in flight)

- **CI reliability hardening**: Disk cleanup + soak test tolerance + stress test skip-list (PR #70)
- **CI green maintenance**: Continuous fix-forward on any regressions
- **Progressive PR strategy**: Small, focused PRs (≤50 files, ≤5K LOC) landed incrementally

**Merged recently (PRs #24-69):**
- PR #69: CHANGELOG + NOW_NEXT_LATER docs update
- PR #68: Pin nightly toolchain to 2026-03-04 for reproducibility
- PR #67: `#![deny(static_mut_refs)]` to 34 non-test crates
- PR #66: Quarantine fabricated Simagic PIDs from active dispatch
- PR #65: Quarantine fabricated OpenFFBoard/VRS PIDs from active dispatch
- PR #64: CI disk space fix — exclude racing-wheel-ui, aggressive cleanup
- PR #63: 85 authoritative PID cross-validation checks across 18 vendors + bezier LUT tolerance fix
- PR #62: README VID corrections + 66 new cross-validation checks
- PR #61: Thrustmaster cross-validation expansion + CHANGELOG/docs update
- PR #60: Authoritative PID cross-validation test (35+ kernel-sourced checks)
- PR #59: README accuracy — games (14→50+), tests (600→25,500+), crates (8→84)
- PR #58: Docs update for PRs #54-57
- PR #57: 10 community-verified peripheral vendors from simracing-hwdb
- PR #56: CHANGELOG update for PRs #24-55
- PR #55: str_as_str fix for Rust 2024 edition
- PR #54: Platform IPC snapshot normalization
- PR #52: PIDFF deduplication — 5 device crates use pidff-common, -1,717 lines
- PR #51: `openracing-pidff-common` shared PIDFF encoder library (37 tests + 8 proptest suites)
- PRs #24-50: CI fixes, PIDFF effects, telemetry enrichment, protocol improvements
- PR #23: 253K LOC, 85 crates, 24,800+ tests — complete device + game + safety + CI suite

## NEXT (queued, ready to start)

- **Simucube PIDFF refactoring**: Align `SetEffect` struct-based API with pidff-common
- **Docs accuracy pass**: Fix vendor count inconsistencies, verify all CLI commands work
- **macOS IOKit HID support**: Native macOS device communication (F-053)
- **macOS CI runner**: Add macOS to GitHub Actions matrix
- **Device capture tooling**: USB sniffer integration for protocol discovery
- **Packaging hardening**: deb/rpm/flatpak improvements, macOS DMG with notarization
- **Line-level code coverage**: Integrate llvm-cov or cargo-tarpaulin into CI
- **Unverified PID resolution**: Get hardware captures from community for remaining unverified PIDs

## LATER (roadmap, not yet scoped)

- **Plugin marketplace**: Community plugin distribution, versioning, and discovery
- **Cloud telemetry**: Profile sharing, backup, and analytics via OpenRacing Hub
- **ML-based calibration**: Machine-learning-driven auto-calibration for wheel/pedal profiles
- **Adaptive RT scheduling**: CPU governor integration, load-aware deadline adjustment
- **Physical hardware capture tooling**: openracing-capture protocol sniffer/mapper
- **Niche vendor support**: Turtle Beach, Simucube 3, SIMTAG, Gomez (MMOS, Oddor, SHH, SimGrade added in PR #57)
- **Full mutation testing coverage**: Expand beyond current safety/engine/protocol scope
- **Performance benchmarking automation**: CI-integrated bench_results.json comparison

---

## Metrics

| Metric | Value |
|--------|-------|
| Supported devices | ~105+ VID/PID pairs across 25+ vendors |
| Supported games | 61 telemetry adapter modules |
| Test count | 24,800+ (unit, integration, proptest, snapshot, E2E, compile-fail, golden-packet, doc-tests, trybuild, BDD, protocol-verification, concurrency-stress, performance-validation, soak-stress, mutation-testing) |
| Fuzz targets | 113+ across all HID protocols, game adapters, replay, diagnostics, calibration, FFB, crypto, CLI |
| Protocol crates | 17 HID vendor protocol microcrates + 1 shared PIDFF library |
| Snapshot tests | 1,400+ snapshot files across 52+ snapshot directories |
| Crate coverage | 80/87 crates have dedicated test files |
| PRs merged | 69 total (PRs #1-69, plus #70 in review) |

---

*Source: [ROADMAP.md](../ROADMAP.md) · [CHANGELOG.md](../CHANGELOG.md) · [FRICTION_LOG.md](FRICTION_LOG.md)*