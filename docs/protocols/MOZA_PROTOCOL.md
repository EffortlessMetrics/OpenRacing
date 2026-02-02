# Moza Racing Protocol Documentation

**Status**: ✅ Fully Supported  
**Protocol Type**: Serial over USB / HID PIDFF

## Overview

Moza Racing wheelbases use a combination of serial-like communication encapsulated in USB HID and standard HID PID for force feedback. The devices generally work with standard PIDFF drivers after proper initialization, but require specific handshake commands to unlock full functionality including high-torque modes and telemetry features.

## Device Identification

### V1 vs V2 Hardware Revisions

Moza Racing wheelbases exist in two hardware revisions, distinguished by their Product ID ranges:

- **V1 Hardware** (PIDs `0x000x`): Original hardware revision with 15-bit encoder resolution
- **V2 Hardware** (PIDs `0x001x`): Updated hardware with higher encoder resolution (18/21-bit) and improved peripheral data aggregation

V2 PIDs follow the pattern of adding `0x0010` to the corresponding V1 PID. For example, R9 V1 uses `0x0002` while R9 V2 uses `0x0012`.

### Wheel Bases

| Model | Vendor ID | V1 PID | V2 PID | Max Torque | Notes |
|-------|-----------|--------|--------|------------|-------|
| R3 | `0x346E` | `0x0005` | `0x0015` | 3.9 Nm | Entry-level DD |
| R5 | `0x346E` | `0x0004` | `0x0014` | 5.5 Nm | Mid-range DD |
| R9 | `0x346E` | `0x0002` | `0x0012` | 9 Nm | High-end DD |
| R12 | `0x346E` | `0x0006` | `0x0016` | 12 Nm | Pro DD |
| R16/R21 | `0x346E` | `0x0000` | `0x0010` | 16-21 Nm | Top-tier/Flagship DD |

*Note: Vendor ID `0x346E` is registered to Moza Racing. Some databases may show `0x3416` (Lenovo) for older firmware versions.*

### Steering Wheels (Rims)

| Model | Rim ID | Notes |
|-------|--------|-------|
| CS V2 | `0x01` | Circular, standard |
| GS V2 | `0x02` | GT style |
| RS V2 | `0x03` | Round, rally |
| FSR | `0x04` | Formula style |
| KS | `0x05` | Butterfly |
| ES | `0x06` | Entry Level |

### KS Wheel Modes

The KS wheel supports two connection modes:

- **I2C Mode**: Connected through the wheelbase via the standard quick-release connection. The wheel appears as part of the wheelbase's composite device and its buttons/inputs are reported in the wheelbase's input report.

- **USB Mode**: When connected via a Universal Hub, the KS wheel can operate in standalone USB mode. This allows independent connection to the PC without requiring the wheelbase, useful for setups where the wheel is used with third-party equipment.

### Pedals

| Model | Vendor ID | Product ID | Notes |
|-------|-----------|------------|-------|
| SR-P | `0x346E` | `0x0003` | 2-pedal set (USB mode) |
| SR-P Lite | N/A | N/A | Analog via RJ11 to wheelbase (no separate USB PID) |
| CRP | `0x346E` | `0x0012` | 3-pedal, load cell |
| CRP2 | `0x346E` | `0x0013` | Updated load cell |

### SR-P Lite Integration

The SR-P Lite pedals connect via RJ11 cable directly to the wheelbase rather than using a separate USB connection:

- **Connection**: RJ11 cable to wheelbase's pedal port
- **Data Path**: Pedal axis data appears as analog axes within the wheelbase's input report
- **USB Enumeration**: No separate USB device; the wheelbase aggregates pedal data
- **Axis Mapping**: Throttle and brake axes are reported at the standard input report offsets (bytes 3-6)

### Other Peripherals

