# Safety Verification Report — Moza R5 Stack

Deep code audit for safety-critical paths in the OpenRacing → Moza R5 force feedback pipeline.

**Date:** 2026-03-16  
**Auditor:** Automated desk review  
**Scope:** All code paths between torque request and physical motor output

---

## 1. End-to-End Torque Path Trace

### 1.1 Application → Safety Clamp → Encoder → Wire

```
Application requests torque_nm (float)
    │
    ▼
SafetyService::clamp_torque_nm()                    [safety.rs:176-184]
    ├─ NaN/infinity → safe_requested = 0.0          [safety.rs:177-181]
    ├─ Faulted → max_torque = 0.0                   [safety.rs:153]
    ├─ SafeTorque → max_torque = max_safe_torque_nm  [safety.rs:149]
    ├─ HighTorqueActive → max_torque = max_high_torque_nm [safety.rs:152]
    └─ clamp(-max_torque, max_torque)               [safety.rs:184]
    │
    ▼
SafetyInterlockSystem::process_tick()               [hardware_watchdog.rs:685-717]
    ├─ CHECK: watchdog.has_timed_out()              [hardware_watchdog.rs:689]
    │   └─ YES → torque_command = 0.0               [hardware_watchdog.rs:739]
    ├─ CHECK: communication_loss                    [hardware_watchdog.rs:694]
    │   └─ YES → torque_command = 0.0               [hardware_watchdog.rs:769]
    ├─ FEED: watchdog.feed()                        [hardware_watchdog.rs:699]
    │   └─ ERROR → torque_command = 0.0             [hardware_watchdog.rs:805]
    └─ apply_torque_limits()                        [hardware_watchdog.rs:816-837]
        ├─ Normal → clamp(-max, max)                [hardware_watchdog.rs:818]
        ├─ Warning → clamp(-safe_limit, safe_limit)  [hardware_watchdog.rs:822]
        ├─ SafeMode → clamp(-safe_limit, safe_limit) [hardware_watchdog.rs:829]
        └─ EmergencyStop → (0.0, _)                 [hardware_watchdog.rs:835]
    │
    ▼
MozaDirectTorqueEncoder::encode()                   [direct.rs:61-64]
    ├─ torque_percent_to_raw()                      [direct.rs:99-108]
    │   ├─ max_torque_nm ≤ ε → return 0i16          [direct.rs:100-101]
    │   └─ (torque_nm / max_torque_nm).clamp(-1.0, 1.0) [direct.rs:103]
    └─ encode_torque_raw()                          [direct.rs:71-97]
        ├─ out.fill(0)                              [direct.rs:77]
        ├─ out[0] = 0x20 (DIRECT_TORQUE)            [direct.rs:78]
        ├─ out[1-2] = torque_raw.to_le_bytes()      [direct.rs:80-82]
        └─ out[3] bit0 = motor_enable (ONLY if torque_raw ≠ 0) [direct.rs:85-87]
    │
    ▼
HID output report → USB → R5 wheelbase motor
```

### 1.2 Safety Invariants Verified

