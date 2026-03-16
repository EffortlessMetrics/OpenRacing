# Roadmap

This document outlines the development roadmap for OpenRacing. It tracks the implementation status of key features, architectural decisions, and future plans.

## Current Status (v0.x.y - Pre-Hardware Sign-Off)

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

### Phase 5: Polish & v0.x.y 🔄 In Progress

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
- [x] **Code Quality**: Workspace-wide elimination of `unwrap()` and `expect()` to enforce robust error handling, including within 29,900+ tests

### Phase 6: Device Enumeration — Moza R5 Stack 🔲 Planned

First hardware bring-up. Zero-risk read-only enumeration of all reference devices.

**Reference hardware:**

| Device | Type | VID | PID (V1 / V2) | Max Torque | Status |
|--------|------|-----|----------------|------------|--------|
| Moza R5 | Wheelbase (direct drive) | `0x346E` | `0x0004` / `0x0014` | 5.5 Nm | Protocol complete, needs HIL |
| Moza KS | Wheel rim | — | rim ID `0x05` | — | Input parser ready |
| Moza ES | Wheel rim | — | rim ID `0x06` | — | Input parser ready |
| Moza SR-P | Pedals + clutch | `0x346E` | `0x0003` | — | Standalone parse ready |
| Moza HBP | Handbrake | `0x346E` | `0x0022` | — | Standalone parse ready |

- [ ] Plug in R5 + KS wheel → run `wheelctl device list` → verify VID/PID enumeration on Windows
- [ ] Confirm product name resolves to "Moza R5" and model to `MozaModel::R5`
- [ ] Plug in SR-P pedals standalone → verify enumeration as separate HID device (PID `0x0003`)
- [ ] Plug in HBP handbrake standalone → verify enumeration (PID `0x0022`)
- [ ] Capture raw HID report descriptor for R5 with `hid-capture` tool → save as golden fixture
- [ ] **Gate: all 5 devices enumerate correctly; no FFB output sent**

### Phase 7: Input Report Capture 🔲 Planned

Read-only input validation. No writes to any device.

- [ ] Read raw wheelbase input reports from R5 → validate `parse_wheelbase_input_report` output
- [ ] Verify steering axis (u16) tracks physical wheel rotation continuously
- [ ] Verify KS rim ID `0x05` appears in `funky` field; validate button/hat/rotary/encoder parsing
- [ ] Swap to ES wheel → verify rim ID `0x06`; validate ES button count and joystick mode
- [ ] Verify SR-P pedal axes (throttle, brake, clutch) via `parse_srp_usb_report_best_effort`
- [ ] Verify HBP handbrake axis via `parse_hbp_usb_report_best_effort`
- [ ] Verify aggregated pedal axes when SR-P is connected via wheelbase pedal port (passthrough)
- [ ] Create known-good input report test fixtures from captured data
- [ ] **Gate: all input parsing passes; axes track physical movement; no FFB output sent**

### Phase 8: Handshake & Feature Reports 🔲 Planned

First device writes — feature reports for initialization and configuration. No FFB torque output yet.

- [ ] Execute `MozaProtocol::initialize_device()` handshake sequence against R5
- [ ] Verify init state transitions: `Uninitialized → Initializing → Ready`
- [ ] Verify `start_input_reports` (report `0x03`) succeeds (device begins streaming input)
- [ ] Verify `set_ffb_mode` (report `0x11`, `Standard`) succeeds
- [ ] Test `set_rotation_range` with known value (e.g. 540°) and verify physical soft-stop changes
- [ ] Test handshake retry on simulated failure (unplug during init → `Failed` state → re-plug → retry)
- [ ] Verify `MozaProtocol::reset_to_uninitialized()` on disconnect clears state
- [ ] **Gate: handshake reliable; rotation range responds; no torque commands sent**

### Phase 9: Low-Torque FFB Output 🔲 Planned

Safety-critical phase. Start at ≤10% of max torque (0.55 Nm) and ramp gradually with manual observation.

- [ ] Ensure safety interlock is in `SafeTorque` state before any output
- [ ] Register R5 with safety service at `5.5 Nm` max torque
- [ ] Send constant zero-torque command via `MozaDirectTorqueEncoder::encode_zero()` → motor disabled
- [ ] Send constant 0.55 Nm (10% of max) via `encode()` → verify subtle resistance on wheel
- [ ] Verify motor-enable flag (byte 3, bit 0) is correctly set/cleared
- [ ] Test `encode_zero()` immediately stops torque — critical for emergency path
- [ ] Gradually increase to 25%, 50%, 75%, 100% of max torque with manual observation
- [ ] Test rapid torque direction reversal (clockwise ↔ counter-clockwise)
- [ ] Verify hardware watchdog: unplug USB mid-torque → wheel must go limp within 100ms
- [ ] **Gate: FFB feels correct; safety interlock stops torque on fault; watchdog works**

### Phase 10: Game Telemetry Integration 🔲 Planned

Full loop: game → telemetry adapter → filter pipeline → FFB output → device.

