# Fanatec Protocol Documentation

**Status**: ✅ Fully Supported  
**Protocol Type**: Custom HID (Proprietary)

## Overview

Fanatec devices use a proprietary HID protocol that requires initialization to switch from "Compatibility Mode" to "Advanced Mode". In Advanced Mode, the device exposes full force feedback capabilities, telemetry output (LEDs, displays), and high-resolution inputs.

## Device Identification

### Wheel Bases

| Model | Vendor ID | Product ID | Max Torque | Notes |
|-------|-----------|------------|------------|-------|
| CSL Elite | `0x0EB7` | `0x0E03` | 6 Nm | Belt-driven |
| CSL Elite PS4 | `0x0EB7` | `0x0005` | 6 Nm | PlayStation compatible |
| CSL DD | `0x0EB7` | `0x0020` | 5/8 Nm | Direct Drive |
| DD1 | `0x0EB7` | `0x0001` | 20 Nm | Direct Drive, Pro |
| DD2 | `0x0EB7` | `0x0004` | 25 Nm | Direct Drive, Pro |
| GT DD Pro | `0x0EB7` | `0x0024` | 5/8 Nm | PlayStation compatible |
| ClubSport V2 | `0x0EB7` | `0x6204` | 8 Nm | Legacy |
| ClubSport V2.5 | `0x0EB7` | `0x0006` | 8 Nm | Belt-driven |
| Podium DD1 | `0x0EB7` | `0x0001` | 20 Nm | Same as DD1 |
| Podium DD2 | `0x0EB7` | `0x0004` | 25 Nm | Same as DD2 |

### Steering Wheels (Rims)

| Model | Rim ID | Notes |
|-------|--------|-------|
| BMW GT2 | `0x01` | Standard buttons |
| Formula V2 | `0x02` | Dual clutch paddles |
| Formula V2.5 | `0x03` | Updated Formula |
| McLaren GT3 V2 | `0x04` | Funky switch |
| Porsche 918 RSR | `0x05` | Limited edition |
| ClubSport RS | `0x06` | Round wheel |
| WRC | `0x07` | Rally style |
| CSL Elite P1 | `0x08` | Xbox compatible |
| Podium Hub | `0x09` | Adapter for 3rd party |

## Initialization Sequence

### Mode Switch (Compatibility → Advanced)

```
Report Type: Feature Report
Report ID: 0x01

Payload Structure:
Byte 0: 0x01 (Report ID)
Byte 1: 0x01 (Command: Set Mode)
Byte 2: 0x03 (Mode: Advanced/PC)
Byte 3: 0x00
Byte 4: 0x00
Byte 5: 0x00
Byte 6: 0x00
Byte 7: 0x00

Response: Device re-enumerates with new descriptors
```

### Query Device Info

```
Report Type: Feature Report (GET)
Report ID: 0x02

Response:
Byte 0: 0x02 (Report ID)
Byte 1: Base Type (0x01 = CSL, 0x02 = DD, etc.)
Byte 2: Rim ID (see table above)
Byte 3: Firmware Major
Byte 4: Firmware Minor
Byte 5: Firmware Patch
Byte 6-7: Serial Number (partial)
```

### Set Operating Mode

```
Report Type: Feature Report
Report ID: 0x03

Payload:
Byte 0: 0x03 (Report ID)
Byte 1: Mode
        0x00: Compatibility (Xbox/PS emulation)
        0x01: PC Mode
        0x02: PS4 Mode
        0x03: Xbox Mode
Byte 2-7: Reserved (0x00)
```

## Input Reports

### Standard Input Report (ID: 0x01)

