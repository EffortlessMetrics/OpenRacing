//! Logitech HID output report encoding.
//!
//! All functions are pure and allocation-free.
//!
//! # Protocol notes
//!
//! The kernel `hid-lg4ff.c` driver and `berarma/new-lg4ff` out-of-tree driver
//! both use a **4-slot** system for force feedback commands. Each slot command
//! is a 7-byte payload sent via `HID_REQ_SET_REPORT`:
//!
//! ```text
//! Byte 0: (slot_id << 4) | operation
//!   Slot IDs: 0 = constant, 1–3 = conditional effects
//!   Operations: 0x01 = start, 0x03 = stop, 0x0c = update
//! Bytes 1–6: effect-specific data
//! ```
//!
//! ## Effect type bytes (new-lg4ff `lg4ff_update_slot`)
//!
//! | Effect   | Byte 1 | Encoding summary |
//! |----------|--------|------------------|
//! | Constant | `0x00` | Force in byte `2 + slot_id` (unsigned 8-bit, 0x80 = center) |
//! | Spring   | `0x0b` | 11-bit deadband positions, 4-bit coefficients, sign bits, 8-bit clip |
//! | Damper   | `0x0c` | 4-bit coefficients, sign bytes, 8-bit clip |
//! | Friction | `0x0e` | 8-bit coefficients, 8-bit clip, sign nibble |
//!
//! The kernel's in-tree driver (`lg4ff_play`) uses a simpler encoding
//! (`{0x11, 0x08, force, 0x80, 0, 0, 0}` for constant force in slot 1,
//! where `force` is unsigned 0x00–0xFF with 0x80 = no force).
//!
//! ## G923 TrueForce
//!
//! TrueForce is a proprietary Logitech haptic feedback feature on the G923.
//! No public protocol documentation exists in any open-source driver project
//! as of this writing. The `new-lg4ff` driver supports G923 standard FFB
//! but does not implement TrueForce.
//!
//! ## Encoder CPR (counts per revolution)
//!
//! Encoder resolution values are hardware specifications from Logitech
//! product data, not present in any driver source code. They are not
//! verified by the open-source drivers.

#![deny(static_mut_refs)]

use crate::ids::{commands, report_ids};

/// Wire size of a Logitech constant-force output report.
pub const CONSTANT_FORCE_REPORT_LEN: usize = 4;

/// Wire size of a Logitech vendor feature/output report (0xF8 commands).
pub const VENDOR_REPORT_LEN: usize = 7;

/// Encoder for Logitech constant-force FFB output reports (report ID 0x12).
///
/// Converts a torque value in Newton-meters to the signed 16-bit Logitech wire
/// format (range ±10000, where 10000 = max torque).
#[derive(Debug, Clone, Copy)]
pub struct LogitechConstantForceEncoder {
    max_torque_nm: f32,
}

impl LogitechConstantForceEncoder {
    /// Create a new encoder for a wheel with the given peak torque.
    pub fn new(max_torque_nm: f32) -> Self {
        Self {
            max_torque_nm: max_torque_nm.max(0.01),
        }
    }

    /// Encode a torque command (Newton-meters) into a constant-force output report.
    ///
    /// Layout (4 bytes):
    /// - Byte 0: `0x12` (report ID)
    /// - Byte 1: effect block index (`1` = slot 1, 1-based)
    /// - Bytes 2–3: signed magnitude, little-endian (range ±10000)
    pub fn encode(&self, torque_nm: f32, out: &mut [u8; CONSTANT_FORCE_REPORT_LEN]) -> usize {
        out.fill(0);
        out[0] = report_ids::CONSTANT_FORCE;
        out[1] = 1; // effect block index (1-based)
        let mag = torque_to_magnitude(torque_nm, self.max_torque_nm);
        let bytes = mag.to_le_bytes();
        out[2] = bytes[0];
        out[3] = bytes[1];
        CONSTANT_FORCE_REPORT_LEN
    }

    /// Encode an explicit zero-force report.
    pub fn encode_zero(&self, out: &mut [u8; CONSTANT_FORCE_REPORT_LEN]) -> usize {
        out.fill(0);
        out[0] = report_ids::CONSTANT_FORCE;
        out[1] = 1;
        CONSTANT_FORCE_REPORT_LEN
    }
}

