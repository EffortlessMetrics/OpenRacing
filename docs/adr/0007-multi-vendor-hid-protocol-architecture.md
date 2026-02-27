# ADR-0007: Multi-Vendor HID Protocol Architecture (SRP Microcrates)

**Status:** Accepted  
**Date:** 2025-01-01  
**Authors:** Architecture Team, Hardware Team  
**Reviewers:** Engineering Team, Community Team  
**Related ADRs:** ADR-0001 (FF Mode Matrix), ADR-0003 (OWP-1 Protocol), ADR-0004 (RT Scheduling), ADR-0005 (Plugin Architecture)

## Context

OpenRacing targets a broad ecosystem of force feedback wheel bases from competing vendors, each of which uses a proprietary HID protocol for device handshake, input parsing, and FFB output encoding. Without a deliberate architecture, vendor-specific logic tends to accumulate inside the engine crate, creating three recurring problems:

1. **Coupling**: vendor quirks bleed into RT hot paths, making allocation-free guarantees hard to uphold.
2. **Testability**: protocol logic can only be verified with real hardware or cumbersome full-stack mocks.
3. **Scalability**: adding a new vendor requires touching the engine, risking regressions in unrelated paths.

The following vendors are in scope for the initial implementation:
MOZA, Fanatec, Simagic, VRS, Heusinkveld, Asetek, Simucube, Thrustmaster, Logitech, Cammus, OpenFFBoard, FFBeast, SimpleMotion V2.

Each vendor device is identified by a USB VID+PID pair reported over HID. Some vendors ship multiple distinct product lines with different encoders, torque ratings, and report layouts that must be handled per-PID.

Some game titles do not support native protocol configuration writes (e.g., they hard-code PID effects). For those titles a bridge contract file (JSON) is used to describe the mapping between game-side FFB parameters and the vendor's native output encoding. These files require manual user placement for unsupported titles.

## Decision

### SRP Microcrate per Vendor

Each vendor is assigned an independent Rust crate named `racing-wheel-hid-{vendor}-protocol` (e.g. `racing-wheel-hid-moza-protocol`). The crate boundary enforces the Single Responsibility Principle: the crate knows everything about one vendor's wire protocol and nothing about the rest of the system.

Each crate exports a fixed public surface:

```
racing-wheel-hid-{vendor}-protocol/
├── src/
│   ├── ids.rs          — VID constant + PID constants for every known product
│   ├── input.rs        — parse_input_report(&[u8]) -> Option<InputState>
│   ├── output.rs       — encode_ffb_output(torque: f32) -> [u8; N]
│   └── lib.rs          — re-exports + VendorProtocol impl
```

All public functions must be **pure** (no I/O, no global state) and **allocation-free** (no `Vec`, `String`, or `Box` at call sites in the RT path). Input parsing returns `Option<InputState>` rather than `Result` to allow zero-cost discard of unrecognised reports without heap allocation.

### `VendorProtocol` Trait

The engine crate defines a trait that each microcrate implements:

```rust
/// Implemented by each vendor microcrate.
/// All methods are pure and allocation-free.
pub trait VendorProtocol: Send + Sync {
    /// USB Vendor ID for this vendor.
    fn vendor_id(&self) -> u16;
    /// True if the given PID is handled by this implementation.
    fn matches_pid(&self, pid: u16) -> bool;
    /// Parse a raw HID input report into normalised input state.
    fn parse_input(&self, report: &[u8]) -> Option<InputState>;
    /// Encode a normalised torque value [-1.0, 1.0] into a HID output report.
    fn encode_ffb(&self, torque: f32, buf: &mut [u8; 64]);
    /// Device-capability metadata (max torque, encoder CPR, etc.).
    fn ffb_config(&self) -> FfbConfig;
}
```

The engine stores a `&'static dyn VendorProtocol` reference resolved once at hot-plug time; subsequent RT calls are single indirect dispatch with no allocation.

### Device Discovery via VID+PID Matching

At hot-plug time the device manager iterates a static registry of `VendorProtocol` implementations and calls `matches_pid` for each until a match is found. The registry is a fixed-size array allocated at program start — no heap growth at runtime. Unknown VID+PID pairs are silently skipped; an `INFO`-level log entry is emitted and the device is ignored.

### Bridge Contract Files

For games that do not support native protocol writes, a JSON bridge contract file describes parameter mappings:

```json
{
  "schema": "wheel.bridge/1",
  "game_id": "example_title",
  "vendor": "moza",
  "mode": "pid_passthrough",
  "gain_curve": [0.0, 0.25, 0.5, 0.75, 1.0]
}
```

