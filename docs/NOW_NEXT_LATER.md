# NOW / NEXT / LATER

One-screen execution plan for OpenRacing. Updated each sprint.

---

## NOW (Active — this sprint)

- Merge PR #79 (simracing-hwdb integration) and PR #82 (packaging completeness)
- macOS CI matrix expansion — currently Linux + Windows only; add `macos-14` runner
- Fix proptest CI flakiness — property tests timeout under load on shared runners
- Service integration test gaps — re-enable disabled tests for `connect_device` and `game_service`

## NEXT (Queued — next 2–4 sprints)

- macOS IOKit HID driver implementation (`crates/hid-macos`)
- Device capture/sniffer tooling for protocol discovery (`openracing-capture` utility)
- Simucube output module rewrite — move from speculative format to USB HID PID protocol
- Linux packaging automation — deb/rpm specs in CI with signing
- FMEA coverage expansion — fault injection acceptance tests for new interlock paths
- Unverified device PID documentation — Cube Controls, Heusinkveld, VRS estimates in `SOURCES.md`

## LATER (Backlog — future work)

- Flatpak packaging
- macOS DMG + notarization
- Windows MSI signing automation
- Adaptive RT scheduling based on system load (CPU governor integration)
- Plugin marketplace / repository
- Telemetry replay visualization tools
- BeamNG.drive deep protocol integration (native shared-memory path)
- Community device capture contribution workflow

---

*Source: [ROADMAP.md](../ROADMAP.md) · [FRICTION_LOG.md](FRICTION_LOG.md) · [RC_READINESS.md](RC_READINESS.md)*