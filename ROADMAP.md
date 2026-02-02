# Roadmap

This document outlines the development roadmap for OpenRacing. It tracks the implementation status of key features, architectural decisions, and future plans.

## Current Status (v0.3.0 - Q1 2026)

**Released Features:**
- **Core FFB Engine**: Real-time force feedback processing at 1kHz with zero-allocation RT path
- **Cross-Platform HID**: Full support for Linux (hidraw/udev) and Windows (overlapped I/O, MMCSS)
- **Plugin System**: WASM sandboxed runtime + Native plugins with Ed25519 signature verification
- **Game Telemetry**: Adapters for iRacing, ACC, Automobilista 2, and rFactor 2
- **Curve-Based FFB**: Customizable response curves (linear, exponential, logarithmic, Bezier)
- **Profile Inheritance**: Hierarchical profiles with up to 5 levels of inheritance
- **Tauri UI**: Device management, real-time telemetry display, profile application
- **CLI Tools**: `wheelctl` for device management, diagnostics, and profile operations
- **Safety System**: Fault detection, safe mode transitions, black box recording
- **Protocol Documentation**: Logitech, Fanatec, Thrustmaster, Simagic, Moza protocols documented

**Architecture**: Established via ADRs 0001-0006 (FFB Mode Matrix, IPC Transport, OWP-1 Protocol, RT Scheduling, Plugin Architecture, Safety Interlocks)

## Milestones

### Phase 1: Foundation ✅ Complete
- [x] Define Core Architecture (ADRs 0001-0006)
- [x] Implement Real-Time Engine loop (1kHz, ≤1000μs budget)
- [x] Implement Linux HID driver (hidraw/udev)
- [x] Implement Linux RT scheduling (SCHED_FIFO/rtkit)
- [x] Implement Windows HID driver (overlapped I/O, MMCSS)
- [x] Initial CLI tools (`wheelctl`) for device management
- [x] Background service (`wheeld`) with IPC

### Phase 2: Feature Completeness ✅ Complete
- [x] **Advanced Force Feedback**
    - [x] Curve-based FFB effects with pre-computed LUTs
    - [x] Profile hierarchy and inheritance (up to 5 levels)
    - [x] Zero-allocation curve application in RT path
- [x] **Game Telemetry Integration**
    - [x] iRacing adapter (shared memory)
    - [x] ACC adapter (UDP)
    - [x] Automobilista 2 adapter (shared memory)
    - [x] rFactor 2 adapter (plugin interface)
    - [x] Telemetry parsing within 1ms budget
- [x] **User Interface**
    - [x] Tauri-based desktop UI
    - [x] Device list and detail views
    - [x] Real-time telemetry display
    - [x] Profile management UI
- [x] **Histogram tracking** for latency metrics (HDRHistogram)

### Phase 3: Production Readiness (Current Focus)
- [ ] **Safety Hardening**
    - [ ] Hardware watchdog integration (100ms timeout)
    - [ ] Fault quarantine implementation (`crates/engine/src/safety/fmea.rs`)
    - [ ] Full replay validation for diagnostics (`crates/engine/src/diagnostic/replay.rs`)
- [ ] **Plugin Ecosystem**
    - [ ] Plugin registry with searchable catalog
    - [ ] `wheelctl plugin install` command
    - [ ] Embedded signature verification (PE/ELF sections)
- [ ] **Firmware Management**
    - [ ] Firmware update system with signature verification
    - [ ] Rollback support on update failure
    - [ ] FFB blocking during firmware updates
- [ ] **Performance Gates (CI)**
    - [ ] RT timing benchmarks in CI pipeline
    - [ ] Automated threshold enforcement (p99 jitter ≤0.25ms)
    - [ ] JSON benchmark output for historical tracking
- [ ] **Migration System**
    - [ ] Automatic profile schema version detection
    - [ ] Profile migration with backup creation

### Phase 4: Ecosystem & Polish
- [ ] **Device Ecosystem Tools**
    - [ ] `openracing-capture` utility (protocol sniffer/mapper)
    - [ ] Device protocol reverse engineering toolkit
- [ ] **macOS Support**
    - [ ] IOKit HID implementation
    - [ ] thread_policy_set RT scheduling
- [ ] **Installer & Packaging**
    - [x] Windows MSI installer (WiX)
    - [ ] Linux packages (deb, rpm, flatpak)
    - [ ] macOS DMG with notarization
- [ ] **Adaptive Scheduling**
    - [ ] Dynamic deadline adjustment based on system load
    - [ ] CPU governor integration

## Future Considerations

- **Cloud Integration**: Profile sharing and cloud backup via OpenRacing Hub
- **Mobile Companion App**: iOS/Android app for remote monitoring and quick adjustments
- **AI/ML Integration**: Adaptive FFB tuning based on driving style analysis
- **Wheel Manufacturer Partnerships**: Official SDK integrations
- **VR Integration**: Direct telemetry to VR headsets for haptic feedback

## Known Technical Debt

The following TODOs exist in the codebase and should be addressed before v1.0.0:

| Location | Issue |
|----------|-------|
| `crates/service/src/security/signature.rs:111` | Replace stub with actual Ed25519 verification |
| `crates/service/src/crypto/mod.rs:204-205` | Implement PE/ELF embedded signature checking |
| `crates/engine/src/scheduler.rs:181` | Implement adaptive scheduling |
| `crates/engine/src/diagnostic/blackbox.rs:152` | Index optimization for large recordings |
| `crates/service/src/integration_tests.rs` | Re-enable disabled integration tests |

## Release Schedule

| Version | Date | Status | Focus |
|---------|------|--------|-------|
| v0.1.0  | 2025-01-01 | ✅ Released | Core Engine & Linux Support |
| v0.2.0  | 2026-02-01 | ✅ Released | Windows Support & Tauri UI |
| v0.3.0  | 2026-02-01 | ✅ Released | WASM Plugins, Game Telemetry, Curve FFB |
| v0.4.0  | 2026-Q2 | Planned | Plugin Registry & Firmware Updates |
| v1.0.0  | 2026-10-15 | Planned | Production Release with Security Audit |

## Contributing

See [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) for development setup and contribution guidelines.

Significant architectural changes require an ADR. See [docs/adr/README.md](docs/adr/README.md) for the process.

---
*Last updated: 2026-02-01. This roadmap is subject to change based on community feedback and technical priorities.*
