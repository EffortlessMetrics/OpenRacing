//! Fanatec USB vendor and product ID constants.
//!
//! Verified against the community Linux kernel driver
//! [`gotzl/hid-fanatecff`](https://github.com/gotzl/hid-fanatecff) (`hid-ftec.h`)
//! and the USB device table in `hid-ftec.c`.
//!
//! ## Web-verification sources (2025-07)
//!
//! | # | Source | Result |
//! |---|--------|--------|
//! | 1 | `gotzl/hid-fanatecff` `hid-ftec.h` + `hid-ftec.c` device table | Primary: all driver-supported PIDs confirmed |
//! | 2 | `gotzl/hid-fanatecff` README known-devices list | Confirms PIDs + product names |
//! | 3 | `the-sz.com` USB ID DB (`?v=0x0EB7`) | VID confirmed as "Endor AG" / "Corsair Memory Inc. (Fanatec)"; only PID 0x038E listed |
//! | 4 | `usb-ids.gowdy.us/read/UD/0EB7` | VID entry present, no PIDs listed (incomplete) |
//! | 5 | `linux-hardware.org/?id=usb:0eb7` | VID confirmed "Endor"; hardware probes exist |
//! | 6 | `libsdl-org/SDL` `src/joystick/SDL_joystick.c` | VID 0x0EB7 recognized as Fanatec wheel vendor |
//! | 7 | Linux kernel `drivers/hid/hid-ids.h` | No Fanatec entries (driver is out-of-tree) |
//! | 8 | `wine-mirror/wine` `dlls/winebus.sys/main.c` HIDRAW whitelist | PID 0x1839 confirmed as "Fanatec ClubSport Pedals v1/v2" |
//! | 9 | `linux-hardware.org/?id=usb:0eb7-1839` | PID 0x1839 = "Clubsport Pedals" by Endor/Fanatec (real hardware probe) |
//!
//! PIDs 0x0024 and 0x01E9 have **no external confirmation** in any
//! public database or open-source driver. The community driver confirms
//! that GT DD Pro, CSL DD, and ClubSport DD all share PID 0x0020 in PC mode
//! (`gotzl/hid-fanatecff` README + issue #21). The unverified PIDs may be
//! console-mode or firmware-variant enumerations.
//! PID 0x1839 was upgraded to verified in 2025-07 via Wine and linux-hardware.org.

#![deny(static_mut_refs)]

/// Fanatec USB vendor ID (Endor AG).
///
/// Web-verified (2025-07):
/// - `gotzl/hid-fanatecff` `hid-ftec.h`: `FANATEC_VENDOR_ID 0x0eb7`
/// - `the-sz.com` USB ID DB: VID 0x0EB7 = "Endor AG" / "Corsair Memory Inc. (Fanatec)"
/// - `linux-hardware.org`: VID 0x0EB7 = "Endor"
pub const FANATEC_VENDOR_ID: u16 = 0x0EB7;

/// Report IDs used in the Fanatec HID protocol.
///
/// Note: The Linux driver (`gotzl/hid-fanatecff`) accesses HID output reports
/// via the kernel `hid_report` API (filling `field[0]->value[0..6]`), so the
/// report ID is implicit. Commands prefixed with `0xf8` (LED, display, range,
/// tuning) and slot-based FFB commands (`(slot_id<<4)|flags`) are all sent
/// over the same HID output report. Our crate prepends an explicit report ID
/// byte for raw HID write compatibility.
pub mod report_ids {
    /// Standard input report (steering, pedals, buttons).
    pub const STANDARD_INPUT: u8 = 0x01;
    /// Extended input / telemetry report (high-res steering, temperatures, fault flags).
    pub const EXTENDED_INPUT: u8 = 0x02;
    /// Mode-switch feature report ID.
    pub const MODE_SWITCH: u8 = 0x01;
    /// Force feedback output report ID.
    pub const FFB_OUTPUT: u8 = 0x01;
    /// LED / display / rumble output report ID.
    pub const LED_DISPLAY: u8 = 0x08;
}

