//! Simagic USB vendor and product ID constants.
//!
//! # Vendor IDs
//!
//! **Legacy VID `0x0483`** (STMicroelectronics generic):
//! Used by Alpha Mini, Alpha, Alpha Ultimate, and M10 wheelbases (all share
//! PID `0x0522`). These devices expose a HID PID descriptor and are handled
//! by the `hid-pidff` kernel driver on old firmware, or by `simagic-ff` on
//! post-fw-v159 firmware. The M10 specifically works via `hid-pidff` with
//! no additional driver needed on kernels ≥6.12.
//!
//! **Modern VID `0x3670`** (Shen Zhen Simagic Technology Co., Limited):
//! Used by the EVO generation of Simagic wheelbases (EVO Sport, EVO, EVO Pro)
//! and the GT Neo wheel rim. These require the `simagic-ff` driver for FFB.
//!
//! **Note on other alleged VIDs**: Some community sources mention VIDs `0x2D5C`
//! or `0x368E`; these are **not confirmed** by the `simagic-ff` kernel driver
//! source or the `linux-steering-wheels` compatibility table.
//!
//! # Sources (web-verified 2025-07)
//!
//! - the-sz.com/products/usbid: VID `0x3670` = "Shen Zhen Simagic Technology Co., Limited" ✅
//! - the-sz.com/products/usbid: VID `0x0483` = "STMicroelectronics" ✅
//! - usb-ids.gowdy.us: VID `0x0483` = "STMicroelectronics" (no Simagic products listed) ✅
//! - usb-ids.gowdy.us: VID `0x3670` not found (404) — too new for this database
//! - JacKeTUs/simagic-ff `hid-simagic.h` (commit 52e73e7):
//!   `USB_VENDOR_ID_SIMAGIC_ALPHA=0x0483`, `USB_VENDOR_ID_SIMAGIC=0x3670`,
//!   `USB_DEVICE_ID_SIMAGIC_ALPHA=0x0522`, `USB_DEVICE_ID_SIMAGIC_EVO=0x0500`,
//!   `USB_DEVICE_ID_SIMAGIC_EVO_1=0x0501`, `USB_DEVICE_ID_SIMAGIC_EVO_2=0x0502`
//! - JacKeTUs/linux-steering-wheels README.md compatibility table:
//!   M10=0x0483:0x0522 (hid-pidff, Silver), Alpha Mini/Alpha/Alpha Ultimate=0x0483:0x0522
//!   (simagic-ff, Silver), EVO Sport=0x3670:0x0500, EVO=0x3670:0x0501, EVO Pro=0x3670:0x0502
//! - JacKeTUs/simagic-ff README.md udev rules: VID `0x3670` for GT Neo + EVO,
//!   VID `0x0483` for older wheelbases
//! - JacKeTUs/simracing-hwdb `90-simagic.hwdb`: TB-RS Handbrake = `v3670p0A04` (PID `0x0A04`)
//!
//! **Note**: Simagic is NOT in mainline Linux kernel `hid-ids.h` — all PIDs are
//! confirmed only via the out-of-tree JacKeTUs/simagic-ff driver and community tables.

#![deny(static_mut_refs)]

/// Simagic EVO-generation USB Vendor ID (Shen Zhen Simagic Technology Co., Limited).
///
/// Verified: `USB_VENDOR_ID_SIMAGIC=0x3670` in JacKeTUs/simagic-ff `hid-simagic.h`.
/// Also confirmed by udev rules for "GT Neo, Evo wheelbases" in the same repo.
/// the-sz.com: "Shen Zhen Simagic Technology Co., Limited" (web-verified 2025-07).
pub const SIMAGIC_VENDOR_ID: u16 = 0x3670;

