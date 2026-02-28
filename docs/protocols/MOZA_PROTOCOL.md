# Moza Racing Protocol

**Status:** Beta  
**Vendor ID:** `0x346E`  
**Maintainer:** OpenRacing Team

## Overview

Moza Racing hardware generally follows a unified HID-over-USB protocol. However, the ecosystem is split into two distinct topology classes:

1. **Direct USB Devices:** High-end peripherals (CRP pedals, HBP handbrake, HGP shifter) and some wheelbases often expose themselves as distinct USB HID devices.
2. **Aggregated Ecosystem (R-Series Bundles):** The SR-P Lite pedals and some shifters connect directly to the wheelbase via RJ11/RJ45. These devices **do not** appear on the USB bus. Their data is aggregated into the wheelbase's primary input report.

## Supported Hardware

| Device | Type | Connection | PID (Approx) | Status |
| :--- | :--- | :--- | :--- | :--- |
| **R3 / R5 / R9 / R12** | Wheelbase | USB | `0x0005`, `0x0002`, etc. | **Supported** |
| **SR-P Lite** | Pedals | **Wheelbase Port** | N/A (Embedded) | **Supported** |
| **SR-P (Standard)** | Pedals | USB | `0x0003` (Typical) | *Partial* |
| **CRP Pedals** | Pedals | USB | `0x0001` (Typical) | *Partial* |
| **HBP Handbrake** | Handbrake | USB | `0x0022` (standalone) | **Supported (Best-Effort)** |

## Moza KS support model (wheel + controls)

The KS is **not** treated as a normal wheel peripheral. OpenRacing uses a topology-first model:

1. **Wheelbase topology (primary):** host sees only the wheelbase USB device; KS controls are aggregated into wheelbase input reports.
2. **Universal Hub topology (secondary):** host may see the hub as a USB HID with exposed wheel and accessory ports; behavior is firmware-dependent and must be capture-gated.

### Recommended canonical handling

- **Never hard-code KS physical layout in runtime code paths.**
- **Derive control interpretation from capture-derived maps** (`device_map.json`) and runtime profile metadata.
- **Gate KS support on explicit transport/signature identity** (VID/PID + interface + descriptor hash + report IDs).
- Use mode-aware normalization for:
  - dual clutches (combined axis / independent axis / button modes),
  - rotaries (button deltas vs knob values),
  - joysticks (button mode vs D-pad mode).
- Treat all mode decisions as potentially changing with firmware and Pit House profile settings unless validated in artifact checks.

### Open items from current implementation

**Implemented:**
- KS rim detection via `funky` byte (`rim_ids::KS = 0x05`) in wheelbase input report.
- Button and hat parsing for KS rim via `default_wheelbase_ks_map()` (uses `input_report::BUTTONS_START` and `input_report::HAT_START`).
- Rotary encoder bytes at `ROTARY_START` / `ROTARY_START + 1` are preserved in `ks_snapshot.encoders[0..1]` for both KS and non-KS rims.
- Fallback snapshot for non-KS rims provides common controls (buttons, hat, rotary[0..1]).
- Clutch, joystick mode, and encoder[2..7] remain `None`/`Unknown` pending capture-validated offsets.

**Pending capture data:**

- Confirm exact report IDs / descriptor signatures for:
  - KS over wheelbase path (including whether rim ID bytes expose “KS attached”),
  - KS via Universal Hub USB mode.
- Reconcile Universal Hub manual wording variance:
  - product page suggests wheel support,
  - manual screenshots can show `Wheel (currently FSR only)`.
- Validate clutch/button mode mapping from capture vectors before enabling mode-specific safety assumptions.
- Do not claim KS support for Universal Hub without a validated report-signature and interface mapping gate.

### KS capture-validated checklist (before shipping production support)

- [ ] Wheelbase path report descriptor captured for at least one KS-verified firmware.
- [ ] Universal Hub path descriptor captured (if supported by product revision) with report IDs and report lengths.
- [ ] Baseline + transition traces for:
  - clutch combined mode,
  - clutch independent axis mode,
  - clutch button mode.
