# Racing Wheel Software — Requirements (v1.0)

**Document status:** Draft for review  
**Owners:** Product, Architecture  
**Change control:** PRs only; IDs are stable once merged

## Introduction

This document outlines the requirements for a comprehensive racing wheel software suite that provides a unified, cross-platform solution for managing racing wheel hardware. The software will replace vendor-specific applications with a single, driver-light system that handles device management, force feedback tuning, game integration, LED/haptics control, and safety features.

## A. Product Principles

- **Schema-first:** Contracts at seams (IPC, profiles, plugins) are the source of truth
- **Real-time protected:** The force loop is sacrosanct: no allocs, no locks, no IO
- **Driver-light:** Prefer HID/PID in user mode; minimize kernel footprint
- **Deterministic & portable:** Same inputs → same outputs across machines
- **Observable:** If it broke, we can prove it and replay it

## B. Out-of-Scope (v1)

Console licensing, virtual controllers for legacy titles, social cloud features.

## C. Constraints & Assumptions

- Windows 10+ and modern Linux; x86-64 baseline
- At least one HID/PID-compliant DD base
- No admin rights required after install (Linux udev rules, Windows user context)

## D. Non-Functional Requirements (NFR)

- **NFR-01 Latency:** Added E2E latency ≤2 ms (sample→device), p99; jitter ≤0.25 ms at 1 kHz
- **NFR-02 CPU/RAM:** Service <3% of one mid-range core with telemetry; <150 MB RSS
- **NFR-03 Reliability:** 48 h soak without missed tick; hot-plug tolerant
- **NFR-04 Security:** No unsigned firmware/plugins; IPC restricted by OS ACLs
- **NFR-05 Privacy:** No outbound analytics by default; explicit opt-in with config file
- **NFR-06 Accessibility:** High-contrast UI, scalable type, color-blind LED presets, audible cues
- **NFR-07 Localization:** Units (Nm/°/mph–kmh), decimal formatting; strings externalized

## E. User Journeys (acceptance flows)

- **UJ-01 First-run:** Detect devices → Safe Torque → choose game → "Configure" → launch sim → LEDs/dash active → profile saved
- **UJ-02 Per-car profile:** Start sim → auto-switch car profile in ≤500 ms → apply DOR/torque/filters → race
- **UJ-03 Fault & recovery:** Thermal fault triggers soft-stop ≤50 ms → audible cue → UI banner → auto-resume when safe
- **UJ-04 Debug:** Repro glitch → 2-min blackbox → attach Support ZIP → dev replays and bisects

## Requirements

### 1. Device Management (DM)

**DM-01 Enumeration**
- **User Story:** As a racing wheel user, I want the software to automatically detect and manage all my racing hardware so that I can configure everything from one application
- **AC:** Given a new device on USB, THEN list within 300 ms with type/capabilities

**DM-02 Disconnect detection**
- **AC:** Given a powered device unplugged, THEN mark "disconnected" within 100 ms and stop sending torque within 50 ms

**DM-03 Restore settings**
- **AC:** On service restart, THEN apply last settings within 100 ms per device

**DM-04 Calibration**
- **AC:** Provide center/DOR/pedal range calibration; persist to profile; results are deterministic (same raw → same calibrated output)

**DM-05 Firmware A/B**
- **AC:** Update is atomic; on failure, THEN auto-rollback; never bricks; progress and slots visible in UI/CLI

### 2. Force Feedback Engine (FFB)

**FFB-01 Tick discipline**
- **User Story:** As a racing enthusiast, I want precise and customizable force feedback that responds at racing-grade speeds so that I can feel realistic car dynamics
- **AC:** 1 kHz with p99 jitter ≤0.25 ms on reference hardware

**FFB-02 Hot path purity**
- **AC:** No heap allocations, syscalls, or locks after pipeline compile

**FFB-03 Filter set**
- **AC:** Reconstruction, friction, damper, inertia, notch/PEQ, slew-rate, curve, torque cap, bumpstop, hands-off (speed-adaptive where applicable)

**FFB-04 Timing budget**
- **AC:** Processing ≤50 µs median, ≤200 µs p99 per tick

**FFB-05 Anomaly handling**
- **AC:** NaN/overflow/runaway triggers soft-stop within 50 ms, logs event with pipeline snapshot

### 3. Game Integration (GI)

**GI-01 One-click telemetry**
- **User Story:** As a sim racer, I want the software to automatically configure my games for telemetry and LED support so that I don't have to manually edit configuration files
- **AC:** For supported sims, write and verify UDP/shared-mem config; rollback on failure

**GI-02 Auto profile switch**
- **AC:** On sim start (and car hint if available), switch active profile within 500 ms

**GI-03 Normalized telemetry**
- **AC:** Publish {ffb_scalar, rpm, speed, slip, gear, flags, car_id, track_id}; document per-sim coverage

**GI-04 Loss handling**
- **AC:** Detect telemetry loss ≤1 s; degrade LEDs/haptics gracefully; FFB unaffected

### 4. LEDs, Displays, Haptics (LDH)

**LDH-01 Latency**
- **User Story:** As a racing wheel owner, I want my wheel's LEDs and displays to show relevant racing information so that I can keep my eyes on the track
- **AC:** LEDs update ≤20 ms from telemetry input

**LDH-02 Stability**
- **AC:** RPM hysteresis prevents flicker at steady RPM (configurable % band)

