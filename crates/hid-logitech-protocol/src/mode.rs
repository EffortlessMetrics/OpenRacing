//! Logitech wheel mode-switch and range-setting command encoders.
//!
//! # Wire protocol (verified from berarma/new-lg4ff)
//!
//! Logitech wheels support a multi-mode compatibility system where newer
//! wheels (G27+) can emulate older models (DF-EX, DFP, G25, etc.).
//! Mode switching and range setting both use 7-byte HID output reports.
//!
//! ## Mode switch protocol
//!
//! Two-step sequence (for G27/DFGT/G29/G923):
//! 1. `[0xf8, 0x0a, 0, 0, 0, 0, 0]` — revert mode on USB reset
//! 2. `[0xf8, 0x09, mode_id, 0x01, detach, 0, 0]` — switch to target mode
//!
//! The `mode_id` byte selects the target compatibility mode:
//! - `0x00` = DF-EX
//! - `0x01` = DFP
//! - `0x02` = G25
//! - `0x03` = DFGT
//! - `0x04` = G27
//! - `0x05` = G29 (detach=0x01)
//! - `0x07` = G923 (detach=0x01)
//!
//! ## G923 PS mode switch
//!
//! The G923 PlayStation model initially enumerates as PID 0xC267 (PS mode).
//! The mode-switch command must be sent with **HID Report ID 0x30** (not the
//! default). After switching, the device re-enumerates as PID 0xC266.
//!
//! Source: `lg4ff_mode_switch_30_g923` in new-lg4ff.
//!
//! ## DFP native mode
//!
//! Single command: `[0xf8, 0x01, 0, 0, 0, 0, 0]`.
//! Source: `lg4ff_mode_switch_ext01_dfp` in new-lg4ff.
//!
//! ## G25 native mode
//!
//! Single command: `[0xf8, 0x10, 0, 0, 0, 0, 0]`.
//! Source: `lg4ff_mode_switch_ext16_g25` in new-lg4ff.
//!
//! ## Steering range (G25/G27/DFGT/G29/G923)
//!
//! `[0xf8, 0x81, range_lo, range_hi, 0, 0, 0]` (LE16, 40–900°).
//! Source: `lg4ff_set_range_g25()` in both kernel and new-lg4ff.
//!
//! ## DFP range
//!
//! Two-step sequence for DFP:
//! 1. `[0xf8, 0x81, range_lo, range_hi, 0, 0, 0]` (if range < 200 or > 900)
//! 2. `[0xf8, 0x03, range_lo, range_hi, 0, 0, 0]` (fine adjust, only if 200–900)
//!
//! Source: `lg4ff_set_range_dfp()` in kernel.
//!
//! ## LEDs (G27/G29)
//!
//! `[0xf8, 0x12, leds, 0, 0, 0, 0]` — 5 shift LEDs as a bitmask.
//! Source: `lg4ff_set_leds()` in both kernel and new-lg4ff.
//!
//! ## Autocenter
//!
//! To activate: `[0xfe, 0x0d, k, k, strength, 0, 0]` followed by
//! `[0x14, 0, 0, 0, 0, 0, 0]`.
//! To deactivate: `[0xf5, 0, 0, 0, 0, 0, 0]`.
//!
//! Source: `lg4ff_set_autocenter_default()` in kernel and new-lg4ff.

/// Report size for Logitech HID output commands.
pub const REPORT_SIZE: usize = 7;

/// Number of shift LEDs on G27/G29.
pub const LED_COUNT: usize = 5;

/// Minimum steering range in degrees.
pub const MIN_RANGE: u16 = 40;

/// Maximum steering range in degrees.
pub const MAX_RANGE: u16 = 900;

// ---------------------------------------------------------------------------
// Target modes for EXT_CMD9 (0x09) switch
// ---------------------------------------------------------------------------

/// Target compatibility mode for mode-switch commands.
///
/// Source: mode switch arrays in berarma/new-lg4ff hid-lg4ff.c.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TargetMode {
    /// Driving Force / Formula EX (mode byte 0x00, detach=0x00).
    DfEx = 0x00,
    /// Driving Force Pro (mode byte 0x01, detach=0x00).
    Dfp = 0x01,
    /// G25 (mode byte 0x02, detach=0x00).
    G25 = 0x02,
    /// Driving Force GT (mode byte 0x03, detach=0x00).
    Dfgt = 0x03,
    /// G27 (mode byte 0x04, detach=0x00).
    G27 = 0x04,
    /// G29 (mode byte 0x05, detach=0x01).
    G29 = 0x05,
    /// G923 native (mode byte 0x07, detach=0x01).
    G923 = 0x07,
}