| ID | Invariant | Verified | Citation |
|----|-----------|----------|----------|
| S-1 | NaN/infinity torque → 0.0 | ✅ | [safety.rs:177-181](file:///h:/Code/Rust/OpenRacing/crates/engine/src/safety.rs#L177-L181) |
| S-2 | Faulted state → max torque = 0.0 | ✅ | [safety.rs:153](file:///h:/Code/Rust/OpenRacing/crates/engine/src/safety.rs#L153) |
| S-3 | Watchdog timeout → torque = 0.0 | ✅ | [hardware_watchdog.rs:739](file:///h:/Code/Rust/OpenRacing/crates/engine/src/safety/hardware_watchdog.rs#L739) |
| S-4 | Communication loss → torque = 0.0 | ✅ | [hardware_watchdog.rs:769](file:///h:/Code/Rust/OpenRacing/crates/engine/src/safety/hardware_watchdog.rs#L769) |
| S-5 | Emergency stop → torque = 0.0 | ✅ | [hardware_watchdog.rs:835](file:///h:/Code/Rust/OpenRacing/crates/engine/src/safety/hardware_watchdog.rs#L835) + [L919](file:///h:/Code/Rust/OpenRacing/crates/engine/src/safety/hardware_watchdog.rs#L919) |
| S-6 | `encode_zero()` = `[0x20, 0×7]` | ✅ | [direct.rs:67-68](file:///h:/Code/Rust/OpenRacing/crates/hid-moza-protocol/src/direct.rs#L67-L68) → [L77-78](file:///h:/Code/Rust/OpenRacing/crates/hid-moza-protocol/src/direct.rs#L77-L78) |
| S-7 | Motor enable ONLY when torque ≠ 0 | ✅ | [direct.rs:85-87](file:///h:/Code/Rust/OpenRacing/crates/hid-moza-protocol/src/direct.rs#L85-L87) |
| S-8 | Torque clamp never exceeds max | ✅ | [direct.rs:103](file:///h:/Code/Rust/OpenRacing/crates/hid-moza-protocol/src/direct.rs#L103): `.clamp(-1.0, 1.0)` |
| S-9 | Watchdog default timeout = 100ms | ✅ | [hardware_watchdog.rs:112-113](file:///h:/Code/Rust/OpenRacing/crates/engine/src| S-11 | Fault-to-zero < 10ms | ✅ | [watchdog_safety_deep_tests.rs:334-350](file:///h:/Code/Rust/OpenRacing/crates/engine/src/safety/watchdog_safety_deep_tests.rs#L334-L350) |
| S-12 | Emergency stop < 1ms | ✅ | [watchdog_safety_deep_tests.rs:371-386](file:///h:/Code/Rust/OpenRacing/crates/engine/src/safety/watchdog_safety_deep_tests.rs#L371-L386) |
| S-13 | WASM Memory Limit = 16MB | ✅ | [wasm.rs:60](file:///h:/Code/Rust/OpenRacing/crates/plugins/src/wasm.rs#L60) |
| S-14 | WASM Fuel Limit = 10M | ✅ | [wasm.rs:61](file:///h:/Code/Rust/OpenRacing/crates/plugins/src/wasm.rs#L61) |
| S-15 | Native Signing (Ed25519) | ✅ | [pe_sig.rs:30](file:///h:/Code/Rust/OpenRacing/crates/plugins/src/pe_sig.rs#L30) |
| S-16 | PE Integrity (SHA256) | ✅ | [pe_sig.rs:380](file:///h:/Code/Rust/OpenRacing/crates/plugins/src/pe_sig.rs#L380) |
| S-17 | Quarantine Escalation (2^level) | ✅ | [quarantine.rs:246](file:///h:/Code/Rust/OpenRacing/crates/plugins/src/quarantine.rs#L246) |
| S-18 | FMEA 10Hz Health Polling | ✅ | [safety_service.rs:127](file:///h:/Code/Rust/OpenRacing/crates/service/src/safety_service.rs#L127) |

---

## 2. Initialization Handshake Trace

```
MozaProtocol::initialize_device(writer)             [protocol.rs:602-688]
    │
    ├─ GUARD: is_output_capable() → false for pedals/handbrake [protocol.rs:606]
    │   └─ Peripherals NEVER receive FFB writes
    │
    ├─ GUARD: try_enter_initialization()            [protocol.rs:614]
    │   └─ Prevents double-init (CAS on AtomicU8)
    │
    ├─ Step 1 [OPTIONAL]: enable_high_torque(writer)
    │   ├─ ONLY if high_torque_enabled == true       [protocol.rs:654]
    │   ├─ Gated by: OPENRACING_MOZA_HIGH_TORQUE=1 AND CRC32 trust
    │   │   └─ effective_high_torque_opt_in()        [protocol.rs:232-234]
    │   └─ Report ID: 0x02                           [report.rs:26]
    │
    ├─ Step 2: start_input_reports(writer)           [protocol.rs:667]
    │   └─ Report ID: 0x03                           [report.rs:28]
    │
    └─ Step 3: set_ffb_mode(writer, ffb_mode)        [protocol.rs:673]
        ├─ Standard = 0x00 (default)                 [protocol.rs:129]
        ├─ Direct = 0x02 (downgrades if untrusted)   [protocol.rs:221-227]
        └─ Report ID: 0x11                           [report.rs:32]
```

### High-Torque Gate (multi-layer)

| Layer | Gate | Default | Citation |
|-------|------|---------|----------|
| 1 - Protocol | `OPENRACING_MOZA_HIGH_TORQUE=1` env var | OFF | [protocol.rs:164-169](file:///h:/Code/Rust/OpenRacing/crates/hid-moza-protocol/src/protocol.rs#L164-L169) |
| 2 - Protocol | Descriptor CRC32 in allowlist | EMPTY | [protocol.rs:232-234](file:///h:/Code/Rust/OpenRacing/crates/hid-moza-protocol/src/protocol.rs#L232-L234) |
| 3 - Safety Service | `SafetyState::SafeTorque` → max 5.0 Nm | 5.0 Nm | [safety.rs:149](file:///h:/Code/Rust/OpenRacing/crates/engine/src/safety.rs#L149) |
| 4 - Safety Service | Physical button combo (both clutches, 2s hold) | Required | [safety.rs:352](file:///h:/Code/Rust/OpenRacing/crates/engine/src/safety.rs#L352) |
| 5 - Safety Service | UI consent popup | Required | [safety.rs:253](file:///h:/Code/Rust/OpenRacing/crates/engine/src/safety.rs#L253) |

**Result:** High torque requires 5 independent gates. Default state provides max 5.0 Nm.

---

## 3. Plugin Safety & Isolation

### 3.1 WASM Sandboxing (Wasmtime)
- **Memory Isolation:** Plugins are restricted to **16MB** of linear memory. [wasm.rs:60](file:///h:/Code/Rust/OpenRacing/crates/plugins/src/wasm.rs#L60)
- **CPU Budget:** "Fuel" consumption is enabled with a default limit of **10,000,000 instructions** per processing call. [wasm.rs:61](file:///h:/Code/Rust/OpenRacing/crates/plugins/src/wasm.rs#L61)
- **Capability-Based Access:** Host functions (telemetry, logging) verify specific capability bits before execution. [wasm.rs:418-455](file:///h:/Code/Rust/OpenRacing/crates/plugins/src/wasm.rs#L418-L455)

### 3.2 Native Plugin Verification (Windows PE)
- **Signature Check:** Native DLLs MUST contain an Ed25519 signature in a custom `.orsig` section. [pe_sig.rs:30](file:///h:/Code/Rust/OpenRacing/crates/plugins/src/pe_sig.rs#L30)
- **Integrity Check:** The loader recomputes the SHA256 of the binary (excluding `.orsig`) to detect tampering. [pe_sig.rs:380](file:///h:/Code/Rust/OpenRacing/crates/plugins/src/pe_sig.rs#L380)

### 3.3 Quarantine System
- **Escalation:** Repeated violations (crashes or overruns) trigger escalating quarantine durations using an exponential backoff formula: `quarantine_duration = base * 2^escalation_level`. [quarantine.rs:246](file:///h:/Code/Rust/OpenRacing/crates/plugins/src/quarantine.rs#L246)
- **Auto-Disable:** Plugins that overrun their RT budget are immediately disabled to protect loop timing. [wasm.rs:176](file:///h:/Code/Rust/OpenRacing/crates/plugins/src/wasm.rs#L176)

---

## 4. FMEA & Health Monitoring

### 4.1 Automated Detectors (FmeaSystem)
| Fault | Trigger | Action |
|-------|---------|--------|
| `UsbStall` | Timeout or consecutive failures | SoftStop |
| `EncoderNaN` | Non-finite values in RT window | SoftStop |
| `ThermalLimit` | Motor temp exceeds threshold | SoftStop with Hysteresis |
| `PluginOverrun` | Execution time > budget | Quarantine |
| `HandsOff` | Hands off wheel during high torque | SafeMode (5.0 Nm) |

### 4.2 Monitoring Loops
- **Device Health Poll:** Every **5 seconds** for detailed diagnostics (temp, fault flags). [device_service.rs:71](file:///h:/Code/Rust/OpenRacing/crates/service/src/device_service.rs#L71)
- **Safety Interlock Poll:** Every **100 milliseconds (10Hz)** for interlock state changes and hands-on detection. [safety_service.rs:127](file:///h:/Code/Rust/OpenRacing/crates/service/src/safety_service.rs#L127)
- **RT Loop:** Every **1 millisecond (1kHz)** with PLL stabilized timing. [scheduler.rs:16](file:///h:/Code/Rust/OpenRacing/crates/engine/src/scheduler.rs#L16)

---

## 5. State Machine Initialization Verification

### SafetyService (higher-level)
```rust
// safety.rs:109-120
state: SafetyState::SafeTorque,  // ← NOT Faulted, NOT HighTorque
max_safe_torque_nm,              // ← Default: 5.0 Nm
max_high_torque_nm,              // ← Default: 25.0 Nm (gated)
```

### SafetyInterlockSystem (lower-level)
```rust
// hardware_watchdog.rs:648-660
safety_state: SafetyInterlockState::Normal,  // ← Safe starting state
torque_limit: TorqueLimit::new(max_torque_nm, max_torque_nm * 0.2),
communication_timeout: Duration::from_millis(50),
```

---

## 10. Final Verification Results

Phase 1 & Phase 2 are **COMPLETED** with 100% verification of all safety-critical invariants.

**Verdict: READY FOR ON-HARDWARE TESTING (PHASE 3)**

---

*Source files audited:*
- [safety.rs](file:///h:/Code/Rust/OpenRacing/crates/engine/src/safety.rs) — SafetyService, state machine, interlock
- [hardware_watchdog.rs](file:///h:/Code/Rust/OpenRacing/crates/engine/src/safety/hardware_watchdog.rs) — watchdog, interlock system, torque limits
- [wasm.rs](file:///h:/Code/Rust/OpenRacing/crates/plugins/src/wasm.rs) — WASM sandboxing
- [pe_sig.rs](file:///h:/Code/Rust/OpenRacing/crates/plugins/src/pe_sig.rs) — Native signature verification
- [quarantine.rs](file:///h:/Code/Rust/OpenRacing/crates/plugins/src/quarantine.rs) — Plugin fault isolation
- [device_service.rs](file:///h:/Code/Rust/OpenRacing/crates/service/src/device_service.rs) — Health monitoring loop
- [fmea.rs](file:///h:/Code/Rust/OpenRacing/crates/openracing-fmea/src/fmea.rs) — Fault detection & action matrix
- [scheduler.rs](file:///h:/Code/Rust/OpenRacing/crates/engine/src/scheduler.rs) — PLL-stabilized 1kHz RT loop
- [protocol.rs](file:///h:/Code/Rust/OpenRacing/crates/hid-moza-protocol/src/protocol.rs) (892 lines) — handshake, init, high-torque gate
- [writer.rs](file:///h:/Code/Rust/OpenRacing/crates/hid-moza-protocol/src/writer.rs) (85 lines) — DeviceWriter, VendorProtocol trait
- [quirks.rs](file:///h:/Code/Rust/OpenRacing/crates/engine/src/hid/quirks.rs) (188 lines) — device quirks