```
Total Size: 64 bytes

Byte 0:     Report ID (0x01)
Byte 1-2:   Steering Axis (16-bit, little-endian)
            Range: 0x0000 - 0xFFFF (center: 0x8000)
Byte 3:     Throttle (8-bit inverted: 0xFF = released)
Byte 4:     Brake (8-bit inverted: 0xFF = released)
Byte 5:     Clutch (8-bit inverted: 0xFF = released)
Byte 6:     Handbrake (8-bit, if equipped)
Byte 7-8:   Buttons Low (16 bits)
            Bit 0:  A/Cross
            Bit 1:  B/Circle
            Bit 2:  X/Square
            Bit 3:  Y/Triangle
            Bit 4:  LB/L1
            Bit 5:  RB/R1
            Bit 6:  LSB (Left Stick Button)
            Bit 7:  RSB (Right Stick Button)
            Bit 8:  Menu/Options
            Bit 9:  View/Share
            Bit 10: Xbox/PS Button
            Bit 11: Paddle Right (Upshift)
            Bit 12: Paddle Left (Downshift)
            Bit 13: Funky Switch Press
            Bit 14: Reserved
            Bit 15: Reserved
Byte 9:     D-Pad (4-bit hat switch)
            0x0: Up, 0x1: Up-Right, 0x2: Right, ...
            0xF: Neutral
Byte 10:    Funky Switch Direction
            0x00: Center
            0x01: Up
            0x02: Right
            0x03: Down
            0x04: Left
Byte 11-12: Rotary Encoder 1 (if equipped)
Byte 13-14: Rotary Encoder 2 (if equipped)
Byte 15:    Dual Clutch Left (Formula wheels)
Byte 16:    Dual Clutch Right (Formula wheels)
Byte 17-63: Reserved / Rim-specific data
```

### Extended Input Report (ID: 0x02)

```
Total Size: 64 bytes

Byte 0:     Report ID (0x02)
Byte 1-2:   High-Resolution Steering (16-bit)
Byte 3-4:   Steering Velocity (signed 16-bit)
Byte 5:     Motor Temperature (°C)
Byte 6:     Board Temperature (°C)
Byte 7:     Current Draw (0.1A units)
Byte 8-9:   Motor Position (raw encoder)
Byte 10:    Fault Flags
            Bit 0: Over-temperature
            Bit 1: Over-current
            Bit 2: Communication error
            Bit 3: Motor fault
            Bit 4-7: Reserved
Byte 11-63: Reserved
```

## Output Reports (Force Feedback)

### Set Constant Force (ID: 0x01)

```
Total Size: 8 bytes

Byte 0: Report ID (0x01)
Byte 1: Command (0x01 = Constant Force)
Byte 2: Force LSB (signed 16-bit)
Byte 3: Force MSB
        Range: -32768 to +32767
        Negative = Left, Positive = Right
Byte 4-7: Reserved (0x00)
```

### Set Periodic Effect (ID: 0x01)

```
Total Size: 16 bytes

Byte 0:  Report ID (0x01)
Byte 1:  Command (0x02 = Periodic)
Byte 2:  Effect Type
         0x01: Sine
         0x02: Square
         0x03: Triangle
         0x04: Sawtooth Up
         0x05: Sawtooth Down
Byte 3:  Magnitude (0-255)
Byte 4-5: Period (ms, 16-bit)
Byte 6-7: Phase (0-35999, 0.01° units)
Byte 8-9: Offset (signed 16-bit)
Byte 10-15: Reserved
```

### Set Condition Effect (ID: 0x01)

```
Total Size: 16 bytes

Byte 0:  Report ID (0x01)
Byte 1:  Command (0x03 = Condition)
Byte 2:  Effect Type
         0x01: Spring
         0x02: Damper
         0x03: Inertia
         0x04: Friction
Byte 3:  Positive Coefficient (0-255)
Byte 4:  Negative Coefficient (0-255)
Byte 5-6: Center Point (signed 16-bit)
Byte 7:  Dead Band (0-255)
Byte 8:  Saturation Positive (0-255)
Byte 9:  Saturation Negative (0-255)
Byte 10-15: Reserved
```

### Set Device Gain (ID: 0x01)

```
Total Size: 8 bytes

Byte 0: Report ID (0x01)
Byte 1: Command (0x10 = Set Gain)
Byte 2: Gain (0-100, percentage)
Byte 3-7: Reserved (0x00)
```

### Stop All Effects (ID: 0x01)

```
Total Size: 8 bytes

Byte 0: Report ID (0x01)
Byte 1: Command (0x0F = Stop All)
Byte 2-7: Reserved (0x00)
```

## Telemetry Output

### LED Control (ID: 0x08)

```
Total Size: 16 bytes

Byte 0:  Report ID (0x08)
Byte 1:  Command (0x80 = LED Control)
Byte 2:  LED Bitmask (Rev Lights)
         Bit 0: LED 1 (leftmost green)
         Bit 1: LED 2 (green)
         Bit 2: LED 3 (green)
         Bit 3: LED 4 (yellow)
         Bit 4: LED 5 (yellow)
         Bit 5: LED 6 (yellow)
         Bit 6: LED 7 (red)
         Bit 7: LED 8 (red)
Byte 3:  LED 9 (rightmost red, bit 0)
Byte 4:  Brightness (0-255)
Byte 5-15: Reserved
```

