# Now $([char]0x00B7) Next $([char]0x00B7) Later

One-screen execution plan for OpenRacing. Updated post-merge of PR #23.

---

## NOW (actively in flight)

- **PR #24 -- CI docs & hardening**: Fix docs.yml (exclude UI crate), regression-prevention.yml (udeps soft-fail), mutation-tests.yml (system deps), remove fabricated Cube Controls PIDs from FFB dispatch, fix flaky scheduler test
- **PR #25 -- Unused deps cleanup**: Remove 28 genuinely unused dependencies from 19 crates, add udeps ignore for false positives
- **CI health on main**: Documentation and Regression Prevention workflows fixed in PR #24; Smoke Test flaky scheduler_under_cpu_busy_loop fixed in PR #24
- **Progressive PR strategy**: Small, focused PRs (<=50 files, <=5K LOC) landed incrementally

**Merged recently:**
- PR #23: 253K LOC, 85 crates, 24,800+ tests -- complete device + game + safety + CI suite
- Stale PRs #18 and #20 closed as SUPERSEDED (content landed in PR #23)
- Fabricated Cube Controls PIDs removed from FFB dispatch (safety fix)
- PID verification research: Cube Controls FABRICATED, VRS DFP V2 UNVERIFIED, OpenFFBoard 0xFFB1 SPECULATIVE

## NEXT (queued, ready to start)

- **Docs accuracy pass**: Fix vendor count inconsistencies, verify all CLI commands work, update overclaims
- **macOS IOKit HID support**: Native macOS device communication (F-053)
- **macOS CI runner**: Add macOS to GitHub Actions matrix
- **Plugin security hardening**: Replace Ed25519 stubs with real verification
- **Unverified PID resolution**: VRS DFP V2 `0xA356` (UNVERIFIED), OpenFFBoard `0xFFB1` (SPECULATIVE) -- need hardware captures
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
- **ACC2 / AC EVO telemetry adapters**: Blocked on Kunos publishing protocol docs (F-022)

---

## Metrics

| Metric | Value |
|--------|-------|
| Supported devices | ~90+ VID/PID pairs across 15+ vendors |
| Supported games | 61 telemetry adapter modules |
| Test count | 24,800+ (unit, integration, proptest, snapshot, E2E, compile-fail, golden-packet, doc-tests, trybuild, BDD, protocol-verification, concurrency-stress, performance-validation, soak-stress, mutation-testing) |
| Fuzz targets | 113+ across all HID protocols, game adapters, replay, diagnostics, calibration, FFB, crypto, CLI |
| Protocol crates | 17 HID vendor protocol microcrates |
| Snapshot tests | 1,400+ snapshot files across 52+ snapshot directories |
| Crate coverage | 79/82 crates have dedicated test files (exceptions: changelog, ui, workspace-hack) |

---

*Source: [ROADMAP.md](../ROADMAP.md) -- [CHANGELOG.md](../CHANGELOG.md) -- [FRICTION_LOG.md](FRICTION_LOG.md)*