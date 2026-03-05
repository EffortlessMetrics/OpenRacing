# NOW / NEXT / LATER

One-screen execution plan for OpenRacing. Updated each sprint.

**Project snapshot:** 83 crates · 29,000+ tests · 505 proptests · 133 fuzz targets · 28 vendors · 61 games

---

## NOW (Active — this sprint)

- CI green on all platforms — fix concurrency-group cancellation cascade (workflow_dispatch in progress)
- Merge wave 121 PRs — CI hardening, macOS IOKit, integration test re-enablement, engine deep tests
- Re-enable disabled integration tests — address technical debt item in ROADMAP
- macOS IOKit HID driver — start actual device I/O on macOS (currently compile-only)
- Engine blackbox/safety/pipeline deep tests — close remaining coverage gaps in safety-critical paths

## NEXT (Queued — next 2–4 sprints)

- Packaging automation — deb/rpm in CI with signing, Windows MSI signing, macOS DMG + notarization
- Adaptive RT scheduling — CPU governor integration for dynamic deadline adjustment
- PXN FFB_REPORT_ID verification — confirm 0x05 report ID with USB capture data
- Code coverage in CI — configure line-level coverage reporting (llvm-cov)
- Performance tuning — benchmark suite expansion, profiling under load
- Device capture tooling refinement — openracing-capture protocol sniffer/mapper

## LATER (Backlog — future work)

- Hardware-in-the-loop testing — USB capture validation against physical devices
- Plugin marketplace / repository — searchable catalog with community submissions
- Telemetry dashboard — replay visualization and real-time telemetry display tools
- Flatpak packaging
- BeamNG.drive deep protocol integration (native shared-memory path)
- Community device capture contribution workflow
- Mobile companion app (iOS/Android)

---

*Source: [ROADMAP.md](../ROADMAP.md) · [FRICTION_LOG.md](FRICTION_LOG.md) · [RC_READINESS.md](RC_READINESS.md)*