/// Convert torque (Nm) to Logitech magnitude units (±10000).
fn torque_to_magnitude(torque_nm: f32, max_torque_nm: f32) -> i16 {
    let normalized = (torque_nm / max_torque_nm).clamp(-1.0, 1.0);
    (normalized * 10_000.0) as i16
}

/// Build the 7-byte "revert mode upon USB reset" feature report (0xF8, cmd 0x0A).
///
/// In the Linux kernel and new-lg4ff drivers, this command is documented as
/// "Revert mode upon USB reset". It is the **first step** of a two-command
/// native-mode switch sequence for G27+ wheels:
///
/// 1. `{0xF8, 0x0A, 0, 0, 0, 0, 0}` — revert mode upon USB reset (this fn)
/// 2. `{0xF8, 0x09, mode, 0x01, detach, 0, 0}` — switch to target mode
///
/// For simpler wheels (DFP, G25), a single command suffices
/// (DFP: `{0xF8, 0x01, ...}`, G25: `{0xF8, 0x10, ...}`).
///
/// For G923 PS (PID 0xC267 → 0xC266), the mode-switch command must be
/// sent with HID report ID `0x30` instead of the default output report ID.
///
/// Source: `lg4ff_mode_switch_ext09_*` in kernel `hid-lg4ff.c` and
/// `berarma/new-lg4ff hid-lg4ff.c`.
///
/// After sending, wait at least 100 ms before issuing further commands.
pub fn build_native_mode_report() -> [u8; VENDOR_REPORT_LEN] {
    [
        report_ids::VENDOR,
        commands::NATIVE_MODE,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
    ]
}

/// Build the 7-byte set-range feature report (0xF8, cmd 0x81).
///
/// `degrees` is the desired full rotation range (e.g. 900 for G920/G923,
/// 1080 for Pro Racing Wheel). Valid range per driver: 40–900 for
/// G25/G27/DFGT/G29/G923 (see `lg4ff_devices[]` in kernel and new-lg4ff).
///
/// Source: `lg4ff_set_range_g25()` in kernel `hid-lg4ff.c` and
/// `berarma/new-lg4ff` — `{0xf8, 0x81, range & 0xff, range >> 8, 0, 0, 0}`.
pub fn build_set_range_report(degrees: u16) -> [u8; VENDOR_REPORT_LEN] {
    let [lsb, msb] = degrees.to_le_bytes();
    [
        report_ids::VENDOR,
        commands::SET_RANGE,
        lsb,
        msb,
        0x00,
        0x00,
        0x00,
    ]
}

/// Build the 7-byte set-autocenter feature report (0xF8, cmd 0x14).
///
/// `strength` is the centering force (0x00–0xFF).
/// `rate` is the centering speed (0x00–0xFF).
///
/// This command activates the device's built-in autocenter spring. The full
/// autocenter protocol (from `lg4ff_set_autocenter_default` in both the
/// kernel and new-lg4ff) is a two-step sequence:
///
/// 1. `{0xFE, 0x0D, k, k, strength, 0, 0}` — configure spring parameters
/// 2. `{0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00}` — activate
///
/// To deactivate autocenter: `{0xF5, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00}`.
///
/// This function builds a simplified single-command activation.
pub fn build_set_autocenter_report(strength: u8, rate: u8) -> [u8; VENDOR_REPORT_LEN] {
    [
        report_ids::VENDOR,
        commands::SET_AUTOCENTER,
        strength,
        rate,
        0x00,
        0x00,
        0x00,
    ]
}

/// Build the 7-byte rev-light LED output report (0xF8, cmd 0x12).
///
/// `led_mask` is a 5-bit bitmask: bit 0 = LED 1 (leftmost), bit 4 = LED 5 (rightmost).
pub fn build_set_leds_report(led_mask: u8) -> [u8; VENDOR_REPORT_LEN] {
    [
        report_ids::VENDOR,
        commands::SET_LEDS,
        led_mask & 0x1F,
        0x00,
        0x00,
        0x00,
        0x00,
    ]
}

