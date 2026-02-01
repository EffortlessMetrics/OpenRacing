# Requirements Document

## Introduction

This document defines the requirements for the OpenRacing release roadmap from the current state (v0.1.0-dev) through to the stable v1.0.0 release. The roadmap covers four major milestones: v0.1.0 Alpha (immediate), v0.2.0 Windows & UI, v0.3.0 Plugin System & Advanced FFB, and v1.0.0 Stable Release.

OpenRacing is a safety-critical racing wheel force feedback software built in Rust, targeting 1kHz real-time processing with deterministic latency. The release roadmap ensures systematic progression from alpha quality to production-ready software.

## Glossary

- **RT_Path**: The real-time processing path that runs at 1kHz with zero heap allocations
- **FFB**: Force Feedback - haptic feedback sent to racing wheel motors
- **HID**: Human Interface Device - USB protocol for input devices
- **WASM**: WebAssembly - sandboxed plugin execution environment
- **CI**: Continuous Integration - automated build and test pipeline
- **MSRV**: Minimum Supported Rust Version
- **FMEA**: Failure Mode and Effects Analysis - safety analysis methodology
- **MSI**: Microsoft Installer - Windows installation package format
- **Tauri**: Cross-platform UI framework using web technologies
- **MMCSS**: Multimedia Class Scheduler Service - Windows RT scheduling API

## Requirements

### Requirement 1: CHANGELOG and Release Documentation

**User Story:** As a user or contributor, I want a well-maintained CHANGELOG, so that I can understand what changed between releases.

#### Acceptance Criteria

1. THE Release_System SHALL maintain a CHANGELOG.md file following Keep a Changelog format
2. WHEN a release is tagged, THE Release_System SHALL include all changes since the previous release in the CHANGELOG
3. THE CHANGELOG SHALL categorize changes as Added, Changed, Deprecated, Removed, Fixed, or Security
4. WHEN a breaking change is introduced, THE CHANGELOG SHALL clearly mark it with a Breaking Change indicator
5. THE Release_System SHALL include release dates in ISO 8601 format for each version

### Requirement 2: Test Stability and CI Reliability

**User Story:** As a developer, I want stable and fast tests, so that CI provides reliable feedback without flaky failures.

#### Acceptance Criteria

1. WHEN the test_hotplug_stress_basic test runs, THE Test_Framework SHALL complete within 30 seconds
2. THE CI_Pipeline SHALL pass consistently on both Ubuntu and Windows runners
3. WHEN a test requires extended time, THE Test_Framework SHALL mark it with #[ignore] and document the reason
4. THE Test_Suite SHALL achieve zero flaky test failures over 10 consecutive CI runs
5. IF a test times out, THEN THE Test_Framework SHALL provide diagnostic output indicating the cause

### Requirement 3: v0.1.0 Alpha Release

**User Story:** As a project maintainer, I want to tag and publish the first alpha release, so that early adopters can test the core functionality.

#### Acceptance Criteria

1. THE Release_System SHALL create a git tag v0.1.0-alpha with signed commits
2. THE README SHALL display CI status badges for build and test status
3. WHEN v0.1.0-alpha is released, THE Documentation SHALL include installation instructions for Linux
4. THE Alpha_Release SHALL include working wheelctl CLI with device list, status, and health commands
5. THE Alpha_Release SHALL include working wheeld service binary
6. WHEN building on Linux, THE Build_System SHALL produce functional binaries without errors

### Requirement 4: Windows HID Driver Completion

**User Story:** As a Windows user, I want full HID device support, so that I can use my racing wheel with OpenRacing on Windows.

#### Acceptance Criteria