| Model | Vendor ID | Product ID | Notes |
|-------|-----------|------------|-------|
| HGP Shifter | `0x346E` | `0x0020` | H-pattern + sequential |
| SGP Sequential | `0x346E` | `0x0021` | Sequential only |
| HBP Handbrake | `0x346E` | `0x0022` | Analog handbrake |

## Initialization Sequence

### Standard Initialization

Moza devices typically work with standard HID PID drivers, but require initialization for full features:

```
Step 1: Query Device Info
Report Type: Feature Report (GET)
Report ID: 0x01

Response:
Byte 0: Report ID (0x01)
Byte 1: Device Type
Byte 2: Firmware Major
Byte 3: Firmware Minor
Byte 4: Firmware Patch
Byte 5-6: Serial (partial)

Step 2: Enable High-Torque Mode (if supported)
Report Type: Feature Report (SET)
Report ID: 0x02

Payload:
Byte 0: 0x02 (Report ID)
Byte 1: 0x01 (Command: Enable High Torque)
Byte 2: 0x01 (Enable)
Byte 3-7: Reserved (0x00)

Step 3: Start Input Reports
Report Type: Feature Report (SET)
Report ID: 0x03

Payload:
Byte 0: 0x03 (Report ID)
Byte 1: 0x01 (Command: Start)
Byte 2-7: Reserved (0x00)
```

### Set Rotation Range

```
Report Type: Feature Report
Report ID: 0x10

Payload:
Byte 0: 0x10 (Report ID)
Byte 1: 0x01 (Command: Set Range)
Byte 2-3: Range (16-bit, degrees)
          Common: 270, 360, 540, 720, 900, 1080

Response: ACK (0x10, 0x01, 0x00)
```

### Set Force Feedback Mode

```
Report Type: Feature Report
Report ID: 0x11

Payload:
Byte 0: 0x11 (Report ID)
Byte 1: Mode
        0x00: Off
        0x01: Standard (PIDFF)
        0x02: Direct (raw torque)
Byte 2-7: Reserved

Note: Direct mode bypasses PIDFF for custom FFB implementations
```

## Input Reports

### Standard Input Report (ID: 0x01)

```
Total Size: 32 bytes (standard) / 64 bytes (extended)

Byte 0:     Report ID (0x01)
Byte 1-2:   Steering Axis (16-bit, little-endian)
            Range: 0x0000 - 0xFFFF (center: 0x8000)
Byte 3-4:   Throttle (16-bit, 0x0000 = released)
Byte 5-6:   Brake (16-bit, 0x0000 = released)
Byte 7-8:   Clutch (16-bit, 0x0000 = released)
Byte 9-10:  Handbrake (16-bit, if connected)
Byte 11-26: Button Bitmap (128 buttons)
            Supports up to 128 buttons across 16 bytes
            Bit ordering: LSB first within each byte
            Byte 11 Bit 0:  Button 1 (varies by rim)
            Byte 11 Bit 1:  Button 2
            ...
            Byte 11 Bit 8:  Paddle Right
            Byte 11 Bit 9:  Paddle Left
            Byte 11 Bit 10: Funky Switch Press
            Byte 11 Bit 11: Rotary 1 Press
            Byte 11 Bit 12: Rotary 2 Press
            Byte 12-26:     Extended button bitmap (rim-specific)
Byte 27:    D-Pad / Hat Switch
            0x0: Up
            0x1: Up-Right
            0x2: Right
            0x3: Down-Right
            0x4: Down
            0x5: Down-Left
            0x6: Left
            0x7: Up-Left
            0x8: Neutral
Byte 28:    Funky Switch Direction
            0x00: Center
            0x01: Up
            0x02: Right
            0x03: Down
            0x04: Left
Byte 29:    Rotary 1 Position (0-255)
Byte 30:    Rotary 2 Position (0-255)
Byte 31:    Reserved (standard report ends here)

Extended Report (64 bytes):
Byte 32-33: Dual Clutch Left (FSR wheel)
Byte 34-35: Dual Clutch Right (FSR wheel)
Byte 36-63: Reserved / Rim-specific / V2 peripheral data
```

