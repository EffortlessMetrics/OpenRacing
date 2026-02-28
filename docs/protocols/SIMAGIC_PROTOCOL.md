# Simagic Protocol Documentation

**Status**: ⚠️ Partial Support (Version Dependent)  
**Protocol Type**: HID PIDFF (Legacy) / Proprietary (Modern)

## Overview

Simagic protocols changed significantly between firmware versions. Legacy firmware (< v159) uses standard USB HID PID, while modern firmware (> v171) uses a proprietary obfuscated protocol. This documentation covers both versions.

**Important**: Modern Simagic firmware intentionally removes FFB descriptors from the USB descriptor, breaking compatibility with standard drivers. Full support requires reverse-engineering the SimPro Manager initialization sequence.

## Device Identification

Simagic has used three distinct USB VID/PID ranges across product generations:

| Generation | Vendor ID | Notes |
|------------|-----------|-------|
| Legacy (Alpha/M10/Alpha Mini/Alpha Ultimate) | `0x0483` | STMicroelectronics generic VID |
| EVO generation (Alpha EVO/EVO Sport/EVO Pro) | `0x3670` | Shen Zhen Simagic Technology Co., Ltd. |

VID `0x3670` is registered to Simagic directly (USB VID registry). The `hid-simagic-protocol`
crate targets EVO-generation devices (VID `0x3670`). Legacy devices (VID `0x0483`) use
standard HID PID handled by the `simagic-ff` kernel module.

### Wheel Bases

| Model | Vendor ID | Product ID | Max Torque | Generation | Notes |
|-------|-----------|------------|------------|------------|-------|
| M10 | `0x0483` | `0x0522` | 10 Nm | Legacy | Shared PID with Alpha/Alpha Mini |
| Alpha Mini | `0x0483` | `0x0522` | 10 Nm | Legacy | Same PID as M10 |
| Alpha | `0x0483` | `0x0522` | 15 Nm | Legacy | Same PID |
| Alpha U | `0x0483` | `0x0523` | 15 Nm | Legacy | Updated Alpha |
| Alpha Ultimate | `0x0483` | `0x0524` | 23 Nm | Legacy | Flagship |
| EVO Sport | `0x3670` | `0x0500` | — | EVO | Verified: linux-steering-wheels |
| EVO | `0x3670` | `0x0501` | — | EVO | Verified: linux-steering-wheels |
| EVO Pro | `0x3670` | `0x0502` | — | EVO | Verified: linux-steering-wheels |

### Steering Wheels (Rims)

| Model | Rim ID | Notes |
|-------|--------|-------|
| GT1 | `0x01` | Round GT style |
| GT4 | `0x02` | GT4 replica |
| FX | `0x03` | Formula style |
| GTC | `0x04` | GT Cup style |
| GT Neo | `0x05` | Modern GT |

### Pedals

| Model | Vendor ID | Product ID | Notes |
|-------|-----------|------------|-------|
| P-HPR | `0x0483` | `0x0530` | Hydraulic |
| P1000 | `0x0483` | `0x0531` | Load cell |
| P2000 | `0x0483` | `0x0532` | Premium load cell |

## Protocol Versions

### Legacy Protocol (Firmware < v159)

Standard USB HID PID with minor quirks:

- **Descriptor Issues**: Missing some effect type descriptors
- **Workaround**: Patch descriptor to add missing effect types
- **Compatibility**: Works with generic HID drivers after patching

### Modern Protocol (Firmware > v171)

Proprietary obfuscated protocol:

- **Descriptor Stripping**: FFB descriptors removed from USB descriptor
- **Report Shifting**: All report data shifted by 1 byte (byte 0 = 0x01)
- **Hardcoded Effects**: Effect types hardcoded in SimPro Manager
- **Initialization**: Requires specific init sequence from SimPro

## Legacy Protocol Details

### Initialization (Legacy)

```
Step 1: Query Device
Report Type: Feature Report (GET)
Report ID: 0x01

Response:
Byte 0: Report ID
Byte 1: Device Type
Byte 2-3: Firmware Version
Byte 4-7: Serial Number

Step 2: Enable FFB
Report Type: Feature Report (SET)
Report ID: 0x02

Payload:
Byte 0: 0x02 (Report ID)
Byte 1: 0x01 (Enable FFB)
Byte 2-7: Reserved (0x00)
```

