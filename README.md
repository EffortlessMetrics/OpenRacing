# OpenRacing

[![CI](https://github.com/EffortlessMetrics/OpenRacing/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/EffortlessMetrics/OpenRacing/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/EffortlessMetrics/OpenRacing/branch/main/graph/badge.svg)](https://codecov.io/gh/EffortlessMetrics/OpenRacing)
[![Documentation](https://img.shields.io/badge/docs-API-blue.svg)](https://effortlessmetrics.github.io/OpenRacing/)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Platform: Windows](https://img.shields.io/badge/platform-Windows%2010%2B-blue.svg)](https://www.microsoft.com/windows)
[![Platform: Linux](https://img.shields.io/badge/platform-Linux%20Kernel%204.0%2B-blue.svg)](https://www.kernel.org)
[![Platform: macOS](https://img.shields.io/badge/platform-macOS-lightgrey.svg)](https://www.apple.com/macos)
[![Rust](https://img.shields.io/badge/rust-nightly-orange.svg)](https://www.rust-lang.org)

OpenRacing is a high-performance, safety-critical racing wheel and force feedback simulation software built in Rust. Designed for sim-racing enthusiasts and professionals, it delivers real-time force feedback processing at 1kHz with deterministic latency and comprehensive safety interlocks.

## Features

- **Real-time Force Feedback at 1kHz** - Deterministic processing pipeline with sub-millisecond latency for authentic racing feel
- **Multi-Game Integration** - Native support for 14 simulators: iRacing, ACC, AMS2, rFactor 2, Assetto Corsa, Forza Motorsport/Horizon, BeamNG.drive, Project CARS 2/3, RaceRoom Experience, AC Rally, Dirt 5, EA WRC, F1 2024, F1 25
- **Safety-Critical Design** - Comprehensive fault injection testing, FMEA analysis, hardware watchdog integration, and 600+ tests
- **Plugin Architecture** - Extensible plugin system supporting both WASM and native plugins for custom DSP, telemetry, and LED effects
- **Cross-Platform Support** - Runs on Windows 10+, Linux kernel 4.0+, and macOS with consistent behavior
- **Zero-Allocation Real-Time Path** - Memory-safe real-time processing without heap allocations
- **Comprehensive Diagnostics** - Black box recording, replay analysis, and support bundle generation
- **Profile Management** - JSON-based force feedback profiles with schema validation and backward compatibility

## Supported Hardware

| Vendor | VID | Models | FFB |
|--------|-----|--------|-----|
| **Logitech** | `0x046D` | G27, G29, G923, G Pro | ✅ HID PIDFF + TrueForce |
| **Fanatec** | `0x0EB7` | CSL DD, GT DD Pro, Podium DD1/DD2, CSW v2.5 | ✅ Custom HID |
| **Thrustmaster** | `0x044F` | T150/Pro, TMX, T300RS/GT, TX, T500RS, T248/X, T-GT/II, TS-PC, TS-XW, T818 | ✅ HID PIDFF |
| **Moza Racing** | `0x346E` | R3, R5 V1/V2, R9 V1/V2, R12 V1/V2, R16, R21 | ✅ Serial/HID PIDFF |
| **Simagic** | `0x3670` / `0x0483` | Alpha, Alpha Mini/EVO, M10, Neo/Mini, pedals | ✅ HID PIDFF |
| **Simucube 2** | `0x16D0` | Sport (17 Nm), Pro (25 Nm), Ultimate (32 Nm) | ✅ HID PIDFF |
| **VRS DirectForce Pro** | `0x0483` | DirectForce Pro V1/V2 (20/25 Nm) | ✅ HID PIDFF |
| **Heusinkveld** | `0x16D0` | Sprint, Ultimate+, Pro pedals | Input only |
| **Asetek SimSports** | `0x2433` | Forte (18 Nm), Invicta (27 Nm), La Prima (12 Nm) | ✅ HID PIDFF |
| **OpenFFBoard** | `0x1209` | All production firmware variants | ✅ HID PIDFF |
| **FFBeast** | `0x045B` | Joystick, rudder, wheel builds | ✅ HID PIDFF |
| **Granite Devices IONI/ARGON** | `0x1D50` | IONI / Simucube 1 (15 Nm), IONI Premium (35 Nm), ARGON (10 Nm) | ✅ SimpleMotion V2 |
| **SimXperience AccuForce** | `0x1FC9` | AccuForce Pro V1/V2 | ✅ HID PIDFF |
| **Cammus** | `0x3416` | C5, C12, CP5/LC100 pedals | ✅ HID PIDFF |
| **Leo Bodnar** | `0x1DD2` | Wheel Interface, FFB Joystick, BBI-32, SLI-Pro | ⚠️ Partial FFB |
| **Cube Controls** | `0x0483` | GT Pro, Formula CSX-3 (provisional) | Input only |
| **Generic HID button box** | `0x1209` | Arduino DIY, BangButtons, SimRacingInputs | Input only |

## Supported Games

| Game | Method | Port/Key |
|------|--------|----------|
| iRacing | Shared memory | `IRSDKMemMapFileName` |
| Assetto Corsa | OutGauge UDP | 9996 |
| AC Competizione (ACC) / AC Rally | Shared memory | — |
| Automobilista 2 / Project CARS 2/3 | Shared memory + UDP | 5606 |
| rFactor 2 | Shared memory | — |
| RaceRoom Experience | R3E shared memory | `$R3E` |
| Forza Motorsport / Horizon | Sled/CarDash UDP | 5300 |
| BeamNG.drive | LFS OutGauge UDP | 4444 |
| Dirt 5 | Codemasters UDP | — |
| EA WRC | Codemasters UDP | — |
| F1 2024 | Codemasters bridge | — |
| F1 25 | Native UDP (format 2025) | 20777 |

## Quick Start

### Prerequisites

- **Rust nightly** - Install from [rustup.rs](https://rustup.rs/) (see `rust-toolchain.toml`)
- **Cargo** - Included with Rust installation
- **Platform-specific requirements**:
  - **Windows**: Windows 10 or later, Visual C++ Redistributable
  - **Linux**: Kernel 4.0+, udev rules for device access
  - **macOS**: macOS 10.15 or later

### Installation

#### From Source

```bash
# Clone the repository
git clone https://github.com/EffortlessMetrics/OpenRacing.git
cd OpenRacing

# Build the project
cargo build --release

# Install the CLI tool
cargo install --path crates/cli
```

#### Platform-Specific Installation

**Linux:**
```bash
# Install udev rules for device access
sudo cp packaging/linux/99-racing-wheel-suite.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger
```

**Windows:**
Run the MSI installer from the [releases page](https://github.com/EffortlessMetrics/OpenRacing/releases).

### Basic Usage

```bash
# List connected devices
wheelctl device list

# Check system health
wheelctl health

# Apply a force feedback profile
wheelctl profile apply <device-id> path/to/profile.json

# View device status
wheelctl device status <device-id>

# Run diagnostics
wheelctl diag test
```

## Documentation

- **[API Documentation](https://effortlessmetrics.github.io/OpenRacing/)** - Generated rustdoc for all public interfaces
- [Development Guide](docs/DEVELOPMENT.md) - Setting up the development environment and contributing
- [System Integration](docs/SYSTEM_INTEGRATION.md) - Integrating OpenRacing with racing games and hardware
- [Architecture Decision Records](docs/adr/INDEX.md) - Design decisions and technical rationale
- [Power Management Guide](docs/POWER_MANAGEMENT_GUIDE.md) - Power management and device configuration
- [Anticheat Compatibility](docs/ANTICHEAT_COMPATIBILITY.md) - Compatibility notes for various anticheat systems

## Project Structure

OpenRacing is organized as a Cargo workspace with the following crates:

| Crate | Description |
|-------|-------------|
| [`cli`](crates/cli/) | Command-line interface for device management, diagnostics, and configuration |
| [`engine`](crates/engine/) | Core force feedback engine with real-time processing pipeline |
| [`plugins`](crates/plugins/) | Plugin system supporting WASM and native extensions |
| [`schemas`](crates/schemas/) | Protocol buffer schemas and JSON schema validation |
| [`service`](crates/service/) | Background service for system-level integration |
| [`ui`](crates/ui/) | User interface components and safety displays |
| [`compat`](crates/compat/) | Compatibility layer for legacy hardware and protocols |
| [`integration-tests`](crates/integration-tests/) | Comprehensive integration and acceptance test suite |

## Contributing

We welcome contributions! Please see [DEVELOPMENT.md](docs/DEVELOPMENT.md) for detailed guidelines on:

- Setting up your development environment
- Running tests and benchmarks
- Code style and formatting requirements
- Submitting pull requests

## License

This project is dual-licensed under either:

- **MIT License** - [LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>
- **Apache License, Version 2.0** - [LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>

You may choose either license for your use.

## Support

- **Issues**: Report bugs and request features via [GitHub Issues](https://github.com/EffortlessMetrics/OpenRacing/issues)
- **Discussions**: Join community discussions at [GitHub Discussions](https://github.com/EffortlessMetrics/OpenRacing/discussions)
- **Documentation**: Comprehensive documentation available in the [`docs/`](docs/) directory

## Acknowledgments

OpenRacing is built with the following open-source projects:
- [Tokio](https://tokio.rs/) - Asynchronous runtime
- [Serde](https://serde.rs/) - Serialization framework
- [Prost](https://github.com/tokio-rs/prost) - Protocol Buffers implementation
- [Tracing](https://tracing.rs/) - Instrumentation framework

---

**Repository**: [https://github.com/EffortlessMetrics/OpenRacing](https://github.com/EffortlessMetrics/OpenRacing)
