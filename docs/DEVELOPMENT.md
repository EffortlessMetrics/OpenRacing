# Development Guide

This document provides an overview of the development process, tooling, and standards for the Racing Wheel Software Suite.

## Architecture Decision Records (ADRs)

All significant architectural decisions are documented in ADRs located in `docs/adr/`. See [ADR README](adr/README.md) for the complete process.

### Current ADRs
- [ADR-0001: Force Feedback Mode Matrix](adr/0001-ffb-mode-matrix.md)
- [ADR-0002: IPC Transport Layer](adr/0002-ipc-transport.md) 
- [ADR-0003: OWP-1 Protocol Specification](adr/0003-owp1-protocol.md)
- [ADR-0004: Real-Time Scheduling Architecture](adr/0004-rt-scheduling-architecture.md)
- [ADR-0005: Plugin Architecture](adr/0005-plugin-architecture.md)
- [ADR-0006: Safety Interlocks and Fault Management](adr/0006-safety-interlocks.md)

## Continuous Integration

The CI pipeline enforces code quality, performance, and security standards:

### Test Matrix
- **Platforms**: Ubuntu, Windows, macOS
- **Rust Versions**: Stable, Beta (Ubuntu only)
- **Test Types**: Unit, integration, doc tests

### Performance Gates
- **P99 Jitter**: ≤ 0.25ms at 1kHz (NFR-01)
- **Missed Ticks**: ≤ 0.001% rate
- **Processing Time**: ≤ 50μs median, ≤ 200μs p99
- **Memory**: Zero heap allocations in RT path

### Security & Compliance
- **Vulnerability Scanning**: `cargo audit` with deny warnings
- **License Compliance**: `cargo deny` with approved license list
- **Dependency Tracking**: Third-party license report generation
- **ADR Validation**: Format and requirement reference checking

## Development Workflow

### 1. Code Standards
```bash
# Format code
cargo fmt --all

# Run lints
cargo clippy --all-targets --all-features -- -D warnings

# Run tests
cargo test --all-features --workspace
```

#### Memory Safety Rules
- **No static mut**: Use `std::sync::OnceLock` instead of `static mut` for thread-safe initialization
- **Lint Guard**: All non-test crates must include `#![deny(static_mut_refs)]` to prevent regression
- **Safe Alternatives**: Prefer `AtomicBool`, `OnceLock`, or `LazyLock` over unsafe static patterns

### 2. Performance Validation
```bash
# Build RT profile
cargo build --profile rt --bin wheeld

# Run benchmarks
cargo bench --bench rt_timing

# Validate performance gates
python scripts/validate_performance.py bench_results.json --strict
```

### 3. Documentation
```bash
# Validate ADRs
python scripts/validate_adr.py --verbose

# Generate documentation index
python scripts/generate_docs_index.py

# Build docs
cargo doc --all-features --workspace
```

## Real-Time Development Guidelines

### Critical Path Rules
1. **No Heap Allocations**: RT thread must not allocate after initialization
2. **No Blocking Operations**: No syscalls, locks, or I/O in RT path
3. **Bounded Execution**: All RT operations must have deterministic timing
4. **Error Handling**: Use `Result<(), RTError>` with pre-allocated error codes

### Performance Budgets
- **Total RT Budget**: 1000μs @ 1kHz
- **Input Processing**: 50μs
- **Filter Pipeline**: 200μs median, 800μs p99
- **Output Formatting**: 50μs
- **HID Write**: 100μs median, 300μs p99
- **Safety Checks**: 50μs

### Testing RT Code
```rust
#[test]
fn test_zero_alloc_rt_path() {
    // Use allocation tracking to ensure no heap usage
    let _guard = allocation_tracker::track();
    
    let mut engine = Engine::new();
    let mut frame = Frame::default();
    
    // This must not allocate
    engine.process_frame(&mut frame).unwrap();
    
    assert_eq!(allocation_tracker::allocations(), 0);
}
```

## Single Responsibility Principle (SRP) Micro-Crates

Use micro-crates when a component has one clear reason to change and can be reused
across runtime layers.

### When to extract

- A module mixes pure protocol/data logic with runtime concerns (HID I/O, env/config, logging).
- The same parsing/normalization/encoding logic is needed in multiple crates.
- Test coverage is easier to maintain when logic is isolated from device plumbing.

### Micro-crate rules

- Keep scope narrow: one domain concern per crate (for example, one hardware protocol parser).
- Prefer pure functions and small value types over stateful services.
- Keep hot-path APIs allocation-free and deterministic.
- Avoid blocking, syscalls, and runtime I/O inside parsing/encoding functions.
- Expose stable, minimal APIs and re-export from higher-level crates only where needed.

### Non-goals

- Do not split crates only for naming or directory symmetry.
- Do not extract code that is tightly coupled to a single runtime implementation.

### Verification checklist for extractions

- Unit tests move with the extracted logic crate.
- Integration paths in consuming crates continue to pass.
- RT safety expectations remain explicit (no allocations, no blocking, bounded execution).
- Public API and dependency changes are documented in `docs/` and ADRs when architectural impact is significant.

## Safety Requirements

All safety-critical code must follow these guidelines:

### Fault Response
- **Detection Time**: ≤ 10ms
- **Response Time**: ≤ 50ms total (fault to safe state)
- **Recovery**: Automatic where safe, manual confirmation for critical faults

### Testing Requirements
- **Fault Injection**: All defined failure modes must be tested
- **Timing Validation**: Oscilloscope measurement for critical timing
- **Soak Testing**: 48+ hour continuous operation validation

