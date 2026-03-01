# Thrustmaster Protocol Documentation

**Status**: ✅ Fully Supported  
**Protocol Type**: USB HID PID (Standard) with Proprietary Initialization

## Overview

Thrustmaster racing wheels use standard USB HID PID (Physical Interface Device) for force feedback after proper initialization. The wheels require a specific initialization sequence to enable the FFB motor, and are sensitive to USB timing and power delivery.

## Device Identification

### Wheel Bases

| Model | Vendor ID | Product ID | Max Torque | Protocol Family | Notes |
|-------|-----------|------------|------------|-----------------|-------|
| T150 | `0x044F` | `0xB677` | 2.5 Nm | T150 | Entry-level belt; `scarburato/t150_driver` |
| T300 RS (PS4 mode) | `0x044F` | `0xB66D` | 4.0 Nm | T300 | T300RS with PS4-mode switch active |
| T300 RS | `0x044F` | `0xB66E` | 4.0 Nm | T300 | Belt-driven (PS3/PC mode) |
| T300 RS GT | `0x044F` | `0xB66F` | 4.0 Nm | T300 | GT Edition (PS3 advanced mode) |
| TX Racing | `0x044F` | `0xB669` | 4.0 Nm | T300 | Xbox variant |
| T500 RS | `0x044F` | `0xB65E` | 4.0 Nm | T500 | Older belt drive; FFB protocol undocumented |
| TMX | `0x044F` | `0xB67F` | 2.5 Nm | T150 | Xbox One variant; same protocol as T150 |
| T248 | `0x044F` | `0xB696` | 4.0 Nm | T300 | Hybrid drive |
| T248X | `0x044F` | `0xB69A` | 4.0 Nm | T300 | Xbox variant (GIP) |
| TS-PC Racer | `0x044F` | `0xB689` | 6.0 Nm | T300 | PC-only, belt |
| TS-XW | `0x044F` | `0xB692` | 6.0 Nm | T300 | Xbox variant (USB/HID mode) |
| T818 | `0x044F` | `0xB69B` | 10.0 Nm | Unknown | Direct Drive; PID unverified |

Source: JacKeTUs/linux-steering-wheels compatibility table.

### Pedals

| Model | Vendor ID | Product ID | Notes |
|-------|-----------|------------|-------|
| T3PA | `0x044F` | `0xB678` | 3-pedal set |
| T3PA Pro | `0x044F` | `0xB679` | Inverted option |
| T-LCM | `0x044F` | `0xB68D` | Load cell brake |
| T-LCM Pro | `0x044F` | `0xB69A` | Updated load cell |

## Initialization Sequence

### Mode Detection

Thrustmaster wheels have a hardware mode switch. For PC use, set switch to **PS3** position.

### FFB Initialization

```
Step 1: Set Device Gain to 0 (disable FFB temporarily)
Step 2: Wait 100ms
Step 3: Set Device Gain to desired level
Step 4: Send Actuator Enable
```

### Set Rotation Range

```
Report Type: Feature Report
Report ID: 0x80 (Vendor Specific)

Payload:
Byte 0: 0x80 (Report ID)
Byte 1: 0x01 (Command: Set Range)
Byte 2: Range LSB (degrees)
Byte 3: Range MSB (degrees)
```

## Input Reports

### Standard Input Report (ID: 0x01)

```
Total Size: 16 bytes

Byte 0:     Report ID (0x01)
Byte 1-2:   Steering Axis (16-bit, little-endian)
Byte 3:     Throttle (8-bit)
Byte 4:     Brake (8-bit)
Byte 5:     Clutch (8-bit)
Byte 6-7:   Buttons (16 bits)
Byte 8:     D-Pad (4-bit hat switch)
Byte 9-15:  Reserved
```

## Output Reports (Force Feedback)

Thrustmaster uses standard USB HID PID protocol for force feedback.

### PID Effect Types Supported

