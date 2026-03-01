# Simucube Protocol Documentation

**Status**: ✅ Fully Supported  
**Protocol Type**: USB HID PID (Standard) — plug-and-play, no initialization required

## Overview

Simucube 2 direct drive wheelbases (by Granite Devices) implement standard USB HID PID
force feedback. They enumerate as USB HID devices and are immediately ready for FFB after
USB connection — no proprietary handshake or initialization sequence is needed.

Simucube devices run at approximately 360 Hz (`bInterval = 3`) and use 64-byte HID reports.

**Vendor ID note:** Simucube 2 devices use VID `0x16D0` (registered to MCS Electronics /
OpenMoko), which is shared with Heusinkveld pedals. Devices are
distinguished by their product ID. Sources: Linux kernel `hid-ids.h` (`USB_VENDOR_ID_MCS =
0x16d0`), JacKeTUs/linux-steering-wheels compatibility table.

## Device Identification

### Wheel Bases

| Model | Vendor ID | Product ID | Max Torque | Encoder | Notes |
|-------|-----------|------------|------------|---------|-------|
| Simucube 1 | `0x16D0` | `0x0D5A` | varies | varies | IONI-based servo drive |
| Simucube 2 Sport | `0x16D0` | `0x0D61` | 17 Nm | 22-bit | Entry direct drive |
| Simucube 2 Pro | `0x16D0` | `0x0D60` | 25 Nm | 22-bit | Mid-tier |
| Simucube 2 Ultimate | `0x16D0` | `0x0D5F` | 32 Nm | 22-bit | Top-tier |

### Accessories

| Model | Vendor ID | Product ID | Notes |
|-------|-----------|------------|-------|
| Simucube SC-Link Hub (ActivePedal) | `0x16D0` | `0x0D66` | Load-cell force feedback pedal hub |
| SimuCube Wireless Wheel | `0x16D0` | `0x0D63` | Wireless steering wheel (PID estimated) |

## Initialization

**No initialization sequence is required.** The Simucube 2 is FFB-ready immediately upon
USB connection. Simply open the HID device and begin sending standard HID PID effect
reports.

## Output Reports (Force Feedback)

Simucube uses standard USB HID PID with 64-byte reports. Report ID is `0x01`.

```
Output Report Size: 64 bytes
Report ID byte: 0x01 (first byte)
bInterval: 3 (≈360 Hz update rate)
Encoder CPR: 4,194,304 (22-bit angle sensor)
```

### PID Effect Types Supported

| Effect Type | Support Level |
|-------------|---------------|
| Constant Force | Full |
| Ramp | Full |
| Spring | Full |
| Damper | Full |
| Inertia | Full |
| Friction | Full |
| Sine, Square, Triangle, Sawtooth | Full |

## Input Reports

Simucube reports steering position via the standard HID input report. The 22-bit encoder
provides extremely high angular resolution (~0.000086° per count).

## Implementation Notes

### Encoder Resolution

The Simucube 2 uses a 22-bit absolute angle sensor:

```
CPR = 4,194,304 (2^22)
Resolution ≈ 0.000086° per count
Full rotation = 4,194,304 counts
```

### Device Capabilities by Model

| Model | Max Torque | Peak Power | Report Rate |
|-------|-----------|------------|-------------|
| Sport | 17 Nm | ~1000 W | ~360 Hz |
| Pro | 25 Nm | ~2000 W | ~360 Hz |
| Ultimate | 32 Nm | ~3000 W | ~360 Hz |

### ActivePedal

The Simucube ActivePedal (PID `0x0D66`, via SC-Link Hub) is an active force-feedback pedal. It is **not**
a wheelbase and does not support motor torque FFB commands. Input-only on the force
feedback channel; it uses its own dedicated HID protocol for pedal feedback.

### ActivePedal Protocol Details

#### Connection Topology

The ActivePedal connects to the PC via **Simucube Link** (real-time Ethernet, RJ45)
through the **SC-Link Hub** (USB-C ↔ Simucube Link bridge). From the host PC's
perspective, the SC-Link Hub appears as a single USB HID device (VID `0x16D0`, PID
`0x0D66`). All ActivePedals and passive pedals behind the Hub are aggregated into
the Hub's HID report.

```
                  Simucube Link (RJ45)
ActivePedal ──────┐
ActivePedal ──────┤── Ethernet Switch ── SC-Link Hub ── USB-C ── PC
Co-Pedal    ──────┘        (optional)     (0x16D0:0x0D66)
```

- **Sub-millisecond latency** between ActivePedal and SC-Link Hub.
- **Single USB port** for unlimited Simucube Link devices.
- **Galvanic isolation** between all devices on the Link network.

#### Axis Reporting

Per the official Simucube developer documentation, the SC-Link Hub exposes:

| Axis | Type | Range | Notes |
|------|------|-------|-------|
| Pedal axes (up to 6) | Unsigned 16-bit | 0–65535 | Mapped via Simucube Tuner |
| Buttons | Bitmask | 128 total | Shared with wheelbase button space |

The ActivePedal reports its position as a standard HID axis (unsigned 16-bit,
0–65535). The axis can be configured in Simucube Tuner to function as brake,
throttle, or clutch. Value `0` = pedal released, `65535` = fully pressed.

#### Resolution

| Property | Value |
|----------|-------|
| Position sensor | Internal encoder (resolution not publicly documented) |
| USB HID report | 16-bit unsigned (0–65535) |
| Force range | Up to 170 kg (default), configurable to 120 kg |
| Travel range | 5–62 mm (default), configurable to 5–79 mm |

#### Passive Pedal Ports

Each ActivePedal has **two RJ12 (6P6C) passive pedal ports** with built-in load cell
amplifiers. Supported pedals (via adapter cable):

- Heusinkveld Sprint, Ultimate, Ultimate+
- Simucube Co-Pedal (passive throttle)
- Any standard 4-wire load cell pedal (1 kΩ typical)

Passive pedals connected to these ports are read by the ActivePedal's ADC and
aggregated into the SC-Link Hub's HID report. They do not enumerate as separate
USB devices.

#### Calibration

ActivePedal calibration is performed through **Simucube Tuner** software:
- Force-travel curve configuration (fully programmable).
- Per-profile settings (GT, Formula, etc.) with automatic profile switching.
- Damping, friction, and telemetry effects configuration.
- Passive pedal axis mapping and calibration.

No USB-level calibration protocol is exposed. Calibration settings are stored
on-device and in Tuner profiles.

#### Force Feedback Effects (Pedal FFB)

The ActivePedal generates its own force effects based on:
- **Pedal profile**: Configurable force-travel curve, damping, friction.
- **Game telemetry**: ABS, traction control, RPM, G-force effects.
- **API** (work in progress): Simucube has announced a public API for developers.

These effects run **on-device** — the host PC sends telemetry data, and the
ActivePedal firmware generates the haptic response locally.

#### Simucube 3 / Future

Simucube 3 wheelbases will also use Simucube Link. The SC-Link Hub bridges both
wheelbase and pedal traffic. ActivePedals are forward-compatible with the
Simucube 3 ecosystem.

## Resources

- **Simucube 2 SDK**: [https://github.com/SimuCUBE/SC2-sdk](https://github.com/SimuCUBE/SC2-sdk)
- **Granite Devices**: [https://granitedevices.com](https://granitedevices.com)
- **True Drive Software**: Simucube configuration application for Windows/Linux
