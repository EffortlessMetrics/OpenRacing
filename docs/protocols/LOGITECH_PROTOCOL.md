# Logitech Protocol Documentation

**Status**: ✅ Fully Supported  
**Protocol Type**: USB HID PID + TrueForce (proprietary)

## Overview

Logitech racing wheels use standard USB HID PID (Physical Interface Device) for force feedback, with proprietary extensions for advanced features like TrueForce haptics. All wheels require an initialization sequence to unlock full functionality.

## Device Identification

| Model | Vendor ID | Product ID | Max Rotation | Max Torque | Notes |
|-------|-----------|------------|--------------|------------|-------|
| G25 | `0x046D` | `0xC299` | 900° | 2.5 Nm | Legacy, discontinued |
| G27 | `0x046D` | `0xC29B` | 900° | 2.5 Nm | Legacy, discontinued |
| G29 (PS) | `0x046D` | `0xC24F` | 900° | 2.2 Nm | PlayStation/PC |
| G29 (Xbox) | `0x046D` | `0xC262` | 900° | 2.2 Nm | Xbox/PC variant |
| G920 | `0x046D` | `0xC262` | 900° | 2.2 Nm | Xbox/PC |
| G923 (PS) | `0x046D` | `0xC267` | 900° | 2.2 Nm | TrueForce, PlayStation/PC |
| G923 (Xbox) | `0x046D` | `0xC26E` | 900° | 2.2 Nm | TrueForce, Xbox/PC |
| Pro Racing Wheel | `0x046D` | `0xC272` | 1080° | 11 Nm | Direct Drive, TrueForce |

## Initialization Sequence

### Native Mode Activation

Logitech wheels boot in "Compatibility Mode" with limited rotation (200°) and basic FFB. To enable full functionality:

```
Report Type: Feature Report
Report ID: 0xF8 (Vendor Specific)

Payload Structure:
Byte 0: 0xF8 (Report ID)
Byte 1: 0x0A (Command: Set Native Mode)
Byte 2: 0x00
Byte 3: 0x00
Byte 4: 0x00
Byte 5: 0x00
Byte 6: 0x00
```

### Set Rotation Range

```
Report Type: Feature Report
Report ID: 0xF8

Payload Structure:
Byte 0: 0xF8 (Report ID)
Byte 1: 0x81 (Command: Set Range)
Byte 2: Range LSB (e.g., 0x84 for 900°)
Byte 3: Range MSB (e.g., 0x03 for 900°)
Byte 4: 0x00
Byte 5: 0x00
Byte 6: 0x00

Common Values:
- 200°: 0xC8, 0x00
- 540°: 0x1C, 0x02
- 900°: 0x84, 0x03
```

### Set Autocenter

```
Report Type: Feature Report
Report ID: 0xF8

Payload Structure:
Byte 0: 0xF8 (Report ID)
Byte 1: 0x14 (Command: Set Autocenter)
Byte 2: Strength (0x00-0xFF)
Byte 3: Rate (0x00-0xFF)
Byte 4: 0x00
Byte 5: 0x00
Byte 6: 0x00
```

## Input Reports

### Standard Input Report (ID: 0x01)

```
Total Size: 12 bytes

Byte 0:    Report ID (0x01)
Byte 1-2:  Steering Axis (16-bit, little-endian)
           Range: 0x0000 - 0xFFFF (center: 0x8000)
Byte 3:    Throttle (8-bit, 0x00 = released, 0xFF = pressed)
Byte 4:    Brake (8-bit, 0x00 = released, 0xFF = pressed)
Byte 5:    Clutch (8-bit, 0x00 = released, 0xFF = pressed)
Byte 6:    Buttons Low (8 bits)
           Bit 0: Cross/A
           Bit 1: Square/X
           Bit 2: Circle/B
           Bit 3: Triangle/Y
           Bit 4: R1/RB
           Bit 5: L1/LB
           Bit 6: R2/RT
           Bit 7: L2/LT
Byte 7:    Buttons High (8 bits)
           Bit 0: Share/Back
           Bit 1: Options/Start
           Bit 2: R3
           Bit 3: L3
           Bit 4: PS/Xbox
           Bit 5: Plus (+)
           Bit 6: Minus (-)
           Bit 7: Enter
Byte 8:    D-Pad (4-bit hat switch)
           0x0: Up, 0x1: Up-Right, 0x2: Right, ...
           0x8: Neutral
Byte 9:    Shifter Buttons
           Bit 0: Paddle Right (Upshift)
           Bit 1: Paddle Left (Downshift)
           Bit 2-7: Reserved
Byte 10-11: Reserved
```

