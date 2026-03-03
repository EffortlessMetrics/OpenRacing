# Now · Next · Later

One-screen execution plan for OpenRacing. Updated alongside the branch `feat/wave15-rc-hardening`.

---

## NOW (actively in flight)

- **PR #22 — Wave 15 RC hardening**: in final review — protocol verification, PID cross-validation, telemetry enrichment all landed
- **Telemetry adapter enrichment**: ACC, PCars2, iRacing, RaceRoom, F1, BeamNG/LFS, Assetto Corsa — G-forces, flags, timing, typed fields promoted
- **Property + snapshot test expansion**: proptest suites added for Simucube, Logitech, Thrustmaster, Fanatec, Moza, PXN, AccuForce; insta snapshots for iRacing, BeamNG, ACC adapters

**Recently completed (this branch):**
- ✅ hwdb-verified PIDs for Fanatec, Thrustmaster, Asetek, Simagic
- ✅ CI governance workflow fix (`track_compat_usage.py --current` flag)
- ✅ Device PID verification across all 17 vendor protocol crates (id_verification suites)
- ✅ Logitech G923 Xbox alt PID (`0xC26D`) and Asetek La Prima Pedals PID (`0xF102`)
- ✅ Heusinkveld VID correction and Leo Bodnar pedals PIDs
- ✅ GT7 extended packet types (316/344 bytes) — F-064 resolved
- ✅ GT Sport port swap fix — F-065 resolved
- ✅ Fanatec sign-fix inversion corrected — F-062 resolved
- ✅ deny.toml updated for libbz2-rs-sys license

## NEXT (queued, ready to start)

- **Merge PR #22** after CI green → cut v1.0.0-rc.2 tag
- **Telemetry adapter test coverage expansion**: remaining adapters need golden-packet integration tests
- **macOS IOKit HID support** (Phase 4): IOKit HID implementation + `thread_policy_set` RT scheduling
- **macOS CI runner** in GitHub Actions matrix (F-053)
- **Plugin system security hardening**: replace Ed25519 stub (`signature.rs:111`), implement PE/ELF embedded signature checking (`crypto/mod.rs:204`)
- **Packaging/installer automation**: Linux deb/rpm/flatpak, macOS DMG with notarization (Windows MSI done)
- **Unverified PID resolution**: Cube Controls `0x0C73–0x0C75`, VRS DFP V2 `0xA356`, OpenFFBoard `0xFFB1` — need hardware captures

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
| Test count | 9,939 across 406 test binaries (unit, integration, proptest, snapshot, E2E) |
| Fuzz targets | 84 across all HID protocols and game adapters |
| Protocol crates | 17 HID vendor protocol microcrates |
| Snapshot tests | 936 snapshot files across 37 snapshot directories |
| Friction log | 65 items total — 13 open, 48 resolved, 2 investigating, 2 noted |

---

*Source: [ROADMAP.md](../ROADMAP.md) · [CHANGELOG.md](../CHANGELOG.md) · [FRICTION_LOG.md](FRICTION_LOG.md)*