### Input Report (Legacy, ID: 0x01)

```
Total Size: 16 bytes

Byte 0:     Report ID (0x01)
Byte 1-2:   Steering Axis (16-bit)
Byte 3:     Throttle (8-bit)
Byte 4:     Brake (8-bit)
Byte 5:     Clutch (8-bit)
Byte 6-7:   Buttons (16 bits)
Byte 8:     D-Pad
Byte 9-15:  Reserved
```

### FFB Effects (Legacy)

Standard PID effects supported:

| Effect Type | Usage ID | Support |
|-------------|----------|---------|
| Constant Force | 0x26 | Full |
| Ramp | 0x27 | Full |
| Square | 0x30 | Full |
| Sine | 0x31 | Full |
| Triangle | 0x32 | Full |
| Spring | 0x40 | Full |
| Damper | 0x41 | Full |
| Friction | 0x43 | Full |

## Modern Protocol Details

### Initialization (Modern)

The modern protocol requires sniffing the SimPro Manager initialization:

```
Step 1: Send Magic Handshake
Report Type: Feature Report (SET)
Report ID: 0x01

Payload (observed from SimPro):
Byte 0: 0x01 (Report ID)
Byte 1: 0x53 ('S')
Byte 2: 0x49 ('I')
Byte 3: 0x4D ('M')
Byte 4: 0x41 ('A')
Byte 5: 0x47 ('G')
Byte 6: 0x49 ('I')
Byte 7: 0x43 ('C')

Step 2: Wait for ACK (100ms timeout)

Step 3: Enable Advanced Mode
Report Type: Feature Report (SET)
Report ID: 0x02

Payload:
Byte 0: 0x02
Byte 1: 0x01 (Enable)
Byte 2-7: Device-specific magic bytes
```

### Input Report (Modern, ID: 0x01)

```
Total Size: 32 bytes

Byte 0:     Report ID (0x01)
Byte 1:     Padding (0x01) - SHIFTED!
Byte 2-3:   Steering Axis (16-bit)
Byte 4-5:   Throttle (16-bit)
Byte 6-7:   Brake (16-bit)
Byte 8-9:   Clutch (16-bit)
Byte 10-11: Buttons Low (16 bits)
Byte 12-13: Buttons High (16 bits)
Byte 14:    D-Pad
Byte 15:    Rotary 1
Byte 16:    Rotary 2
Byte 17-31: Reserved / Rim-specific
```

### FFB Output (Modern, ID: 0x01)

```
Total Size: 16 bytes

Byte 0:  Report ID (0x01)
Byte 1:  Padding (0x01) - SHIFTED!
Byte 2:  Command
         0x01: Constant Force
         0x02: Periodic
         0x03: Condition
         0x04: Stop All
Byte 3:  Effect Index
Byte 4-5: Parameter 1 (effect-dependent)
Byte 6-7: Parameter 2
Byte 8-9: Parameter 3
Byte 10-15: Reserved
```

### Constant Force (Modern)

```
Byte 2:  0x01 (Constant Force)
Byte 3:  Effect Index (1-16)
Byte 4-5: Magnitude (signed 16-bit, shifted)
          Range: -32768 to +32767
Byte 6-7: Duration (ms, 0xFFFF = infinite)
```

### Periodic Effect (Modern)

```
Byte 2:  0x02 (Periodic)
Byte 3:  Effect Index
Byte 4:  Effect Type
         0x01: Sine
         0x02: Square
         0x03: Triangle
         0x04: Sawtooth
Byte 5:  Magnitude (0-255)
Byte 6-7: Period (ms)
Byte 8-9: Phase (0-35999)
```

### Condition Effect (Modern)

```
Byte 2:  0x03 (Condition)
Byte 3:  Effect Index
Byte 4:  Effect Type
         0x01: Spring
         0x02: Damper
         0x03: Friction
Byte 5:  Positive Coefficient
Byte 6:  Negative Coefficient
Byte 7-8: Center Point
Byte 9:  Dead Band
```

## Telemetry Output

### LED Control (ID: 0x10)

