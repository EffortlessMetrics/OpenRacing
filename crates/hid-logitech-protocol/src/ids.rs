//! Logitech USB vendor and product ID constants.
//!
//! # Sources
//!
//! Protocol details in this module were cross-referenced against these
//! open-source Linux driver projects:
//!
//! - **Linux kernel `hid-lg4ff.c`** — `torvalds/linux drivers/hid/hid-lg4ff.c`
//!   (in-tree driver; supports G25 through G29 via the classic Logitech FFB
//!   slot protocol).
//! - **new-lg4ff** — `berarma/new-lg4ff` (out-of-tree driver; extends the
//!   kernel driver with full FF_SPRING / FF_DAMPER / FF_FRICTION support,
//!   high-resolution timer, and G923 PS mode switching).
//! - **oversteer** — `berarma/oversteer` (Linux GUI for Logitech / Thrustmaster
//!   / Fanatec wheels; provides the most complete PID list including G PRO).
//!
//! The G920 and G923 Xbox/PC variant (0xC26E) use the **HID++** protocol
//! rather than the classic lg4ff slot protocol. They are driven by the
//! `hid-logitech-hidpp` kernel module since kernel 6.3. See the new-lg4ff
//! README for details.

#![deny(static_mut_refs)]

/// Logitech USB vendor ID.
pub const LOGITECH_VENDOR_ID: u16 = 0x046D;

/// Report IDs used in the Logitech HID protocol.
///
/// The vendor report ID 0xF8 carries all extended commands (mode switch,
/// range, autocenter, LEDs). The constant-force and device-gain report IDs
/// are from the higher-level HID PID (Physical Interface Device) layer.
///
/// Note: the kernel `hid-lg4ff.c` and `new-lg4ff` drivers send FFB data
/// through a 7-byte HID output report whose report ID is implicit in the
/// HID descriptor, with the first data byte encoding the slot and operation
/// (e.g. `0x11` = slot 1 start). The report IDs below are from a
/// complementary abstraction layer.
pub mod report_ids {
    /// Standard input report (steering, pedals, buttons).
    pub const STANDARD_INPUT: u8 = 0x01;
    /// Vendor-specific feature/output report (init, range, LEDs, autocenter).
    ///
    /// Source: kernel `hid-lg4ff.c` — all extended commands use `0xf8` as the
    /// first byte of the 7-byte output report payload (e.g.
    /// `{0xf8, 0x0a, ...}` for mode reset, `{0xf8, 0x81, ...}` for set-range).
    pub const VENDOR: u8 = 0xF8;
    /// Set Constant Force effect output report.
    pub const CONSTANT_FORCE: u8 = 0x12;
    /// Device Gain output report.
    pub const DEVICE_GAIN: u8 = 0x16;
}

/// Command bytes carried in vendor report 0xF8.
///
/// # Mode-switch protocol (from kernel and new-lg4ff)
///
/// Logitech wheels support multiple compatibility modes. Newer wheels
/// (G27+) can emulate older models (DF-EX, DFP, G25, etc.). Mode
/// switching uses `0xf8`-prefixed 7-byte commands:
///
/// | Command | Byte 1 | Meaning |
/// |---------|--------|---------|
/// | EXT_CMD1  | `0x01` | Switch DFP to native mode |
/// | EXT_CMD9  | `0x09` | Extended mode switch (G27+) — byte 2 selects target |
/// | EXT_CMD10 | `0x0a` | Revert mode upon USB reset (sent before `0x09`) |
/// | EXT_CMD16 | `0x10` | Switch G25 to native mode |
///
/// For G27/DFGT/G29, mode switching is a two-step sequence:
/// 1. `{0xf8, 0x0a, 0, 0, 0, 0, 0}` — set revert-on-reset mode
/// 2. `{0xf8, 0x09, mode, 0x01, detach, 0, 0}` — switch with USB detach
///
/// For G923 PS (PID 0xC267 → 0xC266), the switch command must be sent
/// with HID report ID `0x30` instead of the default report ID
/// (see `lg4ff_switch_from_ps_mode` in `berarma/new-lg4ff`).
pub mod commands {
    /// "Revert mode upon USB reset" — tells the device which mode to
    /// return to after a USB bus reset.
    ///
    /// In practice this is the first step of a native-mode switch for
    /// G27+ wheels: send `{0xf8, 0x0a, 0, 0, 0, 0, 0}` followed by the
    /// appropriate `0x09` command.
    ///
    /// Source: kernel `lg4ff_mode_switch_ext09_*` arrays — every mode
    /// switch begins with `{0xf8, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00}`
    /// ("Revert mode upon USB reset").
    pub const NATIVE_MODE: u8 = 0x0A;
    /// Set wheel rotation range (G25/G27/DFGT/G29/G923).
    ///
    /// Payload: `{0xf8, 0x81, range_lo, range_hi, 0, 0, 0}`.
    ///
    /// Source: `lg4ff_set_range_g25()` in both kernel and new-lg4ff.
    pub const SET_RANGE: u8 = 0x81;
    /// Activate autocenter spring effect.
    ///
    /// The full autocenter sequence is:
    /// 1. `{0xfe, 0x0d, k, k, strength, 0, 0}` — set spring parameters
    /// 2. `{0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00}` — activate
    ///
    /// To deactivate: `{0xf5, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00}`.
    ///
    /// Source: `lg4ff_set_autocenter_default()` in both kernel and new-lg4ff.
    pub const SET_AUTOCENTER: u8 = 0x14;
    /// Set rev-light LEDs.
    pub const SET_LEDS: u8 = 0x12;
    /// Switch to target mode (G27+).
    ///
    /// Payload: `{0xf8, 0x09, mode_id, 0x01, detach, 0, 0}`.
    /// This is the second step of native-mode switching, after `NATIVE_MODE`.
    ///
    /// Source: `lg4ff_mode_switch_ext09_*` arrays in kernel `hid-lg4ff.c`.
    pub const MODE_SWITCH: u8 = 0x09;
}