## Output Reports (Force Feedback)

### Standard PID Effects

Logitech wheels support standard USB HID PID effects:

| Effect Type | Usage ID | Description |
|-------------|----------|-------------|
| Constant Force | 0x26 | Steady directional force |
| Ramp | 0x27 | Linearly changing force |
| Square | 0x30 | Square wave periodic |
| Sine | 0x31 | Sine wave periodic |
| Triangle | 0x32 | Triangle wave periodic |
| Sawtooth Up | 0x33 | Sawtooth up periodic |
| Sawtooth Down | 0x34 | Sawtooth down periodic |
| Spring | 0x40 | Position-dependent |
| Damper | 0x41 | Velocity-dependent |
| Inertia | 0x42 | Acceleration-dependent |
| Friction | 0x43 | Static resistance |

### Set Effect Report (ID: 0x11)

```
Total Size: 7 bytes

Byte 0: Report ID (0x11)
Byte 1: Effect Block Index (1-based)
Byte 2: Effect Type
        0x01: Constant
        0x02: Ramp
        0x03: Square
        0x04: Sine
        0x05: Triangle
        0x06: Sawtooth Up
        0x07: Sawtooth Down
        0x08: Spring
        0x09: Damper
        0x0A: Inertia
        0x0B: Friction
Byte 3: Duration LSB (ms)
Byte 4: Duration MSB (ms)
Byte 5: Trigger Button (0 = none)
Byte 6: Trigger Repeat Interval
```

### Set Constant Force Report (ID: 0x12)

```
Total Size: 4 bytes

Byte 0: Report ID (0x12)
Byte 1: Effect Block Index
Byte 2: Magnitude LSB (signed 16-bit)
Byte 3: Magnitude MSB
        Range: -10000 to +10000
        Negative = Left, Positive = Right
```

### Set Envelope Report (ID: 0x13)

```
Total Size: 9 bytes

Byte 0: Report ID (0x13)
Byte 1: Effect Block Index
Byte 2: Attack Level LSB
Byte 3: Attack Level MSB
Byte 4: Attack Time LSB (ms)
Byte 5: Attack Time MSB
Byte 6: Fade Level LSB
Byte 7: Fade Level MSB
Byte 8: Fade Time LSB (ms)
Byte 9: Fade Time MSB
```

### Set Condition Report (ID: 0x14)

```
Total Size: 12 bytes

Byte 0:  Report ID (0x14)
Byte 1:  Effect Block Index
Byte 2:  Parameter Block Offset (0 = X axis)
Byte 3:  Center Point Offset LSB
Byte 4:  Center Point Offset MSB
Byte 5:  Positive Coefficient LSB
Byte 6:  Positive Coefficient MSB
Byte 7:  Negative Coefficient LSB
Byte 8:  Negative Coefficient MSB
Byte 9:  Positive Saturation LSB
Byte 10: Positive Saturation MSB
Byte 11: Dead Band LSB
Byte 12: Dead Band MSB
```

### Effect Operation Report (ID: 0x15)

```
Total Size: 3 bytes

Byte 0: Report ID (0x15)
Byte 1: Effect Block Index
Byte 2: Operation
        0x01: Start
        0x02: Start Solo (stop others)
        0x03: Stop
```

