# OpenFFBoard Protocol Documentation

**Status**: ✅ Fully Supported  
**Protocol Type**: USB HID PID (Standard) + custom feature reports for initialization  
**Project**: https://github.com/Ultrawipf/OpenFFBoard

## Overview

OpenFFBoard is an open-source direct drive force feedback controller project.
It uses standard USB HID PID force feedback with two feature reports for device
configuration (FFB enable/disable and global gain).

## Device Identification

| Model | Vendor ID | Product ID | Max Torque | Notes |
|-------|-----------|------------|------------|-------|
| OpenFFBoard (main) | `0x1209` | `0xFFB0` | 20 Nm (configurable) | Main firmware PID |
| OpenFFBoard (alt) | `0x1209` | `0xFFB1` | 20 Nm (configurable) | Alt firmware PID |

**VID Note**: `0x1209` is the [pid.codes](https://pid.codes/) open-hardware shared VID.
All udev rules for OpenFFBoard MUST include a product ID filter to avoid matching other
pid.codes devices.

## Initialization

OpenFFBoard requires two feature reports to enable force feedback:

1. **FFB Enable** (report ID `0x60`): Write `[0x60, 0x01]` to enable FFB actuator.
2. **Global Gain** (report ID `0x61`): Write `[0x61, 0xFF]` to set 100% gain.

On shutdown: write `[0x60, 0x00]` to disable the FFB actuator.

## Output Reports (Force Feedback)

Output is via standard HID PID. The torque value is encoded as a signed 16-bit integer
in the range `[-10000, 10000]` representing `[-max_torque, +max_torque]`.

```
Report ID: 0x01
Bytes: [report_id(1), torque_i16_le(2), reserved(2)] = 5 bytes
Range: -10000 to +10000
Encoder CPR: 65535 (16-bit, configurable in firmware)
Report rate: up to 1 kHz (USB full-speed)
Default max torque: 20 Nm (software configurable)
```

### Feature Reports

| Report ID | Description | Payload |
|-----------|-------------|---------|
| `0x60` | FFB enable/disable | `[0x60, 0x01]` = enable, `[0x60, 0x00]` = disable |
| `0x61` | Global gain | `[0x61, gain_u8]` where `0xFF` = 100% |
| `0x30` | Custom command | Protocol-specific extended commands |

### HID PID Effect Types Supported

| Effect Type | Support Level |
|-------------|---------------|
| Constant Force | Full |
| Spring | Full |
| Damper | Full |
| Friction | Full |
| Inertia | Full |
| Sine / Square / Triangle | Full |

## Torque Encoding

```
torque_raw = round(torque_nm / max_torque_nm * 10000)
torque_raw = clamp(torque_raw, -10000, 10000)
```

The raw value is written as little-endian i16 in bytes 1–2 of the output report.

## Configuration

OpenFFBoard is highly configurable via its web UI and serial/CDC interface. Parameters
like max torque, rotation range, and encoder CPR are set through the device configuration
— not via HID. OpenRacing uses the `0x61` gain report to manage the software gain, and
the `0x60` enable report to control the actuator.

## Linux udev Rules

```udev
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="1209", ATTRS{idProduct}=="ffb0", GROUP="input", MODE="0664", TAG+="uaccess"
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="1209", ATTRS{idProduct}=="ffb1", GROUP="input", MODE="0664", TAG+="uaccess"
ACTION=="add", SUBSYSTEM=="usb", ATTRS{idVendor}=="1209", ATTRS{idProduct}=="ffb0", ATTR{power/autosuspend}="-1"
ACTION=="add", SUBSYSTEM=="usb", ATTRS{idVendor}=="1209", ATTRS{idProduct}=="ffb1", ATTR{power/autosuspend}="-1"
```

**Important**: Use product-ID-specific rules for VID `0x1209` (pid.codes shared VID).

## References

- OpenFFBoard firmware: https://github.com/Ultrawipf/OpenFFBoard
- OpenFFBoard hardware: https://github.com/Ultrawipf/OpenFFBoard-configurator
- pid.codes: https://pid.codes/1209/FFB0/ and https://pid.codes/1209/FFB1/
- Linux kernel: `drivers/hid/hid-ids.h` (not yet registered as of 2024)