/// Known Logitech wheel product IDs.
///
/// VID/PID values verified against these authoritative community sources:
///
/// - **Linux kernel `hid-ids.h`** (torvalds/linux `drivers/hid/hid-ids.h`)
/// - **new-lg4ff driver `hid-ids.h`** (berarma/new-lg4ff — out-of-tree driver
///   with broader G923 support)
/// - **oversteer `wheel_ids.py`** (berarma/oversteer — Linux GUI for racing
///   wheels, includes G PRO IDs)
///
/// # G923 dual-PID behaviour
///
/// The G923 PS/PC model has **two PIDs**: 0xC267 enumerates first in
/// PlayStation compatibility mode; a mode-switch command (sent via HID
/// report ID `0x30` — see `lg4ff_switch_from_ps_mode` in new-lg4ff)
/// re-enumerates the device as 0xC266 (native HID mode with full FFB).
/// The Xbox/PC variant always enumerates as 0xC26E.
///
/// # HID++ vs. classic lg4ff protocol
///
/// The G920 (0xC262) and G923 Xbox/PC (0xC26E) use the **HID++**
/// protocol, not the classic Logitech FFB slot protocol. They are
/// supported by the `hid-logitech-hidpp` kernel module (since kernel
/// 6.3). The new-lg4ff README explicitly states it is "not compatible
/// with the Logitech G920 … and XBOX/PC version of the Logitech G923".
///
/// G PRO and G PRO 2 are direct-drive wheels. G PRO 2 PIDs are not yet
/// present in any community driver source as of this writing.
pub mod product_ids {
    // ── Legacy / classic wheels ──────────────────────────────────────────

