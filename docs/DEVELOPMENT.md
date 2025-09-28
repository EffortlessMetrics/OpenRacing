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
- `benches/rt_timing.rs`: Real-time performance benchmarks
- `deny.toml`: Dependency and license configuration
- `clippy.toml`: Linting configuration
- `rustfmt.toml`: Code formatting configuration