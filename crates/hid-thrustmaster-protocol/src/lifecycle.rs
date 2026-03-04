//! Thrustmaster wheel lifecycle: init, open, close, and range commands.
//!
//! # Protocol families (verified from Kimplul/hid-tmff2 kernel driver)
//!
//! Thrustmaster wheels fall into two lifecycle families that share the same
//! FFB effect wire format (T300RS) but differ in initialization and device
//! open/close sequences:
//!
//! ## Family A — T300RS
//!
//! - No setup interrupt commands needed
//! - Single-step open: `\[0x01, 0x05\]`
//! - Single-step close: `\[0x01, 0x00\]`
//! - Models: T300RS (PS3/PS4/F1 variants), T-GT, T-GT II
//! - Max range: 1080°
//!
//! ## Family B — T248 / TS-PC / TS-XW
//!
//! - 7 setup interrupt commands sent during initialization
//! - Two-step open: `\[0x01, 0x04\]` then `\[0x01, 0x05\]`
//! - Two-step close: `\[0x01, 0x05\]` then `\[0x01, 0x00\]`
//! - Models: T248, TS-PC Racer, TS-XW Racer
//! - Max range: 900° (T248) or 1080° (TS-PC, TS-XW)
//!
//! All wheels use Report ID 0x60 with 63-byte payload (USB mode).
//!
//! ## Sources
//!
//! - `hid-tmt300rs.c` — T300RS init, open/close
//! - `hid-tmt248.c` — T248 setup interrupts, open/close, range
//! - `hid-tmtspc.c` — TS-PC setup (identical to T248)
//! - `hid-tmtsxw.c` — TS-XW setup (identical to T248)

#![allow(dead_code)]

/// Which lifecycle family a Thrustmaster wheel belongs to.
///
/// This determines the init, open, and close command sequences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleFamily {
    /// T300RS family: no setup interrupts, single-step open/close.
    ///
    /// Models: T300RS, T-GT, T-GT II.
    T300rs,

    /// T248 / TS-PC / TS-XW family: 7 setup interrupts, two-step open/close.
    ///
    /// Models: T248, TS-PC Racer, TS-XW Racer.
    T248Family,
}

/// Maximum rotation range for a wheel model, in degrees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangeLimits {
    /// Minimum rotation range (degrees).
    pub min_degrees: u16,
    /// Maximum rotation range (degrees).
    pub max_degrees: u16,
}

/// T300RS range limits (verified from `t300rs_set_range()` in hid-tmt300rs.c).
pub const T300RS_RANGE: RangeLimits = RangeLimits {
    min_degrees: 40,
    max_degrees: 1080,
};

/// T248 range limits (verified from `t248_set_range()` in hid-tmt248.c).
pub const T248_RANGE: RangeLimits = RangeLimits {
    min_degrees: 140,
    max_degrees: 900,
};

/// TS-PC Racer range limits (verified from `tspc_set_range()` in hid-tmtspc.c).
pub const TSPC_RANGE: RangeLimits = RangeLimits {
    min_degrees: 140,
    max_degrees: 1080,
};

/// TS-XW Racer range limits (verified from `tsxw_set_range()` in hid-tmtsxw.c).
pub const TSXW_RANGE: RangeLimits = RangeLimits {
    min_degrees: 140,
    max_degrees: 1080,
};

// ---------------------------------------------------------------------------
// Setup interrupt commands (Family B only)
// ---------------------------------------------------------------------------

/// Number of setup interrupt commands for Family B wheels.
pub const SETUP_COMMAND_COUNT: usize = 7;

/// Setup interrupt commands shared by T248, TS-PC, and TS-XW.
///
/// These are sent via USB interrupt transfers during wheel initialization,
/// before FFB is available. The commands are identical across all three
/// wheel models in the kernel driver.
///
/// Source: `setup_arr[]` in hid-tmt248.c, hid-tmtspc.c, hid-tmtsxw.c.
pub const SETUP_COMMANDS: [&[u8]; SETUP_COMMAND_COUNT] = [
    &[0x42, 0x01],
    &[0x0a, 0x04, 0x90, 0x03],
    &[0x0a, 0x04, 0x00, 0x0c],
    &[0x0a, 0x04, 0x12, 0x10],
    &[0x0a, 0x04, 0x00, 0x06],
    &[0x0a, 0x04, 0x00, 0x0e],
    &[0x0a, 0x04, 0x00, 0x0e, 0x01],
];

