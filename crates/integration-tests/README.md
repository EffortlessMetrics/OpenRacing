# Racing Wheel Integration Tests

This crate provides comprehensive integration testing for the Racing Wheel Software suite, implementing the requirements from task 18 of the implementation plan.

## Overview

The integration test suite covers:

- **End-to-end user workflows** (UJ-01 through UJ-04)
- **CI performance gates** (jitter ≤0.25ms, HID latency ≤300μs)
- **48-hour soak testing** for continuous operation validation
- **Acceptance tests** mapped to specific requirement IDs
- **Hot-plug stress testing** with rapid connect/disconnect cycles

## Test Categories

### User Journey Tests (`user_journeys.rs`)

Tests complete end-to-end user workflows:

- **UJ-01**: First-run experience (device detection → safe torque → game config → LEDs active → profile saved)
- **UJ-02**: Per-car profile switching (sim start → auto-switch ≤500ms → apply settings)
- **UJ-03**: Fault handling (thermal fault → soft-stop ≤50ms → recovery)
- **UJ-04**: Debug workflow (blackbox recording → support bundle → replay)

### Performance Gates (`gates.rs`)

Critical performance tests that must pass in CI:

- **FFB Jitter Gate**: P99 jitter ≤0.25ms at 1kHz
- **HID Latency Gate**: P99 write latency ≤300μs
- **Zero Missed Ticks**: No missed ticks over test duration
- **Combined Load**: All systems active with performance maintained

### Stress Tests (`stress.rs`)

System resilience testing:

- **Hot-plug Stress**: Rapid device connect/disconnect cycles
- **Fault Injection**: All fault types with recovery validation
- **Memory Pressure**: Performance under memory constraints
- **CPU Load**: Performance under high CPU utilization

### Soak Tests (`soak.rs`)

Long-duration reliability testing:

- **Full Soak Test**: 48-hour continuous operation
- **CI Soak Test**: 1-hour abbreviated version for CI
- **Checkpoint System**: Hourly performance snapshots
- **Early Termination**: Automatic stop on performance degradation

### Acceptance Tests (`acceptance.rs`)

Requirement-mapped validation tests:

- **Automated DoD Verification**: Each test validates Definition of Done criteria
- **Requirement Coverage**: Tests map to specific requirement IDs (DM-01, FFB-01, etc.)
- **Comprehensive Coverage**: All major requirements have corresponding tests

## Running Tests

### Quick Tests (CI-friendly)

```bash
# Smoke test
cargo test test_smoke_test --package racing-wheel-integration-tests

# Performance gates
cargo run --package racing-wheel-integration-tests --bin performance-gate

# User journeys
cargo test test_user_journey --package racing-wheel-integration-tests

# Acceptance tests
cargo test test_acceptance_tests_subset --package racing-wheel-integration-tests
```

### Stress Tests

```bash
# Hot-plug stress (5 minutes)
cargo run --package racing-wheel-integration-tests --bin hotplug-stress -- --duration 300

# All stress tests
cargo test test_hotplug_stress --package racing-wheel-integration-tests
cargo test test_fault_injection_stress --package racing-wheel-integration-tests
cargo test test_memory_pressure_stress --package racing-wheel-integration-tests
cargo test test_cpu_load_stress --package racing-wheel-integration-tests
```

### Soak Tests

```bash
# CI soak test (1 hour)
cargo run --package racing-wheel-integration-tests --bin soak-test -- --mode ci

# Full soak test (48 hours)
cargo run --package racing-wheel-integration-tests --bin soak-test -- --mode full
```

### Long-running Tests

```bash
# Run ignored tests (includes CI soak test)
cargo test --package racing-wheel-integration-tests -- --ignored
```

## Performance Thresholds

The tests enforce the following performance gates:

| Metric | Threshold | Requirement |
|--------|-----------|-------------|
| FFB Jitter P99 | ≤0.25ms | FFB-01, NFR-01 |
| HID Write Latency P99 | ≤300μs | FFB-01, NFR-01 |
| Missed Ticks | 0 | FFB-01, NFR-03 |
| Memory Usage | ≤150MB | NFR-02 |
| CPU Usage | ≤3% of one core | NFR-02 |

## Test Configuration

Tests use virtual devices by default for CI compatibility. Configuration options:

```rust
TestConfig {
    duration: Duration::from_secs(60),
    sample_rate_hz: 1000,
    virtual_device: true,
    enable_tracing: false,  // Reduce overhead for performance tests
    enable_metrics: true,
    stress_level: StressLevel::Medium,
}
```

