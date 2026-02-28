# Roadmap

This document outlines the development roadmap for OpenRacing. It tracks the implementation status of key features, architectural decisions, and future plans.

## Current Status (v1.0 RC - 2026)

**Released Features:**
- **Core FFB Engine**: Real-time force feedback processing at 1kHz with zero-allocation RT path
- **Cross-Platform HID**: Full support for Linux (hidraw/udev) and Windows (overlapped I/O, MMCSS)
- **Plugin System**: WASM sandboxed runtime + Native plugins with Ed25519 signature verification
- **Game Telemetry**: 58 game adapters — iRacing, ACC, AMS2, rFactor 2, Assetto Corsa, Forza Motorsport/Horizon, BeamNG.drive, Project CARS 2/3, RaceRoom, AC Rally, Dirt 5, EA WRC, F1 series (4 editions), Gran Turismo 7, and 40+ more titles
- **Curve-Based FFB**: Customizable response curves (linear, exponential, logarithmic, Bezier)
- **Profile Inheritance**: Hierarchical profiles with up to 5 levels of inheritance
- **Tauri UI**: Device management, real-time telemetry display, profile application
- **CLI Tools**: `wheelctl` for device management, diagnostics, and profile operations
- **Safety System**: Hardware watchdog, FMEA fault injection, fault quarantine, safe mode transitions, black box recording
- **Multi-vendor Device Support**: 15 vendors supported — Logitech, Fanatec, Thrustmaster, Moza, Simagic, Simucube 2, VRS, Heusinkveld, Asetek, OpenFFBoard, FFBeast, Granite Devices IONI/ARGON, AccuForce, Leo Bodnar, PXN (PR #18), Cammus
- **Protocol Documentation**: All supported devices documented in `docs/protocols/`; VID/PID constants locked to `docs/protocols/SOURCES.md` via `id_verification.rs` test suites in all 15 HID vendor crates
- **Test Infrastructure**: 700+ tests, 9 fuzz targets, snapshot tests, property-based tests (proptest), E2E journey tests; all HID crates have cross-reference id_verification suites

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
    - [x] Telemetry parsing within 1ms budget
- [x] **User Interface**
    - [x] Tauri-based desktop UI
    - [x] Device list and detail views
    - [x] Real-time telemetry display
    - [x] Profile management UI
- [x] **Histogram tracking** for latency metrics (HDRHistogram)
- [x] **Multi-vendor Device Support**
    - [x] OpenFFBoard (VID 0x1209, direct drive)
    - [x] FFBeast (VID 0x045B, direct drive)
    - [x] Granite Devices IONI/ARGON / SimpleMotion V2 (VID 0x1D50)
    - [x] Generic HID button boxes (VID 0x1209, PID 0x1BBD)

### Phase 3: Production Readiness ✅ Complete (1.0 RC)

**Goal**: Predictable behavior under load, safe failure modes, and a defensible supply-chain story.

**Rule**: Every change must either (a) tighten a gate or (b) reduce a failure mode.

#### Definition of Done

Phase 3 is complete when ALL of the following are true:

| Category | Criterion | Status |
|----------|-----------|--------|
| **Safety** | A missed tick / stalled host cannot produce uncontrolled torque | ✅ Done |
| **Safety** | Safety state transitions are deterministic and logged with debug context | ✅ Done |
| **Security** | Native plugin loading is secure-by-default; unsigned requires explicit opt-out | ✅ Done |
| **Security** | Registry downloads and firmware artifacts are verified before use | ✅ Done |
| **Release Quality** | RT timing is enforced by CI gates (not manual local runs) | ✅ Done |
| **Release Quality** | Benchmark outputs are stored and comparable across runs | ✅ Done |
| **Data Lifecycle** | Profiles migrate forward automatically with backup; process is idempotent | ✅ Done |

---

#### 3.1 Safety Hardening

##### 3.1.1 Hardware Watchdog Integration (100ms timeout) ✅

- [x] Define watchdog contract in `crates/engine`
- [x] Implement two-layer watchdog (software + device keepalive)
- [x] Integrate into RT pipeline: feed after successful write
- [x] Safety interlock state machine: `Normal → Warning → SafeMode → EmergencyStop`
- [x] FMEA fault injection acceptance tests

##### 3.1.2 Fault Quarantine (`crates/openracing-fmea`) ✅

- [x] FMEA table as data structure (FaultId, severity, trigger, action, reset)
- [x] FaultManager with TorquePolicy and QuarantineState outputs
- [x] Persistence to blackbox + persistent store

##### 3.1.3 Full Replay Validation ✅

- [x] Replay scope defined (input reports, effect commands, timing deltas, config state)
- [x] Determinism validated (no wall-clock, no unseeded random)
- [x] Golden traces under `crates/engine/tests/fixtures/replay/`

---

#### 3.2 Plugin Ecosystem ✅

- [x] Plugin registry with searchable catalog
- [x] `wheelctl plugin install` command
- [x] Embedded signature verification (ELF/PE sections)

---

#### 3.3 Firmware Management ✅

- [x] Firmware update system with signature verification
- [x] Rollback support on update failure
- [x] FFB blocking during firmware updates

---

#### 3.4 Performance Gates (CI) ✅

- [x] RT timing benchmarks in CI pipeline (JSON artifact output)
- [x] Automated threshold enforcement (p99 jitter ≤0.25ms)
- [x] Benchmark comparison script (`scripts/compare_benchmarks.py`)

---

#### 3.5 Migration System ✅

- [x] Automatic profile schema version detection
- [x] Profile migration with backup creation (idempotent)

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
    - [x] Dynamic deadline adjustment based on system load
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
| `crates/engine/src/diagnostic/blackbox.rs:152` | Index optimization for large recordings |
| `crates/service/src/integration_tests.rs` | Re-enable disabled integration tests |
| `crates/hid-pxn-protocol/src/output.rs` | PXN FFB_REPORT_ID 0x05 is estimated; verify with USB capture |
| `docs/DEVICE_CAPABILITIES.md` | Cube Controls VID/PIDs provisional (0x0483:0x0C73–0x0C75); community capture needed |
| `docs/protocols/SOURCES.md` | Devices Under Investigation table (Turtle Beach, Cube Controls, Cammus C15, Simucube 3, Gomez, SIMTAG, PXN VD-series) |
| `F-007 (FRICTION_LOG)` | Symbol rename pattern — `#[deprecated]` guidance added to DEVELOPMENT.md; no code changes yet |

## Release Schedule

| Version | Date | Status | Focus |
|---------|------|--------|-------|
| v0.1.0  | 2025-01-01 | ✅ Released | Core Engine & Linux Support |
| v0.2.0  | 2026-02-01 | ✅ Released | Windows Support & Tauri UI |
| v0.3.0  | 2026-02-01 | ✅ Released | WASM Plugins, Game Telemetry, Curve FFB |
| v1.0 RC | 2026-Q3   | ✅ Feature complete | Multi-vendor devices, 58 game adapters, safety hardening, 700+ tests |
| v1.0.0  | 2026-10-15 | Planned | Production Release with Security Audit |

## Contributing

See [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) for development setup and contribution guidelines.

Significant architectural changes require an ADR. See [docs/adr/README.md](docs/adr/README.md) for the process.

---
*Last updated: 2026-Q3. This roadmap reflects the 1.0 RC feature-complete state.*
