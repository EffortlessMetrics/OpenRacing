//! Deep tests for OpenFFBoard: command encoding/decoding, motor control,
//! encoder feedback parsing, configuration commands, firmware update protocol,
//! and proptest encode/decode roundtrip.

use racing_wheel_hid_openffboard_protocol::output::{ENABLE_FFB_REPORT_ID, MAX_TORQUE_SCALE};
use racing_wheel_hid_openffboard_protocol::{
    build_enable_ffb, build_set_gain, is_openffboard_product, OpenFFBoardTorqueEncoder,
    OpenFFBoardVariant, CONSTANT_FORCE_REPORT_ID, CONSTANT_FORCE_REPORT_LEN, GAIN_REPORT_ID,
    OPENFFBOARD_PRODUCT_ID, OPENFFBOARD_PRODUCT_ID_ALT, OPENFFBOARD_VENDOR_ID,
};

// ── Command encoding/decoding ────────────────────────────────────────────────

#[test]
fn encode_zero_then_decode_roundtrip() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(0.0);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, 0);
    let decoded = raw as f32 / MAX_TORQUE_SCALE as f32;
    assert!((decoded).abs() < 0.001);
}

#[test]
fn encode_positive_then_decode_roundtrip() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(0.75);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, 7500);
    let decoded = raw as f32 / MAX_TORQUE_SCALE as f32;
    assert!((decoded - 0.75).abs() < 0.001);
}

#[test]
fn encode_negative_then_decode_roundtrip() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(-0.3);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, -3000);
    let decoded = raw as f32 / MAX_TORQUE_SCALE as f32;
    assert!((decoded - (-0.3)).abs() < 0.001);
}

#[test]
fn encode_report_id_always_constant_force() {
    let enc = OpenFFBoardTorqueEncoder;
    for t in [0.0, 0.5, -0.5, 1.0, -1.0, 0.001, -0.001] {
        let report = enc.encode(t);
        assert_eq!(report[0], CONSTANT_FORCE_REPORT_ID);
    }
}

#[test]
fn encode_reserved_bytes_always_zero_across_values() {
    let enc = OpenFFBoardTorqueEncoder;
    let values = [0.0, 0.1, -0.1, 0.5, -0.5, 0.99, -0.99, 1.0, -1.0];
    for t in values {
        let report = enc.encode(t);
        assert_eq!(report[3], 0x00, "Reserved byte 3 non-zero for torque {t}");
        assert_eq!(report[4], 0x00, "Reserved byte 4 non-zero for torque {t}");
    }
}

// ── Motor control commands ───────────────────────────────────────────────────

#[test]
fn motor_control_small_torque_precision() {
    let enc = OpenFFBoardTorqueEncoder;
    // 0.001 * 10000 = 10
    let report = enc.encode(0.001);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, 10);
}

#[test]
fn motor_control_negative_small_torque_precision() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(-0.001);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, -10);
}

#[test]
fn motor_control_clamping_positive_overflow() {
    let enc = OpenFFBoardTorqueEncoder;
    let report_over = enc.encode(5.0);
    let report_max = enc.encode(1.0);
    assert_eq!(report_over, report_max);
}

#[test]
fn motor_control_clamping_negative_overflow() {
    let enc = OpenFFBoardTorqueEncoder;
    let report_under = enc.encode(-5.0);
    let report_min = enc.encode(-1.0);
    assert_eq!(report_under, report_min);
}

#[test]
fn motor_control_le_byte_order_verified() {
    let enc = OpenFFBoardTorqueEncoder;
    // Encode 0.1 → 1000 → 0x03E8 → LE: [0xE8, 0x03]
    let report = enc.encode(0.1);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, 1000);
    assert_eq!(report[1], 0xE8);
    assert_eq!(report[2], 0x03);
}

#[test]
fn motor_control_negative_le_byte_order_verified() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(-1.0);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, -10_000);
    // -10000 i16 = 0xD8F0 → LE: [0xF0, 0xD8]
    assert_eq!(report[1], 0xF0);
    assert_eq!(report[2], 0xD8);
}

// ── Encoder feedback parsing (decode from wire) ──────────────────────────────

fn decode_torque_from_report(report: &[u8; CONSTANT_FORCE_REPORT_LEN]) -> Option<f32> {
    if report[0] != CONSTANT_FORCE_REPORT_ID {
        return None;
    }
    let raw = i16::from_le_bytes([report[1], report[2]]);
    Some(raw as f32 / MAX_TORQUE_SCALE as f32)
}

#[test]
fn encoder_feedback_decode_positive() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(0.6);
    let decoded = decode_torque_from_report(&report);
    assert!(decoded.is_some());
    let val = decoded.unwrap_or(0.0);
    assert!((val - 0.6).abs() < 0.001);
}

#[test]
fn encoder_feedback_decode_negative() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(-0.8);
    let decoded = decode_torque_from_report(&report);
    assert!(decoded.is_some());
    let val = decoded.unwrap_or(0.0);
    assert!((val - (-0.8)).abs() < 0.001);
}