## CI Integration

The tests are designed for CI integration with appropriate timeouts:

- **Smoke tests**: 10 minutes
- **Performance gates**: 15 minutes  
- **User journeys**: 20 minutes
- **Stress tests**: 30 minutes
- **CI soak test**: 75 minutes

See `.github/workflows/integration-tests.yml` for complete CI configuration.

## Output and Reporting

### Performance Reports

Tests generate detailed performance reports:

```
Performance Metrics:
- Jitter P50/P99: 0.089ms / 0.203ms (gate: ≤0.250ms)
- HID Latency P50/P99: 127.3μs / 287.1μs (gate: ≤300.0μs)
- Missed Ticks: 0 / 60000 (0.000000%)
- CPU Usage: 2.1%
- Memory Usage: 142.3MB
- Max Torque Saturation: 0.0%
```

### Test Artifacts

Tests generate artifacts for analysis:

- `target/performance-*.json`: Performance gate results
- `target/soak-*.json`: Soak test checkpoints and summary
- `target/stress-*.json`: Stress test results
- `target/acceptance_test_report.json`: Acceptance test coverage

### Requirement Coverage

Each test reports which requirements it validates:

```rust
TestResult {
    passed: true,
    duration: Duration::from_secs(30),
    metrics: PerformanceMetrics { /* ... */ },
    errors: vec![],
    requirement_coverage: vec!["FFB-01", "NFR-01", "SAFE-03"],
}
```

## Architecture

The integration test suite follows clean architecture principles:

- **Common utilities** (`common.rs`): Test harness, virtual devices, metrics collection
- **Test fixtures** (`fixtures.rs`): Reusable test data and scenarios
- **Performance utilities** (`performance.rs`): Benchmarking and measurement tools
- **Binary executables**: Standalone test runners for CI

### Virtual Device System

Tests use virtual devices that simulate real hardware:

```rust
VirtualDevice {
    id: DeviceId,
    capabilities: DeviceCapabilities,
    connected: bool,
    last_torque_command: f32,
    telemetry_data: VirtualTelemetry,
}
```

This allows testing without physical hardware while maintaining realistic behavior.

## Development

### Adding New Tests

1. **User Journey**: Add to `user_journeys.rs` following UJ-XX pattern
2. **Performance Gate**: Add to `gates.rs` with specific thresholds
3. **Stress Test**: Add to `stress.rs` with appropriate load simulation
4. **Acceptance Test**: Add to `acceptance.rs` with requirement mapping

### Test Naming Convention

- User journeys: `test_ujXX_description`
- Performance gates: `test_XXX_gate`
- Stress tests: `test_XXX_stress`
- Acceptance tests: `test_XXX` (mapped to requirement ID)

### Performance Considerations

- Use `enable_tracing: false` for performance-critical tests
- Virtual devices reduce I/O overhead
- Histogram-based metrics for accurate percentile calculation
- Separate RT simulation from test orchestration

## Troubleshooting

### Common Issues

1. **Performance gate failures**: Check system load, use dedicated test machine
2. **Timing issues**: Ensure system has RT capabilities, check CPU governor
3. **Memory issues**: Monitor for leaks in long-running tests
4. **CI timeouts**: Adjust test durations for CI environment

### Debug Mode

Enable verbose logging for debugging:

```bash
RUST_LOG=racing_wheel=debug,integration_tests=debug cargo test
```

### Performance Analysis

Use the performance utilities for detailed analysis:

```rust
let mut latency_measurement = LatencyMeasurement::new()?;
// ... collect samples ...
println!("{}", latency_measurement.report());
```

## Requirements Mapping

This integration test suite validates the following requirements:

| Requirement | Test Coverage |
|-------------|---------------|
| DM-01, DM-02 | Device management, hot-plug tests |
| FFB-01, FFB-02, FFB-05 | Performance gates, anomaly handling |
| GI-01, GI-02 | Game integration, profile switching |
| LDH-01, LDH-04 | LED latency, rate independence |
| SAFE-01, SAFE-03 | Safety boot mode, fault response |
| PRF-01, PRF-02 | Profile hierarchy, validation |
| DIAG-01, DIAG-02 | Blackbox recording, replay |
| NFR-01, NFR-02, NFR-03 | Performance, resource usage, reliability |

The test suite provides comprehensive validation of the racing wheel software's critical functionality, performance characteristics, and reliability requirements.