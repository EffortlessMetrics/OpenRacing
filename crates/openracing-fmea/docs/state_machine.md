# FMEA State Machine Documentation

## Overview

The FMEA (Failure Mode & Effects Analysis) system implements a state machine for managing fault detection, handling, and recovery in the force feedback system.

## State Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│                         ┌─────────────┐                         │
│                         │   NORMAL    │                         │
│                         └──────┬──────┘                         │
│                                │                                │
│              ┌─────────────────┼─────────────────┐              │
│              │                 │                 │              │
│              ▼                 ▼                 ▼              │
│     ┌─────────────┐   ┌─────────────┐   ┌─────────────┐        │
│     │   SOFT      │   │  QUARANTINE │   │    LOG &    │        │
│     │   STOP      │   │             │   │  CONTINUE   │        │
│     └──────┬──────┘   └──────┬──────┘   └──────┬──────┘        │
│            │                 │                 │                │
│            ▼                 │                 │                │
│     ┌─────────────┐          │                 │                │
│     │   FAULTED   │◄─────────┴─────────────────┘                │
│     └──────┬──────┘                                            │
│            │                                                    │
│            │ can_recover                                        │
│            ▼                                                    │
│     ┌─────────────┐                                            │
│     │ RECOVERING  │                                            │
│     └──────┬──────┘                                            │
│            │                                                    │
│            │ success                                            │
│            └────────────────────────────────────────────────────┘
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## States

### Normal

The system is operating normally with no active faults.

- **Entry**: System initialization, successful fault recovery
- **Exit**: Fault detected
- **Actions**: None

### SoftStop

A fault has been detected and the system is ramping torque to zero.

- **Entry**: Fault with `SoftStop` or `SafeMode` action detected
- **Exit**: Ramp complete (typically 50ms)
- **Actions**: Linear torque ramp from current to zero
- **RT-Safe**: Yes, bounded execution time

### Quarantine

A plugin has exceeded its timing budget and is being isolated.

- **Entry**: Plugin overrun fault detected
- **Exit**: Quarantine period expires (5 minutes default)
- **Actions**: Plugin excluded from execution
- **RT-Safe**: Yes, no blocking operations

### LogAndContinue

A minor fault (timing violation) has been detected but operation continues.

- **Entry**: Timing violation fault detected
- **Exit**: None (transient state)
- **Actions**: Log violation details
- **RT-Safe**: Yes, minimal overhead

### Faulted

A fault has occurred and soft-stop is complete. Waiting for recovery or manual intervention.

- **Entry**: Soft-stop complete, non-recoverable fault
- **Exit**: Manual clear or automatic recovery
- **Actions**: Maintain zero torque, alert user

### Recovering

Automatic recovery procedure is in progress.

- **Entry**: Recoverable fault, soft-stop complete
- **Exit**: Recovery success, failure, or timeout
- **Actions**: Execute recovery steps

## Transitions

| From        | To          | Trigger                  | Action                           |
|-------------|-------------|--------------------------|----------------------------------|
| Normal      | SoftStop    | Critical fault           | Start torque ramp                |
| Normal      | Quarantine  | Plugin overrun           | Quarantine plugin                |
| Normal      | LogAndContinue | Timing violation      | Log and continue                 |
| SoftStop    | Faulted     | Ramp complete            | Trigger audio alert              |
| Faulted     | Recovering  | Recovery initiated       | Execute recovery steps           |
| Recovering  | Normal      | Recovery success         | Reset detection state            |
| Recovering  | Faulted     | Recovery failure         | Alert user                       |
| Any         | Normal      | Manual fault clear       | Reset all states                 |

## Fault Types and Actions

| Fault Type              | Action          | Recoverable | Max Response |
|-------------------------|-----------------|-------------|--------------|
| UsbStall               | SoftStop        | Yes         | 50ms         |
| EncoderNaN             | SoftStop        | No          | 50ms         |
| ThermalLimit           | SoftStop        | Yes         | 50ms         |
| Overcurrent            | SoftStop        | No          | 10ms         |
| PluginOverrun          | Quarantine      | Yes         | 1ms          |
| TimingViolation        | LogAndContinue  | Yes         | 1ms          |
| SafetyInterlockViolation | SafeMode      | No          | 10ms         |
| HandsOffTimeout        | SoftStop        | No          | 50ms         |
| PipelineFault          | Restart         | Yes         | 10ms         |

## RT-Safety Guarantees

All detection methods are RT-safe:

1. **No heap allocations** in hot paths
2. **No blocking operations** (I/O, locks, syscalls)
3. **Bounded execution time** for all operations
4. **Deterministic behavior** regardless of input

## Thread Safety

The FMEA system is designed for single-threaded RT operation. For multi-threaded use:

- Wrap in `Mutex` or `RwLock` for shared access
- Use atomic operations for lock-free statistics access
- Consider lock-free queues for fault event notification

## Performance Characteristics

| Operation              | Typical Time | Max Time |
|------------------------|--------------|----------|
| detect_usb_fault       | < 100ns      | < 500ns  |
| detect_encoder_fault   | < 100ns      | < 500ns  |
| detect_thermal_fault   | < 100ns      | < 500ns  |
| handle_fault           | < 1μs        | < 10μs   |
| update_soft_stop       | < 50ns       | < 100ns  |