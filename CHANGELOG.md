# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **FFBeast open-source FF controller support** (VID `0x045B`):
  - PIDs `0x58F9` (joystick), `0x5968` (rudder), `0x59D7` (wheel)
  - `hid-ffbeast-protocol` microcrate: `FFBeastTorqueEncoder`, `build_enable_ffb`, `build_set_gain` feature reports
  - `FFBeastHandler` vendor handler: 2-step init (enable FFB + set full gain), 20 Nm default, 65535 CPR encoder
  - HID report ID `0x60` (enable/disable), `0x61` (gain), `0x01` (constant force output)
  - udev rules + Windows device registry + capabilities block added

- **Generic HID button boxes** (VID `0x1209`, PID `0x1BBD`):
  - `hid-button-box-protocol` microcrate: `ButtonBoxInputReport` parser (up to 32 buttons)
  - `ButtonBoxProtocolHandler`: input-only, no initialization required
  - Compatible with DIY Arduino button boxes, BangButtons, SimRacingInputs, and similar HID gamepad devices
  - 10 insta snapshot tests + 500-case proptest property tests added
  - `RotaryEncoderState::update` overflow fix: delta computation restricted to half-range to prevent `i32` subtraction overflow

- **Leo Bodnar USB interfaces** (VID `0x1DD2`):
  - `racing-wheel-hid-leo-bodnar-protocol` microcrate: `LeoBodnarDevice` enum covering BBI-32, BU0836A, BU0836X, BU0836 16-bit, USB Joystick, Wheel Interface, FFB Joystick, SLI-M Shift Light
  - Protocol constants: HID PID usage page, max torque, encoder CPR, and report size bounds
  - `LeoBodnarHandler` engine vendor handler integrated in the multi-vendor HID pipeline; HID PID constant-force path used for FFB-capable products (`0x000E`, `0x000F`)
  - 14 insta snapshot tests + 4 proptest property tests (512 cases each)

- **Game Telemetry Adapters** — 29 game adapters now in `telemetry-adapters` crate + game support matrix:
  - **Assetto Corsa** — OutGauge UDP, port 9996
  - **Forza Motorsport / Horizon** — Forza Data Out UDP, port 5300
  - **BeamNG.drive** — OutGauge UDP, port 4444
  - **Project CARS 2 / 3** — shared memory `$pcars2$` + UDP port 5606
  - **RaceRoom Racing Experience** — R3E shared memory `$R3E`; full YAML config entry + `raceroom` config writer
  - **iRacing** — shared memory `IRSDKMemMapFileName`
  - **rFactor 2** — shared memory
  - **AMS2 / Automobilista 2** — PCARS2-compatible shared memory protocol
  - **AC Rally** — ACC shared memory protocol
  - **Dirt 5** — Codemasters UDP
  - **EA WRC** — Codemasters UDP
  - **F1 2024** — Codemasters bridge adapter (alias `f1`)
  - **WRC Generations** — Codemasters Mode 1 UDP, port 6777
  - **DiRT 4** — Codemasters Mode 1 UDP, port 20777
  - **Live For Speed** — OutGauge UDP, port 30000
  - **Euro Truck Simulator 2** — SCS shared memory
  - **American Truck Simulator** — SCS shared memory
  - **Wreckfest** — UDP, port 5606
  - **Rennsport** — UDP, port 9000
  - **GRID Autosport** — Codemasters Mode 1 UDP, port 20777
  - **GRID 2019** — Codemasters Mode 1 UDP, port 20777
  - **GRID Legends** — Codemasters UDP
  - **Automobilista 1** — ISI/reiza UDP (OutGauge-compatible), port 4444
  - **KartKraft** — FlatBuffers UDP, port 5678
  - All adapters registered in `adapter_factories()` and tested via BDD parity validation

- **AccuForce SRP protocol** (`crates/hid-accuforce-protocol` microcrate):
  - VID `0x1FC9`, AccuForce Pro PID `0x804C`
  - Pure protocol logic: torque encoder, HID report layout constants, enable/gain feature reports
  - Integrated into the multi-vendor HID pipeline and `SupportedDevices` registry

- **Game-to-Telemetry Bridge** (`crates/service/src/game_telemetry_bridge.rs`):
  - Monitors running processes and auto-starts the matching telemetry adapter when a supported game is detected
  - Stops the adapter automatically when the game process exits
  - Zero user configuration required for supported titles