### Display Control (ID: 0x08)

```
Total Size: 16 bytes

Byte 0:  Report ID (0x08)
Byte 1:  Command (0x81 = Display)
Byte 2:  Display Mode
         0x00: Off
         0x01: Gear
         0x02: Speed
         0x03: RPM
         0x04: Custom
Byte 3:  Digit 1 (7-segment encoding)
Byte 4:  Digit 2
Byte 5:  Digit 3
Byte 6:  Decimal Point Position (0-3)
Byte 7:  Brightness (0-255)
Byte 8-15: Reserved

7-Segment Encoding:
  Bit 0: Segment A (top)
  Bit 1: Segment B (top-right)
  Bit 2: Segment C (bottom-right)
  Bit 3: Segment D (bottom)
  Bit 4: Segment E (bottom-left)
  Bit 5: Segment F (top-left)
  Bit 6: Segment G (middle)
  Bit 7: Decimal point
```

### Rumble Motors (ID: 0x08)

```
Total Size: 8 bytes

Byte 0: Report ID (0x08)
Byte 1: Command (0x82 = Rumble)
Byte 2: Left Motor (0-255)
Byte 3: Right Motor (0-255)
Byte 4: Duration (10ms units, 0 = continuous)
Byte 5-7: Reserved
```

## Feature Reports

### Get/Set Tuning Menu (ID: 0x10)

```
Report Type: Feature Report
Report ID: 0x10

GET Response / SET Payload:
Byte 0:  Report ID (0x10)
Byte 1:  Parameter ID
         0x01: SEN (Sensitivity/Rotation)
         0x02: FF (Force Feedback Strength)
         0x03: SHO (Shock/Vibration)
         0x04: ABS (Anti-lock simulation)
         0x05: DRI (Drift Mode)
         0x06: FOR (Force Effect Intensity)
         0x07: SPR (Spring Effect)
         0x08: DPR (Damper Effect)
         0x09: FEI (Force Effect Intensity)
         0x0A: INT (Interpolation Filter)
         0x0B: NDP (Natural Damper)
         0x0C: NFR (Natural Friction)
         0x0D: NIN (Natural Inertia)
Byte 2:  Value (0-100 or parameter-specific)
Byte 3-7: Reserved
```

### Firmware Update Mode (ID: 0xF0)

```
Report Type: Feature Report
Report ID: 0xF0

Payload:
Byte 0: 0xF0 (Report ID)
Byte 1: 0x01 (Enter Bootloader)
Byte 2-7: Magic bytes (device-specific)

WARNING: Incorrect use can brick the device!
```

## Implementation Notes

### Timing Requirements

- **Init Delay**: Wait 500ms after mode switch for re-enumeration
- **Effect Update Rate**: Maximum 1000 Hz
- **LED Update Rate**: Maximum 100 Hz
- **Display Update Rate**: Maximum 30 Hz

### Effect Limits

- **Maximum Simultaneous Effects**: 16
- **Effect Memory**: 4KB per effect slot
- **Envelope Support**: Yes, for all effect types

### Safety Features

- **Over-temperature Protection**: Motor shuts down at 80°C
- **Over-current Protection**: Limits at 150% rated current
- **Emergency Stop**: Send Stop All Effects command

### Platform Differences

| Platform | Driver | Notes |
|----------|--------|-------|
| Windows | Fanatec Driver | Full support, tuning menu |
| Linux | hid-fanatecff | FFB only, no tuning |
| macOS | None | Basic HID input only |

## Troubleshooting

### Device Not Detected

1. Check USB cable (use data cable, not charge-only)
2. Try different USB port (USB 3.0 recommended)
3. Power cycle the wheel base
4. Check for firmware updates

### FFB Not Working

1. Verify Advanced Mode is active
2. Check FFB strength in tuning menu
3. Ensure game supports Fanatec wheels
4. Update Fanatec drivers

### LED/Display Not Working

1. Verify rim is properly attached
2. Check rim firmware version
3. Ensure telemetry is enabled in game

## Resources

- **hid-fanatecff**: [https://github.com/gotzl/hid-fanatecff](https://github.com/gotzl/hid-fanatecff)
- **Fanatec Forum**: [https://forum.fanatec.com](https://forum.fanatec.com)
- **FanaLab**: Official Fanatec configuration software
- **Teensy Adapter**: [https://github.com/dchote/fanatecWheelUSB](https://github.com/dchote/fanatecWheelUSB)