/// Build the 2-byte device gain output report (report ID 0x16).
///
/// `gain` is the overall FFB gain (0x00–0xFF, 0 = 0%, 0xFF = 100%).
pub fn build_gain_report(gain: u8) -> [u8; 2] {
    [report_ids::DEVICE_GAIN, gain]
}

/// Build the DFP-specific set-range commands (two reports in sequence).
///
/// The DFP uses a different range encoding than G25+. The kernel sends
/// two HID reports in sequence:
///   1. **Coarse limit**: `[0xf8, 0x03, 0,0,0,0,0]` for >200° or `[0xf8, 0x02, ...]` for ≤200°.
///   2. **Fine limit**:  `[0x81, 0x0b, start_left>>4, start_right>>4, 0xff, (right&0xe)<<4|(left&0xe), 0xff]`.
///      If range is exactly 200 or 900, the fine limit is a no-op (`[0x81, 0x0b, 0,0,0,0,0]`).
///
/// `start_left  = (full_range - range + 1) * 2047 / full_range`
/// `start_right = 0xFFF - start_left`
///
/// Source: `lg4ff_set_range_dfp()` in kernel `hid-lg4ff.c`.
pub fn build_set_range_dfp_reports(degrees: u16) -> [[u8; VENDOR_REPORT_LEN]; 2] {
    let range = degrees.clamp(40, 900) as u32;

    // Coarse limit command
    let coarse_cmd = if range > 200 { 0x03u8 } else { 0x02u8 };
    let coarse = [report_ids::VENDOR, coarse_cmd, 0x00, 0x00, 0x00, 0x00, 0x00];

    // Fine limit command
    let full_range: u32 = if range > 200 { 900 } else { 200 };

    if range == 200 || range == 900 {
        // No fine limit needed — send zeroed fine command
        let fine = [0x81u8, 0x0b, 0x00, 0x00, 0x00, 0x00, 0x00];
        return [coarse, fine];
    }

    let start_left = ((full_range - range + 1) * 2047) / full_range;
    let start_right = 0xFFF - start_left;

    let fine = [
        0x81u8,
        0x0b,
        (start_left >> 4) as u8,
        (start_right >> 4) as u8,
        0xff,
        (((start_right & 0xe) << 4) | (start_left & 0xe)) as u8,
        0xff,
    ];

    [coarse, fine]
}

/// Build a single DFP set-range report (legacy API, returns only the fine limit).
///
/// **Deprecated**: prefer [`build_set_range_dfp_reports`] which returns both
/// coarse and fine limit commands as the kernel expects.
pub fn build_set_range_dfp_report(degrees: u16) -> [u8; VENDOR_REPORT_LEN] {
    let reports = build_set_range_dfp_reports(degrees);
    reports[1] // Return the fine limit command
}

