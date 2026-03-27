# NOW / NEXT / LATER

One-screen execution plan for OpenRacing. Updated each sprint.

**Project snapshot:** 85 crates · 30,461+ tests · 509 proptests · 117 fuzz targets · 28 vendors · 61 games

**First hardware target:** Moza R5 + KS + ES + SR-P + HBP (Phases 6–11)

---

## NOW (Active — this sprint)

- **Phase 6: Device Enumeration** — plug in R5, KS, SR-P, HBP; run `wheelctl device list`; capture HID report descriptors as golden fixtures
- **Phase 7: Input Report Capture** — validate `parse_wheelbase_input_report` with live R5 data; verify steering, pedal, handbrake axes track physical movement
- **Service API completion** — implement `WheelService::game_service()` and `plugin_service()` accessors; re-enable blocked integration tests

## NEXT (Queued — next 2–4 sprints)

- **Phase 8: Handshake & Feature Reports** — execute `initialize_device()` against R5; validate init state machine; test rotation range control
- **Phase 9: Low-Torque FFB Output** — safety-gated torque output starting at ≤10% (0.55 Nm); ramp to 100% with manual observation; watchdog validation
- **Phase 10: Game Telemetry Integration** — full loop from game telemetry through FFB to wheel; test with Assetto Corsa / iRacing
- **Mutation testing expansion** — extend `cargo-mutants` to protocol encoding and telemetry paths
- **macOS IOKit HID driver** — start actual device I/O on macOS

## LATER (Backlog — future work)

- **Phase 11: Extended Validation & Soak** — 1hr continuous FFB, disconnect/reconnect stress, V1 vs V2 firmware, Standard vs Direct FFB comparison
- **Phase 12: Multi-Vendor Verification** — Fanatec, Logitech, Thrustmaster HIL; protocol research; 48hr soak; community capture program
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