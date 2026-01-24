# Contributing to OpenRacing

Thank you for your interest in contributing to OpenRacing! This guide provides comprehensive information for developers who want to contribute to the project.

## Table of Contents

- [Introduction](#introduction)
- [Getting Started](#getting-started)
- [Project Architecture](#project-architecture)
- [Development Workflow](#development-workflow)
- [Coding Standards](#coding-standards)
- [Testing](#testing)
- [Documentation](#documentation)
- [Performance Requirements](#performance-requirements)
- [CI/CD Pipeline](#cicd-pipeline)
- [Platform-Specific Development](#platform-specific-development)
- [Debugging](#debugging)
- [Release Process](#release-process)
- [Resources](#resources)

---

## Introduction

Welcome to the OpenRacing project! OpenRacing is a high-performance, safety-critical racing wheel and force feedback simulation software built in Rust. Our mission is to provide sim-racing enthusiasts and professionals with authentic force feedback experiences through real-time processing at 1kHz with deterministic latency.

### Project Philosophy

- **Safety First**: All code must prioritize safety-critical requirements with comprehensive fault detection and rapid response mechanisms.
- **Real-Time Guarantees**: The force feedback engine operates at 1kHz with strict performance budgets and zero heap allocations in the RT path.
- **Cross-Platform**: Consistent behavior across Windows, Linux, and macOS with platform-specific optimizations.
- **Extensibility**: Plugin architecture supporting both WASM (safe) and native (fast) extensions.
- **Open Development**: Transparent decision-making through Architecture Decision Records (ADRs) and comprehensive documentation.

### Code of Conduct

We are committed to providing a welcoming and inclusive environment for all contributors. Please be respectful, constructive, and collaborative in all interactions.

---

## Getting Started

### Prerequisites

#### Rust Toolchain

OpenRacing uses the latest Rust nightly toolchain. Install Rust via [rustup](https://rustup.rs/):

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install nightly toolchain
rustup install nightly
rustup default nightly

# Verify installation
rustc --version
cargo --version
```

**Required Components:**
- Rust 1.89+ (nightly)
- Cargo
- rustfmt
- clippy

#### Platform-Specific Requirements

**Windows:**
- Windows 10 or later
- Visual C++ Redistributable (latest)
- Windows SDK 10.0 or later
- PowerShell 5.1 or later

**Linux:**
- Kernel 4.0 or later
- Development tools: `build-essential`, `pkg-config`
- udev development: `libudev-dev`
- For RT scheduling: `rtkit` package

```bash
# Ubuntu/Debian
sudo apt-get update
sudo apt-get install build-essential pkg-config libudev-dev

# For RT capabilities (optional but recommended)
sudo apt-get install rtkit
```

**macOS:**
- macOS 10.15 (Catalina) or later
- Xcode Command Line Tools

```bash
xcode-select --install
```

#### Additional Tools

```bash
# Protocol buffer compiler (for schema changes)
# Install from: https://github.com/protocolbuffers/protobuf/releases

# Cargo tools
cargo install cargo-hakari
cargo install cargo-deny
cargo install cargo-nextest
```

### Setting Up the Development Environment

```bash
# Clone the repository
git clone https://github.com/EffortlessMetrics/OpenRacing.git
cd OpenRacing

# Verify toolchain
rustup show

# Install pre-commit hooks (if available)
# scripts/install-hooks.sh
```

### Building the Project

```bash
# Debug build (faster compilation)
cargo build --workspace

# Release build (optimized)
cargo build --release --workspace

# RT profile (for real-time components)
cargo build --profile rt --bin wheeld

# Build specific crate
cargo build --package racing-wheel-engine
```

### Running Tests

```bash
# Run all tests
cargo test --workspace

# Run with output
cargo test --workspace -- --nocapture

# Run specific test
cargo test test_name --package racing-wheel-engine

# Run ignored tests (includes soak tests)
cargo test --workspace -- --ignored

# Run with nextest (faster test runner)
cargo nextest run --workspace
```

---

## Project Architecture

### Workspace Structure Overview

OpenRacing is organized as a Cargo workspace with the following crates:

```
OpenRacing/
├── Cargo.toml                 # Workspace configuration
├── crates/
│   ├── cli/                   # Command-line interface
│   ├── engine/                # Core force feedback engine
│   ├── plugins/               # Plugin system (WASM + native)
│   ├── schemas/               # Protocol buffers and JSON schemas
│   ├── service/               # Background service daemon
│   ├── ui/                    # User interface components
│   ├── compat/                # Compatibility layer
│   ├── integration-tests/     # Integration test suite
│   └── workspace-hack/        # Dependency unification
├── docs/
│   ├── adr/                   # Architecture Decision Records
│   ├── CONTRIBUTING.md        # This file
│   ├── DEVELOPMENT.md         # Development guidelines
│   └── ...
└── benches/                   # Performance benchmarks
```

### Crate Responsibilities and Relationships

| Crate | Responsibility | Dependencies |
|-------|----------------|--------------|
| [`schemas`](../crates/schemas/) | Shared data structures, protobuf definitions, JSON schemas | None (foundational) |
| [`engine`](../crates/engine/) | Real-time force feedback processing, device communication | `schemas`, `tokio`, `hidapi` |
| [`service`](../crates/service/) | Background daemon, IPC, game integration, telemetry | `schemas`, `engine`, `tokio`, `tonic` |
| [`plugins`](../crates/plugins/) | Plugin loading, WASM runtime, native plugin ABI | `schemas`, `engine`, `wasmtime` |
| [`cli`](../crates/cli/) | Command-line tool for user interaction | `schemas`, `service` (via IPC) |
| [`ui`](../crates/ui/) | UI components, safety displays | `schemas`, `service` (via IPC) |
| [`compat`](../crates/compat/) | Legacy API compatibility, migration helpers | `schemas` |
| [`integration-tests`](../crates/integration-tests/) | End-to-end tests, performance gates, soak tests | All crates |
| [`workspace-hack`](../workspace-hack/) | Dependency version unification | None |

**Dependency Graph:**
```
schemas (foundation)
  ├── engine ──────┐
  ├── service ──────┤
  ├── plugins ──────┤
  ├── cli ──────────┼──→ integration-tests
  ├── ui ───────────┤
  └── compat ───────┘
```

### Key Architectural Patterns

#### 1. Real-Time Processing Pipeline

The force feedback engine uses a deterministic processing pipeline:

```
Input → Filter Pipeline → Safety Checks → Output
  ↓         ↓                ↓              ↓
50μs    200μs (median)     50μs         100μs
       800μs (p99)
```

#### 2. Actor-Based Service Architecture

The background service uses Tokio tasks for concurrent operations:

- **Device Service**: HID communication and device state
- **Game Integration Service**: Telemetry parsing and game detection
- **Profile Service**: Configuration management and auto-switching
- **Safety Service**: Fault detection and interlock management
- **IPC Service**: gRPC/Unix socket communication

#### 3. Plugin Isolation

Plugins run in isolated environments:

- **WASM Plugins**: Sandboxed execution with capability-based permissions
- **Native Plugins**: Isolated helper process with SPSC shared memory

#### 4. Schema-Driven Development

All cross-crate communication uses schema-defined contracts:

- **Protobuf**: IPC messages (generated from `.proto` files)
- **JSON Schema**: Configuration files with validation
- **ABI**: Native plugin interface (C FFI)

### Real-Time Scheduling Architecture

The RT thread uses platform-specific scheduling for 1kHz operation:

**Linux:**
```rust
// SCHED_FIFO priority via rtkit
// mlockall(MCL_CURRENT | MCL_FUTURE)
// clock_nanosleep with TIMER_ABSTIME
```

**Windows:**
```rust
// MMCSS "Games" category
// Disable power throttling
// WaitableTimer with high resolution
```

**macOS:**
```rust
// thread_policy_set with THREAD_PRECEDENCE_POLICY
// mach_absolute_time for timing
```

### Plugin System Architecture

Two-tier plugin architecture:

**Safe Plugins (WASM):**
- Update rate: 60-200Hz
- Sandboxed execution
- Capability-based permissions
- Automatic crash recovery

**Fast Plugins (Native):**
- Update rate: 1kHz (RT path)
- Isolated helper process
- Microsecond timing budgets
- Ed25519 code signing required

### Safety Interlocks Architecture

Multi-layered safety system:

1. **Physical Interlock**: Button combination on device
2. **Software Interlock**: Challenge-response protocol
3. **Fault Detection**: USB, encoder, thermal, overcurrent
4. **Response**: ≤50ms to safe state

See [ADR-0006](adr/0006-safety-interlocks.md) for details.

---

## Development Workflow

### Branching Strategy

We use a simplified Git flow:

```
main (protected)
  └── develop (integration branch)
       ├── feature/feature-name
       ├── bugfix/bug-description
       └── hotfix/critical-fix
```

**Branch Types:**
- `main`: Production releases, protected
- `develop`: Integration branch for next release
- `feature/*`: New features and enhancements
- `bugfix/*`: Bug fixes for current release
- `hotfix/*`: Critical fixes requiring immediate release

### Commit Message Conventions

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

**Types:**
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style changes (formatting)
- `refactor`: Code refactoring
- `perf`: Performance improvements
- `test`: Test additions/changes
- `chore`: Build/process changes
- `revert`: Revert previous commit

**Examples:**
```
feat(engine): add adaptive damping filter

fix(service): resolve USB stall on device disconnect

perf(rt): reduce jitter by optimizing filter pipeline

docs(contributing): add performance testing section

test(integration): add hot-plug stress test
```

### Pull Request Process

1. **Create Branch**: From `develop`
   ```bash
   git checkout develop
   git pull origin develop
   git checkout -b feature/my-feature
   ```

2. **Make Changes**: Follow coding standards
   ```bash
   # Format code
   cargo fmt --all

   # Run lints
   cargo clippy --all-targets --all-features -- -D warnings

   # Run tests
   cargo test --workspace
   ```

3. **Create PR**: With descriptive title and template
   - Link related issues
   - Describe changes
   - Add screenshots if applicable
   - List breaking changes

4. **Code Review**: Address feedback
   - Required approvals: 1 maintainer
   - All CI checks must pass
   - Performance gates must not regress

5. **Merge**: Squash and merge to `develop`
   - Maintainer merges after approval
   - Automatic changelog update

### Code Review Guidelines

**For Reviewers:**
- Verify code follows project standards
- Check for security vulnerabilities
- Ensure tests are adequate
- Validate performance impact
- Review documentation updates

**For Authors:**
- Respond to feedback promptly
- Explain complex decisions
- Update documentation
- Add tests for edge cases
- Consider alternative approaches

---

## Coding Standards

### Rust Edition and Version Requirements

- **Edition**: Rust 2024
- **Toolchain**: Nightly (specified in [`rust-toolchain.toml`](../rust-toolchain.toml))
- **Minimum Supported Rust Version (MSRV)**: Nightly (no stable MSRV)

### Linting Rules

#### Clippy

All code must pass clippy with warnings as errors:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

**Key Clippy Rules** (see [`clippy.toml`](../clippy.toml)):
- No `static mut` (use `OnceLock` or `LazyLock`)
- No `unwrap()` in production code
- No `expect()` with unclear messages
- Complex types allowed up to threshold 250

#### rustfmt

All code must be formatted with rustfmt:

```bash
cargo fmt --all -- --check
```

**Key Formatting Rules** (see [`rustfmt.toml`](../rustfmt.toml)):
- Max line width: 100 characters
- 4 spaces per tab
- Trailing commas on vertical lists
- Same-line braces for functions and items

#### cargo-deny

All dependencies must pass security and license checks:

```bash
cargo deny check
```

**Key Rules** (see [`deny.toml`](../deny.toml)):
- Allowed licenses: MIT, Apache-2.0, BSD-2/3-Clause, ISC
- Denied licenses: GPL, AGPL, LGPL
- Security vulnerabilities: deny
- Unmaintained crates: warn

### Error Handling Patterns

Use `Result<T, E>` for fallible operations:

```rust
// Good: Explicit error handling
pub fn process_telemetry(input: &str) -> Result<TelemetryData, ParseError> {
    let data: TelemetryData = serde_json::from_str(input)
        .map_err(|e| ParseError::InvalidJson(e.to_string()))?;
    Ok(data)
}

// Bad: Using unwrap()
pub fn process_telemetry(input: &str) -> TelemetryData {
    serde_json::from_str(input).unwrap()
}
```

Define custom error types with `thiserror`:

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EngineError {
    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("USB communication failed: {0}")]
    UsbError(#[from] hidapi::HidError),

    #[error("Invalid torque value: {0}")]
    InvalidTorque(f32),
}
```

### Memory Safety Requirements

**No `static mut`:** Use thread-safe alternatives:

```rust
// Bad: static mut
static mut COUNTER: u32 = 0;

// Good: OnceLock
use std::sync::OnceLock;
static COUNTER: OnceLock<u32> = OnceLock::new;

// Good: AtomicBool
use std::sync::atomic::{AtomicBool, Ordering};
static ENABLED: AtomicBool = AtomicBool::new(false);
```

**All non-test crates must include:**
```rust
#![deny(static_mut_refs)]
```

### Zero-Allocation RT Path Requirements

The real-time path must not allocate after initialization:

```rust
// Good: Pre-allocated buffers
pub struct RtEngine {
    input_buffer: Box<[f32; 1024]>,
    output_buffer: Box<[f32; 1024]>,
}

impl RtEngine {
    pub fn process(&mut self) -> Result<(), RtError> {
        // No allocations here
        for i in 0..1024 {
            self.output_buffer[i] = self.filter(self.input_buffer[i]);
        }
        Ok(())
    }
}

// Bad: Allocating in RT path
pub fn process(&mut self) -> Result<(), RtError> {
    let temp = vec![0.0; 1024]; // Allocates!
    // ...
}
```

**Testing for zero allocations:**
```rust
#[test]
fn test_zero_alloc_rt_path() {
    let _guard = allocation_tracker::track();
    let mut engine = Engine::new();
    engine.process_frame(&mut frame).unwrap();
    assert_eq!(allocation_tracker::allocations(), 0);
}
```

### Logging with tracing

Use `tracing` for structured logging:

```rust
use tracing::{info, warn, error, debug, instrument};

#[instrument(skip(self))]
pub fn process_telemetry(&self, input: &TelemetryData) -> Result<(), EngineError> {
    debug!(wheel_angle = input.wheel_angle_deg, "Processing telemetry");
    
    if input.temperature_c > 80 {
        warn!(temp = input.temperature_c, "High temperature detected");
    }
    
    // ...
    
    info!("Telemetry processed successfully");
    Ok(())
}
```

**Log Levels:**
- `error`: Errors requiring attention
- `warn`: Warnings that don't prevent operation
- `info`: Important state changes
- `debug`: Detailed diagnostic information
- `trace`: Very detailed tracing (disabled in production)

---

## Testing

### Test Types and When to Use Each

| Test Type | Purpose | Location | Example |
|-----------|---------|----------|---------|
| **Unit Tests** | Test individual functions/modules | `src/` alongside code | `engine/src/filter.rs` |
| **Integration Tests** | Test crate interactions | `tests/` or `integration-tests/` | `service/tests/` |
| **Doc Tests** | Test code examples in docs | `///` comments | Public API docs |
| **Property Tests** | Test invariants with random inputs | `tests/` with `proptest` | Filter behavior |
| **Performance Tests** | Benchmark critical paths | `benches/` | RT timing |
| **Acceptance Tests** | Validate requirements | `integration-tests/` | User journeys |
| **Soak Tests** | Long-duration reliability | `integration-tests/` | 48-hour test |

### Unit Testing Guidelines

**Test Structure:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_basic() {
        let filter = LowPassFilter::new(0.5);
        assert_eq!(filter.process(1.0), 0.5);
    }

    #[test]
    fn test_filter_edge_cases() {
        // Test boundary conditions
    }

    #[test]
    #[should_panic(expected = "Invalid frequency")]
    fn test_filter_invalid_input() {
        // Test panic conditions
    }
}
```

**Guidelines:**
- One assertion per test when possible
- Use descriptive test names
- Test both happy path and error cases
- Mock external dependencies
- Keep tests fast (<1s per test)

### Integration Testing Guidelines

**Test Organization:**
```rust
// integration-tests/src/game_integration.rs
#[tokio::test]
async fn test_iracing_connection() {
    let mut service = GameIntegrationService::new();
    service.connect("127.0.0.1:16000").await.unwrap();
    
    // Simulate game telemetry
    let telemetry = create_mock_telemetry();
    service.process_telemetry(telemetry).await.unwrap();
    
    // Verify results
    assert!(service.is_connected());
}
```

**Guidelines:**
- Use virtual devices when possible
- Test cross-crate interactions
- Validate IPC contracts
- Include setup/teardown
- Use fixtures for common scenarios

### Property-Based Testing with proptest

Use `proptest` for testing invariants:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_filter_stability(freq in 0.0..1.0, input in -1000.0..1000.0) {
        let filter = LowPassFilter::new(freq);
        let output = filter.process(input);
        
        // Property: Output should be bounded by input
        assert!(output.abs() <= input.abs() + 0.001);
    }
}
```

**When to use:**
- Testing mathematical properties
- Validating invariants across input space
- Finding edge cases
- Testing serialization/deserialization

### Performance Testing with Criterion

Use `criterion` for benchmarks:

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_filter(c: &mut Criterion) {
    let filter = LowPassFilter::new(0.5);
    
    c.bench_function("filter_process", |b| {
        b.iter(|| {
            filter.process(black_box(1.0))
        });
    });
}

criterion_group!(benches, bench_filter);
criterion_main!(benches);
```

**Run benchmarks:**
```bash
cargo bench --bench rt_timing
```

### Hardware-in-Loop Testing

For physical device testing:

```rust
#[cfg(feature = "hil")]
#[tokio::test]
async fn test_physical_device() {
    // Only runs with HIL feature
    let device = Device::connect("vid:pid").await?;
    
    // Test with actual hardware
    device.set_torque(5.0).await?;
    let telemetry = device.read_telemetry().await?;
    
    assert_eq!(telemetry.torque_nm, 5.0);
}
```

### Test Coverage Requirements

**Minimum Coverage Targets:**
- Core engine: 90%+
- Service layer: 80%+
- CLI/UI: 70%+
- Overall: 75%+

**Check coverage:**
```bash
cargo install cargo-tarpaulin
cargo tarpaulin --workspace --out Html
```

---

## Documentation

### Code Documentation Requirements

All public APIs must have documentation:

```rust
/// Applies a low-pass filter to the input signal.
///
/// This filter implements a first-order exponential moving average
/// with the specified frequency coefficient.
///
/// # Arguments
///
/// * `input` - The input signal value to filter
///
/// # Returns
///
/// The filtered output value
///
/// # Errors
///
/// Returns an error if the filter frequency is out of range [0, 1]
///
/// # Examples
///
/// ```rust
/// let filter = LowPassFilter::new(0.5);
/// let output = filter.process(1.0);
/// assert_eq!(output, 0.5);
/// ```
pub fn process(&self, input: f32) -> f32 {
    // Implementation
}
```

**Requirements:**
- All `pub` items must have `///` documentation
- Include `# Examples` for non-trivial APIs
- Document panics with `# Panics`
- Document errors with `# Errors`
- Use `#[doc(hidden)]` for internal APIs

### ADR Process

Architecture Decision Records (ADRs) document significant architectural decisions.

**Creating an ADR:**

1. Copy the template from [`docs/adr/template.md`](adr/template.md)
2. Fill in all required sections
3. Use the next sequential number (e.g., `0007`)
4. Submit as a PR with `adr` label
5. Get approval from architecture team

**ADR Structure:**
```markdown
# ADR-XXXX: Title

**Status:** Proposed/Accepted/Superseded
**Date:** YYYY-MM-DD
**Authors:** ...
**Reviewers:** ...
**Related ADRs:** ...

## Context
...

## Decision
...

## Rationale
...

## Consequences
...

## Alternatives Considered
...

## Implementation Notes
...

## References
...
```

See [`docs/adr/INDEX.md`](adr/INDEX.md) for existing ADRs.

### Schema Governance

See [`SCHEMA_GOVERNANCE.md`](SCHEMA_GOVERNANCE.md) for complete policies.

**Key Points:**
- Public APIs have 2-version stability guarantee
- Breaking changes require deprecation window
- Compat debt must trend downward
- Schema changes require owner approval

### Migration Patterns

See [`MIGRATION_PATTERNS.md`](MIGRATION_PATTERNS.md) for migration examples.

**Common Patterns:**
- Field rename: Add new → Alias old → Remove
- Function change: Add new → Deprecate old → Remove
- Module restructure: Move → Re-export → Cleanup

---

## Performance Requirements

### Real-Time Constraints

**Force Feedback Engine (1kHz):**
- **Tick Rate**: 1000 Hz (1ms period)
- **Jitter P99**: ≤ 0.25ms
- **Missed Ticks**: ≤ 0.001% rate
- **Processing Time**: ≤ 50μs median, ≤ 200μs p99

**Timing Budget (1000μs total):**
| Component | Budget |
|-----------|--------|
| Input Processing | 50μs |
| Filter Pipeline | 200μs median, 800μs p99 |
| Output Formatting | 50μs |
| HID Write | 100μs median, 300μs p99 |
| Safety Checks | 50μs |
| Scheduler Overhead | ~50μs |

### Performance Budgets

**Memory:**
- RT path: Zero heap allocations
- Service: ≤ 150MB RSS
- CLI: ≤ 50MB RSS

**CPU:**
- RT thread: ≤ 3% of one core
- Service: ≤ 5% of one core
- CLI: Negligible

### Jitter Requirements

**Jitter Categories:**
- **P50**: Typical case, should be < 0.1ms
- **P99**: Must meet ≤ 0.25ms gate
- **P99.9**: Should not exceed 0.5ms

**Jitter Measurement:**
```bash
cargo bench --bench rt_timing
python scripts/validate_performance.py bench_results.json --strict
```

### Zero-Allocation Requirements

**RT Path Rules:**
1. No `Vec`, `HashMap`, `String` allocations
2. Use fixed-size arrays: `[T; N]`
3. Use pre-allocated buffers: `Box<[T; N]>`
4. Use stack-allocated types: `Option<T>`, `Result<T, E>`

**Allowed in RT Path:**
- Primitive types: `i32`, `f32`, `bool`, etc.
- Fixed-size arrays: `[f32; 1024]`
- Stack-allocated enums
- `Option<T>` and `Result<T, E>` (no heap)

**Forbidden in RT Path:**
- `Vec<T>`, `Box<T>`, `Rc<T>`, `Arc<T>`
- `String`, `HashMap<K, V>`, `BTreeMap<K, V>`
- Any allocation via `Box::new()`, `String::from()`, etc.

---

## CI/CD Pipeline

### Pipeline Overview

The CI pipeline runs on every push and pull request:

```
┌─────────────┐
│   Trigger   │ (push/PR)
└──────┬──────┘
       │
       ▼
┌─────────────┐
│   Build     │ (all platforms)
└──────┬──────┘
       │
       ▼
┌─────────────┐
│    Test     │ (unit + integration)
└──────┬──────┘
       │
       ▼
┌─────────────┐
│    Lint     │ (clippy + rustfmt + deny)
└──────┬──────┘
       │
       ▼
┌─────────────┐
│ Performance │ (benchmarks + gates)
└──────┬──────┘
       │
       ▼
┌─────────────┐
│   Security  │ (audit + license check)
└──────┬──────┘
       │
       ▼
┌─────────────┐
│   Deploy    │ (on main only)
└─────────────┘
```

### Build Phases

**Phase 1: Build**
```yaml
- name: Build (Release)
  run: cargo build --release --workspace

- name: Build (RT Profile)
  run: cargo build --profile rt --bin wheeld
```

**Phase 2: Test**
```yaml
- name: Unit Tests
  run: cargo test --workspace

- name: Integration Tests
  run: cargo test --package racing-wheel-integration-tests

- name: Doc Tests
  run: cargo test --doc --workspace
```

**Phase 3: Lint**
```yaml
- name: Clippy
  run: cargo clippy --all-targets --all-features -- -D warnings

- name: Rustfmt
  run: cargo fmt --all -- --check

- name: Cargo Deny
  run: cargo deny check
```

### Performance Gates

**Critical Performance Tests:**
```yaml
- name: Performance Gate
  run: |
    cargo bench --bench rt_timing
    python scripts/validate_performance.py target/criterion --strict
```

**Gate Thresholds:**
| Metric | Threshold | Fail Action |
|--------|-----------|-------------|
| Jitter P99 | ≤ 0.25ms | Block PR |
| Missed Ticks | 0 | Block PR |
| HID Latency P99 | ≤ 300μs | Block PR |
| Memory Leak | None | Block PR |

### Security Audits

**Vulnerability Scanning:**
```yaml
- name: Security Audit
  run: cargo audit
```

**License Compliance:**
```yaml
- name: License Check
  run: cargo deny check licenses
```

### Cross-Platform Testing

**Test Matrix:**
| Platform | Rust Versions | Test Types |
|----------|---------------|------------|
| Ubuntu Latest | Stable, Beta | All tests |
| Windows Latest | Stable | All tests |
| macOS Latest | Stable | All tests |

---

## Platform-Specific Development

### Windows Development Considerations

**HID Access:**
- Requires administrator privileges for some devices
- Use `hidapi` crate for cross-platform HID
- Handle Windows-specific error codes

**RT Scheduling:**
```rust
#[cfg(windows)]
use windows::Win32::System::Threading::*;

// Set thread priority for RT
unsafe {
    let handle = GetCurrentThread();
    SetThreadPriority(handle, THREAD_PRIORITY_TIME_CRITICAL);
}
```

**Power Management:**
- Disable power throttling for RT thread
- Use `SetThreadExecutionState` to prevent sleep

**Debugging:**
- Use Visual Studio or WinDbg for native debugging
- ETW (Event Tracing for Windows) for performance analysis

### Linux Development Considerations

**Device Access:**
- Install udev rules: `packaging/linux/99-racing-wheel-suite.rules`
- User must be in `plugdev` or `input` group
- For RT: Use `rtkit` for priority elevation

**RT Scheduling:**
```rust
#[cfg(unix)]
use libc::{SCHED_FIFO, sched_param};

// Set RT priority
unsafe {
    let mut param: sched_param = std::mem::zeroed();
    param.sched_priority = 50; // RT priority
    libc::sched_setschedule(0, SCHED_FIFO, &param);
}
```

**Memory Locking:**
```rust
// Lock memory to prevent paging
unsafe {
    libc::mlockall(libc::MCL_CURRENT | libc::MCL_FUTURE);
}
```

**Debugging:**
- Use `gdb` or `lldb` for native debugging
- `perf` for performance profiling
- `strace` for system call tracing

### macOS Development Considerations

**HID Access:**
- Requires accessibility permissions
- Use IOKit framework via `nix` crate

**RT Scheduling:**
```rust
#[cfg(target_os = "macos")]
use libc::{thread_policy_set, THREAD_PRECEDENCE_POLICY};

// Set thread priority
unsafe {
    let policy = THREAD_PRECEDENCE_POLICY { /* ... */ };
    thread_policy_set(0, THREAD_PRECEDENCE_POLICY, &policy, 1);
}
```

**Code Signing:**
- Required for native plugins
- Use `codesign` tool

**Debugging:**
- Use `lldb` for native debugging
- Instruments for performance analysis
- `dtrace` for system tracing

---

## Debugging

### Debugging Tools and Techniques

**Logging:**
```bash
# Enable debug logging
RUST_LOG=racing_wheel=debug cargo run

# Enable trace logging
RUST_LOG=racing_wheel=trace cargo run

# Filter by module
RUST_LOG=racing_wheel::engine=debug cargo run
```

**Tracing:**
```rust
use tracing::{info, instrument};

#[instrument(skip(self))]
pub fn process(&self, input: &Input) -> Result<Output> {
    info!("Processing input");
    // ...
}
```

**Performance Profiling:**
```bash
# Criterion benchmarks
cargo bench --bench rt_timing

# Flamegraph (Linux)
cargo install flamegraph
cargo flamegraph --bin wheeld

# perf (Linux)
perf record -F 99 -g ./target/release/wheeld
perf report
```

### Common Issues and Solutions

**Issue: High jitter**
```bash
# Check CPU governor
cat /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
# Should be 'performance'

# Check RT priority
chrt -p <pid>

# Disable power saving
sudo cpupower frequency-set -g performance
```

**Issue: USB stalls**
```bash
# Check USB devices
lsusb -v

# Monitor USB traffic
sudo usbmon

# Check for conflicts
lsof | grep hid
```

**Issue: Memory leaks**
```bash
# Use valgrind (Linux)
valgrind --leak-check=full ./target/release/wheeld

# Use heaptrack (Linux)
heaptrack ./target/release/wheeld
```

### Performance Profiling

**Linux:**
```bash
# perf profiling
perf record -e cycles,instructions,cache-misses -F 99 -g ./target/release/wheeld
perf report

# Flamegraph
cargo flamegraph --bin wheeld
```

**Windows:**
```bash
# Windows Performance Analyzer (WPA)
# Collect ETW trace
wpr -start GeneralProfile
# Run application
wpr -stop trace.etl

# Analyze with WPA
```

**macOS:**
```bash
# Instruments
# Open Xcode → Open Developer Tool → Instruments
# Select "Time Profiler"
```

---

## Release Process

### Versioning Scheme

OpenRacing follows [Semantic Versioning](https://semver.org/):

```
MAJOR.MINOR.PATCH

MAJOR: Incompatible API changes
MINOR: Backwards-compatible functionality
PATCH: Backwards-compatible bug fixes
```

**Examples:**
- `0.1.0` → `0.2.0`: New features, compatible changes
- `0.2.0` → `1.0.0`: First stable release
- `1.0.0` → `2.0.0`: Breaking changes

### Release Checklist

**Pre-Release:**
- [ ] All tests passing
- [ ] Performance gates passing
- [ ] Security audit clean
- [ ] Documentation updated
- [ ] CHANGELOG.md updated
- [ ] Version bumped in Cargo.toml
- [ ] ADRs reviewed and updated

**Release:**
- [ ] Create release branch
- [ ] Tag release: `git tag -a v1.0.0 -m "Release 1.0.0"`
- [ ] Push tag: `git push origin v1.0.0`
- [ ] Build release artifacts
- [ ] Create GitHub release
- [ ] Publish to crates.io (if applicable)

**Post-Release:**
- [ ] Merge release branch to main
- [ ] Update documentation links
- [ ] Announce release
- [ ] Monitor for issues

### Changelog Maintenance

Follow [Keep a Changelog](https://keepachangelog.com/) format:

```markdown
# [1.0.0] - 2024-01-15

## Added
- Force feedback engine with 1kHz processing
- Plugin system supporting WASM and native plugins
- Multi-game integration (iRacing, ACC, AMS2, rFactor 2)

## Changed
- Improved RT scheduling for lower jitter
- Updated dependency versions

## Fixed
- USB stall handling on device disconnect
- Memory leak in telemetry recorder

## Security
- Added Ed25519 code signing for native plugins
```

---

## Resources

### Links to Existing Documentation

- [README.md](../README.md) - Project overview and quick start
- [DEVELOPMENT.md](DEVELOPMENT.md) - Development guidelines
- [SYSTEM_INTEGRATION.md](SYSTEM_INTEGRATION.md) - System integration guide
- [SCHEMA_GOVERNANCE.md](SCHEMA_GOVERNANCE.md) - Schema governance policy
- [MIGRATION_PATTERNS.md](MIGRATION_PATTERNS.md) - Migration patterns
- [PLUGIN_DEVELOPMENT.md](PLUGIN_DEVELOPMENT.md) - Plugin development guide
- [USER_GUIDE.md](USER_GUIDE.md) - User documentation
- [POWER_MANAGEMENT_GUIDE.md](POWER_MANAGEMENT_GUIDE.md) - Power management
- [ANTICHEAT_COMPATIBILITY.md](ANTICHEAT_COMPATIBILITY.md) - Anticheat notes

### Architecture Decision Records

- [ADR Index](adr/INDEX.md) - Complete ADR list
- [ADR-0001: Force Feedback Mode Matrix](adr/0001-ffb-mode-matrix.md)
- [ADR-0002: IPC Transport Layer](adr/0002-ipc-transport.md)
- [ADR-0003: OWP-1 Protocol](adr/0003-owp1-protocol.md)
- [ADR-0004: Real-Time Scheduling](adr/0004-rt-scheduling-architecture.md)
- [ADR-0005: Plugin Architecture](adr/0005-plugin-architecture.md)
- [ADR-0006: Safety Interlocks](adr/0006-safety-interlocks.md)

### External Resources

**Rust:**
- [The Rust Programming Language](https://doc.rust-lang.org/book/)
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/)
- [API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [Rust Clippy Lints](https://rust-lang.github.io/rust-clippy/)

**Real-Time:**
- [Linux Real-Time Wiki](https://wiki.linuxfoundation.org/realtime/start)
- [Windows MMCSS](https://docs.microsoft.com/en-us/windows/win32/procthread/multimedia-class-scheduler-service)
- [Real-Time Linux Foundation](https://rt.wiki.kernel.org/)

**Testing:**
- [The Rust Testing Book](https://rust-lang.github.io/testing-book/)
- [Criterion.rs](https://bheisler.github.io/criterion.rs/book/)
- [Proptest](https://proptest-rs.github.io/proptest/proptest-tutorial/)

**Protobuf:**
- [Protocol Buffers Guide](https://developers.google.com/protocol-buffers)
- [Prost Documentation](https://docs.rs/prost/)

### Community Channels

- **GitHub Issues**: [Report bugs and request features](https://github.com/EffortlessMetrics/OpenRacing/issues)
- **GitHub Discussions**: [Community discussions](https://github.com/EffortlessMetrics/OpenRacing/discussions)
- **Discord**: (Coming soon)

### Getting Help

If you need help contributing:

1. Check existing documentation
2. Search GitHub issues and discussions
3. Ask a question in GitHub Discussions
4. Join our Discord (when available)

---

Thank you for contributing to OpenRacing! Your contributions help make sim-racing better for everyone.