**LDH-03 Patterns & widgets**
- **AC:** RPM bands, flags, pit-limiter blink, launch cue; dash widgets (gear/speed/delta/ERS/DRS when available)

**LDH-04 Rate independence**
- **AC:** Haptics 60–200 Hz independent of FFB thread; no starvation under load

**LDH-05 Accessibility**
- **AC:** Provide color-blind palettes for flags; audible alternatives for key events

### 5. Profiles & Overrides (PRF)

**PRF-01 Hierarchy**
- **User Story:** As a competitive sim racer, I want different wheel settings for different cars and tracks so that I can optimize my setup for each racing scenario
- **AC:** Deterministic merge: Global → Game → Car → Session

**PRF-02 Validation**
- **AC:** JSON Schema errors show line/column and rule violated; invalid profiles never apply

**PRF-03 Migration**
- **AC:** Migrate prior versions losslessly where defined; retain on-disk backups

**PRF-04 Signing**
- **AC:** Optional Ed25519 signature; UI shows trust state; unsigned still loadable by default

### 6. Safety & Torque Management (SAFE)

**SAFE-01 Safe torque boot**
- **User Story:** As a wheel user concerned about safety, I want the software to prevent dangerous torque levels and provide clear warnings so that I can race safely
- **AC:** Always start in Safe Torque; UI shows state

**SAFE-02 High-torque gate**
- **AC:** Requires per-session consent, no faults, hands-on within last 5 s, temp_c < 80. Persist until device power-cycle

**SAFE-03 Faults**
- **AC:** USB/encoder/thermal/overcurrent → ramp to 0 torque ≤50 ms; log & banner

**SAFE-04 Hands-off**
- **AC:** Detect in ≤5 s; audible cue; optional auto-reduce gain until hands-on

**SAFE-05 Kid/Demo**
- **AC:** Enforce torque/DOR caps at domain + engine; cannot be overridden by profile

### 7. Diagnostics (DIAG)

**DIAG-01 Blackbox**
- **User Story:** As a technical user, I want detailed diagnostic information and the ability to record/replay issues so that I can troubleshoot problems and optimize my setup
- **AC:** Record ≥5 min at 1 kHz with no drops on SSD; includes per-node outputs

**DIAG-02 Replay**
- **AC:** Replaying a capture reproduces outputs within floating-point tolerance

**DIAG-03 Support ZIP**
- **AC:** Two-minute capture <25 MB; includes profiles, health, system info, logs

**DIAG-04 Self-test**
- **AC:** Motor phase, encoder integrity, EMI, USB jitter, pedal hysteresis with pass/fail and suggested actions

### 8. Cross-Platform (XPLAT)

**XPLAT-01 I/O stacks**
- **User Story:** As a user on Windows or Linux, I want the software to work consistently across platforms without requiring kernel drivers so that I have a reliable, portable solution
- **AC:** Windows via hidapi/Win32 (overlapped IO); Linux via /dev/hidraw* + libudev

**XPLAT-02 IPC**
- **AC:** Named Pipes (Win) / UDS (Linux). All calls covered by Protobuf contracts

**XPLAT-03 Install & perms**
- **AC:** No kernel drivers for HID/PID; Linux udev rules; Windows runs as user service

**XPLAT-04 Updates**
- **AC:** Delta app updates; staged firmware with A/B rollback

### 9. Plugins & Extensibility (PLUG)

**PLUG-01 Isolation**
- **User Story:** As a developer or advanced user, I want to extend the software with custom telemetry sources and processing nodes so that I can add support for new games or create custom effects
- **AC:** Plugin crash does not crash service; restart with backoff

**PLUG-02 Contracts**
- **AC:** WASM (safe class) with declared capabilities; optional native fast-path in isolated helper using SPSC shared memory

**PLUG-03 Compatibility**
- **AC:** Feature negotiation within wheel.v1; minor versions backward-compatible

### 10. UI & CLI (UX)

**UX-01 Modes**
- **User Story:** As a wheel user, I want both a graphical interface for easy configuration and a command-line tool for automation so that I can use the software in different contexts
- **AC:** Basic (gain/DOR/3 feel sliders), Advanced (all filters), Engineer (graphs, Bode/step)

**UX-02 CLI parity**
- **AC:** All write ops available in wheelctl; errors return non-zero; --json prints machine-readable output

**UX-03 LED preview**
- **AC:** Live preview driven by sample telemetry

**UX-04 Firmware UI**
- **AC:** Shows A/B slots, progress, and rollback

**UX-05 Onboarding**
- **AC:** First-run wizard configures one sim end-to-end and validates with a test pattern (LEDs/haptics pulse)

## F. Performance Budgets

| Path | Budget |
|------|--------|
| Game sample → engine in | ≤0.3 ms |
| Engine processing | ≤0.2 ms median / ≤0.8 ms p99 |
| Engine → HID write | ≤0.3 ms p99 |
| **Total added** | **≤2.0 ms p99** |

## G. Observability & Metrics

**Counters:** missed ticks, HID write latency p50/p99, loop jitter p50/p99, thermal headroom, torque saturation %, telemetry packet loss

**Logs:** structured, per device/game; blackbox indices for binary captures

**Health events:** 10–20 Hz stream; "degraded" after 2 s silence

## H. Security & Privacy

- Signed firmware and (optionally) profiles/plugins; verify before apply
- IPC permissions via OS ACLs; no network exposure by default
- No outbound telemetry unless explicitly enabled in config