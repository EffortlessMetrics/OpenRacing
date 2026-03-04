//! Hardening tests for the OpenFFBoard HID protocol crate.
//!
//! Covers:
//! 1. Command encoding / decoding round-trip (deterministic)
//! 2. Parameter read/write frame construction (byte-level layout)
//! 3. Response parsing (decode raw bytes back to logical values)
//! 4. Error handling for malformed / edge-case inputs
//! 5. Property tests for parameter encoding
//! 6. Known command constant validation

use proptest::prelude::*;
use racing_wheel_hid_openffboard_protocol::output::{ENABLE_FFB_REPORT_ID, MAX_TORQUE_SCALE};
use racing_wheel_hid_openffboard_protocol::{
    CONSTANT_FORCE_REPORT_ID, CONSTANT_FORCE_REPORT_LEN, GAIN_REPORT_ID, OPENFFBOARD_PRODUCT_ID,
    OPENFFBOARD_PRODUCT_ID_ALT, OPENFFBOARD_VENDOR_ID, OpenFFBoardTorqueEncoder,
    OpenFFBoardVariant, build_enable_ffb, build_set_gain, is_openffboard_product,
};

// ---------------------------------------------------------------------------
// Helper: decode raw torque from a 5-byte constant-force report
// ---------------------------------------------------------------------------
fn decode_torque_raw(report: &[u8; CONSTANT_FORCE_REPORT_LEN]) -> i16 {
    i16::from_le_bytes([report[1], report[2]])
}

fn decode_torque_normalized(report: &[u8; CONSTANT_FORCE_REPORT_LEN]) -> f32 {
    decode_torque_raw(report) as f32 / MAX_TORQUE_SCALE as f32
}

// ===========================================================================
// 1. Command encoding / decoding round-trip (deterministic)
// ===========================================================================

#[test]
fn round_trip_zero_torque() -> Result<(), String> {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(0.0);
    let decoded = decode_torque_normalized(&report);
    if (decoded - 0.0).abs() > f32::EPSILON {
        return Err(format!("expected 0.0, got {decoded}"));
    }
    Ok(())
}

#[test]
fn round_trip_full_positive() -> Result<(), String> {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(1.0);
    let decoded = decode_torque_normalized(&report);
    if (decoded - 1.0).abs() > f32::EPSILON {
        return Err(format!("expected 1.0, got {decoded}"));
    }
    Ok(())
}

#[test]
fn round_trip_full_negative() -> Result<(), String> {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(-1.0);
    let decoded = decode_torque_normalized(&report);
    if (decoded - (-1.0)).abs() > f32::EPSILON {
        return Err(format!("expected -1.0, got {decoded}"));
    }
    Ok(())
}

#[test]
fn round_trip_half_positive() -> Result<(), String> {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(0.5);
    let raw = decode_torque_raw(&report);
    // 0.5 * 10000 = 5000 exactly
    if raw != 5000 {
        return Err(format!("expected raw 5000, got {raw}"));
    }
    Ok(())
}

#[test]
fn round_trip_half_negative() -> Result<(), String> {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(-0.5);
    let raw = decode_torque_raw(&report);
    if raw != -5000 {
        return Err(format!("expected raw -5000, got {raw}"));
    }
    Ok(())
}

#[test]
fn round_trip_quarter() -> Result<(), String> {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(0.25);
    let raw = decode_torque_raw(&report);
    // 0.25 * 10000 = 2500 exactly
    if raw != 2500 {
        return Err(format!("expected raw 2500, got {raw}"));
    }
    Ok(())
}

#[test]
fn round_trip_error_within_quantization() -> Result<(), String> {
    let enc = OpenFFBoardTorqueEncoder;
    // 0.33333 * 10000 = 3333.3 → truncated to 3333
    let report = enc.encode(1.0 / 3.0);
    let raw = decode_torque_raw(&report);
    if raw != 3333 {
        return Err(format!("expected raw 3333, got {raw}"));
    }
    let decoded = decode_torque_normalized(&report);
    let error = (1.0 / 3.0 - decoded).abs();
    let max_quant_error = 1.0 / MAX_TORQUE_SCALE as f32;
    if error >= max_quant_error {
        return Err(format!(
            "round-trip error {error} exceeds quantization step {max_quant_error}"
        ));
    }
    Ok(())
}

// ===========================================================================
// 2. Parameter read/write frame construction (byte-level layout)
// ===========================================================================

#[test]
fn torque_frame_layout_positive() -> Result<(), String> {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(1.0);
    // 10000i16 = 0x2710 → LE bytes: [0x10, 0x27]
    let expected: [u8; 5] = [0x01, 0x10, 0x27, 0x00, 0x00];
    if report != expected {
        return Err(format!("expected {expected:02X?}, got {report:02X?}"));
    }
    Ok(())
}

