# Project Structure

## Workspace Layout
```
OpenRacing/
├── crates/           # Main source crates
├── docs/             # Documentation and ADRs
├── packaging/        # Platform installers (Linux, Windows)
├── scripts/          # Build, validation, and CI scripts
├── benches/          # Performance benchmarks
├── third_party/      # Vendored dependencies (shared_memory)
└── workspace-hack/   # Cargo hakari feature unification
```

## Crates

| Crate | Purpose |
|-------|---------|
| `schemas` | Protocol buffers, JSON schemas, domain types |
| `engine` | Core FFB engine, RT pipeline, safety systems |
| `service` | Background daemon, IPC, game integration |
| `cli` | Command-line interface (`wheelctl`) |
| `plugins` | WASM and native plugin system |
| `ui` | User interface components |
| `compat` | Legacy compatibility layer |
| `integration-tests` | Acceptance and soak tests |

## Key Directories

### `crates/engine/src/`
- `rt.rs` - Real-time loop, Frame, FFBMode
- `pipeline.rs` - Filter processing pipeline
- `safety/` - Watchdog, fault injection, FMEA
- `hid/` - Platform HID abstraction
- `diagnostic/` - Black box, replay, support bundles

### `crates/service/src/`
- `daemon.rs` - System service lifecycle
- `ipc_*.rs` - IPC transport and services
- `telemetry/` - Game telemetry adapters (iRacing, ACC)
- `update/` - Firmware update system
- `crypto/` - Ed25519 signing, trust store

### `crates/schemas/src/`
- `domain.rs` - Core domain types (DeviceId, TorqueNm, etc.)
- `entities.rs` - Device, Profile, Settings structs
- `generated/` - Protobuf generated code

## Architecture Decisions
All significant decisions documented in `docs/adr/`:
- ADR-0001: FFB Mode Matrix
- ADR-0002: IPC Transport Layer
- ADR-0003: OWP-1 Protocol
- ADR-0004: RT Scheduling Architecture
- ADR-0005: Plugin Architecture
- ADR-0006: Safety Interlocks

## Module Conventions
- Public API via `prelude` module for explicit imports
- No glob re-exports (`pub use *`)
- Test modules: `#[cfg(test)]` in same file or `tests/` directory
- Feature-gated test harness: `#[cfg(any(test, feature = "harness"))]`

## Scripts
- `scripts/validate_performance.py` - Performance gate validation
- `scripts/validate_adr.py` - ADR format validation
- `scripts/ci_nix.sh` - Linux CI script
- `scripts/ci_wsl.ps1` - Windows WSL CI wrapper