Bridge contracts are user-managed files placed in the application config directory. The service loads them at startup; missing files cause a non-fatal warning, not an error.

## Rationale

- **Isolation**: protocol logic confined to its crate cannot corrupt RT state in other crates.
- **Allocation-free RT path**: pure functions with stack-only return types satisfy the no-heap rule in RT code (NFR-02, PRF-01).
- **Hardware-independent testing**: pure functions can be tested with byte fixtures without USB hardware.
- **Additive extensibility**: a new vendor crate can be registered without modifying engine internals (DM-03).
- **Safe discovery**: VID+PID matching at hot-plug time is a one-time non-RT operation; the RT path only uses the already-resolved `&'static dyn VendorProtocol` pointer.

## Consequences

### Positive

- Protocol logic is testable in isolation without hardware — byte-level golden tests cover encode/decode round-trips.
- RT hot paths remain allocation-free; `encode_ffb` writes into a caller-provided stack buffer.
- New vendors can be added by creating a new crate and registering it in the static registry — no engine changes required.
- Vendor crates can be versioned and audited independently.
- SRP boundary makes it straightforward to stub an entire vendor for integration testing.

### Negative

- Protocol correctness for new vendors requires hardware or community-sourced HID captures; there is no automated way to validate against real devices in CI.
- Bridge contract files for unsupported game titles require manual user setup; there is no auto-detection mechanism for these titles.
- The static registry must be updated (a one-line addition) whenever a new vendor crate is introduced, creating a minor coupling point.

### Neutral

- Each vendor crate adds a small build-time dependency; workspace-level dependency deduplication mitigates this.
- The `VendorProtocol` trait ABI is internal (no plugin boundary); breaking changes require updating all vendor crates simultaneously.

## Alternatives Considered

1. **Single monolithic `hid-protocols` crate**: Rejected because it concentrates all vendor-specific churn in one place and makes it impossible to compile the engine without every vendor's dependency tree.
2. **Runtime plugin (WASM) per vendor**: Rejected because WASM sandboxing introduces overhead incompatible with the 1kHz RT budget (ADR-0005 reserves WASM for ≤200Hz safe plugins).
3. **Enum dispatch over all vendors**: Rejected because each new vendor requires a match arm change in the engine crate, violating the Open/Closed Principle and risking RT regressions.
4. **OS HID abstraction layer only**: Rejected because OS-level HID abstractions do not expose the vendor-specific feature report sequences needed for device initialisation and raw torque output.

## Implementation Notes

**Vendor crate checklist** (per new vendor):
1. Create `crates/hid-{vendor}-protocol/` with the standard module layout.
2. Populate `ids.rs` with the vendor's VID and all known PIDs.
3. Implement `parse_input` using only stack-allocated types.
4. Implement `encode_ffb` writing into the caller-provided `&mut [u8; 64]` buffer.
5. Implement `VendorProtocol` and add the static instance to the engine registry.
6. Add byte-level golden tests covering at minimum: known-good input reports, full-scale / zero / negative torque encoding, and out-of-range clamp behaviour.

**Torque clamping** (engine responsibility): The engine layer clamps the `torque` argument to `[-1.0, 1.0]` before calling `encode_ffb`. Vendor crates may assert this range in debug builds but must not rely on it for safety.

**Simucube / SimpleMotion V2**: These vendors communicate over a custom USB bulk-transfer protocol rather than standard HID reports. Their `encode_ffb` implementations wrap the SimpleMotion V2 frame format; the `VendorProtocol` trait surface remains identical to HID vendors.

## Compliance & Verification

- Each vendor crate must include unit tests with golden byte vectors for both input parsing and FFB output encoding.
- Integration tests in `racing-wheel-integration-tests` verify end-to-end VID+PID discovery and protocol dispatch using virtual device stubs.
- CI must pass `cargo test --all-features --workspace` with all vendor crates included.
- BDD feature scenarios in `crates/integration-tests/features/device_vendors.feature` provide living documentation of expected discovery and safety behaviour.

## References

- Requirements: DM-01, DM-02, DM-03, DM-04, DM-05, FFB-01, FFB-02, NFR-02, NFR-03, PRF-01, PRF-02, XPLAT-01, XPLAT-02
- Design Document: HID Protocol Architecture
- Related ADRs: ADR-0001 (FF Mode Matrix), ADR-0003 (OWP-1 Protocol), ADR-0004 (RT Scheduling)
- OWP-1 Protocol Specification: `docs/adr/0003-owp1-protocol.md`
- Plugin Architecture (WASM rate limits): `docs/adr/0005-plugin-architecture.md`