#[test]
fn torque_frame_layout_negative() -> Result<(), String> {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(-1.0);
    // -10000i16 in LE = [0xF0, 0xD8]
    let expected: [u8; 5] = [0x01, 0xF0, 0xD8, 0x00, 0x00];
    if report != expected {
        return Err(format!("expected {expected:02X?}, got {report:02X?}"));
    }
    Ok(())
}

#[test]
fn torque_frame_layout_zero() -> Result<(), String> {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(0.0);
    let expected: [u8; 5] = [0x01, 0x00, 0x00, 0x00, 0x00];
    if report != expected {
        return Err(format!("expected {expected:02X?}, got {report:02X?}"));
    }
    Ok(())
}

#[test]
fn enable_ffb_frame_on_layout() -> Result<(), String> {
    let report = build_enable_ffb(true);
    let expected: [u8; 3] = [ENABLE_FFB_REPORT_ID, 0x01, 0x00];
    if report != expected {
        return Err(format!("expected {expected:02X?}, got {report:02X?}"));
    }
    Ok(())
}

#[test]
fn enable_ffb_frame_off_layout() -> Result<(), String> {
    let report = build_enable_ffb(false);
    let expected: [u8; 3] = [ENABLE_FFB_REPORT_ID, 0x00, 0x00];
    if report != expected {
        return Err(format!("expected {expected:02X?}, got {report:02X?}"));
    }
    Ok(())
}

#[test]
fn gain_frame_layout_full() -> Result<(), String> {
    let report = build_set_gain(255);
    let expected: [u8; 3] = [GAIN_REPORT_ID, 0xFF, 0x00];
    if report != expected {
        return Err(format!("expected {expected:02X?}, got {report:02X?}"));
    }
    Ok(())
}

#[test]
fn gain_frame_layout_zero() -> Result<(), String> {
    let report = build_set_gain(0);
    let expected: [u8; 3] = [GAIN_REPORT_ID, 0x00, 0x00];
    if report != expected {
        return Err(format!("expected {expected:02X?}, got {report:02X?}"));
    }
    Ok(())
}

#[test]
fn gain_frame_layout_mid() -> Result<(), String> {
    let report = build_set_gain(128);
    let expected: [u8; 3] = [GAIN_REPORT_ID, 0x80, 0x00];
    if report != expected {
        return Err(format!("expected {expected:02X?}, got {report:02X?}"));
    }
    Ok(())
}

// ===========================================================================
// 3. Response parsing (decode encoded frames)
// ===========================================================================

#[test]
fn parse_constant_force_report_id() -> Result<(), String> {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(0.42);
    if report[0] != CONSTANT_FORCE_REPORT_ID {
        return Err(format!(
            "report ID byte: expected {CONSTANT_FORCE_REPORT_ID:#04X}, got {:#04X}",
            report[0]
        ));
    }
    Ok(())
}

#[test]
fn parse_torque_sign_preserved_through_encoding() -> Result<(), String> {
    let enc = OpenFFBoardTorqueEncoder;
    for &input in &[-0.9, -0.5, -0.1, 0.1, 0.5, 0.9] {
        let report = enc.encode(input);
        let raw = decode_torque_raw(&report);
        if input > 0.0 && raw <= 0 {
            return Err(format!(
                "positive input {input} gave non-positive raw {raw}"
            ));
        }
        if input < 0.0 && raw >= 0 {
            return Err(format!(
                "negative input {input} gave non-negative raw {raw}"
            ));
        }
    }
    Ok(())
}

#[test]
fn parse_enable_report_flag_byte() -> Result<(), String> {
    let on = build_enable_ffb(true);
    let off = build_enable_ffb(false);
    // Consumer would read byte 1 as the enable flag
    if on[1] != 1 {
        return Err(format!("enable=true: expected flag byte 1, got {}", on[1]));
    }
    if off[1] != 0 {
        return Err(format!(
            "enable=false: expected flag byte 0, got {}",
            off[1]
        ));
    }
    Ok(())
}

#[test]
fn parse_gain_value_byte() -> Result<(), String> {
    for gain in [0u8, 1, 64, 128, 200, 255] {
        let report = build_set_gain(gain);
        if report[1] != gain {
            return Err(format!(
                "gain={gain}: expected byte 1 = {gain}, got {}",
                report[1]
            ));
        }
    }
    Ok(())
}