/// Legacy Simagic VID (STMicroelectronics generic, shared with VRS DirectForce Pro).
///
/// Verified: `USB_VENDOR_ID_SIMAGIC_ALPHA=0x0483` in JacKeTUs/simagic-ff `hid-simagic.h`.
/// the-sz.com: "STMicroelectronics"; usb-ids.gowdy.us: "STMicroelectronics" (web-verified 2025-07).
/// Used by: Alpha Mini, Alpha, Alpha Ultimate, and M10 (all share PID `0x0522`).
/// On old firmware (pre-v159), these devices expose a HID PID descriptor and work
/// with the kernel `hid-pidff` driver. On new firmware (post-v159 / SimPro v2),
/// they require the `simagic-ff` driver for FFB.
pub const SIMAGIC_LEGACY_VENDOR_ID: u16 = 0x0483;

/// Legacy Simagic PID shared by Alpha Mini / Alpha / Alpha Ultimate / M10.
///
/// Verified: `USB_DEVICE_ID_SIMAGIC_ALPHA=0x0522` in JacKeTUs/simagic-ff `hid-simagic.h`.
/// Also confirmed in JacKeTUs/linux-steering-wheels table (all four models list
/// VID=0x0483, PID=0x0522). The M10 on old firmware works via `hid-pidff`; the
/// Alpha variants on new firmware require `simagic-ff`.
pub const SIMAGIC_LEGACY_PID: u16 = 0x0522;

/// HID Report IDs used in this crate's Simagic HID protocol abstraction.
///
/// # ⚠ WARNING: These are NOT the real Simagic hardware report IDs
///
/// These report IDs are this crate's own **speculative** abstraction layer
/// and do NOT match what the hardware expects. The transport layer must
/// translate these to the real wire-format report IDs before sending.
///
/// ## Actual hardware report IDs (from kernel driver)
///
/// The `simagic-ff` kernel driver uses standard HID PID semantics with
/// 64-byte output reports. Each report's `field[0]->value` is a 64-element
/// array where `value[0]` is the report type ID:
///
/// | Kernel define                 | value\[0\] | value\[1\]         | Remaining fields               |
/// |-------------------------------|------------|--------------------|---------------------------------|
/// | `SM_SET_EFFECT_REPORT`        | `0x01`     | block_id           | type, duration(LE16), gain=0xFF, trigger=0xFF |
/// | `SM_SET_CONDITION_REPORT`     | `0x03`     | block_id           | right_coeff(±10k), left_coeff(±10k), center, deadband |
/// | `SM_SET_PERIODIC_REPORT`      | `0x04`     | block_id           | magnitude(±10k), offset(±10k), phase, period |
/// | `SM_SET_CONSTANT_REPORT`      | `0x05`     | block_id           | magnitude(±10k)                |
/// | `SM_EFFECT_OPERATION_REPORT`  | `0x0a`     | block_id           | op(1=start,3=stop), loop_count |
/// | `SM_SET_ENVELOPE_REPORT`      | `0x12`     | (envelope params)  |                                |
/// | `SM_SET_RAMP_FORCE_REPORT`    | `0x16`     |                    | (no effect observed on hardware) |
/// | `SM_SET_CUSTOM_FORCE_REPORT`  | `0x17`     |                    | (no effect observed on hardware) |
/// | `SM_SET_GAIN`                 | `0x40`     | gain >> 8          |                                |
///
/// ## Effect block type IDs (used in value\[1\])
///
/// | Define           | ID     | Linux FF type | Status               |
/// |------------------|--------|---------------|----------------------|
/// | `SM_CONSTANT`    | `0x01` | `FF_CONSTANT` | Working              |
/// | `SM_SINE`        | `0x02` | `FF_SINE`     | Working              |
/// | `SM_DAMPER`      | `0x05` | `FF_DAMPER`   | Working              |
/// | `SM_SPRING`      | `0x06` | `FF_SPRING`   | Working              |
/// | `SM_FRICTION`    | `0x07` | `FF_FRICTION` | Working              |
/// | `SM_INERTIA`     | `0x09` | `FF_INERTIA`  | Working              |
/// | `SM_RAMP_FORCE`  | `0x0e` | `FF_RAMP`     | No effect observed   |
/// | `SM_SQUARE`      | `0x0f` | `FF_SQUARE`   | No effect observed   |
/// | `SM_TRIANGLE`    | `0x10` | `FF_TRIANGLE` | No effect observed   |
/// | `SM_SAWTOOTH_UP` | `0x11` | `FF_SAW_UP`   | No effect observed   |
/// | `SM_SAWTOOTH_DOWN`| `0x12`| `FF_SAW_DOWN` | No effect observed   |
///
/// ## Settings Feature Reports
///
/// Settings are read/written via HID Feature Reports:
/// - **Report `0x80`** (set): write wheel parameters (max_angle 90–2520,
///   ff_strength ±100, mechanical centering/damper/friction/inertia 0–100,
///   game centering/inertia/damper/friction 0–200, ring light, filter level
///   0–20, slew rate 0–100).
/// - **Report `0x81`** (get): read current wheel status (same fields).
///
/// Source: JacKeTUs/simagic-ff `hid-simagic.c`, `hid-simagic-settings.h`
/// (commit 52e73e7).
pub mod report_ids {
    /// Standard input report (steering, pedals, buttons).
    pub const STANDARD_INPUT: u8 = 0x01;
    /// Extended input report with additional button states.
    pub const EXTENDED_INPUT: u8 = 0x02;
    /// Device info report.
    pub const DEVICE_INFO: u8 = 0x03;
    /// FFB constant force report.
    pub const CONSTANT_FORCE: u8 = 0x11;
    /// FFB spring effect report.
    pub const SPRING_EFFECT: u8 = 0x12;
    /// FFB damper effect report.
    pub const DAMPER_EFFECT: u8 = 0x13;
    /// FFB friction effect report.
    pub const FRICTION_EFFECT: u8 = 0x14;
    /// FFB sine effect report.
    pub const SINE_EFFECT: u8 = 0x15;
    /// FFB square effect report.
    pub const SQUARE_EFFECT: u8 = 0x16;
    /// FFB triangle effect report.
    pub const TRIANGLE_EFFECT: u8 = 0x17;
    /// Set rotation range.
    pub const ROTATION_RANGE: u8 = 0x20;
    /// Set device gain.
    pub const DEVICE_GAIN: u8 = 0x21;
    /// LED control report.
    pub const LED_CONTROL: u8 = 0x30;
    /// Quick release status query.
    pub const QUICK_RELEASE_STATUS: u8 = 0x40;
}

