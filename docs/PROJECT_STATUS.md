# Project Status

**Last updated:** 2026-03-07

## Current state

OpenRacing is in **pre-validation** stage. The repository contains protocol research, schema work, crate scaffolding, telemetry adapters, and safety-oriented architecture — but has **not been end-to-end validated on real hardware or simulators**.

## What exists today

| Area | State |
|------|-------|
| Protocol research | 150+ devices mapped from kernel drivers, vendor docs, community sources |
| Telemetry adapters | 61 game adapters scaffolded with codec/snapshot tests |
| Engine / RT pipeline | Implemented with safety interlocks, tested in simulation |
| Plugin system | WASM + native plugin runtime implemented |
| CI / test suite | 26,000+ tests, property tests, fuzz targets, snapshot coverage |
| Packaging | Linux deb/rpm/tarball scripts exist; Windows MSI and macOS DMG not yet published |

## What does NOT exist yet

- Real-hardware device validation
- End-to-end simulator testing with live telemetry
- Published binary releases or installers
- Hardware-in-the-loop CI

## Status definitions used in this project

Device and game support docs use their own status keys:

- **[Device Support Matrix](DEVICE_SUPPORT.md)** — Verified / Community / Estimated (refers to VID/PID sourcing, not runtime validation)
- **[Game Support Matrix](GAME_SUPPORT.md)** — Verified / Tested / Experimental / Stub (refers to adapter code coverage, not live game validation)

Neither matrix currently implies real-hardware or real-game validation.

## Immediate priorities

1. Validate one hardware path end-to-end (device -> engine -> FFB output)
2. Validate one telemetry path end-to-end (game -> adapter -> engine)
3. Publish a validation matrix with evidence
4. Publish first binary release

## How to help

Open an issue with: device model, firmware version, OS, game/sim version, connection mode, and logs.