// ---------------------------------------------------------------------------
// Open / close command builders
// ---------------------------------------------------------------------------

/// Build the T300RS-family (Family A) open command.
///
/// Source: `t300rs_send_open()` — single packet `{0x01, 0x05}`.
pub fn build_t300rs_open() -> [u8; 2] {
    [0x01, 0x05]
}

/// Build the T300RS-family (Family A) close command.
///
/// Source: `t300rs_send_close()` — single packet `{0x01, 0x00}`.
pub fn build_t300rs_close() -> [u8; 2] {
    [0x01, 0x00]
}

/// Build the T248/TS-PC/TS-XW (Family B) open command sequence.
///
/// Returns two commands that must be sent in order:
/// 1. `{0x01, 0x04}` — prepare
/// 2. `{0x01, 0x05}` — activate
///
/// Source: `t248_send_open()`, `tspc_send_open()`, `tsxw_send_open()`.
pub fn build_family_b_open() -> [[u8; 2]; 2] {
    [[0x01, 0x04], [0x01, 0x05]]
}

/// Build the T248/TS-PC/TS-XW (Family B) close command sequence.
///
/// Returns two commands that must be sent in order:
/// 1. `{0x01, 0x05}` — deactivate effects
/// 2. `{0x01, 0x00}` — close device
///
/// Source: `t248_send_close()`, `tspc_send_close()`, `tsxw_send_close()`.
pub fn build_family_b_close() -> [[u8; 2]; 2] {
    [[0x01, 0x05], [0x01, 0x00]]
}

// ---------------------------------------------------------------------------
// Range command builder (shared by all families)
// ---------------------------------------------------------------------------

/// Build a range command with clamping to the given limits.
///
/// The wire format is `[0x08, 0x11, lo, hi]` where the value is
/// `degrees * 0x3C` encoded as LE16.
///
/// Source: `t300rs_set_range()` — shared by all families.
pub fn build_range_command(degrees: u16, limits: RangeLimits) -> [u8; 4] {
    let clamped = degrees.clamp(limits.min_degrees, limits.max_degrees);
    let scaled = (clamped as u32) * 0x3C;
    let bytes = (scaled as u16).to_le_bytes();
    [0x08, 0x11, bytes[0], bytes[1]]
}

// ---------------------------------------------------------------------------
// Convenience: lifecycle for a specific model
// ---------------------------------------------------------------------------

/// Get the lifecycle family for a Thrustmaster product ID.
///
/// Returns `None` for unrecognized PIDs.
pub fn lifecycle_family_for_pid(pid: u16) -> Option<LifecycleFamily> {
    // T300RS family (Family A)
    // Source: hid-tmff2 README + hid-tmt300rs.c
    match pid {
        // T300RS PS3 normal mode
        0xb66e |
        // T300RS PS3 advanced mode / F1 attachment
        0xb66f |
        // T300RS PS4 mode
        0xb66d |
        // T-GT (reuses T300RS per README)
        0xb68e |
        // T-GT II (reuses T300RS per README)
        0xb68f => Some(LifecycleFamily::T300rs),

        // T248 (Family B)
        // Source: hid-tmt248.c
        0xb696 => Some(LifecycleFamily::T248Family),

        // TS-PC Racer (Family B)
        // Source: hid-tmtspc.c
        0xb689 => Some(LifecycleFamily::T248Family),

        // TS-XW Racer (Family B)
        // Source: hid-tmtsxw.c
        0xb692 => Some(LifecycleFamily::T248Family),

        _ => None,
    }
}