impl TargetMode {
    /// Whether this mode requires the USB detach flag.
    pub fn requires_detach(self) -> bool {
        matches!(self, Self::G29 | Self::G923)
    }
}

// ---------------------------------------------------------------------------
// Mode switch encoders
// ---------------------------------------------------------------------------

/// Encode the two-step EXT_CMD9 mode switch sequence (G27+).
///
/// Returns two 7-byte reports:
/// 1. `[0xf8, 0x0a, 0, 0, 0, 0, 0]` — revert mode on USB reset
/// 2. `[0xf8, 0x09, mode, 0x01, detach, 0, 0]` — switch with detach
///
/// Source: `lg4ff_mode_switch_ext09_*` arrays in new-lg4ff.
pub fn encode_mode_switch(target: TargetMode) -> [[u8; REPORT_SIZE]; 2] {
    let detach = if target.requires_detach() { 0x01 } else { 0x00 };
    [
        [0xf8, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00],
        [0xf8, 0x09, target as u8, 0x01, detach, 0x00, 0x00],
    ]
}

/// Encode the G923 PS-mode switch command.
///
/// This is the same payload as the EXT_CMD9 G923 switch but must be
/// sent with HID report ID 0x30.
///
/// Returns the 7-byte payload (caller must set report ID to 0x30).
///
/// Source: `lg4ff_mode_switch_30_g923` in new-lg4ff.
pub fn encode_g923_ps_mode_switch() -> [u8; REPORT_SIZE] {
    [0xf8, 0x09, 0x07, 0x01, 0x01, 0x00, 0x00]
}

/// HID report ID required for G923 PS mode switch.
pub const G923_PS_REPORT_ID: u8 = 0x30;

/// Encode the DFP native-mode single-step switch (EXT_CMD1).
///
/// Source: `lg4ff_mode_switch_ext01_dfp` in new-lg4ff.
pub fn encode_dfp_native_mode() -> [u8; REPORT_SIZE] {
    [0xf8, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00]
}

/// Encode the G25 native-mode single-step switch (EXT_CMD16).
///
/// Source: `lg4ff_mode_switch_ext16_g25` in new-lg4ff.
pub fn encode_g25_native_mode() -> [u8; REPORT_SIZE] {
    [0xf8, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00]
}

// ---------------------------------------------------------------------------
// Range
// ---------------------------------------------------------------------------

/// Encode a set-range command (G25/G27/DFGT/G29/G923).
///
/// `range` is clamped to \[40, 900\] degrees.
///
/// Source: `lg4ff_set_range_g25()` in kernel/new-lg4ff.
pub fn encode_range(range: u16) -> [u8; REPORT_SIZE] {
    let r = range.clamp(MIN_RANGE, MAX_RANGE);
    [
        0xf8,
        0x81,
        (r & 0xff) as u8,
        ((r >> 8) & 0xff) as u8,
        0x00,
        0x00,
        0x00,
    ]
}

// ---------------------------------------------------------------------------
// LEDs
// ---------------------------------------------------------------------------

/// Encode a shift-LED command (G27/G29).
///
/// `leds` is a 5-bit bitmask: bit 0 = LED 1, bit 4 = LED 5.
/// Only the lower 5 bits are used.
///
/// Source: `lg4ff_set_leds()` in kernel/new-lg4ff.
pub fn encode_leds(leds: u8) -> [u8; REPORT_SIZE] {
    [0xf8, 0x12, leds & 0x1F, 0x00, 0x00, 0x00, 0x00]
}

// ---------------------------------------------------------------------------
// Autocenter
// ---------------------------------------------------------------------------

