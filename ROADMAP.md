# Roadmap

This document outlines the development roadmap for OpenRacing. It tracks the implementation status of key features, architectural decisions, and future plans.

## Current Status (v1.0 RC - 2026)

**Project Scale:** 86 workspace crates in the Rust workspace.

**Released Features:**
- **Core FFB Engine**: Real-time force feedback processing at 1kHz with zero-allocation RT path (P99 jitter ≤0.25ms)
- **Cross-Platform HID**: Full support for Linux (hidraw/udev) and Windows (overlapped I/O, MMCSS)
- **Plugin System**: WASM sandboxed runtime (60–200Hz) + Native plugins (RT-capable, 1kHz) with Ed25519 signature verification
- **Game Telemetry**: 61 telemetry adapter modules — iRacing, ACC, AMS2, rFactor 2, Assetto Corsa, Forza Motorsport/Horizon, BeamNG.drive, Project CARS 2/3, RaceRoom, AC Rally, Dirt 5, EA WRC, F1 series (4 editions), Gran Turismo 7, and more
- **Curve-Based FFB**: Customizable response curves (linear, exponential, logarithmic, Bezier)
- **Profile Inheritance**: Hierarchical profiles with up to 5 levels of inheritance
- **Tauri UI**: Device management, real-time telemetry display, profile application
- **CLI Tools**: `wheelctl` for device management, diagnostics, and profile operations
- **Safety System**: FMEA analysis, fault injection tests, safety interlocks, hardware watchdog, safe mode transitions, black box recording
- **Multi-vendor Device Support**: 28 vendors (15 wheelbase manufacturers + 13 peripheral-only), 159 unique VID/PID pairs across all device protocol crates
- **Protocol Documentation**: All supported devices documented in `docs/protocols/`; VID/PID constants locked to `docs/protocols/SOURCES.md` via `id_verification.rs` test suites
- **Test Infrastructure**: 29,900+ tests (29,955 #[test] + 509 proptest blocks), 117 fuzz targets; all HID crates have cross-reference id_verification suites
- **Linux Packaging**: udev rules for all devices, hwdb for joystick classification (133 entries), kernel quirks (ALWAYS_POLL) for Asetek and Simagic
- **CI Matrix**: Linux (ubuntu-latest/22.04/24.04) + Windows (windows-latest) + macOS (macos-latest) — macOS compilation fixed (PR #97), RT test ignores added (PR #106)

**Architecture**: Established via ADRs 0001-0008 (FFB Mode Matrix, IPC Transport, OWP-1 Protocol, RT Scheduling, Plugin Architecture, Safety Interlocks, Multi-Vendor HID Protocol Architecture, Game Auto-Configure and Telemetry Bridge)

## Milestones

### Phase 1: Foundation ✅ Complete
- [x] Define Core Architecture (ADRs 0001-0006)
- [x] Implement Real-Time Engine loop (1kHz, ≤1000μs budget)
- [x] Implement Linux HID driver (hidraw/udev)
- [x] Implement Linux RT scheduling (SCHED_FIFO/rtkit)
- [x] Implement Windows HID driver (overlapped I/O, MMCSS)
- [x] Initial CLI tools (`wheelctl`) for device management
- [x] Background service (`wheeld`) with IPC
- [x] Curve-based FFB effects with pre-computed LUTs
- [x] Profile hierarchy and inheritance (up to 5 levels)
- [x] Zero-allocation curve application in RT path
- [x] Tauri-based desktop UI with device list, telemetry display, and profile management
- [x] Histogram tracking for latency metrics (HDRHistogram)

### Phase 2: Device Support ✅ Complete

28 vendors supported (15 wheelbase manufacturers + 13 peripheral-only), 159 unique VID/PID pairs.

- [x] **Wheelbase Manufacturers** (15)
    - [x] Logitech (G29, G923, PRO, etc.)
    - [x] Fanatec (CSL, ClubSport, Podium, etc.)
    - [x] Thrustmaster (T300, T-GT, TX, etc.)
    - [x] Moza (R5, R9, R12, R16, R21)
    - [x] Simagic (Alpha, Alpha Mini, Alpha U)
    - [x] Simucube 2 (Sport, Pro, Ultimate)
    - [x] VRS DirectForce Pro
    - [x] Heusinkveld (Sprint, Ultimate, Sim Pedals)
    - [x] Asetek (Forte, La Prima, Invicta)
    - [x] OpenFFBoard (VID 0x1209, direct drive)
    - [x] FFBeast (VID 0x045B, direct drive)
    - [x] Granite Devices IONI/ARGON / SimpleMotion V2 (VID 0x1D50)
    - [x] AccuForce
    - [x] Leo Bodnar
    - [x] PXN (PR #18)
- [x] **Peripheral-only Vendors** (13) — pedals, shifters, handbrakes, button boxes
    - [x] Cammus
    - [x] Generic HID button boxes (VID 0x1209, PID 0x1BBD)
    - [x] 11 additional peripheral vendors
- [x] **PIDFF Consolidation**: All standard USB HID PID device crates now use `openracing-pidff-common` — ~4,338 lines of duplicated effects code eliminated (PRs #52, #72-74)
- [x] **PID Source Verification**: All vendor protocol crates cross-referenced against Linux kernel `hid-ids.h`, hid-tmff2 community driver, and simracing-hwdb; source citations added to all VID/PID constants (PR #76)

### Phase 3: Game Telemetry ✅ Complete

61 game/simulator integrations with telemetry support.

- [x] iRacing adapter (shared memory)
- [x] ACC adapter (UDP)
- [x] Automobilista 2 adapter (shared memory)
- [x] rFactor 2 adapter (plugin interface)
- [x] Assetto Corsa adapter (OutGauge UDP)
- [x] Forza Motorsport/Horizon adapter (Sled/CarDash UDP)
- [x] BeamNG.drive adapter (LFS OutGauge UDP)
- [x] Project CARS 2/3 adapter (shared memory + UDP)
- [x] RaceRoom Experience adapter (R3E shared memory)
- [x] AC Rally adapter (ACC shared memory)
- [x] Dirt 5 adapter (Codemasters UDP)
- [x] EA WRC adapter (Codemasters UDP)
- [x] F1 2024 adapter (Codemasters bridge)
- [x] F1 25 adapter (native UDP format 2025)
- [x] Gran Turismo 7 adapter
- [x] 46 additional game/simulator adapters
- [x] Telemetry parsing within 1ms budget

### Phase 4: Platform & Distribution 🔄 In Progress

- [x] **Windows Packaging**
    - [x] MSI installer (WiX)
- [x] **Linux Packaging**
    - [x] deb/rpm/tarball packages
    - [x] udev rules for all supported devices
    - [x] hwdb for joystick classification (133 entries)
    - [x] Kernel quirks (ALWAYS_POLL) for Asetek and Simagic
- [x] **macOS Support** (compilation)
    - [x] CI matrix added (macos-latest)
    - [x] Compilation fixes — libudev gated to Linux-only (PR #97)
    - [x] RT scheduling tests ignored on macOS CI (PR #106)
    - [ ] IOKit HID driver implementation
    - [ ] thread_policy_set RT scheduling
    - [ ] DMG with notarization
- [x] **Device Ecosystem Tools**
    - [x] `hid-capture` device capture and fingerprinting tooling (379+ tests)
- [ ] **Remaining Device Ecosystem Tools**
    - [ ] `openracing-capture` utility (protocol sniffer/mapper)
    - [ ] Device protocol reverse engineering toolkit
- [ ] **Adaptive Scheduling**
    - [x] Dynamic deadline adjustment based on system load
    - [ ] CPU governor integration

### Phase 5: Polish & 1.0 RC 🔄 In Progress

- [x] **Test Coverage**: 29,900+ tests (29,955 #[test] + 509 proptest blocks + 117 fuzz targets) across unit, integration, property-based, snapshot, and acceptance tests
- [x] **Documentation**: Comprehensive (setup, user guide, device support, development)
- [x] **Performance Gates**: CI-enforced benchmarks (P99 jitter ≤0.25ms, zero RT heap allocations)
- [x] **Safety Hardening**
    - [x] Hardware watchdog integration (100ms timeout, two-layer: software + device keepalive)
    - [x] Safety interlock state machine: `Normal → Warning → SafeMode → EmergencyStop`
    - [x] FMEA fault injection acceptance tests
    - [x] Fault quarantine with TorquePolicy and QuarantineState (`crates/openracing-fmea`)
    - [x] Full replay validation with golden traces
- [x] **Plugin Ecosystem**
    - [x] Plugin registry with searchable catalog
    - [x] `wheelctl plugin install` command
    - [x] Embedded signature verification (ELF/PE sections)
- [x] **Firmware Management**
    - [x] Firmware update system with signature verification
    - [x] Rollback support on update failure
    - [x] FFB blocking during firmware updates
- [x] **Migration System**
    - [x] Automatic profile schema version detection
    - [x] Profile migration with backup creation (idempotent)
- [x] **Security**: Ed25519 trust store implemented with fail-closed mode (PR #105); native plugin signing end-to-end functional

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
| ~~`crates/service/src/security/signature.rs:111`~~ | ~~Replace stub with actual Ed25519 verification~~ — **RESOLVED**: Full Ed25519 implementation with `ed25519-dalek` 2.2.0 |
| ~~Ed25519 trust store~~ | ~~Needs trust store for public key distribution~~ — **RESOLVED**: Fail-closed trust store implemented (PR #105) |
| ~~`crates/service/src/crypto/mod.rs:204-205`~~ | ~~Implement PE/ELF embedded signature checking~~ — **RESOLVED**: PE/ELF/Mach-O parsing implemented via `goblin` |
| ~~`crates/engine/src/diagnostic/blackbox.rs:152`~~ | ~~Index optimization for large recordings~~ — **RESOLVED**: Binary search (`find_index_at_timestamp`, `find_indices_in_range`) added with O(log n) lookup |
| `crates/service/src/integration_tests.rs` | Re-enable disabled integration tests |
| ~~`crates/hid-pxn-protocol/src/output.rs`~~ | ~~PXN FFB_REPORT_ID 0x05 is estimated; verify with USB capture~~ — **RESOLVED**: PXN uses standard PIDFF; `SET_CONSTANT_FORCE=0x05` is per USB PID spec, not vendor-specific |
| ~~`docs/DEVICE_CAPABILITIES.md`~~ | ~~Cube Controls VID/PIDs provisional~~ — **RESOLVED**: Fabricated PIDs removed from FFB dispatch (PR #24) |
| ~~`docs/protocols/SOURCES.md`~~ | ~~Devices Under Investigation table~~ — **UPDATED**: Research dates added, PXN VD-series status updated (Gold in JacKeTUs but PIDs blank), Simucube 3 speculation noted |
| `F-007 (FRICTION_LOG)` | Symbol rename pattern — `#[deprecated]` guidance added to DEVELOPMENT.md; no code changes yet |

## Release Schedule

| Version | Date | Status | Focus |
|---------|------|--------|-------|
| v0.1.0  | 2025-01-01 | ✅ Released | Core Engine & Linux Support |
| v0.2.0  | 2026-02-01 | ✅ Released | Windows Support & Tauri UI |
| v0.3.0  | 2026-02-01 | ✅ Released | WASM Plugins, Game Telemetry, Curve FFB |
| v1.0 RC | 2026-Q3   | ✅ Feature complete | 28 vendors, 61 game integrations, safety hardening, 29,900+ tests |
| v1.0.0  | 2026-10-15 | Planned | Production Release with Security Audit |

## Contributing

See [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) for development setup and contribution guidelines.

Significant architectural changes require an ADR. See [docs/adr/README.md](docs/adr/README.md) for the process.

---
*Last updated: 2026-Q3. This roadmap reflects the current project state: 86 crates, 28 vendors, 159 devices, 61 games, 29,900+ tests.*
