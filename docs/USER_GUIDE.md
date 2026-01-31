# OpenRacing User Guide

Welcome to OpenRacing, a high-performance racing wheel and force feedback simulation software designed for sim-racing enthusiasts and professionals. This guide will help you get started with OpenRacing and make the most of its features.

## Table of Contents

1. [Introduction](#introduction)
2. [System Requirements](#system-requirements)
3. [Installation](#installation)
4. [Getting Started](#getting-started)
5. [CLI Reference](#cli-reference)
6. [Game Integration](#game-integration)
7. [Profiles](#profiles)
8. [Safety Features](#safety-features)
9. [Troubleshooting](#troubleshooting)
10. [Advanced Topics](#advanced-topics)
11. [FAQ](#faq)
12. [Glossary](#glossary)

---

## Introduction

OpenRacing is a safety-critical racing wheel and force feedback simulation software built in Rust. It delivers real-time force feedback processing at 1kHz with deterministic latency and comprehensive safety interlocks.

### Who is OpenRacing for?

- **Sim-racing enthusiasts** who want authentic force feedback
- **Competitive racers** requiring consistent, low-latency performance
- **Hardware developers** working with racing wheel hardware
- **Modders** creating custom FFB profiles and effects

### Key Features

- **Real-time Force Feedback at 1kHz** - Deterministic processing pipeline with sub-millisecond latency
- **Multi-Game Integration** - Native support for iRacing, ACC, AMS2, and rFactor 2
- **Safety-Critical Design** - Comprehensive fault detection and hardware watchdog integration
- **Cross-Platform Support** - Windows 10+, Linux kernel 4.0+, and macOS
- **Profile Management** - JSON-based force feedback profiles with schema validation
- **Comprehensive Diagnostics** - Black box recording and support bundle generation

---

## System Requirements

### Hardware Requirements

| Component | Minimum | Recommended |
|-----------|---------|-------------|
| CPU | Multi-core processor (x64) | Intel Core i5 / AMD Ryzen 5 or better |
| RAM | 4 GB | 8 GB or more |
| Storage | 500 MB available space | SSD for best performance |
| USB | USB 2.0 port | USB 3.0 port, direct motherboard connection |

### Supported Operating Systems

- **Windows**: Windows 10 or later (x64)
- **Linux**: Modern distribution with kernel 4.0+ (x64)
- **macOS**: macOS 10.15 (Catalina) or later

### Supported Racing Wheels

OpenRacing supports a wide range of racing wheels through HID (Human Interface Device) communication. Commonly supported devices include:

- Logitech G-series wheels (G25, G27, G29, G920, G923)
- Fanatec CSL and ClubSport series
- Thrustmaster T-series and TX/T300 series
- Simucube and direct drive wheels
- Most other HID-compliant racing wheels

> **Note**: For the most up-to-date list of supported devices, check the [GitHub repository](https://github.com/EffortlessMetrics/OpenRacing).

---

## Installation

### Windows Installation

#### Using MSI Installer (Recommended)

1. Download the latest MSI installer from the [releases page](https://github.com/EffortlessMetrics/OpenRacing/releases)
2. Double-click the installer to run it
3. Follow the installation wizard
4. Optionally enable power optimization during installation

```cmd
# Run installer with power optimization
RacingWheelSuite.msi OPTIMIZE_POWER=1
```

#### Manual Setup

1. Download the pre-built binaries from the releases page
2. Extract the archive to a directory of your choice
3. Add the `bin` directory to your PATH:
   ```cmd
   setx PATH "%PATH%;C:\path\to\OpenRacing\bin"
   ```
4. Install the service:
   ```cmd
   wheeld install
   ```

### Linux Installation

#### Package Manager (Ubuntu/Debian)

```bash
# Add the OpenRacing repository
sudo apt-add-repository ppa:openracing/stable
sudo apt update

# Install OpenRacing
sudo apt install openracing openracing-cli

# Install udev rules for device access
sudo cp /usr/share/openracing/99-racing-wheel-suite.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger
```

#### Manual Setup

```bash
# Clone the repository
git clone https://github.com/EffortlessMetrics/OpenRacing.git
cd OpenRacing

# Build the project
cargo build --release

# Install the CLI tool
cargo install --path crates/cli

# Install udev rules
sudo cp packaging/linux/99-racing-wheel-suite.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger

# Install the service (systemd)
sudo cp packaging/linux/wheeld.service /etc/systemd/user/
systemctl --user enable --now wheeld
```

#### Udev Rules

The udev rules file ensures proper permissions for racing wheel devices:

```bash
# /etc/udev/rules.d/99-racing-wheel-suite.rules
ACTION=="add", SUBSYSTEM=="usb", ATTRS{idVendor}=="046d", MODE="0666"
ACTION=="add", SUBSYSTEM=="hidraw", MODE="0666"
```

### macOS Installation

```bash
# Clone the repository
git clone https://github.com/EffortlessMetrics/OpenRacing.git
cd OpenRacing

# Build the project
cargo build --release

# Install the CLI tool
cargo install --path crates/cli

# Install launchd service
cp packaging/macos/com.openracing.wheeld.plist ~/Library/LaunchAgents/
launchctl load ~/Library/LaunchAgents/com.openracing.wheeld.plist
```

### Verification Steps

After installation, verify that OpenRacing is working correctly:

```bash
# Check CLI installation
wheelctl --version

# Check service status
wheelctl health

# List connected devices
wheelctl device list
```

Expected output:
```
OpenRacing CLI version 0.1.0

Service Health Status
  Service: Running
  Overall: Healthy
  Devices: 1
    ✓ Logitech G29 (046d:c29f)
```

---

## Getting Started

### First-Time Setup Wizard

When you first run OpenRacing, the setup wizard will guide you through initial configuration:

```bash
# Launch the setup wizard
wheelctl setup
```

The wizard will:
1. Detect your racing wheel hardware
2. Configure device permissions
3. Set up game integration
4. Create a default profile
5. Configure safety settings

### Device Detection and Calibration

#### Detect Connected Devices

```bash
# List all connected devices
wheelctl device list

# Show detailed device information
wheelctl device list --detailed
```

#### Calibrate Your Wheel

Calibration ensures accurate force feedback and proper device behavior:

```bash
# Full calibration (center, DOR, pedals)
wheelctl device calibrate <device-id> --type all

# Calibrate only center position
wheelctl device calibrate <device-id> --type center

# Calibrate degrees of rotation
wheelctl device calibrate <device-id> --type dor

# Calibrate pedals
wheelctl device calibrate <device-id> --type pedals

# Skip confirmation prompts
wheelctl device calibrate <device-id> --type all --yes
```

**Calibration Tips:**
- Ensure the wheel is centered before starting
- Remove hands from the wheel during DOR calibration
- Press each pedal fully and release during pedal calibration
- Keep the wheel in a stable position throughout

### Basic Configuration

#### Check Device Status

```bash
# Show current device status
wheelctl device status <device-id>

# Watch status in real-time
wheelctl device status <device-id> --watch
```

#### Create Your First Profile

```bash
# Create a default profile
wheelctl profile create my-profile.json

# Create a profile for a specific game
wheelctl profile create profiles/iracing/gt3.json --game iracing --car gt3

# Create from an existing profile
wheelctl profile create profiles/iracing/formula.json --from my-profile.json --game iracing --car formula
```

### Testing Force Feedback

After setting up your device and profile, test the force feedback:

```bash
# Run diagnostic tests
wheelctl diag test

# Run specific test
wheelctl diag test --device <device-id> --type motor

# Watch performance metrics
wheelctl diag metrics --watch
```

You should feel smooth, responsive force feedback when the tests pass.

---

## CLI Reference

The `wheelctl` command-line interface provides comprehensive control over OpenRacing. All commands support the `--json` flag for machine-readable output.

### Global Options

| Option | Description |
|--------|-------------|
| `--json` | Output in JSON format for machine parsing |
| `-v`, `-vv`, `-vvv` | Increase verbosity (info, debug, trace) |
| `--help` | Show help information |
| `--version` | Show version information |

### Device Command

Manage racing wheel hardware.

#### `device list`

List all connected devices.

```bash
wheelctl device list
wheelctl device list --detailed
```

**Output Example:**
```
Available Profiles:
  ● Logitech G29 (046d:c29f)
    Product ID: c29f
    Vendor ID: 046d
    Max Torque: 2.5 Nm
    DOR: 900°
```

#### `device status`

Show device status and telemetry.

```bash
wheelctl device status <device-id>
wheelctl device status <device-id> --watch
```

**Output Example:**
```
Device Status: Logitech G29
  State: Connected
  Angle: 0°
  Speed: 0.0 rad/s
  Temperature: 42°C
  Hands On: ✓
  Faults: None
```

#### `device calibrate`

Calibrate device (center, DOR, pedals).

```bash
wheelctl device calibrate <device-id> --type <center|dor|pedals|all>
wheelctl device calibrate <device-id> --type all --yes
```

**Calibration Types:**
- `center` - Center the wheel position
- `dor` - Calibrate degrees of rotation
- `pedals` - Calibrate pedal ranges
- `all` - Full calibration sequence

#### `device reset`

Reset device to safe state.

```bash
wheelctl device reset <device-id>
wheelctl device reset <device-id> --force
```

> **Warning**: Reset stops all force feedback and returns to default settings.

### Profile Command

Manage force feedback profiles.

#### `profile list`

List available profiles.

```bash
wheelctl profile list
wheelctl profile list --game iracing
wheelctl profile list --game iracing --car gt3
```

#### `profile show`

Show profile details.

```bash
wheelctl profile show <profile-path>
```

#### `profile apply`

Apply profile to device.

```bash
wheelctl profile apply <device-id> <profile-path>
wheelctl profile apply <device-id> <profile-path> --skip-validation
```

#### `profile create`

Create new profile.

```bash
wheelctl profile create <path>
wheelctl profile create <path> --from <base-profile> --game <game> --car <car>
```

#### `profile edit`

Edit profile interactively or with specific field/value.

```bash
# Interactive edit
wheelctl profile edit <profile-path>

# Direct field edit
wheelctl profile edit <profile-path> --field base.ffbGain --value 0.8
wheelctl profile edit <profile-path> --field base.dorDeg --value 900
wheelctl profile edit <profile-path> --field base.torqueCapNm --value 10.0
```

**Editable Fields:**
- `base.ffbGain` - Force feedback gain (0.0-1.0)
- `base.dorDeg` - Degrees of rotation (0-3600)
- `base.torqueCapNm` - Torque cap in Nm
- `scope.game` - Game scope
- `scope.car` - Car scope

#### `profile validate`

Validate profile.

```bash
wheelctl profile validate <path>
wheelctl profile validate <path> --detailed
```

#### `profile export`

Export profile.

```bash
wheelctl profile export <profile-path>
wheelctl profile export <profile-path> --output <output-path>
wheelctl profile export <profile-path> --signed
```

#### `profile import`

Import profile.

```bash
wheelctl profile import <path>
wheelctl profile import <path> --target <target-directory>
wheelctl profile import <path> --verify
```

### Game Command

Manage game integration.

#### `game list`

List supported games.

```bash
wheelctl game list
wheelctl game list --detailed
```

**Supported Games:**
| Game | ID | Status | Features |
|------|-----|--------|----------|
| iRacing | `iracing` | Full Support | FFB Scalar, RPM, Car ID |
| Assetto Corsa Competizione | `acc` | Full Support | FFB Scalar, RPM, Car ID, DRS |
| Automobilista 2 | `ams2` | Read-Only | FFB Scalar, RPM |
| rFactor 2 | `rf2` | Planned | FFB Scalar, RPM, Telemetry |

#### `game configure`

Configure game for telemetry.

```bash
wheelctl game configure <game-id>
wheelctl game configure <game-id> --path <install-path>
wheelctl game configure <game-id> --auto
```

**Example:**
```bash
# Auto-configure iRacing
wheelctl game configure iracing --auto

# Configure ACC with custom path
wheelctl game configure acc --path "C:\Games\ACC"
```

#### `game status`

Show game status.

```bash
wheelctl game status
wheelctl game status --telemetry
```

#### `game test`

Test telemetry connection.

```bash
wheelctl game test <game-id>
wheelctl game test <game-id> --duration 30
```

### Safety Command

Manage safety features and controls.

#### `safety enable`

Enable high torque mode.

```bash
wheelctl safety enable <device-id>
wheelctl safety enable <device-id> --force
```

> **Warning**: High torque mode requires physical confirmation (hold both clutch paddles for 3 seconds).

#### `safety stop`

Emergency stop all devices.

```bash
# Stop all devices
wheelctl safety stop

# Stop specific device
wheelctl safety stop <device-id>
```

#### `safety status`

Show safety status.

```bash
# Show all devices
wheelctl safety status

# Show specific device
wheelctl safety status <device-id>
```

**Output Example:**
```
Safety Status:
  ● Logitech G29 (046d:c29f)
    High Torque: Disabled
    Torque Limit: 2.5 Nm
    Hands On: ✓
    Temperature: ✓ (42°C)
    No Faults: ✓
    ✓ Ready for high torque
```

#### `safety limit`

Set torque limits.

```bash
wheelctl safety limit <device-id> <torque-nm>
wheelctl safety limit <device-id> 8.0 --global
```

### Diag Command

Diagnostic and monitoring commands.

#### `diag test`

Run system diagnostics.

```bash
wheelctl diag test
wheelctl diag test --device <device-id> --type <motor|encoder|usb|thermal|all>
```

**Test Types:**
- `motor` - Motor phase testing
- `encoder` - Encoder integrity testing
- `usb` - USB communication testing
- `thermal` - Thermal management testing
- `all` - Run all tests

#### `diag record`

Record blackbox data.

```bash
wheelctl diag record <device-id>
wheelctl diag record <device-id> --duration 60 --output my-blackbox.wbb
```

#### `diag replay`

Replay blackbox recording.

```bash
wheelctl diag replay <file>
wheelctl diag replay <file> --verbose
```

#### `diag support`

Generate support bundle.

```bash
wheelctl diag support
wheelctl diag support --blackbox --output support-bundle.zip
```

The support bundle includes:
- System information
- Device diagnostics
- Performance metrics
- Fault history
- Optional blackbox recordings

#### `diag metrics`

Show performance metrics.

```bash
wheelctl diag metrics
wheelctl diag metrics --device <device-id> --watch
```

### Health Command

Service health and status monitoring.

```bash
# Show health snapshot
wheelctl health

# Watch health events in real-time
wheelctl health --watch
```

### Shell Completion Setup

Enable tab completion for your shell:

#### Bash

```bash
# Generate completion script
wheelctl completion bash > ~/.local/share/bash-completion/completions/wheelctl

# Source it in your .bashrc
echo 'source ~/.local/share/bash-completion/completions/wheelctl' >> ~/.bashrc
```

#### Zsh

```bash
# Generate completion script
wheelctl completion zsh > ~/.zsh/completion/_wheelctl

# Add to .zshrc
echo 'fpath=(~/.zsh/completion $fpath)' >> ~/.zshrc
echo 'autoload -U compinit && compinit' >> ~/.zshrc
```

#### PowerShell

```powershell
# Generate and source completion script
wheelctl completion powershell | Out-File -Encoding UTF8 wheelctl.ps1
. ./wheelctl.ps1
```

#### Fish

```bash
# Generate completion script
wheelctl completion fish > ~/.config/fish/completions/wheelctl.fish
```

---

## Game Integration

OpenRacing integrates with popular racing simulators to provide enhanced force feedback and telemetry features.

### Supported Games

#### iRacing

**Status**: Full Support

**Configuration Method**: Shared Memory (`app.ini`)

**Features**:
- FFB Scalar
- RPM
- Car ID
- Track ID

**Setup**:
```bash
# Auto-configure iRacing
wheelctl game configure iracing --auto
```

**Manual Configuration**:
1. Open `Documents\iRacing\app.ini`
2. Find or add the `[Telemetry]` section
3. Set `enableTelemetry=1`
4. Set `telemetryPort=9999`
5. Restart iRacing

#### Assetto Corsa Competizione (ACC)

**Status**: Full Support

**Configuration Method**: UDP Broadcast (`broadcasting.json`)

**Features**:
- FFB Scalar
- RPM
- Car ID
- DRS status

**Setup**:
```bash
# Auto-configure ACC
wheelctl game configure acc --auto
```

**Manual Configuration**:
1. Open `Documents\Assetto Corsa Competizione\Setup\broadcasting.json`
2. Set `active` to `true`
3. Set `port` to `9996`
4. Set `connectionIp` to your local IP
5. Restart ACC

#### Automobilista 2 (AMS2)

**Status**: Read-Only

**Configuration Method**: Shared Memory

**Features**:
- FFB Scalar
- RPM

**Setup**:
```bash
# Auto-configure AMS2
wheelctl game configure ams2 --auto
```

No manual configuration required - AMS2 exposes shared memory automatically.

#### rFactor 2

**Status**: Planned

**Configuration Method**: Plugin API

**Features**:
- FFB Scalar
- RPM
- Full Telemetry

**Note**: Plugin installation will be required when support is available.

### Auto Profile Switching

OpenRacing can automatically switch profiles based on the game and car you're driving:

1. Create profiles with game and car scope:
   ```bash
   wheelctl profile create profiles/iracing/gt3.json --game iracing --car gt3
   wheelctl profile create profiles/iracing/formula.json --game iracing --car formula
   ```

2. The service will detect the game and car from telemetry
3. The matching profile will be automatically applied

### Troubleshooting Game Integration

#### Telemetry Not Received

1. **Check game configuration**: Ensure telemetry is enabled in game settings
2. **Verify firewall**: Allow UDP traffic on the telemetry port
3. **Check game is running**: Telemetry is only available during sessions
4. **Test connection**: Run `wheelctl game test <game-id>`

#### Profile Not Switching

1. **Verify profile scope**: Ensure profiles have correct game/car tags
2. **Check telemetry**: Verify car ID is being received
3. **Manual apply**: Use `wheelctl profile apply` to test profile manually

#### Anti-Cheat Concerns

OpenRacing is designed to be fully compatible with all major anti-cheat systems:

- No process injection
- No kernel drivers
- Uses only documented, legitimate APIs
- All binaries are digitally signed

For more details, see [ANTICHEAT_COMPATIBILITY.md](ANTICHEAT_COMPATIBILITY.md).

---

## Profiles

Profiles define force feedback settings for your racing wheel. They are stored as JSON files with schema validation.

### Profile Structure

A profile consists of the following sections:

```json
{
  "schema": "wheel.profile/1",
  "scope": {
    "game": "iracing",
    "car": "gt3",
    "track": null
  },
  "base": {
    "ffbGain": 0.75,
    "dorDeg": 900,
    "torqueCapNm": 8.0,
    "filters": {
      "reconstruction": 0,
      "friction": 0.0,
      "damper": 0.0,
      "inertia": 0.0,
      "bumpstop": {
        "enabled": true,
        "strength": 0.5
      },
      "handsOff": {
        "enabled": true,
        "sensitivity": 0.3
      },
      "torqueCap": 10.0,
      "notchFilters": [],
      "slewRate": 1.0,
      "curvePoints": [
        {"input": 0.0, "output": 0.0},
        {"input": 1.0, "output": 1.0}
      ]
    }
  },
  "leds": {
    "rpmBands": [6000, 8000, 9000],
    "pattern": "sequential",
    "brightness": 0.8,
    "colors": {
      "low": [0, 255, 0],
      "mid": [255, 255, 0],
      "high": [255, 0, 0]
    }
  },
  "haptics": {
    "enabled": true,
    "intensity": 0.5,
    "frequencyHz": 100,
    "effects": {
      "engineVibration": true,
      "gearShift": true,
      "lockup": true
    }
  },
  "signature": null
}
```

### Profile Settings

#### Base Settings

| Setting | Description | Range | Default |
|---------|-------------|-------|---------|
| `ffbGain` | Force feedback gain | 0.0 - 1.0 | 0.75 |
| `dorDeg` | Degrees of rotation | 0 - 3600 | 900 |
| `torqueCapNm` | Maximum torque output | 0.1 - device max | device max |

#### Filter Settings

| Setting | Description | Range | Default |
|---------|-------------|-------|---------|
| `reconstruction` | Reconstruction filter level | 0 - 8 | 0 |
| `friction` | Friction effect | 0.0 - 1.0 | 0.0 |
| `damper` | Damper effect | 0.0 - 1.0 | 0.0 |
| `inertia` | Inertia effect | 0.0 - 1.0 | 0.0 |
| `slewRate` | Torque slew rate limiting | 0.0 - 2.0 | 1.0 |

#### Bumpstop Settings

| Setting | Description | Range | Default |
|---------|-------------|-------|---------|
| `enabled` | Enable bumpstop effect | true/false | true |
| `strength` | Bumpstop strength | 0.0 - 1.0 | 0.5 |

#### Hands-Off Settings

| Setting | Description | Range | Default |
|---------|-------------|-------|---------|
| `enabled` | Enable hands-off detection | true/false | true |
| `sensitivity` | Detection sensitivity | 0.0 - 1.0 | 0.3 |

#### LED Settings

| Setting | Description | Range | Default |
|---------|-------------|-------|---------|
| `rpmBands` | RPM shift light thresholds | array of floats | [6000, 8000, 9000] |
| `pattern` | LED pattern | "sequential", "bar", "center-out" | "sequential" |
| `brightness` | LED brightness | 0.0 - 1.0 | 0.8 |

#### Haptics Settings

| Setting | Description | Range | Default |
|---------|-------------|-------|---------|
| `enabled` | Enable haptic effects | true/false | true |
| `intensity` | Haptic intensity | 0.0 - 1.0 | 0.5 |
| `frequencyHz` | Vibration frequency | 10 - 500 | 100 |

### Creating and Editing Profiles

#### Create a New Profile

```bash
# Create default profile
wheelctl profile create my-profile.json

# Create from template
wheelctl profile create profiles/iracing/gt3.json --from my-profile.json --game iracing --car gt3
```

#### Edit a Profile

```bash
# Interactive edit (opens in default editor)
wheelctl profile edit my-profile.json

# Direct field edit
wheelctl profile edit my-profile.json --field base.ffbGain --value 0.8
```

### Importing/Exporting Profiles

#### Export a Profile

```bash
# Export to file
wheelctl profile export my-profile.json --output shared-profile.json

# Export with signature
wheelctl profile export my-profile.json --signed
```

#### Import a Profile

```bash
# Import to default location
wheelctl profile import shared-profile.json

# Import to specific location
wheelctl profile import shared-profile.json --target profiles/community/

# Verify signature on import
wheelctl profile import shared-profile.json --verify
```

### Profile Validation

Profiles are validated against a JSON schema to ensure correctness:

```bash
# Validate profile
wheelctl profile validate my-profile.json

# Validate with detailed output
wheelctl profile validate my-profile.json --detailed
```

**Validation Checks:**
- Schema version compatibility
- Required fields present
- Value ranges within limits
- Curve points are monotonic
- RPM bands are sorted

---

## Safety Features

OpenRacing includes comprehensive safety features to protect you and your equipment.

### Safety Interlocks Explained

Safety interlocks prevent accidental high-torque operation:

1. **Physical Interlock**: Requires holding both clutch paddles for 3 seconds
2. **UI Consent**: Explicit user acknowledgment in the interface
3. **Session Persistence**: Safety state persists until power cycle
4. **Fault Detection**: Automatic torque reduction on fault detection

### Enabling High-Torque Mode

High-torque mode provides maximum force feedback output but requires explicit confirmation:

```bash
# Enable high torque (with confirmation)
wheelctl safety enable <device-id>

# Force enable (skips safety checks - use with caution)
wheelctl safety enable <device-id> --force
```

**Requirements for High-Torque Mode:**
- No active faults
- Device temperature below 80°C
- Hands detected on wheel
- Physical challenge completed (hold both clutch paddles for 3 seconds)

### Emergency Stop Procedures

In case of emergency, you can immediately stop all force feedback:

```bash
# Emergency stop all devices
wheelctl safety stop

# Emergency stop specific device
wheelctl safety stop <device-id>
```

**Physical Emergency Stop:**
- Most wheels have a physical button for emergency stop
- Press and hold to immediately disable force feedback

### Safety Limits Configuration

Set torque limits to protect yourself and your equipment:

```bash
# Set torque limit for current session
wheelctl safety limit <device-id> 8.0

# Set global torque limit (applies to all profiles)
wheelctl safety limit <device-id> 8.0 --global
```

**Recommended Limits:**
- **Beginners**: 3-5 Nm
- **Intermediate**: 5-8 Nm
- **Advanced**: 8-12 Nm
- **Professional**: Up to device maximum

### Fault Detection

OpenRacing continuously monitors for fault conditions:

| Fault | Detection | Response |
|-------|-----------|----------|
| USB Timeout | No device response within 3 frames | Torque ramp-down within 50ms |
| Encoder Error | NaN or out-of-range values | Immediate torque stop |
| Thermal Limit | Temperature > 80°C | Reduced torque mode |
| Overcurrent | Current exceeds limits | Emergency stop |
| Hands Off | No hands detected | Reduced torque |

---

## Troubleshooting

### Common Issues and Solutions

#### Device Not Detected

**Symptoms**: `wheelctl device list` shows no devices

**Solutions**:
1. Check USB connection - try a different port
2. Verify device is powered on
3. Check udev rules (Linux):
   ```bash
   sudo udevadm control --reload-rules
   sudo udevadm trigger
   ```
4. Try unplugging and reconnecting the device
5. Check device manager for driver issues

#### High Jitter/Latency

**Symptoms**: Inconsistent force feedback, stuttering

**Solutions**:
1. Check power management settings (see [Power Management Guide](POWER_MANAGEMENT_GUIDE.md))
2. Disable USB selective suspend
3. Use USB 2.0 ports instead of 3.0
4. Close unnecessary background applications
5. Verify CPU is not thermal throttling

#### Device Disconnects

**Symptoms**: Wheel randomly disconnects

**Solutions**:
1. Check USB cable quality - use a high-quality, short cable
2. Connect directly to motherboard (avoid hubs)
3. Disable USB power saving
4. Check for loose connections
5. Update USB drivers

#### Force Feedback Weak

**Symptoms**: FFB feels weak or muted

**Solutions**:
1. Increase `ffbGain` in profile
2. Check game FFB settings
3. Verify torque cap isn't limiting output
4. Check for high torque limit in safety settings
5. Recalibrate the device

#### Force Feedback Too Strong

**Symptoms**: Wheel is hard to turn, uncomfortable

**Solutions**:
1. Decrease `ffbGain` in profile
2. Lower torque cap in profile
3. Set lower torque limit via `wheelctl safety limit`
4. Increase damper effect for smoother feel
5. Check for conflicting FFB settings in game

### Diagnostic Tools

#### Run Diagnostics

```bash
# Run all diagnostic tests
wheelctl diag test

# Run specific test
wheelctl diag test --type motor
wheelctl diag test --type encoder
wheelctl diag test --type usb
wheelctl diag test --type thermal
```

#### Record Blackbox

```bash
# Record 2 minutes of data
wheelctl diag record <device-id> --duration 120

# Record with custom output
wheelctl diag record <device-id> --output issue-blackbox.wbb
```

#### Replay Blackbox

```bash
# Replay recording
wheelctl diag replay issue-blackbox.wbb

# Replay with verbose output
wheelctl diag replay issue-blackbox.wbb --verbose
```

### Support Bundle Generation

Generate a comprehensive support bundle for troubleshooting:

```bash
# Generate basic support bundle
wheelctl diag support

# Include blackbox recording
wheelctl diag support --blackbox

# Specify output location
wheelctl diag support --output my-support-bundle.zip
```

The support bundle includes:
- System information (OS, CPU, RAM)
- OpenRacing version and configuration
- Device information and status
- Diagnostic test results
- Performance metrics
- Fault history
- Optional blackbox recording

### Getting Help

If you're still experiencing issues:

1. **Check the documentation**: Review this guide and other docs in the `docs/` directory
2. **Search existing issues**: Check [GitHub Issues](https://github.com/EffortlessMetrics/OpenRacing/issues)
3. **Generate a support bundle**: Run `wheelctl diag support` and attach it to your issue
4. **Join the community**: Participate in [GitHub Discussions](https://github.com/EffortlessMetrics/OpenRacing/discussions)
5. **Contact support**: Reach out through official channels

---

## Advanced Topics

### Power Management

Optimizing power management settings is crucial for consistent performance. See [Power Management Guide](POWER_MANAGEMENT_GUIDE.md) for detailed guidance.

**Quick Tips:**

**Windows:**
```cmd
# Set high performance power plan
powercfg /setactive 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c

# Disable USB selective suspend
# Use Device Manager or registry settings
```

**Linux:**
```bash
# Set CPU governor to performance
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Disable USB autosuspend
echo -1 | sudo tee /sys/bus/usb/devices/*/power/autosuspend_delay_ms
```

### Anti-Cheat Compatibility

OpenRacing is designed to be fully compatible with all major anti-cheat systems:

- **No Process Injection**: External communication only
- **No Kernel Drivers**: User-space operation only
- **Documented Methods**: Official APIs and documented interfaces
- **Signed Binaries**: All executables digitally signed

For detailed information, see [ANTICHEAT_COMPATIBILITY.md](ANTICHEAT_COMPATIBILITY.md).

### Plugin Installation

OpenRacing supports plugins for extending functionality. Plugins can provide:

- Custom DSP (Digital Signal Processing) effects
- Additional telemetry adapters
- Custom LED patterns
- Haptic effects

For plugin development information, see [PLUGIN_DEVELOPMENT.md](PLUGIN_DEVELOPMENT.md).

**Installing a Plugin:**

```bash
# Copy plugin to plugins directory
cp my-plugin.wasm ~/.wheel/plugins/

# Enable plugin
wheelctl plugin enable my-plugin

# List installed plugins
wheelctl plugin list
```

### Performance Tuning

#### Monitor Performance

```bash
# Watch performance metrics in real-time
wheelctl diag metrics --watch

# Check service health
wheelctl health --watch
```

#### Optimize Settings

1. **Reduce filter complexity**: Lower `reconstruction` and `notchFilters` count
2. **Adjust slew rate**: Set `slewRate` to 1.0 for maximum responsiveness
3. **Disable unnecessary effects**: Turn off `haptics` if not needed
4. **Profile-specific tuning**: Create profiles optimized for different scenarios

#### Performance Targets

| Metric | Target | Acceptable |
|--------|--------|------------|
| Tick Rate | 1000 Hz | > 900 Hz |
| Jitter (p99) | ≤ 0.25ms | ≤ 0.5ms |
| Processing Time | ≤ 200μs | ≤ 500μs |
| HID Write Latency (p99) | ≤ 300μs | ≤ 500μs |
| Total Added Latency | ≤ 2ms | ≤ 5ms |

---

## FAQ

### General

**Q: Is OpenRacing free?**  
A: Yes, OpenRacing is open-source and free to use. It is dual-licensed under MIT and Apache-2.0.

**Q: What racing wheels are supported?**  
A: OpenRacing supports most HID-compliant racing wheels. Commonly tested devices include Logitech G-series, Fanatec CSL/ClubSport, Thrustmaster T-series, and direct drive wheels.

**Q: Can I use OpenRacing with multiple wheels?**  
A: Yes, OpenRacing supports multiple connected devices simultaneously.

**Q: Does OpenRacing work on macOS?**  
A: Yes, OpenRacing supports macOS 10.15 (Catalina) and later.

### Installation

**Q: Do I need to install drivers?**  
A: No, OpenRacing uses standard HID drivers provided by your operating system.

**Q: Can I install OpenRacing without admin rights?**  
A: On Windows and macOS, admin rights are not required. On Linux, you may need sudo for udev rules and service installation.

**Q: How do I uninstall OpenRacing?**  
A: Run the uninstaller (Windows) or remove the installed files and service (Linux/macOS).

### Configuration

**Q: Where are profiles stored?**  
A: Profiles are stored in:
- Windows: `%LOCALAPPDATA%\Wheel\profiles\`
- Linux/macOS: `~/.wheel/profiles/`

**Q: Can I share my profiles with others?**  
A: Yes, profiles are JSON files that can be exported and imported. Use `wheelctl profile export` to share.

**Q: How do I reset to default settings?**  
A: Use `wheelctl device reset <device-id>` to reset the device to safe state, or delete/recreate profiles.

### Performance

**Q: Why is my FFB jittery?**  
A: Jitter is usually caused by power management settings. See [Power Management Guide](POWER_MANAGEMENT_GUIDE.md) for optimization tips.

**Q: What is the ideal tick rate?**  
A: OpenRacing targets 1000 Hz (1ms intervals) for optimal force feedback quality.

**Q: Can I reduce latency further?**  
A: Ensure you're using USB 2.0 ports, disable power saving, and close background applications.

### Safety

**Q: Why does high torque mode require confirmation?**  
A: High torque mode can be dangerous. The physical interlock prevents accidental activation.

**Q: What happens if a fault is detected?**  
A: OpenRacing immediately ramps down torque within 50ms and logs the fault for diagnostics.

**Q: Can I disable safety interlocks?**  
A: Safety interlocks cannot be disabled. They are essential for safe operation.

### Games

**Q: Will OpenRacing get me banned?**  
A: No, OpenRacing uses only legitimate, documented methods and is fully compatible with all major anti-cheat systems.

**Q: Can I use OpenRacing with games not officially supported?**  
A: OpenRacing provides basic FFB support for any game. Full telemetry integration requires game-specific adapters.

**Q: How do I add support for a new game?**  
A: See [PLUGIN_DEVELOPMENT.md](PLUGIN_DEVELOPMENT.md) for information on creating telemetry adapters.

---

## Glossary

| Term | Definition |
|------|------------|
| **FFB** | Force Feedback - the tactile feedback from the racing wheel that simulates road surface, tire grip, and vehicle physics |
| **DOR** | Degrees of Rotation - the total angle the wheel can turn from lock to lock |
| **HID** | Human Interface Device - the standard protocol for input devices like racing wheels |
| **IPC** | Inter-Process Communication - how OpenRacing components communicate with each other |
| **Jitter** | Variability in timing - lower jitter means smoother, more consistent force feedback |
| **Nm** | Newton-meter - the unit of torque used to measure force feedback strength |
| **p99** | 99th percentile - a statistical measure indicating that 99% of values fall below this threshold |
| **Profile** | A configuration file containing force feedback settings for a specific game/car combination |
| **RT** | Real-Time - processing that guarantees deterministic timing with minimal latency |
| **Telemetry** | Data from the racing game including speed, RPM, gear, and vehicle physics |
| **Torque Cap** | The maximum torque output allowed by the device or profile |
| **UDP** | User Datagram Protocol - a network protocol used for game telemetry |
| **Udev** | Linux device manager that handles device permissions and hot-plug events |

---

## Additional Resources

- [Development Guide](DEVELOPMENT.md) - Contributing to OpenRacing
- [System Integration](SYSTEM_INTEGRATION.md) - Technical integration details
- [Power Management Guide](POWER_MANAGEMENT_GUIDE.md) - Optimizing system performance
- [Anti-Cheat Compatibility](ANTICHEAT_COMPATIBILITY.md) - Anti-cheat information
- [Plugin Development](PLUGIN_DEVELOPMENT.md) - Creating custom plugins
- [GitHub Repository](https://github.com/EffortlessMetrics/OpenRacing) - Source code and issues
- [GitHub Discussions](https://github.com/EffortlessMetrics/OpenRacing/discussions) - Community discussions

---

**Document Version**: 1.1
**Last Updated**: 2026-01-30
**OpenRacing Version**: 0.1.0