- [ ] Baseline + transition traces for rotary button mode and rotary knob mode.
- [ ] Baseline + transition traces for joystick button mode and D-pad mode.
- [ ] Topology diagnostics for missing controls when pairing is incomplete (stale reports, no deltas, no joystick updates).
- [ ] Golden normalized snapshots committed to `device_map.json`/`capture_notes.json`.

## Discovery & Initialization

### The "Magic" Handshake

Moza wheelbases start in a restricted mode. To enable high-frequency force feedback and full input reporting (including aggregated pedals), the host must send a specific feature report sequence.

**Safe-default sequence (2 reports — always sent in raw-hidraw mode):**

1. **Start Reporting:** Feature Report `0x03` → `[0x03, 0x00, 0x00, 0x00]`
2. **Set FFB Mode:** Feature Report `0x11` → `[0x11, <mode>, 0x00, 0x00]`

**Optional: Enable High Torque (requires explicit arming):**

* **Pre-step (optional): Enable High Torque / Motor:** Feature Report `0x02` → `[0x02, 0x00, 0x00, 0x00]`
  — sent *before* step 1, only when `OPENRACING_MOZA_HIGH_TORQUE=1` and the device signature is trusted.
  See [Signature Fingerprinting and Safe Arming Policy](#signature-fingerprinting-and-safe-arming-policy).

`<mode>` is currently configured in OpenRacing via `OPENRACING_MOZA_FFB_MODE`:

- `standard` or `0` (default): PID/PIDFF mode (`0x00`)
- `direct` or `raw` or `2`: Direct torque mode (`0x02`) — requires trusted signature
- `off`: Disabled (`0xFF`)

On Linux, the runtime transport is also controlled by `OPENRACING_MOZA_TRANSPORT_MODE`:

- `raw-hidraw` or `raw` (default): OpenRacing sends feature reports and direct torque output through `hidraw`.
- `kernel-pidff` or `kernel`: OpenRacing only runs kernel-PIDFF-compatible mode. Vendor handshake and raw writes are skipped so the kernel driver can own FFB control.

*Note: Without the High Torque step, the wheelbase may not report pedal axis changes in some firmware versions.*

## Input Protocols

### Aggregated Input Report (SR-P Lite)

When SR-P Lite pedals are connected to the wheelbase, their axis data is mapped to fixed offsets within the wheelbase's primary input report.

- **Report ID:** `0x01` (Standard)
- **Update Rate:** 1000 Hz (Interval 1ms)
- **Endianness:** Little Endian

| Offset (Byte) | Field | Type | Range | Notes |
| :--- | :--- | :--- | :--- | :--- |
| 0 | Report ID | `u8` | `0x01` | |
| 1-2 | Steering Angle | `u16` | 0-65535 | Center ~32767 |
| **3-4** | **Throttle** | `u16` | **0-65535** | 0 = Released |
| **5-6** | **Brake** | `u16` | **0-65535** | 0 = Released |
| 7-8 | Clutch | `u16` | 0-65535 | Optional (depends on kit) |
| 9-10 | Handbrake | `u16` | 0-65535 | Optional (if connected to base) |

**Normalization:**  
OpenRacing normalizes all axes to `0.0` (released) to `1.0` (fully pressed).  
`Value_Float = Value_Raw / 65535.0`

**Implementation note (wheelbase reports):** Aggregated wheelbase input parsing is isolated in a
single-responsibility microcrate (`crates/moza-wheelbase-report`, crate name
`racing-wheel-moza-wheelbase-report`) and consumed by
`racing-wheel-hid-moza-protocol::MozaProtocol::parse_input_state` / `parse_aggregated_pedal_axes`.

**Implementation note:** Standalone SR-P USB pedal parsing is isolated in a
single-responsibility microcrate (`crates/srp`, crate name `racing-wheel-srp`) and consumed by
`racing-wheel-hid-moza-protocol::MozaProtocol::parse_input_state` for
`PID=0x0003`.

### HBP handbrake topology classes

Moza handbrake input appears in two supported runtime paths:

1. **Direct USB HBP**
   - HID device is present as `VID=0x346E`, `PID=0x0022`.
   - No wheelbase handshake required.
   - Parse path uses a dedicated HBP parser microcrate (`crates/hbp`, crate name `racing-wheel-hbp`) consumed by
     `racing-wheel-hid-moza-protocol::MozaProtocol::parse_input_state` for `PID=0x0022`.

2. **Wheelbase-embedded HBP**
   - HBP is attached to a wheelbase port and exposed through the wheelbase report.
   - Requires normal wheelbase initialization (`0x02`, `0x03`, `0x11`) to start reporting.
   - Axis is expected in the wheelbase report handbrake field (`report_id=0x01`, offset 9..10), when present.

Only topology-level behavior and timing has been implemented in-engine; exact payload layouts and optional button semantics are marked **capture-validated only** until USB traces are added in the capture utility.

### HBP implementation status

Three report layouts are supported via best-effort inference in `racing-wheel-hbp::parse_hbp_usb_report_best_effort`:

1. `[report_id, axis_lo, axis_hi, button_byte]` — report-ID-prefixed (takes priority when first byte ≠ 0).
2. `[axis_lo, axis_hi]` — raw two-byte (no button information).
3. `[axis_lo, axis_hi, button_byte]` — raw with button.

When a `button_byte` is present it is placed in `MozaInputState::buttons[0]`. The axis value is always in `MozaInputState::handbrake_u16` (raw `u16`, full range 0–65535).

Digital (on/off) mode: games wishing to treat the HBP as a binary switch should check `buttons[0] != 0` rather than the axis value; the axis value reflects the physical position regardless of Pit House mode setting.

## HBP Capture and Validation Notes

- **Unknowns to capture before finalizing production support**
  - Whether HBP reports always include a byte suitable for button-mode inference.
  - Whether HBP USB emits explicit report IDs in all firmware modes.
  - Confirmed axis endianness and calibration defaults for both topologies.
- **Capture artifacts required before firmware-specific finalization**
  - `device_map.json`: identity entry for `0x346E:0x0022`.
  - Raw baseline + sweep traces (USB mode and wheelbase-embedded mode).
  - Optional button-mode trace set if community-reported button mode is in use.

### SR-P Lite Specifics

- **No USB Identity:** The OS sees only the wheelbase.
- **Calibration:** The wheelbase firmware usually reports raw 16-bit values from the Hall sensors (approx 0.9V to 1.9V range scaled to 16-bit). OpenRacing may need to apply user-defined min/max calibration on top of these raw values if the Pit House calibration is not burned into the firmware output.
- **Conflict:** Connecting a USB pedal set (SR-P) *and* SR-P Lite simultaneously may cause the base to mute the Lite channels.

## Force Feedback (FFB)

Moza wheelbases support standard HID PID (Physical Interface Device) force feedback.

- **Usage Page:** `0x01` (Generic Desktop) or Vendor Specific
- **Directions:** standard two-axis force vectoring.
- **Safety:** The `Enable High Torque` handshake must be repeated if the device loses power or resets.

## Known Issues / Quirks

1. **"Aggregates Peripherals":** This property is critical. V2 hardware revisions might shift the byte offsets. The current implementation assumes the standard `0x01` report structure defined above.
2. **Linux Permissions:** The device must be accessed via `hidraw`. A udev rule is required to grant permission (VID `0x346E`).

---

## Signature Fingerprinting and Safe Arming Policy

### Overview

OpenRacing uses hardware signature material from `HidDeviceInfo` to gate high-risk device actions (high torque, direct FFB mode) behind a known-good device allowlist.

This design mirrors the [`openracing-capture-ids`](../../crates/openracing-capture-ids/) tooling: capture identifies a device by its descriptor fingerprint, and the runtime policy allows high-risk paths only when that fingerprint is explicitly trusted.

### Signature fields in `HidDeviceInfo`

| Field | Source | Linux | Windows |
|---|---|---|---|
| `interface_number` | hidapi / sysfs | sysfs symlink (`.N` suffix) | hidapi `interface_number()` |
| `usage_page` | report descriptor / hidapi | parsed from first `0x05/0x06` tag | hidapi `usage_page()` |
| `usage` | report descriptor / hidapi | parsed from first `0x09/0x0A` tag | hidapi `usage()` |
| `report_descriptor_len` | raw descriptor bytes | `descriptor.len()` | not available (always `None`) |
| `report_descriptor_crc32` | CRC32 of raw descriptor | `crc32fast::Hasher` over descriptor | not available (always `None`) |

Windows does not expose raw report descriptor bytes via hidapi. The `report_descriptor_crc32` is therefore always `None` on Windows until a separate Windows fingerprinting path is implemented.

### Arming policy

**High torque** (`report_ids::HIGH_TORQUE`, feature report `0x02`):
- Never sent by default.
- Opt in: `OPENRACING_MOZA_HIGH_TORQUE=1` **and** a trusted signature (see below).
- If `OPENRACING_MOZA_HIGH_TORQUE=1` is set but the signature is not trusted, a `warn!` log entry is emitted and high torque is skipped.

**Direct FFB mode** (`OPENRACING_MOZA_FFB_MODE=direct`):
- If the device signature is not trusted, the runtime downgrades to `standard` mode and emits a warning.

### Descriptor CRC32 allowlist

On Linux, a device is trusted when its `report_descriptor_crc32` appears in:

```text
OPENRACING_MOZA_DESCRIPTOR_CRC32_ALLOWLIST=0xDEADBEEF,0x12345678
```

Values are comma- or semicolon-separated hex (`0x` prefix optional) or decimal integers.

**Populate the allowlist via the capture tool:**

```bash
# Step 1: capture signature material from a connected Moza wheelbase
cargo run -p openracing-capture-ids -- --vid 0x346E > moza_capture.json

# Step 2: inspect the crc32 field for the wheelbase interface
# (look for the entry with usage_page=0x01 / usage=0x04 or vendor usage page)

# Step 3: export and run
export OPENRACING_MOZA_DESCRIPTOR_CRC32_ALLOWLIST=0xDEADBEEF
export OPENRACING_MOZA_HIGH_TORQUE=1
```

### Escape hatch (developers only)

```bash
# Bypasses allowlist check entirely. Use for bring-up of unknown hardware.
export OPENRACING_MOZA_ALLOW_UNKNOWN_SIGNATURE=1
export OPENRACING_MOZA_HIGH_TORQUE=1
```

**Warning:** the escape hatch sends high torque to any Moza wheelbase regardless of identity. Only use on hardware you own in a safe environment.

### Windows parity note

Windows currently gets `report_descriptor_crc32=None`, which means:
- `signature_is_trusted(None)` returns `false` by default.
- High torque and direct mode require `OPENRACING_MOZA_ALLOW_UNKNOWN_SIGNATURE=1` on Windows until a Windows descriptor fingerprinting path is implemented.
- This is a deliberate safe-default: do not assume Windows is silently working in an untrusted state.

### Environment variable reference

| Variable | Values | Effect |
|---|---|---|
| `OPENRACING_MOZA_FFB_MODE` | `standard` (default), `direct`, `off` | Selects FFB mode; `direct` is downgraded if untrusted |
| `OPENRACING_MOZA_HIGH_TORQUE` | `1` / `true` | Requests high torque (requires trusted signature) |
| `OPENRACING_MOZA_DESCRIPTOR_CRC32_ALLOWLIST` | `0xHEX,...` | Comma-separated trusted descriptor CRC32s (Linux) |
| `OPENRACING_MOZA_ALLOW_UNKNOWN_SIGNATURE` | `1` / `true` | Bypass allowlist check (developer escape hatch) |
| `OPENRACING_MOZA_TRANSPORT_MODE` | `raw-hidraw` (default), `kernel-pidff` | Linux transport; `kernel-pidff` skips vendor handshake |
