# Now · Next · Later

One-screen execution plan for OpenRacing. Updated alongside the branch `feat/wave15-rc-hardening`.

---

## NOW (actively in flight)

- **PR #22 — Wave 15 RC hardening**: waves 22-27 complete — golden packets, safety soak, plugin security, schema evolution, compile-fail tests, doc-tests, telemetry deep, protocol deep, trybuild all landed
- **Test suite at 15,444+**: unit, integration, proptest, snapshot, E2E, compile-fail, golden-packet, safety-soak, doc-tests, trybuild
- **100+ fuzz targets**: all HID protocols, game telemetry adapters, and new wave 24 targets

**Recently completed (this branch):**
- ✅ Wave 25: Telemetry adapter deep (AMS2, SimHub, KartKraft, MudRunner, Rennsport), watchdog/FMEA deep, protocol snapshots, full-stack E2E, performance gates
- ✅ Wave 26: Remaining adapters (F1, Forza, LFS, RaceRoom, WRC), protocol deep (Moza, Fanatec, Thrustmaster), peripherals deep, SM-V2+filters deep, FFB+calibration+pipeline deep
- ✅ Wave 27: iRacing+ACC+BeamNG, DiRT Rally+ETS2+GT7, 9 HID protocol deep tests, tracing+support+core+streams deep
- ✅ Wave 22: Engine device/game tests, IPC snapshots, service lifecycle, error exhaustiveness
- ✅ Wave 23: Golden packets (6 adapters), safety soak (10K ticks), plugin security, schema evolution, CLI/profile deep
- ✅ Wave 24: Trybuild compile-fail tests, config/firmware deep, atomic stress, scheduler deep, doc-tests, 4 new fuzz targets
- ✅ hwdb-verified PIDs for Fanatec, Thrustmaster, Asetek, Simagic
- ✅ CI governance workflow fix (`track_compat_usage.py --current` flag)
- ✅ Device PID verification across all 17 vendor protocol crates (id_verification suites)
- ✅ Logitech G923 Xbox alt PID (`0xC26D`) and Asetek La Prima Pedals PID (`0xF102`)
- ✅ Heusinkveld VID correction and Leo Bodnar pedals PIDs
- ✅ GT7 extended packet types (316/344 bytes) — F-064 resolved
- ✅ GT Sport port swap fix — F-065 resolved
- ✅ Fanatec sign-fix inversion corrected — F-062 resolved
- ✅ deny.toml updated for libbz2-rs-sys license
- ✅ Waves 19-20 deep test expansion: 13,075 → 14,017+ tests passing across all crates
- ✅ Waves 25-27 deep test expansion: 14,017 → 15,444+ tests passing across all crates

## NEXT (queued, ready to start)

- **Merge PR #22** after CI green → cut v1.0.0-rc.2 tag
- **Line-level code coverage**: integrate `llvm-cov` or `cargo-tarpaulin` into CI to identify uncovered branches
- **macOS IOKit HID support** (Phase 4): IOKit HID implementation + `thread_policy_set` RT scheduling
- **macOS CI runner** in GitHub Actions matrix (F-053)
- **Plugin system security hardening**: replace Ed25519 stub (`signature.rs:111`), implement PE/ELF embedded signature checking (`crypto/mod.rs:204`)
- **Packaging/installer automation**: Linux deb/rpm/flatpak, macOS DMG with notarization (Windows MSI done)
- **Unverified PID resolution**: Cube Controls `0x0C73–0x0C75`, VRS DFP V2 `0xA356`, OpenFFBoard `0xFFB1` — need hardware captures
- **Remaining golden-packet tests**: expand golden-packet coverage beyond 6 adapters to all high-priority telemetry adapters

## LATER (roadmap, not yet scoped)

- **Adaptive RT scheduling**: CPU governor integration, load-aware deadline adjustment
- **Physical hardware capture tooling**: `openracing-capture` protocol sniffer/mapper for reverse engineering
- **Niche vendor support**: MMOS, Oddor, SHH, SimGrade, Turtle Beach, Simucube 3, SIMTAG, Gomez
- **Full mutation testing coverage**: expand beyond current Moza/ks/input-maps/filters scope
- **Performance benchmarking automation**: CI-integrated `bench_results.json` comparison across runs
- **Cloud integration**: profile sharing and backup via OpenRacing Hub
- **ACC2 / AC EVO telemetry adapters**: blocked on Kunos publishing protocol docs (F-022)

---

## Metrics

| Metric | Value |
|--------|-------|
| Supported devices | ~90+ VID/PID pairs across 16+ vendors |
| Supported games | 56 telemetry adapter modules |
| Test count | 15,444+ across 526+ test binaries (unit, integration, proptest, snapshot, E2E, compile-fail, golden-packet, doc-tests, trybuild) |
| Fuzz targets | 100+ across all HID protocols and game adapters |
| Protocol crates | 17 HID vendor protocol microcrates |
| Snapshot tests | 1,000+ snapshot files across 38+ snapshot directories |
| Crate coverage | 80/82 crates have dedicated test files (exceptions: changelog, ui) |
| Friction log | 68 items total — 15 open, 49 resolved, 1 investigating, 2 noted, 1 won't fix |

---

*Source: [ROADMAP.md](../ROADMAP.md) · [CHANGELOG.md](../CHANGELOG.md) · [FRICTION_LOG.md](FRICTION_LOG.md)*
