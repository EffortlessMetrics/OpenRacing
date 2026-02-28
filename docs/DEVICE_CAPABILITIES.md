# Device Capability Matrix

This document provides a consolidated reference of every racing device vendor supported by OpenRacing, the USB identifiers used to detect them, and the force-feedback capabilities exposed through the corresponding `crates/hid-*-protocol` microcrates.

All VID/PID values are sourced from `docs/protocols/SOURCES.md` (the golden reference). Torque, encoder resolution, and FFB effect columns are derived from the relevant `types.rs` and `ids.rs` source files in each protocol crate.

---

## Wheelbase Capability Table

### Column key

| Column | Description |
|---|---|
| **Device** | Commercial product name |
| **Vendor** | Manufacturer |
| **USB VID** | USB Vendor ID (hex) |
| **Protocol Type** | FFB wire protocol (see [Protocol Types](#force-feedback-protocol-types)) |
| **Max Torque (Nm)** | Peak rated torque; source: protocol crate `types.rs` |
| **FFB Axes** | Number of FFB-capable axes (steering = 1 for all wheelbases) |
| **Force Feedback Types** | Named effect categories the device accepts |
| **Encoder Resolution** | Angle-sensor bits or counts-per-rev (CPR) |
| **Notes** |  |

### Moza Racing — VID `0x346E`

Source: `crates/hid-moza-protocol`; VID/PID status: **Verified** (see `docs/protocols/SOURCES.md#moza-racing`).

| Device | Vendor | USB VID | Protocol Type | Max Torque (Nm) | FFB Axes | Force Feedback Types | Encoder Resolution | Notes |
|---|---|---|---|---|---|---|---|---|
| Moza R3 | Moza Racing | `0x346E` | Moza Proprietary | 3.9 | 1 | Constant, Spring, Damper, Friction | 16-bit (i16 torque command) | PIDs `0x0005` (V1), `0x0015` (V2) |
| Moza R5 | Moza Racing | `0x346E` | Moza Proprietary | 5.5 | 1 | Constant, Spring, Damper, Friction | 16-bit | PIDs `0x0004` (V1), `0x0014` (V2) |
| Moza R9 | Moza Racing | `0x346E` | Moza Proprietary | 9.0 | 1 | Constant, Spring, Damper, Friction | 16-bit | PIDs `0x0002` (V1, no ES), `0x0012` (V2) |
| Moza R12 | Moza Racing | `0x346E` | Moza Proprietary | 12.0 | 1 | Constant, Spring, Damper, Friction | 16-bit | PIDs `0x0006` (V1), `0x0016` (V2) |
| Moza R16 / R21 | Moza Racing | `0x346E` | Moza Proprietary | 16.0 / 21.0 | 1 | Constant, Spring, Damper, Friction | 16-bit | PIDs `0x0000` (V1), `0x0010` (V2); R16/R21 share PID |

### Fanatec — VID `0x0EB7`

Source: `crates/hid-fanatec-protocol`; VID/PID status: **Verified (wheelbases)** / Community (pedals) (see `docs/protocols/SOURCES.md#fanatec`).

| Device | Vendor | USB VID | Protocol Type | Max Torque (Nm) | FFB Axes | Force Feedback Types | Encoder Resolution | Notes |
|---|---|---|---|---|---|---|---|---|
| Fanatec CSL Elite | Fanatec | `0x0EB7` | Fanatec Proprietary | 6.0 | 1 | Constant, Damper, Spring, Rumble | 4096 CPR | PIDs `0x0004`, `0x0E03` |
| Fanatec ClubSport V2 / V2.5 | Fanatec | `0x0EB7` | Fanatec Proprietary | 8.0 | 1 | Constant, Damper, Spring, Rumble | 4096 CPR | PIDs `0x0001`, `0x0005`, `0x6204` |
| Fanatec CSL DD | Fanatec | `0x0EB7` | Fanatec Proprietary | 8.0 | 1 | Constant, Damper, Spring, Rumble | 16 384 CPR | PIDs `0x0020`, `0x0011`; boost kit unlocks higher torque |
| Fanatec Gran Turismo DD Pro | Fanatec | `0x0EB7` | Fanatec Proprietary | 8.0 | 1 | Constant, Damper, Spring, Rumble | 16 384 CPR | PID `0x0024`; boost kit available |
| Fanatec Podium DD1 | Fanatec | `0x0EB7` | Fanatec Proprietary | 20.0 | 1 | Constant, Damper, Spring, Rumble | 16 384 CPR | PID `0x0006` |
| Fanatec Podium DD2 | Fanatec | `0x0EB7` | Fanatec Proprietary | 25.0 | 1 | Constant, Damper, Spring, Rumble | 16 384 CPR | PID `0x0007` |

### Logitech — VID `0x046D`

Source: `crates/hid-logitech-protocol`; VID/PID status: **Verified** (see `docs/protocols/SOURCES.md#logitech`).

| Device | Vendor | USB VID | Protocol Type | Max Torque (Nm) | FFB Axes | Force Feedback Types | Encoder Resolution | Notes |
|---|---|---|---|---|---|---|---|---|
| Logitech G25 | Logitech | `0x046D` | Logitech Native / HID PID | 2.5 | 1 | Constant, Spring, Damper, Friction | 900° pot | PID `0xC299`; 900° max rotation |
| Logitech G27 | Logitech | `0x046D` | Logitech Native / HID PID | 2.5 | 1 | Constant, Spring, Damper, Friction | 900° pot | PIDs `0xC29B`, `0xC294`; 900° max rotation |
| Logitech G29 | Logitech | `0x046D` | Logitech Native / HID PID | 2.2 | 1 | Constant, Spring, Damper, Friction | 900° pot | PIDs `0xC24F`, `0xC260`; PS & Xbox variants |
| Logitech G920 | Logitech | `0x046D` | Logitech Native / HID PID | 2.2 | 1 | Constant, Spring, Damper, Friction | 900° pot | PIDs `0xC262`, `0xC261` |
| Logitech G923 | Logitech | `0x046D` | Logitech TrueForce | 2.2 | 1 | Constant, Spring, Damper, Friction, **TrueForce haptic** | 900° pot | PIDs `0xC267` (PS), `0xC26E` (Xbox) |
| Logitech G PRO | Logitech | `0x046D` | Logitech Native / HID PID | 2.2 | 1 | Constant, Spring, Damper, Friction | 900° pot | PIDs `0xC268` (PS), `0xC272` (Xbox) |

### Thrustmaster — VID `0x044F`

Source: `crates/hid-thrustmaster-protocol`; VID/PID status: **Verified (T300/TMX)** / Community (others) (see `docs/protocols/SOURCES.md#thrustmaster`).

| Device | Vendor | USB VID | Protocol Type | Max Torque (Nm) | FFB Axes | Force Feedback Types | Encoder Resolution | Notes |
|---|---|---|---|---|---|---|---|---|
| Thrustmaster T150 / TMX | Thrustmaster | `0x044F` | Thrustmaster Proprietary | 2.5 | 1 | Constant, Spring, Damper, Friction | 900° | PIDs `0xB65D`, `0xB65E`, `0xB67F` |
| Thrustmaster T300 RS / GT / TX | Thrustmaster | `0x044F` | Thrustmaster Proprietary | 4.0 | 1 | Constant, Spring, Damper, Friction | 900° | PIDs `0xB66D`–`0xB66F`, `0xB669` |
| Thrustmaster T500 RS | Thrustmaster | `0x044F` | Thrustmaster Proprietary | 2.5 | 1 | Constant, Spring, Damper, Friction | 1080° | PID `0xB677` |
| Thrustmaster T248 / T248X | Thrustmaster | `0x044F` | Thrustmaster Proprietary | 4.0 | 1 | Constant, Spring, Damper, Friction | 900° | PIDs `0xB696`, `0xB697` |
| Thrustmaster T-GT / T-GT II | Thrustmaster | `0x044F` | Thrustmaster Proprietary | 6.0 | 1 | Constant, Spring, Damper, Friction | 1080° | PIDs `0xB68E`, `0xB692` |
| Thrustmaster TS-PC / TS-XW | Thrustmaster | `0x044F` | Thrustmaster Proprietary | 6.0 | 1 | Constant, Spring, Damper, Friction | 1070° | PIDs `0xB689`, `0xB691` |
| Thrustmaster T818 | Thrustmaster | `0x044F` | Thrustmaster Proprietary | 10.0 | 1 | Constant, Spring, Damper, Friction | 1080° | PID `0xB69B`; direct drive |

### Simagic — VID `0x3670` (EVO gen) / `0x0483` (legacy)

Source: `crates/hid-simagic-protocol`; VID/PID status: **Verified (EVO, legacy `0x0522`)** / Estimated (accessories) (see `docs/protocols/SOURCES.md#simagic`).

| Device | Vendor | USB VID | Protocol Type | Max Torque (Nm) | FFB Axes | Force Feedback Types | Encoder Resolution | Notes |
|---|---|---|---|---|---|---|---|---|
| Simagic EVO Sport | Simagic | `0x3670` | Simagic Proprietary | 9.0 | 1 | Constant, Spring, Damper, Friction, Sine, Square, Triangle | N/A (proprietary) | PID `0x0500`; verified |
| Simagic EVO | Simagic | `0x3670` | Simagic Proprietary | 12.0 | 1 | Constant, Spring, Damper, Friction, Sine, Square, Triangle | N/A | PID `0x0501`; verified |
| Simagic EVO Pro | Simagic | `0x3670` | Simagic Proprietary | 18.0 | 1 | Constant, Spring, Damper, Friction, Sine, Square, Triangle | N/A | PID `0x0502`; verified |
| Simagic Alpha EVO | Simagic | `0x3670` | Simagic Proprietary | 15.0 | 1 | Constant, Spring, Damper, Friction, Sine, Square, Triangle | N/A | PID `0x0600`; estimated |
| Simagic Neo | Simagic | `0x3670` | Simagic Proprietary | 10.0 | 1 | Constant, Spring, Damper, Friction, Sine, Square, Triangle | N/A | PID `0x0700`; estimated |
| Simagic Neo Mini | Simagic | `0x3670` | Simagic Proprietary | 7.0 | 1 | Constant, Spring, Damper, Friction, Sine, Square, Triangle | N/A | PID `0x0701`; estimated |
| Simagic Alpha / Alpha Mini / M10 (legacy) | Simagic | `0x0483` | Simagic Proprietary | ~15.0 | 1 | Constant, Spring, Damper | N/A | PID `0x0522`; shared with VRS; disambiguate via `iProduct` |

### Simucube 2 (Granite Devices) — VID `0x16D0`

Source: `crates/hid-simucube-protocol`; VID/PID status: **Verified** (see `docs/protocols/SOURCES.md#simucube-granite-devices`).

| Device | Vendor | USB VID | Protocol Type | Max Torque (Nm) | FFB Axes | Force Feedback Types | Encoder Resolution | Notes |
|---|---|---|---|---|---|---|---|---|
| Simucube 2 Sport | Granite Devices | `0x16D0` | Simucube Proprietary | 17.0 | 1 | Constant, Spring, Damper, Friction | 22-bit (4 194 303 steps) | PID `0x0D61`; 360 Hz FFB update rate |
| Simucube 2 Pro | Granite Devices | `0x16D0` | Simucube Proprietary | 25.0 | 1 | Constant, Spring, Damper, Friction | 22-bit | PID `0x0D60`; wireless wheel support |
| Simucube 2 Ultimate | Granite Devices | `0x16D0` | Simucube Proprietary | 32.0 | 1 | Constant, Spring, Damper, Friction | 22-bit | PID `0x0D5F`; wireless wheel support |
| Simucube ActivePedal | Granite Devices | `0x16D0` | Simucube Proprietary | — | 0 | n/a (pedal actuator) | 16-bit | PID `0x0D66` (SC-Link Hub); active pedal only |

### VRS DirectForce — VID `0x0483`

Source: `crates/hid-vrs-protocol`; VID/PID status: **Verified (DirectForce Pro)** / Community (V2, accessories) (see `docs/protocols/SOURCES.md#vrs-directforce`).

| Device | Vendor | USB VID | Protocol Type | Max Torque (Nm) | FFB Axes | Force Feedback Types | Encoder Resolution | Notes |
|---|---|---|---|---|---|---|---|---|
| VRS DirectForce Pro | VRS | `0x0483` | HID PIDFF | 20.0 | 1 | Constant, Ramp, Square, Sine, Triangle, Sawtooth Up/Down, Spring, Damper, Friction, Custom | N/A (PIDFF standard) | PID `0xA355`; shared STM VID — see note below |
| VRS DirectForce Pro V2 | VRS | `0x0483` | HID PIDFF | 25.0 | 1 | Constant, Ramp, Square, Sine, Triangle, Sawtooth Up/Down, Spring, Damper, Friction, Custom | N/A | PID `0xA356`; community-sourced |

> **VID collision:** VID `0x0483` is also used by legacy Simagic devices (PID `0x0522`). The engine disambiguates using the USB `iProduct` string descriptor.

### Cammus — VID `0x3416`

Source: `crates/hid-cammus-protocol`; VID/PID status: **Verified** (see `docs/protocols/SOURCES.md#cammus`).

| Device | Vendor | USB VID | Protocol Type | Max Torque (Nm) | FFB Axes | Force Feedback Types | Encoder Resolution | Notes |
|---|---|---|---|---|---|---|---|---|
| Cammus C5 | Cammus | `0x3416` | Cammus Proprietary Direct | 5.0 | 1 | Constant (direct torque) | N/A (direct torque) | PID `0x0301`; direct torque via report `0x64` |
| Cammus C12 | Cammus | `0x3416` | Cammus Proprietary Direct | 12.0 | 1 | Constant (direct torque) | N/A | PID `0x0302` |

### Asetek SimSports — VID `0x2433`

Source: `crates/hid-asetek-protocol`; VID/PID status: **Verified (Invicta, Forte)** / Community (LaPrima, TK) (see `docs/protocols/SOURCES.md#asetek-simsports`).

| Device | Vendor | USB VID | Protocol Type | Max Torque (Nm) | FFB Axes | Force Feedback Types | Encoder Resolution | Notes |
|---|---|---|---|---|---|---|---|---|
| Asetek Invicta | Asetek | `0x2433` | Asetek Proprietary | 15.0 | 1 | Constant, Spring, Damper | N/A | PID `0xF300`; quick-release system |
| Asetek Forte | Asetek | `0x2433` | Asetek Proprietary | 20.0 | 1 | Constant, Spring, Damper | N/A | PID `0xF301`; quick-release system |
| Asetek LaPrima | Asetek | `0x2433` | Asetek Proprietary | 10.0 | 1 | Constant, Spring, Damper | N/A | PID `0xF303`; community-sourced |

### Granite Devices / OSW (SimpleMotion V2) — VID `0x1D50`

Source: `crates/simplemotion-v2`; VID/PID status: community-sourced.

| Device | Vendor | USB VID | Protocol Type | Max Torque (Nm) | FFB Axes | Force Feedback Types | Encoder Resolution | Notes |
|---|---|---|---|---|---|---|---|---|
| Simucube 1 / IONI Drive | Granite Devices | `0x1D50` | SimpleMotion V2 (Proprietary Serial) | 15.0 | 1 | Constant (direct torque) | 17-bit (131 072 CPR default) | PID `0x6050`; legacy OSW base |
| IONI Premium / Simucube 2 (legacy) | Granite Devices | `0x1D50` | SimpleMotion V2 | 35.0 | 1 | Constant (direct torque) | 17-bit | PID `0x6051` |
| ARGON Servo Drive / Simucube Sport | Granite Devices | `0x1D50` | SimpleMotion V2 | 10.0 | 1 | Constant (direct torque) | 17-bit | PID `0x6052` |

### Leo Bodnar — VID `0x1DD2`

Source: `crates/hid-leo-bodnar-protocol`; VID/PID status: Verified / Community (see module docs).

| Device | Vendor | USB VID | Protocol Type | Max Torque (Nm) | FFB Axes | Force Feedback Types | Encoder Resolution | Notes |
|---|---|---|---|---|---|---|---|---|
| Leo Bodnar USB Sim Racing Wheel Interface | Leo Bodnar | `0x1DD2` | HID PIDFF | ~10.0 (user-configurable) | 1 | Constant, Spring, Damper (PIDFF standard) | 16-bit (65 535 CPR) | PID `0x000E`; actual torque depends on motor/PSU |
| Leo Bodnar FFB Joystick | Leo Bodnar | `0x1DD2` | HID PIDFF | ~10.0 (user-configurable) | 1–2 | Constant, Spring, Damper | 16-bit | PID `0x000F` |
| Leo Bodnar BU0836A / BU0836X / BU0836 16-bit | Leo Bodnar | `0x1DD2` | Input only | — | 0 | n/a | 12-bit / 16-bit ADC | PIDs `0x000B`, `0x0030`, `0x0031`; joystick interfaces |
| Leo Bodnar BBI-32 | Leo Bodnar | `0x1DD2` | Input only | — | 0 | n/a | — | PID `0x000C`; 32-button box |
| Leo Bodnar SLI-M | Leo Bodnar | `0x1DD2` | Output only | — | 0 | n/a | — | PID `0xBEEF`; RPM/gear display |

### SimExperience AccuForce — VID `0x1FC9`

Source: `crates/hid-accuforce-protocol`; VID/PID status: **Community** (USB captures, RetroBat Wheels.cs).

| Device | Vendor | USB VID | Protocol Type | Max Torque (Nm) | FFB Axes | Force Feedback Types | Encoder Resolution | Notes |
|---|---|---|---|---|---|---|---|---|
| SimExperience AccuForce Pro | SimExperience | `0x1FC9` | HID PIDFF | 7.0 | 1 | Constant, Spring, Damper, Friction (PIDFF standard) | N/A (PIDFF standard) | PID `0x804C`; NXP USB chip VID |

### FFBeast (Open Source) — VID `0x045B`

Source: `crates/engine/src/hid/vendor/ffbeast.rs`; VID/PID status: **Verified** (see `docs/protocols/SOURCES.md#ffbeast`).

| Device | Vendor | USB VID | Protocol Type | Max Torque (Nm) | FFB Axes | Force Feedback Types | Encoder Resolution | Notes |
|---|---|---|---|---|---|---|---|---|
| FFBeast Wheel | FFBeast (open-source) | `0x045B` | HID PIDFF + vendor feature reports | ~20.0 (user-configurable) | 1 | Constant (PIDFF) | 16-bit (65 535 CPR typical) | PID `0x59D7`; actual torque depends on build |
| FFBeast Joystick | FFBeast | `0x045B` | HID PIDFF + vendor feature reports | ~20.0 | 1–2 | Constant | 16-bit | PID `0x58F9` |
| FFBeast Rudder | FFBeast | `0x045B` | HID PIDFF + vendor feature reports | ~20.0 | 1–2 | Constant | 16-bit | PID `0x5968` |

### OpenFFBoard (Open Source) — VID `0x1209`

Source: `crates/engine/src/hid/vendor/openffboard.rs`; VID/PID status: **Verified** (see `docs/protocols/SOURCES.md#openffboard`).

| Device | Vendor | USB VID | Protocol Type | Max Torque (Nm) | FFB Axes | Force Feedback Types | Encoder Resolution | Notes |
|---|---|---|---|---|---|---|---|---|
| OpenFFBoard | Ultrawipf (open-source) | `0x1209` | HID PIDFF + vendor feature reports | ~20.0 (user-configurable) | 1 | Constant (PIDFF) | 16-bit (65 536 CPR typical) | PIDs `0xFFB0`, `0xFFB1`; motor/PSU dependent |

### Cube Controls — VID `0x0483` *(provisional — input devices only)*

Source: `crates/engine/src/hid/vendor/cube_controls.rs`.

> ⚠️ **PROVISIONAL**: The USB VID/PIDs listed here are **unconfirmed**. Cube Controls products are **steering wheel button boxes** (input-only devices), not wheelbases. They do not produce force feedback. Update this table once USB descriptor captures from real hardware are available.

| Device | Vendor | USB VID | Protocol Type | Max Torque (Nm) | FFB Axes | Force Feedback Types | Encoder Resolution | Notes |
|---|---|---|---|---|---|---|---|---|
| Cube Controls GT Pro | Cube Controls | `0x0483` *(prov.)* | Input-only (HID gamepad) | N/A | 0 | None (input device) | N/A | PID `0x0C73` (provisional); steering wheel button box |
| Cube Controls Formula Pro | Cube Controls | `0x0483` *(prov.)* | Input-only (HID gamepad) | N/A | 0 | None (input device) | N/A | PID `0x0C74` (provisional); steering wheel button box |
| Cube Controls CSX3 | Cube Controls | `0x0483` *(prov.)* | Input-only (HID gamepad) | N/A | 0 | None (input device) | N/A | PID `0x0C75` (provisional); steering wheel with touchscreen |

### PXN — VID `0x11FF`

Source: `crates/hid-pxn-protocol`; VID/PID status: **Community** (JacKeTUs/linux-steering-wheels) (see `docs/protocols/SOURCES.md#pxn`).

| Device | Vendor | USB VID | Protocol Type | Max Torque (Nm) | FFB Axes | Force Feedback Types | Encoder Resolution | Notes |
|---|---|---|---|---|---|---|---|---|
| PXN V10 | PXN | `0x11FF` | HID PIDFF | ~10.0 | 1 | Constant, Spring, Damper (PIDFF standard) | N/A (PIDFF standard) | PID `0x3245`; community-sourced |
| PXN V12 | PXN | `0x11FF` | HID PIDFF | ~12.0 | 1 | Constant, Spring, Damper (PIDFF standard) | N/A | PID `0x1212`; community-sourced |
| PXN V12 Lite | PXN | `0x11FF` | HID PIDFF | ~12.0 | 1 | Constant, Spring, Damper (PIDFF standard) | N/A | PID `0x1112`; community-sourced |
| PXN V12 Lite SE | PXN | `0x11FF` | HID PIDFF | ~12.0 | 1 | Constant, Spring, Damper (PIDFF standard) | N/A | PID `0x1211`; community-sourced |
| PXN GT987 FF | PXN | `0x11FF` | HID PIDFF | ~5.0 | 1 | Constant, Spring, Damper (PIDFF standard) | N/A | PID `0x2141`; community-sourced; Lite Star OEM |

---

## Non-Wheelbase Peripherals

The following devices are supported for pedal, shifter, or handbrake input. They do not transmit force feedback.

| Device | Vendor | USB VID | Protocol | Axes | Notes |
|---|---|---|---|---|---|
| Moza SR-P Pedals | Moza Racing | `0x346E` | Moza Proprietary | 3 (T/B/C) | PID `0x0003`; standalone USB |
| Moza HBP Handbrake | Moza Racing | `0x346E` | Moza Proprietary | 1 | PID `0x0022` |
| Moza HGP / SGP Shifter | Moza Racing | `0x346E` | Moza Proprietary | — | PIDs `0x0020`, `0x0021` |
| Fanatec ClubSport Pedals V1/V2 | Fanatec | `0x0EB7` | Fanatec Proprietary | 2 | PID `0x1839` |
| Fanatec ClubSport Pedals V3 | Fanatec | `0x0EB7` | Fanatec Proprietary | 3 (load cell) | PID `0x183B` |
| Fanatec CSL Pedals / CSL Pedals V2 | Fanatec | `0x0EB7` | Fanatec Proprietary | 3 | PIDs `0x6205`, `0x6206` |
| Thrustmaster T3PA / T3PA Pro | Thrustmaster | `0x044F` | Thrustmaster Proprietary | 3 | PIDs `0xB678`, `0xB679` |
| Thrustmaster T-LCM / T-LCM Pro | Thrustmaster | `0x044F` | Thrustmaster Proprietary | 3 (load cell) | PIDs `0xB68D`, `0xB69A` |
| Heusinkveld Sprint | Heusinkveld | `0x16D0` | USB HID (load cell) | 3 | PID `0x1156`; up to 200 kg load cell |
| Heusinkveld Ultimate+ | Heusinkveld | `0x16D0` | USB HID (load cell) | 3 | PID `0x1157` |
| Heusinkveld Pro | Heusinkveld | `0x16D0` | USB HID (load cell) | 3 | PID `0x1158` |
| Simagic P1000 / P1000A Pedals | Simagic | `0x3670` | Simagic Proprietary | 3 | PIDs `0x1001`, `0x1003` (estimated) |
| Simagic P2000 Pedals | Simagic | `0x3670` | Simagic Proprietary | 3 | PID `0x1002` (estimated) |
| Simagic H-Pattern / Sequential Shifter | Simagic | `0x3670` | Simagic Proprietary | — | PIDs `0x2001`, `0x2002` (estimated) |
| Simagic Handbrake | Simagic | `0x3670` | Simagic Proprietary | 1 | PID `0x3001` (estimated) |
| VRS Pedals V1 / V2 | VRS | `0x0483` | USB HID | 3 | PIDs `0xA357`, `0xA358` |
| VRS Handbrake | VRS | `0x0483` | USB HID | 1 | PID `0xA359` |
| VRS Shifter | VRS | `0x0483` | USB HID | — | PID `0xA35A` |
| Cammus CP5 Pedals | Cammus | `0x3416` | USB HID | 3 | PID `0x1018`; community-sourced |
| Cammus LC100 Pedals | Cammus | `0x3416` | USB HID | 3 | PID `0x1019`; community-sourced |

---

## Force Feedback Protocol Types

The table below summarises each wire protocol referenced in the capability tables above.

| Protocol Type | Description | Devices Using This Protocol |
|---|---|---|
| **HID PIDFF** | Standard USB HID Physical Interface Device (PID) force feedback, Usage Page `0x000F`. Effects are managed through the OS HID driver via effect create/update/destroy reports. Supports a wide set of effect types defined by the USB HID spec. | VRS DirectForce Pro, AccuForce Pro, FFBeast, OpenFFBoard, Leo Bodnar Wheel Interface/FFB Joystick |
| **Moza Proprietary** | Custom HID vendor usage page. Torque output uses report `0x20` (direct torque, signed `i16`, percent-of-max). Handshake sequence required at connect. Rim identity, pedal axes, and KS control-surface snapshots multiplexed through the same USB endpoint. | Moza R3–R21 |
| **Fanatec Proprietary** | Endor AG / Fanatec vendor HID protocol. Supports constant-force, gain, LED, display, and mode-switch feature reports. | Fanatec CSL DD, Podium DD1/DD2, ClubSport, CSL Elite |
| **Logitech Native / HID PID** | Logitech wheels start in compatibility mode and must be switched to native mode via a vendor command before exposing the full effect set. HID PID reports are used after mode switch. | Logitech G25–G920 |
| **Logitech TrueForce** | Extension of Logitech Native mode that adds high-frequency haptic output layered on top of standard effects. The G923 uses this for road-surface texture simulation. | Logitech G923 |
| **Thrustmaster Proprietary** | Thrustmaster vendor HID protocol. Uses proprietary HID reports for constant-force, spring, damper, friction, and device gain. Requires an initialization sequence (`hid-tminit`-style). | Thrustmaster T150–T818 |
| **Simagic Proprietary** | Simagic vendor HID protocol. Supports constant-force and conditional effects (spring, damper, friction) plus waveform effects (sine, square, triangle) via custom report IDs. | Simagic EVO, Alpha, Neo families |
| **Simucube Proprietary** | Granite Devices proprietary protocol over USB HID, providing direct torque control at 360 Hz. Supports 22-bit angle sensor resolution and wireless wheel modules. | Simucube 2 Sport/Pro/Ultimate |
| **Asetek Proprietary** | Asetek SimSports vendor HID protocol. Supports constant-force, spring, and damper effects. Quick-release system. | Asetek Invicta, Forte, LaPrima |
| **Cammus Proprietary Direct** | Cammus vendor HID protocol. Direct torque command via report `0x64`. Two modes: configuration mode and game mode. | Cammus C5, C12 |
| **SimpleMotion V2 (Proprietary Serial)** | Granite Devices SimpleMotion V2 protocol over USB CDC/serial. Used for IONI/ARGON servo drives and legacy OSW builds. Direct torque command at up to 1 kHz. | Granite Devices IONI, ARGON, OSW / Simucube 1 |

---

## Tested Status

Devices are assigned one of three status levels based on available evidence.

| Status | Definition | How to Upgrade |
|---|---|---|
| **Verified** | VID/PID confirmed from an official USB descriptor dump, official SDK, or Linux kernel `hid-ids.h`. Protocol behaviour confirmed by capture or documentation. | Add a link to the capture or SDK source in `docs/protocols/SOURCES.md`. |
| **Community-reported** | VID/PID and protocol behaviour confirmed from a community-maintained compatibility table (e.g., [JacKeTUs/linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels), iRacing forum captures, or SimHub issues). Not independently verified against hardware by the OpenRacing project. | Provide a USB descriptor capture or official source; escalate to Verified. |
| **Protocol documented / Estimated** | PID logically extrapolated from a known VID or a sibling model, or assigned by OpenRacing based on community discussion with no independent confirmation. Must be confirmed before production release. | Obtain a USB descriptor capture (`lsusb -v`, USBTreeView, or Zadig) from real hardware. |

### Current status summary

| Vendor | Status | Verification Detail |
|---|---|---|
| Moza Racing (wheelbases V1/V2) | Verified | All 11 PIDs web-verified (universal-pidff, mozaracing.com) |
| Fanatec (DD1, DD2, CSL DD, GT DD Pro) | Verified (others Community) | Wheelbase PIDs verified; pedal PIDs community |
| Logitech (G25–G PRO) | Verified | All PIDs from Linux kernel hid-ids.h + oversteer |
| Thrustmaster (T300, TMX) | Verified (others Community) | T500 RS PID corrected; T-GT/T-GT II PIDs unknown |
| Simagic EVO series | Verified (accessories Estimated) | EVO torques corrected (9/12/18 Nm); PID collision resolved |
| Simucube 2 Sport/Pro/Ultimate | Verified | SC2 Sport 17 Nm, Ultimate 32 Nm corrected; SC1 PID added |
| VRS DirectForce Pro | Verified (V2 Community) | VID collision with Simagic documented |
| Cammus C5 / C12 | Verified | All confirmed against hid-ids.h |
| Asetek Invicta / Forte | Verified (LaPrima Community) | Torques corrected (12/18/27 Nm) |
| Heusinkveld (Sprint, Ultimate+, Pro) | Community-reported | VID `0x16D0` confirmed; VID collision with Simucube documented |
| FFBeast | Verified | Dead links replaced; PIDs confirmed via hid-ids.h |
| OpenFFBoard | Verified | Main PID `0xFFB0` confirmed (pid.codes); alt `0xFFB1` unverified |
| AccuForce Pro | Community-reported | PID `0x804C` confirmed; V1 vs V2 torque documented |
| Leo Bodnar Wheel Interface / FFB Joystick | Community-reported | VID confirmed; SLI-M PID `0xBEEF` flagged as placeholder |
| Granite Devices / OSW (SimpleMotion V2) | Community-reported | — |
| Cube Controls | Estimated (**Provisional — PIDs unconfirmed; input-only devices, not wheelbases**) | Reclassified as button boxes (non-FFB) |

---

## Adding New Devices

### Overview

Each vendor is implemented as a self-contained "microcrate" in `crates/hid-<vendor>-protocol/`. The crate is intentionally I/O-free and allocation-free so it can be tested and fuzzed without hardware.

### Step-by-step

1. **Obtain authoritative VID/PID values.**
   - Use `lsusb -v` (Linux), USBTreeView (Windows), or a Wireshark/Zadig capture.
   - Check the official vendor SDK or Linux kernel `hid-ids.h` if available.
   - Record the source in `docs/protocols/SOURCES.md` with a **Verified**, **Community**, or **Estimated** tag before writing any code.

2. **Create the protocol microcrate.**
   ```
   crates/hid-<vendor>-protocol/
     src/
       ids.rs      # VENDOR_ID, product_ids, is_<vendor>_product()
       types.rs    # DeviceIdentity, Model enum, max_torque_nm()
       input.rs    # report parsing, InputState struct
       output.rs   # FFB encoders, build_* functions
       lib.rs      # #![deny(static_mut_refs)]; flat re-exports
   ```
   - Add the new crate to the workspace `Cargo.toml`.
   - Use workspace dependencies where possible.
   - All hot-path code must be allocation-free (no `Vec`, `HashMap`, `String`).

3. **Register the vendor in the engine.**
   - Add a match arm in `crates/engine/src/hid/vendor/mod.rs` → `get_vendor_protocol()`.
   - Create `crates/engine/src/hid/vendor/<vendor>.rs` implementing `VendorProtocol`.
   - Register the `FfbConfig` (max torque, encoder CPR, vendor usage page flag).

4. **Write tests.**
   - Unit tests in `crates/hid-<vendor>-protocol/src/*.rs` (inline `#[test]`).
   - Vendor integration tests in `crates/engine/src/hid/vendor/<vendor>_tests.rs`.
   - Use `Result`-returning test functions; no `unwrap()`/`expect()` in test code.
   - Add snapshot/regression tests for report encoding golden values.

5. **Update documentation.**
   - Add a row to the relevant table(s) in this file (`docs/DEVICE_CAPABILITIES.md`).
   - Add a section to `docs/protocols/SOURCES.md` citing the VID/PID source.

6. **Run CI checks.**
   ```bash
   cargo fmt --all
   cargo clippy --all-targets --all-features -- -D warnings
   cargo test --all-features --workspace
   cargo deny check
   ```

### VID collision handling

Several vendors share a USB VID (see `docs/protocols/SOURCES.md#vid-collision-map`). When adding a new device under a shared VID, update the disambiguation logic in `get_vendor_protocol()` and add a comment explaining the collision resolution strategy (PID range, `iProduct` string, or feature report probe).

---

## Source Citations

All VID/PID data in this document traces back to `docs/protocols/SOURCES.md`. Torque, encoder resolution, and effect-type data is derived directly from the Rust source; see the files listed below.

| Data | Source file |
|---|---|
| Moza VID/PIDs, torque | `crates/hid-moza-protocol/src/ids.rs`, `types.rs` |
| Fanatec VID/PIDs, torque, encoder | `crates/hid-fanatec-protocol/src/ids.rs`, `types.rs` |
| Logitech VID/PIDs, torque, TrueForce | `crates/hid-logitech-protocol/src/ids.rs`, `types.rs` |
| Thrustmaster VID/PIDs, torque | `crates/hid-thrustmaster-protocol/src/ids.rs`, `types.rs` |
| Simagic VID/PIDs, torque, effects | `crates/hid-simagic-protocol/src/ids.rs`, `types.rs` |
| Simucube VID/PIDs, torque, encoder | `crates/hid-simucube-protocol/src/lib.rs`, `types.rs` |
| VRS VID/PIDs, torque, effects | `crates/hid-vrs-protocol/src/ids.rs`, `types.rs` |
| Cammus VID/PIDs, torque | `crates/hid-cammus-protocol/src/ids.rs`, `types.rs` |
| Asetek VID/PIDs, torque | `crates/hid-asetek-protocol/src/lib.rs`, `types.rs` |
| Heusinkveld VID/PIDs | `crates/hid-heusinkveld-protocol/src/lib.rs` |
| AccuForce VID/PIDs, torque | `crates/hid-accuforce-protocol/src/ids.rs`, `types.rs` |
| Leo Bodnar VID/PIDs, encoder | `crates/hid-leo-bodnar-protocol/src/ids.rs`, `report.rs`, `types.rs` |
| FFBeast VID/PIDs, encoder | `crates/engine/src/hid/vendor/ffbeast.rs` |
| OpenFFBoard VID/PIDs, encoder | `crates/engine/src/hid/vendor/openffboard.rs` |
| Granite Devices / OSW torque, encoder | `crates/simplemotion-v2/src/types.rs` |
| Cube Controls (provisional) | `crates/engine/src/hid/vendor/cube_controls.rs` |
| Supported vendor registry | `crates/engine/src/hid/vendor/mod.rs` |
| All VID/PID authoritative sources | `docs/protocols/SOURCES.md` |
