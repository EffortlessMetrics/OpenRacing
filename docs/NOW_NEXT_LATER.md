# NOW / NEXT / LATER

One-screen execution plan for OpenRacing. Updated each sprint.

---

## NOW (Active — this sprint)

- Plugin ABI stabilization — finalize native plugin ABI for v1.0 freeze
- Diagnostics dashboard — wire diagnostic service data into Tauri UI
- CLI test expansion — remaining subcommand coverage gaps
- Service integration test stabilization — re-enable remaining disabled tests
- Code coverage in CI — configure line-level coverage reporting (llvm-cov)

## NEXT (Queued — next 2–4 sprints)

- Linux packaging CI automation — deb/rpm in CI with signing
- Real hardware verification — USB capture validation against physical devices
- Performance tuning — benchmark suite expansion, profiling under load
- Plugin example crate — ecosystem adoption starter kit
- Simucube output module rewrite — move from speculative format to USB HID PID protocol
- Windows MSI signing automation

## LATER (Backlog — future work)

- macOS IOKit HID driver implementation (`crates/hid-macos`)
- macOS DMG + notarization
- Flatpak packaging
- Adaptive RT scheduling (CPU governor integration)
- Plugin marketplace / repository
- Telemetry replay visualization tools
- BeamNG.drive deep protocol integration (native shared-memory path)
- Community device capture contribution workflow
- Mobile companion app (iOS/Android)

---

*Source: [ROADMAP.md](../ROADMAP.md) · [FRICTION_LOG.md](FRICTION_LOG.md) · [RC_READINESS.md](RC_READINESS.md)*