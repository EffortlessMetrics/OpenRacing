//! BDD end-to-end tests for the Simucube protocol stack.
//!
//! Each test follows a Given/When/Then pattern to verify observable hardware-ready
//! behaviors without real USB hardware.

use hid_simucube_protocol::{
    DeviceStatus, EffectType, SimucubeError, SimucubeInputReport, SimucubeOutputReport,
    SimucubeModel, WheelCapabilities, WheelModel,
    ANGLE_SENSOR_MAX, MAX_TORQUE_NM, MAX_TORQUE_PRO, MAX_TORQUE_SPORT, MAX_TORQUE_ULTIMATE,
    PRODUCT_ID_PRO, PRODUCT_ID_SPORT, PRODUCT_ID_ULTIMATE, REPORT_SIZE_INPUT, REPORT_SIZE_OUTPUT,
    SIMUCUBE_1_PID, SIMUCUBE_2_PRO_PID, SIMUCUBE_2_SPORT_PID, SIMUCUBE_2_ULTIMATE_PID,
    SIMUCUBE_ACTIVE_PEDAL_PID, SIMUCUBE_VENDOR_ID, SIMUCUBE_WIRELESS_WHEEL_PID, VENDOR_ID,
    is_simucube_device, simucube_model_from_info,
};

// ─── Output report building ──────────────────────────────────────────────────

// ─── Scenario 1: zero torque produces a neutral output report ────────────────

#[test]
fn scenario_output_given_zero_torque_when_built_then_torque_bytes_are_zero(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: an output report with zero torque
    let report = SimucubeOutputReport::new(1).with_torque(0.0);

    // When: built into wire bytes
    let data = report.build()?;

    // Then: torque field (bytes 3–4) is zero
    let torque = i16::from_le_bytes([data[3], data[4]]);
    assert_eq!(torque, 0, "zero torque must encode to 0");

    // Then: report is exactly REPORT_SIZE_OUTPUT bytes
    assert_eq!(data.len(), REPORT_SIZE_OUTPUT);

    Ok(())
}

// ─── Scenario 2: positive torque encodes correctly ───────────────────────────

#[test]
fn scenario_output_given_positive_torque_when_built_then_cnm_is_positive(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: 10.5 Nm torque
    let report = SimucubeOutputReport::new(42).with_torque(10.5);

    // When: built
    let data = report.build()?;

    // Then: torque_cNm = 1050 (10.5 * 100)
    let torque = i16::from_le_bytes([data[3], data[4]]);
    assert_eq!(torque, 1050, "10.5 Nm must encode to 1050 cNm");

    Ok(())
}

// ─── Scenario 3: negative torque encodes correctly ───────────────────────────

#[test]
fn scenario_output_given_negative_torque_when_built_then_cnm_is_negative(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: -8.0 Nm torque
    let report = SimucubeOutputReport::new(0).with_torque(-8.0);

    // When: built
    let data = report.build()?;

    // Then: torque_cNm = -800
    let torque = i16::from_le_bytes([data[3], data[4]]);
    assert_eq!(torque, -800, "-8.0 Nm must encode to -800 cNm");

    Ok(())
}

// ─── Scenario 4: torque saturates at MAX_TORQUE_NM ───────────────────────────

#[test]
fn scenario_output_given_excessive_torque_when_built_then_clamped_to_max(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: torque far exceeding maximum
    let report_pos = SimucubeOutputReport::new(0).with_torque(100.0);
    let report_neg = SimucubeOutputReport::new(0).with_torque(-100.0);

    // When: built
    let data_pos = report_pos.build()?;
    let data_neg = report_neg.build()?;

    // Then: clamped to ±MAX_TORQUE_NM in cNm
    let torque_pos = i16::from_le_bytes([data_pos[3], data_pos[4]]);
    let torque_neg = i16::from_le_bytes([data_neg[3], data_neg[4]]);
    let max_cnm = (MAX_TORQUE_NM * 100.0) as i16;

    assert_eq!(torque_pos, max_cnm, "positive must clamp to MAX_TORQUE_NM");
    assert_eq!(
        torque_neg,
        -max_cnm,
        "negative must clamp to -MAX_TORQUE_NM"
    );

    Ok(())
}

// ─── Model-specific torque limits ────────────────────────────────────────────

// ─── Scenario 5: Sport model torque limit is 17 Nm ──────────────────────────

