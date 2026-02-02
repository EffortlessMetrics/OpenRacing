# Roadmap

This document outlines the development roadmap for OpenRacing. It tracks the implementation status of key features, architectural decisions, and future plans.

## Current Status (v0.3.0 - Q1 2026)

**Released Features:**
- **Core FFB Engine**: Real-time force feedback processing at 1kHz with zero-allocation RT path
- **Cross-Platform HID**: Full support for Linux (hidraw/udev) and Windows (overlapped I/O, MMCSS)
- **Plugin System**: WASM sandboxed runtime + Native plugins with Ed25519 signature verification
- **Game Telemetry**: Adapters for iRacing, ACC, Automobilista 2, and rFactor 2
- **Curve-Based FFB**: Customizable response curves (linear, exponential, logarithmic, Bezier)
- **Profile Inheritance**: Hierarchical profiles with up to 5 levels of inheritance
- **Tauri UI**: Device management, real-time telemetry display, profile application
- **CLI Tools**: `wheelctl` for device management, diagnostics, and profile operations
- **Safety System**: Fault detection, safe mode transitions, black box recording
- **Protocol Documentation**: Logitech, Fanatec, Thrustmaster, Simagic, Moza protocols documented

**Architecture**: Established via ADRs 0001-0006 (FFB Mode Matrix, IPC Transport, OWP-1 Protocol, RT Scheduling, Plugin Architecture, Safety Interlocks)

## Milestones

### Phase 1: Foundation ✅ Complete
- [x] Define Core Architecture (ADRs 0001-0006)
- [x] Implement Real-Time Engine loop (1kHz, ≤1000μs budget)
- [x] Implement Linux HID driver (hidraw/udev)
- [x] Implement Linux RT scheduling (SCHED_FIFO/rtkit)
- [x] Implement Windows HID driver (overlapped I/O, MMCSS)
- [x] Initial CLI tools (`wheelctl`) for device management
- [x] Background service (`wheeld`) with IPC

### Phase 2: Feature Completeness ✅ Complete
- [x] **Advanced Force Feedback**
    - [x] Curve-based FFB effects with pre-computed LUTs
    - [x] Profile hierarchy and inheritance (up to 5 levels)
    - [x] Zero-allocation curve application in RT path
- [x] **Game Telemetry Integration**
    - [x] iRacing adapter (shared memory)
    - [x] ACC adapter (UDP)
    - [x] Automobilista 2 adapter (shared memory)
    - [x] rFactor 2 adapter (plugin interface)
    - [x] Telemetry parsing within 1ms budget
- [x] **User Interface**
    - [x] Tauri-based desktop UI
    - [x] Device list and detail views
    - [x] Real-time telemetry display
    - [x] Profile management UI
- [x] **Histogram tracking** for latency metrics (HDRHistogram)

### Phase 3: Production Readiness (Current Focus)

**Goal**: Predictable behavior under load, safe failure modes, and a defensible supply-chain story.

**Rule**: Every change must either (a) tighten a gate or (b) reduce a failure mode.

#### Definition of Done

Phase 3 is complete when ALL of the following are true:

| Category | Criterion |
|----------|-----------|
| **Safety** | A missed tick / stalled host cannot produce uncontrolled torque |
| **Safety** | Safety state transitions are deterministic and logged with debug context |
| **Security** | Native plugin loading is secure-by-default; unsigned requires explicit opt-out |
| **Security** | Registry downloads and firmware artifacts are verified before use |
| **Release Quality** | RT timing is enforced by CI gates (not manual local runs) |
| **Release Quality** | Benchmark outputs are stored and comparable across runs |
| **Data Lifecycle** | Profiles migrate forward automatically with backup; process is idempotent |

---

#### 3.1 Safety Hardening

##### 3.1.1 Hardware Watchdog Integration (100ms timeout)

**Objective**: Guarantee torque collapses to safe output when the system loses control of the loop.

**Design Decisions**:
- Watchdog fed by RT loop only (never UI thread / async task)
- "Healthy" = RT tick within budget + successful device write + no disconnect/errors
- Expiry → immediate zero-torque + SafeMode/EStop transition + blackbox marker

**Implementation**:
- [ ] Define watchdog contract in `crates/engine`:
    - `Watchdog::arm(timeout)`, `Watchdog::feed(now, health_snapshot)`, `Watchdog::expire()`
    - Emit `SafetyEvent::WatchdogExpired { reason, last_healthy_tick, … }`