- **Game Auto-Configure** (`crates/service/src/game_auto_configure.rs`):
  - Writes per-game telemetry config files (UDP port, format) on first detection of a new game
  - Idempotent: no-ops on subsequent runs once the config file exists
  - Enables fully plug-and-play telemetry for all 29 supported games without manual in-game setup steps

- **Expanded test infrastructure**:
  - 9 cargo-fuzz targets for protocol parsers (FFBeast, SimpleMotion V2, Moza, F1 25, Codemasters UDP, and more)
  - Snapshot tests via `insta` crate for all telemetry adapter normalizers
  - End-to-end user journey tests covering device connect → profile apply → FFB output
  - Hardware watchdog FMEA fault scenario tests (missed tick, write failure, thermal warning)
  - Profile migration idempotency tests
  - Expanded property-based tests with `proptest` (500 cases each for torque encoders, protocol parsers, feedback parsing)
  - Total workspace test count: **600+**

- **Safety improvements**:
  - Hardware watchdog acceptance tests: "no feed within 100ms ⇒ SafeMode + zero torque"
  - FMEA fault injection scenario tests (`crates/openracing-fmea`)
  - Safety challenge-response validation integrated into watchdog state machine tests

- **OpenFFBoard open-source direct drive support** (VID `0x1209`):
  - PIDs `0xFFB0` (main) and `0xFFB1` (alt) — covers all production OpenFFBoard firmware releases
  - `racing-wheel-hid-openffboard-protocol` microcrate: `OpenFFBoardTorqueEncoder`, FFB enable/gain feature reports
  - `OpenFFBoardHandler` vendor handler: initializes via feature reports (enable FFB + set max gain), 20 Nm default
  - 8 tests in `openffboard_tests.rs` (all `Result`-returning, no unwrap)
  - udev rules + Windows device registry + capabilities block added

- **Granite Devices SimpleMotion V2 / Simucube 1** (VID `0x1D50`):
  - PIDs: `0x6050` (IONI / Simucube 1, 15 Nm), `0x6051` (IONI Premium, 35 Nm), `0x6052` (ARGON / OSW, 10 Nm)
  - Added to `SupportedDevices::all()`, `determine_device_capabilities()`, `supported_vendor_ids()`, `get_manufacturer_name()`
  - udev rules entry with autosuspend disabled

- **Multi-vendor plug-and-play device support** — 7 vendors now fully handled:
  - **Thrustmaster** (VID `0x044F`): T150, T150 Pro, TMX, T300RS/GT, TX Racing, T500RS, T248/T248X, T-GT/T-GT II, TS-PC Racer, TS-XW, T818 (direct drive), T3PA/T3PA Pro, T-LCM/T-LCM Pro pedals
    - 4-step FFB init: reset gain → set full gain → enable actuators → set rotation range
    - Per-model rotation limits (T818/T-GT/T500RS: 1080°; TS-PC/TS-XW: 1070°; others: 900°)
    - `racing-wheel-hid-thrustmaster-protocol` microcrate
  - **Simucube 2 / Granite Devices** (VID `0x2D6A`): Sport (15 Nm), Pro (25 Nm), Ultimate (35 Nm), ActivePedal, Wireless Wheel
    - Plug-and-play — no initialization sequence required
    - 22-bit angle sensor (4,194,304 CPR), ~360 Hz, 64-byte HID reports
    - `hid-simucube-protocol` microcrate
  - **Asetek SimSports** (VID `0x2E5A`): Forte (20 Nm), Invicta (15 Nm), LaPrima (10 Nm)
    - Plug-and-play — no initialization sequence required
    - `hid-asetek-protocol` microcrate
  - **VRS DirectForce Pro** (VID `0x0483`, PIDs `0xA3xx`): DirectForce Pro (20 Nm), DirectForce Pro V2 (25 Nm), Pedals V1/V2, Handbrake, Shifter
    - 3-step init: enable FFB → set device gain → set 1080° rotation range
    - `racing-wheel-hid-vrs-protocol` microcrate
  - **Heusinkveld** (VID `0x16D0`, PIDs `0x115x`): Sprint (2-pedal), Ultimate+ (3-pedal, 140 kg), Pro (3-pedal, 200 kg)
    - Input-only; no force feedback — pure HID input device
    - `hid-heusinkveld-protocol` microcrate
  - **Simagic modern** (VID `0x2D5C`): Alpha (15 Nm), Alpha Mini (10 Nm), Alpha EVO (15 Nm), M10 (10 Nm), Neo (10 Nm), Neo Mini (7 Nm), P1000/P2000/P1000A pedals, H/Seq shifters, handbrake
    - Active FFB initialization (gain + rotation range) for modern firmware
    - `racing-wheel-hid-simagic-protocol` microcrate (upgraded from passive capture)
  - **VID disambiguation**: `0x0483` (STM) → VRS if PID in `0xA3xx`, else Simagic legacy; `0x16D0` (OpenMoko) → Heusinkveld if PID in `0x115x`, else Simagic legacy