/// FFB command bytes carried in output report 0x01.
///
/// The Linux driver (`hid-ftecff.c`) uses a slot-based protocol where byte 0
/// is `(slot_id << 4) | flags` and byte 1 is the slot command (0x08 for
/// constant force, 0x0b for spring, 0x0c for damper/inertia/friction).
/// The constants below are used in our higher-level report encoding.
pub mod ffb_commands {
    /// Constant force effect.
    pub const CONSTANT_FORCE: u8 = 0x01;
    /// Set steering rotation range (degrees, u16 LE in bytes 2–3).
    pub const SET_ROTATION_RANGE: u8 = 0x12;
    /// Set overall device gain (0–100 %).
    pub const SET_GAIN: u8 = 0x10;
    /// Stop all active effects.
    pub const STOP_ALL: u8 = 0x0F;
}

/// LED / display / rumble command bytes carried in output report 0x08.
///
/// The Linux driver (`hid-ftecff.c`) sends these via `0xf8`-prefixed commands:
/// - Wheel LEDs: `[0xf8, 0x09, 0x08, leds_hi, leds_lo, 0, 0]`
///   (LED bit order is reversed: first LED = highest bit).
/// - Wheelbase LEDs (CSL Elite, `FTEC_WHEELBASE_LEDS` quirk):
///   `[0xf8, 0x13, leds_lo, 0, 0, 0, 0]`.
/// - Display: `[0xf8, 0x09, 0x01, 0x02, seg1, seg2, seg3]`
///   with 7-segment encoding per character.
/// - Rumble (wheelbase): `[0xf8, 0x09, 0x01, 0x03, val_hi, val_mid, val_lo]`.
/// - Rumble (pedals): `[0xf8, 0x09, 0x01, 0x04, val_hi, val_mid, val_lo]`.
pub mod led_commands {
    /// Set rev-light LEDs on the attached steering-wheel rim.
    pub const REV_LIGHTS: u8 = 0x80;
    /// Set the numeric display (3-digit / OLED) on the attached rim.
    pub const DISPLAY: u8 = 0x81;
    /// Activate rumble motors on the attached rim.
    pub const RUMBLE: u8 = 0x82;
}