#[test]
fn scenario_model_given_sport_when_queried_then_max_torque_is_17nm(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: Sport model
    let caps = WheelCapabilities::for_model(WheelModel::Simucube2Sport);

    // Then: 17 Nm limit
    assert!(
        (caps.max_torque_nm - 17.0).abs() < f32::EPSILON,
        "Sport max torque must be 17.0 Nm, got {}",
        caps.max_torque_nm
    );

    // Then: SimucubeModel agrees
    let model = SimucubeModel::from_product_id(SIMUCUBE_2_SPORT_PID);
    assert!(
        (model.max_torque_nm() - 17.0).abs() < f32::EPSILON,
        "SimucubeModel::Sport must report 17.0 Nm"
    );

    Ok(())
}

// ─── Scenario 6: Pro model torque limit is 25 Nm ────────────────────────────

#[test]
fn scenario_model_given_pro_when_queried_then_max_torque_is_25nm(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: Pro model
    let caps = WheelCapabilities::for_model(WheelModel::Simucube2Pro);

    // Then: 25 Nm limit
    assert!(
        (caps.max_torque_nm - 25.0).abs() < f32::EPSILON,
        "Pro max torque must be 25.0 Nm, got {}",
        caps.max_torque_nm
    );

    let model = SimucubeModel::from_product_id(SIMUCUBE_2_PRO_PID);
    assert!(
        (model.max_torque_nm() - 25.0).abs() < f32::EPSILON,
        "SimucubeModel::Pro must report 25.0 Nm"
    );

    Ok(())
}

// ─── Scenario 7: Ultimate model torque limit is 32 Nm ───────────────────────

#[test]
fn scenario_model_given_ultimate_when_queried_then_max_torque_is_32nm(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: Ultimate model
    let caps = WheelCapabilities::for_model(WheelModel::Simucube2Ultimate);

    // Then: 32 Nm limit
    assert!(
        (caps.max_torque_nm - 32.0).abs() < f32::EPSILON,
        "Ultimate max torque must be 32.0 Nm, got {}",
        caps.max_torque_nm
    );

    let model = SimucubeModel::from_product_id(SIMUCUBE_2_ULTIMATE_PID);
    assert!(
        (model.max_torque_nm() - 32.0).abs() < f32::EPSILON,
        "SimucubeModel::Ultimate must report 32.0 Nm"
    );

    Ok(())
}

// ─── Scenario 8: model constants agree with WheelCapabilities ────────────────

#[test]
fn scenario_model_given_all_models_when_compared_then_constants_agree(
) -> Result<(), Box<dyn std::error::Error>> {
    // Then: module-level constants match WheelCapabilities for each model
    assert!(
        (MAX_TORQUE_SPORT
            - WheelCapabilities::for_model(WheelModel::Simucube2Sport).max_torque_nm)
            .abs()
            < f32::EPSILON
    );
    assert!(
        (MAX_TORQUE_PRO - WheelCapabilities::for_model(WheelModel::Simucube2Pro).max_torque_nm)
            .abs()
            < f32::EPSILON
    );
    assert!(
        (MAX_TORQUE_ULTIMATE
            - WheelCapabilities::for_model(WheelModel::Simucube2Ultimate).max_torque_nm)
            .abs()
            < f32::EPSILON
    );

    Ok(())
}

// ─── Input report parsing ────────────────────────────────────────────────────

// Helper: build a minimal 16-byte input report from field values.
fn build_input_bytes(
    seq: u16,
    angle: u32,
    speed: i16,
    torque: i16,
    temp: u8,
    faults: u8,
    _reserved: u8,
    status: u8,
) -> [u8; 16] {
    let mut buf = [0u8; 16];
    buf[0..2].copy_from_slice(&seq.to_le_bytes());
    buf[2..6].copy_from_slice(&angle.to_le_bytes());
    buf[6..8].copy_from_slice(&speed.to_le_bytes());
    buf[8..10].copy_from_slice(&torque.to_le_bytes());
    buf[10] = temp;
    buf[11] = faults;
    buf[12] = _reserved;
    buf[13] = status;
    buf
}

// ─── Scenario 9: wheel angle parses and converts to degrees ──────────────────

#[test]
fn scenario_input_given_quarter_turn_when_parsed_then_angle_is_90_degrees(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: raw angle at 1/4 of sensor range
    let raw = build_input_bytes(1, ANGLE_SENSOR_MAX / 4, 0, 0, 25, 0, 0, 0x03);

    // When: parsed
    let report = SimucubeInputReport::parse(&raw)?;

    // Then: angle ≈ 90°
    let degrees = report.wheel_angle_degrees();
    assert!(
        (degrees - 90.0).abs() < 0.1,
        "quarter sensor range must be ~90°, got {degrees}"
    );

    Ok(())
}

