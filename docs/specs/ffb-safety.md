# Safety-Critical FFB Control Loop Specification

This document defines the requirements for OpenRacing's force-feedback (FFB) control loop to be treated as safety-critical.

It is intentionally written as an engineering spec, not a narrative:
- What runs on the **real-time (RT) path**
- What MUST happen on faults
- What evidence (tests/benchmarks) is required to claim safety properties

---

## 1) Scope and threat model

### 1.1 What is "safety-critical" here?

We treat the torque output path as safety-critical because:
- unexpected torque can cause injury, and
- the system must fail "quietly" (torque collapses to 0) on software failure.

This spec covers:
- RT scheduling behavior
- bounded execution requirements
- torque collapse semantics
- watchdog semantics (software + optional hardware)
- recording for post-fault analysis

This spec does NOT claim:
- the wheel base hardware is inherently safe
- USB/HID is deterministic
- the OS is real-time by default

Instead, we define how OpenRacing must behave given those constraints.

---

## 2) Control loop architecture

### 2.1 Threads and responsibilities

**RT thread (the control loop) MUST:**
- read pre-prepared inputs (telemetry snapshot, user inputs, profile params)
- compute torque command
- apply safety clamps / fault multipliers
- write the command to the device
- feed watchdog (if supported) after successful write
- publish a bounded "heartbeat" to supervisor (atomic or lock-free)

**Non-RT supervisor MUST:**
- ingest telemetry transports (network, shared memory)
- perform expensive diagnostics/logging
- manage configs and profile updates
- flush black-box recordings to disk
- request emergency stop on missed RT heartbeat deadlines

### 2.2 Data flow (high level)

Inputs (telemetry/user) -> Profile/controller -> Torque command
-> Safety clamp + fault multiplier -> Device write (USB/HID)
-> Watchdog feed + heartbeat update

---

## 3) Real-time invariants

### 3.1 Scheduling

The scheduler MUST be "absolute time" based:
- next deadline = (start_time + n * period)
- not "sleep(period)" drift-based timing.

Tick period:
- nominal: **1 ms** (1 kHz)

### 3.2 Bounded execution

The RT tick MUST have a defined budget, e.g.:
- "< 500 us typical, < 900 us max" (thresholds are policy, but MUST exist)

The RT tick MUST NOT:
- block on OS primitives (mutexes, file IO, network IO)
- allocate on the heap after initialization
- perform unbounded work (e.g., iterating over unbounded collections)

### 3.3 Allocation and locking policy

After startup, the RT thread MUST:
- perform **0 heap allocations**
- take **0 blocking locks**

Allowed RT communication patterns:
- atomics
- lock-free / wait-free ring buffer writes
- preallocated fixed-capacity buffers

---

## 4) Torque output contract

### 4.1 Symbols

- `tau_cmd`: controller output torque command (Nm or normalized units)
- `tau_max`: configured absolute maximum torque magnitude
- `m_fault`: fault multiplier in [0, 1] (soft-stop / ramp-down)
- `estop`: emergency stop boolean

### 4.2 Required behavior

Final torque MUST be computed as:

1. Controller torque:
   - `tau_cmd = controller(inputs)`

2. Fault modulation:
   - `tau_mod = tau_cmd * m_fault`

3. Safety clamp:
   - `tau_safe = clamp(tau_mod, -tau_max, +tau_max)`

4. Emergency stop:
   - if `estop == true`, output torque MUST be **exactly 0** regardless of `tau_cmd`

Implementation note:
- Runtime output should be clamped in **Nm** using one authoritative safety function
  (`Faulted -> 0 Nm`), instead of split normalized-ratio paths.

### 4.3 Fault-to-zero deadlines

If a fault is raised:
- `m_fault` MUST reach 0 within a bounded number of ticks (policy-defined, but MUST be testable).
- `estop` MUST force 0 on the next tick (no ramp).

---

## 5) Fault model and state machine

### 5.1 Fault categories (minimum)

- RT scheduling fault (deadline miss, stall)
- Device IO fault (write failure, device disconnect)
- Sensor/telemetry fault (stale data, out-of-range)
- Thermal / power limits (if exposed by device)
- External emergency stop input (if present)

### 5.2 State requirements

The safety subsystem MUST expose:
- a state machine that is visible to both RT and supervisor
- a single "authoritative" decision for:
  - `m_fault`
  - `estop`
  - current fault reason(s)

State transitions MUST be deterministic and testable.

---

## 6) Watchdog semantics

### 6.1 Software watchdog (mandatory)

The supervisor MUST maintain a "heartbeat deadline," e.g.:
- RT thread updates `last_tick_ns` each tick (atomic store).
- Supervisor checks:
  - if `now - last_tick_ns > threshold`, request emergency stop.

### 6.2 Hardware watchdog (optional capability)

If the device supports a hardware watchdog:
- The device interface MUST expose:
  - `arm_watchdog(timeout_ms)`
  - `feed_watchdog()`
  - `disarm_watchdog()`

RT thread MUST:
- feed the watchdog **only after** successfully writing torque output.

On RT stall:
- hardware watchdog SHOULD autonomously collapse torque to 0 (device responsibility),
- supervisor MUST still request emergency stop when it detects missed heartbeat.

---

## 7) Black-box recording ("flight recorder")

### 7.1 Requirements

The black-box recorder MUST:
- capture RT-relevant state without blocking the RT loop
- write to a preallocated ring buffer (RT)
- flush to disk from a non-RT thread

### 7.2 Minimum per-tick record (suggested)

- monotonic tick time or timestamp
- sequence number
- input snapshot (or hashes/pointers to snapshots)
- `tau_cmd`, `tau_safe`
- safety state (`m_fault`, `estop`, fault codes)
- device write result / error flags

### 7.3 Triggers

Recorder SHOULD flush:
- on fault transition
- on explicit user request
- optionally on periodic snapshots (with bounded overhead)

---

## 8) Evidence and CI gates

To claim safety properties, OpenRacing MUST ship tests that prove:

### 8.1 Torque collapse tests
- fault injected at tick T -> `tau_safe == 0` by tick T+N (policy-defined)
- estop injected -> `tau_safe == 0` next tick

### 8.2 Allocation tests
- in an "rt-hardening" build:
  - heap allocations after init == 0

### 8.3 Timing/jitter tests
- benchmark harness records tick latency/jitter
- CI gate enforces thresholds appropriate for the runner environment
  (thresholds must be conservative and explainable)

---

## 9) Implementation touchpoints

Likely files to anchor the spec:
- `crates/engine/src/engine.rs` (RT loop)
- `crates/engine/src/scheduler.rs` (timing)
- `crates/engine/src/safety.rs` and `crates/engine/src/safety/*` (fault + soft-stop)
- `crates/engine/src/ports.rs` (device IO + watchdog capability surface)
- `crates/cli/src/commands/diag.rs` (black-box tooling surface; must not be "mock" in a safety claim)

---

## 10) "Claim checklist" (PR review aid)

You may only claim "safety-critical" if all are true:
- RT loop has absolute scheduling and bounded tick time.
- RT thread allocations after init are impossible (enforced).
- Fault -> torque collapse is deterministic and covered by tests.
- Watchdog semantics are explicit at the device interface.
- Diagnostics/recording cannot interfere with RT timing (ring buffer + non-RT flush).