- **Linux udev rules** (`packaging/linux/99-racing-wheel-suite.rules`): Complete rewrite covering all vendors, correct VIDs for Simucube (now `0x2D6A`), VRS, Heusinkveld, Simagic modern, power autosuspend disabled for all racing peripherals
- **Windows device registry** (`crates/engine/src/hid/windows.rs`): All new vendors added to `SupportedDevices` with correct VIDs/PIDs, capability blocks, and manufacturer names; Thrustmaster PID table corrected (T248=`0xB696`, T-LCM Pro=`0xB69A`)
- **Protocol documentation**: Added `SIMUCUBE_PROTOCOL.md`, `VRS_PROTOCOL.md`, `HEUSINKVELD_PROTOCOL.md`, `ASETEK_PROTOCOL.md`; fixed wrong PIDs in `THRUSTMASTER_PROTOCOL.md` (T150, T150 Pro, TMX)
- **VID/PID sources documentation** (`docs/protocols/SOURCES.md`): New reference document recording the authoritative source (verified, community, or estimated) for every USB Vendor ID and Product ID across all protocol crates; required citation policy enforced for new device additions

- **Service IPC capabilities** now properly populated: `DeviceCapabilities` block is read from the device during `initialize_device()` and stored on `ManagedDevice`; IPC `GetDeviceStatus` responses now carry correct capability data instead of `None`

- **Firmware rollback detection** improved: `FirmwareBundleMetadata` gains a `rollback_version` field; `is_upgrade_allowed()` rejects downgrades below the minimum version; corresponding unit test `test_rollback_protection` validates both allow and deny paths

- **YAML sync CI check** added (`ci/yaml-sync-check.yml`): New GitHub Actions workflow runs on every push and pull request; uses `scripts/check_yaml_sync.py` to assert that `crates/telemetry-config/src/game_support_matrix.yaml` and `crates/telemetry-support/src/game_support_matrix.yaml` are byte-for-byte identical, preventing silent divergence

- **Moza Racing hardware support** (wheelbase + peripherals, hardware-ready):
  - `racing-wheel-hid-moza-protocol` microcrate: pure protocol logic (report IDs/offsets, product IDs, handshake frame generator, wheelbase input parser, direct torque encoder, standalone HBP parser, signature verification)
  - `racing-wheel-srp` microcrate: standalone SR-P pedal USB report parser + normalization primitives
  - `racing-wheel-ks` microcrate: map-driven KS wheel controls parser (`KsReportMap`, `KsReportSnapshot`)
  - `racing-wheel-input-maps` microcrate: `DeviceInputMap` schema + `compile_ks_map()` helper
  - Supported wheelbases: R3, R5 V1/V2, R9 V1/V2, R12 V1/V2, R16, R21
  - SR-P Lite embedded pedals (throttle/brake/clutch) via wheelbase aggregated report
  - HBP handbrake: embedded path (wheelbase offset 9–10) + standalone USB (best-effort + capture-driven)
  - KS wheel controls via capture-derived `device_map.json` (never hard-coded)
  - High torque mode is opt-in only (`OPENRACING_MOZA_HIGH_TORQUE=1` or explicit config); default handshake skips HIGH_TORQUE report
  - Handshake state machine with bounded retry (`MozaRetryPolicy`, default 3 retries, exponential backoff), `PermanentFailure` terminal state, and `reset_to_uninitialized()` for reconnect
  - `DeviceSignature` + `verify_signature()` for identity gating (known PIDs → handshake allowed; unknown/peripheral → gated)
  - `VirtualMozaDevice` in `racing-wheel-integration-tests` for deterministic e2e testing without hardware
  - 10 BDD e2e scenarios covering handshake, high-torque gate, retry, disconnect/reconnect, FFB mode, peripherals
  - 4 cargo-fuzz targets: `fuzz_moza_wheelbase_report`, `fuzz_moza_hbp_report`, `fuzz_moza_handshake_frames`, `fuzz_moza_direct_torque_encode`
  - `mutants.toml` scoping cargo-mutants to `hid-moza-protocol`, `ks`, and `input-maps` crates
  - Engine re-exports all Moza types via thin wrappers — no downstream churn