```
Report Type: Output Report
Report ID: 0x10

Payload:
Byte 0:  Report ID (0x10)
Byte 1:  Padding (0x01) - Modern only
Byte 2:  LED Bitmask
Byte 3:  Brightness (0-255)
Byte 4-7: Reserved
```

### Display Control (ID: 0x11)

```
Report Type: Output Report
Report ID: 0x11

Payload:
Byte 0:  Report ID (0x11)
Byte 1:  Padding (0x01) - Modern only
Byte 2:  Display Mode
Byte 3-5: Display Data
Byte 6:  Brightness
Byte 7:  Reserved
```

## Feature Reports

### Get Device Info (ID: 0x80)

```
Report Type: Feature Report (GET)
Report ID: 0x80

Response:
Byte 0:  Report ID (0x80)
Byte 1:  Device Type
Byte 2:  Firmware Major
Byte 3:  Firmware Minor
Byte 4:  Firmware Patch
Byte 5:  Protocol Version
         0x01: Legacy
         0x02: Modern
Byte 6-7: Max Torque (0.1 Nm units)
```

### Get/Set Tuning (ID: 0x81)

```
Report Type: Feature Report
Report ID: 0x81

Payload:
Byte 0:  Report ID (0x81)
Byte 1:  Parameter ID
         0x01: FFB Strength
         0x02: Damping
         0x03: Friction
         0x04: Inertia
         0x05: Spring
         0x06: Road Feel
         0x07: Torque Limit
Byte 2:  Value (0-100)
Byte 3-7: Reserved
```

## Implementation Strategy

### Legacy Support

1. Detect firmware version via Feature Report 0x80
2. If Legacy (< v159), use standard HID PID
3. Patch descriptor if needed for missing effects

### Modern Support

1. Detect firmware version
2. If Modern (> v171), attempt SimPro handshake
3. Use shifted report format for all communication
4. Fall back to basic input-only mode if handshake fails

### Descriptor Patching (Legacy)

```rust
// Pseudo-code for descriptor patching
fn patch_simagic_descriptor(desc: &mut [u8]) {
    // Find PID usage page
    // Add missing effect type usages
    // Recalculate descriptor length
}
```

## Implementation Notes

### Timing Requirements

- **Init Delay**: 500ms after USB enumeration
- **Handshake Timeout**: 100ms for ACK
- **Effect Update Rate**: 500 Hz maximum
- **Report Polling**: 1000 Hz

### Known Issues

1. **Firmware Detection**: No reliable way to detect version without trying
2. **Protocol Switching**: Device may switch protocols after firmware update
3. **SimPro Dependency**: Full modern support requires SimPro analysis

### Platform Differences

| Platform | Driver | Notes |
|----------|--------|-------|
| Windows | SimPro Manager | Full support (modern) |
| Linux | simagic-ff | Legacy only |
| macOS | None | Basic HID only |

## Troubleshooting

### FFB Not Working (Modern)

1. Check firmware version in SimPro
2. Verify handshake sequence
3. Check for shifted byte format
4. Try downgrading firmware to legacy

### Device Not Detected

1. Check USB connection
2. Verify VID/PID matches
3. Try different USB port
4. Check for driver conflicts

### Inconsistent Behavior

1. Firmware version may have changed
2. Check SimPro for updates
3. Verify protocol version detection

## Reverse Engineering Notes

### USB Traffic Analysis

To capture the SimPro initialization:

1. Install USBPcap
2. Start Wireshark capture on Simagic device
3. Launch SimPro Manager
4. Capture the init sequence
5. Document the magic bytes

### Key Observations

- Modern protocol adds 0x01 padding byte after Report ID
- Effect types are not in USB descriptor
- SimPro sends "SIMAGIC" string as handshake
- ACK response contains device capabilities

## Resources

- **simagic-ff (Legacy)**: [https://github.com/JacKeTUs/simagic-ff](https://github.com/JacKeTUs/simagic-ff)
- **linux-steering-wheels**: [https://github.com/JacKeTUs/linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels)
- **Discussion Thread**: [https://github.com/JacKeTUs/linux-steering-wheels/issues/3](https://github.com/JacKeTUs/linux-steering-wheels/issues/3)
- **SimPro Manager**: Official Simagic configuration software
