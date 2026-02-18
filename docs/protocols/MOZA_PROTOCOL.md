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
| **HBP Handbrake** | Handbrake | USB | `0x0022` (standalone) | *Planned* |

## Discovery & Initialization

### The "Magic" Handshake

Moza wheelbases start in a restricted mode. To enable high-frequency force feedback and full input reporting (including aggregated pedals), the host must send a specific feature report sequence.

**Required Sequence:**

1. **Enable High Torque / Motor:** Feature Report `0x02` -> `[0x02, 0x00, 0x00, 0x00]`
2. **Start Reporting:** Feature Report `0x03` -> `[0x03, 0x00, 0x00, 0x00]`
3. **Set Standard FFB Mode:** Feature Report `0x11` -> `[0x11, 0x00, 0x00, 0x00]`

*Note: Without Step 2, the wheelbase may not report pedal axis changes.*

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

### HBP handbrake topology classes

Moza handbrake input appears in two supported runtime paths:

1. **Direct USB HBP**
   - HID device is present as `VID=0x346E`, `PID=0x0022`.
   - No wheelbase handshake required.
   - Parse path uses a dedicated HBP parser in `crates/engine/src/hid/vendor/moza.rs`.

2. **Wheelbase-embedded HBP**
   - HBP is attached to a wheelbase port and exposed through the wheelbase report.
   - Requires normal wheelbase initialization (`0x02`, `0x03`, `0x11`) to start reporting.
   - Axis is expected in the wheelbase report handbrake field (`report_id=0x01`, offset 9..10), when present.

Only topology-level behavior and timing has been implemented in-engine; exact payload layouts and optional button semantics are marked **capture-validated only** until USB traces are added in the capture utility.

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
