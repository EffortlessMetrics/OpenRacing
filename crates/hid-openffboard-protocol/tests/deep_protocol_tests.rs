//! Deep protocol tests for OpenFFBoard HID protocol.
//!
//! Tests cover device identification, command encoding, report structure,
//! and OWP-1 protocol compliance.

use racing_wheel_hid_openffboard_protocol::{
    CONSTANT_FORCE_REPORT_ID, CONSTANT_FORCE_REPORT_LEN, GAIN_REPORT_ID, OPENFFBOARD_PRODUCT_ID,
    OPENFFBOARD_PRODUCT_ID_ALT, OPENFFBOARD_VENDOR_ID, OpenFFBoardTorqueEncoder,
    OpenFFBoardVariant, build_enable_ffb, build_set_gain, is_openffboard_product,
};

// ─── Device identification ───────────────────────────────────────────────────

#[test]
fn vendor_id_matches_pid_codes_registry() {
    assert_eq!(OPENFFBOARD_VENDOR_ID, 0x1209, "pid.codes open hardware VID");
}

#[test]
fn main_product_id_is_ffb0() {
    assert_eq!(OPENFFBOARD_PRODUCT_ID, 0xFFB0);
}

#[test]
fn alt_product_id_is_ffb1() {
    assert_eq!(OPENFFBOARD_PRODUCT_ID_ALT, 0xFFB1);
}

#[test]
fn is_openffboard_recognises_main_pid_only() {
    assert!(is_openffboard_product(0xFFB0));
    assert!(!is_openffboard_product(0xFFB1));
}

#[test]
fn is_openffboard_rejects_unknown_pid() {
    assert!(!is_openffboard_product(0x0000));
    assert!(!is_openffboard_product(0x1234));
    assert!(!is_openffboard_product(0xFFFF));
    assert!(!is_openffboard_product(0xFFB1));
}

#[test]
fn variant_all_contains_both() {
    assert_eq!(OpenFFBoardVariant::ALL.len(), 2);
    assert!(OpenFFBoardVariant::ALL.contains(&OpenFFBoardVariant::Main));
    assert!(OpenFFBoardVariant::ALL.contains(&OpenFFBoardVariant::Alternate));
}

#[test]
fn variant_product_ids_are_unique() {
    let main_pid = OpenFFBoardVariant::Main.product_id();
    let alt_pid = OpenFFBoardVariant::Alternate.product_id();
    assert_ne!(main_pid, alt_pid);
}

#[test]
fn variant_vendor_id_shared() {
    assert_eq!(
        OpenFFBoardVariant::Main.vendor_id(),
        OpenFFBoardVariant::Alternate.vendor_id()
    );
    assert_eq!(OpenFFBoardVariant::Main.vendor_id(), OPENFFBOARD_VENDOR_ID);
}

#[test]
fn variant_names_non_empty() {
    for v in &OpenFFBoardVariant::ALL {
        assert!(!v.name().is_empty());
    }
}

// ─── Torque encoding ─────────────────────────────────────────────────────────

#[test]
fn encode_zero_torque_produces_report_id_and_zero_payload() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(0.0);
    assert_eq!(report.len(), CONSTANT_FORCE_REPORT_LEN);
    assert_eq!(report[0], CONSTANT_FORCE_REPORT_ID);
    assert_eq!(i16::from_le_bytes([report[1], report[2]]), 0);
    assert_eq!(report[3], 0x00);
    assert_eq!(report[4], 0x00);
}

#[test]
fn encode_full_positive_maps_to_10000() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(1.0);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, 10_000);
}

#[test]
fn encode_full_negative_maps_to_minus_10000() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(-1.0);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, -10_000);
}

#[test]
fn encode_half_torque_maps_to_5000() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(0.5);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, 5_000);
}

#[test]
fn encode_clamps_above_one() {
    let enc = OpenFFBoardTorqueEncoder;
    assert_eq!(enc.encode(1.5), enc.encode(1.0));
    assert_eq!(enc.encode(100.0), enc.encode(1.0));
}

#[test]
fn encode_clamps_below_negative_one() {
    let enc = OpenFFBoardTorqueEncoder;
    assert_eq!(enc.encode(-1.5), enc.encode(-1.0));
    assert_eq!(enc.encode(-100.0), enc.encode(-1.0));
}

#[test]
fn encode_report_length_is_constant() {
    let enc = OpenFFBoardTorqueEncoder;
    for t in [0.0, 0.5, -0.5, 1.0, -1.0, 2.0, -2.0] {
        assert_eq!(enc.encode(t).len(), CONSTANT_FORCE_REPORT_LEN);
    }
}

#[test]
fn encode_reserved_bytes_always_zero() {
    let enc = OpenFFBoardTorqueEncoder;
    for t in [0.0, 0.25, -0.75, 1.0, -1.0] {
        let report = enc.encode(t);
        assert_eq!(report[3], 0x00);
        assert_eq!(report[4], 0x00);
    }
}

// ─── Feature report encoding ────────────────────────────────────────────────

#[test]
fn enable_ffb_true_produces_correct_report() {
    let report = build_enable_ffb(true);
    assert_eq!(report.len(), 3);
    assert_eq!(report[0], 0x60);
    assert_eq!(report[1], 0x01);
    assert_eq!(report[2], 0x00);
}

#[test]
fn enable_ffb_false_produces_correct_report() {
    let report = build_enable_ffb(false);
    assert_eq!(report[0], 0x60);
    assert_eq!(report[1], 0x00);
    assert_eq!(report[2], 0x00);
}

#[test]
fn set_gain_full_scale() {
    let report = build_set_gain(255);
    assert_eq!(report[0], GAIN_REPORT_ID);
    assert_eq!(report[1], 255);
    assert_eq!(report[2], 0x00);
}

#[test]
fn set_gain_zero() {
    let report = build_set_gain(0);
    assert_eq!(report[0], GAIN_REPORT_ID);
    assert_eq!(report[1], 0);
}

// ─── OWP-1 protocol compliance ──────────────────────────────────────────────

#[test]
fn torque_encoding_is_little_endian_i16() {
    let enc = OpenFFBoardTorqueEncoder;
    // 0x0100 = 256 in LE → bytes should be [0x00, 0x01]
    let torque_fraction = 256.0 / 10_000.0;
    let report = enc.encode(torque_fraction);
    assert_eq!(report[1], 0x00); // lo byte
    assert_eq!(report[2], 0x01); // hi byte
}

#[test]
fn negative_torque_encoding_uses_twos_complement_le() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(-1.0);
    // -10000 in i16 LE: 0xD8F0 → [0xF0, 0xD8]
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, -10_000);
}

#[test]
fn encode_does_not_panic_on_nan() {
    let enc = OpenFFBoardTorqueEncoder;
    let _ = enc.encode(f32::NAN);
}

#[test]
fn encode_does_not_panic_on_infinity() {
    let enc = OpenFFBoardTorqueEncoder;
    let _ = enc.encode(f32::INFINITY);
    let _ = enc.encode(f32::NEG_INFINITY);
}
