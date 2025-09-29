# Implementation Plan

## Overview

This implementation plan converts the racing wheel software design into milestone-gated development with critical path sequencing. Each milestone has explicit deliverables and Definition of Done (DoD) criteria that must be met before proceeding. The plan prioritizes the 1kHz force feedback core and safety systems, then scales out to game integration, UI, and extensibility features.

## Milestone Structure

**M0 — Foundation (Repo, Contracts, CI, Virtual Device)**
- Goal: Compile, generate contracts, and run RT loop against virtual wheel in CI
- DoD: wheeld boots; wheelctl lists virtual device; engine tick @1kHz with p99 jitter ≤0.25ms; schemas generate types

**M1 — FFB Mode Matrix + OWP-1 v0 + RT Scheduler**  
- Goal: End-to-end torque to device using defined protocol
- DoD: Capability negotiation selects PID/Raw-Torque/Telemetry-Synth; meets timing budgets; FMEA draft

**M2 — Safety Interlocks + Watchdogs**
- Goal: Cannot hurt anyone at 5-25 Nm
- DoD: Fault→torque→0 in ≤50ms; physical interlock + UI consent for high torque; kid/demo caps enforced

**M3 — Profiles & Deterministic Apply**
- Goal: Deterministic, portable settings with two-phase apply
- DoD: Same inputs→same profile hash; invalid JSON shows line/column; no partial apply possible

**M4 — Game Integration**
- Goal: One-click config for two sims; normalized telemetry
- DoD: "Configure" writes expected diffs; LED heartbeat proves telemetry; auto-switch ≤500ms

**M5 — LEDs/Dash/Haptics**
- Goal: Visuals/haptics that don't interfere with FFB
- DoD: LED latency ≤20ms; no flicker at steady RPM; FFB jitter unchanged with haptics on

**M6 — Diagnostics & Tracing**
- Goal: Prove and reproduce issues
- DoD: Record ≥5min@1kHz with no drops; replay matches outputs within tolerance; ETW/trace shows p99 jitter

**M7 — IPC/UI/CLI Parity**
- Goal: Configure everything from UI/CLI
- DoD: All write paths in CLI; --json outputs; health stream 10-20Hz; firmware A/B UI stubs

**M8 — Plugins**
- Goal: Community extends without breaking RT
- DoD: Crash isolation proven; budget violations eject plugin; engine continues; sample plugin ships

**M9 — Packaging & Hardening**
- Goal: Install, run, and update safely
- DoD: Clean install; signatures verified; rollback works; no admin required at runtime

## Task List

### M0 — Foundation

- [-] 0 ADR & CI scaffolding






- [x] 0.1 ADR & CI scaffolding




  - Create ADR template and decision records for FFB modes, IPC transport, OWP-1 protocol
  - Set up rustfmt/clippy/cargo-deny with per-OS CI runners
  - Add performance gate job that fails on p99 jitter >0.25ms or missed ticks >0.001%
  - Create license audit and third-party dependency tracking
  - _Requirements: NFR-01, NFR-02, Governance_

- [x] 0.2 Virtual Device (Loopback Base)








  - Implement software HID target or in-process mock for 1kHz OUT/IN flows
  - Create virtual device that exercises torque commands and telemetry reports
  - Build test harness for RT loop validation without physical hardware
  - Write integration tests for device enumeration and I/O with virtual base
  - _Requirements: DM-01, DM-02, Testability_


- [x] 1. Set up project structure and core domain types










  - Create Rust workspace with clean architecture crate structure (/schemas, /engine, /service, /cli, /ui)
  - Implement core domain value objects (TorqueNm, Degrees, DeviceId, ProfileId) with unit safety
  - Define domain entities (Device, Profile, BaseSettings, FilterConfig) as pure Rust structs
  - Write unit tests for value object constructors and validation rules
  - _Requirements: DM-01, PRF-02_



 [-] 2. Implement schema-first contracts and code generation

- [x] 2. Implement schema-first contracts and code generation





  - Define Protobuf schemas for IPC service contracts (WheelService, DeviceInfo, Profile messages)
  - Create JSON Schema for profile validation (wheel.profile/1) with migration support
  - Set up build.rs to generate Rust types from schemas during compilation
  - Add buf/schema compatibility checks to prevent breaking changes in CI
  - Write schema validation tests to ensure generated types match expected contracts
  - _Requirements: PLUG-03, PRF-02, PRF-03_

### M1 — FFB Mode Matrix + Protocol + RT Scheduler

