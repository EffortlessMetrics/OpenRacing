# Fanatec Protocol Documentation

**Status**: Proprietary / Well Documented by Community

## Overview

Fanatec devices boot in a "PC Mode" (Generic HID) but require a specific initialization sequence to switch to "Advanced Mode" (sometimes called "Native Mode") which enables:
- High-resolution inputs.
- Telemetry (LEDs, Display).
- Full Force Feedback capabilities.

## Initialization "Magic Bytes"

To switch the device to advanced mode, the host must send a specific USB feature report.

* **Report ID**: `0x01` (typically).
* **Payload**: `0x01, 0x03, 0x00, ...` (Varies by base).

## Protocol Details

### Input Reports
- **Generic Mode**: Standard HID Joystick (Buttons + Axes).
- **Advanced Mode**:
    - Vendor-specific report structure.
    - Includes detailed status (tuning menu active, rim type attached).

### Output Reports (Telemetry)
- **LEDs/Display**: Sent via HID Output Report (often ID `0x01` or `0x08`).
- **Structure**:
    - Byte 0: Report ID.
    - Byte 1: Command (e.g., `0x80` for LED).
    - Byte 2: LED Bitmask (Rev lights).
    - Byte 3-5: 7-Segment Display Data.

## Resources

- **hid-fanatecff**: [https://github.com/gotzl/hid-fanatecff](https://github.com/gotzl/hid-fanatecff)
- **Teensy Adapter**: [https://github.com/dchote/fanatecWheelUSB](https://github.com/dchote/fanatecWheelUSB)
