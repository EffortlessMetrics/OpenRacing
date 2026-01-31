# OpenRacing

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Platform: Windows](https://img.shields.io/badge/platform-Windows%2010%2B-blue.svg)](https://www.microsoft.com/windows)
[![Platform: Linux](https://img.shields.io/badge/platform-Linux%20Kernel%204.0%2B-blue.svg)](https://www.kernel.org)
[![Platform: macOS](https://img.shields.io/badge/platform-macOS-lightgrey.svg)](https://www.apple.com/macos)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org)

OpenRacing is a high-performance, safety-critical racing wheel and force feedback simulation software built in Rust. Designed for sim-racing enthusiasts and professionals, it delivers real-time force feedback processing at 1kHz with deterministic latency and comprehensive safety interlocks.

## Features

- **Real-time Force Feedback at 1kHz** - Deterministic processing pipeline with sub-millisecond latency for authentic racing feel
- **Multi-Game Integration** - Native support for iRacing, Assetto Corsa Competizione (ACC), Automobilista 2 (AMS2), and rFactor 2
- **Safety-Critical Design** - Comprehensive fault injection testing, FMEA analysis, and hardware watchdog integration
- **Plugin Architecture** - Extensible plugin system supporting both WASM and native plugins for custom DSP, telemetry, and LED effects
- **Cross-Platform Support** - Runs on Windows 10+, Linux kernel 4.0+, and macOS with consistent behavior
- **Zero-Allocation Real-Time Path** - Memory-safe real-time processing without heap allocations
- **Comprehensive Diagnostics** - Black box recording, replay analysis, and support bundle generation
- **Profile Management** - JSON-based force feedback profiles with schema validation and backward compatibility

## Quick Start

### Prerequisites

- **Rust 1.89 or later** - Install from [rustup.rs](https://rustup.rs/)
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
