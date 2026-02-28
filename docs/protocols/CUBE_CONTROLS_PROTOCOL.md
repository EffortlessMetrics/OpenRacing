# Cube Controls Steering Wheel Protocol

## Overview

Cube Controls S.r.l. (Italy) produces premium sim-racing steering wheels:
**GT Pro**, **Formula Pro**, and **CSX3**. These devices likely implement the
**standard USB HID PID (Physical Interface Device — Force Feedback)** protocol.

> ⚠️ **PROVISIONAL DATA** — The VID/PID values for Cube Controls devices have
> **not** been confirmed from official documentation or independent USB captures.
> See "Verification Status" below.

**VID:** `0x0483` (STMicroelectronics shared VID — PROVISIONAL)

| Model              | PID (est.) | Peak Torque  | Notes                          |
|--------------------|------------|--------------|--------------------------------|
| GT Pro             | `0x0C73`   | ~20 Nm       | F1-style wheel                 |
| Formula Pro        | `0x0C74`   | ~20 Nm       | Formula racing wheel           |
| CSX3               | `0x0C75`   | ~20 Nm       | High-end customizable wheel    |

## Force Feedback

Community reports indicate Cube Controls wheels present a standard HID PID
force feedback interface (Usage Page `0x000F`), similar to Simucube 2 and VRS
DirectForce which share the same STM VID.

No proprietary vendor-specific command extensions have been documented. All FFB
effects are expected to go through the standard HID PID effect pipeline.

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
| VID `0x0483` | ⚠️ Community reports, not confirmed |
| PIDs `0x0C73`–`0x0C75` | ❌ Internal estimates |
| FFB protocol (HID PID) | ⚠️ Assumed standard, not captured |
| Input report format | ❌ Unknown |

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
- **JacKeTUs/linux-steering-wheels** — no Cube Controls entries as of 2025-01.
- **Community forum reports** — suggest STM VID, PIDs unverified.
- **This documentation** — best-effort based on the above; requires hardware capture.