## Plugin Development

### Safe Plugins (WASM)
- **Update Rate**: 60-200Hz
- **Sandboxing**: Capability-based permissions
- **Memory Limit**: Configurable per plugin
- **Crash Isolation**: Automatic restart with backoff

### Fast Plugins (Native)
- **Update Rate**: 1kHz (RT path)
- **Timing Budget**: Microsecond-level enforcement
- **ABI Versioning**: Semantic compatibility checking
- **Code Signing**: Ed25519 signatures required

## Troubleshooting

### Performance Issues
1. Check RT thread priority and affinity
2. Validate timing with `cargo bench --bench rt_timing`
3. Use system tracing (ETW/tracepoints) for detailed analysis
4. Review blackbox recordings for timing anomalies

### Build Issues
1. Ensure system dependencies are installed (libudev-dev, pkg-config)
2. Check Rust toolchain version compatibility
3. Validate cargo deny configuration for new dependencies
4. Review CI logs for platform-specific issues

### Safety System Issues
1. Check device capability negotiation
2. Validate safety state machine transitions
3. Review fault detection thresholds
4. Test physical interlock mechanisms

## Contributing

1. **Create ADR**: For architectural changes, create an ADR first
2. **Follow Standards**: Use rustfmt, clippy, and pass all CI checks
3. **Test Thoroughly**: Include unit, integration, and performance tests
4. **Document Changes**: Update relevant documentation and ADRs
5. **Performance Impact**: Validate that changes don't regress performance gates

## Tools and Scripts

- `scripts/validate_performance.py`: Performance gate validation
- `scripts/validate_adr.py`: ADR format and reference validation
- `scripts/generate_docs_index.py`: Documentation index generation
- `scripts/sync_yaml.py`: Game support matrix YAML sync tool (see below)
- `benches/rt_timing.rs`: Real-time performance benchmarks
- `deny.toml`: Dependency and license configuration
- `clippy.toml`: Linting configuration
- `rustfmt.toml`: Code formatting configuration

### Keeping game support matrix files in sync

Two YAML files must always be identical:

- `crates/telemetry-config/src/game_support_matrix.yaml` (canonical — runtime)
- `crates/telemetry-support/src/game_support_matrix.yaml` (mirror — tests)

**Whenever you edit `crates/telemetry-config/src/game_support_matrix.yaml`, run:**

```bash
python scripts/sync_yaml.py --fix
```

This copies the canonical file to the mirror. To check without writing:

```bash
python scripts/sync_yaml.py --check   # exits 1 if files differ
```

The CI workflow (`.github/workflows/yaml-sync-check.yml`) enforces this on every push and PR.

## WSL + Nix CI Runner (Windows)

If you want to run the Linux CI-equivalent checks from Windows without moving the
repo into WSL, use the WSL wrapper script. It maps the Windows path into WSL and
executes the Nix dev shell before running the CI script.

### Prerequisites
- WSL2 with a Linux distro (e.g., Ubuntu)
- Nix installed in WSL (recommended: [Determinate Systems installer](https://install.determinate.systems/nix))
- Nix flakes enabled (for `flake.nix`)

### Usage

Run from PowerShell in the repo root:
```powershell
.\scripts\ci_wsl.ps1 -- --mode fast
.\scripts\ci_wsl.ps1 -- --mode full
```

The `--` delimiter separates PowerShell args from Linux script args.

### CI Modes

| Mode | Description |
|------|-------------|
| `fast` | Isolation builds, workspace default, lint gates, final validation |
| `full` | All phases including schema validation, feature combinations, dependency governance, performance gates, security audit, coverage |

### Common Flags

| Flag | Description |
|------|-------------|
| `--allow-dirty` | Skip clean-tree checks (useful during iteration) |
| `--skip-performance` | Skip performance gate steps |
| `--force-performance` | Run performance gates even on WSL |
| `--skip-coverage` | Skip coverage collection |
| `--skip-security` | Skip security audit and license checks |
| `--skip-minimal-versions` | Skip nightly minimal-versions check |
| `--allow-lock-update` | Allow Cargo.lock to change (otherwise fails if lockfile changes) |
| `--buf-against <ref>` | Run buf breaking checks against a git ref |

### Examples

Quick iteration (dirty tree, skip perf):
```powershell
.\scripts\ci_wsl.ps1 -- --mode fast --allow-dirty
```

Full CI with lockfile changes allowed:
```powershell
.\scripts\ci_wsl.ps1 -- --mode full --allow-lock-update
```

Select a specific WSL distro:
```powershell
$env:OPENRACING_WSL_DISTRO = "Ubuntu-22.04"
.\scripts\ci_wsl.ps1 -- --mode fast
```

Skip Nix (if already in a nix shell or debugging):
```powershell
.\scripts\ci_wsl.ps1 -NoNix -- --mode fast
```

### On Linux (or inside WSL)

Run the CI script directly:
```bash
nix develop --command bash scripts/ci_nix.sh --mode fast
# or without nix:
scripts/ci_nix.sh --mode fast
```

### Troubleshooting

- **Nix not found**: Install Nix in your WSL distro
- **Cargo.lock changed**: The CI run modified the lockfile. Re-run with `--allow-lock-update` if intentional, or revert and regenerate
- **Performance gate failures on WSL**: WSL timing is unreliable; use `--skip-performance` or `--force-performance` to run anyway
- **Path mapping failed**: Ensure the repo is accessible from both Windows and WSL