- **EA F1 25 Native UDP Adapter** (`game_id = "f1_25"`): Native binary protocol support
  - Parses EA F1 25 UDP packets (format 2025) directly — no bridge required
  - Decodes PacketCarTelemetryData (ID=6): speed, gear, RPM, DRS, throttle, brake, tyre pressures/temps
  - Decodes PacketCarStatusData (ID=7): fuel mass, ERS energy, pit limiter, tyre compound, engine power
  - Decodes PacketSessionData (ID=1): track ID, ambient/track temperature
  - All F1-specific fields exposed as typed extended telemetry (`drs_active`, `fuel_remaining_kg`, `ers_store_j`, `tyre_pressure_*`, etc.)
  - Config writer generates `Documents/OpenRacing/f1_25_contract.json` with game setup instructions
  - In-game setup: Settings → Telemetry → UDP Telemetry: On, Port: 20777, Format: 2025
  - `f1_25` registered in game support matrix, adapter factory, and config writer factory
  - BDD parity validation automatically covers `f1_25` (adapter + writer both registered)
  - 40+ unit tests, property tests (100 cases, <1ms budget), golden tests, binary fixture codec tests
  - cargo-fuzz targets for header, CarTelemetry, CarStatus, and end-to-end normalize() parsing
  - `f1_2025` is now an alias for `f1_25`; legacy `f1` (Codemasters bridge) adapter unchanged

## [1.0.0-rc.1] - 2026-11-01

### Added

- **Cammus C5/C12 direct drive support**:
  - New `racing-wheel-hid-cammus-protocol` SRP microcrate with `CammusInputReport` parser
  - 64-byte HID report ID `0x01`: steering (±540°), throttle/brake/clutch/handbrake axes, 16-button bitmask
  - `CammusTorqueEncoder` for constant-force output, `build_enable_ffb` / `build_set_gain` feature reports
  - Covers C5 (8 Nm, PID `0x0C5x`) and C12 (12 Nm, PID `0x0C12`)
  - udev rules + Windows device registry + capabilities block added

- **DiRT Rally 2.0 telemetry adapter** (`game_id = "dirt_rally_2"`):
  - Codemasters UDP Mode 1 protocol, port 20777
  - Decodes all motion channels: speed, RPM, gear, throttle, brake, steering angle, lateral/longitudinal G, fuel %, tyre temps/pressures
  - Config writer generates instructions for enabling UDP output in-game
  - Registered in `adapter_factories()`, game support matrix, and config writer factory

- **Gran Turismo 7 telemetry adapter** (`game_id = "gran_turismo_7"`):
  - Salsa20-encrypted UDP on port 33740 (PS4/PS5 console broadcast)
  - Pure-Rust Salsa20 keystream implementation using the embedded 32-byte key and 8-byte nonce at offset `0x40`
  - Decodes: RPM, speed, gear, throttle, brake, fuel %, engine temp, per-corner tyre temps, flags, car code
  - Config writer documents PS4/PS5 network setup to direct packets to the host PC
  - Registered in `adapter_factories()`, game support matrix, and config writer factory

- **Richard Burns Rally telemetry adapter** (`game_id = "rbr"`):
  - RSF LiveData UDP protocol, port 6776
  - Decodes: speed, RPM, gear, throttle, brake, clutch, steering angle, FFB scalar, handbrake flag
  - Config writer generates RSF plugin configuration (`RBRPlugin.ini` path and UDP target)
  - Registered in `adapter_factories()`, game support matrix, and config writer factory

- **Config writers for `dirt_rally_2`, `rbr`, and `gran_turismo_7`** games added to the config-writers crate

- **Game support matrix synchronization**: `crates/telemetry-config` and `crates/telemetry-support` both now carry the full 12-game matrix (iracing, acc, ac_rally, ams2, rfactor2, eawrc, f1, f1_25, dirt5, dirt_rally_2, rbr, gran_turismo_7) with consistent `config_writer` entries

