# Now · Next · Later

One-screen execution plan for OpenRacing. Updated alongside the branch `feat/wave15-rc-hardening`.

---

## NOW (actively in flight)

- **PR #22 — Wave 15 RC hardening**: waves 22-43 complete — golden packets, safety soak, plugin security, schema evolution, compile-fail tests, doc-tests, telemetry deep, protocol deep, trybuild, BDD scenarios, protocol verification, hot-swap, FFB pipeline E2E, concurrency stress, performance validation, capture tooling, profile/diagnostics deep, core infrastructure deep, input/peripherals deep, WASM/native plugin deep, safety/engine deep, full telemetry adapter re-verification, CI gate verification, game support matrix, udev expansion, example plugin tests all landed
- **Wave 44 — RT enforcement, fault injection, protocol roundtrip, IPC compat**: RT allocation detection, fault injection framework expansion, protocol roundtrip fuzz, IPC backward compatibility validation
- **Test suite at 21,374+**: unit, integration, proptest, snapshot, E2E, compile-fail, golden-packet, safety-soak, doc-tests, trybuild, BDD, protocol-verification, concurrency-stress, performance-validation
- **104 fuzz targets**: all HID protocols, game telemetry adapters, and new wave 31 targets (telemetry packet, profile, calibration, filter pipeline)
- **Protocol verification complete**: ALL 14 HID crates cross-verified against community sources (kernel drivers, pid.codes, vendor docs)

**Recently completed (this branch):**
- ✅ Wave 43: CI gate verification (fmt, deny, ADR), workspace-hack sync, game support matrix (61 adapters), udev rules expansion (+75 rules), example plugin tests (51 tests), docs alignment fixes
- ✅ Wave 41: FFB (107) + calibration (84) deep tests, service lifecycle (37) + IPC (37) deep tests, engine safety (76) + device management (53) deep, schemas (97) + IPC protocol (76) deep, compat (40) + firmware update (71) deep, capture IDs (45) + test helpers (149)
- ✅ Wave 40: Integration E2E (plugin 23 + telemetry 22 + device protocol 22), telemetry adapter re-verification (AMS2, F1, Rennsport, SimHub, RaceRoom, LFS, KartKraft, MudRunner, WRC — 374 tests), profile (97) + repo (94) + config writers (48), telemetry config (73) + streams (52)
- ✅ Wave 39: Native plugin (90) + plugin ABI (81) deep tests, crypto (52) + FMEA (50), filters (101) + pipeline (62), watchdog software (58) + hardware (81)
- ✅ Wave 38: Simagic protocol verification (38) + deep tests (68), WASM runtime deep (54), diagnostic + SRP + capture deep (251), Forza deep (90) + support deep (25)
- ✅ Wave 37: Telemetry core (58), integration (59), rate-limiter (35) deep tests, HBP (43) + Moza wheelbase report (59) protocol deep, peripherals deep (handbrake, shifter, device-types), 13 BDD device + game behavior scenarios
- ✅ Wave 36: Property-based tests for FFB (17), pipeline (11), schemas (29), IPC (15), HID common (72) + scheduler (79) + atomic (100) deep tests, input maps (67) + KS representation (83), SimpleMotion V2 protocol verification (79), doc-tests across 5 crates
- ✅ Wave 35: Service diagnostics deep tests (40), comprehensive profile system tests (64), tracing+curves+calibration deep tests + snapshots (86)
- ✅ Wave 34: Concurrency stress tests (23 multi-threaded scenarios), performance validation (12 RT timing checks), device capture tooling tests (83), extended telemetry adapter verification for 9 adapters (110)
- ✅ Wave 33: Protocol verification for ALL remaining HID crates (AccuForce, Asetek, Button Box, Cammus, Cube Controls, FFBeast, Leo Bodnar, VRS), FFB pipeline E2E tests, compat+config deep migration+validation tests
- ✅ Wave 32: Telemetry adapter constants cross-verified against game APIs, schemas+IPC+service deep tests, Heusinkveld+PXN protocol verification, firmware update deep tests
- ✅ Wave 31: Plugin system lifecycle+security deep tests, 4 new fuzz targets (104 total), SimuCube+OpenFFBoard protocol verification, Moza+Fanatec+Logitech+Thrustmaster cross-verified against community sources
- ✅ Wave 30: Device hot-swap simulation tests, CLI comprehensive E2E tests, safety property-based invariant tests (23 tests, 256+ cases each)
- ✅ Wave 29: 15 BDD Given/When/Then behavior scenarios, proptest regression cleanup
- ✅ Wave 28: Telemetry-config-writers+streams coverage, snapshot tests for FFB/profile/pipeline/engine crates
- ✅ Wave 27: iRacing+ACC+BeamNG, DiRT Rally+ETS2+GT7, 9 HID protocol deep tests, tracing+support+core+streams deep
- ✅ Wave 26: Remaining adapters (F1, Forza, LFS, RaceRoom, WRC), protocol deep (Moza, Fanatec, Thrustmaster), peripherals deep, SM-V2+filters deep, FFB+calibration+pipeline deep
- ✅ Wave 25: Telemetry adapter deep (AMS2, SimHub, KartKraft, MudRunner, Rennsport), watchdog/FMEA deep, protocol snapshots, full-stack E2E, performance gates
- ✅ Wave 24: Trybuild compile-fail tests, config/firmware deep, atomic stress, scheduler deep, doc-tests, 4 new fuzz targets
- ✅ Wave 23: Golden packets (6 adapters), safety soak (10K ticks), plugin security, schema evolution, CLI/profile deep
- ✅ Wave 22: Engine device/game tests, IPC snapshots, service lifecycle, error exhaustiveness
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
- ✅ Waves 28-29 final hardening: 15,444 → 15,820+ tests passing across all crates
- ✅ Waves 30-33 protocol verification + deep testing: 15,820 → 16,742+ tests passing across all crates
- ✅ Waves 34-35 concurrency, performance, capture, diagnostics, profile deep: 16,742 → 17,696+ tests passing across all crates
- ✅ Waves 36-37 core infrastructure, input, protocols, telemetry, peripherals, BDD deep: 17,696 → 18,645+ tests passing across all crates
- ✅ Waves 38-41 plugin/safety/engine/telemetry/infrastructure comprehensive deep: 18,645 → 21,043+ tests passing across all crates
- ✅ Wave 43 CI verification + game support + packaging + example plugins: 21,043 → 21,374+ tests passing across all crates

