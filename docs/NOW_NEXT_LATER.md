# NOW / NEXT / LATER

One-screen execution plan for OpenRacing. Updated each sprint.

**Project snapshot:** 83 crates · 26,900+ tests · 28 vendors · 61 games

---

## NOW (Active — this sprint)

- CI green on all platforms — fix remaining failures on Ubuntu/Windows/macOS matrix
- Merge remaining test PRs — clear backlog of open test-coverage PRs
- CHANGELOG up to date — ensure all merged work since v0.3.0 is documented
- Service integration test stabilization — re-enable remaining disabled tests
- Code coverage in CI — configure line-level coverage reporting (llvm-cov)

## NEXT (Queued — next 2–4 sprints)

- macOS IOKit HID driver implementation (`crates/hid-macos`) — unblock macOS device support
- Packaging automation — deb/rpm in CI with signing, Windows MSI signing, macOS DMG + notarization
- Adaptive RT scheduling — CPU governor integration for dynamic deadline adjustment
- Plugin example crate — ecosystem adoption starter kit
- Performance tuning — benchmark suite expansion, profiling under load

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