/// Known Simagic product IDs (VID `0x3670` EVO generation unless otherwise noted).
///
/// # Verified PIDs
///
/// The following PIDs are confirmed by the JacKeTUs/simagic-ff kernel driver
/// `hid-simagic.h` header and the linux-steering-wheels compatibility table:
///
/// | PID      | Model       | VID    | Source                          |
/// |----------|-------------|--------|---------------------------------|
/// | `0x0522` | M10 / Alpha Mini / Alpha / Alpha Ultimate | `0x0483` | `USB_DEVICE_ID_SIMAGIC_ALPHA` |
/// | `0x0500` | EVO Sport   | `0x3670` | `USB_DEVICE_ID_SIMAGIC_EVO`   |
/// | `0x0501` | EVO         | `0x3670` | `USB_DEVICE_ID_SIMAGIC_EVO_1` |
/// | `0x0502` | EVO Pro     | `0x3670` | `USB_DEVICE_ID_SIMAGIC_EVO_2` |
///
/// # Estimated PIDs
///
/// All other PIDs below (Alpha EVO, Neo, pedals, shifters, rims)
/// are **unverified estimates** — they do not appear in any open-source driver
/// or USB descriptor dump available at the time of writing.
/// The **Handbrake** PID (`0x0A04`) is verified via JacKeTUs/simracing-hwdb.
pub mod product_ids {
    // ── EVO generation wheelbases (VID 0x3670) ─────────────────────────────
    // Verified: JacKeTUs/simagic-ff `hid-simagic.h` and linux-steering-wheels.
    /// Simagic EVO Sport wheelbase (VID `0x3670`).
    ///
    /// Verified: `USB_DEVICE_ID_SIMAGIC_EVO=0x0500` in `hid-simagic.h`.
    /// linux-steering-wheels: Silver rating, driver `simagic-ff`.
    pub const EVO_SPORT: u16 = 0x0500;
    /// Simagic EVO wheelbase (VID `0x3670`).
    ///
    /// Verified: `USB_DEVICE_ID_SIMAGIC_EVO_1=0x0501` in `hid-simagic.h`.
    /// linux-steering-wheels: Silver rating, driver `simagic-ff`.
    pub const EVO: u16 = 0x0501;
    /// Simagic EVO Pro wheelbase (VID `0x3670`).
    ///
    /// Verified: `USB_DEVICE_ID_SIMAGIC_EVO_2=0x0502` in `hid-simagic.h`.
    /// linux-steering-wheels: Silver rating, driver `simagic-ff`.
    pub const EVO_PRO: u16 = 0x0502;

