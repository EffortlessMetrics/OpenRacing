# VRS DirectForce Pro Protocol Documentation

**Status**: ✅ Fully Supported  
**Protocol Type**: USB HID PID (Standard) with Proprietary Initialization

## Overview

VRS (Virtual Racing School) DirectForce Pro wheelbases use standard USB HID PID force
feedback with a 3-step proprietary initialization sequence. After initialization the
device behaves as a standard PIDFF device.

VRS devices use VID `0x0483` (STMicroelectronics USB), shared with legacy Simagic devices.
They are distinguished by Product ID ranges in `0xA3xx`.

## Device Identification

### Wheel Bases

| Model | Vendor ID | Product ID | Max Torque | Notes |
|-------|-----------|------------|------------|-------|
| DirectForce Pro | `0x0483` | `0xA355` | 20 Nm | Direct drive |
| DirectForce Pro V2 | `0x0483` | `0xA356` | 25 Nm | Updated model |

### Accessories (Input-only, no FFB)

| Model | Vendor ID | Product ID | Notes |
|-------|-----------|------------|-------|
| Pedals V1 | `0x0483` | `0xA357` | Analog load-cell pedals |
| Pedals V2 | `0x0483` | `0xA358` | Updated pedal set |
| Handbrake | `0x0483` | `0xA359` | Digital handbrake |
| Shifter | `0x0483` | `0xA35A` | H-pattern + sequential shifter |

## VID Disambiguation

VID `0x0483` is shared with legacy Simagic devices (PIDs `0x0522`–`0x0524`,
`0x0D5A`–`0x0D5B`). VRS products are identified by PIDs in the `0xA3xx` range.

```
VID 0x0483 + PID 0xA3xx → VRS DirectForce
VID 0x0483 + other PIDs  → Simagic legacy
```

## Initialization Sequence (Wheelbase Only)

Pedals, handbrake, and shifter require **no initialization**.

For the DirectForce Pro wheelbase:

```
Step 1: Send Feature Report — FFB Enable (Device Control, enable FFB subsystem)
Step 2: Send Feature Report — Device Gain (0xFF = full gain)
Step 3: Send Feature Report — Set Rotation Range (1080° for Pro/V2)
```

### FFB Enable Report

```
Report ID: 0x0B (DEVICE_CONTROL)
Byte 0: 0x0B
Byte 1: 0x01 (Enable FFB)
```

### Device Gain Report

```
Report ID: varies (vendor-specific)
Value: 0xFF (full gain, range 0x00–0xFF)
```

### Rotation Range Report

```
Report ID: varies (vendor-specific)
Value: rotation degrees (u16, little-endian)
```

## Shutdown

On device close, the FFB is disabled by sending an FFB Enable report with `0x00`.

## Output Reports (Force Feedback)

VRS uses standard HID PID on report ID `0x11` (CONSTANT_FORCE).

```
Constant Force Report ID: 0x11
Report rate: ~1 kHz (bInterval=1)
Encoder CPR: 1,048,576 (20-bit)
```

### PID Report IDs

| Report | ID | Purpose |
|--------|----|---------|
| Standard Input | `0x01` | Steering, buttons |
| Set Effect | `0x02` | PID block load |
| Effect Operation | `0x0A` | Play/stop |
| Device Control | `0x0B` | Enable/disable FFB |
| Constant Force | `0x11` | CF output |
| Spring | `0x19` | Spring effect |
| Damper | `0x1A` | Damper effect |
| Friction | `0x1B` | Friction effect |

## Implementation Notes

### Encoder Resolution

```
CPR = 1,048,576 (2^20, estimated 20-bit)
Resolution ≈ 0.00034° per count
```

### Torque Specifications

| Model | Continuous | Peak |
|-------|-----------|------|
| DirectForce Pro | 20 Nm | 27 Nm |
| DirectForce Pro V2 | 25 Nm | 35 Nm |

## Resources

- **VRS DirectForce Pro**: [https://www.virtualracingschool.com](https://www.virtualracingschool.com)
- **OpenFFBoard** (similar PIDFF implementation): [https://github.com/Ultrawipf/OpenFFBoard](https://github.com/Ultrawipf/OpenFFBoard)