1. THE Windows_HID_Driver SHALL enumerate all supported racing wheel devices (Logitech, Fanatec, Thrustmaster, Moza, Simagic)
2. WHEN a device is connected, THE Windows_HID_Driver SHALL detect it within 2 seconds
3. THE Windows_HID_Driver SHALL use overlapped I/O for non-blocking writes in the RT path
4. WHEN writing FFB reports, THE Windows_HID_Driver SHALL complete within 200μs p99 latency
5. THE Windows_HID_Driver SHALL integrate with MMCSS for RT thread priority
6. WHEN a device is disconnected, THE Windows_HID_Driver SHALL emit a DeviceEvent::Disconnected event within 500ms
7. IF a HID write fails, THEN THE Windows_HID_Driver SHALL return an appropriate RTError without blocking

### Requirement 5: UI Crate Linux Build Fix

**User Story:** As a Linux developer, I want the UI crate to build on modern Linux distributions, so that I can develop and test the UI locally.

#### Acceptance Criteria

1. WHEN building on Ubuntu 24.04, THE UI_Crate SHALL compile without webkit2gtk version errors
2. THE UI_Crate SHALL depend on Tauri 2.x with WebKitGTK 4.1 compatibility
3. IF webkit2gtk is unavailable, THEN THE Build_System SHALL provide a clear error message with resolution steps
4. THE UI_Crate SHALL build successfully on Ubuntu 22.04 and 24.04

### Requirement 6: Windows Installer and Service

**User Story:** As a Windows user, I want a professional installer, so that I can easily install and manage OpenRacing.

#### Acceptance Criteria

1. THE Windows_Installer SHALL produce an MSI package using WiX toolset
2. WHEN installed, THE Windows_Installer SHALL register wheeld as a Windows service
3. THE Windows_Installer SHALL install udev-equivalent device permissions via SetupAPI
4. WHEN uninstalled, THE Windows_Installer SHALL cleanly remove all files and service registrations
5. THE Windows_Installer SHALL support silent installation via msiexec /quiet
6. THE Windows_Service SHALL start automatically on system boot with appropriate privileges

### Requirement 7: Basic Tauri UI

**User Story:** As a user, I want a graphical interface, so that I can manage devices and profiles without using the command line.

#### Acceptance Criteria

1. THE Tauri_UI SHALL display a list of connected racing wheel devices
2. WHEN a device is selected, THE Tauri_UI SHALL show device status including connection state and health
3. THE Tauri_UI SHALL allow loading and applying FFB profiles to devices
4. THE Tauri_UI SHALL display real-time telemetry data (wheel angle, temperature, fault status)
5. WHEN an error occurs, THE Tauri_UI SHALL display a user-friendly error message
6. THE Tauri_UI SHALL communicate with wheeld service via the existing IPC interface

### Requirement 8: WASM Plugin Runtime

**User Story:** As a plugin developer, I want to write safe plugins in any language that compiles to WASM, so that I can extend OpenRacing without risking system stability.

#### Acceptance Criteria

1. THE WASM_Runtime SHALL load and execute WASM plugins using wasmtime
2. WHEN a WASM plugin is loaded, THE WASM_Runtime SHALL sandbox it with memory and CPU limits
3. THE WASM_Runtime SHALL expose a stable ABI for DSP filter plugins
4. WHEN a WASM plugin panics, THE WASM_Runtime SHALL catch the error and disable the plugin without crashing
5. THE WASM_Runtime SHALL support hot-reloading of plugins without service restart
6. IF a WASM plugin exceeds resource limits, THEN THE WASM_Runtime SHALL terminate it and log the violation

### Requirement 9: Native Plugin Loading with Signature Verification

**User Story:** As a power user, I want to load native plugins for maximum performance, so that I can run custom DSP filters in the RT path.

#### Acceptance Criteria

1. THE Native_Plugin_Loader SHALL load shared libraries (.dll/.so) at runtime
2. WHEN loading a native plugin, THE Native_Plugin_Loader SHALL verify Ed25519 signatures
3. IF a plugin signature is invalid, THEN THE Native_Plugin_Loader SHALL refuse to load it and log a security warning
4. THE Native_Plugin_Loader SHALL support a trust store for managing trusted plugin signers
5. WHEN a native plugin is loaded, THE Native_Plugin_Loader SHALL verify ABI compatibility before execution
6. THE Native_Plugin_Loader SHALL allow unsigned plugins only when explicitly configured

