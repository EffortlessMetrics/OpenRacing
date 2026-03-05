//! Deep integration tests for Simucube HID protocol.
//!
//! Covers PIDFF effect encoding/decoding roundtrips, block load parsing,
//! device control commands, output report byte-level wire encoding,
//! extended input report diagnostics, HID joystick report fuzzing,
//! and cross-module consistency checks.

use hid_simucube_protocol::{
    // PIDFF re-exports
    BlockLoadStatus,
    DURATION_INFINITE,
    EffectOp,
    EffectType as SimuEffectType,
    HID_ADDITIONAL_AXES,
    HID_BUTTON_BYTES,
    HID_BUTTON_COUNT,
    HID_JOYSTICK_REPORT_MIN_BYTES,
    MAX_EFFECTS,
    MAX_TORQUE_PRO,
    MAX_TORQUE_SPORT,
    MAX_TORQUE_ULTIMATE,
    PidEffectType,
    REPORT_SIZE_INPUT,
    REPORT_SIZE_OUTPUT,
    SIMUCUBE_1_BOOTLOADER_PID,
    SIMUCUBE_1_PID,
    SIMUCUBE_2_BOOTLOADER_PID,
    SIMUCUBE_2_PRO_PID,
    SIMUCUBE_2_SPORT_PID,
    SIMUCUBE_2_ULTIMATE_PID,
    SIMUCUBE_ACTIVE_PEDAL_PID,
    SIMUCUBE_VENDOR_ID,
    SIMUCUBE_WIRELESS_WHEEL_PID,
    SimucubeError,
    SimucubeHidReport,
    SimucubeInputReport,
    SimucubeModel,
    SimucubeOutputReport,
    VENDOR_ID,
    WheelCapabilities,
    WheelModel,
    encode_block_free,
    encode_create_new_effect,
    encode_device_control,
    encode_device_gain,
    encode_effect_operation,
    encode_set_condition,
    encode_set_constant_force,
    encode_set_effect,
    encode_set_envelope,
    encode_set_periodic,
    encode_set_ramp_force,
    parse_block_load,
    simucube_model_from_info,
};

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn make_hid_report(steering: u16, y: u16, axes: [u16; 6], buttons: [u8; 16]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(HID_JOYSTICK_REPORT_MIN_BYTES);
    buf.extend_from_slice(&steering.to_le_bytes());
    buf.extend_from_slice(&y.to_le_bytes());
    for ax in &axes {
        buf.extend_from_slice(&ax.to_le_bytes());
    }
    buf.extend_from_slice(&buttons);
    buf
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. PIDFF effect encoding roundtrips
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn pidff_set_effect_encodes_all_fields() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_set_effect(3, PidEffectType::Sine, DURATION_INFINITE, 200, 0x1234);
    assert_eq!(buf[0], 0x01, "report ID = SET_EFFECT");
    assert_eq!(buf[1], 3, "block index");
    assert_eq!(buf[2], PidEffectType::Sine as u8, "effect type");
    let dur = u16::from_le_bytes([buf[3], buf[4]]);
    assert_eq!(dur, DURATION_INFINITE);
    assert_eq!(buf[9], 200, "gain");
    let dir = u16::from_le_bytes([buf[11], buf[12]]);
    assert_eq!(dir, 0x1234, "direction");
    Ok(())
}

#[test]
fn pidff_set_constant_force_magnitude_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    for mag in [i16::MIN, -10000, -1, 0, 1, 10000, i16::MAX] {
        let buf = encode_set_constant_force(1, mag);
        assert_eq!(buf[0], 0x05, "report ID = SET_CONSTANT_FORCE");
        assert_eq!(buf[1], 1, "block index");
        let decoded = i16::from_le_bytes([buf[2], buf[3]]);
        assert_eq!(decoded, mag, "magnitude roundtrip for {mag}");
    }
    Ok(())
}

#[test]
fn pidff_set_periodic_fields_preserved() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_set_periodic(2, 5000, -3000, 9000, 50);
    assert_eq!(buf[0], 0x04, "report ID = SET_PERIODIC");
    assert_eq!(buf[1], 2, "block index");
    assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 5000, "magnitude");
    assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), -3000, "offset");
    assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), 9000, "phase");
    assert_eq!(u16::from_le_bytes([buf[8], buf[9]]), 50, "period");
    Ok(())
}