| Effect Type | Usage ID | Support Level |
|-------------|----------|---------------|
| Constant Force | 0x26 | Full |
| Ramp | 0x27 | Full |
| Square | 0x30 | Full |
| Sine | 0x31 | Full |
| Triangle | 0x32 | Full |
| Sawtooth Up | 0x33 | Full |
| Sawtooth Down | 0x34 | Full |
| Spring | 0x40 | Full |
| Damper | 0x41 | Full |
| Inertia | 0x42 | Partial |
| Friction | 0x43 | Full |

## Implementation Notes

### USB Quirks

1. **Power Requirements**: T300/T-GT require USB 3.0 port or powered hub
2. **Initialization Timing**: Wait 200ms between commands during init
3. **Effect Slot Management**: Always free unused effect slots

### Timing Requirements

- **Init Delay**: 200ms between init commands
- **Effect Update Rate**: Maximum 500 Hz
- **Gain Change Delay**: 50ms after changing device gain

### Platform Differences

| Platform | Driver | Notes |
|----------|--------|-------|
| Windows | Thrustmaster Driver | Full support (all models) |
| Linux | hid-tmff2 | T300RS family FFB (T300/T248/TX/TS-XW/TS-PC) |
| Linux | scarburato/t150_driver | T150/TMX FFB (separate protocol from T300RS) |
| Linux | hid-tminit / tmdrv | Init/mode-switch only (T500RS, TX, TMX, TS-XW) |
| macOS | None | Basic HID only |

## Protocol Families

Thrustmaster wheels use **three distinct FFB protocol families**. Sending commands
from one family to a wheel of a different family will not work.

### T300 Family (hid-tmff2)

- **Wheels**: T300RS (all modes), T248, TX, TS-XW, TS-PC, T-GT II
- **Output**: HID Report ID 0x60, 63-byte payloads (31 in PS4 mode)
- **Range**: `degrees * 0x3C` encoding
- **Init switch**: 0x0005
- **Driver**: `Kimplul/hid-tmff2` (Linux), official Thrustmaster (Windows)

### T150 Family (t150_driver)

- **Wheels**: T150, TMX
- **Output**: USB interrupt OUT endpoint (not HID report 0x60)
- **Range**: `[0x40, 0x11, <u16_le>]` — 0xFFFF = max range (1080° T150, 900° TMX)
- **Gain**: `[0x43, <u8>]`
- **Effects**: 3-packet upload: `ff_first` → `ff_update` → `ff_commit`
- **Play/stop**: `[0x41, <id>, <mode>, <times>]`
- **Effect codes**: 0x4000 (constant), 0x4022 (sine), 0x4023 (saw up), 0x4024 (saw down), 0x4040 (spring), 0x4041 (damper)
- **Init switch**: 0x0006
- **Driver**: `scarburato/t150_driver` (Linux), official Thrustmaster (Windows)

### T500 Family (undocumented)

- **Wheels**: T500RS
- **Output**: Unknown — no community FFB driver exists
- **Init switch**: 0x0002
- **hid-tminit model bytes**: 0x0200
- **Status**: FFB wire format is completely undocumented. Only init/mode-switch
  is known (via `her001/tmdrv`). hid-tmff2 issue #18 is an open request.

## Resources

- **hid-tmff2** (T300 family FFB): [https://github.com/Kimplul/hid-tmff2](https://github.com/Kimplul/hid-tmff2)
- **scarburato/t150_driver** (T150/TMX FFB): [https://github.com/scarburato/t150_driver](https://github.com/scarburato/t150_driver)
- **scarburato/hid-tminit** (init driver): [https://github.com/scarburato/hid-tminit](https://github.com/scarburato/hid-tminit)
- **her001/tmdrv** (T500RS init): [https://github.com/her001/tmdrv](https://github.com/her001/tmdrv)
- **linux-steering-wheels**: [https://github.com/JacKeTUs/linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels)
- **Thrustmaster Support**: [https://support.thrustmaster.com](https://support.thrustmaster.com)