/// Encode autocenter spring commands.
///
/// `spring_k` is the spring coefficient (0–255).
/// `strength` is the spring strength (0–255).
///
/// Returns 2 reports:
/// 1. `[0xfe, 0x0d, k, k, strength, 0, 0]` — set spring params
/// 2. `[0x14, 0, 0, 0, 0, 0, 0]` — activate autocenter
///
/// Source: `lg4ff_set_autocenter_default()` in kernel/new-lg4ff.
pub fn encode_autocenter(spring_k: u8, strength: u8) -> [[u8; REPORT_SIZE]; 2] {
    [
        [0xfe, 0x0d, spring_k, spring_k, strength, 0x00, 0x00],
        [0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    ]
}

/// Encode the autocenter-off command (stop all effects).
///
/// Source: `lg4ff_set_autocenter_default()` deactivation path.
pub fn encode_autocenter_off() -> [u8; REPORT_SIZE] {
    [0xf5, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // TargetMode
    // -----------------------------------------------------------------------

    #[test]
    fn target_mode_dfex_byte() {
        assert_eq!(TargetMode::DfEx as u8, 0x00);
    }

    #[test]
    fn target_mode_g923_byte() {
        assert_eq!(TargetMode::G923 as u8, 0x07);
    }

    #[test]
    fn g29_requires_detach() {
        assert!(TargetMode::G29.requires_detach());
    }

    #[test]
    fn g923_requires_detach() {
        assert!(TargetMode::G923.requires_detach());
    }

    #[test]
    fn g27_no_detach() {
        assert!(!TargetMode::G27.requires_detach());
    }

    #[test]
    fn dfex_no_detach() {
        assert!(!TargetMode::DfEx.requires_detach());
    }

    // -----------------------------------------------------------------------
    // Mode switch — EXT_CMD9
    // -----------------------------------------------------------------------

    #[test]
    fn mode_switch_dfex() {
        let cmds = encode_mode_switch(TargetMode::DfEx);
        assert_eq!(cmds[0], [0xf8, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(cmds[1], [0xf8, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn mode_switch_dfp() {
        let cmds = encode_mode_switch(TargetMode::Dfp);
        assert_eq!(cmds[1], [0xf8, 0x09, 0x01, 0x01, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn mode_switch_g25() {
        let cmds = encode_mode_switch(TargetMode::G25);
        assert_eq!(cmds[1], [0xf8, 0x09, 0x02, 0x01, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn mode_switch_dfgt() {
        let cmds = encode_mode_switch(TargetMode::Dfgt);
        assert_eq!(cmds[1], [0xf8, 0x09, 0x03, 0x01, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn mode_switch_g27() {
        let cmds = encode_mode_switch(TargetMode::G27);
        assert_eq!(cmds[1], [0xf8, 0x09, 0x04, 0x01, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn mode_switch_g29() {
        let cmds = encode_mode_switch(TargetMode::G29);
        assert_eq!(cmds[1], [0xf8, 0x09, 0x05, 0x01, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn mode_switch_g923() {
        let cmds = encode_mode_switch(TargetMode::G923);
        assert_eq!(cmds[0], [0xf8, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(cmds[1], [0xf8, 0x09, 0x07, 0x01, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn mode_switch_revert_always_same() {
        // The first command is always the same regardless of target
        for target in [
            TargetMode::DfEx,
            TargetMode::Dfp,
            TargetMode::G25,
            TargetMode::Dfgt,
            TargetMode::G27,
            TargetMode::G29,
            TargetMode::G923,
        ] {
            let cmds = encode_mode_switch(target);
            assert_eq!(
                cmds[0],
                [0xf8, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00],
                "revert command differs for {:?}",
                target
            );
        }
    }

    // -----------------------------------------------------------------------
    // G923 PS mode switch
    // -----------------------------------------------------------------------

    #[test]
    fn g923_ps_mode_switch_payload() {
        let cmd = encode_g923_ps_mode_switch();
        assert_eq!(cmd, [0xf8, 0x09, 0x07, 0x01, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn g923_ps_report_id() {
        assert_eq!(G923_PS_REPORT_ID, 0x30);
    }

    // -----------------------------------------------------------------------
    // DFP / G25 single-step native mode
    // -----------------------------------------------------------------------

    #[test]
    fn dfp_native_mode() {
        assert_eq!(encode_dfp_native_mode(), [0xf8, 0x01, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn g25_native_mode() {
        assert_eq!(encode_g25_native_mode(), [0xf8, 0x10, 0, 0, 0, 0, 0]);
    }

    // -----------------------------------------------------------------------
    // Range
    // -----------------------------------------------------------------------

    #[test]
    fn range_900() {
        let cmd = encode_range(900);
        assert_eq!(cmd[0], 0xf8);
        assert_eq!(cmd[1], 0x81);
        let range = u16::from_le_bytes([cmd[2], cmd[3]]);
        assert_eq!(range, 900);
    }

    #[test]
    fn range_40() {
        let cmd = encode_range(40);
        let range = u16::from_le_bytes([cmd[2], cmd[3]]);
        assert_eq!(range, 40);
    }

    #[test]
    fn range_clamped_low() {
        let cmd = encode_range(10);
        let range = u16::from_le_bytes([cmd[2], cmd[3]]);
        assert_eq!(range, MIN_RANGE);
    }

    #[test]
    fn range_clamped_high() {
        let cmd = encode_range(2000);
        let range = u16::from_le_bytes([cmd[2], cmd[3]]);
        assert_eq!(range, MAX_RANGE);
    }

    #[test]
    fn range_roundtrip() {
        for deg in [40, 90, 180, 270, 360, 540, 900] {
            let cmd = encode_range(deg);
            let got = u16::from_le_bytes([cmd[2], cmd[3]]);
            assert_eq!(got, deg, "range roundtrip failed for {deg}");
        }
    }

    // -----------------------------------------------------------------------
    // LEDs
    // -----------------------------------------------------------------------

    #[test]
    fn leds_all_off() {
        let cmd = encode_leds(0);
        assert_eq!(cmd, [0xf8, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn leds_all_on() {
        let cmd = encode_leds(0x1F);
        assert_eq!(cmd[2], 0x1F);
    }

    #[test]
    fn leds_masked_to_5_bits() {
        let cmd = encode_leds(0xFF);
        assert_eq!(cmd[2], 0x1F);
    }

    #[test]
    fn leds_single() {
        let cmd = encode_leds(0x01);
        assert_eq!(cmd[2], 0x01);
    }

    // -----------------------------------------------------------------------
    // Autocenter
    // -----------------------------------------------------------------------

    #[test]
    fn autocenter_on() {
        let cmds = encode_autocenter(0x80, 0xFF);
        assert_eq!(cmds[0], [0xfe, 0x0d, 0x80, 0x80, 0xFF, 0x00, 0x00]);
        assert_eq!(cmds[1], [0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn autocenter_off() {
        assert_eq!(
            encode_autocenter_off(),
            [0xf5, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn autocenter_zero_spring() {
        let cmds = encode_autocenter(0x00, 0x00);
        assert_eq!(cmds[0], [0xfe, 0x0d, 0x00, 0x00, 0x00, 0x00, 0x00]);
    }

    // -----------------------------------------------------------------------
    // All reports are 7 bytes
    // -----------------------------------------------------------------------

    #[test]
    fn all_reports_correct_size() {
        assert_eq!(encode_mode_switch(TargetMode::G29)[0].len(), REPORT_SIZE);
        assert_eq!(encode_mode_switch(TargetMode::G29)[1].len(), REPORT_SIZE);
        assert_eq!(encode_g923_ps_mode_switch().len(), REPORT_SIZE);
        assert_eq!(encode_dfp_native_mode().len(), REPORT_SIZE);
        assert_eq!(encode_g25_native_mode().len(), REPORT_SIZE);
        assert_eq!(encode_range(540).len(), REPORT_SIZE);
        assert_eq!(encode_leds(0).len(), REPORT_SIZE);
        assert_eq!(encode_autocenter(0, 0)[0].len(), REPORT_SIZE);
        assert_eq!(encode_autocenter(0, 0)[1].len(), REPORT_SIZE);
        assert_eq!(encode_autocenter_off().len(), REPORT_SIZE);
    }

    // -----------------------------------------------------------------------
    // Cross-check: mode switch matches exact kernel byte sequences
    // -----------------------------------------------------------------------

    #[test]
    fn verify_kernel_mode_switch_ext09_dfex() {
        // From lg4ff_mode_switch_ext09_dfex in new-lg4ff
        let cmds = encode_mode_switch(TargetMode::DfEx);
        assert_eq!(cmds[0], [0xf8, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(cmds[1], [0xf8, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn verify_kernel_mode_switch_ext09_g29() {
        // From lg4ff_mode_switch_ext09_g29 in new-lg4ff
        let cmds = encode_mode_switch(TargetMode::G29);
        assert_eq!(cmds[0], [0xf8, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(cmds[1], [0xf8, 0x09, 0x05, 0x01, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn verify_kernel_mode_switch_ext09_g923() {
        // From lg4ff_mode_switch_ext09_g923 in new-lg4ff
        let cmds = encode_mode_switch(TargetMode::G923);
        assert_eq!(cmds[0], [0xf8, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(cmds[1], [0xf8, 0x09, 0x07, 0x01, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn verify_kernel_mode_switch_30_g923() {
        // From lg4ff_mode_switch_30_g923 in new-lg4ff
        // Report ID must be 0x30, payload matches ext09 g923
        let cmd = encode_g923_ps_mode_switch();
        assert_eq!(cmd, [0xf8, 0x09, 0x07, 0x01, 0x01, 0x00, 0x00]);
    }
}