    // ── Newer wheelbases (VID 0x3670 assumed, PIDs estimated) ─────────────
    // UNVERIFIED: These PIDs are NOT present in any open-source driver source.
    // They are placeholders based on community speculation and may be wrong.
    /// Simagic Alpha EVO wheelbase (**estimated PID — not independently verified**).
    pub const ALPHA_EVO: u16 = 0x0600;
    /// Simagic Neo wheelbase (**estimated PID — not independently verified**).
    /// Note: GT Neo uses VID `0x3670` per simagic-ff udev rules, but its
    /// PID is not in the driver's device table.
    pub const NEO: u16 = 0x0700;
    /// Simagic Neo Mini wheelbase (**estimated PID — not independently verified**).
    pub const NEO_MINI: u16 = 0x0701;

    // ── Accessories (VID 0x3670 assumed, PIDs estimated) ─────────────────
    // UNVERIFIED: No open-source driver or USB descriptor dump confirms these.
    // Simagic rims/pedals connect to the wheelbase and report as part of the
    // composite device (per simagic-ff README: "all rims/pedals will work"
    // through the base, no separate driver handling needed).
    /// Simagic P1000 pedals (**estimated PID**).
    pub const P1000_PEDALS: u16 = 0x1001;
    /// Simagic P2000 pedals (**estimated PID**).
    pub const P2000_PEDALS: u16 = 0x1002;
    /// Simagic P1000A pedals (**estimated PID**).
    pub const P1000A_PEDALS: u16 = 0x1003;
    /// Simagic H-pattern shifter (**estimated PID**).
    pub const SHIFTER_H: u16 = 0x2001;
    /// Simagic Sequential shifter (**estimated PID**).
    pub const SHIFTER_SEQ: u16 = 0x2002;
    /// Simagic TB-RS Handbrake (VID `0x3670`, PID `0x0A04`).
    ///
    /// Verified (web-verified 2025-07):
    /// - JacKeTUs/simracing-hwdb `90-simagic.hwdb`: `v3670p0A04` with
    ///   `ID_INPUT_JOYSTICK=1` label "Simagic TB-RS Handbrake".
    /// - JacKeTUs is the author of both the `simagic-ff` kernel driver and
    ///   the `simracing-hwdb` udev database — same maintainer, two repos.
    /// - No contradicting PID found in any other open-source source.
    pub const HANDBRAKE: u16 = 0x0A04;
    /// Simagic WR1 steering wheel rim (**estimated PID**).
    pub const RIM_WR1: u16 = 0x4001;
    /// Simagic GT1 steering wheel rim (**estimated PID**).
    pub const RIM_GT1: u16 = 0x4002;
    /// Simagic GT Neo steering wheel rim (**estimated PID**).
    pub const RIM_GT_NEO: u16 = 0x4003;
    /// Simagic Formula steering wheel rim (**estimated PID**).
    pub const RIM_FORMULA: u16 = 0x4004;
}
