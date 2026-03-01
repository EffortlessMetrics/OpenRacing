# VID/PID Sources

This document records the authoritative source for every USB Vendor ID and Product ID used in the OpenRacing protocol crates. A source citation is **required** when adding a new device. This file is the golden reference; adding a wrong value here should fail the unit tests in `crates/hid-moza-protocol/tests/id_verification.rs`.

See friction log entry **F-005** for the history of why this document was created.

---

## Policy

- **Verified** — confirmed from an official USB descriptor dump, official SDK, or Linux kernel `hid-ids.h`.
- **Community** — confirmed from a community-maintained compatibility table (e.g., [JacKeTUs/linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels)).
- **Estimated** — assigned by OpenRacing based on logical extrapolation; **not independently verified**. Must be confirmed before release.

---

## Moza Racing

**VID:** `0x346E`  
**Source:** USB VID registry; [moza-racing/community USB captures](https://github.com/JacKeTUs/linux-steering-wheels); Moza HID descriptor dumps on the [iRacing forum](https://forums.iracing.com/discussion/44524).  
**Status:** Verified

| PID      | Device Name         | Status    |
|----------|---------------------|-----------|
| `0x0000` | R16 / R21 (V1)      | Verified  |
| `0x0002` | R9 (V1)             | Verified  |
| `0x0004` | R5 (V1)             | Verified  |
| `0x0005` | R3 (V1)             | Verified  |
| `0x0006` | R12 (V1)            | Verified  |
| `0x0010` | R16 / R21 (V2)      | Community |
| `0x0012` | R9 (V2)             | Community |
| `0x0014` | R5 (V2)             | Community |
| `0x0015` | R3 (V2)             | Community |
| `0x0016` | R12 (V2)            | Community |
| `0x0003` | SR-P Pedals         | Community |
| `0x0020` | HGP Shifter         | Community |
| `0x0021` | SGP Shifter         | Community |
| `0x0022` | HBP Handbrake       | Community |

---

## Simucube (Granite Devices)

**VID:** `0x16D0`  
**Source:** [Official Simucube developer documentation](https://github.com/Simucube/simucube-docs.github.io) (`docs/Simucube 2/Developers.md` — authoritative PID table); [JacKeTUs/linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels) compatibility table; [Granite Devices wiki udev rules](https://granitedevices.com/wiki/Using_Simucube_wheel_base_in_Linux); USB VID registry (MCS Electronics / OpenMoko VID shared for open hardware).  
**Status:** Verified

| PID      | Device Name                  | Status    |
|----------|------------------------------|-----------|
| `0x0D5A` | Simucube 1                   | Verified  |
| `0x0D5F` | Simucube 2 Ultimate (32 Nm)  | Verified  |
| `0x0D60` | Simucube 2 Pro (25 Nm)       | Verified  |
| `0x0D61` | Simucube 2 Sport (17 Nm)     | Verified  |
| `0x0D63` | SimuCUBE Wireless Wheel      | Estimated |
| `0x0D66` | Simucube SC-Link Hub (ActivePedal) | Verified  |

> **Note:** VID `0x16D0` is also used by Heusinkveld — disambiguation is by PID.
>
> **Bootloader PIDs** (firmware flashing only, not for HID protocol enumeration):
> `0x0D5E` — SC2 firmware-upgrade mode; `0x0D5B` — SC1 firmware-upgrade mode.
> (Source: Granite Devices wiki udev rules.)

---

## Asetek SimSports

**VID:** `0x2433`  
**Source:** [USB VID registry (the-sz.com)](https://www.the-sz.com/products/usbid/index.php?v=2433); [JacKeTUs/linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels) compatibility table.  
**Status:** Verified

| PID      | Device Name                  | Status    |
|----------|------------------------------|-----------|
| `0xF300` | Asetek Invicta (27 Nm)       | Verified  |
| `0xF301` | Asetek Forte (18 Nm)         | Verified  |
| `0xF303` | Asetek La Prima (12 Nm)      | Community |
| `0xF306` | Asetek Tony Kanaan Edition (27 Nm) | Community |

---

## Simagic

**VID (EVO generation):** `0x3670`  
**VID (Legacy / STM generic):** `0x0483`  
**Source:** [USB VID registry (the-sz.com)](https://www.the-sz.com/products/usbid/index.php?v=3670) for `0x3670`; [JacKeTUs/linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels); [JacKeTUs/simagic-ff](https://github.com/JacKeTUs/simagic-ff) kernel driver `hid-simagic.h` header for `0x0483:0x0522` and EVO PIDs; [VansonLeung/poc\_simagic\_control\_input\_api](https://github.com/VansonLeung/poc_simagic_control_input_api) C# DirectInput PoC for input axis ranges.  
**Protocol:** FFB uses 64-byte HID Output Reports (report IDs `0x01`, `0x03`, `0x04`, `0x05`, `0x0a`, `0x40`); settings use Feature Reports `0x80`/`0x81`. Periodic effects (square/triangle/sawtooth) are defined but produce no effect on hardware. See `hid-simagic-protocol` crate docs for full wire-format details.  
**Status:** EVO PIDs verified; legacy PID verified; accessory PIDs estimated.

| VID      | PID      | Device Name                          | Status    |
|----------|----------|--------------------------------------|-----------|
| `0x3670` | `0x0500` | Simagic EVO Sport                    | Verified  |
| `0x3670` | `0x0501` | Simagic EVO                          | Verified  |
| `0x3670` | `0x0502` | Simagic EVO Pro                      | Verified  |
| `0x3670` | `0x0600` | Simagic Alpha EVO                    | Estimated |
| `0x3670` | `0x0700` | Simagic Neo                          | Estimated |
| `0x3670` | `0x0701` | Simagic Neo Mini                     | Estimated |
| `0x3670` | `0x1001` | Simagic P1000 Pedals                 | Estimated |
| `0x3670` | `0x1002` | Simagic P2000 Pedals                 | Estimated |
| `0x3670` | `0x1003` | Simagic P1000A Pedals                | Estimated |
| `0x3670` | `0x2001` | Simagic H-Pattern Shifter            | Estimated |
| `0x3670` | `0x2002` | Simagic Sequential Shifter           | Estimated |
| `0x3670` | `0x3001` | Simagic Handbrake                    | Estimated |
| `0x3670` | `0x4001` | Simagic WR1 Steering Rim             | Estimated |
| `0x3670` | `0x4002` | Simagic GT1 Steering Rim             | Estimated |
| `0x3670` | `0x4003` | Simagic GT Neo Steering Rim          | Estimated |
| `0x3670` | `0x4004` | Simagic Formula Steering Rim         | Estimated |
| `0x0483` | `0x0522` | Alpha / Alpha Mini / M10 / Ultimate (Legacy) | Verified |

> **Note:** The legacy PID `0x0483:0x0522` is also used by the VRS DirectForce Pro (different device class). Disambiguation requires reading the USB `iProduct` string descriptor.

---

## Logitech

**VID:** `0x046D`  
**Source:** [Linux kernel `hid-ids.h`](https://github.com/torvalds/linux/blob/master/drivers/hid/hid-ids.h); [berarma/oversteer](https://github.com/berarma/oversteer) device list; [JacKeTUs/linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels).  
**Status:** Verified

| PID      | Device Name                         | Status   |
|----------|-------------------------------------|----------|
| `0xC294` | Driving Force / Formula EX          | Verified |
| `0xC293` | WingMan Formula Force GP            | Verified |
| `0xC291` | WingMan Formula Force               | Verified |
| `0xC295` | MOMO Racing                         | Verified |
| `0xCA03` | MOMO Racing 2                       | Verified |
| `0xCA04` | Vibration Wheel                     | Verified |
| `0xC298` | Driving Force Pro (DFP)             | Verified |
| `0xC299` | G25 (900°, belt-drive)              | Verified |
| `0xC29A` | Driving Force GT (DFGT)             | Verified |
| `0xC29B` | G27 (900°, belt-drive)              | Verified |
| `0xC29C` | Speed Force Wireless (Wii)          | Verified |
| `0xC24F` | G29 PlayStation/PC                  | Verified |
| `0xC260` | G29 Xbox (pre-production variant)   | Verified |
| `0xC261` | G920 V1 (pre-production)            | Verified |
| `0xC262` | G920 Xbox/PC                        | Verified |
| `0xC267` | G923 PlayStation/PC (TrueForce)     | Verified |
| `0xC26E` | G923 Xbox/PC (TrueForce)            | Verified |
| `0xC268` | G PRO PlayStation/PC                | Verified |
| `0xC272` | G PRO Xbox/PC                       | Verified |

---

## Thrustmaster

**VID:** `0x044F`  
**Source:** [Linux kernel `hid-thrustmaster.c`](https://github.com/torvalds/linux/blob/master/drivers/hid/hid-thrustmaster.c); [Kimplul/hid-tmff2](https://github.com/Kimplul/hid-tmff2) (community FFB driver); [JacKeTUs/linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels) compatibility table; [linux-hardware.org](https://linux-hardware.org); [devicehunt.com](https://devicehunt.com).  
**Status:** Verified (T300/TMX/T150/T248/TS-XW/TS-PC confirmed via hid-tmff2); others community-sourced.

| PID      | Device Name                        | Status    |
|----------|------------------------------------|-----------|
| `0xB65D` | FFB Wheel (generic pre-init PID)   | Verified  |
| `0xB65E` | T500 RS                            | Verified  |
| `0xB66D` | T300 RS (PS4 mode)                 | Verified  |
| `0xB66E` | T300 RS                            | Verified  |
| `0xB66F` | T300 RS GT                         | Verified  |
| `0xB669` | TX Racing (Xbox)                   | Verified  |
| `0xB677` | T150                               | Verified  |
| `0xB67F` | TMX (Xbox)                         | Verified  |
| `0xB689` | TS-PC Racer                        | Verified  |
| `0xB68D` | ~~T-LCM~~ → T.Flight Hotas One     | Removed   |
| `0xB691` | TS-XW (GIP/Xbox mode)              | Verified  |
| `0xB692` | TS-XW (USB/HID mode)               | Verified  |
| `0xB696` | T248                               | Verified  |
| `0xB69A` | T248X (Xbox, GIP)                  | Verified  |
| `0xB69B` | T818 (direct drive)                | Unverified|
| `0xB605` | NASCAR Pro Force Feedback 2 (legacy) | Community |
| `0xB651` | FGT Rumble Force (legacy)            | Community |
| `0xB653` | RGT FF Clutch (legacy)               | Community |
| `0xB654` | FGT Force Feedback (legacy)          | Community |
| `0xB65A` | F430 Force Feedback (legacy)         | Community |
| `0xB668` | T80 Racing Wheel (rumble only)       | Verified  |
| `0xB66A` | T80 Ferrari 488 GTB (no FFB)         | Verified  |
| `0xB664` | TX Racing Wheel (original PID)       | Verified  |
| `0xB678` | ~~T3PA~~ → T.Flight Rudder Pedals   | Removed   |
| `0xB679` | ~~T3PA Pro~~ → T-Rudder             | Removed   |

> **Removed PIDs (previously incorrect):**
> - `0xB68E` was listed as T-GT but is actually "TPR Rudder Bulk" (flight sim pedals) per linux-hardware.org.
> - `0xB68D` was listed as T-LCM but is actually "T.Flight Hotas One" (flight controller) per linux-hardware.org. Real T-LCM PID is unknown.
> - `0xB697` was listed as T248X but the actual T248X PID is `0xB69A` per linux-hardware.org.
> - `0xB678` was listed as T3PA but is actually "T.Flight Rudder Pedals" per devicehunt.com. T3PA connects via RJ12 to wheelbase.
> - `0xB679` was listed as T3PA Pro but is actually "T-Rudder" per devicehunt.com.
> - T-GT and T-GT II PIDs are unknown. Per hid-tmff2 README, T-GT II reuses T300 USB PIDs.

---

## Cammus

**VID:** `0x3416`  
**Source:** [JacKeTUs/linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels) compatibility table; [USB VID registry](https://www.the-sz.com/products/usbid/index.php?v=3416) (assigned to Shenzhen Cammus Electronic Technology Co., Ltd.).  
**Status:** Wheelbases verified; pedal PIDs community-sourced.

| PID      | Device Name           | Status    |
|----------|-----------------------|-----------|
| `0x0301` | Cammus C5 (5 Nm)      | Verified  |
| `0x0302` | Cammus C12 (12 Nm)    | Verified  |
| `0x1018` | Cammus CP5 Pedals     | Community |
| `0x1019` | Cammus LC100 Pedals   | Community |

> **Note:** A C15 / DDMAX (15 Nm) model has been announced; PID unknown — excluded pending USB capture.

---

## Fanatec

**VID:** `0x0EB7`  
**Source:** [USB VID registry](https://www.the-sz.com/products/usbid/index.php?v=0EB7) (Endor AG / Fanatec); community USB descriptor captures on [iRacing forum](https://forums.iracing.com/) and [SimHub GitHub issues](https://github.com/SHWotever/SimHub); [berarma/oversteer](https://github.com/berarma/oversteer).  
**Status:** Wheelbases verified; pedal PIDs community-sourced.

| PID      | Device Name                       | Status    |
|----------|-----------------------------------|-----------|
| `0x0001` | ClubSport Wheel Base V2 (8 Nm)    | Community |
| `0x0004` | CSL Elite Wheel Base (6 Nm)       | Community |
| `0x0005` | ClubSport Wheel Base V2.5 (8 Nm)  | Community |
| `0x0006` | Podium Wheel Base DD1 (20 Nm)     | Verified  |
| `0x0007` | Podium Wheel Base DD2 (25 Nm)     | Verified  |
| `0x0011` | CSL DD (legacy USB stack, 8 Nm)   | Community |
| `0x0020` | CSL DD (8 Nm)                     | Verified  |
| `0x0024` | Gran Turismo DD Pro (8 Nm)        | Verified  |
| `0x0E03` | CSL Elite V1 (6 Nm, alt SKU)      | Community |
| `0x6204` | ClubSport V2 (legacy HID stack)   | Community |
| `0x1839` | ClubSport Pedals V1/V2            | Community |
| `0x183B` | ClubSport Pedals V3               | Community |
| `0x6205` | CSL Pedals with Load Cell Kit     | Community |
| `0x6206` | CSL Pedals V2                     | Community |

---

## Heusinkveld

**VID:** `0x16D0`  
**Source:** Attributed to community USB descriptor captures; however, no specific external source has been independently verified (see audit note below).  
**Status:** ⚠ Unverified

| PID      | Device Name              | Status       |
|----------|--------------------------|--------------|
| `0x1156` | Heusinkveld Sprint       | ⚠ Unverified |
| `0x1157` | Heusinkveld Ultimate+    | ⚠ Unverified |
| `0x1158` | Heusinkveld Pro          | ⚠ Unverified |

> **Note:** VID `0x16D0` is also used by Simucube — disambiguation is by PID.

> **Audit (2026-03):** A prior version of this file cited
> [JacKeTUs/linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels)
> as a source. That repository covers FFB steering wheels only and contains **zero**
> Heusinkveld entries. The citation has been removed. These PIDs are not present in
> usb-ids.gowdy.us, devicehunt.com, linux-hardware.org, the Linux kernel `hid-ids.h`,
> systemd hwdb, or any other public USB database examined. A USB descriptor dump from
> real Heusinkveld hardware is needed to confirm them.

---

## FFBeast

**VID:** `0x045B`  
**Source:** [Linux kernel `hid-ids.h`](https://github.com/torvalds/linux/blob/master/drivers/hid/hid-ids.h) (`USB_VENDOR_ID_FFBEAST`); [HF-Robotics/FFBeast](https://github.com/HF-Robotics/FFBeast) project.  
**Status:** Verified

| PID      | Device Name         | Status   |
|----------|---------------------|----------|
| `0x58F9` | FFBeast Joystick    | Verified |
| `0x5968` | FFBeast Rudder      | Verified |
| `0x59D7` | FFBeast Wheel       | Verified |

---

## OpenFFBoard

**VID:** `0x1209`  
**Source:** [pid.codes open hardware registry — 1209:FFB0](https://pid.codes/1209/FFB0/); [Ultrawipf/OpenFFBoard](https://github.com/Ultrawipf/OpenFFBoard).  
**Status:** Verified

| PID      | Device Name                       | Status   |
|----------|-----------------------------------|----------|
| `0xFFB0` | OpenFFBoard (main firmware)       | Verified   |
| `0xFFB1` | OpenFFBoard (alternate firmware)  | Unverified |

> **Note:** PID `0xFFB1` is not registered at pid.codes (returns 404) and does not appear in
> the OpenFFBoard firmware source (`usb_descriptors.cpp` uses only `0xFFB0`). Retained as
> a placeholder for possible community builds.

---

## VRS DirectForce

**VID:** `0x0483`  
**Source:** [USB VID registry](https://www.the-sz.com/products/usbid/index.php?v=0483) (STMicroelectronics generic VID, used by many open/community devices); community USB descriptor captures; [VRS DirectForce Pro product page](https://www.vrs-true-force.com/).  
**Status:** DirectForce Pro verified; V2 and accessories community-sourced.

| PID      | Device Name                  | Status    |
|----------|------------------------------|-----------|
| `0xA355` | VRS DirectForce Pro          | Verified  |
| `0xA356` | VRS DirectForce Pro V2       | Community |
| `0xA357` | VRS Pedals (analog)          | Community |
| `0xA358` | VRS Pedals (load cell)       | Community |
| `0xA359` | VRS Handbrake                | Community |
| `0xA35A` | VRS Shifter                  | Community |

> **Note:** VID `0x0483` is also used by legacy Simagic devices (PID `0x0522`). Disambiguation requires reading the USB `iProduct` string descriptor.

---

## Leo Bodnar Electronics

**VID:** `0x1DD2`  
**Source:** [USB VID registry (the-sz.com)](https://www.the-sz.com/products/usbid/index.php?v=1DD2) (assigned to Leo Bodnar Electronics Ltd, UK); community USB descriptor captures and [linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels).  
**Status:** Wheel Interface / BBI-32 / SLI-M confirmed; BU0836 series estimated.

| PID      | Device Name                           | Status    |
|----------|---------------------------------------|-----------|
| `0x0001` | USB Joystick (generic input)          | Verified  |
| `0x000B` | BU0836A 12-bit joystick interface     | Estimated |
| `0x000C` | BBI-32 Button Box Interface (32 btn)  | Verified  |
| `0x000E` | USB Sim Racing Wheel Interface (PIDFF)| Verified  |
| `0x000F` | FFB Joystick (direct drive FF)        | Community |
| `0x0030` | BU0836X 12-bit joystick interface     | Estimated |
| `0x0031` | BU0836 16-bit joystick interface      | Estimated |
| `0x1301` | SLI-Pro Shift Light Indicator          | Estimated |

---

## SimXperience AccuForce

**VID:** `0x1FC9`  
**Source:** VID `0x1FC9` is assigned to NXP Semiconductors and is used by the NXP USB microcontrollers inside AccuForce wheelbases. Three independent sources confirm PID `0x804C`:
- [RetroBat Wheels.cs](https://github.com/RetroBat-Official/emulatorlauncher/blob/master/emulatorLauncher/Common/Wheels.cs) (commit 0a54752: `VID_1FC9&PID_804C`)
- [JacKeTUs/linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels) compatibility table (VID `1fc9`, PID `804c`, **Platinum** with `hid-pidff`)
- [Apkallu-Industries/Pitwall](https://github.com/Apkallu-Industries/Pitwall) `SimXAccuforce.xml` (`productId="804C" vendorId="1FC9"`)

**Status:** Community-sourced (multiple independent confirmations).

| PID      | Device Name                       | Status    |
|----------|-----------------------------------|-----------|
| `0x804C` | AccuForce Pro (V1 and V2)         | Community |

> **Note:** The AccuForce Pro V1 and V2 share the same VID/PID — no V2-specific
> PID has been observed in any public source.

---

## PXN (Shenzhen Jinyu Technology Co., Ltd.)

**VID:** `0x11FF`  
**Source:** [Linux kernel `hid-ids.h`](https://github.com/torvalds/linux/blob/master/drivers/hid/hid-ids.h) (`USB_VENDOR_ID_LITE_STAR`); [JacKeTUs/linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels) compatibility table.  
**Status:** Verified (kernel hid-ids.h)

| PID      | Device Name            | Status   |
|----------|------------------------|----------|
| `0x3245` | PXN V10 (direct drive) | Verified |
| `0x1212` | PXN V12                | Verified |
| `0x1112` | PXN V12 Lite           | Verified |
| `0x1211` | PXN V12 Lite SE        | Verified |
| `0x2141` | PXN GT987 FF           | Verified |

> **Note:** PXN VD4, VD6, and VD10 PIDs are unknown — they are not listed in the JacKeTUs compatibility table or any other public source at the time of writing.  
> `FFB_REPORT_ID = 0x05` is an estimate; standard PIDFF uses `0x01`. Verify against a USB capture when hardware is available.

---

## FlashFire

**VID:** `0x2F24`  
**Source:** [berarma/oversteer](https://github.com/berarma/oversteer) wheel_ids.py (`FF_FLASHFIRE_900R = '2f24:010d'`).  
**Status:** Community

| PID      | Device Name         | Status    |
|----------|---------------------|-----------|
| `0x010D` | FlashFire 900R      | Community |

---

## Guillemot (legacy Thrustmaster)

**VID:** `0x06F8`  
**Source:** [berarma/oversteer](https://github.com/berarma/oversteer) wheel_ids.py (`TM_FFRW = '06f8:0004'`); Linux kernel `hid-tmff.c`.  
**Status:** Community

| PID      | Device Name                       | Status    |
|----------|-----------------------------------|-----------|
| `0x0004` | Guillemot Force Feedback Racing Wheel | Community |

> **Note:** Guillemot was the parent company of Thrustmaster. Some older TM-branded wheels use this VID.

---

## VID Collision Map

Several vendors share a VID. Always disambiguate using the PID (and `iProduct` string if necessary).

| VID      | Users                              |
|----------|------------------------------------|
| `0x0483` | STMicroelectronics (generic): VRS DirectForce, legacy Simagic, Cube Controls |
| `0x16D0` | MCS Electronics / OpenMoko (open HW): Simucube 2, Heusinkveld |
| `0x044F` / `0x06F8` | Thrustmaster / Guillemot (parent company) |

---

## Verification Sources

The following external references were used during the verification waves documented in `docs/FRICTION_LOG.md`:

| Source | URL / Reference | Used For |
|--------|----------------|----------|
| JacKeTUs/universal-pidff | [github.com/JacKeTUs/universal-pidff](https://github.com/JacKeTUs/universal-pidff) | Linux kernel 6.15 PIDFF driver; authoritative VID/PID + quirk flags for Moza, Cammus, FFBeast, PXN, Simagic, and others |
| Linux kernel hid-ids.h | [torvalds/linux hid-ids.h](https://github.com/torvalds/linux/blob/master/drivers/hid/hid-ids.h) | Canonical USB VID/PID constants (`USB_VENDOR_ID_*`, `USB_DEVICE_ID_*`) for kernel-supported devices |
| pid.codes registry | [pid.codes](https://pid.codes/) | Open-hardware PID allocations; used for OpenFFBoard (`1209:FFB0`) |
| RetroBat Wheels.cs | [github.com/RetroBat/retrobat](https://github.com/RetroBat/retrobat) | AccuForce PID `0x804C`, Fanatec and Thrustmaster PID cross-reference |
| simracingcockpit.gg | [simracingcockpit.gg](https://simracingcockpit.gg/) | Torque spec cross-reference for Simagic EVO, Asetek, and Simucube 2 product lines |
| rF2SharedMemoryMap (rF2State.h) | [github.com/TheIronWolf/rF2SharedMemoryMapPlugin](https://github.com/TheIronWolf/rF2SharedMemoryMapPlugin) | Authoritative struct definitions for rFactor 2 shared memory telemetry adapter rewrite |
| berarma/oversteer | [github.com/berarma/oversteer](https://github.com/berarma/oversteer) | Linux steering wheel tool; Logitech, Fanatec, Thrustmaster PID cross-reference |
| devicehunt.com | [devicehunt.com](https://devicehunt.com/) | USB device database; Thrustmaster T500 RS PID correction |
| the-sz.com USB ID database | [the-sz.com/products/usbid](https://www.the-sz.com/products/usbid/) | USB VID lookups for Leo Bodnar, Asetek, Cammus, PXN, VRS, Fanatec |
| Kimplul/hid-tmff2 | [github.com/Kimplul/hid-tmff2](https://github.com/Kimplul/hid-tmff2) | Thrustmaster community FFB driver; T-GT II PID reuse confirmation, TS-XW correction; confirms T300RS family protocol (Report ID 0x60, 63-byte payloads) shared by T248/TX/TS-XW/TS-PC; T500RS **not** supported (issue #18); T150/TMX **not** supported |
| scarburato/t150_driver | [github.com/scarburato/t150_driver](https://github.com/scarburato/t150_driver) | T150/TMX FFB protocol documentation; confirms T150 uses a **different** protocol from T300RS (commands 0x40/0x41/0x43, 3-packet effect upload, USB interrupt OUT). Supports T150 (0xB677) and TMX (0xB67F). |
| scarburato/hid-tminit | [github.com/scarburato/hid-tminit](https://github.com/scarburato/hid-tminit) | Thrustmaster wheel init driver; model query/response table (T150 bytes 0x0603, T300 bytes 0x0602, T500 bytes 0x0200, TMX bytes 0x0704); init switch values per model |
| her001/tmdrv | [github.com/her001/tmdrv](https://github.com/her001/tmdrv) (archived: [gitlab.com/her0/tmdrv](https://gitlab.com/her0/tmdrv)) | Python init tool for T500RS, TX, TMX, TS-XW; mode-switch only, **no FFB support** |
| linux-hardware.org | [linux-hardware.org](https://linux-hardware.org/) | Hardware probe database; Thrustmaster PID `0xB677` correction (T500 RS → T150) |
| JacKeTUs/simagic-ff | [github.com/JacKeTUs/simagic-ff](https://github.com/JacKeTUs/simagic-ff) | Simagic kernel driver; legacy PID `0x0483:0x0522` verification; **FFB protocol**: report IDs `0x01`/`0x03`/`0x04`/`0x05`/`0x0a`/`0x40`, effect block types, `sm_rescale_signed_to_10k()` (±10000 scaling), 64-byte HID Output Reports; **settings**: Feature Reports `0x80`/`0x81` (max\_angle 90–2520, ff\_strength ±100, ring light, filter level, slew rate); periodic effects (square/triangle/sawtooth) defined but "no effect seen on wheelbase"; key commit: 52e73e7; files: `hid-simagic.c`, `hid-simagic.h`, `hid-simagic-settings.h`, `hid-simagic-settings.c` |
| VansonLeung/poc\_simagic\_control\_input\_api | [github.com/VansonLeung/poc\_simagic\_control\_input\_api](https://github.com/VansonLeung/poc_simagic_control_input_api) | C# DirectInput proof-of-concept for Simagic; confirms axes via Windows DirectInput (steering 0–65535 center 32767, throttle/brake 0–65535); no raw HID protocol details (uses SharpDX abstraction) |
| FFBeast project | [ffbeast.github.io](https://ffbeast.github.io/) | FFBeast VID/PID and torque scale documentation |
| Ultrawipf/OpenFFBoard | [github.com/Ultrawipf/OpenFFBoard](https://github.com/Ultrawipf/OpenFFBoard) | OpenFFBoard firmware source; PID `0xFFB0` confirmation |

---

## Adding a New Device

1. Obtain the VID from the USB-IF registry or an official vendor SDK.
2. Obtain the PID from a USB descriptor dump (`lsusb -v`, USBTreeView, or Wireshark/Zadig capture) or official documentation.
3. Add a row to the appropriate vendor table above with the correct **Status** tag.
4. Update the constants in the relevant `crates/hid-*-protocol/src/ids.rs` file.
5. If the test in `crates/hid-moza-protocol/tests/id_verification.rs` needs updating (new Moza device), add the assertion there.

---

## Devices Under Investigation

The following devices are known to exist but lack confirmed USB VID/PID values. Community USB captures are needed.

| Device | Status | Notes |
|--------|--------|-------|
| Turtle Beach VelocityOne Race | VID unknown | Not in linux-steering-wheels or hwdb; audio VID 0x1C59 does not apply |
| Cube Controls GT Pro V2 / Formula CSX-3 / GT-X2 / F-CORE | PIDs unverified | Input-only steering wheels (button boxes), NOT wheelbases. VID 0x0483 (STMicro shared) plausible; PIDs 0x0C73–0x0C75 are internal estimates not found in devicehunt.com, RetroBat Wheels.cs, SDL GameControllerDB, or any USB database. JacKeTUs/linux-steering-wheels, Reddit, and RaceDepartment checked 2025-07 — no entries. These devices do not produce force feedback. |
| Cammus C15 / DDMAX (15 Nm) | PID unknown | Announced; not yet in community tables |
| Simucube 3 | Not yet released | No public USB descriptor at time of writing |
| Gomez Racer devices | Unknown | No public VID/PID found in any community source |
| SIMTAG pedals | Unknown | No public VID/PID found in any community source |
| PXN VD4 / VD6 / VD10 | PIDs unknown | Not in JacKeTUs table or any other public source |

To contribute a USB capture, follow the guide in `docs/CONTRIBUTING_CAPTURES.md` (to be created).