/// Known Fanatec wheelbase product IDs.
///
/// Web-verified (2025-07) against `gotzl/hid-fanatecff` `hid-ftec.h` defines,
/// the `hid_device_id` table in `hid-ftec.c`, and the driver README known-devices
/// list. Cross-referenced with `the-sz.com`, `linux-hardware.org`, and
/// `libsdl-org/SDL`.
pub mod product_ids {
    /// ClubSport Wheel Base V2 (8 Nm belt-drive).
    ///
    /// Web-verified (2025-07):
    /// - `hid-ftec.h`: `CLUBSPORT_V2_WHEELBASE_DEVICE_ID 0x0001`
    /// - `hid-ftec.c`: device table entry with `FTEC_FF` quirk
    /// - Driver README: "0EB7:0001 FANATEC ClubSport Wheel Base V2" (experimental)
    pub const CLUBSPORT_V2: u16 = 0x0001;
    /// ClubSport Wheel Base V2.5 (8 Nm belt-drive).
    ///
    /// Web-verified (2025-07):
    /// - `hid-ftec.h`: `CLUBSPORT_V25_WHEELBASE_DEVICE_ID 0x0004`
    /// - `hid-ftec.c`: device table entry with `FTEC_FF` quirk
    /// - Driver README: "0EB7:0004 FANATEC ClubSport Wheel Base V2.5" (experimental)
    pub const CLUBSPORT_V2_5: u16 = 0x0004;
    /// CSL Elite Wheel Base PS4 (6 Nm belt-drive, PlayStation variant).
    ///
    /// Web-verified (2025-07):
    /// - `hid-ftec.h`: `CSL_ELITE_PS4_WHEELBASE_DEVICE_ID 0x0005`
    /// - `hid-ftec.c`: device table entry with `FTEC_FF | FTEC_TUNING_MENU | FTEC_WHEELBASE_LEDS`
    /// - Driver README: "0EB7:0005 FANATEC CSL Elite Wheel Base PS4"
    pub const CSL_ELITE_PS4: u16 = 0x0005;
    /// Podium Wheel Base DD1 (20 Nm direct-drive).
    ///
    /// Web-verified (2025-07):
    /// - `hid-ftec.h`: `PODIUM_WHEELBASE_DD1_DEVICE_ID 0x0006`
    /// - `hid-ftec.c`: device table entry with `FTEC_FF | FTEC_TUNING_MENU | FTEC_HIGHRES`
    /// - Driver README: "0EB7:0006 Podium Wheel Base DD1" (experimental)
    pub const DD1: u16 = 0x0006;
    /// Podium Wheel Base DD2 (25 Nm direct-drive).
    ///
    /// Web-verified (2025-07):
    /// - `hid-ftec.h`: `PODIUM_WHEELBASE_DD2_DEVICE_ID 0x0007`
    /// - `hid-ftec.c`: device table entry with `FTEC_FF | FTEC_TUNING_MENU | FTEC_HIGHRES`
    /// - Driver README: "0EB7:0007 Podium Wheel Base DD2" (experimental)
    pub const DD2: u16 = 0x0007;
    /// CSR Elite / Forza Motorsport Wheel Base (legacy belt-drive).
    ///
    /// Web-verified (2025-07):
    /// - `hid-ftec.h`: `CSR_ELITE_WHEELBASE_DEVICE_ID 0x0011`
    /// - `hid-ftec.c`: device table entry with `FTEC_FF` quirk
    /// - Driver README: "0EB7:0011 CSR Elite/Forza Motorsport Wheel Base" (experimental)
    pub const CSR_ELITE: u16 = 0x0011;
    /// CSL DD (8 Nm direct-drive). Also covers DD Pro and ClubSport DD in PC mode.
    ///
    /// Web-verified (2025-07):
    /// - `hid-ftec.h`: `CSL_DD_WHEELBASE_DEVICE_ID 0x0020`
    /// - `hid-ftec.c`: device table entry with `FTEC_FF | FTEC_TUNING_MENU | FTEC_HIGHRES`
    /// - Driver README: "0EB7:0020 FANATEC CSL DD / DD Pro / ClubSport DD Wheel Base"
    ///
    /// **Note:** The community driver confirms DD Pro and ClubSport DD share this
    /// PID in PC mode. See `GT_DD_PRO` (0x0024) and `CLUBSPORT_DD` (0x01E9)
    /// for possible console/alternate-mode PIDs that lack external confirmation.
    pub const CSL_DD: u16 = 0x0020;
    /// Gran Turismo DD Pro (8 Nm direct-drive, possible PlayStation/GT-mode PID).
    ///
    /// **Unverified (2025-07):** Not present in any external source consulted:
    /// - Not in `gotzl/hid-fanatecff` (`hid-ftec.h`, `hid-ftec.c`, or README)
    /// - Not in `the-sz.com`, `linux-hardware.org`, or `libsdl-org/SDL`
    /// - Not in Linux kernel `hid-ids.h`
    /// - Not found via GitHub-wide code search for `fanatec` + `0x0024`
    ///
    /// **Key finding (2025-07):** `gotzl/hid-fanatecff` README and issue #21
    /// confirm the GT DD Pro enumerates as PID **0x0020** (same as CSL DD) in
    /// PC mode (red LED). In PS-compatibility mode (yellow LED), it reports as
    /// PID 0x0004 (ClubSport V2.5 ID, backwards-compat). PID 0x0024 has zero
    /// corroboration in any public source and may not be a real enumeration.
    /// Retained for completeness but callers should match `CSL_DD` (0x0020)
    /// as the primary PID for this device.
    pub const GT_DD_PRO: u16 = 0x0024;
    /// CSL Elite Wheel Base — PC mode (6 Nm belt-drive).
    ///
    /// Web-verified (2025-07):
    /// - `hid-ftec.h`: `CSL_ELITE_WHEELBASE_DEVICE_ID 0x0E03`
    /// - `hid-ftec.c`: device table entry with `FTEC_FF | FTEC_TUNING_MENU | FTEC_WHEELBASE_LEDS`
    /// - Driver README: "0EB7:0E03 FANATEC CSL Elite Wheel Base"
    pub const CSL_ELITE: u16 = 0x0E03;
    /// ClubSport DD+ (12 Nm direct-drive, 2022 premium base).
    ///
    /// **Unverified (2025-07):** Not present in any external source consulted:
    /// - Not in `gotzl/hid-fanatecff` (`hid-ftec.h`, `hid-ftec.c`, or README)
    /// - Not in `the-sz.com`, `linux-hardware.org`, or `libsdl-org/SDL`
    /// - Not in Linux kernel `hid-ids.h`
    /// - Not found via GitHub-wide code search for `fanatec` + `0x01E9`
    /// - `linux-hardware.org/?id=usb:0eb7-01e9` returns no hardware probes
    ///
    /// **Key finding (2025-07):** `gotzl/hid-fanatecff` README explicitly lists
    /// "0EB7:0020 FANATEC CSL DD / DD Pro / **ClubSport DD** Wheel Base",
    /// confirming the ClubSport DD shares PID 0x0020 in PC mode. PID 0x01E9
    /// has zero corroboration and may not be a real enumeration. Retained for
    /// completeness but callers should match `CSL_DD` (0x0020) as the primary
    /// PID for this device.
    pub const CLUBSPORT_DD: u16 = 0x01E9;