// ─── Scenario 10: wheel speed converts to rad/s ─────────────────────────────

#[test]
fn scenario_input_given_60rpm_when_parsed_then_speed_is_2pi_rad_s(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: 60 RPM
    let raw = build_input_bytes(0, 0, 60, 0, 25, 0, 0, 0x03);

    // When: parsed
    let report = SimucubeInputReport::parse(&raw)?;

    // Then: speed ≈ 2π rad/s
    let rad_s = report.wheel_speed_rad_s();
    assert!(
        (rad_s - 2.0 * std::f32::consts::PI).abs() < 0.01,
        "60 RPM must be ~2π rad/s, got {rad_s}"
    );

    Ok(())
}

// ─── Scenario 11: fault flags detection ──────────────────────────────────────

#[test]
fn scenario_input_given_fault_flags_when_parsed_then_has_fault_is_true(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: non-zero fault flags
    let raw = build_input_bytes(0, 0, 0, 0, 25, 0x04, 0, 0x03);

    // When: parsed
    let report = SimucubeInputReport::parse(&raw)?;

    // Then: has_fault is true
    assert!(report.has_fault(), "non-zero fault flags must indicate fault");

    // Given: zero fault flags
    let raw_clean = build_input_bytes(0, 0, 0, 0, 25, 0x00, 0, 0x03);
    let report_clean = SimucubeInputReport::parse(&raw_clean)?;

    // Then: has_fault is false
    assert!(
        !report_clean.has_fault(),
        "zero fault flags must indicate no fault"
    );

    Ok(())
}

// ─── Scenario 12: connection status flags ────────────────────────────────────

#[test]
fn scenario_input_given_status_flags_when_parsed_then_connected_and_enabled_correct(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: connected + enabled (0x03)
    let raw = build_input_bytes(0, 0, 0, 0, 25, 0, 0, 0x03);
    let report = SimucubeInputReport::parse(&raw)?;
    assert!(report.is_connected(), "0x03 must be connected");
    assert!(report.is_enabled(), "0x03 must be enabled");

    // Given: connected only (0x01)
    let raw = build_input_bytes(0, 0, 0, 0, 25, 0, 0, 0x01);
    let report = SimucubeInputReport::parse(&raw)?;
    assert!(report.is_connected(), "0x01 must be connected");
    assert!(!report.is_enabled(), "0x01 must not be enabled");

    // Given: disconnected (0x00)
    let raw = build_input_bytes(0, 0, 0, 0, 25, 0, 0, 0x00);
    let report = SimucubeInputReport::parse(&raw)?;
    assert!(!report.is_connected(), "0x00 must not be connected");
    assert!(!report.is_enabled(), "0x00 must not be enabled");

    Ok(())
}

// ─── Scenario 13: applied torque parses from cNm ────────────────────────────

#[test]
fn scenario_input_given_torque_1500cnm_when_parsed_then_applied_torque_is_15nm(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: torque field = 1500 (cNm)
    let raw = build_input_bytes(0, 0, 0, 1500, 25, 0, 0, 0x03);

    // When: parsed
    let report = SimucubeInputReport::parse(&raw)?;

    // Then: applied_torque_nm = 15.0
    let torque = report.applied_torque_nm();
    assert!(
        (torque - 15.0).abs() < 0.01,
        "1500 cNm must be 15.0 Nm, got {torque}"
    );

    Ok(())
}

// ─── Effect types ────────────────────────────────────────────────────────────

// ─── Scenario 14: constant effect type encodes correctly ─────────────────────

#[test]
fn scenario_effect_given_constant_when_built_then_effect_byte_is_1(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: constant effect with parameter 1000
    let report = SimucubeOutputReport::new(0).with_effect(EffectType::Constant, 1000);

    // When: built
    let data = report.build()?;

    // Then: effect byte (offset 8) = 1, parameter (offset 9-10) = 1000 LE
    assert_eq!(data[8], EffectType::Constant as u8);
    let param = u16::from_le_bytes([data[9], data[10]]);
    assert_eq!(param, 1000, "effect parameter must be 1000");

    Ok(())
}

// ─── Scenario 15: spring effect type encodes correctly ───────────────────────

#[test]
fn scenario_effect_given_spring_when_built_then_effect_byte_is_8(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: spring effect
    let report = SimucubeOutputReport::new(0).with_effect(EffectType::Spring, 500);
    let data = report.build()?;

    // Then: effect type byte = 8
    assert_eq!(data[8], 8, "Spring effect type must be 8");

    Ok(())
}