- [ ] Start a supported game (e.g. Assetto Corsa, iRacing)
- [ ] Verify game auto-detection and telemetry adapter startup
- [ ] Verify telemetry data (speed, RPM, wheel angle) flows through pipeline
- [ ] Verify FFB output reflects in-game forces (cornering, kerbs, surface detail)
- [ ] Test game exit → FFB stops cleanly → no residual torque
- [ ] Test device unplug during gameplay → safe state transition
- [ ] Profile hot-swap: switch profiles during active FFB → verify smooth transition
- [ ] **Gate: end-to-end loop works; FFB feels natural; safety transitions clean**

### Phase 11: Extended Validation & Soak 🔲 Planned

Sustained operation testing and final regression capture.

- [ ] 1-hour continuous FFB session with telemetry logging → check for jitter, missed ticks
- [ ] Verify RT timing budget (1kHz loop, P99 jitter ≤ 0.25ms) with real device I/O
- [ ] Full disconnect/reconnect cycle test (10x) → verify clean recovery every time
- [ ] Test with both V1 (`0x0004`) and V2 (`0x0014`) R5 firmware if available
- [ ] Compare `Standard` (PIDFF) vs `Direct` FFB mode feel and latency
- [ ] Validate KS clutch paddles and rotary encoders work in-game for secondary controls
- [ ] Save final golden captures and test fixtures for regression suite
- [ ] **Gate: passes 1hr soak; all peripherals functional; ready for daily use**

### Phase 12: Multi-Vendor Verification, Research & Hardening 🔲 Planned

Expand beyond Moza to all 28 vendors. Protocol research, extended stress testing, and ecosystem tooling.

- [ ] **Hardware-in-the-Loop (HIL) Testing**
    - [ ] USB capture validation against physical Fanatec, Logitech, and Thrustmaster devices
    - [ ] Verify wire-format encoding with oscilloscope/protocol analyzer on at least 3 vendor wheelbases
    - [ ] Validate FFB latency end-to-end (HID write → motor response) with hardware timing equipment
    - [ ] Compare torque output accuracy against manufacturer calibration data
- [ ] **Protocol Research & Cross-Validation**
    - [ ] Cube Controls VID/PID hardware verification (currently PROVISIONAL — requires physical device captures)
    - [ ] VRS DirectForce V2, Pedals V2, Handbrake, Shifter PID confirmation via USB captures
    - [ ] OpenFFBoard `0xFFB1` alt PID field validation (currently removed from dispatch — confirm if real hardware uses it)
    - [ ] Simucube 3 PID research once hardware ships
    - [ ] Cross-validate all 159 VID/PID pairs against latest Linux kernel `hid-ids.h` and community sources
    - [ ] Validate PXN VD-series PIDs (Gold status in JacKeTUs/simracing-hwdb but PIDs blank)
- [ ] **Extended Soak & Stress Testing**
    - [ ] 48-hour continuous operation soak test on Windows and Linux with real devices
    - [ ] Memory leak detection under sustained 1kHz operation (valgrind/DHAT on Linux, ETW on Windows)
    - [ ] Concurrent multi-device stress test (≥3 devices simultaneously)
    - [ ] Profile hot-swap stress test under load (rapid profile switching during active FFB)
    - [ ] IPC stress test with 50+ concurrent client connections
- [ ] **Mutation Testing Expansion**
    - [ ] Expand `cargo-mutants` coverage beyond engine safety to all protocol encoding/decoding paths
    - [ ] Mutation testing for telemetry normalization pipeline (NaN/Inf rejection, unit conversion)
    - [ ] Mutation testing for IPC codec (message framing, version negotiation)
    - [ ] Target: zero surviving mutants in all safety-critical paths
- [ ] **Fuzz Testing Hardening**
    - [ ] Run all 117 fuzz targets for ≥1 hour each with corpus accumulation
    - [ ] Integrate `cargo fuzz` coverage reports into CI dashboard
    - [ ] Add structured fuzzing for IPC wire format (protobuf + custom framing)
    - [ ] Add differential fuzzing: compare telemetry adapter output against reference implementations
- [ ] **Community Device Capture Program**
    - [ ] Publish `CONTRIBUTING_CAPTURES.md` workflow for community USB capture submissions
    - [ ] Create golden capture corpus for regression testing (≥1 capture per vendor)
    - [ ] Automate capture → test fixture pipeline (capture file → known-good byte sequence test)
- [ ] **Service API Completion**
    - [ ] Implement `WheelService::game_service()` accessor to unblock integration tests
    - [ ] Implement `WheelService::plugin_service()` accessor to unblock integration tests
    - [ ] Re-enable `test_game_integration` and `test_plugin_system` integration tests
    - [ ] Implement `connect_device`, `send_ffb_frame`, `get_device_statistics` device APIs
- [ ] **Deprecation & Symbol Rename (F-007)**
    - [ ] Audit all protocol crates for symbols that need `#[deprecated]` migration
    - [ ] Apply `#[deprecated(since = "...", note = "...")]` to identified symbols
    - [ ] Update all internal call sites in the same pass
    - [ ] Schedule removal of deprecated aliases for the following release


## Future Considerations

