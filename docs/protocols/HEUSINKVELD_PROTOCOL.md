# Heusinkveld Protocol Documentation

**Status**: ✅ Fully Supported (Input-only; no force feedback)  
**Protocol Type**: USB HID Input — plug-and-play, no initialization required

## Overview

Heusinkveld pedal sets are high-precision sim racing pedals that communicate as standard
USB HID input devices. They do **not** support force feedback and require no initialization.
All three models (Sprint, Ultimate+, Pro) enumerate and report pedal positions immediately
on USB connection.

Heusinkveld products use VID `0x04D8` (Microchip Technology), reflecting their
PIC-based USB firmware. They are distinguished by Product IDs in the `0xF6Dx` range.

## Device Identification

> 🔶 **Verification note (web-verified 2025-07):** The PIDs listed below are
> sourced from the OpenFlight sister project (`EffortlessMetrics/OpenFlight`)
> community device manifests. They have **zero presence** in any public USB ID
> database (the-sz.com, devicehunt.com), Linux kernel (`hid-ids.h`), SDL
> (`usb_ids.h`), or JacKeTUs/linux-steering-wheels. The Pro PID (`0xF6D3`) is
> estimated from the sequential pattern and has **zero external evidence**.
> See `crates/hid-heusinkveld-protocol/src/ids.rs` for a full source audit.

| Model | Vendor ID | Product ID | Pedals | Max Load | Confidence |
|-------|-----------|------------|--------|----------|------------|
| Sprint | `0x04D8` | `0xF6D0` | 2 (throttle + brake) | 55 kg brake | 🔶 Community |
| Ultimate+ | `0x04D8` | `0xF6D2` | 3 (throttle + brake + clutch) | 140 kg brake | 🔶 Community |
| Pro | `0x04D8` | `0xF6D3` | 3 (throttle + brake + clutch) | 200 kg brake | ⚠ Estimated |

## VID Disambiguation

VID `0x04D8` is the generic Microchip Technology vendor ID, used by many PIC-based
devices. Heusinkveld products are identified by PIDs in the `0xF6D0`–`0xF6D3` range.

```
VID 0x04D8 + PID 0xF6D0..=0xF6D3 → Heusinkveld
VID 0x04D8 + other PIDs           → unknown (not dispatched)
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

## Pedal Protocol Details

### Connection Topology

Heusinkveld pedals are **standalone USB devices**. Each pedal set enumerates as its own
USB HID device with a unique VID/PID pair — they are never connected through a
wheelbase. Pedals connect directly to the PC via a USB cable (typically USB 2.0 Full
Speed). Each model has a distinct Product ID (see Device Identification above).

### Axis Reporting

Pedal axes are reported via standard HID input reports using the Generic Desktop
usage page. Each axis is a 16-bit unsigned value.

| Axis | HID Usage | Bit Depth | Range | Notes |
|------|-----------|-----------|-------|-------|
| Throttle | Z (Usage `0x32`) | 16-bit | `0x0000`–`0xFFFF` | 0 = released |
| Brake | Rz (Usage `0x35`) | 16-bit | `0x0000`–`0xFFFF` | 0 = released |
| Clutch | Slider or Rx | 16-bit | `0x0000`–`0xFFFF` | 3-pedal sets only |

The internal ADC resolution is 12–16 bit depending on the model and sensor type.
Strain gauge (load cell) channels use higher-resolution ADC sampling than
elastomer/hall effect channels. The USB HID report always exposes the full 16-bit
range regardless of internal ADC resolution.

### Resolution by Sensor Type

| Sensor | Internal ADC | USB Report | Effective Resolution |
|--------|-------------|------------|---------------------|
| Strain gauge (load cell) | 16-bit | 16-bit | Full 65536 counts |
| Hall effect magnetic | 12-bit | 16-bit (upscaled) | ~4096 effective counts |
| Elastomer (potentiometer) | 12-bit | 16-bit (upscaled) | ~4096 effective counts |

### Calibration Protocol

Heusinkveld pedals store calibration in **on-device EEPROM**. There is no host-side
calibration protocol over USB — calibration is performed entirely through the
**Heusinkveld Engineering Utility** (Windows/macOS desktop application).

The calibration workflow:
1. User opens Heusinkveld Engineering Utility.
2. Utility communicates with the pedal controller via USB HID feature reports.
3. User sweeps pedals through full range; utility captures min/max values.
4. Calibration values are written to EEPROM on the pedal controller MCU.
5. After calibration, the pedal reports normalized 16-bit values autonomously.

**Host-side calibration** (OpenRacing `PedalCalibrator`): Because on-device calibration
may not always reflect the user's desired range, OpenRacing also supports software
min/max calibration via `openracing-calibration::PedalCalibrator`. This collects raw
16-bit samples and computes per-axis min/max for normalization to `0.0`–`1.0`.

### Simucube ActivePedal Integration

Heusinkveld Sprint, Ultimate, and Ultimate+ pedals can also connect to the Simucube
ActivePedal's passive pedal ports (RJ12 6P6C) via a separately sold adapter cable.
When connected this way, the pedal's load cell signal is read by the ActivePedal's
built-in amplifier and routed through Simucube Link. The pedal does **not** appear as
a separate USB device in this topology — it is aggregated into the SC-Link Hub's
HID report. See `SIMUCUBE_PROTOCOL.md` for details.

## Resources

- **Heusinkveld Engineering**: [https://heusinkveld.com](https://heusinkveld.com)
- **Heusinkveld Utility**: Available from [https://heusinkveld.com/downloads](https://heusinkveld.com/downloads)
- **Simucube ActivePedal compatibility**: [https://docs.simucube.com/ActivePedal/Specifications](https://docs.simucube.com/ActivePedal/Specifications)