- [x] 1.1 FFB Mode Matrix & Capability Negotiation


  - Implement three FFB modes: PID pass-through, raw-torque @1kHz, telemetry-synth fallback
  - Create device capability negotiation on connect (Feature Report 0x01)
  - Build mode selection policy based on device capabilities and game compatibility
  - Write unit tests for mode selection logic and capability parsing
  - _Requirements: FFB-01, FFB-02, GI-03_

- [x] 1.2 OWP-1 v0 Spec + HID Descriptors + Golden Tests


  - Define HID report descriptors for torque commands (0x20) and telemetry (0x21)
  - Implement Feature Reports for capabilities (0x01) and configuration (0x02)
  - Create endian-safe structs with sequence numbers and CRC validation
  - Write golden tests that parse/build reports with known good data
  - _Requirements: XPLAT-01, DM-01, DM-02, SAFE-03_

- [x] 1.3 AbsoluteScheduler & PLL with RT Setup


  - Create platform-specific absolute timer (clock_nanosleep/WaitableTimer)
  - Implement PLL for drift correction and busy-spin tail for final precision
  - Add OS-specific RT setup (MMCSS "Games"/SCHED_FIFO, mlockall, power throttling off)
  - Build jitter metrics collection and CI performance gates
  - Write timing validation tests with oscilloscope-level precision requirements
  - _Requirements: FFB-01, NFR-01_

- [x] 3. Create port traits and domain policies




  - Define HidPort, HidDevice, TelemetryPort, and ProfileRepo trait interfaces
  - Implement safety policies (can_enable_high_torque, validate_torque_limits) in domain layer
  - Create profile hierarchy resolution logic with deterministic merge behavior
  - Write property-based tests for profile merging and safety policy edge cases
  - _Requirements: SAFE-01, SAFE-02, PRF-01_

### M2 — Safety Interlocks + Watchdogs

- [x] 2.1 Safety Interlock (Physical + UI)





  - Implement wheel button combo challenge/acknowledgment protocol
  - Create tokened unlock system that persists until device power-cycle
  - Build UI consent flow with explicit high-torque warnings and disclaimers
  - Write tests for interlock state machine and token validation
  - _Requirements: SAFE-02_

- [x] 2.2 FMEA + Watchdogs + Fault Matrix








  - Define fault detection→action→post-mortem for USB stalls, encoder NaNs, thermal limits
  - Implement watchdog thresholds and quarantine policy for plugin overruns
  - Create soft-stop mechanism with torque ramping ≤50ms and audible alerts
  - Build blackbox fault markers and recovery procedures
  - Write fault injection tests for all defined failure modes
  - _Requirements: SAFE-03, SAFE-04, DIAG-01_

### M3 — Profiles & Deterministic Apply

- [x] 3.1 Zero-Alloc Pipeline Compile & Two-Phase Apply





  - Implement pipeline compilation from FilterConfig to function pointer vector
  - Create two-phase apply: compile off-thread → swap at tick boundary → ack to UI
  - Add CI assertion for no heap allocations on hot path after pipeline compile
  - Build deterministic merge engine with monotonic curve validation
  - Write tests for pipeline swap atomicity and deterministic profile resolution
  - _Requirements: FFB-02, PRF-01, PRF-02_

- [x] 3.2 Filter Node Library with Speed-Adaptive Variants











  - Implement filter nodes: reconstruction, friction, damper, inertia, notch/PEQ, slew-rate
  - Add curve mapping, torque cap, bumpstop model, and hands-off detector
  - Create speed-adaptive variants where applicable (friction/damper based on wheel_speed)
  - Write unit tests for each node with closed-form expectations and bounds checking
  - _Requirements: FFB-03, FFB-04, FFB-05_

- [x] 5. Implement HID adapters with OS-specific RT optimizations








  - Create Windows HID adapter: hidapi with overlapped I/O, avoid HidD_* in hot path
  - Add Windows RT setup: MMCSS category, process power throttling off, USB selective suspend guidance
  - Implement Linux HID adapter: /dev/hidraw* with libudev, non-blocking writes
  - Add Linux RT setup: SCHED_FIFO via rtkit, mlockall, udev rules for device permissions
  - Write integration tests with virtual device for enumeration and I/O validation
  - _Requirements: DM-01, DM-02, XPLAT-01, NFR-01_

- [x] 6. Create application services and use cases





  - Implement ProfileService for CRUD operations and hierarchy resolution
  - Build DeviceService for enumeration, calibration, and health monitoring
  - Create SafetyService with state machine for torque gate management
  - Write unit tests for service orchestration and error handling scenarios
  - _Requirements: DM-03, SAFE-03, PRF-01_

