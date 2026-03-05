# NOW / NEXT / LATER

One-screen execution plan for OpenRacing. Updated each sprint.

---

## NOW (Active — this sprint)

- macOS CI results review — first run ever; check for compilation issues and fix-forward
- Dead code cleanup — `engine/profile_service.rs` is a dead duplicate of the service layer
- Remove commented-out code in old `game_integration.rs`
- Service integration test stabilization — re-enable disabled tests for `connect_device` and `game_service`
- FMEA coverage expansion — fault injection acceptance tests for new interlock paths

## NEXT (Queued — next 2–4 sprints)

- macOS IOKit HID driver implementation (`crates/hid-macos`)
- Simucube output module rewrite — move from speculative format to USB HID PID protocol
- Linux packaging CI automation — deb/rpm in CI with signing
- Ed25519 trust store replacement — currently a stub, fail-closed; wire to real key store
- Code coverage in CI — line-level coverage reporting
- Plugin example crate for ecosystem adoption

## LATER (Backlog — future work)

- Flatpak packaging
- macOS DMG + notarization
- Windows MSI signing automation
- Adaptive RT scheduling based on system load (CPU governor integration)
- Plugin marketplace / repository
- Telemetry replay visualization tools
- BeamNG.drive deep protocol integration (native shared-memory path)
- Community device capture contribution workflow
- Real hardware verification program

---

*Source: [ROADMAP.md](../ROADMAP.md) · [FRICTION_LOG.md](FRICTION_LOG.md) · [RC_READINESS.md](RC_READINESS.md)*