### Changed

- **`NormalizedTelemetry` snapshot serialization**: switched internal `extended` map from `HashMap` to `BTreeMap` for deterministic key ordering in JSON/YAML snapshots and golden tests
- **`has_rpm_data()` semantics fixed**: now returns `true` only when a valid RPM value is present (non-zero, non-NaN); new companion `has_rpm_display_data()` returns `true` when RPM is available for gauge/LED display even if not used for FFB
- **`is_game_running()` semantics fixed**: returns `Ok(false)` (not an error) when the game ID is known but has no active adapter registered, eliminating spurious error log noise during normal operation

### Fixed

- **Test stability — soft-stop multiplier**: clamped the configurable soft-stop torque multiplier to `[0.0, 1.0]` to prevent oscillation in tests that use boundary values
- **Test stability — zero-alloc stderr capture**: replaced heap-allocating stderr capture helper with a fixed-size ring buffer, eliminating spurious allocation warnings in RT path tests

## [1.0.0] - 2026-10-15

### Added

- **Production Safety Interlocks**: FMEA-validated safety system
  - Hardware watchdog integration with 100ms timeout
  - Automatic zero-torque command on watchdog timeout within 1ms
  - Maximum torque limit enforcement based on device capabilities
  - Fault detection with automatic safe mode transition
  - Communication loss handling with safe state within 50ms
  - Emergency stop via dedicated input or software command
- **Performance Validation Gates**: Automated performance regression prevention
  - RT timing benchmarks integrated into CI pipeline
  - Threshold enforcement: RT loop ≤1000μs, p99 jitter ≤0.25ms
  - Processing time gates: ≤50μs median, ≤200μs p99
  - Missed tick rate validation: ≤0.001%
  - JSON benchmark output for historical tracking
- **Plugin Registry**: Centralized plugin discovery and installation
  - Searchable plugin catalog with metadata
  - Signature verification for registry plugins
  - Semantic versioning compatibility checking
  - `wheelctl plugin install` command for easy installation
- **Firmware Update System**: Safe device firmware management
  - Firmware image signature verification
  - Rollback support on update failure
  - FFB operation blocking during updates
  - Local firmware cache for offline updates
- **Migration System**: Seamless upgrade path from previous versions
  - Automatic profile schema version detection
  - Profile migration with backup creation
  - Backup restoration on migration failure
  - Backward compatibility within major versions
- **Complete Documentation**: Comprehensive user and developer guides
  - User Guide with installation and configuration instructions
  - API documentation via rustdoc for all public interfaces
  - Plugin Development Guide with WASM and native examples
  - Protocol documentation for all supported wheel manufacturers
  - Troubleshooting guides for common issues

### Changed

- **BREAKING**: Profile schema v2 with inheritance support
  - Profiles now support parent-child relationships
  - Settings merge with child values overriding parent values
  - Inheritance chain resolution up to 5 levels deep

### Security

- Completed third-party security audit
- All cryptographic implementations verified (Ed25519 signatures)
- Plugin sandboxing escape prevention validated
- IPC interface injection attack prevention verified
- Zero critical vulnerabilities in dependency audit (cargo-audit, cargo-deny)

## [0.3.0] - 2026-02-01

### Added

- **WASM Plugin Runtime**: Sandboxed plugin execution using wasmtime
  - Memory and CPU resource limits for plugin isolation
  - Stable ABI for DSP filter plugins
  - Panic isolation - plugin crashes don't affect the service
  - Hot-reload support without service restart
  - Resource limit enforcement with automatic plugin termination
- **Native Plugin Signature Verification**: Ed25519 cryptographic signatures
  - Signature verification for all native plugins before loading
  - Detached signature file support (.sig files)
  - Security warnings logged for invalid signatures
  - Configurable unsigned plugin policy (allow_unsigned_plugins option)
- **Trust Store**: Centralized management of trusted plugin signers
  - Add/remove/query operations for trusted public keys
  - Persistent storage to disk
  - Key fingerprint-based trust verification
- **Native Plugin ABI Compatibility**: Version checking for native plugins
  - ABI version verification before plugin execution
  - Clear error messages for version mismatches
- **Curve-Based FFB Effects**: Customizable force feedback response curves
  - Cubic Bezier curves for torque response mapping
  - Multiple curve types: linear, exponential, logarithmic, custom Bezier
  - Pre-computed lookup tables (LUT) for RT-safe evaluation
  - Zero-allocation curve application in the RT path
  - Curve parameter validation with descriptive error messages