/// Build the mode-switch command to transition to native mode (G27+).
///
/// `mode_id` selects the target mode (from kernel `lg4ff_mode_switch_ext09_*`):
///   - `0x00`: DF-EX (Driving Force / Formula EX compatibility)
///   - `0x01`: DFP (Driving Force Pro compatibility)
///   - `0x02`: G25
///   - `0x03`: DFGT (Driving Force GT)
///   - `0x04`: G27
///   - `0x05`: G29
///   - `0x07`: G923 PS (from `berarma/new-lg4ff` — **note**: the G923 PS
///     uses HID report ID `0x30` instead of the default vendor report ID;
///     see `lg4ff_mode_switch_30_g923` in new-lg4ff)
///
/// `detach`: if `true`, byte 4 = `0x01` (detach from current HID device);
///           if `false`, byte 4 = `0x00`.
///
/// Source: `lg4ff_mode_switch_ext09_*` arrays in kernel `hid-lg4ff.c`.
///
/// Cross-verified 2025-07 against new-lg4ff `lg4ff_mode_switch_ext09_g923`:
/// `{1, {0xf8, 0x09, 0x07, 0x01, 0x01, 0x00, 0x00}}` — confirming
/// mode_id=0x07 and detach=0x01 for the G923 PS native mode transition.
pub fn build_mode_switch_report(mode_id: u8, detach: bool) -> [u8; VENDOR_REPORT_LEN] {
    [
        report_ids::VENDOR,
        commands::MODE_SWITCH,
        mode_id,
        0x01,
        if detach { 0x01 } else { 0x00 },
        0x00,
        0x00,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn test_native_mode_report() -> Result<(), Box<dyn std::error::Error>> {
        let r = build_native_mode_report();
        assert_eq!(r[0], 0xF8, "report ID must be 0xF8");
        assert_eq!(r[1], 0x0A, "command must be NATIVE_MODE (0x0A)");
        assert_eq!(&r[2..], &[0u8; 5], "remaining bytes must be zero");
        Ok(())
    }

    #[test]
    fn test_set_range_900_degrees() -> Result<(), Box<dyn std::error::Error>> {
        let r = build_set_range_report(900);
        assert_eq!(r[0], 0xF8);
        assert_eq!(r[1], 0x81, "command must be SET_RANGE (0x81)");
        // 900 dec = 0x0384; little-endian = [0x84, 0x03]
        assert_eq!(r[2], 0x84, "LSB of 900 = 0x84");
        assert_eq!(r[3], 0x03, "MSB of 900 = 0x03");
        assert_eq!(&r[4..], &[0u8; 3]);
        Ok(())
    }

    #[test]
    fn test_set_range_200_degrees() -> Result<(), Box<dyn std::error::Error>> {
        let r = build_set_range_report(200);
        // 200 dec = 0x00C8; little-endian = [0xC8, 0x00]
        assert_eq!(r[2], 0xC8);
        assert_eq!(r[3], 0x00);
        Ok(())
    }

    #[test]
    fn test_set_range_1080_degrees() -> Result<(), Box<dyn std::error::Error>> {
        let r = build_set_range_report(1080);
        // 1080 dec = 0x0438; little-endian = [0x38, 0x04]
        assert_eq!(r[2], 0x38, "LSB of 1080 = 0x38");
        assert_eq!(r[3], 0x04, "MSB of 1080 = 0x04");
        Ok(())
    }

    #[test]
    fn test_set_autocenter_report() -> Result<(), Box<dyn std::error::Error>> {
        let r = build_set_autocenter_report(0x40, 0x80);
        assert_eq!(r[0], 0xF8);
        assert_eq!(r[1], 0x14, "command must be SET_AUTOCENTER (0x14)");
        assert_eq!(r[2], 0x40, "strength byte");
        assert_eq!(r[3], 0x80, "rate byte");
        assert_eq!(&r[4..], &[0u8; 3]);
        Ok(())
    }

    #[test]
    fn test_set_autocenter_zero() -> Result<(), Box<dyn std::error::Error>> {
        let r = build_set_autocenter_report(0x00, 0x00);
        assert_eq!(r[0], 0xF8);
        assert_eq!(r[1], 0x14);
        assert_eq!(r[2], 0x00, "zero strength");
        assert_eq!(r[3], 0x00, "zero rate");
        assert_eq!(&r[4..], &[0u8; 3]);
        Ok(())
    }

    #[test]
    fn test_set_leds_report_all_on() -> Result<(), Box<dyn std::error::Error>> {
        let r = build_set_leds_report(0b00011111);
        assert_eq!(r[0], 0xF8);
        assert_eq!(r[1], 0x12, "command must be SET_LEDS (0x12)");
        assert_eq!(r[2], 0x1F, "all 5 LEDs on");
        assert_eq!(&r[3..], &[0u8; 4]);
        Ok(())
    }

    #[test]
    fn test_set_leds_masks_high_bits() -> Result<(), Box<dyn std::error::Error>> {
        let r = build_set_leds_report(0xFF);
        assert_eq!(r[2], 0x1F, "upper bits must be masked to 5-bit range");
        Ok(())
    }

    #[test]
    fn test_gain_report() -> Result<(), Box<dyn std::error::Error>> {
        let r_full = build_gain_report(0xFF);
        assert_eq!(r_full[0], 0x16, "Device Gain report ID");
        assert_eq!(r_full[1], 0xFF, "full gain");
        let r_zero = build_gain_report(0);
        assert_eq!(r_zero[0], 0x16);
        assert_eq!(r_zero[1], 0, "zero gain");
        Ok(())
    }

    #[test]
    fn test_constant_force_encoder_positive() -> Result<(), Box<dyn std::error::Error>> {
        let enc = LogitechConstantForceEncoder::new(2.2);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(1.1, &mut out);
        assert_eq!(out[0], 0x12, "report ID");
        assert_eq!(out[1], 1, "effect block index");
        // 1.1 / 2.2 = 0.5 normalized → 5000 magnitude
        let mag = i16::from_le_bytes([out[2], out[3]]);
        assert_eq!(mag, 5000);
        Ok(())
    }

    #[test]
    fn test_constant_force_encoder_full_negative() -> Result<(), Box<dyn std::error::Error>> {
        let enc = LogitechConstantForceEncoder::new(2.2);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(-2.2, &mut out);
        let mag = i16::from_le_bytes([out[2], out[3]]);
        assert_eq!(mag, -10000);
        Ok(())
    }

    #[test]
    fn test_constant_force_encoder_zero() -> Result<(), Box<dyn std::error::Error>> {
        let enc = LogitechConstantForceEncoder::new(2.2);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode_zero(&mut out);
        let mag = i16::from_le_bytes([out[2], out[3]]);
        assert_eq!(mag, 0);
        Ok(())
    }

    #[test]
    fn test_constant_force_saturation() -> Result<(), Box<dyn std::error::Error>> {
        let enc = LogitechConstantForceEncoder::new(2.2);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(100.0, &mut out);
        let mag = i16::from_le_bytes([out[2], out[3]]);
        assert_eq!(mag, 10000, "over-torque must saturate at +10000");
        enc.encode(-100.0, &mut out);
        let mag = i16::from_le_bytes([out[2], out[3]]);
        assert_eq!(mag, -10000, "over-torque must saturate at -10000");
        Ok(())
    }

    /// Verify that vendor (0xF8) report bytes 2–6 are zero for all commands.
    #[test]
    fn test_vendor_report_padding_zero() -> Result<(), Box<dyn std::error::Error>> {
        let reports: [[u8; VENDOR_REPORT_LEN]; 3] = [
            build_native_mode_report(),
            build_set_range_report(0),
            build_set_autocenter_report(0, 0),
        ];
        for r in &reports {
            assert_eq!(r[0], 0xF8, "report ID must always be 0xF8");
        }
        // Native mode: bytes 2–6 all zero
        assert_eq!(&build_native_mode_report()[2..], &[0u8; 5]);
        Ok(())
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        #[test]
        fn prop_encode_no_overflow(torque_nm in proptest::num::f32::ANY) {
            let enc = LogitechConstantForceEncoder::new(2.2);
            let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
            enc.encode(torque_nm, &mut out);
            let mag = i16::from_le_bytes([out[2], out[3]]);
            prop_assert!(
                (-10_000..=10_000).contains(&mag),
                "magnitude {} out of range for torque_nm={}",
                mag,
                torque_nm
            );
        }

        #[test]
        fn prop_report_id_always_correct(torque_nm in proptest::num::f32::ANY) {
            let enc = LogitechConstantForceEncoder::new(2.2);
            let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
            enc.encode(torque_nm, &mut out);
            prop_assert_eq!(out[0], 0x12, "report ID must always be 0x12");
            prop_assert_eq!(out[1], 1u8, "effect block index must always be 1");
        }

        /// Verify that encoding within [-max, +max] is monotone (larger input → larger or equal output).
        #[test]
        fn prop_encode_monotone(
            a in -2.2f32..=2.2f32,
            b in -2.2f32..=2.2f32,
        ) {
            let enc = LogitechConstantForceEncoder::new(2.2);
            let mut out_a = [0u8; CONSTANT_FORCE_REPORT_LEN];
            let mut out_b = [0u8; CONSTANT_FORCE_REPORT_LEN];
            enc.encode(a, &mut out_a);
            enc.encode(b, &mut out_b);
            let mag_a = i16::from_le_bytes([out_a[2], out_a[3]]);
            let mag_b = i16::from_le_bytes([out_b[2], out_b[3]]);
            if a <= b {
                prop_assert!(mag_a <= mag_b, "monotone violated: encode({}) = {} > encode({}) = {}", a, mag_a, b, mag_b);
            } else {
                prop_assert!(mag_a >= mag_b, "monotone violated: encode({}) = {} < encode({}) = {}", a, mag_a, b, mag_b);
            }
        }

        /// Boundary: normalized input ±1.0 × max_torque must produce ±10000.
        #[test]
        fn prop_boundary_inputs_produce_full_scale(
            max_torque in 0.01f32..=20.0f32,
        ) {
            let enc = LogitechConstantForceEncoder::new(max_torque);
            let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];

            enc.encode(max_torque, &mut out);
            let mag_pos = i16::from_le_bytes([out[2], out[3]]);
            prop_assert_eq!(mag_pos, 10_000i16, "positive full scale must be 10000");

            enc.encode(-max_torque, &mut out);
            let mag_neg = i16::from_le_bytes([out[2], out[3]]);
            prop_assert_eq!(mag_neg, -10_000i16, "negative full scale must be -10000");
        }

        /// Zero torque must always encode to zero magnitude.
        #[test]
        fn prop_zero_input_produces_zero(
            max_torque in 0.01f32..=20.0f32,
        ) {
            let enc = LogitechConstantForceEncoder::new(max_torque);
            let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
            enc.encode(0.0, &mut out);
            let mag = i16::from_le_bytes([out[2], out[3]]);
            prop_assert_eq!(mag, 0i16, "zero torque must encode to zero");
        }
    }

    /// Verify the DFP two-report range sequence matches kernel lg4ff_set_range_dfp().
    ///
    /// The kernel sends:
    ///   1. Coarse: [0xf8, 0x03, 0,0,0,0,0] for >200° or [0xf8, 0x02, ...] for ≤200°
    ///   2. Fine:   [0x81, 0x0b, left>>4, right>>4, 0xff, nibbles, 0xff]
    ///      Exact 200 and 900 → fine is all-zero (no fine limit applied)
    #[test]
    fn test_dfp_range_reports_known_values() -> Result<(), Box<dyn std::error::Error>> {
        // 200° → coarse 0x02 (short), fine is no-op
        let [coarse, fine] = build_set_range_dfp_reports(200);
        assert_eq!(coarse[0], report_ids::VENDOR);
        assert_eq!(coarse[1], 0x02, "200° coarse must be 0x02");
        assert_eq!(fine[0], 0x81);
        assert_eq!(fine[1], 0x0b);
        assert_eq!(
            &fine[2..7],
            &[0, 0, 0, 0, 0],
            "200° fine must be zeroed (no-op)"
        );

        // 900° → coarse 0x03 (long), fine is no-op
        let [coarse, fine] = build_set_range_dfp_reports(900);
        assert_eq!(coarse[1], 0x03, "900° coarse must be 0x03");
        assert_eq!(
            &fine[2..7],
            &[0, 0, 0, 0, 0],
            "900° fine must be zeroed (no-op)"
        );

        // 540° → coarse 0x03 (>200), fine has non-trivial values
        // full_range=900, start_left = (900-540+1)*2047/900 = 361*2047/900 = 820
        // start_right = 0xFFF - 820 = 3275
        let [coarse, fine] = build_set_range_dfp_reports(540);
        assert_eq!(coarse[1], 0x03);
        let start_left = (900u32 - 540 + 1) * 2047 / 900; // 820 = 0x334
        let start_right = 0xFFF - start_left; // 3275 = 0xCCB
        assert_eq!(fine[2], (start_left >> 4) as u8, "540° fine left>>4");
        assert_eq!(fine[3], (start_right >> 4) as u8, "540° fine right>>4");
        assert_eq!(fine[4], 0xff);
        assert_eq!(
            fine[5],
            (((start_right & 0xe) << 4) | (start_left & 0xe)) as u8,
            "540° fine nibble byte"
        );
        assert_eq!(fine[6], 0xff);

        Ok(())
    }

    /// Legacy single-report API returns the fine limit command
    #[test]
    fn test_dfp_range_report_known_values() -> Result<(), Box<dyn std::error::Error>> {
        let r200 = build_set_range_dfp_report(200);
        assert_eq!(r200[0], 0x81, "report byte 0 must be 0x81");
        assert_eq!(r200[1], 0x0b, "report byte 1 must be 0x0b");

        // Verify clamping: 0° should clamp to 40°
        let r40 = build_set_range_dfp_report(40);
        let r0 = build_set_range_dfp_report(0);
        assert_eq!(r40, r0, "0° should clamp to 40°");

        // Verify clamping: above 900° should clamp to 900
        let r900 = build_set_range_dfp_report(900);
        let r_over = build_set_range_dfp_report(1500);
        assert_eq!(r900, r_over, "1500° should clamp to 900°");

        Ok(())
    }

    /// Verify the DFP range report produces different fine-limit encoded
    /// values as degrees increase (monotonicity of start_left).
    #[test]
    fn test_dfp_range_report_monotone() -> Result<(), Box<dyn std::error::Error>> {
        // start_left = (full_range - range + 1) * 2047 / full_range
        // As range increases, start_left DECREASES (fine limit narrows)
        // For ranges > 200, full_range = 900
        let mut prev_left = u32::MAX;
        for deg in (250..=900).step_by(50) {
            let [_, fine] = build_set_range_dfp_reports(deg as u16);
            // Extract start_left from fine[2] (upper nibble) and fine[5] (lower nibble)
            let left_upper = (fine[2] as u32) << 4;
            let left_lower = (fine[5] as u32) & 0x0F; // actually & 0xe per kernel
            let approx_left = left_upper | left_lower;
            if deg < 900 {
                assert!(
                    approx_left < prev_left,
                    "DFP fine start_left must decrease as range {}° increases: prev={}, cur={}",
                    deg,
                    prev_left,
                    approx_left
                );
            }
            prev_left = approx_left;
        }
        Ok(())
    }

    /// Verify the DFP arithmetic matches the kernel formula exactly.
    #[test]
    fn test_dfp_range_report_arithmetic_detail() -> Result<(), Box<dyn std::error::Error>> {
        // For 100° (≤200): full_range=200, start_left=(200-100+1)*2047/200 = 101*2047/200 = 1033
        // start_right = 0xFFF - 1033 = 3062
        let [coarse, fine] = build_set_range_dfp_reports(100);
        assert_eq!(coarse[1], 0x02, "100° coarse cmd must be 0x02");
        let start_left = (200u32 - 100 + 1) * 2047 / 200;
        let start_right = 0xFFF - start_left;
        assert_eq!(fine[2], (start_left >> 4) as u8);
        assert_eq!(fine[3], (start_right >> 4) as u8);

        Ok(())
    }

    /// Verify G923 mode-switch command matches `lg4ff_mode_switch_ext09_g923`
    /// from `berarma/new-lg4ff`: `{1, {0xf8, 0x09, 0x07, 0x01, 0x01, 0x00, 0x00}}`.
    #[test]
    fn test_g923_mode_switch_matches_new_lg4ff() -> Result<(), Box<dyn std::error::Error>> {
        let report = build_mode_switch_report(0x07, true);
        assert_eq!(
            report,
            [0xF8, 0x09, 0x07, 0x01, 0x01, 0x00, 0x00],
            "G923 mode-switch must match new-lg4ff lg4ff_mode_switch_ext09_g923"
        );
        Ok(())
    }

    /// Verify G29 mode-switch command matches `lg4ff_mode_switch_ext09_g29`
    /// from kernel `hid-lg4ff.c`: `{1, {0xf8, 0x09, 0x05, 0x01, 0x01, 0x00, 0x00}}`.
    #[test]
    fn test_g29_mode_switch_matches_kernel() -> Result<(), Box<dyn std::error::Error>> {
        let report = build_mode_switch_report(0x05, true);
        assert_eq!(
            report,
            [0xF8, 0x09, 0x05, 0x01, 0x01, 0x00, 0x00],
            "G29 mode-switch must match kernel lg4ff_mode_switch_ext09_g29"
        );
        Ok(())
    }
}
