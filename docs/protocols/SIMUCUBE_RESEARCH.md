# Simucube Protocol Research

**Date**: 2025-07  
**Status**: Research complete  
**Researcher**: Automated research via public sources

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Device Identification (PIDs)](#device-identification)
3. [Communication Protocol](#communication-protocol)
4. [HID Report Structure](#hid-report-structure)
5. [Force Feedback Protocol](#force-feedback-protocol)
6. [Rotation Range Setting](#rotation-range-setting)
7. [Simucube 1 (IONI) vs Simucube 2](#simucube-1-vs-simucube-2)
8. [Simucube Link / Simucube 3](#simucube-link)
9. [Open-Source Implementations](#open-source-implementations)
10. [Corrections Needed in Our Crate](#corrections-needed)
11. [Sources](#sources)

---

## Executive Summary

Simucube wheelbases (by Granite Devices Oy, Finland) use **standard USB HID PID
(Physical Interface Device)** for force feedback. They do NOT use a proprietary binary
protocol — they implement the USB-IF HID PID specification (Document `pid1_01.pdf`),
which is the same standard used by many other direct drive wheelbases (Moza, Asetek,
VRS DirectForce Pro, FFBeast, etc.).

On Windows, FFB is accessed via **DirectInput** (which natively understands HID PID).
On Linux, the **`hid-pidff`** kernel driver handles FFB. No custom driver or
initialization handshake is needed.

**Key correction**: Our current `hid-simucube-protocol` crate implements a custom
binary report layout that does NOT match the actual Simucube protocol. The device
uses standard HID PID reports, not a proprietary byte-level format.

---

## Device Identification

### Confirmed PIDs (✅ VERIFIED — Official Source)

All PIDs below are confirmed via the **official Simucube developer documentation**
at `Simucube/simucube-docs.github.io → docs/Simucube 2/Developers.md` and
cross-verified with Granite Devices wiki udev rules and `JacKeTUs/linux-steering-wheels`.

| Product | VID | PID | Windows Name | Confidence |
|---------|-----|-----|-------------|------------|
| Simucube 1 (IONI) | `0x16D0` | `0x0D5A` | SimuCUBE | ✅ Official docs + udev rules |
| Simucube 2 Sport | `0x16D0` | `0x0D61` | Simucube 2 Sport | ✅ Official docs + udev rules |
| Simucube 2 Pro | `0x16D0` | `0x0D60` | Simucube 2 Pro | ✅ Official docs + udev rules |
| Simucube 2 Ultimate | `0x16D0` | `0x0D5F` | Simucube 2 Ultimate | ✅ Official docs + udev rules |
| SC-Link Hub (ActivePedal) | `0x16D0` | `0x0D66` | SC-Link | ✅ Official docs |

### Firmware Upgrade PIDs (not for normal enumeration)

| Product | VID | PID | Confidence |
|---------|-----|-----|------------|
| Simucube 2 FW upgrade mode | `0x16D0` | `0x0D5E` | ✅ udev rules (Granite Devices wiki) |
| Simucube 1 FW upgrade mode | `0x16D0` | `0x0D5B` | ✅ udev rules (Granite Devices wiki) |

### Unconfirmed PIDs

| Product | VID | PID | Notes | Confidence |
|---------|-----|-----|-------|------------|
| Wireless Wheel | `0x16D0` | `0x0D63` | **NOT in official PID table**; our crate uses this value but it is speculative | ⚠️ Unverified |

### Windows guiProduct ID Format

The official docs specify a Windows GUID pattern for device matching:
```
{PPPP16D0-0000-0000-0000-504944564944}
```
where `PPPP` is the PID in uppercase hex. The suffix `504944564944` decodes to ASCII
`PIDVID`, confirming this is a standard DirectInput device GUID format.

Example: Simucube 2 Pro → `{0D6016D0-0000-0000-0000-504944564944}`

### VID Note

VID `0x16D0` is registered to **MCS Electronics / OpenMoko** (not Granite Devices).
Multiple sim racing vendors share this VID (e.g. Heusinkveld pedals also use `0x16D0`).
Source: Linux kernel `hid-ids.h` (`USB_VENDOR_ID_MCS = 0x16d0`).

---

## Communication Protocol

### Protocol Type: Standard USB HID PID

**CONFIRMED**: Simucube uses the **USB HID PID (Physical Interface Device)**
specification for force feedback. This is a USB-IF standard, not a proprietary protocol.

Evidence:
- On Linux, Simucube works with the kernel's generic `hid-pidff` driver — no custom
  driver needed (Source: JacKeTUs/linux-steering-wheels, rated "Silver")
- Firmware 1.0.49 changelog: "USB descriptor was tuned to make device compatible with
  Linux" and "Re-tuned DirectInput effects [...] as per USB Physical Device standards"
  (Source: Granite Devices wiki firmware releases)
- Firmware 1.0.49 fixed the `0xa7` (effect delay) HID descriptor that was missing,
  which is required by the Linux `hid-pidff` driver (Source: JacKeTUs/linux-steering-wheels)
- SDL (libsdl-org/SDL) recognizes Simucube VID/PID as a standard joystick

### USB Parameters

| Parameter | Value | Source |
|-----------|-------|--------|
| USB class | HID | Confirmed (HIDRAW on Linux) |
| bInterval | 3 | SIMUCUBE_PROTOCOL.md (device observation) |
| Report rate | ~333 Hz default, up to 360 Hz (iRacing mode) | Firmware 1.0.60 changelog |
| Report size | 64 bytes | Device observation |

### Platform Support

- **Windows**: DirectInput (native HID PID → DirectInput mapping by Windows)
- **Linux**: `hid-pidff` kernel driver (Linux ≥6.12.24 or ≥6.15 recommended)
- **Wine/Proton**: Works via HIDRAW (Proton has HIDRAW enabled by default; vanilla
  Wine may need manual registry configuration)

---

## HID Report Structure

### Input Reports (Device → Host)

Per the **official Simucube developer documentation**, the USB HID input report
contains:

| Field | Type | Range | Notes |
|-------|------|-------|-------|
| X axis (steering) | Unsigned 16-bit | 0–65535 | Wheel position |
| Y axis | Unsigned 16-bit | 0–65535 | Idles at center; user-mappable to pedal/handbrake |
| 6 additional axes | Unsigned 16-bit each | 0–65535 | Pedals, handbrakes, clutch paddles from wireless wheels |
| 128 buttons | Bitmask | 0/1 | Physical buttons + wireless wheel buttons |

**IMPORTANT**: The 22-bit encoder resolution (4,194,304 CPR) is the **internal**
sensor resolution of the servo motor. The USB HID report exposes the wheel position
as a **16-bit unsigned value** (0–65535), which is the standard HID axis format. The
device firmware maps the 22-bit internal position to the 16-bit USB axis value based
on the configured rotation range.

This means:
- At 900° rotation range: 65536 counts / 900° ≈ 72.8 counts per degree
- At 360° rotation range: 65536 counts / 360° ≈ 182 counts per degree
- The internal 22-bit resolution provides smooth interpolation but is NOT directly
  exposed via USB

### Output Reports (Host → Device) — HID PID Standard

Simucube uses **standard HID PID output reports**, NOT a proprietary format. The HID
PID protocol defines structured reports for:

1. **Create New Effect** — allocates an effect slot on the device
2. **Set Effect Report** — configures effect type, duration, gain, direction, etc.
3. **Set Envelope Report** — attack/fade parameters
4. **Set Condition Report** — spring/damper/friction coefficients
5. **Set Periodic Report** — sine/square/triangle/sawtooth parameters
6. **Set Constant Force Report** — constant force magnitude
7. **Set Ramp Force Report** — ramp start/end values
8. **Effect Operation Report** — start/stop/solo effects
9. **Device Gain Report** — global device gain (0–100%)
10. **PID Block Free Report** — release an effect slot

All of these follow the USB HID PID 1.0 specification. Report IDs are defined in
the device's HID report descriptor (obtained during USB enumeration).

### Report Size

Reports are 64 bytes (padded). This is the HID max report size configured by the
device. Individual PID reports may use fewer bytes with the remainder zero-padded.

---

## Force Feedback Protocol

### Mechanism: Standard HID PID Effects

FFB is NOT sent as a simple "torque value". Instead, applications (or DirectInput)
upload structured **effects** to the device, which the device's firmware then
executes autonomously. This is the standard HID PID paradigm.

### Supported Effects

Per firmware changelog analysis and Linux driver testing:

| Effect Type | HID PID Usage | Status |
|-------------|---------------|--------|
| Constant Force | `0x26` | ✅ Full support |
| Ramp | `0x27` | ✅ Full support |
| Square | `0x30` | ✅ Fixed in FW 1.0.49 |
| Sine | `0x31` | ✅ Fixed in FW 1.0.49 |
| Triangle | `0x32` | ✅ Fixed in FW 1.0.49 |
| Sawtooth Up | `0x33` | ✅ Fixed in FW 1.0.49 |
| Sawtooth Down | `0x34` | ✅ Fixed in FW 1.0.49 |
| Spring | `0x40` | ✅ Re-tuned in FW 1.0.49 |
| Damper | `0x41` | ✅ Re-tuned in FW 1.0.49 |
| Inertia | `0x42` | ✅ Supported |
| Friction | `0x43` | ✅ Re-tuned in FW 1.0.49 |

### Effect Processing Pipeline

The Simucube processes FFB effects through multiple stages:

1. **Game sends effects** via DirectInput / HID PID
2. **Device firmware** interpolates and sums active effects
3. **Simucube Force Reconstruction Filter** (device-side, configurable in True Drive)
4. **IONI servo drive filters** (damping, friction, inertia — device-side)
5. **Torque Bandwidth Limit** (device-side filter)
6. **Motor output** at servo loop rate (internal, much higher than USB rate)

### Device Gain

The HID PID Device Gain report controls overall FFB strength. Per FW 1.0.49
changelog: "FFB Device Gain set by the game does not affect bumpstops anymore" and
"Individual gain parameter of the effects are now respected as per USB Physical
Device standards."

### iRacing 360 Hz Mode

Firmware 1.0.60 (Feb 2025) added support for iRacing's 360 Hz FFB mode. This
likely involves the device accepting HID PID effect updates at a higher rate.
The `bInterval = 3` already supports ~333 Hz polling; 360 Hz mode may use
`bInterval = 2` or a specific iRacing protocol extension.

---

## Rotation Range Setting

### How It Works

The wheel rotation range is **NOT set via USB HID commands**. It is configured
through the device's companion software:

- **Simucube 2**: True Drive software (Windows) or Simucube Tuner
- **Simucube 1**: SimuCUBE Configuration Tool

The rotation range is stored as a device-side profile parameter. The device
implements **software bumpstops** at the configured endpoints — these are
spring-like force effects that prevent the wheel from rotating beyond the set range.

Per firmware changelogs:
- Bumpstop angles are per-profile settings (FW 1.0.22+)
- Bumpstops begin at exactly the configured angle (FW 1.0.30+)
- Bumpstops are independent of game-generated FFB (FW 1.0.49+)
- Three bumpstop feel settings: soft/medium/hard (FW 1.0.22+)

### Implications for Our Implementation

We **cannot** programmatically set the rotation range via the USB protocol. This is
a deliberate design choice by Granite Devices — safety-critical parameters like
rotation range are managed by the device firmware and configured through the
official software only.

The USB HID report maps the full 16-bit axis range (0–65535) to whatever rotation
range the user has configured. Software reading the wheel position should normalize
based on the known rotation range (which must be obtained out-of-band, e.g. user
configuration).

---

## Simucube 1 (IONI) vs Simucube 2

### Architecture Differences

| Feature | Simucube 1 | Simucube 2 |
|---------|-----------|-----------|
| Architecture | STM32 MCU + IONI servo drive | Integrated design |
| USB PID | `0x0D5A` | `0x0D5F`/`0x0D60`/`0x0D61` |
| FFB Protocol | HID PID (same standard) | HID PID (same standard) |
| Firmware | Open-source releases on GitHub | Closed-source |
| Configuration | SimuCUBE Configuration Tool | True Drive / Simucube Tuner |
| Motor | External (user-supplied) | Integrated motor |
| Encoder | User-supplied (varies) | 22-bit absolute encoder |
| Wireless Wheels | Supported (with adapter) | Built-in support |

### Simucube 1 Details

The Simucube 1 is built on:
- **SimuCUBE board**: STM32-based main controller
- **IONI servo drive**: Granite Devices' motor controller (SimpleMotion V2 protocol)
- **External motor**: User-supplied (typically Mige, Lenze, or similar)
- **External encoder**: User-supplied

The IONI servo drive communicates with the STM32 via the **SimpleMotion V2** protocol
(a separate Granite Devices protocol for motor control — NOT exposed over USB to the
host PC). The USB interface to the host PC is standard HID PID, same as Simucube 2.

### Torque Specs

| Model | Peak Torque | Continuous | Notes |
|-------|------------|------------|-------|
| Simucube 1 | Varies by motor | Varies | Depends on user-chosen motor |
| Simucube 2 Sport | 17 Nm | ~12 Nm | Entry-level direct drive |
| Simucube 2 Pro | 25 Nm | ~17 Nm | Mid-tier |
| Simucube 2 Ultimate | 32 Nm | ~22 Nm | Flagship |

---

## Simucube Link

### Simucube 3 / Future Architecture

Simucube is transitioning to **Simucube Link**, a real-time Ethernet-based protocol
for connecting multiple devices. Key features:

- **Transport**: Real-time Ethernet with custom protocol
- **Latency**: Sub-millisecond
- **Topology**: Star via Ethernet switch, or daisy-chain
- **Bridge**: SC-Link Hub (USB-C ↔ Simucube Link RJ45)
- **Scalability**: Unlimited devices, one USB port

The SC-Link Hub (PID `0x0D66`) bridges USB to the Simucube Link network. From the
host PC's perspective, devices behind the SC-Link Hub still appear as USB HID devices.

Source: `Simucube/simucube-docs.github.io → docs/Developers/Simucube Link.md`

---

## Open-Source Implementations

### Official / Semi-Official

| Resource | URL | Notes |
|----------|-----|-------|
| SimuCUBE 1 Firmware Releases | `github.com/SimuCUBE/Firmware-Releases` | Binary releases (not source), 29 stars |
| SimpleMotion V2 Library | `github.com/GraniteDevices/SimpleMotionV2` | Motor control protocol (Apache-2.0), 48 stars |
| Argon Servo Drive Firmware | `github.com/GraniteDevices/ArgonServoDriveFirmware` | IONI ARM MCU firmware (open source), 18 stars |
| Simucube Official Docs | `github.com/Simucube/simucube-docs.github.io` | Official developer documentation |

### Community / Third-Party

| Resource | URL | Notes |
|----------|-----|-------|
| linux-steering-wheels | `github.com/JacKeTUs/linux-steering-wheels` | Linux FFB compatibility table; confirms SC PIDs |
| Linux `hid-pidff` driver | Linux kernel `drivers/hid/usbhid/hid-pidff.c` | Generic HID PID FFB driver; works with Simucube |
| `hid-universal-pidff` | Linux kernel (≥6.15) | Enhanced PID driver for wider device support |
| Granite Devices Wiki | `granitedevices.com/wiki/` | Linux setup guide with udev rules |

### What's NOT Open Source

- **Simucube 2 firmware**: Fully closed-source
- **True Drive / Simucube Tuner**: Closed-source Windows/Linux configuration tool
- **HID report descriptor**: Not publicly documented in detail (must be captured from
  a live device via USB descriptor dump)

---

## Corrections Needed in Our Crate

### Critical Issues in `hid-simucube-protocol`

#### 1. Input Report Structure (`input.rs`) — ⚠️ INCORRECT

Our `SimucubeInputReport` implements a custom binary layout that does NOT match the
actual Simucube HID report:

**Our code assumes:**
- `wheel_angle_raw: u32` (22-bit value)
- `wheel_speed_rpm: i16`
- `torque_nm: i16`
- `temperature_c: u8`
- `fault_flags: u8`
- `status_flags: u8`
- Custom byte offsets for each field

**Actual protocol:**
- Standard HID input report with axes defined by the HID report descriptor
- Wheel position: **X axis, unsigned 16-bit** (0–65535), per official docs
- Y axis: unsigned 16-bit (user-mappable)
- 6 additional axes: unsigned 16-bit each
- 128 buttons as bitmask
- No raw 22-bit encoder value, no temperature, no speed, no fault flags in HID report

**Recommendation**: Rewrite `input.rs` to parse standard HID joystick reports using
the device's HID report descriptor, OR use a high-level HID/DirectInput API that
handles descriptor parsing automatically.

#### 2. Output Report Structure (`output.rs`) — ⚠️ INCORRECT

Our `SimucubeOutputReport` sends a custom binary report with raw torque values. This
is NOT how Simucube FFB works.

**Our code assumes:**
- Report ID `0x01`
- Sequence number + raw centi-Newton-meter torque value
- RGB LED control
- Custom effect type enum

**Actual protocol:**
- Standard HID PID effect reports (Create Effect, Set Effect, Set Constant Force, etc.)
- Effects are uploaded and managed, NOT streamed as raw torque values
- The device runs effects autonomously once uploaded
- Report IDs are defined by the device's HID report descriptor

**Recommendation**: Implement standard HID PID effect management, or use
DirectInput/`hid-pidff` abstractions. Our custom binary output format will be
silently ignored or cause undefined behavior on real hardware.

#### 3. Effect Types Enum (`output.rs`) — ⚠️ PARTIALLY CORRECT

Our `EffectType` enum lists correct effect type *names*, but the numeric values do
NOT match the HID PID specification:

| Our Value | HID PID Usage Page Value |
|-----------|--------------------------|
| `Constant = 1` | `0x26` |
| `Ramp = 2` | `0x27` |
| `Square = 3` | `0x30` |
| `Sine = 4` | `0x31` |
| `Spring = 8` | `0x40` |
| `Damper = 9` | `0x41` |
| `Friction = 10` | `0x43` |

**Recommendation**: Align effect type values with HID PID usage page constants, or
use an abstraction layer.

#### 4. Angle Calculation (`input.rs`, `lib.rs`) — ⚠️ MISLEADING

Constants like `ANGLE_SENSOR_BITS = 22` and `ANGLE_SENSOR_MAX = 0x3FFFFF` suggest we
receive 22-bit encoder values over USB. We do NOT. The USB axis is 16-bit (0–65535).
The 22-bit encoder is an internal hardware specification only.

**Recommendation**: Change angle calculations to use 16-bit axis values. The mapping
from axis value to degrees depends on the user's configured rotation range, which is
NOT available via USB.

#### 5. Wireless Wheel PID (`ids.rs`) — ⚠️ SPECULATIVE

`SIMUCUBE_WIRELESS_WHEEL_PID = 0x0D63` is NOT present in the official Simucube
developer PID table. Our code correctly documents this as estimated, but it should
not be used for device matching without independent confirmation. The wireless wheel
may not have its own USB PID — it communicates wirelessly with the base station,
and its buttons appear as part of the wheelbase's 128-button HID report.

#### 6. What IS Correct

- ✅ VID `0x16D0` — confirmed
- ✅ All wheelbase PIDs (`0x0D5A`, `0x0D61`, `0x0D60`, `0x0D5F`) — confirmed
- ✅ ActivePedal/SC-Link Hub PID `0x0D66` — confirmed
- ✅ Max torque values (17/25/32 Nm) — confirmed
- ✅ Report size 64 bytes — confirmed
- ✅ FW upgrade PIDs (`0x0D5E`, `0x0D5B`) documented in comments — confirmed
- ✅ `SimucubeModel` enum and `from_product_id()` logic — correct

---

## Sources

All sources accessed July 2025.

### Official / Authoritative

1. **Simucube Developer Documentation** (PID table, axis description, button count)  
   `github.com/Simucube/simucube-docs.github.io` → `docs/Simucube 2/Developers.md`  
   SHA: `a1c958bb221257d087cd4112ced69d3d872a39e5`

2. **Simucube Link Documentation**  
   `github.com/Simucube/simucube-docs.github.io` → `docs/Developers/Simucube Link.md`

3. **Granite Devices Wiki — Linux Setup** (udev rules with PIDs)  
   `granitedevices.com/wiki/Using_Simucube_wheel_base_in_Linux`

4. **Granite Devices Wiki — Firmware Releases** (changelog, HID PID fixes)  
   `granitedevices.com/wiki/SimuCUBE_firmware_releases`

5. **Granite Devices Wiki — Firmware User Guide**  
   `granitedevices.com/wiki/SimuCUBE_Firmware_User_Guide`

6. **SimuCUBE Firmware Releases (GitHub)**  
   `github.com/SimuCUBE/Firmware-Releases`

### Community / Cross-Reference

7. **JacKeTUs/linux-steering-wheels** (PID verification, driver compatibility)  
   `github.com/JacKeTUs/linux-steering-wheels`  
   Confirms: SC1=16d0:0d5a, SC2 Sport=16d0:0d61, SC2 Pro=16d0:0d60, SC2 Ultimate=16d0:0d5f  
   Driver: `hid-pidff` (Silver rating)

8. **Linux Kernel `hid-pidff`** (HID PID driver, `0xa7` descriptor fix)  
   `drivers/hid/usbhid/hid-pidff.c`

9. **Granite Devices GitHub** (SimpleMotion V2, Argon firmware)  
   `github.com/GraniteDevices`

10. **USB HID PID Specification**  
    `usb.org/sites/default/files/documents/pid1_01.pdf` (PID 1.01)

### Not Found / Does Not Exist

- `github.com/nicokimmel/simucube-udp` — 404, does not exist
- `community.granitedevices.com/t/simucube-2-usb-hid-protocol/3419` — 404
- No public HID report descriptor dumps found for Simucube devices
- No SC2 SDK repository found at `github.com/SimuCUBE/SC2-sdk` (referenced in
  existing docs but may not exist publicly)