### Requirement 10: Curve-Based FFB Effects

**User Story:** As a sim racer, I want customizable FFB response curves, so that I can tune the feel to my preferences.

#### Acceptance Criteria

1. THE FFB_Engine SHALL support cubic Bezier curves for torque response mapping
2. WHEN applying a curve, THE FFB_Engine SHALL interpolate values in the RT path without allocation
3. THE FFB_Engine SHALL support multiple curve types: linear, exponential, logarithmic, and custom Bezier
4. WHEN a profile specifies a curve, THE FFB_Engine SHALL apply it to all torque outputs
5. THE Curve_System SHALL validate curve parameters to prevent invalid configurations

### Requirement 11: Profile Hierarchy and Inheritance

**User Story:** As a user with multiple games, I want profile inheritance, so that I can maintain a base configuration with game-specific overrides.

#### Acceptance Criteria

1. THE Profile_System SHALL support parent-child profile relationships
2. WHEN loading a child profile, THE Profile_System SHALL merge it with parent settings
3. THE Profile_System SHALL resolve inheritance chains up to 5 levels deep
4. WHEN a parent profile changes, THE Profile_System SHALL notify dependent child profiles
5. IF a circular inheritance is detected, THEN THE Profile_System SHALL reject the configuration with a clear error

### Requirement 12: Game Telemetry Adapters

**User Story:** As a sim racer, I want native game integration, so that OpenRacing can receive telemetry directly from my racing games.

#### Acceptance Criteria

1. THE Telemetry_System SHALL support iRacing telemetry via shared memory
2. THE Telemetry_System SHALL support Assetto Corsa Competizione telemetry via UDP
3. THE Telemetry_System SHALL support Automobilista 2 telemetry via shared memory
4. THE Telemetry_System SHALL support rFactor 2 telemetry via plugin interface
5. WHEN telemetry is received, THE Telemetry_System SHALL parse it within 1ms
6. IF a game disconnects, THEN THE Telemetry_System SHALL gracefully handle the disconnection and notify the FFB engine

### Requirement 13: Full Documentation

**User Story:** As a new user or contributor, I want comprehensive documentation, so that I can understand and use OpenRacing effectively.

#### Acceptance Criteria

1. THE Documentation SHALL include a complete User Guide with installation and configuration instructions
2. THE Documentation SHALL include API documentation generated by rustdoc for all public interfaces
3. THE Documentation SHALL include a Plugin Development Guide with examples
4. THE Documentation SHALL include protocol documentation for all supported wheel manufacturers
5. WHEN a new feature is added, THE Documentation SHALL be updated before the feature is released
6. THE Documentation SHALL include troubleshooting guides for common issues

### Requirement 14: Performance Validation Gates

**User Story:** As a maintainer, I want automated performance validation, so that regressions are caught before release.

#### Acceptance Criteria

1. THE CI_Pipeline SHALL run RT timing benchmarks on every PR
2. WHEN benchmark results exceed thresholds, THE CI_Pipeline SHALL fail the build
3. THE Performance_Gates SHALL enforce: RT loop ≤1000μs total, p99 jitter ≤0.25ms, missed ticks ≤0.001%
4. THE Performance_Gates SHALL enforce: processing time ≤50μs median, ≤200μs p99
5. WHEN performance regresses, THE CI_Pipeline SHALL report the specific metric that failed
6. THE Benchmark_System SHALL produce JSON output for historical tracking

### Requirement 15: Security Audit Completion

**User Story:** As a user of safety-critical software, I want security assurance, so that I can trust OpenRacing with my hardware.

#### Acceptance Criteria

