# OpenRacing Product Overview

OpenRacing is a safety-critical racing wheel and force feedback simulation software built in Rust.

## Purpose
Real-time force feedback processing at 1kHz with deterministic latency for sim-racing enthusiasts and professionals.

## Key Features
- Real-time FFB at 1kHz with sub-millisecond latency
- Multi-game integration: iRacing, ACC, AMS2, rFactor 2
- Safety-critical design with FMEA analysis and hardware watchdog
- Plugin architecture (WASM + native) for DSP, telemetry, LED effects
- Cross-platform: Windows 10+, Linux 4.0+, macOS
- Zero-allocation real-time path
- Black box recording and replay diagnostics
- JSON-based profile management with schema validation

## Target Users
- Sim-racing enthusiasts
- Professional racing simulator operators
- Hardware developers building racing peripherals

## CLI Tool
The `wheelctl` CLI provides device management:
- `wheelctl device list` - List connected devices
- `wheelctl device status <id>` - View device status
- `wheelctl profile apply <id> <path>` - Apply FFB profile
- `wheelctl health` - Check system health
- `wheelctl diag test` - Run diagnostics
