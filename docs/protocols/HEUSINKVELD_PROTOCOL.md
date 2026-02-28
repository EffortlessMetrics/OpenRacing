# Heusinkveld Protocol Documentation

**Status**: ✅ Fully Supported (Input-only; no force feedback)  
**Protocol Type**: USB HID Input — plug-and-play, no initialization required

## Overview

Heusinkveld pedal sets are high-precision sim racing pedals that communicate as standard
USB HID input devices. They do **not** support force feedback and require no initialization.
All three models (Sprint, Ultimate+, Pro) enumerate and report pedal positions immediately
on USB connection.

Heusinkveld products use VID `0x16D0` (OpenMoko), shared with legacy Simagic devices.
They are distinguished by Product IDs in the `0x115x` range.

## Device Identification

| Model | Vendor ID | Product ID | Pedals | Max Load |
|-------|-----------|------------|--------|----------|
| Sprint | `0x16D0` | `0x1156` | 2 (throttle + brake) | 55 kg brake |
| Ultimate+ | `0x16D0` | `0x1157` | 3 (throttle + brake + clutch) | 140 kg brake |
| Pro | `0x16D0` | `0x1158` | 3 (throttle + brake + clutch) | 200 kg brake |

## VID Disambiguation

VID `0x16D0` is shared with legacy Simagic devices (PIDs `0x0D5A`, `0x0D5B`).
Heusinkveld products are identified by PIDs in the `0x115x` range.

```
VID 0x16D0 + PID 0x1156..=0x1158 → Heusinkveld
VID 0x16D0 + other PIDs          → Simagic legacy
```

## Initialization

**No initialization required.** Heusinkveld pedals are plug-and-play. Connect the USB
cable and the device immediately reports pedal positions using standard HID input reports.

## Input Reports

Heusinkveld pedals use high-resolution (12–16 bit) ADC sensors. They report axes as
signed integer values in the standard HID axis format.

### Sprint (2 pedals)

```
Axes: Throttle (Z axis), Brake (Rz axis)
Brake sensor: Strain gauge load cell, 55 kg max
```

### Ultimate+ and Pro (3 pedals)

```
Axes: Throttle (Z), Brake (Rz), Clutch (Slider or Rx)
Brake sensor: Strain gauge load cell
Ultimate+: 140 kg max brake
Pro: 200 kg max brake
```

## Force Feedback

**Heusinkveld pedals do not support force feedback.** They are input-only devices.
The `initialize_device` and `shutdown_device` methods are no-ops; no FFB reports
should be sent to these devices.

## Implementation Notes

### Sensor Technology

| Model | Brake Sensor | Throttle/Clutch |
|-------|-------------|-----------------|
| Sprint | Strain gauge | Elastomer |
| Ultimate+ | Strain gauge (140 kg) | Elastomer / adjustable |
| Pro | Strain gauge (200 kg) | Hall effect magnetic |

### Calibration

Heusinkveld pedals are calibrated through the **Heusinkveld Engineering Utility**
(Windows/macOS). Calibration settings are stored in EEPROM on the pedal controller
and do not require host-side processing.

### Update Rate

Heusinkveld pedals report at up to 1000 Hz (1 ms USB poll interval).

## Resources

- **Heusinkveld Engineering**: [https://heusinkveld.com](https://heusinkveld.com)
- **Heusinkveld Utility**: Available from [https://heusinkveld.com/downloads](https://heusinkveld.com/downloads)