#[test]
fn parse_reserved_bytes_always_zero() -> Result<(), String> {
    let enc = OpenFFBoardTorqueEncoder;
    for &t in &[-1.0, -0.5, 0.0, 0.5, 1.0] {
        let report = enc.encode(t);
        if report[3] != 0 || report[4] != 0 {
            return Err(format!(
                "torque={t}: reserved bytes [{:#04X}, {:#04X}] must be [0x00, 0x00]",
                report[3], report[4]
            ));
        }
    }
    Ok(())
}

#[test]
fn parse_enable_ffb_reserved_byte_zero() -> Result<(), String> {
    for enabled in [true, false] {
        let report = build_enable_ffb(enabled);
        if report[2] != 0 {
            return Err(format!(
                "enable={enabled}: reserved byte 2 = {:#04X}, expected 0x00",
                report[2]
            ));
        }
    }
    Ok(())
}

#[test]
fn parse_gain_reserved_byte_zero() -> Result<(), String> {
    for gain in [0u8, 128, 255] {
        let report = build_set_gain(gain);
        if report[2] != 0 {
            return Err(format!(
                "gain={gain}: reserved byte 2 = {:#04X}, expected 0x00",
                report[2]
            ));
        }
    }
    Ok(())
}

// ===========================================================================
// 4. Error handling for malformed / edge-case inputs
// ===========================================================================

#[test]
fn edge_nan_torque_produces_valid_report() -> Result<(), String> {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(f32::NAN);
    // NaN.clamp(-1.0,1.0) is implementation-defined but the report must
    // still have the correct structure: report ID and reserved zeros.
    if report[0] != CONSTANT_FORCE_REPORT_ID {
        return Err(format!("NaN: report ID = {:#04X}", report[0]));
    }
    if report[3] != 0 || report[4] != 0 {
        return Err("NaN: reserved bytes not zero".into());
    }
    let raw = decode_torque_raw(&report);
    if !(-MAX_TORQUE_SCALE..=MAX_TORQUE_SCALE).contains(&raw) {
        return Err(format!("NaN: raw {raw} out of range"));
    }
    Ok(())
}

#[test]
fn edge_positive_infinity_clamps_to_max() -> Result<(), String> {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(f32::INFINITY);
    let at_max = enc.encode(1.0);
    if report != at_max {
        return Err(format!(
            "+inf report {report:02X?} differs from max {at_max:02X?}"
        ));
    }
    Ok(())
}

#[test]
fn edge_negative_infinity_clamps_to_min() -> Result<(), String> {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(f32::NEG_INFINITY);
    let at_min = enc.encode(-1.0);
    if report != at_min {
        return Err(format!(
            "-inf report {report:02X?} differs from min {at_min:02X?}"
        ));
    }
    Ok(())
}

#[test]
fn edge_subnormal_torque_produces_zero_raw() -> Result<(), String> {
    let enc = OpenFFBoardTorqueEncoder;
    // Smallest positive subnormal f32
    let subnormal = f32::from_bits(1);
    let report = enc.encode(subnormal);
    let raw = decode_torque_raw(&report);
    // subnormal * 10000 truncates to 0
    if raw != 0 {
        return Err(format!("subnormal input produced raw {raw}, expected 0"));
    }
    Ok(())
}

#[test]
fn edge_negative_zero_encodes_as_zero() -> Result<(), String> {
    let enc = OpenFFBoardTorqueEncoder;
    let pos = enc.encode(0.0);
    let neg = enc.encode(-0.0);
    if pos != neg {
        return Err(format!(
            "-0.0 report {neg:02X?} differs from +0.0 report {pos:02X?}"
        ));
    }
    Ok(())
}

#[test]
fn edge_extreme_clamping_positive() -> Result<(), String> {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(f32::MAX);
    let at_max = enc.encode(1.0);
    if report != at_max {
        return Err(format!(
            "f32::MAX report {report:02X?} differs from max {at_max:02X?}"
        ));
    }
    Ok(())
}

#[test]
fn edge_extreme_clamping_negative() -> Result<(), String> {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(f32::MIN);
    let at_min = enc.encode(-1.0);
    if report != at_min {
        return Err(format!(
            "f32::MIN report {report:02X?} differs from min {at_min:02X?}"
        ));
    }
    Ok(())
}

#[test]
fn edge_just_inside_boundary_positive() -> Result<(), String> {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(0.9999);
    let raw = decode_torque_raw(&report);
    // 0.9999 * 10000 = 9999
    if raw != 9999 {
        return Err(format!("0.9999 encoded as {raw}, expected 9999"));
    }
    Ok(())
}