- [ ] 7. Build real-time engine with integrated safety (depends on M2)
  - Integrate filter pipeline with HID device writer in RT thread
  - Wire safety watchdogs and fault handlers from M2 into engine loop
  - Implement SPSC rings for game→engine and engine→blackbox communication
  - Write HIL tests with synthetic FFB data to validate timing and safety responses
  - _Requirements: FFB-05, SAFE-03, SAFE-04_

### M4 — Game Integration

- [ ] 4.1 Game Support Matrix & Golden Writers
  - Create YAML-based support matrix defining per-sim/version capabilities and config paths
  - Implement table-driven configuration writers that apply expected diffs
  - Build golden file tests that compare generated configs against known fixtures
  - Document telemetry field coverage and normalization mapping per sim
  - _Requirements: GI-01, GI-03_

- [ ] 8. Implement game telemetry adapters with rate limiting
  - Create iRacing telemetry adapter with shared memory interface and SDK integration
  - Build ACC telemetry adapter using UDP broadcast protocol with packet validation
  - Add rate limiter to protect RT thread from telemetry parsing overhead
  - Implement telemetry normalization to common NormalizedTelemetry struct
  - Create record-and-replay fixtures for CI testing without running actual games
  - Write adapter tests with recorded game data for validation
  - _Requirements: GI-03, GI-04_

- [ ] 9. Create game integration and auto-configuration
  - Implement one-click telemetry configuration writers using support matrix from 4.1
  - Build process detection and auto profile switching logic with ≤500ms response time
  - Create validation system to verify configuration file changes were applied correctly
  - Write end-to-end tests for configuration file generation and LED heartbeat validation
  - _Requirements: GI-01, GI-02_

### M5 — LEDs/Dash/Haptics

- [ ] 10. Build LED and haptics output system with rate independence
  - Implement LED mapping engine with rule-based pattern generation and RPM hysteresis
  - Create haptics routing for rim vibration and pedal feedback at 60-200Hz
  - Build dash widget system for gear, RPM, and flag displays with live preview
  - Add proof that FFB jitter remains unchanged when LEDs/haptics are active
  - Write tests for LED pattern generation, timing validation, and rate independence
  - _Requirements: LDH-01, LDH-02, LDH-03, LDH-04_

### M6 — Diagnostics & Tracing

- [x] 6.1 ETW/Tracepoints for Real-Time Observability


  - Implement Windows ETW provider with TickStart/End, HidWrite, DeadlineMiss events
  - Add Linux tracepoints or Perfetto integration for RT loop observability
  - Create structured logging with device/game context for non-RT events
  - Build metrics collection for jitter, latency, and missed tick counters
  - _Requirements: DIAG-04, NFR-01_

- [ ] 13. Build diagnostic and blackbox recording system
  - Implement 1kHz blackbox recorder with .wbb v1 format (magic WBB1, CRC32C, index every 100ms)
  - Create three streams: A (1kHz frames + per-node outputs), B (60Hz telemetry), C (health/fault events)
  - Build replay system with deterministic seed to reproduce outputs within floating-point tolerance
  - Create support bundle generation (<25MB for 2-minute capture) with logs, profiles, and system info
  - Write tests for blackbox recording, compression, and replay accuracy
  - _Requirements: DIAG-01, DIAG-02, DIAG-03_

### M7 — IPC/UI/CLI Parity

- [ ] 11. Implement IPC server and client communication
  - Create gRPC server implementation using generated Protobuf contracts
  - Build platform-specific IPC transport (Named Pipes on Windows, UDS on Linux) with ACL restrictions
  - Add feature negotiation RPC for backward compatibility within wheel.v1 namespace
  - Implement streaming health events (10-20Hz) and device enumeration endpoints
  - Write IPC integration tests with mock clients for all service methods
  - _Requirements: UX-02, XPLAT-02_

- [ ] 12. Create profile persistence and validation system
  - Implement file-based profile repository with JSON Schema validation and line/column error reporting
  - Build profile migration system for schema version upgrades with lossless conversion
  - Create profile signing and verification using Ed25519 signatures with trust state UI
  - Add deterministic profile merge with Global→Game→Car→Session hierarchy
  - Write tests for profile serialization, validation, migration, and merge scenarios
  - _Requirements: PRF-01, PRF-02, PRF-03, PRF-04_

- [ ] 15. Create CLI application (wheelctl) with full parity
  - Implement command-line interface with device, profile, and diagnostic commands
  - Build JSON output formatting (--json flag) for machine-readable responses
  - Ensure all write operations available in CLI match UI capabilities
  - Create bash/zsh completion scripts for CLI commands
  - Write CLI integration tests covering all major command workflows with error code validation
  - _Requirements: UX-02_