/// Get the range limits for a Thrustmaster product ID.
///
/// Returns `None` for unrecognized PIDs.
pub fn range_limits_for_pid(pid: u16) -> Option<RangeLimits> {
    match pid {
        // T300RS variants: 40–1080°
        0xb66e | 0xb66f | 0xb66d | 0xb68e | 0xb68f => Some(T300RS_RANGE),
        // T248: 140–900°
        0xb696 => Some(T248_RANGE),
        // TS-PC Racer: 140–1080°
        0xb689 => Some(TSPC_RANGE),
        // TS-XW Racer: 140–1080°
        0xb692 => Some(TSXW_RANGE),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Report descriptor metadata
// ---------------------------------------------------------------------------

/// T248 HID report descriptor input report metadata.
///
/// Source: `t248_pc_rdesc_fixed[]` in hid-tmt248.c.
pub mod t248_input {
    /// Report ID for T248 input reports.
    pub const REPORT_ID: u8 = 0x07;
    /// Steering axis (Usage: X) — 16-bit, range 0–65535.
    pub const STEERING_BITS: u8 = 16;
    /// Pedal axes (Y/Rz/Slider/Z) — 10-bit, range 0–1023.
    pub const PEDAL_BITS: u8 = 10;
    /// Number of buttons (Usage minimum 1, maximum 26).
    pub const BUTTON_COUNT: u8 = 26;
}

/// TS-PC / TS-XW HID report descriptor input report metadata.
///
/// Source: `tspc_pc_rdesc_fixed[]` / `tsxw_pc_rdesc_fixed[]`.
pub mod tspc_input {
    /// Report ID for TS-PC/TS-XW input reports.
    pub const REPORT_ID: u8 = 0x07;
    /// Steering axis (Usage: X) — 16-bit, range 0–65535.
    pub const STEERING_BITS: u8 = 16;
    /// Pedal axes (Rz/Z/Y) — 10-bit, range 0–1023.
    pub const PEDAL_BITS: u8 = 10;
    /// Number of buttons (Usage minimum 1, maximum 13).
    pub const BUTTON_COUNT: u8 = 13;
}

// ---------------------------------------------------------------------------
// Supported effects list
// ---------------------------------------------------------------------------

/// Force feedback effects supported by T248 / TS-PC / TS-XW (and T300RS).
///
/// All Thrustmaster wheels in the hid-tmff2 driver support the same set.
/// This matches the `t248_effects[]` / `tspc_effects[]` / `tsxw_effects[]`
/// arrays in the kernel source.
pub const SUPPORTED_EFFECTS: &[&str] = &[
    "FF_CONSTANT",
    "FF_RAMP",
    "FF_SPRING",
    "FF_DAMPER",
    "FF_FRICTION",
    "FF_INERTIA",
    "FF_PERIODIC",
    "FF_SINE",
    "FF_TRIANGLE",
    "FF_SQUARE",
    "FF_SAW_UP",
    "FF_SAW_DOWN",
    "FF_AUTOCENTER",
    "FF_GAIN",
];

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Setup commands
    // -----------------------------------------------------------------------

    #[test]
    fn setup_commands_count() {
        assert_eq!(SETUP_COMMANDS.len(), 7);
    }

    #[test]
    fn setup_command_0_is_0x42_0x01() {
        assert_eq!(SETUP_COMMANDS[0], &[0x42, 0x01]);
    }

    #[test]
    fn setup_command_6_has_five_bytes() {
        assert_eq!(SETUP_COMMANDS[6].len(), 5);
        assert_eq!(SETUP_COMMANDS[6], &[0x0a, 0x04, 0x00, 0x0e, 0x01]);
    }

    #[test]
    fn all_setup_commands_start_with_expected_prefix() {
        // Command 0 starts with 0x42, commands 1-6 start with 0x0a
        assert_eq!(SETUP_COMMANDS[0][0], 0x42);
        for cmd in &SETUP_COMMANDS[1..] {
            assert_eq!(cmd[0], 0x0a);
        }
    }

    // -----------------------------------------------------------------------
    // Open / close commands
    // -----------------------------------------------------------------------

    #[test]
    fn t300rs_open_command() {
        assert_eq!(build_t300rs_open(), [0x01, 0x05]);
    }

    #[test]
    fn t300rs_close_command() {
        assert_eq!(build_t300rs_close(), [0x01, 0x00]);
    }

    #[test]
    fn family_b_open_two_steps() {
        let cmds = build_family_b_open();
        assert_eq!(cmds[0], [0x01, 0x04]);
        assert_eq!(cmds[1], [0x01, 0x05]);
    }

    #[test]
    fn family_b_close_two_steps() {
        let cmds = build_family_b_close();
        assert_eq!(cmds[0], [0x01, 0x05]);
        assert_eq!(cmds[1], [0x01, 0x00]);
    }

    #[test]
    fn family_b_close_step1_equals_t300rs_open() {
        // The first close command is identical to the open command
        let close = build_family_b_close();
        let open = build_t300rs_open();
        assert_eq!(close[0], open);
    }

    #[test]
    fn family_b_close_step2_equals_t300rs_close() {
        let close = build_family_b_close();
        let t300_close = build_t300rs_close();
        assert_eq!(close[1], t300_close);
    }

    // -----------------------------------------------------------------------
    // Range commands
    // -----------------------------------------------------------------------

    #[test]
    fn range_900_t300rs() {
        let cmd = build_range_command(900, T300RS_RANGE);
        assert_eq!(cmd[0], 0x08);
        assert_eq!(cmd[1], 0x11);
        let value = u16::from_le_bytes([cmd[2], cmd[3]]);
        assert_eq!(value, (900u32 * 0x3C) as u16);
    }

    #[test]
    fn range_clamped_below_t248() {
        let cmd = build_range_command(50, T248_RANGE);
        let value = u16::from_le_bytes([cmd[2], cmd[3]]);
        assert_eq!(value, (140u32 * 0x3C) as u16);
    }

    #[test]
    fn range_clamped_above_t248() {
        let cmd = build_range_command(1080, T248_RANGE);
        let value = u16::from_le_bytes([cmd[2], cmd[3]]);
        assert_eq!(value, (900u32 * 0x3C) as u16);
    }

    #[test]
    fn range_clamped_below_t300rs() {
        let cmd = build_range_command(10, T300RS_RANGE);
        let value = u16::from_le_bytes([cmd[2], cmd[3]]);
        assert_eq!(value, (40u32 * 0x3C) as u16);
    }

    #[test]
    fn range_1080_max_t300rs() {
        let cmd = build_range_command(1080, T300RS_RANGE);
        let value = u16::from_le_bytes([cmd[2], cmd[3]]);
        assert_eq!(value, (1080u32 * 0x3C) as u16);
    }

    #[test]
    fn range_140_tspc() {
        let cmd = build_range_command(140, TSPC_RANGE);
        let value = u16::from_le_bytes([cmd[2], cmd[3]]);
        assert_eq!(value, (140u32 * 0x3C) as u16);
    }

    // -----------------------------------------------------------------------
    // Lifecycle family lookup
    // -----------------------------------------------------------------------

    #[test]
    fn t300rs_ps3_is_family_a() {
        assert_eq!(
            lifecycle_family_for_pid(0xb66e),
            Some(LifecycleFamily::T300rs)
        );
    }

    #[test]
    fn t300rs_ps4_is_family_a() {
        assert_eq!(
            lifecycle_family_for_pid(0xb66d),
            Some(LifecycleFamily::T300rs)
        );
    }

    #[test]
    fn tgt_is_family_a() {
        assert_eq!(
            lifecycle_family_for_pid(0xb68e),
            Some(LifecycleFamily::T300rs)
        );
    }

    #[test]
    fn tgt_ii_is_family_a() {
        assert_eq!(
            lifecycle_family_for_pid(0xb68f),
            Some(LifecycleFamily::T300rs)
        );
    }

    #[test]
    fn t248_is_family_b() {
        assert_eq!(
            lifecycle_family_for_pid(0xb696),
            Some(LifecycleFamily::T248Family)
        );
    }

    #[test]
    fn tspc_is_family_b() {
        assert_eq!(
            lifecycle_family_for_pid(0xb689),
            Some(LifecycleFamily::T248Family)
        );
    }

    #[test]
    fn tsxw_is_family_b() {
        assert_eq!(
            lifecycle_family_for_pid(0xb692),
            Some(LifecycleFamily::T248Family)
        );
    }

    #[test]
    fn unknown_pid_returns_none() {
        assert_eq!(lifecycle_family_for_pid(0x0000), None);
    }

    // -----------------------------------------------------------------------
    // Range limits lookup
    // -----------------------------------------------------------------------

    #[test]
    fn t300rs_range_limits() {
        let r = range_limits_for_pid(0xb66e);
        assert_eq!(r, Some(T300RS_RANGE));
        assert_eq!(r.map(|l| l.max_degrees), Some(1080));
    }

    #[test]
    fn t248_range_limits() {
        let r = range_limits_for_pid(0xb696);
        assert_eq!(r, Some(T248_RANGE));
        assert_eq!(r.map(|l| l.max_degrees), Some(900));
    }

    #[test]
    fn tspc_range_limits() {
        let r = range_limits_for_pid(0xb689);
        assert_eq!(r, Some(TSPC_RANGE));
        assert_eq!(r.map(|l| l.min_degrees), Some(140));
    }

    #[test]
    fn tsxw_range_limits() {
        let r = range_limits_for_pid(0xb692);
        assert_eq!(r, Some(TSXW_RANGE));
    }

    #[test]
    fn unknown_pid_range_none() {
        assert_eq!(range_limits_for_pid(0x0000), None);
    }

    // -----------------------------------------------------------------------
    // Input report metadata
    // -----------------------------------------------------------------------

    #[test]
    fn t248_has_26_buttons() {
        assert_eq!(t248_input::BUTTON_COUNT, 26);
    }

    #[test]
    fn tspc_has_13_buttons() {
        assert_eq!(tspc_input::BUTTON_COUNT, 13);
    }

    #[test]
    fn all_use_report_id_7_for_input() {
        assert_eq!(t248_input::REPORT_ID, 0x07);
        assert_eq!(tspc_input::REPORT_ID, 0x07);
    }

    // -----------------------------------------------------------------------
    // Supported effects
    // -----------------------------------------------------------------------

    #[test]
    fn supported_effects_count() {
        assert_eq!(SUPPORTED_EFFECTS.len(), 14);
    }

    #[test]
    fn supported_effects_include_constant_and_gain() {
        assert!(SUPPORTED_EFFECTS.contains(&"FF_CONSTANT"));
        assert!(SUPPORTED_EFFECTS.contains(&"FF_GAIN"));
    }

    // -----------------------------------------------------------------------
    // Property tests
    // -----------------------------------------------------------------------

    #[test]
    fn range_command_always_has_correct_header() {
        for degrees in [0, 40, 140, 450, 900, 1080, 2000, u16::MAX] {
            let cmd = build_range_command(degrees, T300RS_RANGE);
            assert_eq!(cmd[0], 0x08, "header byte 0");
            assert_eq!(cmd[1], 0x11, "header byte 1");
        }
    }

    #[test]
    fn range_command_clamping_is_monotonic() {
        let mut prev = 0u16;
        for degrees in (0..=1200).step_by(10) {
            let cmd = build_range_command(degrees, T300RS_RANGE);
            let value = u16::from_le_bytes([cmd[2], cmd[3]]);
            assert!(value >= prev, "non-monotonic at {degrees}°");
            prev = value;
        }
    }

    #[test]
    fn all_pids_have_consistent_family_and_range() {
        // Every PID with a lifecycle family must also have range limits
        let pids: &[u16] = &[
            0xb66e, 0xb66f, 0xb66d, 0xb68e, 0xb68f, // Family A
            0xb696, 0xb689, 0xb692, // Family B
        ];
        for &pid in pids {
            assert!(
                lifecycle_family_for_pid(pid).is_some(),
                "PID 0x{pid:04x} missing lifecycle family"
            );
            assert!(
                range_limits_for_pid(pid).is_some(),
                "PID 0x{pid:04x} missing range limits"
            );
        }
    }
}