#[test]
fn edge_just_inside_boundary_negative() -> Result<(), String> {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(-0.9999);
    let raw = decode_torque_raw(&report);
    if raw != -9999 {
        return Err(format!("-0.9999 encoded as {raw}, expected -9999"));
    }
    Ok(())
}

// ===========================================================================
// 5. Property tests for parameter encoding
// ===========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Gain round-trips: byte written into the frame is recoverable.
    #[test]
    fn prop_gain_round_trip(gain: u8) {
        let report = build_set_gain(gain);
        prop_assert_eq!(report[1], gain, "gain byte must round-trip");
    }

    /// Enable flag round-trips correctly.
    #[test]
    fn prop_enable_flag_round_trip(enabled: bool) {
        let report = build_enable_ffb(enabled);
        let decoded = report[1] != 0;
        prop_assert_eq!(decoded, enabled, "enable flag must round-trip");
    }

    /// Torque encoding quantization error is bounded.
    #[test]
    fn prop_quantization_error_bounded(torque in -1.0f32..=1.0f32) {
        let enc = OpenFFBoardTorqueEncoder;
        let report = enc.encode(torque);
        let decoded = decode_torque_normalized(&report);
        let error = (torque - decoded).abs();
        let bound = 1.0 / MAX_TORQUE_SCALE as f32;
        prop_assert!(
            error < bound,
            "torque {torque} error {error} >= bound {bound}"
        );
    }

    /// All three report types use distinct report IDs.
    #[test]
    fn prop_report_ids_distinct(_unused: u8) {
        let ids = [CONSTANT_FORCE_REPORT_ID, ENABLE_FFB_REPORT_ID, GAIN_REPORT_ID];
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                prop_assert_ne!(ids[i], ids[j],
                    "report IDs at index {} and {} must differ", i, j);
            }
        }
    }

    /// Encoding torque and immediately decoding must not lose sign information
    /// for values with sufficient magnitude.
    #[test]
    fn prop_sign_fidelity(torque in -1.0f32..=1.0f32) {
        let enc = OpenFFBoardTorqueEncoder;
        let report = enc.encode(torque);
        let raw = decode_torque_raw(&report);
        if torque > 0.0001 {
            prop_assert!(raw > 0, "positive torque {torque} lost sign: raw={raw}");
        } else if torque < -0.0001 {
            prop_assert!(raw < 0, "negative torque {torque} lost sign: raw={raw}");
        }
    }

    /// Torque frames always have correct length and report ID regardless of input.
    #[test]
    fn prop_torque_frame_structural_invariants(torque in prop::num::f32::ANY) {
        let enc = OpenFFBoardTorqueEncoder;
        let report = enc.encode(torque);
        prop_assert_eq!(report.len(), CONSTANT_FORCE_REPORT_LEN);
        prop_assert_eq!(report[0], CONSTANT_FORCE_REPORT_ID);
        prop_assert_eq!(report[3], 0u8, "reserved byte 3");
        prop_assert_eq!(report[4], 0u8, "reserved byte 4");
    }

    /// Gain frames always have the correct report ID and reserved trailing byte.
    #[test]
    fn prop_gain_frame_structural_invariants(gain: u8) {
        let report = build_set_gain(gain);
        prop_assert_eq!(report[0], GAIN_REPORT_ID);
        prop_assert_eq!(report[2], 0u8, "gain reserved byte");
    }

    /// Enable FFB frames always have the correct report ID and reserved byte.
    #[test]
    fn prop_enable_ffb_frame_structural_invariants(enabled: bool) {
        let report = build_enable_ffb(enabled);
        prop_assert_eq!(report[0], ENABLE_FFB_REPORT_ID);
        prop_assert_eq!(report[2], 0u8, "enable reserved byte");
        let flag = report[1];
        prop_assert!(flag == 0 || flag == 1, "enable flag must be 0 or 1, got {flag}");
    }
}

// ===========================================================================
// 6. Known command constant validation
// ===========================================================================

#[test]
fn constant_force_report_id_is_0x01() -> Result<(), String> {
    if CONSTANT_FORCE_REPORT_ID != 0x01 {
        return Err(format!(
            "CONSTANT_FORCE_REPORT_ID = {:#04X}, expected 0x01",
            CONSTANT_FORCE_REPORT_ID
        ));
    }
    Ok(())
}

#[test]
fn constant_force_report_len_is_5() -> Result<(), String> {
    if CONSTANT_FORCE_REPORT_LEN != 5 {
        return Err(format!(
            "CONSTANT_FORCE_REPORT_LEN = {}, expected 5",
            CONSTANT_FORCE_REPORT_LEN
        ));
    }
    Ok(())
}