#[test]
fn encoder_feedback_decode_zero() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(0.0);
    let decoded = decode_torque_from_report(&report);
    assert!(decoded.is_some());
    let val = decoded.unwrap_or(f32::NAN);
    assert!(val.abs() < 0.001);
}

#[test]
fn encoder_feedback_rejects_wrong_report_id() {
    let bad_report: [u8; CONSTANT_FORCE_REPORT_LEN] = [0xFF, 0x00, 0x00, 0x00, 0x00];
    assert!(decode_torque_from_report(&bad_report).is_none());
}

// ── Configuration commands ───────────────────────────────────────────────────

#[test]
fn config_enable_ffb_report_structure() {
    let on = build_enable_ffb(true);
    assert_eq!(on.len(), 3);
    assert_eq!(on[0], ENABLE_FFB_REPORT_ID);
    assert_eq!(on[1], 0x01);
    assert_eq!(on[2], 0x00);
}

#[test]
fn config_disable_ffb_report_structure() {
    let off = build_enable_ffb(false);
    assert_eq!(off[0], ENABLE_FFB_REPORT_ID);
    assert_eq!(off[1], 0x00);
    assert_eq!(off[2], 0x00);
}

#[test]
fn config_set_gain_boundary_values() {
    for gain in [0u8, 1, 64, 127, 128, 254, 255] {
        let report = build_set_gain(gain);
        assert_eq!(report[0], GAIN_REPORT_ID);
        assert_eq!(report[1], gain, "Gain roundtrip failed for {gain}");
        assert_eq!(report[2], 0x00, "Reserved byte non-zero for gain {gain}");
    }
}

#[test]
fn config_report_ids_are_distinct() {
    assert_ne!(CONSTANT_FORCE_REPORT_ID, ENABLE_FFB_REPORT_ID);
    assert_ne!(CONSTANT_FORCE_REPORT_ID, GAIN_REPORT_ID);
    assert_ne!(ENABLE_FFB_REPORT_ID, GAIN_REPORT_ID);
}

// ── Firmware update / variant protocol ───────────────────────────────────────

#[test]
fn firmware_variant_product_ids_match_constants() {
    assert_eq!(
        OpenFFBoardVariant::Main.product_id(),
        OPENFFBOARD_PRODUCT_ID
    );
    assert_eq!(
        OpenFFBoardVariant::Alternate.product_id(),
        OPENFFBOARD_PRODUCT_ID_ALT
    );
}

#[test]
fn firmware_variant_vendor_id_is_pid_codes() {
    for v in &OpenFFBoardVariant::ALL {
        assert_eq!(v.vendor_id(), 0x1209);
    }
}

#[test]
fn firmware_variant_main_recognized_by_is_openffboard() {
    assert!(
        is_openffboard_product(OpenFFBoardVariant::Main.product_id()),
        "Main variant PID 0x{:04X} must be recognized",
        OpenFFBoardVariant::Main.product_id()
    );
}

#[test]
fn firmware_variant_alt_not_recognized_by_is_openffboard() {
    assert!(
        !is_openffboard_product(OpenFFBoardVariant::Alternate.product_id()),
        "Alternate variant PID 0x{:04X} must NOT be recognized",
        OpenFFBoardVariant::Alternate.product_id()
    );
}

#[test]
fn firmware_variant_names_unique_and_non_empty() {
    let names: Vec<&str> = OpenFFBoardVariant::ALL.iter().map(|v| v.name()).collect();
    for name in &names {
        assert!(!name.is_empty());
    }
    // All names are distinct
    for (i, a) in names.iter().enumerate() {
        for (j, b) in names.iter().enumerate() {
            if i != j {
                assert_ne!(a, b, "Variant names at {i} and {j} must differ");
            }
        }
    }
}

#[test]
fn firmware_vendor_id_constant_correct() {
    assert_eq!(OPENFFBOARD_VENDOR_ID, 0x1209);
}

// ── Proptest encode/decode roundtrip ─────────────────────────────────────────

mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_encode_decode_roundtrip(torque in -1.0f32..=1.0f32) {
            let enc = OpenFFBoardTorqueEncoder;
            let report = enc.encode(torque);
            let raw = i16::from_le_bytes([report[1], report[2]]);
            let decoded = raw as f32 / MAX_TORQUE_SCALE as f32;
            // Allow ±0.0002 tolerance from f32→i16 truncation
            prop_assert!((decoded - torque).abs() < 0.0002,
                "roundtrip error: encoded {torque}, decoded {decoded}");
        }

        #[test]
        fn prop_encode_never_panics(torque in proptest::num::f32::ANY) {
            let enc = OpenFFBoardTorqueEncoder;
            let _ = enc.encode(torque);
        }

        #[test]
        fn prop_encoded_magnitude_within_scale(torque in -10.0f32..10.0f32) {
            let enc = OpenFFBoardTorqueEncoder;
            let report = enc.encode(torque);
            let raw = i16::from_le_bytes([report[1], report[2]]);
            prop_assert!(raw >= -MAX_TORQUE_SCALE);
            prop_assert!(raw <= MAX_TORQUE_SCALE);
        }

        #[test]
        fn prop_clamping_preserves_sign(torque in -10.0f32..10.0f32) {
            let enc = OpenFFBoardTorqueEncoder;
            let report = enc.encode(torque);
            let raw = i16::from_le_bytes([report[1], report[2]]);
            if torque > 0.0001 {
                prop_assert!(raw > 0, "positive torque {torque} yielded non-positive raw {raw}");
            } else if torque < -0.0001 {
                prop_assert!(raw < 0, "negative torque {torque} yielded non-negative raw {raw}");
            }
        }

        #[test]
        fn prop_report_always_correct_length(torque in -2.0f32..2.0f32) {
            let enc = OpenFFBoardTorqueEncoder;
            let report = enc.encode(torque);
            prop_assert_eq!(report.len(), CONSTANT_FORCE_REPORT_LEN);
            prop_assert_eq!(report[0], CONSTANT_FORCE_REPORT_ID);
        }

        #[test]
        fn prop_gain_report_roundtrip(gain in 0u8..=255u8) {
            let report = build_set_gain(gain);
            prop_assert_eq!(report[0], GAIN_REPORT_ID);
            prop_assert_eq!(report[1], gain);
            prop_assert_eq!(report[2], 0x00);
        }

        #[test]
        fn prop_enable_ffb_idempotent(enable in proptest::bool::ANY) {
            let r1 = build_enable_ffb(enable);
            let r2 = build_enable_ffb(enable);
            prop_assert_eq!(r1, r2);
        }
    }
}

// ── Vendor command device identification ─────────────────────────────────────

use racing_wheel_hid_openffboard_protocol::commands::{class_ids, system_cmds, INSTANCE_BROADCAST};
use racing_wheel_hid_openffboard_protocol::{
    build_request, build_request_device_id, build_request_fw_version, build_request_hw_type,
    build_reset_device, build_save_config, build_write, CmdType, VendorCommand,
    VENDOR_CMD_REPORT_ID, VENDOR_CMD_REPORT_LEN,
};

#[test]
fn vendor_cmd_fw_version_roundtrip() -> Result<(), String> {
    let buf = build_request_fw_version();
    let parsed = VendorCommand::parse(&buf).ok_or("parse failed")?;
    if parsed.cmd_type != CmdType::Request {
        return Err(format!("expected Request, got {:?}", parsed.cmd_type));
    }
    if parsed.class_id != class_ids::SYSTEM {
        return Err(format!("expected SYSTEM class, got {}", parsed.class_id));
    }
    if parsed.command != system_cmds::FW_VERSION {
        return Err(format!("expected FW_VERSION cmd, got {}", parsed.command));
    }
    Ok(())
}

#[test]
fn vendor_cmd_hw_type_roundtrip() -> Result<(), String> {
    let buf = build_request_hw_type();
    let parsed = VendorCommand::parse(&buf).ok_or("parse failed")?;
    if parsed.command != system_cmds::HW_TYPE {
        return Err(format!("expected HW_TYPE cmd, got {}", parsed.command));
    }
    Ok(())
}

#[test]
fn vendor_cmd_device_id_roundtrip() -> Result<(), String> {
    let buf = build_request_device_id();
    let parsed = VendorCommand::parse(&buf).ok_or("parse failed")?;
    if parsed.command != system_cmds::DEVICE_ID {
        return Err(format!("expected DEVICE_ID cmd, got {}", parsed.command));
    }
    Ok(())
}

#[test]
fn vendor_cmd_reset_is_write() -> Result<(), String> {
    let buf = build_reset_device();
    let parsed = VendorCommand::parse(&buf).ok_or("parse failed")?;
    if parsed.cmd_type != CmdType::Write {
        return Err(format!("expected Write, got {:?}", parsed.cmd_type));
    }
    if parsed.command != system_cmds::RESET {
        return Err(format!("expected RESET cmd, got {}", parsed.command));
    }
    Ok(())
}

#[test]
fn vendor_cmd_save_config_is_write() -> Result<(), String> {
    let buf = build_save_config();
    let parsed = VendorCommand::parse(&buf).ok_or("parse failed")?;
    if parsed.cmd_type != CmdType::Write {
        return Err(format!("expected Write, got {:?}", parsed.cmd_type));
    }
    if parsed.command != system_cmds::SAVE {
        return Err(format!("expected SAVE cmd, got {}", parsed.command));
    }
    Ok(())
}

#[test]
fn vendor_cmd_generic_write_roundtrip() -> Result<(), String> {
    let buf = build_write(class_ids::FFB_AXIS, 2, 0x42, 0xDEAD);
    let parsed = VendorCommand::parse(&buf).ok_or("parse failed")?;
    if parsed.cmd_type != CmdType::Write {
        return Err(format!("expected Write, got {:?}", parsed.cmd_type));
    }
    if parsed.class_id != class_ids::FFB_AXIS {
        return Err(format!("expected FFB_AXIS class, got {}", parsed.class_id));
    }
    if parsed.instance != 2 {
        return Err(format!("expected instance 2, got {}", parsed.instance));
    }
    if parsed.command != 0x42 {
        return Err(format!("expected command 0x42, got {}", parsed.command));
    }
    if parsed.data != 0xDEAD {
        return Err(format!("expected data 0xDEAD, got {}", parsed.data));
    }
    Ok(())
}

