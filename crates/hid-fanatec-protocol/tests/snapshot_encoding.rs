//! Extended snapshot tests for Fanatec wire-format encoding.
//!
//! These tests supplement `snapshot_tests.rs` by covering build helpers,
//! input parsing, device identification, and boundary-value encodings
//! that would detect wire-format regressions.

use insta::assert_snapshot;
use racing_wheel_hid_fanatec_protocol::{
    self as fan, CONSTANT_FORCE_REPORT_LEN, FanatecConstantForceEncoder,
};

// ── Encoder boundary values ──────────────────────────────────────────────────

#[test]
fn test_snapshot_encode_quarter_positive_torque() {
    let encoder = FanatecConstantForceEncoder::new(1.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(0.25, 0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_encode_quarter_negative_torque() {
    let encoder = FanatecConstantForceEncoder::new(1.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(-0.25, 0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_encode_clamp_above_max() {
    let encoder = FanatecConstantForceEncoder::new(1.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(5.0, 0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_encode_clamp_below_neg_max() {
    let encoder = FanatecConstantForceEncoder::new(1.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(-5.0, 0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

// ── Display and rumble report builders ───────────────────────────────────────

#[test]
fn test_snapshot_display_report_zeros() {
    let report = fan::build_display_report(0, [0, 0, 0], 0);
    assert_snapshot!(format!("{report:02X?}"));
}

#[test]
fn test_snapshot_display_report_all_nines() {
    let report = fan::build_display_report(1, [9, 9, 9], 255);
    assert_snapshot!(format!("{report:02X?}"));
}

#[test]
fn test_snapshot_rumble_report_short_burst() {
    let report = fan::build_rumble_report(128, 128, 10);
    assert_snapshot!(format!("{report:02X?}"));
}

#[test]
fn test_snapshot_rumble_report_full_left_only() {
    let report = fan::build_rumble_report(255, 0, 255);
    assert_snapshot!(format!("{report:02X?}"));
}

// ── Input report parsing ─────────────────────────────────────────────────────

#[test]
fn test_snapshot_parse_standard_report_center() -> Result<(), String> {
    // Construct a 16-byte standard report: centered steering, no pedals, no buttons
    let mut data = [0u8; 16];
    data[0] = 0x01; // report ID
    // steering center: 0x8000 LE
    data[1] = 0x00;
    data[2] = 0x80;
    // hat neutral
    data[9] = 0x0F;
    let state = fan::parse_standard_report(&data).ok_or("parse_standard_report returned None")?;
    assert_snapshot!(format!(
        "steering={:.4}, throttle={:.4}, brake={:.4}, clutch={:.4}, buttons=0x{:04X}, hat=0x{:X}",
        state.steering, state.throttle, state.brake, state.clutch, state.buttons, state.hat,
    ));
    Ok(())
}

// ── Device identification ────────────────────────────────────────────────────

#[test]
fn test_snapshot_is_wheelbase_known_pids() {
    let results: Vec<String> = [
        fan::product_ids::DD1,
        fan::product_ids::DD2,
        fan::product_ids::CSL_DD,
        fan::product_ids::GT_DD_PRO,
        fan::product_ids::CLUBSPORT_DD,
        fan::product_ids::CLUBSPORT_V2,
        fan::product_ids::CSL_ELITE,
    ]
    .iter()
    .map(|&pid| format!("0x{pid:04X}={}", fan::is_wheelbase_product(pid)))
    .collect();
    assert_snapshot!(results.join(", "));
}

#[test]
fn test_snapshot_is_wheelbase_unknown_pid() {
    assert_snapshot!(format!("unknown={}", fan::is_wheelbase_product(0xFFFF)));
}

#[test]
fn test_snapshot_is_pedal_known_pids() {
    let results: Vec<String> = [
        fan::product_ids::CLUBSPORT_PEDALS_V3,
        fan::product_ids::CSL_ELITE_PEDALS,
        fan::product_ids::CSL_PEDALS_LC,
    ]
    .iter()
    .map(|&pid| format!("0x{pid:04X}={}", fan::is_pedal_product(pid)))
    .collect();
    assert_snapshot!(results.join(", "));
}

#[test]
fn test_snapshot_is_pedal_unknown_pid() {
    assert_snapshot!(format!("unknown={}", fan::is_pedal_product(0xFFFF)));
}

// ── Gain report boundary ─────────────────────────────────────────────────────

#[test]
fn test_snapshot_set_gain_report_zero() {
    let report = fan::build_set_gain_report(0);
    assert_snapshot!(format!("{report:02X?}"));
}

// ── Rotation range boundary ──────────────────────────────────────────────────

#[test]
fn test_snapshot_rotation_range_900() {
    let report = fan::build_rotation_range_report(900);
    assert_snapshot!(format!("{report:02X?}"));
}