    /// MOMO Racing wheel (900°, 2.2 Nm gear-driven).
    ///
    /// Verified: linux-steering-wheels (Platinum, hid-logitech),
    /// oversteer `LG_MOMO = '046d:c295'`.
    pub const MOMO: u16 = 0xC295;
    /// Driving Force Pro (900°, belt-driven, first wheel with native mode switching).
    ///
    /// Verified: linux-steering-wheels (Platinum, hid-logitech),
    /// oversteer `LG_DFP = '046d:c298'`.
    pub const DRIVING_FORCE_PRO: u16 = 0xC298;
    /// Driving Force GT (900°, belt-driven, with shift LEDs).
    ///
    /// Verified: linux-steering-wheels (Platinum, hid-logitech),
    /// oversteer `LG_DFGT = '046d:c29a'`.
    pub const DRIVING_FORCE_GT: u16 = 0xC29A;
    /// Speed Force Wireless (Wii racing wheel).
    ///
    /// Verified: oversteer `LG_SFW = '046d:c29c'`,
    /// kernel `USB_DEVICE_ID_LOGITECH_WII_WHEEL = 0xc29c`.
    pub const SPEED_FORCE_WIRELESS: u16 = 0xC29C;
    /// MOMO Racing Force Feedback Wheel (second generation).
    ///
    /// Verified: kernel `USB_DEVICE_ID_LOGITECH_MOMO_WHEEL2 = 0xca03`,
    /// oversteer `LG_MOMO2 = '046d:ca03'`.
    pub const MOMO_2: u16 = 0xCA03;
    /// WingMan Formula Force GP (early FFB wheel, ~2000).
    ///
    /// Verified: kernel `USB_DEVICE_ID_LOGITECH_WINGMAN_FFG = 0xc293`,
    /// oversteer `LG_WFFG = '046d:c293'`.
    pub const WINGMAN_FORMULA_FORCE_GP: u16 = 0xC293;
    /// WingMan Formula Force (original, non-GP version).
    ///
    /// Verified: kernel `USB_DEVICE_ID_LOGITECH_WINGMAN_FF = 0xc291`,
    /// oversteer `LG_WFF = '046d:c291'`.
    pub const WINGMAN_FORMULA_FORCE: u16 = 0xC291;
    /// Vibration Wheel (basic rumble-only wheel).
    ///
    /// Verified: kernel `USB_DEVICE_ID_LOGITECH_VIBRATION_WHEEL = 0xca04`.
    pub const VIBRATION_WHEEL: u16 = 0xCA04;