#[test]
fn vendor_cmd_generic_request_roundtrip() -> Result<(), String> {
    let buf = build_request(class_ids::BUTTON_SOURCE, 0, 0x99);
    let parsed = VendorCommand::parse(&buf).ok_or("parse failed")?;
    if parsed.cmd_type != CmdType::Request {
        return Err(format!("expected Request, got {:?}", parsed.cmd_type));
    }
    if parsed.class_id != class_ids::BUTTON_SOURCE {
        return Err("wrong class_id".into());
    }
    if parsed.command != 0x99 {
        return Err("wrong command".into());
    }
    Ok(())
}

#[test]
fn vendor_cmd_report_always_correct_length() -> Result<(), String> {
    let builders: Vec<[u8; VENDOR_CMD_REPORT_LEN]> = vec![
        build_request_fw_version(),
        build_request_hw_type(),
        build_request_device_id(),
        build_reset_device(),
        build_save_config(),
        build_write(0, 0, 0, 0),
        build_request(0, 0, 0),
    ];
    for (i, buf) in builders.iter().enumerate() {
        if buf.len() != VENDOR_CMD_REPORT_LEN {
            return Err(format!("builder {i} produced len {}", buf.len()));
        }
        if buf[0] != VENDOR_CMD_REPORT_ID {
            return Err(format!("builder {i} has wrong report ID {:#x}", buf[0]));
        }
    }
    Ok(())
}

#[test]
fn vendor_cmd_parse_rejects_short_buffer() -> Result<(), String> {
    let short = [VENDOR_CMD_REPORT_ID; 10];
    if VendorCommand::parse(&short).is_some() {
        return Err("should reject short buffer".into());
    }
    Ok(())
}

#[test]
fn vendor_cmd_parse_rejects_wrong_report_id() -> Result<(), String> {
    let mut buf = [0u8; VENDOR_CMD_REPORT_LEN];
    buf[0] = 0x42;
    buf[1] = CmdType::Write as u8;
    if VendorCommand::parse(&buf).is_some() {
        return Err("should reject wrong report ID".into());
    }
    Ok(())
}

#[test]
fn vendor_cmd_parse_rejects_unknown_cmd_type() -> Result<(), String> {
    let mut buf = [0u8; VENDOR_CMD_REPORT_LEN];
    buf[0] = VENDOR_CMD_REPORT_ID;
    buf[1] = 0x09; // not a valid CmdType
    if VendorCommand::parse(&buf).is_some() {
        return Err("should reject unknown CmdType".into());
    }
    Ok(())
}

#[test]
fn vendor_cmd_broadcast_instance_roundtrip() -> Result<(), String> {
    let cmd = VendorCommand {
        cmd_type: CmdType::Request,
        class_id: class_ids::ANALOG_AXIS,
        instance: INSTANCE_BROADCAST,
        command: 0x10,
        data: 0,
        addr: 0,
    };
    let buf = cmd.encode();
    let parsed = VendorCommand::parse(&buf).ok_or("parse failed")?;
    if parsed.instance != INSTANCE_BROADCAST {
        return Err(format!("expected broadcast 0xFF, got {}", parsed.instance));
    }
    Ok(())
}

#[test]
fn vendor_cmd_full_payload_roundtrip() -> Result<(), String> {
    let cmd = VendorCommand {
        cmd_type: CmdType::Ack,
        class_id: 0xABCD,
        instance: 7,
        command: 0xDEADBEEF,
        data: 0x0102030405060708,
        addr: 0xAABBCCDDEEFF0011,
    };
    let buf = cmd.encode();
    let parsed = VendorCommand::parse(&buf).ok_or("parse failed")?;
    if parsed != cmd {
        return Err("full roundtrip mismatch".into());
    }
    Ok(())
}

// ── Input report: axis reports ───────────────────────────────────────────────

use racing_wheel_hid_openffboard_protocol::input::{
    OpenFFBoardInputReport, AXIS_MAX, BUTTON_BYTES, INPUT_REPORT_ID, INPUT_REPORT_LEN, MAX_BUTTONS,
    NUM_AXES,
};

fn make_input_report() -> [u8; INPUT_REPORT_LEN] {
    let mut r = [0u8; INPUT_REPORT_LEN];
    r[0] = INPUT_REPORT_ID;
    r
}

#[test]
fn input_report_parse_all_zeros() -> Result<(), String> {
    let r = make_input_report();
    let parsed = OpenFFBoardInputReport::parse(&r).ok_or("parse failed")?;
    for (i, &axis) in parsed.axes.iter().enumerate() {
        if axis != 0 {
            return Err(format!("axis {i} should be 0, got {axis}"));
        }
    }
    if parsed.buttons_pressed() != 0 {
        return Err("expected 0 buttons pressed".into());
    }
    Ok(())
}

