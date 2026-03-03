# Asetek SimSports Protocol Documentation

**Status**: ✅ Fully Supported  
**Protocol Type**: USB HID PID (Standard) — plug-and-play, no initialization required

## Overview

Asetek SimSports direct drive wheelbases implement standard USB HID PID force feedback.
Like Simucube, they are plug-and-play: no proprietary handshake or initialization
sequence is needed. The device is FFB-ready immediately after USB connection.

## Device Identification

| Model | Vendor ID | Product ID | Max Torque | Confidence | Notes |
|-------|-----------|------------|------------|------------|-------|
| Invicta | `0x2433` | `0xF300` | 27 Nm | ✅ Confirmed | Linux kernel, JacKeTUs, USB registries |
| Forte | `0x2433` | `0xF301` | 18 Nm | ✅ Confirmed | Linux kernel, JacKeTUs, USB registries |
| La Prima | `0x2433` | `0xF303` | 12 Nm | ✅ Confirmed | Linux kernel, JacKeTUs, asetek_wheelbase_cli |
| Tony Kanaan Edition | `0x2433` | `0xF306` | 27 Nm | ✅ Confirmed | Linux kernel, JacKeTUs |

**Vendor ID note:** VID `0x2433` is registered to Asetek A/S in the USB VID registry.
Confirmed by the-sz.com, devicehunt.com, and Linux `hid-ids.h`.

## Initialization

**No initialization sequence is required.** Asetek wheelbases are FFB-ready on USB
plug-in. Simply open the HID device and send standard HID PID effect reports.

## Output Reports (Force Feedback)

Asetek uses standard USB HID PID protocol. The output report does not use a separate
report ID byte prefix (the full HID report struct is used directly).

```
Encoder CPR: ~1,048,576 (estimated 20-bit)
Report rate: ~1 kHz
```

### PID Effect Types Supported

| Effect Type | Support Level |
|-------------|---------------|
| Constant Force | Full |
| Spring | Full |
| Damper | Full |
| Inertia | Full |
| Friction | Full |
| Periodic Effects | Full |

## Input Reports

Asetek wheels report steering position and button states via standard HID input reports
at up to 1 kHz.

## Device Capabilities by Model

| Model | Max Torque | Bearing | Notes |
|-------|-----------|---------|-------|
| Invicta | 27 Nm | Ball bearing | Premium model, 27 Nm peak |
| Forte | 18 Nm | Ball bearing | Mid-range |
| LaPrima | 12 Nm | Ball bearing | Entry |
| Tony Kanaan Edition | 27 Nm | Ball bearing | Invicta-based limited edition |

## Implementation Notes

### Encoder Resolution

Asetek wheelbases use an estimated 20-bit encoder:

```
CPR ≈ 1,048,576 (estimated 2^20)
```

### USB Power

Asetek wheelbases require adequate USB 3.0 power. A powered USB hub is recommended
for reliable operation on systems with limited USB bus power.

### Quick Release Compatibility

All three models support quick-release steering wheel connectors. Wheel rims are
hot-swappable when the wheelbase is powered.

## Resources

- **Asetek SimSports**: [https://asetek.com/simsports](https://asetek.com/simsports)
- **Asetek GitHub**: [https://github.com/asetek](https://github.com/asetek)

## Pedal Protocol Details

### Asetek Pedal Product Line

Asetek SimSports sells the **Invicta**, **Forte**, and **La Prima** as **wheelbase-only
products**. They do **not** include pedals. Asetek sells pedal sets as separate
accessories.

> 🔶 **Verification note (2025-07):** Asetek pedal USB VID/PIDs have not been
> confirmed from hardware captures. The pedals are assumed to share VID `0x2433`
> with the wheelbases (since all Asetek SimSports products use the Asetek
> registered VID), but the pedal Product IDs are unknown. No Asetek pedals appear
> in JacKeTUs/linux-steering-wheels, the Linux kernel `hid-ids.h`, or any public
> USB ID database.

### Known Asetek USB PIDs (Wheelbases Only)

| Model | VID | PID | Type | Confirmed |
|-------|-----|-----|------|-----------|
| Invicta (wheelbase) | `0x2433` | `0xF300` | Wheelbase | ✅ |
| Forte (wheelbase) | `0x2433` | `0xF301` | Wheelbase | ✅ |
| La Prima (wheelbase) | `0x2433` | `0xF303` | Wheelbase | ✅ |
| Tony Kanaan (wheelbase) | `0x2433` | `0xF306` | Wheelbase | ✅ |
| La Prima Steering Wheel | `0x2433` | `0xF203` | Wheel rim | ✅ (udev rules) |
| Pedals | `0x2433` | Unknown | Pedals | ⚠ Not confirmed |

### Pedal Connection Topology

Based on available information, Asetek pedals connect as **standalone USB HID
devices**. They do not aggregate through the wheelbase. Each pedal set has its own
USB connection and enumerates independently.

### Axis Reporting (Estimated)

Asetek pedals likely report axes via standard HID input reports, consistent with
their wheelbase approach (standard USB HID PID). Expected format:

- **Resolution:** 16-bit unsigned per axis (consistent with Asetek wheelbase HID approach).
- **Axes:** Throttle, Brake, Clutch (3-pedal sets).
- **Report rate:** Up to 1000 Hz (estimated, matching wheelbase poll rate).

**This section requires hardware capture validation.** No USB descriptor dumps
from Asetek pedals are publicly available.

### Calibration

Asetek pedals are configured through the **RaceHub** software (Windows only). RaceHub
communicates with devices over USB HID feature reports using a proprietary binary
protocol (see `moonrail/asetek_wheelbase_cli` for partial reverse-engineering of the
wheelbase configuration protocol).

No USB-level calibration protocol has been documented for pedals. Host-side
calibration via `PedalCalibrator` is recommended as a fallback.

### Capture Artifacts Required

Before finalizing Asetek pedal support:
- [ ] USB descriptor dump from at least one Asetek pedal set.
- [ ] Confirmed VID/PID pair for pedals.
- [ ] Report structure (report ID, axis offsets, byte order).
- [ ] Calibration protocol (if different from wheelbase config protocol).