- [ ] Implement two-layer watchdog:
    - Layer A: Software watchdog (host side, deterministic)
    - Layer B: Device keepalive (vendor-specific periodic "alive" packet)
- [ ] Integrate into RT pipeline: feed **after** successful write (not before)
- [ ] Define backpressure behavior for pending writes (Windows overlapped / HID latency)
- [ ] Safety interlock state machine: `Normal → Warning → SafeMode → EmergencyStop`
    - Explicit manual reset path with optional cool-down to prevent flapping

**Acceptance Tests**:
- Unit: "no feed within 100ms ⇒ transitions to SafeMode and emits zero torque"
- Integration: "device disconnect mid-loop ⇒ safe torque within ≤1 tick"
- Regression: "stuck write pending ⇒ safe within ≤100ms, no deadlock"

**Artifacts**: `docs/safety/watchdog-behavior.md` documenting "what we do when the loop stalls"

---

##### 3.1.2 Fault Quarantine (`crates/engine/src/safety/fmea.rs`)

**Objective**: Stop repeated faults from becoming repeated damage. Faults "stick" in a controlled way.

**Implementation**:
- [ ] Build FMEA table as data structure (not scattered `if`s):
    - `FaultId`, severity, trigger condition, recommended action, reset conditions
    - Fault types: missed tick rate, repeated write failures, thermal warning, invalid effect commands, signature verification failures
- [ ] Implement `FaultManager`:
    - Inputs: `SafetyEvent`, `DeviceHealth`, `PerfStats`
    - Outputs: `TorquePolicy`, `QuarantineState`, `FaultLog`
- [ ] Add persistence: write to blackbox + small persistent store (survives restarts)
- [ ] Clean reset path: stable health for N seconds + explicit operator request

**Acceptance Tests**:
- "X repeated faults within window ⇒ quarantine"
- "Quarantine blocks risky operations (plugin load, firmware update)"
- "Quarantine clears only when reset conditions satisfied"

---

##### 3.1.3 Full Replay Validation (`crates/engine/src/diagnostic/replay.rs`)

**Objective**: Make blackbox logs executable to prove deterministic behavior and catch regressions.

**Implementation**:
- [ ] Define replay scope: input reports, effect commands, timing deltas, configuration state
- [ ] Ensure determinism: no wall-clock dependencies, no random seeds without capture, no allocation-driven differences
- [ ] Implement validators for invariants:
    - Torque range constraints, no NaN/inf
    - Expected safety state transitions
    - Timing budget compliance
- [ ] Add golden traces under `crates/engine/tests/fixtures/replay/`
- [ ] CI runs replay and checks invariants on every PR

**Acceptance Tests**:
- "Replay of trace X produces identical (or tolerance-bound) output metrics"
- "Replay detects regression and fails CI with actionable diff"

---

#### 3.2 Plugin Ecosystem

##### 3.2.1 Plugin Registry with Searchable Catalog

**Objective**: Registry as a trust boundary, not just a website.

**Implementation**:
- [ ] Specify plugin manifest format (`plugin.toml`):
    - Plugin ID, version, ABI version constraints, capabilities, minimum OpenRacing version
    - Supported OS/arch, hashes, signature metadata, download URLs
- [ ] Implement signed index (`index.json` signed with Ed25519)
- [ ] Implement registry client in `crates/plugins` or `crates/service`:
    - Fetch index, verify signature, cache with ETag + expiry
- [ ] Implement search: keyword + filters (OS/arch, capability, verified-only)
- [ ] Define "verified" semantics:
    - **Signed**: has a signature
    - **Verified**: signature validated against trust store

**Acceptance Tests**:
- "Tampered index fails verification"
- "Plugin with mismatched ABI is rejected"
- "Cache behaves (offline mode uses last-good index)"

---

##### 3.2.2 `wheelctl plugin install` Command

**Objective**: Make plugin install reproducible and observable.

**Implementation**:
- [ ] CLI UX: `search`, `install <id>@<version>`, `verify`, `list --installed`
- [ ] Install pipeline: download → hash verify → signature verify → unpack → register → activate
- [ ] Safe activation: don't hot-load into RT loop by default; require explicit "apply profile / restart service"
- [ ] Logging: machine-readable JSON output + human output with clear "verified vs unverified"