#[test]
fn input_report_all_axes_set() -> Result<(), String> {
    let mut r = make_input_report();
    let values: [i16; NUM_AXES] = [1000, -2000, 3000, -4000, 5000, -6000, 7000, -8000];
    for (i, &v) in values.iter().enumerate() {
        let bytes = v.to_le_bytes();
        r[9 + i * 2] = bytes[0];
        r[9 + i * 2 + 1] = bytes[1];
    }
    let parsed = OpenFFBoardInputReport::parse(&r).ok_or("parse failed")?;
    if parsed.axes != values {
        return Err(format!("axes mismatch: {:?} != {:?}", parsed.axes, values));
    }
    if parsed.x() != 1000 {
        return Err("x() wrong".into());
    }
    if parsed.y() != -2000 {
        return Err("y() wrong".into());
    }
    if parsed.z() != 3000 {
        return Err("z() wrong".into());
    }
    if parsed.rx() != -4000 {
        return Err("rx() wrong".into());
    }
    if parsed.ry() != 5000 {
        return Err("ry() wrong".into());
    }
    if parsed.rz() != -6000 {
        return Err("rz() wrong".into());
    }
    if parsed.dial() != 7000 {
        return Err("dial() wrong".into());
    }
    if parsed.slider() != -8000 {
        return Err("slider() wrong".into());
    }
    Ok(())
}

#[test]
fn input_report_max_positive_axes() -> Result<(), String> {
    let mut r = make_input_report();
    for i in 0..NUM_AXES {
        let bytes = AXIS_MAX.to_le_bytes();
        r[9 + i * 2] = bytes[0];
        r[9 + i * 2 + 1] = bytes[1];
    }
    let parsed = OpenFFBoardInputReport::parse(&r).ok_or("parse failed")?;
    for (i, &axis) in parsed.axes.iter().enumerate() {
        if axis != AXIS_MAX {
            return Err(format!("axis {i} should be {AXIS_MAX}, got {axis}"));
        }
    }
    Ok(())
}

#[test]
fn input_report_max_negative_axes() -> Result<(), String> {
    let mut r = make_input_report();
    let min_val: i16 = -AXIS_MAX;
    for i in 0..NUM_AXES {
        let bytes = min_val.to_le_bytes();
        r[9 + i * 2] = bytes[0];
        r[9 + i * 2 + 1] = bytes[1];
    }
    let parsed = OpenFFBoardInputReport::parse(&r).ok_or("parse failed")?;
    for (i, &axis) in parsed.axes.iter().enumerate() {
        if axis != min_val {
            return Err(format!("axis {i} should be {min_val}, got {axis}"));
        }
    }
    Ok(())
}

#[test]
fn input_report_steering_normalized_boundaries() -> Result<(), String> {
    let mut r = make_input_report();
    // Full positive
    let bytes = AXIS_MAX.to_le_bytes();
    r[9] = bytes[0];
    r[10] = bytes[1];
    let parsed = OpenFFBoardInputReport::parse(&r).ok_or("parse failed")?;
    if (parsed.steering_normalized() - 1.0).abs() > 0.001 {
        return Err(format!(
            "full pos normalized: {}",
            parsed.steering_normalized()
        ));
    }
    // Full negative
    let bytes = (-AXIS_MAX).to_le_bytes();
    r[9] = bytes[0];
    r[10] = bytes[1];
    let parsed = OpenFFBoardInputReport::parse(&r).ok_or("parse failed")?;
    if (parsed.steering_normalized() + 1.0).abs() > 0.001 {
        return Err(format!(
            "full neg normalized: {}",
            parsed.steering_normalized()
        ));
    }
    // Center
    r[9] = 0;
    r[10] = 0;
    let parsed = OpenFFBoardInputReport::parse(&r).ok_or("parse failed")?;
    if parsed.steering_normalized().abs() > 0.001 {
        return Err(format!(
            "center normalized: {}",
            parsed.steering_normalized()
        ));
    }
    Ok(())
}

#[test]
fn input_report_rejects_short_buffer() -> Result<(), String> {
    let short = [INPUT_REPORT_ID; 10];
    if OpenFFBoardInputReport::parse(&short).is_some() {
        return Err("should reject short buffer".into());
    }
    Ok(())
}

#[test]
fn input_report_rejects_wrong_report_id() -> Result<(), String> {
    let mut r = make_input_report();
    r[0] = 0xFF;
    if OpenFFBoardInputReport::parse(&r).is_some() {
        return Err("should reject wrong report ID".into());
    }
    Ok(())
}

#[test]
fn input_report_accepts_longer_buffer() -> Result<(), String> {
    let mut r = [0u8; 30];
    r[0] = INPUT_REPORT_ID;
    if OpenFFBoardInputReport::parse(&r).is_none() {
        return Err("should accept longer buffer".into());
    }
    Ok(())
}

// ── Button matrix ────────────────────────────────────────────────────────────

