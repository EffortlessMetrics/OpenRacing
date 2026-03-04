//! Insta snapshot tests for the Fanatec HID protocol encoding.
//!
//! These tests pin the exact wire-format bytes produced by the encoder
//! for the three canonical inputs: full-negative, zero, and full-positive force.

use insta::assert_snapshot;
use racing_wheel_hid_fanatec_protocol::{CONSTANT_FORCE_REPORT_LEN, FanatecConstantForceEncoder};

/// Helper: encode `torque_nm` with `max_torque_nm = 1.0` and return formatted bytes.
fn encode_bytes(torque_nm: f32) -> String {
    let encoder = FanatecConstantForceEncoder::new(1.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(torque_nm, 0, &mut out);
    format!("{:?}", out)
}

/// Helper: encode for a specific max torque.
fn encode_bytes_with_max(torque_nm: f32, max_torque_nm: f32) -> String {
    let encoder = FanatecConstantForceEncoder::new(max_torque_nm);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(torque_nm, 0, &mut out);
    format!("{:?}", out)
}

#[test]
fn test_snapshot_encode_constant_force_neg_one() {
    assert_snapshot!(encode_bytes(-1.0));
}

#[test]
fn test_snapshot_encode_constant_force_zero() {
    assert_snapshot!(encode_bytes(0.0));
}

#[test]
fn test_snapshot_encode_constant_force_pos_one() {
    assert_snapshot!(encode_bytes(1.0));
}

#[test]
fn test_snapshot_encode_zero_report() {
    let encoder = FanatecConstantForceEncoder::new(1.0);
    let mut out = [0xFFu8; CONSTANT_FORCE_REPORT_LEN];
    encoder.encode_zero(&mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_mode_switch_report() {
    use racing_wheel_hid_fanatec_protocol::build_mode_switch_report;
    let report = build_mode_switch_report();
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_stop_all_report() {
    use racing_wheel_hid_fanatec_protocol::build_stop_all_report;
    let report = build_stop_all_report();
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_set_gain_report_full() {
    use racing_wheel_hid_fanatec_protocol::build_set_gain_report;
    let report = build_set_gain_report(100);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_led_report_all_on() {
    use racing_wheel_hid_fanatec_protocol::build_led_report;
    let report = build_led_report(0xFFFF, 255);
    assert_snapshot!(format!("{:?}", report));
}

/// ClubSport DD+ (12 Nm) at full positive torque — pin wire format for regression detection.
#[test]
fn test_snapshot_clubsport_dd_full_positive_torque() {
    assert_snapshot!(encode_bytes_with_max(12.0, 12.0));
}

/// ClubSport DD+ (12 Nm) at full negative torque.
#[test]
fn test_snapshot_clubsport_dd_full_negative_torque() {
    assert_snapshot!(encode_bytes_with_max(-12.0, 12.0));
}

/// ClubSport DD+ (12 Nm) at half torque.
#[test]
fn test_snapshot_clubsport_dd_half_torque() {
    assert_snapshot!(encode_bytes_with_max(6.0, 12.0));
}

/// Rotation range report for 540 degrees (common full-lock for GT car).
#[test]
fn test_snapshot_rotation_range_540() {
    use racing_wheel_hid_fanatec_protocol::build_rotation_range_report;
    let report = build_rotation_range_report(540);
    assert_snapshot!(format!("{:?}", report));
}

/// Rotation range report clamped to minimum (90 degrees).
#[test]
fn test_snapshot_rotation_range_clamp_min() {
    use racing_wheel_hid_fanatec_protocol::build_rotation_range_report;
    let report = build_rotation_range_report(0);
    assert_snapshot!(format!("{:?}", report));
}

/// Rotation range report clamped to maximum (1080 degrees).
#[test]
fn test_snapshot_rotation_range_clamp_max() {
    use racing_wheel_hid_fanatec_protocol::build_rotation_range_report;
    let report = build_rotation_range_report(u16::MAX);
    assert_snapshot!(format!("{:?}", report));
}

/// Kernel range sequence for 900 degrees (common GT car).
#[test]
fn test_snapshot_kernel_range_sequence_900() {
    use racing_wheel_hid_fanatec_protocol::build_kernel_range_sequence;
    let seq = build_kernel_range_sequence(900);
    assert_snapshot!(format!("{:?}", seq));
}

/// Kernel range sequence for 2520 degrees (DD max).
#[test]
fn test_snapshot_kernel_range_sequence_2520() {
    use racing_wheel_hid_fanatec_protocol::build_kernel_range_sequence;
    let seq = build_kernel_range_sequence(2520);
    assert_snapshot!(format!("{:?}", seq));
}

/// Kernel range sequence clamped to minimum (below 90).
#[test]
fn test_snapshot_kernel_range_sequence_clamp_min() {
    use racing_wheel_hid_fanatec_protocol::build_kernel_range_sequence;
    let seq = build_kernel_range_sequence(40);
    assert_snapshot!(format!("{:?}", seq));
}

/// fix_report_values with mixed values above and below 0x80.
#[test]
fn test_snapshot_fix_report_values_mixed() {
    use racing_wheel_hid_fanatec_protocol::fix_report_values;
    let mut values: [i16; 7] = [0x00, 0x7F, 0x80, 0xFF, 0xA0, 0x01, 0xFE];
    fix_report_values(&mut values);
    assert_snapshot!(format!("{:?}", values));
}

// ── Device variant matrix snapshot ───────────────────────────────────────────

/// Pin the complete wheelbase model matrix: model name, torque, CPR, 1000 Hz
/// support, max rotation, highres flag, and sign-fix flag for every known PID.
#[test]
fn test_snapshot_all_wheelbase_variants() {
    use racing_wheel_hid_fanatec_protocol::{FanatecModel, product_ids};
    let pids: &[(u16, &str)] = &[
        (product_ids::CLUBSPORT_V2, "ClubSport V2"),
        (product_ids::CLUBSPORT_V2_5, "ClubSport V2.5"),
        (product_ids::CSL_ELITE_PS4, "CSL Elite PS4"),
        (product_ids::CSL_ELITE, "CSL Elite"),
        (product_ids::DD1, "DD1"),
        (product_ids::DD2, "DD2"),
        (product_ids::CSR_ELITE, "CSR Elite"),
        (product_ids::CSL_DD, "CSL DD"),
        (product_ids::GT_DD_PRO, "GT DD Pro"),
        (product_ids::CLUBSPORT_DD, "ClubSport DD"),
    ];
    let mut lines = Vec::new();
    for &(pid, name) in pids {
        let model = FanatecModel::from_product_id(pid);
        lines.push(format!(
            "0x{pid:04X} {name}: model={model:?}, torque={:.1}Nm, cpr={}, 1kHz={}, max_rot={}°, highres={}, sign_fix={}",
            model.max_torque_nm(),
            model.encoder_cpr(),
            model.supports_1000hz(),
            model.max_rotation_degrees(),
            model.is_highres(),
            model.needs_sign_fix(),
        ));
    }
    assert_snapshot!(lines.join("\n"));
}

/// Pin the complete pedal model matrix: model name, axis count for every known PID.
#[test]
fn test_snapshot_all_pedal_variants() {
    use racing_wheel_hid_fanatec_protocol::{FanatecPedalModel, product_ids};
    let pids: &[(u16, &str)] = &[
        (
            product_ids::CLUBSPORT_PEDALS_V1_V2,
            "ClubSport Pedals V1/V2",
        ),
        (product_ids::CLUBSPORT_PEDALS_V3, "ClubSport Pedals V3"),
        (product_ids::CSL_ELITE_PEDALS, "CSL Elite Pedals"),
        (product_ids::CSL_PEDALS_LC, "CSL Pedals LC"),
        (product_ids::CSL_PEDALS_V2, "CSL Pedals V2"),
    ];
    let mut lines = Vec::new();
    for &(pid, name) in pids {
        let model = FanatecPedalModel::from_product_id(pid);
        lines.push(format!(
            "0x{pid:04X} {name}: model={model:?}, axes={}",
            model.axis_count(),
        ));
    }
    assert_snapshot!(lines.join("\n"));
}

/// Pin the complete rim ID matrix: byte value, variant, capabilities.
#[test]
fn test_snapshot_all_rim_variants() {
    use racing_wheel_hid_fanatec_protocol::{FanatecRimId, rim_ids};
    let rims: &[(u8, &str)] = &[
        (rim_ids::BMW_GT2, "BMW GT2"),
        (rim_ids::FORMULA_V2, "Formula V2"),
        (rim_ids::FORMULA_V2_5, "Formula V2.5"),
        (rim_ids::CSL_ELITE_P1, "CSL Elite P1"),
        (rim_ids::MCLAREN_GT3_V2, "McLaren GT3 V2"),
        (rim_ids::PORSCHE_911_GT3_R, "Porsche 911 GT3 R"),
        (rim_ids::PORSCHE_918_RSR, "Porsche 918 RSR"),
        (rim_ids::CLUBSPORT_RS, "ClubSport RS"),
        (rim_ids::WRC, "WRC"),
        (rim_ids::PODIUM_HUB, "Podium Hub"),
    ];
    let mut lines = Vec::new();
    for &(byte, name) in rims {
        let rim = FanatecRimId::from_byte(byte);
        lines.push(format!(
            "0x{byte:02X} {name}: variant={rim:?}, funky_switch={}, dual_clutch={}, rotary_encoders={}",
            rim.has_funky_switch(),
            rim.has_dual_clutch(),
            rim.has_rotary_encoders(),
        ));
    }
    assert_snapshot!(lines.join("\n"));
}

// ── Protocol constants snapshot ──────────────────────────────────────────────

/// Pin all protocol constants (VID, report IDs, FFB commands, LED commands)
/// to detect accidental changes.
#[test]
fn test_snapshot_protocol_constants() {
    use racing_wheel_hid_fanatec_protocol::{FANATEC_VENDOR_ID, ids};
    let lines = [
        format!("VENDOR_ID=0x{:04X}", FANATEC_VENDOR_ID),
        format!(
            "report_ids::STANDARD_INPUT=0x{:02X}",
            ids::report_ids::STANDARD_INPUT
        ),
        format!(
            "report_ids::EXTENDED_INPUT=0x{:02X}",
            ids::report_ids::EXTENDED_INPUT
        ),
        format!(
            "report_ids::MODE_SWITCH=0x{:02X}",
            ids::report_ids::MODE_SWITCH
        ),
        format!(
            "report_ids::FFB_OUTPUT=0x{:02X}",
            ids::report_ids::FFB_OUTPUT
        ),
        format!(
            "report_ids::LED_DISPLAY=0x{:02X}",
            ids::report_ids::LED_DISPLAY
        ),
        format!(
            "ffb_commands::CONSTANT_FORCE=0x{:02X}",
            ids::ffb_commands::CONSTANT_FORCE
        ),
        format!(
            "ffb_commands::SET_ROTATION_RANGE=0x{:02X}",
            ids::ffb_commands::SET_ROTATION_RANGE
        ),
        format!(
            "ffb_commands::SET_GAIN=0x{:02X}",
            ids::ffb_commands::SET_GAIN
        ),
        format!(
            "ffb_commands::STOP_ALL=0x{:02X}",
            ids::ffb_commands::STOP_ALL
        ),
        format!(
            "led_commands::REV_LIGHTS=0x{:02X}",
            ids::led_commands::REV_LIGHTS
        ),
        format!("led_commands::DISPLAY=0x{:02X}", ids::led_commands::DISPLAY),
        format!("led_commands::RUMBLE=0x{:02X}", ids::led_commands::RUMBLE),
    ];
    assert_snapshot!(lines.join("\n"));
}
