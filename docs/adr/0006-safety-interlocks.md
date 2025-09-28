# ADR-0006: Safety Interlocks and Fault Management

**Status:** Accepted  
**Date:** 2024-01-15  
**Authors:** Safety Team, Architecture Team  
**Reviewers:** Engineering Team, Legal Team  
**Related ADRs:** ADR-0003 (OWP-1 Protocol), ADR-0004 (RT Scheduling)

## Context

Racing wheel hardware can generate significant torque (5-25+ Nm) that poses safety risks if not properly controlled. The software must implement comprehensive safety measures including:

1. Physical interlock mechanisms for high-torque operation
2. Fault detection and rapid response (≤50ms to safe state)
3. Hands-off detection and automatic torque reduction
4. Temperature monitoring and thermal protection
5. Comprehensive FMEA (Failure Mode & Effects Analysis)

## Decision

Implement a multi-layered safety system with hardware and software interlocks:

**Physical Interlock Protocol:**
1. High-torque mode requires physical button combination on device
2. Challenge-response protocol with rolling tokens
3. UI consent flow with explicit warnings and disclaimers
4. State persists until device power-cycle or fault condition

**Fault Detection Matrix:**
- USB communication failures (timeout, stall, disconnect)
- Encoder anomalies (NaN values, impossible velocities)
- Thermal limits (temperature thresholds with hysteresis)
- Overcurrent protection (device-reported fault conditions)
- Plugin overruns (RT deadline violations)

**Response Actions (≤50ms):**
- Immediate torque ramp to zero with configurable rate
- Audible alert generation for user notification
- Blackbox fault marker with 2-second pre-fault history
- UI notification with recovery instructions
- Automatic quarantine for repeated plugin failures

## Rationale

- **Defense in Depth**: Multiple independent safety layers prevent single points of failure
- **Physical Confirmation**: Button combination ensures user intent for high-torque operation
- **Rapid Response**: 50ms fault-to-safe meets industry safety standards
- **Observability**: Comprehensive fault logging enables root cause analysis
- **User Agency**: Clear consent process with explicit risk acknowledgment

## Consequences

### Positive
- Comprehensive protection against hardware and software failures
- Clear user consent process reduces liability exposure
- Rapid fault response minimizes potential for injury
- Detailed fault logging enables continuous safety improvement
- Plugin isolation prevents third-party code from compromising safety

### Negative
- Additional complexity in safety state machine implementation
- User friction for high-torque mode activation
- Conservative fault thresholds may trigger false positives
- Requires careful tuning of thermal and timing thresholds

### Neutral
- Safety testing requires specialized equipment and procedures
- Fault injection testing needed for comprehensive validation
- Documentation and training requirements for safe operation

## Alternatives Considered

1. **Software-Only Safety**: Rejected due to single point of failure risk
2. **Always-On High Torque**: Rejected due to safety liability concerns
3. **Time-Based Unlock**: Rejected due to lack of positive user confirmation
4. **External Hardware Interlock**: Rejected due to complexity and cost

## Implementation Notes

**Safety State Machine:**
```rust
pub enum SafetyState {
    SafeTorque,
    HighTorqueChallenge { challenge_token: u32, expires: Instant },
    HighTorqueActive { since: Instant, device_token: u32 },
    Faulted { fault: FaultType, since: Instant },
}
```

**Fault Response Times:**
- Detection: ≤10ms (RT thread monitoring)
- Decision: ≤5ms (safety policy evaluation)
- Action: ≤35ms (torque ramp to zero)
- Total: ≤50ms (requirement compliance)

**Physical Interlock Sequence:**
1. User requests high-torque mode in UI
2. Service sends challenge to device (Feature Report 0x03)
3. Device requires button combination (e.g., both clutch paddles 2s)
4. Device responds with signed token in telemetry report
5. Service validates token and enables high-torque mode
6. State persists until power-cycle or fault

**FMEA Coverage:**
- USB stall/timeout → Ramp torque to zero, retry with backoff
- Encoder NaN/overflow → Latched fault, require restart
- Thermal limit → Gradual torque reduction, cooldown hysteresis
- Plugin overrun → Drop plugin, continue engine operation
- Power loss → Hardware failsafe (spring return to center)

## Compliance & Verification

**Safety Testing Requirements:**
- Fault injection testing for all defined failure modes
- Timing validation with oscilloscope measurement
- Thermal testing with controlled temperature chambers
- USB stress testing with deliberate disconnections
- Plugin crash testing with malicious code injection

**Validation Metrics:**
- Fault detection time: ≤10ms measured
- Response time: ≤50ms total measured
- False positive rate: <0.1% during normal operation
- Recovery success rate: >99% after transient faults

**Documentation Requirements:**
- User safety manual with clear warnings and procedures
- Technical safety specification for device manufacturers
- Fault code reference with troubleshooting procedures
- Safety test procedures and acceptance criteria

## References

- Requirements: SAFE-01 through SAFE-05, NFR-01 (timing)
- Design Document: Safety State Machine
- IEC 61508: Functional Safety Standard
- ISO 26262: Automotive Safety Lifecycle
- Fault Response Procedures: `docs/safety/fault-response.md`