**Note on V2 Hardware**: V2 wheelbases use the 64-byte extended input report format by default and aggregate peripheral data at different byte offsets compared to V1. V2 hardware may include additional peripheral status and aggregated axis data in bytes 36-63.

### Extended Telemetry Report (ID: 0x02)

```
Total Size: 32 bytes

Byte 0:     Report ID (0x02)
Byte 1-2:   Motor Temperature (0.1°C units)
Byte 3-4:   Board Temperature (0.1°C units)
Byte 5-6:   Current Draw (mA)
Byte 7-8:   Voltage (mV)
Byte 9-12:  Motor Position (32-bit encoder count)
Byte 13-14: Motor Velocity (signed, RPM)
Byte 15:    Fault Flags
            Bit 0: Over-temperature
            Bit 1: Over-current
            Bit 2: Under-voltage
            Bit 3: Communication error
            Bit 4: Motor fault
            Bit 5: Encoder fault
            Bit 6-7: Reserved
Byte 16:    Operating Mode
Byte 17-31: Reserved
```

## Output Reports (Force Feedback)

### Standard PID Effects

Moza supports standard USB HID PID effects:

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
| Inertia | 0x42 | Full |
| Friction | 0x43 | Full |

### Direct Torque Mode (ID: 0x20)

For applications requiring direct motor control:

```
Report Type: Output Report
Report ID: 0x20

Payload:
Byte 0: Report ID (0x20)
Byte 1-2: Torque Command (signed 16-bit)
          Range: -32768 to +32767
          Maps to: -100% to +100% of max torque
Byte 3: Flags
        Bit 0: Enable motor
        Bit 1: Use slew rate limiting
        Bit 2-7: Reserved
Byte 4-5: Slew Rate (if enabled, Nm/s)
Byte 6-7: Reserved

Update Rate: Up to 1000 Hz
Latency: < 1ms typical
```

### Set Device Gain (ID: 0x21)

```
Report Type: Output Report
Report ID: 0x21

Payload:
Byte 0: Report ID (0x21)
Byte 1: Overall Gain (0-100, percentage)
Byte 2: Spring Gain (0-100)
Byte 3: Damper Gain (0-100)
Byte 4: Friction Gain (0-100)
Byte 5: Inertia Gain (0-100)
Byte 6-7: Reserved
```

## Telemetry Output

### LED Control (ID: 0x30)

```
Report Type: Output Report
Report ID: 0x30

Payload:
Byte 0:  Report ID (0x30)
Byte 1:  Command (0x01 = Rev Lights)
Byte 2:  LED Bitmask Low
         Bit 0: LED 1 (leftmost)
         Bit 1: LED 2
         Bit 2: LED 3
         Bit 3: LED 4
         Bit 4: LED 5
         Bit 5: LED 6
         Bit 6: LED 7
         Bit 7: LED 8
Byte 3:  LED Bitmask High
         Bit 0: LED 9
         Bit 1: LED 10 (rightmost)
         Bit 2-7: Reserved
Byte 4:  Color R (0-255)
Byte 5:  Color G (0-255)
Byte 6:  Color B (0-255)
Byte 7:  Brightness (0-255)
```

### Display Control (ID: 0x31)

```
Report Type: Output Report
Report ID: 0x31

Payload:
Byte 0:  Report ID (0x31)
Byte 1:  Display Mode
         0x00: Off
         0x01: Gear
         0x02: Speed
         0x03: RPM
         0x04: Lap Time
         0x05: Custom Text
Byte 2:  Data Byte 1 (mode-dependent)
Byte 3:  Data Byte 2
Byte 4:  Data Byte 3
Byte 5:  Data Byte 4
Byte 6:  Brightness (0-255)
Byte 7:  Reserved
```

## Feature Reports