// ─── Scenario 16: damper effect type encodes correctly ───────────────────────

#[test]
fn scenario_effect_given_damper_when_built_then_effect_byte_is_9(
) -> Result<(), Box<dyn std::error::Error>> {
    let report = SimucubeOutputReport::new(0).with_effect(EffectType::Damper, 250);
    let data = report.build()?;

    assert_eq!(data[8], 9, "Damper effect type must be 9");

    Ok(())
}

// ─── Scenario 17: sine effect type encodes correctly ─────────────────────────

#[test]
fn scenario_effect_given_sine_when_built_then_effect_byte_is_4(
) -> Result<(), Box<dyn std::error::Error>> {
    let report = SimucubeOutputReport::new(0).with_effect(EffectType::Sine, 800);
    let data = report.build()?;

    assert_eq!(data[8], 4, "Sine effect type must be 4");
    let param = u16::from_le_bytes([data[9], data[10]]);
    assert_eq!(param, 800);

    Ok(())
}

// ─── Scenario 18: friction effect type encodes correctly ─────────────────────

#[test]
fn scenario_effect_given_friction_when_built_then_effect_byte_is_10(
) -> Result<(), Box<dyn std::error::Error>> {
    let report = SimucubeOutputReport::new(0).with_effect(EffectType::Friction, 300);
    let data = report.build()?;

    assert_eq!(data[8], 10, "Friction effect type must be 10");

    Ok(())
}

// ─── Scenario 19: all effect type discriminants are unique ───────────────────

#[test]
fn scenario_effect_given_all_types_when_compared_then_discriminants_unique(
) -> Result<(), Box<dyn std::error::Error>> {
    let types = [
        EffectType::None,
        EffectType::Constant,
        EffectType::Ramp,
        EffectType::Square,
        EffectType::Sine,
        EffectType::Triangle,
        EffectType::SawtoothUp,
        EffectType::SawtoothDown,
        EffectType::Spring,
        EffectType::Damper,
        EffectType::Friction,
    ];

    // Then: all discriminant values are distinct
    let mut seen = std::collections::HashSet::new();
    for t in &types {
        assert!(
            seen.insert(*t as u8),
            "duplicate effect type discriminant: {}",
            *t as u8
        );
    }

    // Then: contiguous range 0..=10
    assert_eq!(types.len(), 11, "must have 11 effect types");
    assert_eq!(EffectType::None as u8, 0);
    assert_eq!(EffectType::Friction as u8, 10);

    Ok(())
}

// ─── RGB LED control ─────────────────────────────────────────────────────────

// ─── Scenario 20: RGB values encode in correct byte positions ────────────────

#[test]
fn scenario_rgb_given_color_when_built_then_bytes_at_correct_offsets(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: specific RGB values
    let report = SimucubeOutputReport::new(0).with_rgb(255, 128, 64);

    // When: built
    let data = report.build()?;

    // Then: R at byte 5, G at byte 6, B at byte 7
    assert_eq!(data[5], 255, "red must be at byte 5");
    assert_eq!(data[6], 128, "green must be at byte 6");
    assert_eq!(data[7], 64, "blue must be at byte 7");

    Ok(())
}

// ─── Scenario 21: RGB with torque and effect coexist ─────────────────────────

#[test]
fn scenario_rgb_given_torque_and_effect_when_built_then_all_fields_independent(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: report with torque, RGB, and effect
    let report = SimucubeOutputReport::new(99)
        .with_torque(5.0)
        .with_rgb(100, 200, 50)
        .with_effect(EffectType::Damper, 750);

    // When: built
    let data = report.build()?;

    // Then: all fields are correct simultaneously
    let seq = u16::from_le_bytes([data[1], data[2]]);
    assert_eq!(seq, 99, "sequence must be 99");

    let torque = i16::from_le_bytes([data[3], data[4]]);
    assert_eq!(torque, 500, "5.0 Nm must be 500 cNm");

    assert_eq!(data[5], 100, "red");
    assert_eq!(data[6], 200, "green");
    assert_eq!(data[7], 50, "blue");

    assert_eq!(data[8], EffectType::Damper as u8);
    let param = u16::from_le_bytes([data[9], data[10]]);
    assert_eq!(param, 750);

    Ok(())
}

// ─── Wireless wheel detection ────────────────────────────────────────────────

// ─── Scenario 22: extended report with wireless wheel data ───────────────────

