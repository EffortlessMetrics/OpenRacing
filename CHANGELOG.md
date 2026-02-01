# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