**Acceptance Tests**:
- Install happy path
- Install rollback if unpack fails mid-way
- Verify output correctly reports signed vs verified

---

##### 3.2.3 Embedded Signature Verification (PE/ELF sections)

**Objective**: Signature travels with artifact (no swappable sidecar `.sig` files).

**Implementation**:
- [ ] ELF: custom section `.openracing.sig`
- [ ] PE: certificate table or custom section
- [ ] Implement extraction + verification: parse binary, extract signature block + metadata
- [ ] Backwards compatibility: prefer embedded, fallback to sidecar during transition
- [ ] Update trust store: key rotation support, explicit "dev keys" vs "prod keys"

**Acceptance Tests**:
- "Embedded signature verifies"
- "Signature present but untrusted key ⇒ treated as unverified"
- "Require verification ⇒ fails load if unverified"

---

#### 3.3 Firmware Management

##### 3.3.1 Firmware Update System with Signature Verification

**Objective**: Only install trusted firmware with preflight checks to prevent bricks.

**Implementation**:
- [ ] Define firmware bundle format:
    - Metadata: device model(s), hw rev, min version, target version, checksum, signature
    - Payload: binary
    - Signatures: Ed25519 over manifest + payload hash
- [ ] Implement updater pipeline in service layer (`wheeld` owns coordination)
- [ ] Preflight checks: device ID match (VID/PID + hw rev), power stable, temperature safe
- [ ] Progress and failure handling: stream events, quarantine device on failure

**Acceptance Tests**:
- "Wrong device firmware rejected"
- "Tampered firmware rejected"
- "Update interruption produces controlled failure mode"

---

##### 3.3.2 Rollback Support on Update Failure

**Objective**: Use dual-bank firmware if supported; otherwise, cache and retry last-known-good.

**Implementation**:
- [ ] Capability detection: expose `supports_rollback` per device/vendor
- [ ] Rollback mechanisms: dual-bank → trigger bank swap; else → cache + reflash last-known-good
- [ ] Persist update state: attempted version, result, last-known-good

**Acceptance Tests**:
- Simulate failed update → confirm rollback path invoked (or explicitly unavailable)

---

##### 3.3.3 FFB Blocking During Firmware Updates

**Objective**: Guarantee no torque output while flashing.

**Implementation**:
- [ ] Engine-level torque policy: "FFB blocked" overrides all effects, forces torque=0
- [ ] UI/CLI workflow: require explicit acknowledgement ("wheel disabled during update")
- [ ] Post-update safety: require re-initialization handshake + explicit re-enable

**Acceptance Tests**:
- "Start update ⇒ torque becomes 0 immediately and stays 0"
- "After update ⇒ remains safe until operator re-enables"

---

#### 3.4 Performance Gates (CI)

##### 3.4.1 RT Timing Benchmarks in CI Pipeline

**Implementation**:
- [ ] Standardize benchmark output: single JSON schema (`BenchmarkResults`)
- [ ] CI job: run on consistent runners, record runner metadata, upload JSON artifact

**Acceptance Criteria**:
- CI always produces bench artifacts, even on failure

---

##### 3.4.2 Automated Threshold Enforcement (p99 jitter ≤0.25ms)

**Implementation**:
- [ ] Fix metric mapping: jitter percentiles vs jitter thresholds, processing vs processing thresholds
- [ ] "Fail loud" report: which metric failed, threshold, observed, source

**Acceptance Criteria**:
- Synthetic regression (introduce known sleep) fails CI reliably
- Normal runs don't false-fail due to metric misclassification

---

##### 3.4.3 JSON Benchmark Output for Historical Tracking

**Implementation**:
- [ ] Store artifacts per CI run
- [ ] Optional publishing: push JSON to `gh-pages` or attach to GitHub Releases
- [ ] Comparison tool: `scripts/compare_benchmarks.py old.json new.json`

**Acceptance Criteria**:
- Can diff two benchmark runs and get actionable summary

---

#### 3.5 Migration System

##### 3.5.1 Automatic Profile Schema Version Detection

**Implementation**:
- [ ] Make version explicit: every profile includes `schema_version`
- [ ] Fallback detection for legacy profiles (heuristic), immediately rewrite to explicit version

**Acceptance Criteria**:
- Loading legacy profiles works
- Loading new profiles is strict and predictable

---

##### 3.5.2 Profile Migration with Backup Creation

