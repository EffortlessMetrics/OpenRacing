# NOW / NEXT / LATER

One-screen execution plan for OpenRacing. Updated each sprint.

**Project snapshot:** 86 crates · 29,900+ tests · 509 proptests · 117 fuzz targets · 28 vendors · 61 games

**First hardware target:** Moza R5 + KS + ES + SR-P + HBP (Phase 5.5)

---

## NOW (Active — this sprint)

- **Moza R5 hardware onramp (Stage 0–1)** — read-only enumeration and input capture with physical R5, KS, ES, SR-P, HBP devices; create golden test fixtures from captured reports
- **Service API completion** — implement `WheelService::game_service()` and `plugin_service()` accessors; re-enable blocked integration tests
- **Symbol rename audit (F-007)** — audit protocol crates for symbols needing `#[deprecated]` migration

## NEXT (Queued — next 2–4 sprints)

- **Moza R5 hardware onramp (Stage 2–4)** — handshake validation, low-torque FFB output (start at ≤10% max / 0.55 Nm), game telemetry integration, soak testing
- **Protocol research & cross-validation** — Cube Controls, VRS V2, PXN VD-series PID confirmation via physical device captures
- **Mutation testing expansion** — extend `cargo-mutants` to protocol encoding, telemetry normalization, and IPC codec
- **Fuzz corpus accumulation** — run all 117 fuzz targets for ≥1 hour each
- **macOS IOKit HID driver** — start actual device I/O on macOS
- **Packaging automation** — deb/rpm + MSI signing in CI
- **Performance tuning** — benchmark under sustained real-device load

## LATER (Backlog — future work)

- **Multi-vendor HIL testing** — Fanatec, Logitech, Thrustmaster device validation
- **Extended soak testing** — 48-hour continuous operation, multi-device stress
- **Cloud integration** — profile sharing and cross-machine sync
- **Telemetry dashboard** — browser-based replay visualization and session comparison
- **AI/ML integration** — adaptive FFB tuning from driving style analysis
- **Plugin marketplace** — searchable catalog with community submissions
- **VR / motion rig integration** — haptic feedback via OpenXR
- **Mobile companion app** (iOS/Android)
- **Accessibility** — screen reader support, high-contrast mode
- **Localization** — multi-language UI and docs

---

*Source: [ROADMAP.md](../ROADMAP.md) · [FRICTION_LOG.md](FRICTION_LOG.md) · [RC_READINESS.md](RC_READINESS.md)*