#[test]
fn scenario_wireless_given_extended_report_when_parsed_then_wheel_detected(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: 17-byte report with wireless button and battery data
    let mut data = [0u8; 17];
    data[14] = 0b0000_0101; // buttons: 0 and 2 pressed
    data[15] = 0x00;
    data[16] = 85; // 85% battery

    // When: parsed
    let report = SimucubeInputReport::parse(&data)?;

    // Then: wireless wheel is detected
    assert!(
        report.has_wireless_wheel(),
        "must detect wireless wheel with battery data"
    );
    assert_eq!(report.wireless_buttons, 0b0000_0101);
    assert_eq!(report.wireless_battery_pct, 85);

    Ok(())
}

// ─── Scenario 23: short report has no wireless fields ────────────────────────

#[test]
fn scenario_wireless_given_16byte_report_when_parsed_then_no_wireless(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: standard 16-byte report (no wireless extension)
    let data = [0u8; 16];

    // When: parsed
    let report = SimucubeInputReport::parse(&data)?;

    // Then: no wireless wheel
    assert!(
        !report.has_wireless_wheel(),
        "16-byte report must not indicate wireless wheel"
    );
    assert_eq!(report.wireless_buttons, 0);
    assert_eq!(report.wireless_battery_pct, 0);

    Ok(())
}

// ─── Scenario 24: all wireless buttons pressed ──────────────────────────────

#[test]
fn scenario_wireless_given_all_buttons_pressed_when_parsed_then_bitmask_is_ffff(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: all 16 wireless buttons pressed, full battery
    let mut data = [0u8; 17];
    data[14] = 0xFF;
    data[15] = 0xFF;
    data[16] = 100;

    // When: parsed
    let report = SimucubeInputReport::parse(&data)?;

    // Then: full bitmask
    assert_eq!(report.wireless_buttons, 0xFFFF);
    assert_eq!(report.wireless_battery_pct, 100);
    assert!(report.has_wireless_wheel());

    Ok(())
}

// ─── Scenario 25: wireless wheel PID recognized ──────────────────────────────

#[test]
fn scenario_wireless_given_wireless_pid_when_identified_then_model_is_wireless_wheel(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: wireless wheel product ID
    let model = SimucubeModel::from_product_id(SIMUCUBE_WIRELESS_WHEEL_PID);

    // Then: identified as WirelessWheel
    assert_eq!(model, SimucubeModel::WirelessWheel);
    assert_eq!(model.display_name(), "SimuCube Wireless Wheel");
    assert!(
        (model.max_torque_nm()).abs() < f32::EPSILON,
        "wireless wheel must have 0 torque"
    );

    Ok(())
}

// ─── Torque sign preservation and monotonicity ───────────────────────────────

// ─── Scenario 26: torque sign is preserved through build ─────────────────────

#[test]
fn scenario_torque_given_positive_and_negative_when_built_then_signs_preserved(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: positive and negative torque values
    let pos = SimucubeOutputReport::new(0).with_torque(12.0);
    let neg = SimucubeOutputReport::new(0).with_torque(-12.0);

    // When: built
    let data_pos = pos.build()?;
    let data_neg = neg.build()?;

    // Then: signs are opposite
    let t_pos = i16::from_le_bytes([data_pos[3], data_pos[4]]);
    let t_neg = i16::from_le_bytes([data_neg[3], data_neg[4]]);

    assert!(t_pos > 0, "positive torque must encode positive");
    assert!(t_neg < 0, "negative torque must encode negative");
    assert_eq!(t_pos, -t_neg, "magnitudes must be equal");

    Ok(())
}

// ─── Scenario 27: torque encoding is monotonic ──────────────────────────────

#[test]
fn scenario_torque_given_increasing_values_when_built_then_encoding_monotonic(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: increasing torque values from -MAX to +MAX
    let steps: Vec<f32> = (-20..=20).map(|i| i as f32).collect();

    let mut prev_cnm = i16::MIN;
    for &nm in &steps {
        let report = SimucubeOutputReport::new(0).with_torque(nm);
        let data = report.build()?;
        let cnm = i16::from_le_bytes([data[3], data[4]]);

        assert!(
            cnm >= prev_cnm,
            "torque encoding must be monotonic: {nm} Nm encoded to {cnm}, prev was {prev_cnm}"
        );
        prev_cnm = cnm;
    }

    Ok(())
}

// ─── Byte layout / wire format verification ──────────────────────────────────

// ─── Scenario 28: output report wire layout matches specification ────────────