    /// G25 racing wheel (900°, 2.5 Nm belt-drive).
    ///
    /// Verified: kernel `USB_DEVICE_ID_LOGITECH_G25_WHEEL = 0xc299`,
    /// new-lg4ff `0xc299`, oversteer `LG_G25 = '046d:c299'`.
    pub const G25: u16 = 0xC299;
    /// Driving Force / Formula EX wheel (270°, gear-driven, ~2.0 Nm).
    ///
    /// This is the *original* Logitech Driving Force (c. 2003). It also
    /// appears when a G25/G27/DFGT/G29 is in DF-EX compatibility
    /// (emulation) mode — the kernel's multimode switching table uses
    /// this PID as the base emulation target.
    ///
    /// Verified: kernel `USB_DEVICE_ID_LOGITECH_WHEEL = 0xc294`,
    /// new-lg4ff `0xc294`, oversteer `LG_DF = '046d:c294'`.
    pub const DRIVING_FORCE_EX: u16 = 0xC294;
    /// Deprecated alias — use [`DRIVING_FORCE_EX`] instead.
    #[deprecated(
        since = "0.1.0",
        note = "Use DRIVING_FORCE_EX; 0xC294 is DF/EX, not G27"
    )]
    pub const G27_A: u16 = DRIVING_FORCE_EX;
    /// G27 racing wheel (900°, 2.5 Nm belt-drive).
    ///
    /// Verified: kernel `USB_DEVICE_ID_LOGITECH_G27_WHEEL = 0xc29b`,
    /// new-lg4ff `0xc29b`, oversteer `LG_G27 = '046d:c29b'`.
    pub const G27: u16 = 0xC29B;
    /// G29 racing wheel (PlayStation/PC, 900°, 2.2 Nm).
    ///
    /// Verified: kernel `USB_DEVICE_ID_LOGITECH_G29_WHEEL = 0xc24f`,
    /// new-lg4ff `0xc24f`, oversteer `LG_G29 = '046d:c24f'`.
    pub const G29_PS: u16 = 0xC24F;
    /// G920 racing wheel (Xbox/PC, 900°, 2.2 Nm).
    ///
    /// Verified: kernel `USB_DEVICE_ID_LOGITECH_G920_WHEEL = 0xc262`,
    /// new-lg4ff `0xc262`, oversteer `LG_G920 = '046d:c262'`.
    pub const G920: u16 = 0xC262;
    /// G923 racing wheel — native/HID mode (after mode switch from PS compat).
    ///
    /// Not in mainline kernel. Oversteer labels this `LG_G923P` ("P" =
    /// PlayStation hardware variant; the PID itself is the *native* mode).
    ///
    /// # Protocol
    ///
    /// In native mode (0xC266), the G923 PS uses the **classic lg4ff slot
    /// protocol** (same 7-byte HID output reports as G25–G29). Standard FFB
    /// effects (constant, spring, damper, friction, periodic) are fully
    /// supported by `berarma/new-lg4ff`. The set-range command
    /// (`{0xf8, 0x81, ...}`) uses `lg4ff_set_range_g25()` (same as G29).
    ///
    /// # TrueForce
    ///
    /// TrueForce is a proprietary Logitech high-frequency haptic feedback
    /// system available on the G923. It provides vibrations beyond standard
    /// FFB effects, reportedly synchronized with game audio. **No public
    /// protocol documentation exists** — the TrueForce wire format is not
    /// present in any open-source driver (kernel hid-lg4ff, new-lg4ff,
    /// hid-logitech-hidpp, libhidpp, SDL, or libratbag) as of this
    /// writing. TrueForce is only accessible through the proprietary
    /// Logitech G HUB software on Windows. The G HUB SDK requires an NDA.
    ///
    /// Verified: new-lg4ff `USB_DEVICE_ID_LOGITECH_G923_WHEEL = 0xc266`,
    /// oversteer `LG_G923P = '046d:c266'`.
    pub const G923: u16 = 0xC266;
    /// G923 racing wheel — PlayStation compatibility mode (initial enumeration).
    ///
    /// The PS/PC G923 first enumerates with this PID; send the native-mode
    /// command to switch to [`G923`] (0xC266). Not in mainline kernel or
    /// oversteer (oversteer only sees native-mode devices).
    ///
    /// # Mode switching
    ///
    /// The mode-switch command must be sent with **HID report ID `0x30`**
    /// (not the default output report ID). The payload is:
    /// `{0xf8, 0x09, 0x07, 0x01, 0x01, 0x00, 0x00}` (mode byte `0x07`).
    /// Source: `lg4ff_mode_switch_30_g923` in `berarma/new-lg4ff`.
    ///
    /// The G923 PS mode identification mask is `0xff00` with result `0x3800`
    /// (from `lg4ff_g923_ident_info` in new-lg4ff), used to detect whether
    /// the wheel is in PS compat mode or has already been switched.
    ///
    /// Verified: new-lg4ff `USB_DEVICE_ID_LOGITECH_G923_PS_WHEEL = 0xc267`.
    pub const G923_PS: u16 = 0xC267;
    /// G923 racing wheel (Xbox/PC, 900°, 2.2 Nm, TrueForce).
    ///
    /// # Protocol
    ///
    /// The Xbox/PC G923 uses the **HID++ protocol** (NOT the classic lg4ff
    /// slot protocol). In the Linux kernel (`hid-logitech-hidpp.c`, since
    /// v6.3), it is registered with the same quirks as the G920:
    /// `HIDPP_QUIRK_CLASS_G920 | HIDPP_QUIRK_FORCE_OUTPUT_REPORTS`.
    /// HID++ communication uses three report IDs:
    ///   - `0x10`: short reports (7 bytes)
    ///   - `0x11`: long reports (20 bytes)
    ///   - `0x12`: very long reports (up to 64 bytes)
    ///
    /// The Xbox variant is **incompatible with new-lg4ff** (which uses the
    /// classic lg4ff slot protocol). The new-lg4ff README explicitly notes
    /// PIDs 0xC26D and 0xC26E as incompatible. (Note: 0xC26D does not appear
    /// in any kernel or driver PID table; it may be a pre-production PID or
    /// documentation error.)
    ///
    /// TrueForce hardware is present but, as with the PS variant, no public
    /// protocol documentation exists and no open-source driver implements
    /// TrueForce. See [`G923`] (0xC266) for detailed TrueForce notes.
    ///
    /// Verified: kernel `USB_DEVICE_ID_LOGITECH_G923_XBOX_WHEEL = 0xc26e`,
    /// new-lg4ff `0xc26e`, oversteer `LG_G923X = '046d:c26e'`.
    pub const G923_XBOX: u16 = 0xC26E;
    /// G PRO racing wheel (PlayStation/PC, direct drive, 11 Nm, 1080°).
    ///
    /// Not yet in mainline kernel or new-lg4ff.
    ///
    /// Verified: oversteer `LG_GPRO_PS = '046d:c268'`.
    pub const G_PRO: u16 = 0xC268;
    /// G PRO racing wheel (Xbox/PC, direct drive, 11 Nm, 1080°).
    ///
    /// Not yet in mainline kernel or new-lg4ff.
    ///
    /// Verified: oversteer `LG_GPRO_XBOX = '046d:c272'`.
    pub const G_PRO_XBOX: u16 = 0xC272;
}