- **Cloud Integration**: Profile sharing and cloud backup via OpenRacing Hub; cross-machine profile sync
- **Mobile Companion App**: iOS/Android app for remote monitoring, quick adjustments, and telemetry review
- **AI/ML Integration**: Adaptive FFB tuning based on driving style analysis; automatic profile generation from telemetry sessions
- **Wheel Manufacturer Partnerships**: Official SDK integrations with Fanatec, Moza, and Simagic for native API access
- **VR Integration**: Direct telemetry to VR headsets for haptic feedback; motion rig integration via OpenXR
- **Telemetry Dashboard**: Browser-based replay visualization, real-time telemetry overlay, and session comparison tools
- **Advanced Diagnostics**: Live FFB waveform visualization, frequency spectrum analysis, and torque vs. wheel-angle plots
- **Multi-Rig Support**: Manage multiple racing setups (e.g. motion rig + desktop) from a single profile repository
- **Accessibility**: Screen reader support in Tauri UI, high-contrast mode, keyboard-only navigation
- **Localization**: Multi-language support for UI and documentation (community-driven translations)

## Known Technical Debt

The following TODOs exist in the codebase and should be addressed before production release (post-hardware sign-off):

| Location | Issue |
|----------|-------|
| ~~`crates/service/src/security/signature.rs:111`~~ | ~~Replace stub with actual Ed25519 verification~~ — **RESOLVED**: Full Ed25519 implementation with `ed25519-dalek` 2.2.0 |
| ~~Ed25519 trust store~~ | ~~Needs trust store for public key distribution~~ — **RESOLVED**: Fail-closed trust store implemented (PR #105) |
| ~~`crates/service/src/crypto/mod.rs:204-205`~~ | ~~Implement PE/ELF embedded signature checking~~ — **RESOLVED**: PE/ELF/Mach-O parsing implemented via `goblin` |
| ~~`crates/engine/src/diagnostic/blackbox.rs:152`~~ | ~~Index optimization for large recordings~~ — **RESOLVED**: Binary search (`find_index_at_timestamp`, `find_indices_in_range`) added with O(log n) lookup |
| `crates/service/src/integration_tests.rs` | Re-enable disabled integration tests — blocked on `WheelService::game_service()` and `plugin_service()` accessors (see Phase 12) |
| ~~`crates/hid-pxn-protocol/src/output.rs`~~ | ~~PXN FFB_REPORT_ID 0x05 is estimated; verify with USB capture~~ — **RESOLVED**: PXN uses standard PIDFF; `SET_CONSTANT_FORCE=0x05` is per USB PID spec, not vendor-specific |
| ~~`docs/DEVICE_CAPABILITIES.md`~~ | ~~Cube Controls VID/PIDs provisional~~ — **RESOLVED**: Fabricated PIDs removed from FFB dispatch (PR #24) |
| ~~`docs/protocols/SOURCES.md`~~ | ~~Devices Under Investigation table~~ — **UPDATED**: Research dates added, PXN VD-series status updated (Gold in JacKeTUs but PIDs blank), Simucube 3 speculation noted |
| `F-007 (FRICTION_LOG)` | Symbol rename pattern — `#[deprecated]` guidance added to DEVELOPMENT.md; audit and code changes tracked in Phase 12 |
| Cube Controls PIDs | PROVISIONAL — requires physical device captures to confirm; tracked in Phase 12 |
| VRS V2 device PIDs | `0xA356`–`0xA35A` removed as fabricated — need real device captures to determine actual PIDs |
| Soak test coverage | No automated 48-hour soak tests in CI yet; tracked in Phases 11–12 |

## Release Schedule

| Version | Date | Status | Focus |
|---------|------|--------|-------|
| v0.1.0  | 2025-01-01 | ✅ Released | Core Engine & Linux Support |
| v0.2.0  | 2026-02-01 | ✅ Released | Windows Support & Tauri UI |
| v0.3.0  | 2026-02-01 | ✅ Released | WASM Plugins, Game Telemetry, Curve FFB |
| v0.x.y  | 2026-Q3   | 🔄 In Progress | 28 vendors, 61 game integrations, safety hardening, 29,900+ tests, pre-hardware sign-off |
| v0.x.y+1| 2026-Q3/Q4| 🔲 Planned | Phases 6–9: Moza R5 hardware bring-up — enumeration, input capture, handshake, low-torque FFB |
| v0.x.y+2| 2026-Q4   | 🔲 Planned | Phases 10–11: Game telemetry integration, extended soak testing, daily-driver readiness |
| v0.x.y+3| 2027-Q1   | 🔲 Planned | Phase 12: Multi-vendor HIL testing, protocol research, mutation/fuzz hardening |
| v1.0.0  | TBD       | Planned | Production Release with Hardware Sign-Off |

## Contributing

See [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) for development setup and contribution guidelines.

Significant architectural changes require an ADR. See [docs/adr/README.md](docs/adr/README.md) for the process.

---
*Last updated: 2026-03-16. This roadmap reflects the current project state: 86 crates, 28 vendors, 159 devices, 61 games, 29,900+ tests. Version is v0.x.y until hardware sign-off.*