#[test]
fn pidff_set_envelope_fields_preserved() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_set_envelope(1, 8000, 2000, 100, 500);
    assert_eq!(buf[0], 0x02, "report ID = SET_ENVELOPE");
    assert_eq!(buf[1], 1);
    assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 8000, "attack level");
    assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), 2000, "fade level");
    assert_eq!(u16::from_le_bytes([buf[6], buf[7]]), 100, "attack time");
    assert_eq!(u16::from_le_bytes([buf[8], buf[9]]), 500, "fade time");
    Ok(())
}

#[test]
fn pidff_set_condition_fields_preserved() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_set_condition(4, 0, 1000, 2000, -2000, 5000, 5000, 10);
    assert_eq!(buf[0], 0x03, "report ID = SET_CONDITION");
    assert_eq!(buf[1], 4, "block index");
    assert_eq!(buf[2], 0, "axis");
    assert_eq!(i16::from_le_bytes([buf[3], buf[4]]), 1000, "center point");
    assert_eq!(i16::from_le_bytes([buf[5], buf[6]]), 2000, "positive coeff");
    assert_eq!(
        i16::from_le_bytes([buf[7], buf[8]]),
        -2000,
        "negative coeff"
    );
    assert_eq!(buf[13], 10, "dead band");
    Ok(())
}

#[test]
fn pidff_set_ramp_force_start_end() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_set_ramp_force(1, -5000, 5000);
    assert_eq!(buf[0], 0x06, "report ID = SET_RAMP_FORCE");
    assert_eq!(buf[1], 1, "block index");
    assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), -5000, "start");
    assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), 5000, "end");
    Ok(())
}

#[test]
fn pidff_effect_operation_start_solo() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_effect_operation(7, EffectOp::StartSolo, 0);
    assert_eq!(buf[0], 0x0A, "report ID = EFFECT_OPERATION");
    assert_eq!(buf[1], 7, "block index");
    assert_eq!(buf[2], EffectOp::StartSolo as u8);
    assert_eq!(buf[3], 0, "loop count");
    Ok(())
}

#[test]
fn pidff_effect_operation_stop() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_effect_operation(1, EffectOp::Stop, 255);
    assert_eq!(buf[2], EffectOp::Stop as u8);
    assert_eq!(buf[3], 255);
    Ok(())
}

#[test]
fn pidff_block_free_encoding() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_block_free(42);
    assert_eq!(buf[0], 0x0B, "report ID = BLOCK_FREE");
    assert_eq!(buf[1], 42, "block index");
    Ok(())
}

#[test]
fn pidff_device_control_all_commands() -> Result<(), Box<dyn std::error::Error>> {
    let commands: &[(u8, &str)] = &[
        (0x01, "enable actuators"),
        (0x02, "disable actuators"),
        (0x04, "stop all effects"),
        (0x08, "device reset"),
        (0x10, "device pause"),
        (0x20, "device continue"),
    ];
    for &(cmd, label) in commands {
        let buf = encode_device_control(cmd);
        assert_eq!(buf[0], 0x0C, "report ID for {label}");
        assert_eq!(buf[1], cmd, "command byte for {label}");
    }
    Ok(())
}

#[test]
fn pidff_device_gain_clamps_to_10000() -> Result<(), Box<dyn std::error::Error>> {
    let buf_normal = encode_device_gain(7500);
    assert_eq!(
        u16::from_le_bytes([buf_normal[2], buf_normal[3]]),
        7500,
        "normal gain preserved"
    );

    let buf_over = encode_device_gain(20000);
    assert_eq!(
        u16::from_le_bytes([buf_over[2], buf_over[3]]),
        10000,
        "over-range clamped"
    );

    let buf_zero = encode_device_gain(0);
    assert_eq!(
        u16::from_le_bytes([buf_zero[2], buf_zero[3]]),
        0,
        "zero preserved"
    );
    Ok(())
}

