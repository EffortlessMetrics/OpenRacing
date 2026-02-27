# Thrustmaster Protocol Documentation

**Status**: ✅ Fully Supported  
**Protocol Type**: USB HID PID (Standard) with Proprietary Initialization

## Overview

Thrustmaster racing wheels use standard USB HID PID (Physical Interface Device) for force feedback after proper initialization. The wheels require a specific initialization sequence to enable the FFB motor, and are sensitive to USB timing and power delivery.

## Device Identification

### Wheel Bases

| Model | Vendor ID | Product ID | Max Torque | Notes |
|-------|-----------|------------|------------|-------|
| T150 | `0x044F` | `0xB65D` | 2.5 Nm | Entry-level, belt |
| T150 Pro | `0x044F` | `0xB65E` | 2.5 Nm | With T3PA pedals |
| TMX | `0x044F` | `0xB66D` | 2.5 Nm | Xbox variant of T150 |
| T248 | `0x044F` | `0xB696` | 4.0 Nm | Hybrid drive |
| T248X | `0x044F` | `0xB697` | 4.0 Nm | Xbox variant |
| T300 RS | `0x044F` | `0xB66E` | 4.0 Nm | Belt-driven |
| T300 RS GT | `0x044F` | `0xB66F` | 4.0 Nm | GT Edition |
| TX Racing | `0x044F` | `0xB669` | 4.0 Nm | Xbox variant |
| T-GT | `0x044F` | `0xB68E` | 6.0 Nm | Gran Turismo |
| T-GT II | `0x044F` | `0xB692` | 6.0 Nm | Updated T-GT |
| TS-PC Racer | `0x044F` | `0xB689` | 6.0 Nm | PC-only, belt |
| TS-XW | `0x044F` | `0xB691` | 6.0 Nm | Xbox variant |
| T818 | `0x044F` | `0xB69B` | 10.0 Nm | Direct Drive |

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
| Windows | Thrustmaster Driver | Full support |
| Linux | hid-tmff2 | Full FFB support |
| macOS | None | Basic HID only |

## Resources

- **hid-tmff2**: [https://github.com/Kimplul/hid-tmff2](https://github.com/Kimplul/hid-tmff2)
- **Thrustmaster Support**: [https://support.thrustmaster.com](https://support.thrustmaster.com)
