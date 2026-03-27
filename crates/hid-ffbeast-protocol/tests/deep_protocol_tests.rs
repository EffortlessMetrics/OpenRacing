//! Deep protocol tests for FFBeast HID protocol.
//!
//! Tests cover device identification, torque encoding, feature reports,
//! and configuration commands.

use racing_wheel_hid_ffbeast_protocol::{
    CONSTANT_FORCE_REPORT_ID, CONSTANT_FORCE_REPORT_LEN, FFBEAST_PRODUCT_ID_JOYSTICK,
    FFBEAST_PRODUCT_ID_RUDDER, FFBEAST_PRODUCT_ID_WHEEL, FFBEAST_VENDOR_ID, FFBeastTorqueEncoder,
    GAIN_REPORT_ID, build_enable_ffb, build_set_gain, is_ffbeast_product,
};

// ─── Device identification ───────────────────────────────────────────────────

#[test]
fn vendor_id_is_renesas() {
    assert_eq!(FFBEAST_VENDOR_ID, 0x045B);
}

#[test]
fn joystick_pid() {
    assert_eq!(FFBEAST_PRODUCT_ID_JOYSTICK, 0x58F9);
}

#[test]
fn rudder_pid() {
    assert_eq!(FFBEAST_PRODUCT_ID_RUDDER, 0x5968);
}

#[test]
fn wheel_pid() {
    assert_eq!(FFBEAST_PRODUCT_ID_WHEEL, 0x59D7);
}

#[test]
fn is_ffbeast_recognises_all_three_products() {
    assert!(is_ffbeast_product(FFBEAST_PRODUCT_ID_JOYSTICK));
    assert!(is_ffbeast_product(FFBEAST_PRODUCT_ID_RUDDER));
    assert!(is_ffbeast_product(FFBEAST_PRODUCT_ID_WHEEL));
}

#[test]
fn is_ffbeast_rejects_unknown_pid() {
    assert!(!is_ffbeast_product(0x0000));
    assert!(!is_ffbeast_product(0xFFFF));
    assert!(!is_ffbeast_product(0x1234));
}

#[test]
fn all_product_ids_are_unique() {
    let pids = [
        FFBEAST_PRODUCT_ID_JOYSTICK,
        FFBEAST_PRODUCT_ID_RUDDER,
        FFBEAST_PRODUCT_ID_WHEEL,
    ];
    for i in 0..pids.len() {
        for j in (i + 1)..pids.len() {
            assert_ne!(pids[i], pids[j], "PIDs at index {i} and {j} must differ");
        }
    }
}

// ─── Torque encoding ─────────────────────────────────────────────────────────

#[test]
fn encode_zero_torque() {
    let enc = FFBeastTorqueEncoder;
    let report = enc.encode(0.0);
    assert_eq!(report[0], CONSTANT_FORCE_REPORT_ID);
    assert_eq!(i16::from_le_bytes([report[1], report[2]]), 0);
    assert_eq!(report[3], 0x00);
    assert_eq!(report[4], 0x00);
}

#[test]
fn encode_full_positive_torque() {
    let enc = FFBeastTorqueEncoder;
    let report = enc.encode(1.0);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, 10_000);
}

#[test]
fn encode_full_negative_torque() {
    let enc = FFBeastTorqueEncoder;
    let report = enc.encode(-1.0);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, -10_000);
}

#[test]
fn encode_quarter_torque() {
    let enc = FFBeastTorqueEncoder;
    let report = enc.encode(0.25);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, 2_500);
}

#[test]
fn encode_clamps_above_range() {
    let enc = FFBeastTorqueEncoder;
    assert_eq!(enc.encode(5.0), enc.encode(1.0));
}

#[test]
fn encode_clamps_below_range() {
    let enc = FFBeastTorqueEncoder;
    assert_eq!(enc.encode(-5.0), enc.encode(-1.0));
}

#[test]
fn encode_report_id_always_0x01() {
    let enc = FFBeastTorqueEncoder;
    for t in [0.0, 0.5, -0.5, 1.0, -1.0, 50.0, -50.0] {
        assert_eq!(enc.encode(t)[0], 0x01);
    }
}

#[test]
fn encode_report_length_always_5() {
    let enc = FFBeastTorqueEncoder;
    for t in [0.0, 0.5, -0.5, 1.0, -1.0] {
        assert_eq!(enc.encode(t).len(), CONSTANT_FORCE_REPORT_LEN);
    }
}

#[test]
fn encode_reserved_bytes_zero() {
    let enc = FFBeastTorqueEncoder;
    for t in [0.0, 0.33, -0.77, 1.0, -1.0] {
        let r = enc.encode(t);
        assert_eq!(r[3], 0x00);
        assert_eq!(r[4], 0x00);
    }
}

#[test]
fn encode_symmetry_positive_negative() {
    let enc = FFBeastTorqueEncoder;
    let pos = enc.encode(0.5);
    let neg = enc.encode(-0.5);
    let pos_raw = i16::from_le_bytes([pos[1], pos[2]]);
    let neg_raw = i16::from_le_bytes([neg[1], neg[2]]);
    assert_eq!(pos_raw, -neg_raw);
}

#[test]
fn encode_does_not_panic_on_special_floats() {
    let enc = FFBeastTorqueEncoder;
    let _ = enc.encode(f32::NAN);
    let _ = enc.encode(f32::INFINITY);
    let _ = enc.encode(f32::NEG_INFINITY);
    let _ = enc.encode(f32::MIN);
    let _ = enc.encode(f32::MAX);
}

// ─── Configuration commands ──────────────────────────────────────────────────

#[test]
fn enable_ffb_true() {
    let report = build_enable_ffb(true);
    assert_eq!(report, [0x60, 0x01, 0x00]);
}

#[test]
fn enable_ffb_false() {
    let report = build_enable_ffb(false);
    assert_eq!(report, [0x60, 0x00, 0x00]);
}

#[test]
fn set_gain_full() {
    let report = build_set_gain(255);
    assert_eq!(report[0], GAIN_REPORT_ID);
    assert_eq!(report[1], 255);
    assert_eq!(report[2], 0x00);
}

#[test]
fn set_gain_zero() {
    let report = build_set_gain(0);
    assert_eq!(report, [GAIN_REPORT_ID, 0, 0x00]);
}

#[test]
fn set_gain_mid_range() {
    let report = build_set_gain(128);
    assert_eq!(report[1], 128);
}