#[test]
fn pidff_create_new_effect_all_types() -> Result<(), Box<dyn std::error::Error>> {
    let types = [
        PidEffectType::Constant,
        PidEffectType::Ramp,
        PidEffectType::Square,
        PidEffectType::Sine,
        PidEffectType::Triangle,
        PidEffectType::SawtoothUp,
        PidEffectType::SawtoothDown,
        PidEffectType::Spring,
        PidEffectType::Damper,
        PidEffectType::Inertia,
        PidEffectType::Friction,
    ];
    for et in types {
        let buf = encode_create_new_effect(et);
        assert_eq!(buf[0], 0x11, "report ID = CREATE_NEW_EFFECT");
        assert_eq!(buf[1], et as u8, "effect type byte");
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. Block load parsing
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn block_load_success_parsed() -> Result<(), Box<dyn std::error::Error>> {
    let buf = [
        0x12, // report ID = BLOCK_LOAD
        5,    // block index
        BlockLoadStatus::Success as u8,
        0x00,
        0x10, // ram pool = 4096
    ];
    let report = parse_block_load(&buf).ok_or("expected Some from parse_block_load")?;
    assert_eq!(report.block_index, 5);
    assert_eq!(report.status, BlockLoadStatus::Success);
    assert_eq!(report.ram_pool_available, 0x1000);
    Ok(())
}

#[test]
fn block_load_full_parsed() -> Result<(), Box<dyn std::error::Error>> {
    let buf = [0x12, 0, BlockLoadStatus::Full as u8, 0x00, 0x00];
    let report = parse_block_load(&buf).ok_or("expected Some")?;
    assert_eq!(report.status, BlockLoadStatus::Full);
    assert_eq!(report.ram_pool_available, 0);
    Ok(())
}

#[test]
fn block_load_error_parsed() -> Result<(), Box<dyn std::error::Error>> {
    let buf = [0x12, 1, BlockLoadStatus::Error as u8, 0xFF, 0xFF];
    let report = parse_block_load(&buf).ok_or("expected Some")?;
    assert_eq!(report.status, BlockLoadStatus::Error);
    assert_eq!(report.ram_pool_available, 0xFFFF);
    Ok(())
}

#[test]
fn block_load_too_short_returns_none() {
    assert!(parse_block_load(&[0x12, 0, 1]).is_none());
    assert!(parse_block_load(&[]).is_none());
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. Output report wire-level byte encoding
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn output_report_wire_bytes_correct() -> Result<(), SimucubeError> {
    let report = SimucubeOutputReport::new(0x00FF)
        .with_torque(10.0)
        .with_rgb(0xAA, 0xBB, 0xCC)
        .with_effect(SimuEffectType::Spring, 0x1234);
    let data = report.build()?;

    assert_eq!(data[0], 0x01, "report ID");
    assert_eq!(u16::from_le_bytes([data[1], data[2]]), 0x00FF, "sequence");
    assert_eq!(
        i16::from_le_bytes([data[3], data[4]]),
        1000,
        "torque 10.0 Nm = 1000 cNm"
    );
    assert_eq!(data[5], 0xAA, "LED R");
    assert_eq!(data[6], 0xBB, "LED G");
    assert_eq!(data[7], 0xCC, "LED B");
    assert_eq!(data[8], SimuEffectType::Spring as u8, "effect type");
    assert_eq!(
        u16::from_le_bytes([data[9], data[10]]),
        0x1234,
        "effect param"
    );
    // Remaining bytes should be zero-padded
    for &b in &data[11..] {
        assert_eq!(b, 0, "trailing bytes should be zero");
    }
    Ok(())
}

#[test]
fn output_report_default_builds_cleanly() -> Result<(), SimucubeError> {
    let data = SimucubeOutputReport::default().build()?;
    assert_eq!(data.len(), REPORT_SIZE_OUTPUT);
    assert_eq!(data[0], 0x01);
    // All fields zero except report ID
    assert_eq!(i16::from_le_bytes([data[3], data[4]]), 0);
    Ok(())
}

#[test]
fn output_report_chaining_order_independent() -> Result<(), SimucubeError> {
    let a = SimucubeOutputReport::new(1)
        .with_torque(5.0)
        .with_rgb(10, 20, 30)
        .with_effect(SimuEffectType::Damper, 100);
    let b = SimucubeOutputReport::new(1)
        .with_effect(SimuEffectType::Damper, 100)
        .with_rgb(10, 20, 30)
        .with_torque(5.0);
    assert_eq!(a, b);
    assert_eq!(a.build()?, b.build()?);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. Extended input report diagnostics
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn input_report_angle_full_rotation() -> Result<(), Box<dyn std::error::Error>> {
    let report = SimucubeInputReport {
        wheel_angle_raw: hid_simucube_protocol::ANGLE_SENSOR_MAX,
        ..Default::default()
    };
    let deg = report.wheel_angle_degrees();
    assert!(
        (deg - 360.0).abs() < 0.01,
        "full sensor range = 360°, got {deg}"
    );
    Ok(())
}

#[test]
fn input_report_angle_zero() -> Result<(), Box<dyn std::error::Error>> {
    let report = SimucubeInputReport {
        wheel_angle_raw: 0,
        ..Default::default()
    };
    assert!((report.wheel_angle_degrees()).abs() < 0.001);
    assert!((report.wheel_angle_radians()).abs() < 0.001);
    Ok(())
}

#[test]
fn input_report_applied_torque_sign() -> Result<(), Box<dyn std::error::Error>> {
    let pos = SimucubeInputReport {
        torque_nm: 500,
        ..Default::default()
    };
    assert!((pos.applied_torque_nm() - 5.0).abs() < 0.01);

    let neg = SimucubeInputReport {
        torque_nm: -500,
        ..Default::default()
    };
    assert!((neg.applied_torque_nm() + 5.0).abs() < 0.01);
    Ok(())
}

#[test]
fn input_report_fault_flag_detection() -> Result<(), Box<dyn std::error::Error>> {
    let no_fault = SimucubeInputReport {
        fault_flags: 0,
        ..Default::default()
    };
    assert!(!no_fault.has_fault());

    let faulted = SimucubeInputReport {
        fault_flags: 0x01,
        ..Default::default()
    };
    assert!(faulted.has_fault());

    let multi_fault = SimucubeInputReport {
        fault_flags: 0xFF,
        ..Default::default()
    };
    assert!(multi_fault.has_fault());
    Ok(())
}

#[test]
fn input_report_status_connected_enabled_independent() -> Result<(), Box<dyn std::error::Error>> {
    let connected_only = SimucubeInputReport {
        status_flags: 0x01,
        ..Default::default()
    };
    assert!(connected_only.is_connected());
    assert!(!connected_only.is_enabled());

    let enabled_only = SimucubeInputReport {
        status_flags: 0x02,
        ..Default::default()
    };
    assert!(!enabled_only.is_connected());
    assert!(enabled_only.is_enabled());
    Ok(())
}

#[test]
fn input_report_negative_speed() -> Result<(), Box<dyn std::error::Error>> {
    let report = SimucubeInputReport {
        wheel_speed_rpm: -120,
        ..Default::default()
    };
    let rad_s = report.wheel_speed_rad_s();
    assert!(rad_s < 0.0, "negative RPM should yield negative rad/s");
    assert!((rad_s - (-120.0 * 2.0 * std::f32::consts::PI / 60.0)).abs() < 0.01);
    Ok(())
}

#[test]
fn input_report_parse_exact_16_bytes() -> Result<(), SimucubeError> {
    let data = [0u8; 16];
    let report = SimucubeInputReport::parse(&data)?;
    assert_eq!(report.sequence, 0);
    assert_eq!(report.wheel_angle_raw, 0);
    assert!(!report.has_wireless_wheel());
    Ok(())
}

#[test]
fn input_report_parse_15_bytes_rejected() {
    let data = [0u8; 15];
    assert!(matches!(
        SimucubeInputReport::parse(&data),
        Err(SimucubeError::InvalidReportSize {
            expected: 16,
            actual: 15
        })
    ));
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. HID joystick report — all-axes encoding
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn hid_report_all_axes_max() -> Result<(), SimucubeError> {
    let data = make_hid_report(
        u16::MAX,
        u16::MAX,
        [u16::MAX; HID_ADDITIONAL_AXES],
        [0; HID_BUTTON_BYTES],
    );
    let report = SimucubeHidReport::parse(&data)?;
    assert_eq!(report.steering, u16::MAX);
    assert_eq!(report.y_axis, u16::MAX);
    for i in 0..HID_ADDITIONAL_AXES {
        assert_eq!(report.axes[i], u16::MAX);
        assert!((report.axis_normalized(i) - 1.0).abs() < 0.001);
    }
    Ok(())
}

#[test]
fn hid_report_all_buttons_pressed() -> Result<(), SimucubeError> {
    let data = make_hid_report(0x8000, 0x8000, [0; 6], [0xFF; 16]);
    let report = SimucubeHidReport::parse(&data)?;
    assert_eq!(report.pressed_count(), HID_BUTTON_COUNT as u32);
    for i in 0..HID_BUTTON_COUNT {
        assert!(report.button_pressed(i), "button {i} should be pressed");
    }
    Ok(())
}

#[test]
fn hid_report_no_buttons_pressed() -> Result<(), SimucubeError> {
    let data = make_hid_report(0x8000, 0x8000, [0; 6], [0; 16]);
    let report = SimucubeHidReport::parse(&data)?;
    assert_eq!(report.pressed_count(), 0);
    Ok(())
}

#[test]
fn hid_report_padded_to_64_bytes_accepted() -> Result<(), SimucubeError> {
    let mut data = vec![0u8; REPORT_SIZE_INPUT];
    data[0] = 0x00;
    data[1] = 0x40; // steering = 0x4000
    let report = SimucubeHidReport::parse(&data)?;
    assert_eq!(report.steering, 0x4000);
    Ok(())
}

#[test]
fn hid_report_each_axis_individually() -> Result<(), SimucubeError> {
    for idx in 0..HID_ADDITIONAL_AXES {
        let mut axes = [0u16; HID_ADDITIONAL_AXES];
        axes[idx] = 0x1234;
        let data = make_hid_report(0x8000, 0x8000, axes, [0; 16]);
        let report = SimucubeHidReport::parse(&data)?;
        assert_eq!(report.axes[idx], 0x1234, "axis {idx} mismatch");
        for other in 0..HID_ADDITIONAL_AXES {
            if other != idx {
                assert_eq!(report.axes[other], 0, "axis {other} should be zero");
            }
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. VID/PID cross-consistency
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn vendor_id_aliases_match() {
    assert_eq!(VENDOR_ID, SIMUCUBE_VENDOR_ID);
}

#[test]
fn all_runtime_pids_are_unique() -> Result<(), Box<dyn std::error::Error>> {
    let pids = [
        SIMUCUBE_1_PID,
        SIMUCUBE_2_SPORT_PID,
        SIMUCUBE_2_PRO_PID,
        SIMUCUBE_2_ULTIMATE_PID,
        SIMUCUBE_ACTIVE_PEDAL_PID,
        SIMUCUBE_WIRELESS_WHEEL_PID,
    ];
    for i in 0..pids.len() {
        for j in (i + 1)..pids.len() {
            assert_ne!(pids[i], pids[j], "PIDs at {i} and {j} must differ");
        }
    }
    Ok(())
}

#[test]
fn bootloader_pids_do_not_overlap_runtime() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = [
        SIMUCUBE_1_PID,
        SIMUCUBE_2_SPORT_PID,
        SIMUCUBE_2_PRO_PID,
        SIMUCUBE_2_ULTIMATE_PID,
        SIMUCUBE_ACTIVE_PEDAL_PID,
        SIMUCUBE_WIRELESS_WHEEL_PID,
    ];
    let bootloader = [SIMUCUBE_1_BOOTLOADER_PID, SIMUCUBE_2_BOOTLOADER_PID];
    for bl in bootloader {
        for &rt in &runtime {
            assert_ne!(bl, rt, "bootloader PID 0x{bl:04X} must not match runtime");
        }
    }
    Ok(())
}

#[test]
fn model_display_name_not_empty() -> Result<(), Box<dyn std::error::Error>> {
    let models = [
        SimucubeModel::Simucube1,
        SimucubeModel::Sport,
        SimucubeModel::Pro,
        SimucubeModel::Ultimate,
        SimucubeModel::ActivePedal,
        SimucubeModel::WirelessWheel,
        SimucubeModel::Unknown,
    ];
    for m in models {
        assert!(
            !m.display_name().is_empty(),
            "display name for {m:?} must not be empty"
        );
    }
    Ok(())
}

#[test]
fn model_torque_is_non_negative() -> Result<(), Box<dyn std::error::Error>> {
    let models = [
        SimucubeModel::Simucube1,
        SimucubeModel::Sport,
        SimucubeModel::Pro,
        SimucubeModel::Ultimate,
        SimucubeModel::ActivePedal,
        SimucubeModel::WirelessWheel,
        SimucubeModel::Unknown,
    ];
    for m in models {
        assert!(m.max_torque_nm() >= 0.0, "torque for {m:?} must be >= 0");
    }
    Ok(())
}

#[test]
fn torque_hierarchy_sport_lt_pro_lt_ultimate() {
    const { assert!(MAX_TORQUE_SPORT < MAX_TORQUE_PRO) };
    const { assert!(MAX_TORQUE_PRO < MAX_TORQUE_ULTIMATE) };
}

#[test]
fn max_effects_within_u8_range() {
    const { assert!(MAX_EFFECTS > 0) };
    // MAX_EFFECTS is u8, so it is inherently <= 255.
}

// ═══════════════════════════════════════════════════════════════════════════
// 7. Error formatting
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn error_display_invalid_report_size() {
    let err = SimucubeError::InvalidReportSize {
        expected: 32,
        actual: 10,
    };
    let msg = format!("{err}");
    assert!(msg.contains("32"), "should mention expected size");
    assert!(msg.contains("10"), "should mention actual size");
}

#[test]
fn error_display_invalid_torque() {
    let err = SimucubeError::InvalidTorque(99.9);
    let msg = format!("{err}");
    assert!(msg.contains("99.9"), "should mention value");
}

#[test]
fn error_display_device_not_found() {
    let err = SimucubeError::DeviceNotFound("test".into());
    let msg = format!("{err}");
    assert!(msg.contains("test"));
}

#[test]
fn error_display_communication() {
    let err = SimucubeError::Communication("timeout".into());
    let msg = format!("{err}");
    assert!(msg.contains("timeout"));
}

// ═══════════════════════════════════════════════════════════════════════════
// 8. WheelCapabilities cross-model consistency
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wheel_capabilities_encoder_bits_consistent() -> Result<(), Box<dyn std::error::Error>> {
    let wheelbase_models = [
        WheelModel::Simucube2Sport,
        WheelModel::Simucube2Pro,
        WheelModel::Simucube2Ultimate,
    ];
    for m in wheelbase_models {
        let caps = WheelCapabilities::for_model(m);
        assert_eq!(
            caps.encoder_resolution_bits, 22,
            "{m:?} should have 22-bit encoder"
        );
        assert!(caps.supports_wireless, "{m:?} should support wireless");
    }
    Ok(())
}

#[test]
fn wheel_capabilities_default_is_sane() {
    let caps = WheelCapabilities::default();
    assert!(caps.max_torque_nm > 0.0);
    assert!(caps.encoder_resolution_bits > 0);
    assert!(caps.max_speed_rpm > 0);
}

// ═══════════════════════════════════════════════════════════════════════════
// 9. Proptest fuzzing
// ═══════════════════════════════════════════════════════════════════════════

mod proptest_deep {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        /// Any 32+ byte slice must parse without panic.
        #[test]
        fn prop_hid_report_parse_never_panics(data in proptest::collection::vec(any::<u8>(), 32..=128)) {
            let _ = SimucubeHidReport::parse(&data);
        }

        /// Any 16+ byte slice must parse without panic.
        #[test]
        fn prop_input_report_parse_never_panics(data in proptest::collection::vec(any::<u8>(), 16..=128)) {
            let _ = SimucubeInputReport::parse(&data);
        }

        /// Slices shorter than 32 must always return an error.
        #[test]
        fn prop_hid_report_rejects_short(data in proptest::collection::vec(any::<u8>(), 0..32)) {
            let result = SimucubeHidReport::parse(&data);
            prop_assert!(result.is_err());
        }

        /// Slices shorter than 16 must always return an error.
        #[test]
        fn prop_input_report_rejects_short(data in proptest::collection::vec(any::<u8>(), 0..16)) {
            let result = SimucubeInputReport::parse(&data);
            prop_assert!(result.is_err());
        }

        /// HID report steering roundtrip: write u16 LE, parse, read back same value.
        #[test]
        fn prop_hid_steering_roundtrip(steering in any::<u16>(), y in any::<u16>()) {
            let data = make_hid_report(steering, y, [0; 6], [0; 16]);
            let report = SimucubeHidReport::parse(&data);
            prop_assert!(report.is_ok());
            if let Ok(r) = report {
                prop_assert_eq!(r.steering, steering);
                prop_assert_eq!(r.y_axis, y);
            }
        }

        /// HID report axes roundtrip.
        #[test]
        fn prop_hid_axes_roundtrip(
            a0 in any::<u16>(), a1 in any::<u16>(), a2 in any::<u16>(),
            a3 in any::<u16>(), a4 in any::<u16>(), a5 in any::<u16>()
        ) {
            let axes = [a0, a1, a2, a3, a4, a5];
            let data = make_hid_report(0x8000, 0x8000, axes, [0; 16]);
            let report = SimucubeHidReport::parse(&data);
            prop_assert!(report.is_ok());
            if let Ok(r) = report {
                prop_assert_eq!(r.axes, axes);
            }
        }

        /// Output report build never fails for any valid sequence.
        #[test]
        fn prop_output_build_always_succeeds(seq in any::<u16>(), torque in -25.0f32..=25.0f32) {
            let report = SimucubeOutputReport::new(seq).with_torque(torque);
            let result = report.build();
            prop_assert!(result.is_ok());
            if let Ok(data) = result {
                prop_assert_eq!(data.len(), REPORT_SIZE_OUTPUT);
            }
        }

        /// PIDFF constant force magnitude is preserved exactly.
        #[test]
        fn prop_pidff_constant_force_roundtrip(block in 0u8..=255u8, mag in any::<i16>()) {
            let buf = encode_set_constant_force(block, mag);
            prop_assert_eq!(buf[1], block);
            prop_assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), mag);
        }

        /// PIDFF device gain is always clamped to [0, 10000].
        #[test]
        fn prop_pidff_device_gain_bounded(gain in any::<u16>()) {
            let buf = encode_device_gain(gain);
            let encoded = u16::from_le_bytes([buf[2], buf[3]]);
            prop_assert!(encoded <= 10000);
        }

        /// Model lookup is deterministic.
        #[test]
        fn prop_model_from_info_deterministic(vid in any::<u16>(), pid in any::<u16>()) {
            let a = simucube_model_from_info(vid, pid);
            let b = simucube_model_from_info(vid, pid);
            prop_assert_eq!(a, b);
        }

        /// steering_signed is always in [-1.0, 1.0].
        #[test]
        fn prop_hid_steering_signed_range(steering in any::<u16>()) {
            let data = make_hid_report(steering, 0x8000, [0; 6], [0; 16]);
            if let Ok(r) = SimucubeHidReport::parse(&data) {
                let s = r.steering_signed();
                prop_assert!((-1.0..=1.0).contains(&s), "steering_signed={s} out of range");
            }
        }

        /// steering_normalized is always in [0.0, 1.0].
        #[test]
        fn prop_hid_steering_normalized_range(steering in any::<u16>()) {
            let data = make_hid_report(steering, 0x8000, [0; 6], [0; 16]);
            if let Ok(r) = SimucubeHidReport::parse(&data) {
                let n = r.steering_normalized();
                prop_assert!((0.0..=1.0).contains(&n), "steering_normalized={n} out of range");
            }
        }
    }
}
