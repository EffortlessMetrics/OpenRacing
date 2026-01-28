# Logitech Protocol Documentation

**Status**: Mixed (Standard HID vs TrueForce)

## Generations

### Legacy (G25, G27, G29, G920)
- **Protocol**: Standard USB HID PID (Physical Interface Device).
- **Vendor ID**: `0x046D` (Logitech).
- **Initialization**:
    - Requires a "Native Mode" command to unlock full rotation (900 degrees) and separate pedals.
    - Without init, behaves as a generic 200-degree wheel.

### Modern (G923, Pro Racing Wheel)
- **Protocol**: TrueForce (Proprietary).
- **Features**:
    - High-frequency haptics (audio-based).
    - Requires specific game integration.
- **Fallback**: Can usually operate in "Legacy Mode" (Standard PID) if TrueForce is not sent.

## Initialization Command (Legacy)

To switch G29/G920 to "Native Mode":
- **Command**: `0xF8` (vendor specific).
- **Payload**: `0x0A, 0x00, 0x00, ...` (Example, varies).

## Resources

- **new-lg4ff**: [https://github.com/berarma/new-lg4ff](https://github.com/berarma/new-lg4ff)
- **libratbag**: Configures specialized Logitech features.
