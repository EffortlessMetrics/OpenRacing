# PXN V10/V12 Direct Drive Wheel Protocol

## Overview

PXN (Shenzhen Jinyu Technology Co., Ltd.) direct drive wheelbases implement the
**standard USB HID PID (Physical Interface Device — Force Feedback)** protocol.
No proprietary vendor-specific command extensions have been observed.

**VID:** `0x11FF` (Shenzhen Jinyu Technology Co., Ltd.)

| Model              | PID      | Peak Torque | Notes                     |
|--------------------|----------|-------------|---------------------------|
| V10                | `0x3245` | 10 Nm       | Entry-level DD             |
| V12                | `0x1212` | 12 Nm       | Mid-range DD               |
| V12 Lite           | `0x1112` | 12 Nm       | Compact form factor        |
| V12 Lite SE        | `0x1211` | 12 Nm       | Special edition compact    |
| GT987 FF           | `0x2141` | ~5 Nm est.  | Lite Star OEM, shared VID  |

> **Unknown PIDs:** VD4, VD6, VD10+ PIDs are not yet confirmed. Excluded until
> USB captures are available.

## Force Feedback

PXN wheels advertise a standard HID PID Usage Page (`0x000F`) interface. FFB
effects are sent as standard HID PID Effect Reports.

**Output Report ID:** Estimated as `0x05` (standard HID PID effect command).
⚠️ This is **not yet confirmed** via USB capture — a volunteer with hardware
should run `usb-hid-analyzer` or Wireshark with USBPcap and share the capture.

## Input Reports

Input reports use **Report ID `0x01`** at byte 0. The field layout:

| Byte(s) | Field    | Encoding                                  |
|---------|----------|-------------------------------------------|
| 0       | Report ID | `0x01`                                   |
| 1–2     | Steering | Little-endian i16; full range ±32767      |
| 3–4     | Throttle | Little-endian u16; 0 = released, 65535 = full |
| 5–6     | Brake    | Little-endian u16; 0 = released, 65535 = full |
| 7       | Buttons  | Bit-packed (bits 0–7 = buttons 1–8)       |
| 8       | Buttons  | Bit-packed (bits 0–7 = buttons 9–16)      |
| 9–10    | Clutch   | Little-endian u16; 0 = released, 65535 = full |

**Total buffer length:** 11 bytes (1 report ID + 10 data bytes).

**Steering range:** ±900° (1800° total) typical for DD wheels.

## Protocol Source

- **Primary:** [JacKeTUs/linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels) compatibility table
- **Secondary:** Community USB captures shared on sim racing forums (no official PXN developer docs published)
- **Protocol:** Standard HID PID — no vendor extensions documented

## OpenRacing Implementation

| Component | Location |
|-----------|----------|
| IDs | `crates/hid-pxn-protocol/src/ids.rs` |
| Input parser | `crates/hid-pxn-protocol/src/input.rs` |
| Output encoder | `crates/hid-pxn-protocol/src/output.rs` |
| Model classification | `crates/hid-pxn-protocol/src/types.rs` |
| Engine integration | `crates/engine/src/hid/vendor/pxn.rs` |

## Verification Status

| Item | Status |
|------|--------|
| VID `0x11FF` | ✅ Community-verified (linux-steering-wheels table) |
| PIDs V10/V12/Lite/LiteSE/GT987 | ✅ Community-verified |
| Input report format | ⚠️ Derived from standard HID descriptor parsing |
| FFB report ID `0x05` | ⚠️ Estimated — USB capture needed |
| VD-series PIDs | ❌ Unknown |

## Capture Needed

If you own a PXN V10, V12, or VD-series wheel, please run:

```bash
# Linux
sudo usbhid-dump -d 11ff: -i 0 -t 10 > pxn_hid_dump.txt

# Windows (Wireshark + USBPcap)
# Filter: usb.idVendor == 0x11ff
```

Share the capture in the OpenRacing GitHub Discussions to help confirm the
FFB report ID and input report layout.