- **Profile Inheritance**: Hierarchical profile system
  - Parent-child profile relationships
  - Settings merge with child values overriding parent values
  - Inheritance chain resolution up to 5 levels deep
  - Circular inheritance detection with clear error messages
  - Parent change notification for dependent child profiles
- **Game Telemetry Adapters**: Native integration with racing simulators
  - iRacing adapter via shared memory
  - Assetto Corsa Competizione (ACC) adapter via UDP
  - Automobilista 2 (AMS2) adapter via shared memory
  - rFactor 2 adapter via plugin interface
  - Telemetry parsing within 1ms performance budget
  - Graceful disconnection handling with FFB engine notification

### Changed

- Profile schema updated to support optional parent field for inheritance
- Pipeline compilation now supports response curve integration

### Fixed

- Various clippy warnings resolved across the codebase

## [0.2.0] - 2026-02-01

### Added

- **Windows HID Driver**: Full Windows HID device support with overlapped I/O
  - Real device enumeration using hidapi for all supported wheel manufacturers
  - Device filtering by VID/PID for Logitech, Fanatec, Thrustmaster, Moza, and Simagic wheels
  - Windows device notification registration for hotplug events (WM_DEVICECHANGE)
  - Overlapped I/O for non-blocking HID writes in the RT path
  - MMCSS integration for real-time thread priority ("Pro Audio" category)
  - DeviceEvent::Connected/Disconnected events within 500ms of device state change
- **Tauri UI**: Graphical user interface for device and profile management
  - Device list view showing connected racing wheel devices
  - Device detail view with health, temperature, and fault status
  - Profile management with loading and applying FFB profiles
  - Real-time telemetry display (wheel angle, temperature, fault status)
  - Error banner component for user-friendly error messages
  - IPC communication with wheeld service
- **Windows Installer**: Professional MSI installer using WiX toolset
  - wheeld service registration with automatic startup
  - Device permissions configuration via SetupAPI (udev-equivalent)
  - MMCSS task registration for real-time priority
  - Power management configuration (USB selective suspend disabled)
  - Clean uninstallation with service stop/remove, file cleanup, and registry cleanup
  - Silent installation support via `msiexec /quiet`
  - Start menu and desktop shortcuts

### Changed

- Updated Tauri dependency to 2.x with WebKitGTK 4.1 support for Linux compatibility
- UI crate now builds successfully on Ubuntu 22.04 and 24.04

### Fixed

- Fixed webkit2gtk version compatibility issues on Ubuntu 24.04
- Fixed rand_core version conflict with ed25519-dalek for cryptographic operations

## [0.1.0] - 2025-01-01

### Added

- **Core FFB Engine**: Real-time force feedback processing at 1kHz with deterministic latency
  - Zero-allocation real-time path for memory-safe processing
  - Configurable FFB pipeline with filter chain support
  - Frame-based processing architecture
- **Linux HID Support**: Full HID device support via hidraw/udev
  - Device enumeration and hotplug detection
  - Asynchronous HID read/write operations
  - udev rules for device permissions
- **CLI Tool (`wheelctl`)**: Command-line interface for device management
  - `wheelctl device list` - List connected racing wheel devices
  - `wheelctl device status <id>` - View device status and health
  - `wheelctl profile apply <id> <path>` - Apply FFB profiles to devices
  - `wheelctl health` - Check system health status
  - `wheelctl diag test` - Run diagnostic tests
- **Background Service (`wheeld`)**: System service for continuous device management
  - IPC interface for CLI and UI communication
  - Device lifecycle management
  - Profile persistence and application
- **Safety System**: Foundational safety interlocks
  - Fault detection and logging
  - Safe mode transitions
  - Black box recording for diagnostics
- **Profile Management**: JSON-based FFB profile system
  - Schema validation for profile files
  - Profile loading and application
- **Diagnostic System**: Comprehensive diagnostic capabilities
  - Black box recording and replay
  - Support bundle generation
- **Schemas Crate**: Protocol buffer and JSON schema definitions
  - Domain types (DeviceId, TorqueNm, etc.)
  - Entity definitions (Device, Profile, Settings)
- **Plugin Architecture Foundation**: Initial plugin system structure
  - Plugin trait definitions
  - WASM and native plugin scaffolding
