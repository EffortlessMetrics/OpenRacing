# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