#[test]
fn button_matrix_individual_buttons() -> Result<(), String> {
    for btn in 0..MAX_BUTTONS {
        let mut r = make_input_report();
        let byte_idx = btn / 8;
        let bit_idx = btn % 8;
        r[1 + byte_idx] = 1 << bit_idx;
        let parsed = OpenFFBoardInputReport::parse(&r).ok_or("parse failed")?;
        if !parsed.button(btn) {
            return Err(format!("button {btn} should be pressed"));
        }
        if parsed.buttons_pressed() != 1 {
            return Err(format!(
                "expected 1 button, got {}",
                parsed.buttons_pressed()
            ));
        }
        // All other buttons should be unpressed
        for other in 0..MAX_BUTTONS {
            if other != btn && parsed.button(other) {
                return Err(format!(
                    "button {other} should not be pressed when testing {btn}"
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn button_matrix_all_pressed() -> Result<(), String> {
    let mut r = make_input_report();
    for byte in &mut r[1..1 + BUTTON_BYTES] {
        *byte = 0xFF;
    }
    let parsed = OpenFFBoardInputReport::parse(&r).ok_or("parse failed")?;
    if parsed.buttons_pressed() != 64 {
        return Err(format!(
            "expected 64 buttons, got {}",
            parsed.buttons_pressed()
        ));
    }
    for btn in 0..MAX_BUTTONS {
        if !parsed.button(btn) {
            return Err(format!("button {btn} should be pressed"));
        }
    }
    Ok(())
}

#[test]
fn button_matrix_out_of_range_returns_false() -> Result<(), String> {
    let r = make_input_report();
    let parsed = OpenFFBoardInputReport::parse(&r).ok_or("parse failed")?;
    if parsed.button(64) {
        return Err("button(64) should be false".into());
    }
    if parsed.button(100) {
        return Err("button(100) should be false".into());
    }
    if parsed.button(usize::MAX) {
        return Err("button(MAX) should be false".into());
    }
    Ok(())
}

#[test]
fn button_matrix_alternating_pattern() -> Result<(), String> {
    let mut r = make_input_report();
    // Set even bytes to 0xAA (bits: 10101010) and odd bytes to 0x55 (bits: 01010101)
    for i in 0..BUTTON_BYTES {
        r[1 + i] = if i % 2 == 0 { 0xAA } else { 0x55 };
    }
    let parsed = OpenFFBoardInputReport::parse(&r).ok_or("parse failed")?;
    // 0xAA = 4 bits set, 0x55 = 4 bits set, 4 bytes each = 32 total
    if parsed.buttons_pressed() != 32 {
        return Err(format!("expected 32, got {}", parsed.buttons_pressed()));
    }
    Ok(())
}

// ── Force feedback command encoding (PIDFF re-exports) ───────────────────────

use racing_wheel_hid_openffboard_protocol::effects::report_ids;
use racing_wheel_hid_openffboard_protocol::effects::{
    encode_block_free, encode_device_control, encode_device_gain, encode_effect_operation,
    encode_set_condition, encode_set_constant_force, encode_set_effect, encode_set_envelope,
    encode_set_periodic, encode_set_ramp_force, parse_block_load, BlockLoadStatus, EffectOp,
    EffectType, DURATION_INFINITE, MAX_EFFECTS,
};

#[test]
fn ffb_set_effect_constant_infinite() -> Result<(), String> {
    let buf = encode_set_effect(1, EffectType::Constant, DURATION_INFINITE, 255, 0);
    if buf[0] != report_ids::SET_EFFECT {
        return Err(format!("wrong report ID: {:#x}", buf[0]));
    }
    if buf[1] != 1 {
        return Err("wrong block index".into());
    }
    if buf[2] != EffectType::Constant as u8 {
        return Err("wrong effect type".into());
    }
    let duration = u16::from_le_bytes([buf[3], buf[4]]);
    if duration != DURATION_INFINITE {
        return Err(format!("wrong duration: {duration}"));
    }
    if buf[9] != 255 {
        return Err("wrong gain".into());
    }
    Ok(())
}

#[test]
fn ffb_set_effect_direction_encoding() -> Result<(), String> {
    let buf = encode_set_effect(2, EffectType::Sine, 1000, 128, 18000);
    let direction = u16::from_le_bytes([buf[11], buf[12]]);
    if direction != 18000 {
        return Err(format!("expected direction 18000, got {direction}"));
    }
    if buf[2] != EffectType::Sine as u8 {
        return Err("wrong effect type".into());
    }
    Ok(())
}

#[test]
fn ffb_constant_force_signed_magnitude() -> Result<(), String> {
    for mag in [0i16, 10000, -10000, 1, -1, i16::MAX, i16::MIN] {
        let buf = encode_set_constant_force(1, mag);
        if buf[0] != report_ids::SET_CONSTANT_FORCE {
            return Err(format!("wrong report ID for mag {mag}"));
        }
        let decoded = i16::from_le_bytes([buf[2], buf[3]]);
        if decoded != mag {
            return Err(format!("magnitude mismatch: sent {mag}, got {decoded}"));
        }
    }
    Ok(())
}

#[test]
fn ffb_ramp_force_start_end() -> Result<(), String> {
    let buf = encode_set_ramp_force(3, -5000, 5000);
    let start = i16::from_le_bytes([buf[2], buf[3]]);
    let end = i16::from_le_bytes([buf[4], buf[5]]);
    if start != -5000 {
        return Err(format!("wrong start: {start}"));
    }
    if end != 5000 {
        return Err(format!("wrong end: {end}"));
    }
    Ok(())
}

#[test]
fn ffb_periodic_sine_params() -> Result<(), String> {
    let buf = encode_set_periodic(1, 7500, -2000, 9000, 250);
    if buf[0] != report_ids::SET_PERIODIC {
        return Err("wrong report ID".into());
    }
    let mag = u16::from_le_bytes([buf[2], buf[3]]);
    let offset = i16::from_le_bytes([buf[4], buf[5]]);
    let phase = u16::from_le_bytes([buf[6], buf[7]]);
    let period = u16::from_le_bytes([buf[8], buf[9]]);
    if mag != 7500 {
        return Err(format!("wrong magnitude: {mag}"));
    }
    if offset != -2000 {
        return Err(format!("wrong offset: {offset}"));
    }
    if phase != 9000 {
        return Err(format!("wrong phase: {phase}"));
    }
    if period != 250 {
        return Err(format!("wrong period: {period}"));
    }
    Ok(())
}

#[test]
fn ffb_condition_spring_params() -> Result<(), String> {
    let buf = encode_set_condition(1, 0, -500, 3000, -2000, 10000, 8000, 50);
    if buf[0] != report_ids::SET_CONDITION {
        return Err("wrong report ID".into());
    }
    let center = i16::from_le_bytes([buf[3], buf[4]]);
    let pos_coeff = i16::from_le_bytes([buf[5], buf[6]]);
    let neg_coeff = i16::from_le_bytes([buf[7], buf[8]]);
    let pos_sat = u16::from_le_bytes([buf[9], buf[10]]);
    let neg_sat = u16::from_le_bytes([buf[11], buf[12]]);
    if center != -500 {
        return Err(format!("wrong center: {center}"));
    }
    if pos_coeff != 3000 {
        return Err(format!("wrong pos_coeff: {pos_coeff}"));
    }
    if neg_coeff != -2000 {
        return Err(format!("wrong neg_coeff: {neg_coeff}"));
    }
    if pos_sat != 10000 {
        return Err(format!("wrong pos_sat: {pos_sat}"));
    }
    if neg_sat != 8000 {
        return Err(format!("wrong neg_sat: {neg_sat}"));
    }
    if buf[13] != 50 {
        return Err(format!("wrong dead_band: {}", buf[13]));
    }
    Ok(())
}

#[test]
fn ffb_envelope_timing() -> Result<(), String> {
    let buf = encode_set_envelope(1, 5000, 8000, 100, 200);
    if buf[0] != report_ids::SET_ENVELOPE {
        return Err("wrong report ID".into());
    }
    let attack = u16::from_le_bytes([buf[2], buf[3]]);
    let fade = u16::from_le_bytes([buf[4], buf[5]]);
    let at_ms = u16::from_le_bytes([buf[6], buf[7]]);
    let ft_ms = u16::from_le_bytes([buf[8], buf[9]]);
    if attack != 5000 {
        return Err(format!("wrong attack: {attack}"));
    }
    if fade != 8000 {
        return Err(format!("wrong fade: {fade}"));
    }
    if at_ms != 100 {
        return Err(format!("wrong attack time: {at_ms}"));
    }
    if ft_ms != 200 {
        return Err(format!("wrong fade time: {ft_ms}"));
    }
    Ok(())
}

#[test]
fn ffb_effect_operation_all_ops() -> Result<(), String> {
    let ops = [
        (EffectOp::Start, 1u8),
        (EffectOp::StartSolo, 2),
        (EffectOp::Stop, 3),
    ];
    for (op, expected_byte) in ops {
        let buf = encode_effect_operation(1, op, 0);
        if buf[0] != report_ids::EFFECT_OPERATION {
            return Err(format!("wrong report ID for {:?}", op));
        }
        if buf[2] != expected_byte {
            return Err(format!("wrong op byte for {:?}: got {}", op, buf[2]));
        }
    }
    Ok(())
}

#[test]
fn ffb_block_free_and_device_control() -> Result<(), String> {
    let buf = encode_block_free(5);
    if buf != [report_ids::BLOCK_FREE, 5] {
        return Err(format!("block_free wrong: {:?}", buf));
    }
    let buf = encode_device_control(0x01);
    if buf != [report_ids::DEVICE_CONTROL, 0x01] {
        return Err(format!("device_control wrong: {:?}", buf));
    }
    Ok(())
}

#[test]
fn ffb_device_gain_clamps_at_10000() -> Result<(), String> {
    let buf = encode_device_gain(20000);
    let gain = u16::from_le_bytes([buf[2], buf[3]]);
    if gain != 10000 {
        return Err(format!("expected clamped to 10000, got {gain}"));
    }
    let buf = encode_device_gain(5000);
    let gain = u16::from_le_bytes([buf[2], buf[3]]);
    if gain != 5000 {
        return Err(format!("expected 5000, got {gain}"));
    }
    Ok(())
}

#[test]
fn ffb_block_load_parse_success() -> Result<(), String> {
    let buf = [0x12, 3, 1, 0x00, 0x10];
    let report = parse_block_load(&buf).ok_or("parse failed")?;
    if report.block_index != 3 {
        return Err("wrong block_index".into());
    }
    if report.status != BlockLoadStatus::Success {
        return Err("wrong status".into());
    }
    if report.ram_pool_available != 0x1000 {
        return Err("wrong ram_pool".into());
    }
    Ok(())
}

#[test]
fn ffb_block_load_parse_full_and_error() -> Result<(), String> {
    let full = [0x12, 0, 2, 0, 0];
    let r = parse_block_load(&full).ok_or("parse failed for Full")?;
    if r.status != BlockLoadStatus::Full {
        return Err("expected Full".into());
    }
    let err = [0x12, 0, 3, 0, 0];
    let r = parse_block_load(&err).ok_or("parse failed for Error")?;
    if r.status != BlockLoadStatus::Error {
        return Err("expected Error".into());
    }
    Ok(())
}

#[test]
fn ffb_block_load_rejects_invalid() -> Result<(), String> {
    if parse_block_load(&[0x12, 0, 1]).is_some() {
        return Err("should reject short buffer".into());
    }
    if parse_block_load(&[0x13, 0, 1, 0, 0]).is_some() {
        return Err("should reject wrong report ID".into());
    }
    if parse_block_load(&[0x12, 0, 0, 0, 0]).is_some() {
        return Err("should reject invalid status 0".into());
    }
    Ok(())
}

#[test]
fn ffb_max_effects_matches_firmware() -> Result<(), String> {
    if MAX_EFFECTS != 40 {
        return Err(format!("expected 40, got {MAX_EFFECTS}"));
    }
    Ok(())
}

// ── Additional proptests (vendor commands, input reports) ────────────────────

mod proptests_extended {
    use super::*;
    use proptest::prelude::*;

    fn arb_cmd_type() -> impl Strategy<Value = CmdType> {
        prop_oneof![
            Just(CmdType::Write),
            Just(CmdType::Request),
            Just(CmdType::Info),
            Just(CmdType::WriteAddr),
            Just(CmdType::RequestAddr),
            Just(CmdType::Ack),
            Just(CmdType::NotFound),
            Just(CmdType::Notification),
            Just(CmdType::Error),
        ]
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_vendor_cmd_roundtrip(
            cmd_type in arb_cmd_type(),
            class_id in 0u16..=u16::MAX,
            instance in 0u8..=255u8,
            command in 0u32..=u32::MAX,
            data in 0u64..=u64::MAX,
            addr in 0u64..=u64::MAX,
        ) {
            let cmd = VendorCommand { cmd_type, class_id, instance, command, data, addr };
            let buf = cmd.encode();
            prop_assert_eq!(buf.len(), VENDOR_CMD_REPORT_LEN);
            prop_assert_eq!(buf[0], VENDOR_CMD_REPORT_ID);
            let parsed = VendorCommand::parse(&buf);
            prop_assert_eq!(parsed, Some(cmd));
        }

        #[test]
        fn prop_input_report_axes_roundtrip(
            axes in proptest::array::uniform8(-32767i16..=32767i16),
        ) {
            let mut r = [0u8; INPUT_REPORT_LEN];
            r[0] = INPUT_REPORT_ID;
            for (i, &v) in axes.iter().enumerate() {
                let bytes = v.to_le_bytes();
                r[9 + i * 2] = bytes[0];
                r[9 + i * 2 + 1] = bytes[1];
            }
            let parsed = OpenFFBoardInputReport::parse(&r);
            prop_assert!(parsed.is_some());
            if let Some(p) = parsed {
                prop_assert_eq!(p.axes, axes);
            }
        }

        #[test]
        fn prop_input_report_buttons_count(buttons in proptest::array::uniform8(0u8..=255u8)) {
            let mut r = [0u8; INPUT_REPORT_LEN];
            r[0] = INPUT_REPORT_ID;
            for (i, &b) in buttons.iter().enumerate() {
                r[1 + i] = b;
            }
            let parsed = OpenFFBoardInputReport::parse(&r);
            prop_assert!(parsed.is_some());
            if let Some(p) = parsed {
                let expected: u32 = buttons.iter().map(|b| b.count_ones()).sum();
                prop_assert_eq!(p.buttons_pressed(), expected);
            }
        }

        #[test]
        fn prop_input_report_button_individual(
            btn_idx in 0usize..64usize,
        ) {
            let mut r = [0u8; INPUT_REPORT_LEN];
            r[0] = INPUT_REPORT_ID;
            let byte_idx = btn_idx / 8;
            let bit_idx = btn_idx % 8;
            r[1 + byte_idx] = 1 << bit_idx;
            let parsed = OpenFFBoardInputReport::parse(&r);
            prop_assert!(parsed.is_some());
            if let Some(p) = parsed {
                prop_assert!(p.button(btn_idx));
                prop_assert_eq!(p.buttons_pressed(), 1);
            }
        }

        #[test]
        fn prop_cmd_type_from_byte_unknown(b in 0u8..=255u8) {
            let known = [0, 1, 2, 3, 4, 10, 13, 14, 15];
            let result = CmdType::from_byte(b);
            if known.contains(&b) {
                prop_assert!(result.is_some());
            } else {
                prop_assert!(result.is_none());
            }
        }
    }
}