#[test]
fn scenario_wire_given_known_values_when_built_then_byte_layout_matches(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: report with known values
    let report = SimucubeOutputReport::new(0x1234)
        .with_torque(5.0)
        .with_rgb(0xAA, 0xBB, 0xCC)
        .with_effect(EffectType::Spring, 0x0300);

    // When: built
    let data = report.build()?;

    // Then: byte 0 = report ID (0x01)
    assert_eq!(data[0], 0x01, "byte 0 must be report ID 0x01");

    // Then: bytes 1-2 = sequence LE (0x34, 0x12)
    assert_eq!(data[1], 0x34, "sequence low byte");
    assert_eq!(data[2], 0x12, "sequence high byte");

    // Then: bytes 3-4 = torque_cNm LE (500 = 0x01F4)
    assert_eq!(data[3], 0xF4, "torque low byte");
    assert_eq!(data[4], 0x01, "torque high byte");

    // Then: bytes 5-7 = RGB
    assert_eq!(data[5], 0xAA, "red");
    assert_eq!(data[6], 0xBB, "green");
    assert_eq!(data[7], 0xCC, "blue");

    // Then: byte 8 = effect type
    assert_eq!(data[8], EffectType::Spring as u8, "effect type byte");

    // Then: bytes 9-10 = effect parameter LE
    assert_eq!(data[9], 0x00, "effect param low");
    assert_eq!(data[10], 0x03, "effect param high");

    // Then: remaining bytes zero-padded to REPORT_SIZE_OUTPUT
    assert_eq!(data.len(), REPORT_SIZE_OUTPUT);
    assert!(
        data[11..].iter().all(|&b| b == 0),
        "trailing bytes must be zero-padded"
    );

    Ok(())
}

// ─── Scenario 29: input report parses all fields from known bytes ────────────

#[test]
fn scenario_wire_given_known_input_bytes_when_parsed_then_all_fields_correct(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: hand-crafted 16-byte input report
    let raw = build_input_bytes(
        0x0042, // sequence = 66
        0x0010_0000, // angle (mid-range)
        -120,  // speed = -120 RPM
        750,   // torque = 750 cNm = 7.5 Nm
        45,    // temperature = 45°C
        0x02,  // fault flags
        0x00,  // reserved
        0x03,  // connected + enabled
    );

    // When: parsed
    let report = SimucubeInputReport::parse(&raw)?;

    // Then: all fields match
    assert_eq!(report.sequence, 0x0042);
    assert_eq!(report.wheel_angle_raw, 0x0010_0000);
    assert_eq!(report.wheel_speed_rpm, -120);
    assert_eq!(report.torque_nm, 750);
    assert_eq!(report.temperature_c, 45);
    assert_eq!(report.fault_flags, 0x02);
    assert_eq!(report.status_flags, 0x03);
    assert!(report.has_fault());
    assert!(report.is_connected());
    assert!(report.is_enabled());

    let torque = report.applied_torque_nm();
    assert!(
        (torque - 7.5).abs() < 0.01,
        "750 cNm must be 7.5 Nm, got {torque}"
    );

    Ok(())
}

// ─── Scenario 30: report size constants are 64 bytes ─────────────────────────

#[test]
fn scenario_wire_given_report_size_constants_then_both_are_64() {
    assert_eq!(REPORT_SIZE_INPUT, 64, "input report size must be 64");
    assert_eq!(REPORT_SIZE_OUTPUT, 64, "output report size must be 64");
}

// ─── Error handling ──────────────────────────────────────────────────────────

// ─── Scenario 31: short buffer rejected ──────────────────────────────────────

#[test]
fn scenario_error_given_short_buffer_when_parsed_then_invalid_report_size() {
    // Given: buffer shorter than minimum (16 bytes)
    let short = [0u8; 8];

    // When: parsed
    let result = SimucubeInputReport::parse(&short);

    // Then: InvalidReportSize error with correct expected/actual
    assert!(
        matches!(
            result,
            Err(SimucubeError::InvalidReportSize {
                expected: 16,
                actual: 8
            })
        ),
        "short buffer must return InvalidReportSize"
    );
}

// ─── Scenario 32: empty buffer rejected ──────────────────────────────────────

#[test]
fn scenario_error_given_empty_buffer_when_parsed_then_invalid_report_size() {
    let result = SimucubeInputReport::parse(&[]);

    assert!(
        matches!(result, Err(SimucubeError::InvalidReportSize { .. })),
        "empty buffer must return InvalidReportSize"
    );
}

// ─── Scenario 33: validate_torque rejects out-of-range values ────────────────

