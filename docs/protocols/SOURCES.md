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
**Source:** [JacKeTUs/linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels) compatibility table; [Granite Devices support portal](https://granitedevices.com/wiki/Simucube_2_USB_HID_protocol); USB VID registry (MCS Electronics / OpenMoko VID shared for open hardware).  
**Status:** Verified

| PID      | Device Name                  | Status    |
|----------|------------------------------|-----------|
| `0x0D5F` | Simucube 2 Ultimate (35 Nm)  | Verified  |
| `0x0D60` | Simucube 2 Pro (25 Nm)       | Verified  |
| `0x0D61` | Simucube 2 Sport (15 Nm)     | Verified  |
| `0x0D62` | Simucube ActivePedal         | Estimated |
| `0x0D63` | SimuCUBE Wireless Wheel      | Estimated |

> **Note:** VID `0x16D0` is also used by Heusinkveld — disambiguation is by PID.

---

## Asetek SimSports

**VID:** `0x2433`  
**Source:** [USB VID registry (the-sz.com)](https://www.the-sz.com/products/usbid/index.php?v=2433); [JacKeTUs/linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels) compatibility table.  
**Status:** Verified

| PID      | Device Name                  | Status    |
|----------|------------------------------|-----------|
| `0xF300` | Asetek Invicta (15 Nm)       | Verified  |
| `0xF301` | Asetek Forte (20 Nm)         | Verified  |
| `0xF303` | Asetek La Prima (10 Nm)      | Community |
| `0xF306` | Asetek Tony Kanaan Edition   | Community |

---

## Simagic

**VID (EVO generation):** `0x3670`  
**VID (Legacy / STM generic):** `0x0483`  
**Source:** [USB VID registry (the-sz.com)](https://www.the-sz.com/products/usbid/index.php?v=3670) for `0x3670`; [JacKeTUs/linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels); [JacKeTUs/simagic-ff](https://github.com/JacKeTUs/simagic-ff) kernel driver source for `0x0483:0x0522`.  
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
| `0xC294` | Driving Force / G27 compat mode     | Verified |
| `0xC299` | G25 (900°, belt-drive)              | Verified |
| `0xC29B` | G27 (900°, belt-drive)              | Verified |
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
**Source:** [JacKeTUs/linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels) compatibility table; [Linux kernel `hid-ids.h`](https://github.com/torvalds/linux/blob/master/drivers/hid/hid-ids.h).  
**Status:** Verified (T300/TMX confirmed); others community-sourced.

| PID      | Device Name                        | Status    |
|----------|------------------------------------|-----------|
| `0xB65D` | T150                               | Community |
| `0xB65E` | T150 Pro                           | Community |
| `0xB66D` | T300 RS (PS4 mode)                 | Verified  |
| `0xB66E` | T300 RS                            | Verified  |
| `0xB66F` | T300 RS GT                         | Community |
| `0xB669` | TX Racing (Xbox)                   | Community |
| `0xB677` | T500 RS                            | Community |
| `0xB67F` | TMX (Xbox)                         | Verified  |
| `0xB689` | TS-PC Racer                        | Community |
| `0xB68D` | T-LCM (load cell brake)            | Community |
| `0xB68E` | T-GT                               | Community |
| `0xB691` | TS-XW (Xbox)                       | Community |
| `0xB692` | T-GT II                            | Community |
| `0xB696` | T248                               | Community |
| `0xB697` | T248X (Xbox)                       | Community |
| `0xB69A` | T-LCM Pro                          | Community |
| `0xB69B` | T818 (direct drive)                | Community |
| `0xB678` | T3PA Pedal Set                     | Community |
| `0xB679` | T3PA Pro Pedal Set                 | Community |

---

## Cammus

**VID:** `0x3416`  
**Source:** [JacKeTUs/linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels) compatibility table; [USB VID registry](https://www.the-sz.com/products/usbid/index.php?v=3416) (assigned to Shenzhen Cammus Electronic Technology Co., Ltd.).  
**Status:** Verified

| PID      | Device Name        | Status   |
|----------|--------------------|----------|
| `0x0301` | Cammus C5 (5 Nm)   | Verified |
| `0x0302` | Cammus C12 (12 Nm) | Verified |

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
**Source:** [JacKeTUs/linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels) compatibility table; community USB descriptor captures via [SimHub](https://github.com/SHWotever/SimHub).  
**Status:** Community-sourced

| PID      | Device Name              | Status    |
|----------|--------------------------|-----------|
| `0x1156` | Heusinkveld Sprint       | Community |
| `0x1157` | Heusinkveld Ultimate+    | Community |
| `0x1158` | Heusinkveld Pro          | Community |

> **Note:** VID `0x16D0` is also used by Simucube — disambiguation is by PID.

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
| `0xFFB0` | OpenFFBoard (main firmware)       | Verified |
| `0xFFB1` | OpenFFBoard (alternate firmware)  | Verified |

---

## PXN

**VID:** `0x11FF`  
**Source:** [JacKeTUs/linux-steering-wheels](https://github.com/JacKeTUs/linux-steering-wheels) compatibility table.  
**Status:** Community-verified. FFB output report ID `0x05` is estimated — requires USB capture to confirm.

| PID      | Device Name                  | Status    |
|----------|------------------------------|-----------|
| `0x3245` | PXN V10 (10 Nm)              | Community |
| `0x1212` | PXN V12 (12 Nm)              | Community |
| `0x1112` | PXN V12 Lite (12 Nm compact) | Community |
| `0x1211` | PXN V12 Lite SE              | Community |
| `0x2141` | GT987 FF (Lite Star OEM)     | Community |

> **Note:** VD4, VD6, VD10+ PIDs are unknown — excluded until USB captures confirm.

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

## VID Collision Map

Several vendors share a VID. Always disambiguate using the PID (and `iProduct` string if necessary).

| VID      | Users                              |
|----------|------------------------------------|
| `0x0483` | STMicroelectronics (generic): VRS DirectForce, legacy Simagic |
| `0x16D0` | MCS Electronics / OpenMoko (open HW): Simucube 2, Heusinkveld |

---

## Adding a New Device

1. Obtain the VID from the USB-IF registry or an official vendor SDK.
2. Obtain the PID from a USB descriptor dump (`lsusb -v`, USBTreeView, or Wireshark/Zadig capture) or official documentation.
3. Add a row to the appropriate vendor table above with the correct **Status** tag.
4. Update the constants in the relevant `crates/hid-*-protocol/src/ids.rs` file.
5. If the test in `crates/hid-moza-protocol/tests/id_verification.rs` needs updating (new Moza device), add the assertion there.
