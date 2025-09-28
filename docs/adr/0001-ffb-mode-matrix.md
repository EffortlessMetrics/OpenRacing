# ADR-0001: Force Feedback Mode Matrix

**Status:** Accepted  
**Date:** 2024-01-15  
**Authors:** Architecture Team  
**Reviewers:** Engineering Team  
**Related ADRs:** ADR-0002 (IPC Transport), ADR-0003 (OWP-1 Protocol)

## Context

Racing wheel software needs to support different force feedback delivery mechanisms depending on device capabilities and game compatibility. We need a unified approach that can handle:

1. Legacy wheels that only support DirectInput/PID effects
2. Modern open-protocol wheels that accept raw torque commands
3. Games that don't provide robust FFB but do provide telemetry

The system must maintain 1kHz precision while supporting capability negotiation and graceful fallbacks.

## Decision

Implement a three-mode FFB matrix with automatic capability negotiation:

1. **PID Pass-through Mode**: Game emits DirectInput/PID effects → Device processes via HID PID (0x0F)
2. **Raw-Torque Mode** (preferred): Host synthesizes torque @1kHz → Device via OWP-1 HID OUT report
3. **Telemetry-Synth Mode** (fallback): Host computes torque from game telemetry → Device via OWP-1

Mode selection occurs during device enumeration via Feature Report 0x01 capability negotiation.

## Rationale

- **Compatibility**: Supports both legacy and modern hardware without requiring separate codepaths
- **Performance**: Raw-torque mode enables full filter pipeline with 1kHz precision
- **Fallback**: Telemetry-synth ensures basic functionality even with limited game support
- **Deterministic**: Mode selection is based on declared capabilities, not runtime detection

## Consequences

### Positive
- Single codebase supports wide range of hardware
- Optimal performance path for modern devices
- Graceful degradation for limited scenarios
- Clear capability negotiation protocol

### Negative
- Increased complexity in engine initialization
- Need to maintain three different output paths
- Telemetry-synth mode has inherent latency limitations

### Neutral
- Mode switching requires device reconnection
- Each device operates in single mode for session duration

## Alternatives Considered

1. **Single Raw-Torque Mode**: Rejected due to legacy hardware compatibility requirements
2. **Runtime Mode Switching**: Rejected due to complexity and potential for timing issues
3. **Separate Applications**: Rejected due to user experience fragmentation

## Implementation Notes

- Capability negotiation happens once during device enumeration
- Mode stored in device state and persists for session
- Filter pipeline compilation varies by mode (full for raw-torque, limited for others)
- Safety systems operate consistently across all modes

## Compliance & Verification

- Unit tests for capability parsing and mode selection logic
- Integration tests with mock devices for each mode
- Performance tests verify 1kHz timing in raw-torque mode
- Compatibility tests with reference hardware for each mode

## References

- Requirements: FFB-01, FFB-02, GI-03, DM-01
- Design Document: FFB Engine Architecture
- OWP-1 Protocol Specification (ADR-0003)