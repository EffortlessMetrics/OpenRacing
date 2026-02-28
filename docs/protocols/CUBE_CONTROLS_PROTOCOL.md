# Cube Controls Steering Wheel Protocol

## Overview

Cube Controls S.r.l. (Italy) produces premium sim-racing **steering wheels**
(button boxes / rims): **GT Pro**, **Formula CSX-3**, **F-CORE**, and others.
These are **input-only** USB/Bluetooth HID devices with buttons, rotary
encoders, and paddles. They do **not** produce force feedback — FFB comes
from the wheelbase (a separate device by another vendor such as Simucube).

> ⚠️ **PROVISIONAL DATA** — The VID/PID values for Cube Controls devices have
> **not** been confirmed from official documentation or independent USB captures.
> See "Verification Status" below.

**VID:** `0x0483` (STMicroelectronics shared VID — PROVISIONAL)

| Model              | PID (est.) | Torque | Notes                                       |
|--------------------|------------|--------|---------------------------------------------|
| GT Pro             | `0x0C73`   | N/A    | F1-style wireless steering wheel            |
| Formula Pro        | `0x0C74`   | N/A    | Formula racing steering wheel               |
| CSX3               | `0x0C75`   | N/A    | Steering wheel with 4" touchscreen          |

> **Note:** Torque is not applicable — these are input devices, not wheelbases.

## Device Type

Cube Controls products are **steering wheel button boxes**, not wheelbases:

- They connect to the PC via USB (for configuration/charging) and Bluetooth
  (for wireless input during racing)
- They present standard USB HID game controller interfaces (buttons, axes)
- They do **not** implement HID PID (force feedback) descriptors
- Force feedback is handled by the wheelbase (Simucube, VRS, etc.)
- The SP-01 is a pedal set, also an input-only device

## VID Collision

VID `0x0483` (STMicroelectronics) is shared by multiple sim-racing devices:

| PID Range  | User                              |
|------------|-----------------------------------|
| `0x0522`   | Simagic legacy (Alpha Mini)       |
| `0xA355`+  | VRS DirectForce Pro               |
| `0x0C73`–`0x0C75` | Cube Controls (PROVISIONAL) |

The OpenRacing engine disambiguates these using `is_cube_controls_product()`,
`is_vrs_product()`, and falls through to Simagic otherwise.

## Verification Status

| Item | Status |
|------|--------|
| VID `0x0483` | ⚠️ Plausible (STM32 MCU), not confirmed from hardware |
| PIDs `0x0C73`–`0x0C75` | ❌ Internal estimates, not found in any database |
| Device type | ℹ️ Input-only (buttons/encoders), not force feedback |
| Input report format | ❌ Unknown |

**Research pass (2025-06):** The following sources were checked with no
Cube Controls VID/PID found:

- JacKeTUs/linux-steering-wheels: no Cube Controls entries
- devicehunt.com (VID 0x0483): PIDs 0x0C73–0x0C75 not registered
- cubecontrols.com: no USB VID/PID information published
- Linux kernel hid-ids.h: no entries
- GitHub code search: no independent USB captures

**Action required:** A volunteer with Cube Controls hardware should run:

```bash
# Linux
lsusb -v | grep -A 20 "Cube Controls\|cube"
sudo usbhid-dump -d 0483: -i 0 -t 10 > cube_hid_dump.txt

# Windows (Device Manager / USBTreeView)
# Look for VID_0483 in Device Manager > Human Interface Devices
```

Share the VID/PID in the OpenRacing GitHub Discussions under "Hardware Captures".
Once confirmed, update `crates/hid-cube-controls-protocol/src/ids.rs` and
remove the PROVISIONAL annotations.

## OpenRacing Implementation

| Component | Location |
|-----------|----------|
| IDs + model types | `crates/hid-cube-controls-protocol/src/ids.rs` |
| Engine integration | `crates/engine/src/hid/vendor/cube_controls.rs` |

## Protocol Sources

- **No official SDK** — Cube Controls does not publish USB protocol documentation.
- **JacKeTUs/linux-steering-wheels** — no Cube Controls entries (checked 2025-06).
- **devicehunt.com** — PIDs 0x0C73–0x0C75 not in STMicroelectronics database.
- **Community forum reports** — suggest STM VID, PIDs unverified.
- **This documentation** — best-effort based on the above; requires hardware capture.