    // ── Standalone pedal devices ───────────────────────────────────────────

    /// ClubSport Pedals V1 / V2 (USB, 2-axis or 3-axis set).
    ///
    /// Web-verified (2025-07):
    /// - `wine-mirror/wine` `dlls/winebus.sys/main.c`: HIDRAW whitelist entry
    ///   `pid == 0x1839` with comment "Fanatec ClubSport Pedals v1/v2"
    /// - `linux-hardware.org/?id=usb:0eb7-1839`: hardware probe confirms
    ///   VID 0x0EB7 PID 0x1839 = "Clubsport Pedals" by Endor/Fanatec
    /// - Not in `gotzl/hid-fanatecff` (driver only lists V3 pedals as 0x183B)
    pub const CLUBSPORT_PEDALS_V1_V2: u16 = 0x1839;
    /// ClubSport Pedals V3 (USB, 3-axis with load cell brake).
    ///
    /// Web-verified (2025-07):
    /// - `hid-ftec.h`: `CLUBSPORT_PEDALS_V3_DEVICE_ID 0x183b`
    /// - `hid-ftec.c`: device table entry with `FTEC_PEDALS` quirk
    /// - Driver README: "0EB7:183b FANATEC ClubSport Pedals V3" (experimental)
    pub const CLUBSPORT_PEDALS_V3: u16 = 0x183B;
    /// CSL Elite Pedals (USB).
    ///
    /// Web-verified (2025-07):
    /// - `hid-ftec.h`: `CSL_ELITE_PEDALS_DEVICE_ID 0x6204`
    /// - `hid-ftec.c`: device table entry with `FTEC_PEDALS` quirk
    /// - Driver README: "0EB7:6204 FANATEC CSL Elite Pedals"
    pub const CSL_ELITE_PEDALS: u16 = 0x6204;
    /// CSL Pedals with Load Cell Kit (USB adapter).
    ///
    /// Web-verified (2025-07):
    /// - `hid-ftec.h`: `CSL_LC_PEDALS_DEVICE_ID 0x6205`
    /// - `hid-ftec.c`: device table entry with `FTEC_PEDALS` quirk
    /// - Driver README: "0EB7:6205 FANATEC CSL Pedals Loadcell" (experimental)
    pub const CSL_PEDALS_LC: u16 = 0x6205;
    /// CSL Pedals V2 (USB adapter, updated Hall sensors).
    ///
    /// Web-verified (2025-07):
    /// - `hid-ftec.h`: `CSL_LC_V2_PEDALS_DEVICE_ID 0x6206`
    /// - `hid-ftec.c`: device table entry with `FTEC_PEDALS` quirk
    /// - Driver README: "0EB7:6206 FANATEC CSL Pedals LC V2 Loadcell" (experimental)
    pub const CSL_PEDALS_V2: u16 = 0x6206;