### Get/Set Tuning Parameters (ID: 0x40)

```
Report Type: Feature Report
Report ID: 0x40

GET Response / SET Payload:
Byte 0:  Report ID (0x40)
Byte 1:  Parameter ID
         0x01: FFB Strength (0-100)
         0x02: Road Feel (0-100)
         0x03: Damping (0-100)
         0x04: Friction (0-100)
         0x05: Inertia (0-100)
         0x06: Spring (0-100)
         0x07: Speed Sensitivity (0-100)
         0x08: Torque Limit (0-100)
         0x09: Natural Damper (0-100)
         0x0A: Natural Friction (0-100)
         0x0B: Natural Inertia (0-100)
         0x0C: Hands-Off Detection (0/1)
         0x0D: Soft Lock (0/1)
Byte 2:  Value
Byte 3-7: Reserved
```

### Calibration (ID: 0x50)

```
Report Type: Feature Report
Report ID: 0x50

Commands:
Byte 1 = 0x01: Start Calibration
Byte 1 = 0x02: Save Calibration
Byte 1 = 0x03: Reset to Factory

GET Response (after calibration):
Byte 0:  Report ID (0x50)
Byte 1:  Status (0x00 = OK, 0x01 = In Progress, 0xFF = Error)
Byte 2-3: Center Position
Byte 4-5: Min Position
Byte 6-7: Max Position
```

## Implementation Notes

### Timing Requirements

- **Init Delay**: 100ms after power-on before sending commands
- **Effect Update Rate**: Up to 1000 Hz in Direct mode
- **Telemetry Poll Rate**: 100 Hz recommended
- **LED Update Rate**: 60 Hz maximum

### Effect Limits

- **Maximum Simultaneous Effects**: 16 (PIDFF mode)
- **Direct Mode**: Single torque command, unlimited rate
- **Envelope Support**: Full support in PIDFF mode

### Known Quirks

1. **Firmware Variations**: Protocol may differ between firmware versions
2. **Pit House Integration**: Some features require Pit House software
3. **USB Enumeration**: May enumerate as composite device
4. **Conditional Direction Inversion**: Spring, Damper, Friction, and Inertia effects have inverted positive/negative coefficients compared to the HID PID specification. When creating these effect types, swap the positive and negative coefficient values to achieve correct behavior.
5. **Vendor Usage Page**: Moza devices use custom HID usage pages for vendor-specific features. These non-standard usage pages may not be recognized by generic HID parsers.
6. **V2 Encoder Resolution**: V2 hardware features significantly higher encoder resolution (18-bit or 21-bit depending on model) compared to V1 hardware (15-bit). This provides finer positional accuracy but may require scaling adjustments when processing position data.

### Platform Differences

| Platform | Driver | Notes |
|----------|--------|-------|
| Windows | Moza Pit House | Full support |
| Linux | universal-pidff | PIDFF mode only |
| macOS | None | Basic HID only |

## Troubleshooting

### FFB Not Working

1. Verify device is in PIDFF mode (not Direct)
2. Check FFB strength in Pit House
3. Ensure game supports HID PID
4. Update firmware via Pit House

### High Latency

1. Use Direct mode for lowest latency
2. Disable USB power saving
3. Use USB 3.0 port
4. Check for USB hub issues

### Calibration Issues

1. Run calibration in Pit House
2. Check for mechanical binding
3. Verify encoder is functioning

## Resources

- **Boxflat**: [https://github.com/Lawstorant/boxflat](https://github.com/Lawstorant/boxflat) - Protocol documentation
- **Universal PIDFF**: [https://github.com/JacKeTUs/universal-pidff](https://github.com/JacKeTUs/universal-pidff)
- **Moza Pit House**: Official configuration software
- **Arduino Emulator**: [https://github.com/MikeSzklarz/Arduino-Moza-Emulator](https://github.com/MikeSzklarz/Arduino-Moza-Emulator)