#[test]
fn scenario_error_given_excessive_cnm_when_validated_then_invalid_torque() {
    // Given: manually set torque_cNm beyond MAX_TORQUE_NM
    let report = SimucubeOutputReport {
        torque_cNm: (MAX_TORQUE_NM * 200.0) as i16,
        ..Default::default()
    };

    // When: validated
    let result = report.validate_torque();

    // Then: InvalidTorque error
    assert!(
        matches!(result, Err(SimucubeError::InvalidTorque(_))),
        "excessive torque must fail validation"
    );
}

// ─── Scenario 34: validate_torque accepts in-range values ────────────────────

#[test]
fn scenario_error_given_valid_torque_when_validated_then_ok(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: torque within limits
    let report = SimucubeOutputReport::new(0).with_torque(10.0);

    // When: validated
    let result = report.validate_torque();

    // Then: ok
    assert!(result.is_ok(), "in-range torque must pass validation");

    Ok(())
}

// ─── Scenario 35: 15-byte buffer rejected (one byte short) ──────────────────

#[test]
fn scenario_error_given_15_byte_buffer_when_parsed_then_rejected() {
    let result = SimucubeInputReport::parse(&[0u8; 15]);

    assert!(
        matches!(
            result,
            Err(SimucubeError::InvalidReportSize {
                expected: 16,
                actual: 15
            })
        ),
        "15-byte buffer must be rejected"
    );
}

// ─── Device identification ───────────────────────────────────────────────────

// ─── Scenario 36: vendor ID identifies Simucube devices ──────────────────────

#[test]
fn scenario_ids_given_simucube_vendor_id_when_checked_then_recognized(
) -> Result<(), Box<dyn std::error::Error>> {
    assert!(
        is_simucube_device(SIMUCUBE_VENDOR_ID),
        "Simucube VID must be recognized"
    );
    assert!(
        is_simucube_device(VENDOR_ID),
        "VENDOR_ID alias must be recognized"
    );
    assert!(
        !is_simucube_device(0x1234),
        "random VID must not be recognized"
    );

    Ok(())
}

// ─── Scenario 37: model from VID+PID ────────────────────────────────────────

#[test]
fn scenario_ids_given_vid_pid_when_resolved_then_correct_model(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: Simucube VID + Sport PID
    let model = simucube_model_from_info(SIMUCUBE_VENDOR_ID, SIMUCUBE_2_SPORT_PID);
    assert_eq!(model, SimucubeModel::Sport);

    // Given: wrong VID
    let model = simucube_model_from_info(0x1234, SIMUCUBE_2_SPORT_PID);
    assert_eq!(model, SimucubeModel::Unknown, "wrong VID must give Unknown");

    Ok(())
}

// ─── Scenario 38: all known PIDs resolve to correct models ───────────────────

#[test]
fn scenario_ids_given_all_known_pids_when_resolved_then_correct() {
    assert_eq!(
        SimucubeModel::from_product_id(SIMUCUBE_1_PID),
        SimucubeModel::Simucube1
    );
    assert_eq!(
        SimucubeModel::from_product_id(SIMUCUBE_2_SPORT_PID),
        SimucubeModel::Sport
    );
    assert_eq!(
        SimucubeModel::from_product_id(SIMUCUBE_2_PRO_PID),
        SimucubeModel::Pro
    );
    assert_eq!(
        SimucubeModel::from_product_id(SIMUCUBE_2_ULTIMATE_PID),
        SimucubeModel::Ultimate
    );
    assert_eq!(
        SimucubeModel::from_product_id(SIMUCUBE_ACTIVE_PEDAL_PID),
        SimucubeModel::ActivePedal
    );
    assert_eq!(
        SimucubeModel::from_product_id(SIMUCUBE_WIRELESS_WHEEL_PID),
        SimucubeModel::WirelessWheel
    );
    assert_eq!(
        SimucubeModel::from_product_id(0xFFFF),
        SimucubeModel::Unknown
    );
}

// ─── Scenario 39: display names are human-readable ───────────────────────────

#[test]
fn scenario_ids_given_models_when_display_name_then_non_empty(
) -> Result<(), Box<dyn std::error::Error>> {
    let models = [
        SimucubeModel::Simucube1,
        SimucubeModel::Sport,
        SimucubeModel::Pro,
        SimucubeModel::Ultimate,
        SimucubeModel::ActivePedal,
        SimucubeModel::WirelessWheel,
        SimucubeModel::Unknown,
    ];

    for model in &models {
        let name = model.display_name();
        assert!(
            !name.is_empty(),
            "{model:?} must have a non-empty display name"
        );
    }

    Ok(())
}

// ─── DeviceStatus from flags ─────────────────────────────────────────────────