    /// ClubSport Shifter — standalone USB shifter.
    ///
    /// **Verified:** `JacKeTUs/simracing-hwdb` `90-fanatec.hwdb`.
    pub const CLUBSPORT_SHIFTER: u16 = 0x1A92;

    /// ClubSport Handbrake — standalone USB handbrake.
    ///
    /// **Verified:** `JacKeTUs/simracing-hwdb` `90-fanatec.hwdb`.
    pub const CLUBSPORT_HANDBRAKE: u16 = 0x1A93;
}

/// Rim IDs reported in byte 0x1F of the standard input report (report ID 0x01).
///
/// These identify which steering wheel rim is attached to the wheelbase.
/// The community Linux driver reads this field via `data[0x1f]` in `ftecff_raw_event`.
///
/// Verified IDs are cross-referenced against `gotzl/hid-fanatecff` `hid-ftec.h`.
pub mod rim_ids {
    /// BMW GT2 steering wheel rim. **Unverified** — not present in community driver.
    pub const BMW_GT2: u8 = 0x01;
    /// ClubSport Steering Wheel Formula V2 — dual-clutch paddles, funky switch.
    /// Verified: `CLUBSPORT_STEERING_WHEEL_FORMULA_V2_ID 0x0a` in `hid-ftec.h`.
    pub const FORMULA_V2: u8 = 0x0A;
    /// ClubSport Steering Wheel Formula V2.5 / V2.5 X.
    /// **Unverified** — not present in community driver.
    pub const FORMULA_V2_5: u8 = 0x03;
    /// CSL Elite P1 V2 steering wheel rim.
    /// Verified: `CSL_STEERING_WHEEL_P1_V2 0x08` in `hid-ftec.h`.
    pub const CSL_ELITE_P1: u8 = 0x08;
    /// McLaren GT3 V2 — has funky switch, rotary encoders, dual clutch paddles.
    /// Verified: `CSL_ELITE_STEERING_WHEEL_MCLAREN_GT3_V2_ID 0x0b` in `hid-ftec.h`.
    pub const MCLAREN_GT3_V2: u8 = 0x0B;
    /// Podium Steering Wheel Porsche 911 GT3 R.
    /// Verified: `PODIUM_STEERING_WHEEL_PORSCHE_911_GT3_R_ID 0x0c` in `hid-ftec.h`.
    pub const PORSCHE_911_GT3_R: u8 = 0x0C;
    /// Porsche 918 RSR steering wheel rim.
    /// **Unverified** — not present in community driver; may overlap with a different rim.
    pub const PORSCHE_918_RSR: u8 = 0x05;
    /// ClubSport RS steering wheel rim. **Unverified** — not present in community driver.
    pub const CLUBSPORT_RS: u8 = 0x06;
    /// CSL Elite Steering Wheel WRC.
    /// Verified: `CSL_ELITE_STEERING_WHEEL_WRC_ID 0x12` in `hid-ftec.h`.
    /// Note: shares value 0x12 with `CLUBSPORT_STEERING_WHEEL_F1_IS_ID` in the
    /// Linux driver header; the driver does not distinguish between these rims.
    pub const WRC: u8 = 0x12;
    /// Podium Hub. **Unverified** — not present in community driver.
    pub const PODIUM_HUB: u8 = 0x09;
}