### M8 — Plugins

- [ ] 14. Implement plugin system with two classes
  - Create WASM plugin host (safe class) with capability-based sandboxing for 60-200Hz operations
  - Build native plugin helper process (fast class) with SPSC shared memory and watchdog for RT nodes
  - Implement plugin manifest validation, loading system, and crash isolation
  - Add budget violation detection that ejects plugins while keeping engine running
  - Create quarantine policy for repeatedly failing plugins
  - Write plugin SDK tests with sample telemetry processing and DSP filter plugins
  - _Requirements: PLUG-01, PLUG-02_

### M9 — Packaging & Hardening

- [ ] 9.1 Packaging & Hardening with Security
  - Create MSI installer (Windows) and systemd user unit templates (Linux)
  - Add udev rules for device permissions and rtkit/MMCSS setup documentation
  - Implement signed app/firmware/plugin verification with Ed25519
  - Build delta update system with rollback capability and health probes
  - Add power management guidance (USB selective suspend, CPU throttling)
  - Create reproducible builds with third-party license audit
  - _Requirements: XPLAT-03, XPLAT-04, Security_

- [ ] 16. Build service daemon and process management
  - Implement wheeld service with proper signal handling and graceful shutdown
  - Create platform-specific service installation with no admin rights required at runtime
  - Build service health monitoring and automatic restart capabilities
  - Add IPC ACL restrictions (pipe ACLs on Windows, socket permissions on Linux)
  - Write service lifecycle tests for startup, shutdown, and crash recovery
  - _Requirements: XPLAT-03_

- [ ] 17. Implement firmware update system (if firmware control available)
  - Create A/B partition firmware update mechanism with atomic swaps
  - Build firmware validation and rollback logic for failed updates with health checks
  - Implement progress reporting and error handling for update operations
  - Add staged rollout capability with automatic rollback on error threshold
  - Write firmware update tests with mock devices and failure injection
  - _Requirements: DM-05_

### Integration & Validation

- [ ] 18. Create comprehensive integration test suite with performance gates
  - Build end-to-end test scenarios covering complete user workflows (UJ-01 through UJ-04)
  - Implement CI performance gates: p99 jitter ≤0.25ms, HID write latency p99 ≤300μs
  - Create soak tests for 48-hour continuous operation validation with no missed ticks
  - Add acceptance tests mapping to specific requirement IDs with automated DoD verification
  - Build hot-plug stress testing with rapid connect/disconnect cycles
  - _Requirements: FFB-01, SAFE-03, GI-01, NFR-01, NFR-03_

- [ ] 19. Add observability and metrics collection
  - Implement structured logging with device and game context using tracing crate
  - Create performance metrics collection (latency, jitter, CPU usage) with Prometheus export
  - Build health event streaming for real-time monitoring at 10-20Hz
  - Add counters for missed ticks, torque saturation %, telemetry packet loss
  - Write metrics validation tests and alerting threshold verification
  - _Requirements: DIAG-04, NFR-02_

- [ ] 20. Integrate and validate complete system
  - Wire all components together in main service application with graceful degradation
  - Implement system-level configuration management and validation
  - Create anti-cheat compatibility documentation (no DLL injection, documented telemetry methods)
  - Add feature flags for development (--mode=pid|raw|synth, --rt=off for CI)
  - Write full system integration tests with virtual hardware simulation
  - _Requirements: All requirements integration_

## Critical Path Dependencies

```
M0 (Foundation) → M1 (FFB Core) → M2 (Safety) → M3 (Profiles)
                                                      ↓
M4 (Game Integration) ← M5 (LEDs) ← M6 (Diagnostics) ← M7 (IPC/UI)
                                                      ↓
                                              M8 (Plugins) → M9 (Packaging)
```

## Definition of Done (DoD) Criteria

**FFB Core DoD:** p99 jitter ≤0.25ms over 10min; no heap allocs post-compile; HID write p99 ≤300μs; anomaly→soft-stop ≤50ms

**Safety DoD:** Physical interlock required; all listed faults trigger 50ms ramp-down; blackbox contains 2s pre-fault history

**Game Integration DoD:** "Configure" produces exact expected diffs for two sims; LED heartbeat confirmed; auto-profile switch ≤500ms

**Profiles DoD:** Deterministic merge hash; monotonic curve validation; two-phase apply (no partial state)

**Plugins DoD:** Crash isolation verified; overrun watchdog trips; helper quarantines plugin and service continues