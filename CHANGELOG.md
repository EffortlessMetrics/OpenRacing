# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