1. THE Security_Audit SHALL verify all cryptographic implementations (Ed25519 signatures)
2. THE Security_Audit SHALL verify plugin sandboxing prevents escape
3. THE Security_Audit SHALL verify IPC interfaces are not vulnerable to injection attacks
4. WHEN a security issue is found, THE Security_Process SHALL document it and track remediation
5. THE Security_Audit SHALL be performed by an independent reviewer before v1.0.0
6. THE Dependency_Audit SHALL pass cargo-audit and cargo-deny checks with zero critical vulnerabilities

### Requirement 16: Plugin Marketplace/Registry

**User Story:** As a plugin developer, I want a central registry, so that users can discover and install my plugins easily.

#### Acceptance Criteria

1. THE Plugin_Registry SHALL provide a searchable catalog of available plugins
2. WHEN a plugin is submitted, THE Plugin_Registry SHALL verify its signature
3. THE Plugin_Registry SHALL display plugin metadata: name, author, version, description, compatibility
4. THE CLI SHALL support installing plugins from the registry via wheelctl plugin install
5. WHEN installing a plugin, THE CLI SHALL verify the signature matches the registry entry
6. THE Plugin_Registry SHALL support semantic versioning for plugin compatibility

### Requirement 17: Firmware Update System

**User Story:** As a hardware owner, I want to update my wheel's firmware through OpenRacing, so that I can get the latest features and fixes.

#### Acceptance Criteria

1. THE Firmware_System SHALL detect available firmware updates for connected devices
2. WHEN updating firmware, THE Firmware_System SHALL verify the firmware image signature
3. THE Firmware_System SHALL support rollback to previous firmware version on failure
4. WHEN a firmware update is in progress, THE Firmware_System SHALL prevent FFB operations
5. IF a firmware update fails, THEN THE Firmware_System SHALL attempt recovery and log the failure
6. THE Firmware_System SHALL maintain a local cache of firmware images for offline updates

### Requirement 18: Production Safety Interlocks

**User Story:** As a user of high-torque wheels, I want safety guarantees, so that the software cannot cause injury or equipment damage.

#### Acceptance Criteria

1. THE Safety_System SHALL implement hardware watchdog integration with 100ms timeout
2. WHEN the watchdog times out, THE Safety_System SHALL command zero torque immediately
3. THE Safety_System SHALL enforce maximum torque limits based on device capabilities
4. WHEN a fault is detected, THE Safety_System SHALL enter safe mode and log the fault
5. THE Safety_System SHALL support emergency stop via dedicated input or software command
6. IF communication with the device is lost, THEN THE Safety_System SHALL assume safe state within 50ms
7. THE Safety_System SHALL pass FMEA analysis for all identified failure modes

### Requirement 19: Cross-Platform Release Artifacts

**User Story:** As a user on any supported platform, I want official release packages, so that I can install OpenRacing easily.

#### Acceptance Criteria

1. THE Release_System SHALL produce Linux packages: .deb (Debian/Ubuntu), .rpm (Fedora/RHEL), and tarball
2. THE Release_System SHALL produce Windows packages: MSI installer and portable ZIP
3. THE Release_System SHALL produce macOS packages: .dmg installer (when macOS support is complete)
4. WHEN a release is published, THE Release_System SHALL upload artifacts to GitHub Releases
5. THE Release_Artifacts SHALL include SHA256 checksums for verification
6. THE Release_Artifacts SHALL be signed with the project's release key

### Requirement 20: Version Compatibility and Migration

**User Story:** As an existing user, I want smooth upgrades, so that my profiles and settings are preserved across versions.

#### Acceptance Criteria

1. THE Migration_System SHALL detect and migrate profiles from previous versions
2. WHEN a breaking schema change occurs, THE Migration_System SHALL provide automatic migration
3. THE Migration_System SHALL backup existing configuration before migration
4. IF migration fails, THEN THE Migration_System SHALL restore the backup and report the error
5. THE Schema_System SHALL maintain backward compatibility within major versions
6. THE Documentation SHALL include migration guides for each major version upgrade
