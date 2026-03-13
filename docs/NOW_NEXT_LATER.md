# NOW / NEXT / LATER

One-screen execution plan for OpenRacing. Updated each sprint.

**Project snapshot:** 85 crates · 30,461+ tests · 509 proptests · 117 fuzz targets · 28 vendors · 61 games

---

## NOW (Active — this sprint)

- Documentation accuracy pass — verify all commands, counts, and feature claims across docs for RC readiness
- CI green on all platforms — fix concurrency-group cancellation cascade (workflow_dispatch in progress)
- Merge wave 121 PRs — CI hardening, macOS IOKit, integration test re-enablement, engine deep tests
- Re-enable disabled integration tests — address remaining technical debt item in ROADMAP
- Engine blackbox/safety/pipeline deep tests — close remaining coverage gaps in safety-critical paths

## NEXT (Queued — next 2–4 sprints)

- macOS IOKit HID driver — start actual device I/O on macOS (currently compile-only)
- macOS DMG packaging with notarization
- Packaging automation — deb/rpm in CI with signing, Windows MSI signing
- Adaptive RT scheduling — CPU governor integration for dynamic deadline adjustment
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