## NEXT (queued, ready to start)

- **Wave 45+ hardening**: macOS IOKit HID support, packaging hardening (deb/rpm/flatpak), adaptive RT scheduling
- **Merge PR #22** after CI green → cut v1.0.0-rc.2 tag
- **Line-level code coverage**: integrate `llvm-cov` or `cargo-tarpaulin` into CI to identify uncovered branches
- **macOS CI runner** in GitHub Actions matrix (F-053)
- **Plugin system security hardening**: replace Ed25519 stub (`signature.rs:111`), implement PE/ELF embedded signature checking (`crypto/mod.rs:204`)
- **Packaging/installer automation**: Linux deb/rpm/flatpak, macOS DMG with notarization (Windows MSI done)
- **Unverified PID resolution**: Cube Controls `0x0C73–0x0C75`, VRS DFP V2 `0xA356`, OpenFFBoard `0xFFB1` — need hardware captures
- **Remaining golden-packet tests**: expand golden-packet coverage beyond 6 adapters to all high-priority telemetry adapters

## LATER (roadmap, not yet scoped)

- **Plugin marketplace**: community plugin distribution, versioning, and discovery
- **Cloud telemetry**: profile sharing, backup, and analytics via OpenRacing Hub
- **ML-based calibration**: machine-learning-driven auto-calibration for wheel/pedal profiles
- **Adaptive RT scheduling**: CPU governor integration, load-aware deadline adjustment
- **Physical hardware capture tooling**: `openracing-capture` protocol sniffer/mapper for reverse engineering
- **Niche vendor support**: MMOS, Oddor, SHH, SimGrade, Turtle Beach, Simucube 3, SIMTAG, Gomez
- **Full mutation testing coverage**: expand beyond current Moza/ks/input-maps/filters scope
- **Performance benchmarking automation**: CI-integrated `bench_results.json` comparison across runs
- **ACC2 / AC EVO telemetry adapters**: blocked on Kunos publishing protocol docs (F-022)

---

## Metrics

| Metric | Value |
|--------|-------|
| Supported devices | ~90+ VID/PID pairs across 16+ vendors |
| Supported games | 61 telemetry adapter modules |
| Test count | 21,374+ across 640+ test binaries (unit, integration, proptest, snapshot, E2E, compile-fail, golden-packet, doc-tests, trybuild, BDD, protocol-verification, concurrency-stress, performance-validation) |
| Fuzz targets | 104 across all HID protocols and game adapters |
| Protocol crates | 17 HID vendor protocol microcrates |
| Snapshot tests | 1,327 snapshot files across 52 snapshot directories |
| Crate coverage | 79/82 crates have dedicated test files (exceptions: changelog, ui, workspace-hack) |
| Friction log | 68 items total — 15 open, 49 resolved, 1 investigating, 2 noted, 1 won't fix |

---

*Source: [ROADMAP.md](../ROADMAP.md) · [CHANGELOG.md](../CHANGELOG.md) · [FRICTION_LOG.md](FRICTION_LOG.md)*