// ─── Scenario 40: device status transitions from flag values ─────────────────

#[test]
fn scenario_status_given_flag_values_when_decoded_then_correct_state() {
    assert_eq!(DeviceStatus::from_flags(0x00), DeviceStatus::Disconnected);
    assert_eq!(DeviceStatus::from_flags(0x01), DeviceStatus::Ready);
    assert_eq!(DeviceStatus::from_flags(0x03), DeviceStatus::Enabled);
    assert_eq!(DeviceStatus::from_flags(0x05), DeviceStatus::Calibrating);
}

// ─── Scenario 41: ActivePedal has zero torque and no wireless ────────────────

#[test]
fn scenario_model_given_active_pedal_when_queried_then_zero_torque_no_wireless(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: ActivePedal model
    let caps = WheelCapabilities::for_model(WheelModel::SimucubeActivePedal);

    // Then: zero torque, no wireless
    assert!(
        caps.max_torque_nm.abs() < f32::EPSILON,
        "ActivePedal must have 0 torque"
    );
    assert!(
        !caps.supports_wireless,
        "ActivePedal must not support wireless"
    );

    Ok(())
}

// ─── Scenario 42: PID constants match between lib.rs and ids.rs ──────────────

#[test]
fn scenario_ids_given_lib_and_ids_constants_when_compared_then_agree() {
    assert_eq!(VENDOR_ID, SIMUCUBE_VENDOR_ID, "VID must agree across modules");
    assert_eq!(PRODUCT_ID_SPORT, SIMUCUBE_2_SPORT_PID, "Sport PID must agree");
    assert_eq!(PRODUCT_ID_PRO, SIMUCUBE_2_PRO_PID, "Pro PID must agree");
    assert_eq!(
        PRODUCT_ID_ULTIMATE, SIMUCUBE_2_ULTIMATE_PID,
        "Ultimate PID must agree"
    );
}

// ─── Scenario 43: angle at sensor max is 360 degrees ─────────────────────────

#[test]
fn scenario_input_given_max_angle_when_converted_then_360_degrees(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: angle at sensor max
    let report = SimucubeInputReport {
        wheel_angle_raw: ANGLE_SENSOR_MAX,
        ..Default::default()
    };

    // Then: angle ≈ 360°
    let degrees = report.wheel_angle_degrees();
    assert!(
        (degrees - 360.0).abs() < 0.01,
        "max sensor must be ~360°, got {degrees}"
    );

    // Then: radians ≈ 2π
    let radians = report.wheel_angle_radians();
    assert!(
        (radians - 2.0 * std::f32::consts::PI).abs() < 0.01,
        "max sensor must be ~2π rad, got {radians}"
    );

    Ok(())
}

// ─── Scenario 44: angle at zero is 0 degrees ────────────────────────────────

#[test]
fn scenario_input_given_zero_angle_when_converted_then_zero_degrees() {
    let report = SimucubeInputReport {
        wheel_angle_raw: 0,
        ..Default::default()
    };

    let degrees = report.wheel_angle_degrees();
    assert!(
        degrees.abs() < f32::EPSILON,
        "zero sensor must be 0°, got {degrees}"
    );
}

// ─── Scenario 45: negative speed converts correctly ──────────────────────────

#[test]
fn scenario_input_given_negative_speed_when_converted_then_negative_rad_s(
) -> Result<(), Box<dyn std::error::Error>> {
    let report = SimucubeInputReport {
        wheel_speed_rpm: -60,
        ..Default::default()
    };

    let rad_s = report.wheel_speed_rad_s();
    assert!(
        (rad_s + 2.0 * std::f32::consts::PI).abs() < 0.01,
        "-60 RPM must be ~-2π rad/s, got {rad_s}"
    );

    Ok(())
}

// ─── Scenario 46: default output report is safe (zero torque, no effect) ─────

#[test]
fn scenario_output_given_default_when_inspected_then_safe_state(
) -> Result<(), Box<dyn std::error::Error>> {
    let report = SimucubeOutputReport::default();

    assert_eq!(report.sequence, 0);
    assert_eq!(report.torque_cNm, 0, "default must have zero torque");
    assert_eq!(report.led_r, 0);
    assert_eq!(report.led_g, 0);
    assert_eq!(report.led_b, 0);
    assert_eq!(report.effect_type, EffectType::None);
    assert_eq!(report.effect_parameter, 0);

    // Build must succeed
    let data = report.build()?;
    assert_eq!(data.len(), REPORT_SIZE_OUTPUT);

    Ok(())
}