#[test]
fn enable_ffb_report_id_is_0x60() -> Result<(), String> {
    if ENABLE_FFB_REPORT_ID != 0x60 {
        return Err(format!(
            "ENABLE_FFB_REPORT_ID = {:#04X}, expected 0x60",
            ENABLE_FFB_REPORT_ID
        ));
    }
    Ok(())
}

#[test]
fn gain_report_id_is_0x61() -> Result<(), String> {
    if GAIN_REPORT_ID != 0x61 {
        return Err(format!(
            "GAIN_REPORT_ID = {:#04X}, expected 0x61",
            GAIN_REPORT_ID
        ));
    }
    Ok(())
}

#[test]
fn max_torque_scale_is_10000() -> Result<(), String> {
    if MAX_TORQUE_SCALE != 10_000 {
        return Err(format!(
            "MAX_TORQUE_SCALE = {}, expected 10000",
            MAX_TORQUE_SCALE
        ));
    }
    Ok(())
}

#[test]
fn vendor_id_matches_pid_codes_open_hardware() -> Result<(), String> {
    if OPENFFBOARD_VENDOR_ID != 0x1209 {
        return Err(format!(
            "OPENFFBOARD_VENDOR_ID = {:#06X}, expected 0x1209",
            OPENFFBOARD_VENDOR_ID
        ));
    }
    Ok(())
}

#[test]
fn main_product_id_is_ffb0() -> Result<(), String> {
    if OPENFFBOARD_PRODUCT_ID != 0xFFB0 {
        return Err(format!(
            "OPENFFBOARD_PRODUCT_ID = {:#06X}, expected 0xFFB0",
            OPENFFBOARD_PRODUCT_ID
        ));
    }
    Ok(())
}

#[test]
fn alt_product_id_is_ffb1() -> Result<(), String> {
    if OPENFFBOARD_PRODUCT_ID_ALT != 0xFFB1 {
        return Err(format!(
            "OPENFFBOARD_PRODUCT_ID_ALT = {:#06X}, expected 0xFFB1",
            OPENFFBOARD_PRODUCT_ID_ALT
        ));
    }
    Ok(())
}

#[test]
fn report_ids_are_all_distinct() -> Result<(), String> {
    let ids: [u8; 3] = [
        CONSTANT_FORCE_REPORT_ID,
        ENABLE_FFB_REPORT_ID,
        GAIN_REPORT_ID,
    ];
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            if ids[i] == ids[j] {
                return Err(format!(
                    "report IDs at index {i} and {j} collide: {:#04X}",
                    ids[i]
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn variant_all_array_has_correct_count() -> Result<(), String> {
    if OpenFFBoardVariant::ALL.len() != 2 {
        return Err(format!(
            "OpenFFBoardVariant::ALL has {} entries, expected 2",
            OpenFFBoardVariant::ALL.len()
        ));
    }
    Ok(())
}

#[test]
fn variant_product_ids_recognised_by_helper() -> Result<(), String> {
    for variant in &OpenFFBoardVariant::ALL {
        if !is_openffboard_product(variant.product_id()) {
            return Err(format!(
                "variant {:?} PID {:#06X} not recognised by is_openffboard_product",
                variant,
                variant.product_id()
            ));
        }
    }
    Ok(())
}

#[test]
fn variant_names_are_nonempty_and_printable() -> Result<(), String> {
    for variant in &OpenFFBoardVariant::ALL {
        let name = variant.name();
        if name.is_empty() {
            return Err(format!("variant {:?} has empty name", variant));
        }
        if let Some(ch) = name.chars().find(|c| c.is_control()) {
            return Err(format!(
                "variant {:?} name contains control char {:?}",
                variant, ch
            ));
        }
    }
    Ok(())
}

#[test]
fn variant_main_has_expected_pid() -> Result<(), String> {
    let pid = OpenFFBoardVariant::Main.product_id();
    if pid != OPENFFBOARD_PRODUCT_ID {
        return Err(format!(
            "Main variant PID {:#06X} != OPENFFBOARD_PRODUCT_ID {:#06X}",
            pid, OPENFFBOARD_PRODUCT_ID
        ));
    }
    Ok(())
}

#[test]
fn variant_alternate_has_expected_pid() -> Result<(), String> {
    let pid = OpenFFBoardVariant::Alternate.product_id();
    if pid != OPENFFBOARD_PRODUCT_ID_ALT {
        return Err(format!(
            "Alternate variant PID {:#06X} != OPENFFBOARD_PRODUCT_ID_ALT {:#06X}",
            pid, OPENFFBOARD_PRODUCT_ID_ALT
        ));
    }
    Ok(())
}