**Implementation**:
- [ ] Migration engine: `migrate(from_version, to_version)` → transformed document + report
- [ ] Backups: copy original to `*.bak.<timestamp>` or dedicated backups directory
- [ ] Idempotency: rerunning migration should not mutate already-migrated profiles
- [ ] Validation: schema validation after transformation

**Acceptance Criteria**:
- Migration leaves a backup
- Migration is deterministic
- Migration validates against schema after transformation

---

#### Execution Order (Minimal Cross-Stream Blocking)

Run in parallel where possible, but these items should land early:

1. **Performance gate correctness** (fix thresholds + CI artifact outputs)
   → Prevents RT regressions during other work
2. **Safety watchdog contract + RT integration**
   → Reliable dead-man switch while doing riskier work
3. **Secure-by-default plugin verification semantics**
   → Lock trust boundary before building registry UX
4. **Migration engine**
   → Low coupling, stabilizes config evolution
5. **Firmware update pipeline**
   → Last, depends on trust store + safety gating

---

#### PR Stack (Recommended)

| PR | Scope | Dependencies |
|----|-------|--------------|
| 1 | Perf gate fix + artifact publishing | None |
| 2 | Watchdog contract + RT feed + safe torque policy | None |
| 3 | Fault quarantine skeleton + persistence | PR 2 |
| 4 | Replay validator + first golden trace | PR 3 |
| 5 | Plugin verification defaults + "verified vs signed" semantics | None |
| 6 | Registry client + signed index | PR 5 |
| 7 | `wheelctl plugin install` + rollback-safe install | PR 6 |
| 8 | Embedded signature extraction/verification (ELF first, PE next) | PR 5 |
| 9 | Firmware bundle format + verify + preflight | PR 5 |
| 10 | Firmware apply + FFB blocking + rollback | PR 2, PR 9 |
| 11 | Migration detection + backup + idempotency + tests | None |

### Phase 4: Ecosystem & Polish
- [ ] **Device Ecosystem Tools**
    - [ ] `openracing-capture` utility (protocol sniffer/mapper)
    - [ ] Device protocol reverse engineering toolkit
- [ ] **macOS Support**
    - [ ] IOKit HID implementation
    - [ ] thread_policy_set RT scheduling
- [ ] **Installer & Packaging**
    - [x] Windows MSI installer (WiX)
    - [ ] Linux packages (deb, rpm, flatpak)
    - [ ] macOS DMG with notarization
- [ ] **Adaptive Scheduling**
    - [ ] Dynamic deadline adjustment based on system load
    - [ ] CPU governor integration

## Future Considerations

- **Cloud Integration**: Profile sharing and cloud backup via OpenRacing Hub
- **Mobile Companion App**: iOS/Android app for remote monitoring and quick adjustments
- **AI/ML Integration**: Adaptive FFB tuning based on driving style analysis
- **Wheel Manufacturer Partnerships**: Official SDK integrations
- **VR Integration**: Direct telemetry to VR headsets for haptic feedback

## Known Technical Debt

The following TODOs exist in the codebase and should be addressed before v1.0.0:

| Location | Issue |
|----------|-------|
| `crates/service/src/security/signature.rs:111` | Replace stub with actual Ed25519 verification |
| `crates/service/src/crypto/mod.rs:204-205` | Implement PE/ELF embedded signature checking |
| `crates/engine/src/scheduler.rs:181` | Implement adaptive scheduling |
| `crates/engine/src/diagnostic/blackbox.rs:152` | Index optimization for large recordings |
| `crates/service/src/integration_tests.rs` | Re-enable disabled integration tests |

## Release Schedule

| Version | Date | Status | Focus |
|---------|------|--------|-------|
| v0.1.0  | 2025-01-01 | ✅ Released | Core Engine & Linux Support |
| v0.2.0  | 2026-02-01 | ✅ Released | Windows Support & Tauri UI |
| v0.3.0  | 2026-02-01 | ✅ Released | WASM Plugins, Game Telemetry, Curve FFB |
| v0.4.0  | 2026-Q2 | Planned | Plugin Registry & Firmware Updates |
| v1.0.0  | 2026-10-15 | Planned | Production Release with Security Audit |

## Contributing

See [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) for development setup and contribution guidelines.

Significant architectural changes require an ADR. See [docs/adr/README.md](docs/adr/README.md) for the process.

---
*Last updated: 2026-02-01. This roadmap is subject to change based on community feedback and technical priorities.*
