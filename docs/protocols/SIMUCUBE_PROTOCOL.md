# Simucube Protocol Documentation

**Status**: ✅ Fully Supported  
**Protocol Type**: USB HID PID (Standard) — plug-and-play, no initialization required

## Overview

Simucube 2 direct drive wheelbases (by Granite Devices) implement standard USB HID PID
force feedback. They enumerate as USB HID devices and are immediately ready for FFB after
USB connection — no proprietary handshake or initialization sequence is needed.

Simucube devices run at approximately 360 Hz (`bInterval = 3`) and use 64-byte HID reports.

**Vendor ID note:** Simucube 2 devices use VID `0x16D0` (registered to MCS Electronics /
OpenMoko), which is shared with Heusinkveld pedals and legacy Simagic products. Devices are
distinguished by their product ID. Sources: Linux kernel `hid-ids.h` (`USB_VENDOR_ID_MCS =
0x16d0`), JacKeTUs/linux-steering-wheels compatibility table.

## Device Identification

### Wheel Bases

| Model | Vendor ID | Product ID | Max Torque | Encoder | Notes |
|-------|-----------|------------|------------|---------|-------|
| Simucube 2 Sport | `0x16D0` | `0x0D61` | 15 Nm | 22-bit | Entry direct drive |
| Simucube 2 Pro | `0x16D0` | `0x0D60` | 25 Nm | 22-bit | Mid-tier |
| Simucube 2 Ultimate | `0x16D0` | `0x0D5F` | 35 Nm | 22-bit | Top-tier |

### Accessories

| Model | Vendor ID | Product ID | Notes |
|-------|-----------|------------|-------|
| Simucube ActivePedal | `0x16D0` | `0x0D62` | Load-cell force feedback pedal (PID estimated) |
| SimuCube Wireless Wheel | `0x16D0` | `0x0D63` | Wireless steering wheel (PID estimated) |

## Initialization

**No initialization sequence is required.** The Simucube 2 is FFB-ready immediately upon
USB connection. Simply open the HID device and begin sending standard HID PID effect
reports.

## Output Reports (Force Feedback)

Simucube uses standard USB HID PID with 64-byte reports. Report ID is `0x01`.

```
Output Report Size: 64 bytes
Report ID byte: 0x01 (first byte)
bInterval: 3 (≈360 Hz update rate)
Encoder CPR: 4,194,304 (22-bit angle sensor)
```

### PID Effect Types Supported

| Effect Type | Support Level |
|-------------|---------------|
| Constant Force | Full |
| Ramp | Full |
| Spring | Full |
| Damper | Full |
| Inertia | Full |
| Friction | Full |
| Sine, Square, Triangle, Sawtooth | Full |

## Input Reports

Simucube reports steering position via the standard HID input report. The 22-bit encoder
provides extremely high angular resolution (~0.000086° per count).

## Implementation Notes

### Encoder Resolution

The Simucube 2 uses a 22-bit absolute angle sensor:

```
CPR = 4,194,304 (2^22)
Resolution ≈ 0.000086° per count
Full rotation = 4,194,304 counts
```

### Device Capabilities by Model

| Model | Max Torque | Peak Power | Report Rate |
|-------|-----------|------------|-------------|
| Sport | 15 Nm | ~1000 W | ~360 Hz |
| Pro | 25 Nm | ~2000 W | ~360 Hz |
| Ultimate | 35 Nm | ~3000 W | ~360 Hz |

### ActivePedal

The Simucube ActivePedal (PID `0x0201`) is an active force-feedback pedal. It is **not**
a wheelbase and does not support motor torque FFB commands. Input-only on the force
feedback channel; it uses its own dedicated HID protocol for pedal feedback.

## Resources

- **Simucube 2 SDK**: [https://github.com/SimuCUBE/SC2-sdk](https://github.com/SimuCUBE/SC2-sdk)
- **Granite Devices**: [https://granitedevices.com](https://granitedevices.com)
- **True Drive Software**: Simucube configuration application for Windows/Linux
