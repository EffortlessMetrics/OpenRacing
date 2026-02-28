# Cammus Protocol Documentation

**Status**: ⚠️ Partial Support (Input only; FFB initialization partially documented)  
**Protocol Type**: USB HID PID (Standard) with Proprietary Features

## Overview

Cammus direct drive wheelbases (by Shenzhen Cammus Electronic Technology Co., Ltd.)
implement USB HID for input reporting and proprietary HID reports for force feedback
configuration. The FFB sub-protocol appears similar to other Chinese DD vendors but has
not been fully reverse-engineered.

## Device Identification

| Model | Vendor ID | Product ID | Max Torque | Notes |
|-------|-----------|------------|------------|-------|
| Cammus C5 | `0x3416` | `0x0301` | 5 Nm | Desktop direct drive |
| Cammus C12 | `0x3416` | `0x0302` | 12 Nm | Desktop direct drive |

**Vendor ID note:** VID `0x3416` is registered to Shenzhen Cammus Electronic Technology
Co., Ltd. Sources: USB VID registry (the-sz.com), JacKeTUs/linux-steering-wheels
compatibility table.

## Input Report Layout

```
Report ID: 0x01
Total Size: 8 bytes (typical)

Byte 0:     Report ID (0x01)
Byte 1-2:   Steering Axis (16-bit, center = 0x8000)
Byte 3:     Throttle (8-bit)
Byte 4:     Brake (8-bit)
Byte 5:     Clutch (8-bit)
Byte 6-7:   Buttons (16-bit bitmask)
```

## Force Feedback

Cammus wheelbases enumerate standard USB HID PID usage pages. After USB connection
the device should respond to standard HID PID output reports for basic effects.

### Output Report — Constant Force

```
Report ID: 0x01  (Output)
Total Size: 8 bytes

Byte 0:     Report ID (0x01)
Byte 1:     Effect Type (0x01 = Constant Force)
Byte 2-3:   Magnitude (signed 16-bit, ±32767)
Byte 4:     Effect Index (1-based)
Byte 5-7:   Reserved (0x00)
```

> **Note:** The exact output report layout has not been independently verified. Cammus
> may use standard USB HID PID reports (Usage Page 0x0F). The above is a best-estimate
> based on observed USB traffic. Full reverse engineering of the Cammus PC software
> (Cammus Pit Stop) is needed to confirm.

## Initialization

No proprietary initialization sequence has been confirmed. Plug-and-play USB HID PID
is expected to work for basic FFB. Advanced features (tuning, rotation range) may
require vendor-specific feature reports found in the Cammus Pit Stop software.

## Resources

- **linux-steering-wheels**: [https://github.com/JacKeTUs/linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels)
- **Cammus official**: [https://www.cammus.com](https://www.cammus.com)
- USB VID registry for 0x3416: [https://the-sz.com/products/usbid/](https://the-sz.com/products/usbid/)