### Device Gain Report (ID: 0x16)

```
Total Size: 2 bytes

Byte 0: Report ID (0x16)
Byte 1: Gain (0x00 - 0xFF)
        0x00 = 0%, 0xFF = 100%
```

## TrueForce Protocol (G923+)

TrueForce provides high-frequency haptic feedback synchronized with game audio.

> **⚠ UNVERIFIED — The protocol details below (report ID, packet format,
> endpoint, sample rate) are NOT confirmed by any public open-source source.**
> No open-source driver (Linux kernel, new-lg4ff, libhidpp, SDL) implements
> TrueForce. The only known implementation is in Logitech's proprietary
> G HUB software, which requires an NDA. The following details should be
> treated as speculative until independently verified through USB packet
> capture or official public documentation. See
> `crates/hid-logitech-protocol/src/ids.rs` for the verified public facts
> about G923 hardware and protocol support.

### TrueForce Endpoint

- **Endpoint**: Separate USB endpoint (typically EP 0x03)
- **Transfer Type**: Isochronous or Interrupt
- **Sample Rate**: 48 kHz (audio-rate)

### TrueForce Data Format

```
Total Size: 64 bytes per packet

Byte 0:    Packet Type (0x01 = Audio Data)
Byte 1:    Sequence Number
Byte 2-3:  Reserved
Byte 4-63: Audio Samples (30 x 16-bit signed PCM)
           Left channel = motor force
           Right channel = reserved
```

### TrueForce Initialization

```
Report Type: Feature Report
Report ID: 0xF5

Payload:
Byte 0: 0xF5 (Report ID)
Byte 1: 0x01 (Enable TrueForce)
Byte 2: 0x00
Byte 3: 0x00
```

## LED Control

### Rev Lights (G29/G920)

```
Report Type: Output Report
Report ID: 0xF8

Payload:
Byte 0: 0xF8 (Report ID)
Byte 1: 0x12 (Command: Set LEDs)
Byte 2: LED Bitmask
        Bit 0: LED 1 (leftmost)
        Bit 1: LED 2
        Bit 2: LED 3
        Bit 3: LED 4
        Bit 4: LED 5 (rightmost)
Byte 3: 0x00
Byte 4: 0x00
Byte 5: 0x00
Byte 6: 0x00
```

## Calibration

### Read Calibration Data

```
Report Type: Feature Report (GET)
Report ID: 0xF8

Response:
Byte 0: 0xF8
Byte 1: 0x88 (Calibration Data)
Byte 2-3: Center Position
Byte 4-5: Min Position
Byte 6-7: Max Position
```

### Set Calibration

```
Report Type: Feature Report (SET)
Report ID: 0xF8

Payload:
Byte 0: 0xF8
Byte 1: 0x89 (Set Calibration)
Byte 2-3: Center Position
Byte 4-5: Min Position
Byte 6-7: Max Position
```

## Implementation Notes

### Timing Requirements

- **Init Delay**: Wait 100ms after sending native mode command
- **Effect Update Rate**: Maximum 1000 Hz (1ms between updates)
- **LED Update Rate**: Maximum 60 Hz

### Error Handling

- **Disconnection**: Device may reset to compatibility mode on USB reset
- **Effect Overflow**: Maximum 16 simultaneous effects
- **Invalid Parameters**: Device ignores invalid effect parameters silently

### Platform Differences

| Platform | Driver | Notes |
|----------|--------|-------|
| Windows | Logitech G HUB | Full TrueForce support |
| Linux | new-lg4ff | Standard FFB, no TrueForce |
| macOS | None | Basic HID only |

## Resources

- **new-lg4ff**: [https://github.com/berarma/new-lg4ff](https://github.com/berarma/new-lg4ff)
- **lg4ff (kernel)**: Linux kernel driver source
- **libratbag**: [https://github.com/libratbag/libratbag](https://github.com/libratbag/libratbag)
- **G HUB SDK**: Logitech developer documentation (NDA required)
