# Roadmap

This document outlines the development roadmap for the Racing Wheel Software Suite. It tracks the implementation status of key features, architectural decisions, and future plans.

## Current Status (Q1 2026)

- **Core Architecture**: Established via ADRs (0001-0006).
- **Basic Engine**: Real-time engine framework implemented with basic FFB pipeline.
- **Linux Support**: Initial HID implementation for Linux (hidraw/udev).
- **Plugin System**: Basic structure for WASM and Native plugins.
- **Safety System**: Initial safety checks and fault management.
- **Code Health**: Core crates cleaned up and lint-free (Engine, Service).

## Known Issues

- **Build Dependency (Linux/Ubuntu 24.04)**: The UI crate dependency chain (`tauri` -> `webkit2gtk`) requires `libjavascriptcoregtk-4.0-dev`, but Ubuntu Noble (24.04) only provides `4.1`. This causes build failures for `javascriptcore-rs-sys` v0.4.0.
    - *Workaround*: Requires updating Tauri/wry dependencies to versions compatible with WebKitGTK 4.1, or building against an older base image.

## Milestones

### Phase 1: Foundation (Current)
- [x] Define Core Architecture (ADRs 0001-0006)
- [x] Implement basic Real-Time Engine loop
- [x] Implement Linux HID driver (hidraw/udev)
- [x] Implement Linux RT scheduling (SCHED_FIFO/rtkit)
- [ ] Complete Windows HID driver implementation
- [ ] Integrate basic telemetry monitoring
- [ ] Initial CLI tools for management
- [ ] **Device Ecosystem**
    - [ ] Protocol Documentation (Moza, Fanatec, Simagic, etc.)
    - [ ] `openracing-capture` Utility (Mapper/Sniffer)

### Phase 2: Feature Completeness
- [ ] **Advanced Force Feedback**
    - [ ] Curve-based FFB effects (TODO: `crates/engine/src/pipeline.rs`)
    - [ ] Profile hierarchy and inheritance (TODO: `crates/engine/src/profile_service.rs`)
    - [ ] Profile optimization (TODO: `crates/engine/src/profile_merge.rs`)
- [ ] **Enhanced Telemetry**
    - [ ] Game service integration (TODO: `crates/engine/src/metrics.rs`)
    - [x] Histogram tracking for latency metrics (RTSampleQueues + HDRHistogram)
- [ ] **Safety & Reliability**
    - [ ] Fault quarantine implementation (TODO: `crates/engine/src/safety/fmea.rs`)
    - [ ] Full replay validation (TODO: `crates/engine/src/diagnostic/replay.rs`)
    - [ ] Adaptive scheduling (TODO: `crates/engine/src/scheduler.rs`)

### Phase 3: Security & Ecosystem
- [ ] **Plugin Security**
    - [ ] Real Ed25519 signature verification (TODO: `crates/service/src/security/signature.rs`)
    - [ ] Embedded signatures check (PE/ELF) (TODO: `crates/service/src/crypto/mod.rs`)
- [ ] **Firmware Management**
    - [ ] Rollback tracking and recovery (TODO: `crates/service/src/update/firmware.rs`)
- [ ] **Performance**
    - [ ] Index optimization for diagnostics (TODO: `crates/engine/src/diagnostic/blackbox.rs`)

## Future Considerations

- **Multi-Platform Support**: Expand support to macOS (currently partial).
- **Cloud Integration**: Profile sharing and cloud backup.
- **Mobile Companion App**: Remote control and monitoring via mobile device.
- **AI/ML Integration**: Adaptive FFB tuning based on driving style.

## Release Schedule

| Version | Target Date | Focus |
|---------|-------------|-------|
| v0.1.0  | Q1 2026     | Core Engine & Linux Support (Alpha) |
| v0.2.0  | Q2 2026     | Windows Support & Basic UI |
| v0.3.0  | Q3 2026     | Plugin System & Advanced FFB |
| v1.0.0  | Q4 2026     | Stable Release |

---
*Note: This roadmap is subject to change based on community feedback and technical challenges.*
