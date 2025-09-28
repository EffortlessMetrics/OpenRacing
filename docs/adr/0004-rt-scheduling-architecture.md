# ADR-0004: Real-Time Scheduling Architecture

**Status:** Accepted  
**Date:** 2024-01-15  
**Authors:** Architecture Team, Performance Team  
**Reviewers:** Engineering Team, Safety Team  
**Related ADRs:** ADR-0001 (FFB Modes), ADR-0003 (OWP-1 Protocol)

## Context

The racing wheel software requires precise 1kHz force feedback timing with strict jitter requirements (p99 ≤ 0.25ms) as specified in NFR-01. The system must maintain real-time guarantees across different operating systems while handling:

1. Absolute timing discipline for 1kHz tick generation
2. Platform-specific RT scheduling and priority management
3. PLL-based drift correction for USB frame synchronization
4. Busy-spin tail for final precision timing
5. Performance monitoring and gate validation

## Decision

Implement a multi-layered RT scheduling architecture:

**AbsoluteScheduler Core:**
- Platform-specific absolute timer implementation
- Linux: `clock_nanosleep(..., TIMER_ABSTIME)` with `CLOCK_MONOTONIC`
- Windows: `WaitableTimer` with high-resolution timing
- PLL for drift correction against USB frames
- Busy-spin tail (~50-80μs) for final precision

**RT Thread Setup:**
- Linux: `SCHED_FIFO` via rtkit, `mlockall(MCL_CURRENT|MCL_FUTURE)`
- Windows: MMCSS "Games" category, disable power throttling
- Dedicated CPU core affinity where possible
- Memory pre-allocation and page locking

**Performance Monitoring:**
- Continuous jitter measurement and p99 tracking
- Missed tick detection and rate calculation
- ETW/tracepoints for system-level observability
- CI performance gates with automatic validation

## Rationale

- **Precision**: Absolute timing prevents cumulative drift from relative delays
- **Platform Optimization**: OS-specific APIs provide best possible timing guarantees
- **Drift Correction**: PLL maintains synchronization with hardware timing references
- **Observability**: Comprehensive metrics enable performance validation and debugging
- **Deterministic**: Predictable timing behavior across different system loads

## Consequences

### Positive
- Meets strict 1kHz timing requirements with measurable jitter bounds
- Platform-optimized implementation for best performance on each OS
- Comprehensive monitoring enables proactive performance management
- CI gates prevent performance regressions
- Clear separation between timing and processing concerns

### Negative
- Complex platform-specific implementation and testing requirements
- RT privileges may require special setup (rtkit, MMCSS registration)
- Busy-spin increases CPU usage for precision timing
- Performance sensitive to system configuration and background load

### Neutral
- Requires careful tuning of PLL parameters for different hardware
- Performance gates may need adjustment for different CI environments
- RT thread isolation limits flexibility in processing pipeline

## Alternatives Considered

1. **Relative Sleep Timing**: Rejected due to cumulative drift and jitter accumulation
2. **Hardware Timer Interrupts**: Rejected due to kernel driver requirements
3. **Audio Callback Timing**: Rejected due to dependency on audio subsystem
4. **Cooperative Scheduling**: Rejected due to lack of timing guarantees

## Implementation Notes

**Timing Budget Allocation (1000μs @ 1kHz):**
- Input processing: 50μs
- Filter pipeline: 200μs (median), 800μs (p99)
- Output formatting: 50μs
- HID write: 100μs (median), 300μs (p99)
- Safety checks: 50μs
- Scheduler overhead: ~50μs

**PLL Configuration:**
- Target: USB frame timing (1ms nominal)
- Proportional gain: 0.1
- Integral gain: 0.01
- Maximum correction: ±100μs per tick

**Performance Gates:**
- P99 jitter ≤ 250μs (0.25ms)
- Missed tick rate ≤ 0.001%
- Processing median ≤ 50μs
- Processing p99 ≤ 200μs

## Compliance & Verification

- Continuous benchmarking with Criterion for statistical validation
- CI performance gates prevent regressions
- Platform-specific timing validation on reference hardware
- Oscilloscope validation for precision measurement
- Soak testing for long-term stability (48+ hours)

**Test Coverage:**
- Unit tests for scheduler logic and PLL behavior
- Integration tests with mock devices and synthetic load
- Performance tests with statistical analysis
- Platform-specific RT setup validation
- Fault injection for missed tick scenarios

## References

- Requirements: NFR-01 (Latency), FFB-01 (Tick discipline), FFB-04 (Timing budget)
- Design Document: Real-Time Force Feedback Engine
- Linux RT Documentation: https://wiki.linuxfoundation.org/realtime/start
- Windows MMCSS: https://docs.microsoft.com/en-us/windows/win32/procthread/multimedia-class-scheduler-service
- Performance Validation Script: `scripts/validate_performance.py`