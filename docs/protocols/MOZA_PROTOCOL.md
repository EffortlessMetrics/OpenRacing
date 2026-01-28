# Moza Racing Protocol Documentation

**Status**: Partially Known / Standard PIDFF with Quirks

## Device Identification

| Model | Vendor ID | Product ID | Notes |
|-------|-----------|------------|-------|
| R9    | `0x3416`  | `0x0381`   | Verified (Community) |
| R5    | `0x3416`  | `0x0382`   | Likely |

*Note: Vendor ID `0x3416` is sometimes identified as "Lenovo" in generic databases, but used by Moza.*

## Initialization

Moza wheelbases generally behave as standard HID PID (Physical Interface Device) Force Feedback devices. However, to unlock full 16-bit resolution or specific telemetry features, a "handshake" or specific feature report may be required.

### Standard Operation
- **FFB Protocol**: Standard USB HID PID (Usage Page `0x0F`).
- **Axes**: 
    - Steering: Often generic HID `X` axis (16-bit).
    - Pedals: `Y`, `Z`, `Rz` axes depending on configuration.

## Known Quirks (from `universal-pidff`)

1. **Initialization Sequence**: 
   - Some devices require a "Start" command to begin sending input reports reliably.
   - Command: `0x01` (Report ID) followed by vendor specific payload. (Requires sniffing).

2. **Force Feedback**:
   - Uses standard PID Set Effect reports.
   - May require specific "Enable Actuators" command (`0x01` on PID block).

## Telemetry (Reverse Engineered)

Communication for dashboard data (RPM, Gear) often happens via a separate Endpoint or Report ID (often `0x02` or vendor specific).

- **Structure**:
  - Byte 0: Report ID
  - Byte 1: Command Type (e.g., `0x02` for Telemetry)
  - Bytes 2-N: Payload (RPM, Speed, Gear)

## Resources

- **Universal PIDFF Driver**: [https://github.com/JacKeTUs/universal-pidff](https://github.com/JacKeTUs/universal-pidff)
- **Arduino Emulator**: [https://github.com/MikeSzklarz/Arduino-Moza-Emulator](https://github.com/MikeSzklarz/Arduino-Moza-Emulator)
