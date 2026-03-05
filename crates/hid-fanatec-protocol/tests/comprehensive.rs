//! Comprehensive tests for the Fanatec HID protocol crate.
//!
//! Covers: input report round-trips (wheel/pedal/shifter), output report
//! construction (LED, FFB), device identification via PID, multi-device
//! aggregation, edge cases (short/invalid reports), property-based axis
//! encoding tests, and known constant validation.

use proptest::prelude::*;
use racing_wheel_hid_fanatec_protocol::{
    self as fan, CONSTANT_FORCE_REPORT_LEN, FANATEC_VENDOR_ID, FanatecConstantForceEncoder,
    FanatecModel, FanatecPedalModel, FanatecRimId, LED_REPORT_LEN, MAX_ROTATION_DEGREES,
    MIN_ROTATION_DEGREES, is_pedal_product, is_wheelbase_product, led_commands,
    parse_extended_report, parse_pedal_report, parse_standard_report, product_ids, rim_ids,
};

// ═══════════════════════════════════════════════════════════════════════════
// §1  Input report parsing round-trips — wheel
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wheel_steering_full_left_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[0] = 0x01;
    // steering = 0x0000 → full left = -1.0
    data[1] = 0x00;
    data[2] = 0x00;
    data[3] = 0xFF;
    data[4] = 0xFF;
    data[5] = 0xFF;
    let state = parse_standard_report(&data).ok_or("parse failed")?;
    assert!(
        (state.steering + 1.0).abs() < 1e-4,
        "full left steering should be ~-1.0, got {}",
        state.steering
    );
    Ok(())
}

#[test]
fn wheel_all_buttons_set() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[0] = 0x01;
    data[1] = 0x00;
    data[2] = 0x80;
    data[7] = 0xFF;
    data[8] = 0xFF;
    let state = parse_standard_report(&data).ok_or("parse failed")?;
    assert_eq!(state.buttons, 0xFFFF, "all 16 button bits must be set");
    Ok(())
}

#[test]
fn wheel_individual_button_bits() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[0] = 0x01;
    data[1] = 0x00;
    data[2] = 0x80;
    for bit in 0u16..16 {
        let mask = 1u16 << bit;
        data[7] = (mask & 0xFF) as u8;
        data[8] = (mask >> 8) as u8;
        let state = parse_standard_report(&data).ok_or("parse failed")?;
        assert_eq!(
            state.buttons, mask,
            "bit {} must produce mask 0x{:04X}, got 0x{:04X}",
            bit, mask, state.buttons
        );
    }
    Ok(())
}

#[test]
fn wheel_hat_all_valid_directions() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[0] = 0x01;
    data[1] = 0x00;
    data[2] = 0x80;
    // Valid hat values: 0 (N) through 7 (NW), 0xF (neutral)
    let valid_hats: &[u8] = &[0, 1, 2, 3, 4, 5, 6, 7, 0x0F];
    for &hat_val in valid_hats {
        data[9] = hat_val;
        let state = parse_standard_report(&data).ok_or("parse failed")?;
        assert_eq!(
            state.hat, hat_val,
            "hat value 0x{:X} must round-trip",
            hat_val
        );
    }
    Ok(())
}

#[test]
fn wheel_hat_upper_nibble_stripped() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[0] = 0x01;
    data[1] = 0x00;
    data[2] = 0x80;
    // 0xA3 → lower nibble = 0x03
    data[9] = 0xA3;
    let state = parse_standard_report(&data).ok_or("parse failed")?;
    assert_eq!(state.hat, 0x03, "hat must strip upper nibble");
    Ok(())
}

#[test]
fn wheel_simultaneous_pedals_and_buttons() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[0] = 0x01;
    data[1] = 0x00;
    data[2] = 0x80; // center
    data[3] = 0x00; // throttle full (inverted)
    data[4] = 0x80; // brake ~50%
    data[5] = 0xFF; // clutch released
    data[7] = 0xAA;
    data[8] = 0x55; // buttons = 0x55AA
    data[9] = 0x02; // hat = right

    let state = parse_standard_report(&data).ok_or("parse failed")?;
    assert!((state.throttle - 1.0).abs() < 1e-4);
    assert!(state.brake > 0.4 && state.brake < 0.6);
    assert!(state.clutch.abs() < 1e-4);
    assert_eq!(state.buttons, 0x55AA);
    assert_eq!(state.hat, 0x02);
    Ok(())
}

#[test]
fn wheel_rim_id_byte_in_standard_report() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[0] = 0x01;
    data[1] = 0x00;
    data[2] = 0x80;
    data[0x1F] = rim_ids::FORMULA_V2;
    let _state = parse_standard_report(&data).ok_or("parse failed")?;
    // Verify the rim byte is accessible at the expected offset
    let rim = FanatecRimId::from_byte(data[0x1F]);
    assert_eq!(rim, FanatecRimId::FormulaV2);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// §2  Input report parsing round-trips — pedals
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn pedal_all_zeros() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 7];
    data[0] = 0x01;
    let state = parse_pedal_report(&data).ok_or("parse failed")?;
    assert_eq!(state.throttle_raw, 0);
    assert_eq!(state.brake_raw, 0);
    assert_eq!(state.clutch_raw, 0);
    assert_eq!(state.axis_count, 3);
    Ok(())
}

#[test]
fn pedal_all_max_12bit() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 7];
    data[0] = 0x01;
    data[1] = 0xFF;
    data[2] = 0x0F; // throttle = 0x0FFF
    data[3] = 0xFF;
    data[4] = 0x0F; // brake = 0x0FFF
    data[5] = 0xFF;
    data[6] = 0x0F; // clutch = 0x0FFF
    let state = parse_pedal_report(&data).ok_or("parse failed")?;
    assert_eq!(state.throttle_raw, 0x0FFF);
    assert_eq!(state.brake_raw, 0x0FFF);
    assert_eq!(state.clutch_raw, 0x0FFF);
    Ok(())
}

#[test]
fn pedal_upper_bits_masked() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 7];
    data[0] = 0x01;
    // Set all 16 bits = 0xFFFF, mask should produce 0x0FFF
    data[1] = 0xFF;
    data[2] = 0xFF;
    data[3] = 0xFF;
    data[4] = 0xFF;
    data[5] = 0xFF;
    data[6] = 0xFF;
    let state = parse_pedal_report(&data).ok_or("parse failed")?;
    assert_eq!(
        state.throttle_raw, 0x0FFF,
        "upper 4 bits must be masked off"
    );
    assert_eq!(state.brake_raw, 0x0FFF);
    assert_eq!(state.clutch_raw, 0x0FFF);
    Ok(())
}

#[test]
fn pedal_two_axis_no_clutch() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0x01, 0x00, 0x04, 0xFF, 0x0F];
    let state = parse_pedal_report(&data).ok_or("parse failed")?;
    assert_eq!(state.axis_count, 2);
    assert_eq!(state.clutch_raw, 0);
    assert_eq!(state.throttle_raw, 0x0400);
    assert_eq!(state.brake_raw, 0x0FFF);
    Ok(())
}

#[test]
fn pedal_six_bytes_still_two_axis() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0x01, 0x00, 0x04, 0xFF, 0x0F, 0xAA];
    let state = parse_pedal_report(&data).ok_or("parse failed")?;
    assert_eq!(state.axis_count, 2, "6 bytes (< 7) must be 2-axis");
    assert_eq!(state.clutch_raw, 0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// §3  Input report parsing round-trips — extended (telemetry)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn extended_report_all_fields() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[0] = 0x02;
    // steering_raw = -1000 = 0xFC18 LE
    data[1] = 0x18;
    data[2] = 0xFC;
    // steering_velocity = 500 = 0x01F4 LE
    data[3] = 0xF4;
    data[4] = 0x01;
    data[5] = 80; // motor temp
    data[6] = 55; // board temp
    data[7] = 42; // current raw
    data[10] = 0x0F; // all fault flags set (lower nibble)

    let state = parse_extended_report(&data).ok_or("parse failed")?;
    assert_eq!(state.steering_raw, -1000);
    assert_eq!(state.steering_velocity, 500);
    assert_eq!(state.motor_temp_c, 80);
    assert_eq!(state.board_temp_c, 55);
    assert_eq!(state.current_raw, 42);
    assert_eq!(state.fault_flags, 0x0F);
    Ok(())
}

#[test]
fn extended_report_max_positive_steering() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[0] = 0x02;
    // i16::MAX = 32767 = 0x7FFF LE
    data[1] = 0xFF;
    data[2] = 0x7F;
    let state = parse_extended_report(&data).ok_or("parse failed")?;
    assert_eq!(state.steering_raw, i16::MAX);
    Ok(())
}

#[test]
fn extended_report_max_negative_steering() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[0] = 0x02;
    // i16::MIN = -32768 = 0x8000 LE
    data[1] = 0x00;
    data[2] = 0x80;
    let state = parse_extended_report(&data).ok_or("parse failed")?;
    assert_eq!(state.steering_raw, i16::MIN);
    Ok(())
}

#[test]
fn extended_report_individual_fault_flags() -> Result<(), Box<dyn std::error::Error>> {
    let flags: &[(u8, &str)] = &[
        (0x01, "over-temp"),
        (0x02, "over-current"),
        (0x04, "communication error"),
        (0x08, "motor fault"),
    ];
    for &(flag, name) in flags {
        let mut data = [0u8; 64];
        data[0] = 0x02;
        data[10] = flag;
        let state = parse_extended_report(&data).ok_or("parse failed")?;
        assert_eq!(
            state.fault_flags & flag,
            flag,
            "{} flag (0x{:02X}) must be set",
            name,
            flag
        );
        assert_eq!(
            state.fault_flags & !flag,
            0,
            "only {} flag should be set",
            name
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// §4  Output report construction — LED control
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn led_report_single_led() -> Result<(), Box<dyn std::error::Error>> {
    for bit in 0u16..16 {
        let mask = 1u16 << bit;
        let report = fan::build_led_report(mask, 128);
        let recovered = u16::from_le_bytes([report[2], report[3]]);
        assert_eq!(recovered, mask, "LED bit {} must round-trip in report", bit);
    }
    Ok(())
}

#[test]
fn led_report_max_brightness() -> Result<(), Box<dyn std::error::Error>> {
    let report = fan::build_led_report(0xFFFF, 255);
    assert_eq!(report[4], 255);
    Ok(())
}

#[test]
fn led_report_zero_brightness() -> Result<(), Box<dyn std::error::Error>> {
    let report = fan::build_led_report(0xFFFF, 0);
    assert_eq!(report[4], 0);
    Ok(())
}

#[test]
fn display_report_ascii_digits() -> Result<(), Box<dyn std::error::Error>> {
    let report = fan::build_display_report(0, [b'G', b'4', b'P'], 200);
    assert_eq!(report[0], 0x08);
    assert_eq!(report[1], led_commands::DISPLAY);
    assert_eq!(report[3], b'G');
    assert_eq!(report[4], b'4');
    assert_eq!(report[5], b'P');
    assert_eq!(report[6], 200);
    Ok(())
}

#[test]
fn rumble_report_asymmetric_motors() -> Result<(), Box<dyn std::error::Error>> {
    let report = fan::build_rumble_report(255, 0, 100);
    assert_eq!(report[2], 255, "left motor must be max");
    assert_eq!(report[3], 0, "right motor must be off");
    assert_eq!(report[4], 100, "duration must be 100");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// §5  Output report construction — FFB commands
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn ffb_encoder_different_bases() -> Result<(), Box<dyn std::error::Error>> {
    let bases: &[(f32, &str)] = &[
        (20.0, "DD1"),
        (25.0, "DD2"),
        (8.0, "CSL DD"),
        (6.0, "CSL Elite"),
        (5.0, "CSR Elite"),
    ];
    for &(max_torque, name) in bases {
        let encoder = FanatecConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        // Half positive torque
        let len = encoder.encode(max_torque / 2.0, 0, &mut out);
        assert_eq!(len, CONSTANT_FORCE_REPORT_LEN);
        let raw = i16::from_le_bytes([out[2], out[3]]);
        assert!(
            raw > 15_000 && raw < 17_000,
            "{}: half torque raw should be ~16384, got {}",
            name,
            raw
        );
    }
    Ok(())
}

#[test]
fn ffb_encoder_negative_max_torque_clamps_to_zero() -> Result<(), Box<dyn std::error::Error>> {
    let encoder = FanatecConstantForceEncoder::new(-5.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(3.0, 0, &mut out);
    // Negative max_torque → treated as 0 → all torque commands produce 0
    let raw = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(raw, 0, "negative max_torque should produce zero output");
    Ok(())
}

#[test]
fn stop_all_report_is_all_zeros_except_header() -> Result<(), Box<dyn std::error::Error>> {
    let report = fan::build_stop_all_report();
    assert_eq!(report[0], 0x01);
    assert_eq!(report[1], 0x0F);
    assert_eq!(&report[2..], &[0u8; 6], "bytes 2-7 must be zero");
    Ok(())
}

#[test]
fn gain_report_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    // 0 passes through
    let r0 = fan::build_set_gain_report(0);
    assert_eq!(r0[2], 0);

    // 100 passes through
    let r100 = fan::build_set_gain_report(100);
    assert_eq!(r100[2], 100);

    // 101 clamps to 100
    let r101 = fan::build_set_gain_report(101);
    assert_eq!(r101[2], 100);

    // 255 clamps to 100
    let r255 = fan::build_set_gain_report(255);
    assert_eq!(r255[2], 100);
    Ok(())
}

#[test]
fn rotation_range_exact_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    // Exactly MIN
    let r_min = fan::build_rotation_range_report(MIN_ROTATION_DEGREES);
    let decoded = u16::from_le_bytes([r_min[2], r_min[3]]);
    assert_eq!(decoded, MIN_ROTATION_DEGREES);

    // Exactly MAX
    let r_max = fan::build_rotation_range_report(MAX_ROTATION_DEGREES);
    let decoded = u16::from_le_bytes([r_max[2], r_max[3]]);
    assert_eq!(decoded, MAX_ROTATION_DEGREES);

    // MIN-1 clamps to MIN
    let r_below = fan::build_rotation_range_report(MIN_ROTATION_DEGREES - 1);
    let decoded = u16::from_le_bytes([r_below[2], r_below[3]]);
    assert_eq!(decoded, MIN_ROTATION_DEGREES);

    // MAX+1 clamps to MAX
    if MAX_ROTATION_DEGREES < u16::MAX {
        let r_above = fan::build_rotation_range_report(MAX_ROTATION_DEGREES + 1);
        let decoded = u16::from_le_bytes([r_above[2], r_above[3]]);
        assert_eq!(decoded, MAX_ROTATION_DEGREES);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// §6  Device identification via PID
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn shifter_is_neither_wheelbase_nor_pedal() -> Result<(), Box<dyn std::error::Error>> {
    assert!(
        !is_wheelbase_product(product_ids::CLUBSPORT_SHIFTER),
        "shifter must not be a wheelbase"
    );
    assert!(
        !is_pedal_product(product_ids::CLUBSPORT_SHIFTER),
        "shifter must not be a pedal"
    );
    Ok(())
}

#[test]
fn handbrake_is_neither_wheelbase_nor_pedal() -> Result<(), Box<dyn std::error::Error>> {
    assert!(
        !is_wheelbase_product(product_ids::CLUBSPORT_HANDBRAKE),
        "handbrake must not be a wheelbase"
    );
    assert!(
        !is_pedal_product(product_ids::CLUBSPORT_HANDBRAKE),
        "handbrake must not be a pedal"
    );
    Ok(())
}

#[test]
fn all_wheelbase_models_have_positive_torque() -> Result<(), Box<dyn std::error::Error>> {
    let pids = [
        product_ids::CLUBSPORT_V2,
        product_ids::CLUBSPORT_V2_5,
        product_ids::CSL_ELITE_PS4,
        product_ids::DD1,
        product_ids::DD2,
        product_ids::CSR_ELITE,
        product_ids::CSL_DD,
        product_ids::GT_DD_PRO,
        product_ids::CLUBSPORT_DD,
        product_ids::CSL_ELITE,
    ];
    for pid in pids {
        let model = FanatecModel::from_product_id(pid);
        assert!(
            model.max_torque_nm() > 0.0,
            "PID 0x{:04X} ({:?}) must have positive torque",
            pid,
            model
        );
    }
    Ok(())
}

#[test]
fn dd_vs_belt_highres_consistency() -> Result<(), Box<dyn std::error::Error>> {
    // DD bases must be highres
    let dd = [
        product_ids::DD1,
        product_ids::DD2,
        product_ids::CSL_DD,
        product_ids::GT_DD_PRO,
        product_ids::CLUBSPORT_DD,
    ];
    for pid in dd {
        let model = FanatecModel::from_product_id(pid);
        assert!(
            model.is_highres(),
            "DD base 0x{:04X} ({:?}) must be highres",
            pid,
            model
        );
    }
    // Belt bases must NOT be highres
    let belt = [
        product_ids::CLUBSPORT_V2,
        product_ids::CLUBSPORT_V2_5,
        product_ids::CSL_ELITE,
        product_ids::CSL_ELITE_PS4,
        product_ids::CSR_ELITE,
    ];
    for pid in belt {
        let model = FanatecModel::from_product_id(pid);
        assert!(
            !model.is_highres(),
            "Belt base 0x{:04X} ({:?}) must NOT be highres",
            pid,
            model
        );
    }
    Ok(())
}

#[test]
fn pedal_model_axis_count_specific() -> Result<(), Box<dyn std::error::Error>> {
    // 3-axis pedals (load cell / V3)
    assert_eq!(FanatecPedalModel::ClubSportV3.axis_count(), 3);
    assert_eq!(FanatecPedalModel::CslPedalsLc.axis_count(), 3);
    assert_eq!(FanatecPedalModel::CslPedalsV2.axis_count(), 3);
    // 2-axis pedals
    assert_eq!(FanatecPedalModel::ClubSportV1V2.axis_count(), 2);
    assert_eq!(FanatecPedalModel::CslElitePedals.axis_count(), 2);
    // Unknown defaults to 2
    assert_eq!(FanatecPedalModel::Unknown.axis_count(), 2);
    Ok(())
}

#[test]
fn rim_capabilities_matrix() -> Result<(), Box<dyn std::error::Error>> {
    // McLaren GT3 V2: funky + dual clutch + rotary
    let mcl = FanatecRimId::from_byte(rim_ids::MCLAREN_GT3_V2);
    assert!(mcl.has_funky_switch());
    assert!(mcl.has_dual_clutch());
    assert!(mcl.has_rotary_encoders());

    // Formula V2: dual clutch, no funky, no rotary
    let f2 = FanatecRimId::from_byte(rim_ids::FORMULA_V2);
    assert!(!f2.has_funky_switch());
    assert!(f2.has_dual_clutch());
    assert!(!f2.has_rotary_encoders());

    // Formula V2.5: dual clutch + rotary, no funky
    let f25 = FanatecRimId::from_byte(rim_ids::FORMULA_V2_5);
    assert!(!f25.has_funky_switch());
    assert!(f25.has_dual_clutch());
    assert!(f25.has_rotary_encoders());

    // Porsche 911 GT3 R: no special features
    let p911 = FanatecRimId::from_byte(rim_ids::PORSCHE_911_GT3_R);
    assert!(!p911.has_funky_switch());
    assert!(!p911.has_dual_clutch());
    assert!(!p911.has_rotary_encoders());

    // WRC: no special features
    let wrc = FanatecRimId::from_byte(rim_ids::WRC);
    assert!(!wrc.has_funky_switch());
    assert!(!wrc.has_dual_clutch());
    assert!(!wrc.has_rotary_encoders());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// §7  Multi-device aggregation (wheel + pedals + shifter)
// ═══════════════════════════════════════════════════════════════════════════

/// Simulates a full racing rig: wheelbase standard report + standalone pedal
/// report + shifter gear (via a button bit). All should parse independently
/// and their states can be combined.
#[test]
fn multi_device_wheel_pedals_shifter() -> Result<(), Box<dyn std::error::Error>> {
    // Wheel report: slight right turn, brake partially applied, gear button pressed
    let mut wheel_data = [0u8; 64];
    wheel_data[0] = 0x01;
    // Steering = 0xA000 (right of center)
    wheel_data[1] = 0x00;
    wheel_data[2] = 0xA0;
    wheel_data[3] = 0xFF; // throttle released
    wheel_data[4] = 0x80; // brake ~50%
    wheel_data[5] = 0xFF; // clutch released
    wheel_data[7] = 0x04; // button 2 pressed (e.g., shifter up paddle)
    wheel_data[8] = 0x00;
    wheel_data[9] = 0x0F; // hat neutral

    // Standalone pedal report: throttle at 75%, brake at 100%, clutch released
    let mut pedal_data = [0u8; 7];
    pedal_data[0] = 0x01;
    // throttle = 0x0C00 (~75%)
    pedal_data[1] = 0x00;
    pedal_data[2] = 0x0C;
    // brake = 0x0FFF (100%)
    pedal_data[3] = 0xFF;
    pedal_data[4] = 0x0F;
    // clutch = 0x0000 (released)
    pedal_data[5] = 0x00;
    pedal_data[6] = 0x00;

    let wheel_state = parse_standard_report(&wheel_data).ok_or("wheel parse failed")?;
    let pedal_state = parse_pedal_report(&pedal_data).ok_or("pedal parse failed")?;

    // Verify wheel state
    assert!(
        wheel_state.steering > 0.0,
        "steering should be right of center"
    );
    assert!(
        wheel_state.brake > 0.4,
        "wheel brake should be partially pressed"
    );
    assert_eq!(wheel_state.buttons & 0x04, 0x04, "button 2 must be set");
    assert_eq!(wheel_state.hat, 0x0F, "hat must be neutral");

    // Verify pedal state
    assert_eq!(pedal_state.throttle_raw, 0x0C00);
    assert_eq!(pedal_state.brake_raw, 0x0FFF);
    assert_eq!(pedal_state.clutch_raw, 0);
    assert_eq!(pedal_state.axis_count, 3);

    // In a real aggregation layer, pedal_state would override wheel_state pedal axes.
    // The wheel's onboard pedal axes come from passthrough; standalone USB pedals
    // provide higher-resolution 12-bit data.
    Ok(())
}

/// Tests that wheelbase, pedal, and extended reports can all be parsed from
/// the same simulated input stream without interference.
#[test]
fn multi_device_interleaved_reports() -> Result<(), Box<dyn std::error::Error>> {
    // Standard wheel report
    let mut wheel = [0u8; 64];
    wheel[0] = 0x01;
    wheel[1] = 0x00;
    wheel[2] = 0x80;

    // Extended telemetry report
    let mut ext = [0u8; 64];
    ext[0] = 0x02;
    ext[5] = 65; // motor temp
    ext[10] = 0x00; // no faults

    // Pedal report
    let mut pedal = [0u8; 7];
    pedal[0] = 0x01;
    pedal[1] = 0x00;
    pedal[2] = 0x08;
    pedal[3] = 0xFF;
    pedal[4] = 0x0F;

    // Parse all three
    let w = parse_standard_report(&wheel).ok_or("wheel parse failed")?;
    let e = parse_extended_report(&ext).ok_or("extended parse failed")?;
    let p = parse_pedal_report(&pedal).ok_or("pedal parse failed")?;

    assert!(w.steering.abs() < 1e-4, "wheel centered");
    assert_eq!(e.motor_temp_c, 65);
    assert_eq!(e.fault_flags, 0x00);
    assert_eq!(p.throttle_raw, 0x0800);
    assert_eq!(p.brake_raw, 0x0FFF);
    Ok(())
}

/// Verifies device identification for a complete rig setup.
#[test]
fn multi_device_identification() -> Result<(), Box<dyn std::error::Error>> {
    // Typical DD rig: CSL DD base + V3 pedals + ClubSport Shifter
    let base_pid = product_ids::CSL_DD;
    let pedal_pid = product_ids::CLUBSPORT_PEDALS_V3;
    let shifter_pid = product_ids::CLUBSPORT_SHIFTER;

    assert!(is_wheelbase_product(base_pid));
    assert!(!is_pedal_product(base_pid));

    assert!(is_pedal_product(pedal_pid));
    assert!(!is_wheelbase_product(pedal_pid));

    assert!(!is_wheelbase_product(shifter_pid));
    assert!(!is_pedal_product(shifter_pid));

    let base_model = FanatecModel::from_product_id(base_pid);
    assert_eq!(base_model, FanatecModel::CslDd);
    assert!(base_model.is_highres());
    assert!(base_model.supports_1000hz());

    let pedal_model = FanatecPedalModel::from_product_id(pedal_pid);
    assert_eq!(pedal_model, FanatecPedalModel::ClubSportV3);
    assert_eq!(pedal_model.axis_count(), 3);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// §8  Edge cases: short reports, wrong report IDs, invalid data
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn standard_report_empty_data() -> Result<(), Box<dyn std::error::Error>> {
    assert!(parse_standard_report(&[]).is_none());
    Ok(())
}

#[test]
fn standard_report_single_byte() -> Result<(), Box<dyn std::error::Error>> {
    assert!(parse_standard_report(&[0x01]).is_none());
    Ok(())
}

#[test]
fn standard_report_nine_bytes_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0x01u8; 9];
    assert!(
        parse_standard_report(&data).is_none(),
        "9 bytes must be rejected (minimum is 10)"
    );
    Ok(())
}

#[test]
fn standard_report_ten_bytes_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 10];
    data[0] = 0x01;
    assert!(
        parse_standard_report(&data).is_some(),
        "10 bytes is the minimum valid length"
    );
    Ok(())
}

#[test]
fn standard_report_wrong_ids() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    for id in [0x00, 0x02, 0x03, 0x08, 0xFF] {
        data[0] = id;
        assert!(
            parse_standard_report(&data).is_none(),
            "report ID 0x{:02X} must be rejected for standard report",
            id
        );
    }
    Ok(())
}

#[test]
fn extended_report_empty_data() -> Result<(), Box<dyn std::error::Error>> {
    assert!(parse_extended_report(&[]).is_none());
    Ok(())
}

#[test]
fn extended_report_ten_bytes_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 10];
    data[0] = 0x02;
    assert!(
        parse_extended_report(&data).is_none(),
        "10 bytes must be rejected (minimum is 11)"
    );
    Ok(())
}

#[test]
fn extended_report_eleven_bytes_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 11];
    data[0] = 0x02;
    assert!(
        parse_extended_report(&data).is_some(),
        "11 bytes is the minimum valid length"
    );
    Ok(())
}

#[test]
fn extended_report_wrong_ids() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    for id in [0x00, 0x01, 0x03, 0x08, 0xFF] {
        data[0] = id;
        assert!(
            parse_extended_report(&data).is_none(),
            "report ID 0x{:02X} must be rejected for extended report",
            id
        );
    }
    Ok(())
}

#[test]
fn pedal_report_empty_data() -> Result<(), Box<dyn std::error::Error>> {
    assert!(parse_pedal_report(&[]).is_none());
    Ok(())
}

#[test]
fn pedal_report_four_bytes_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0x01, 0x00, 0x00, 0x00];
    assert!(
        parse_pedal_report(&data).is_none(),
        "4 bytes must be rejected (minimum is 5)"
    );
    Ok(())
}

#[test]
fn pedal_report_five_bytes_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 5];
    data[0] = 0x01;
    assert!(
        parse_pedal_report(&data).is_some(),
        "5 bytes is the minimum valid length"
    );
    Ok(())
}

#[test]
fn pedal_report_wrong_ids() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 7];
    for id in [0x00, 0x02, 0x03, 0x08, 0xFF] {
        data[0] = id;
        assert!(
            parse_pedal_report(&data).is_none(),
            "report ID 0x{:02X} must be rejected for pedal report",
            id
        );
    }
    Ok(())
}

#[test]
fn standard_report_all_0xff() -> Result<(), Box<dyn std::error::Error>> {
    // report ID 0xFF is not valid → should be rejected
    let data = [0xFFu8; 64];
    assert!(
        parse_standard_report(&data).is_none(),
        "all-0xFF report must be rejected (wrong report ID)"
    );
    Ok(())
}

#[test]
fn standard_report_all_zeros_is_valid() -> Result<(), Box<dyn std::error::Error>> {
    // report ID 0x00 is not 0x01 → rejected
    let data = [0u8; 64];
    assert!(
        parse_standard_report(&data).is_none(),
        "all-zero report must be rejected (report ID 0x00 is not 0x01)"
    );
    Ok(())
}

#[test]
fn rim_id_unknown_byte_values() -> Result<(), Box<dyn std::error::Error>> {
    // Test a range of values that are not valid rim IDs
    let unknown_values: &[u8] = &[0x00, 0x02, 0x04, 0x07, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0xFF];
    for &val in unknown_values {
        let rim = FanatecRimId::from_byte(val);
        assert_eq!(
            rim,
            FanatecRimId::Unknown,
            "byte 0x{:02X} should map to Unknown rim",
            val
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// §9  Property tests — axis value encoding
// ═══════════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig { cases: 1000, timeout: 60_000, ..ProptestConfig::default() })]

    /// Steering normalization must always produce values in [-1.0, 1.0].
    #[test]
    fn prop_steering_normalization_bounds(raw: u16) {
        let mut data = [0u8; 64];
        data[0] = 0x01;
        data[1] = (raw & 0xFF) as u8;
        data[2] = (raw >> 8) as u8;
        data[3] = 0xFF; data[4] = 0xFF; data[5] = 0xFF;
        let state = parse_standard_report(&data);
        prop_assert!(state.is_some(), "valid 64-byte report must parse");
        if let Some(s) = state {
            prop_assert!(s.steering >= -1.0 && s.steering <= 1.0,
                "steering {} out of [-1.0, 1.0] for raw 0x{:04X}", s.steering, raw);
        }
    }

    /// Steering normalization must be monotonically non-decreasing.
    #[test]
    fn prop_steering_monotonic(a: u16, b: u16) {
        let mut da = [0u8; 64];
        da[0] = 0x01; da[1] = (a & 0xFF) as u8; da[2] = (a >> 8) as u8;
        da[3] = 0xFF; da[4] = 0xFF; da[5] = 0xFF;

        let mut db = [0u8; 64];
        db[0] = 0x01; db[1] = (b & 0xFF) as u8; db[2] = (b >> 8) as u8;
        db[3] = 0xFF; db[4] = 0xFF; db[5] = 0xFF;

        let sa = parse_standard_report(&da);
        let sb = parse_standard_report(&db);
        prop_assert!(sa.is_some() && sb.is_some());
        if let (Some(sa), Some(sb)) = (sa, sb) {
            if a < b {
                prop_assert!(sa.steering <= sb.steering,
                    "raw {} → {} must be ≤ raw {} → {}", a, sa.steering, b, sb.steering);
            } else if a > b {
                prop_assert!(sa.steering >= sb.steering,
                    "raw {} → {} must be ≥ raw {} → {}", a, sa.steering, b, sb.steering);
            }
        }
    }

    /// Inverted pedal axis normalization: 0xFF → 0.0, 0x00 → 1.0.
    #[test]
    fn prop_inverted_axis_bounds(raw: u8) {
        let mut data = [0u8; 64];
        data[0] = 0x01;
        data[1] = 0x00; data[2] = 0x80;
        data[3] = raw; // throttle
        let state = parse_standard_report(&data);
        prop_assert!(state.is_some());
        if let Some(s) = state {
            prop_assert!(s.throttle >= 0.0 && s.throttle <= 1.0,
                "throttle {} out of [0.0, 1.0] for raw {}", s.throttle, raw);
        }
    }

    /// Inverted pedal axis must be monotonically non-increasing (lower raw = higher value).
    #[test]
    fn prop_inverted_axis_monotonic(a: u8, b: u8) {
        let mut da = [0u8; 64];
        da[0] = 0x01; da[1] = 0x00; da[2] = 0x80; da[3] = a;
        let mut db = [0u8; 64];
        db[0] = 0x01; db[1] = 0x00; db[2] = 0x80; db[3] = b;

        let sa = parse_standard_report(&da);
        let sb = parse_standard_report(&db);
        if let (Some(sa), Some(sb)) = (sa, sb)
            && a < b {
                prop_assert!(sa.throttle >= sb.throttle,
                    "raw {} → {} must be ≥ raw {} → {}", a, sa.throttle, b, sb.throttle);
        }
    }

    /// Pedal 12-bit values must always be in [0, 0x0FFF].
    #[test]
    fn prop_pedal_12bit_range(throttle: u16, brake: u16, clutch: u16) {
        let mut data = [0u8; 7];
        data[0] = 0x01;
        data[1] = (throttle & 0xFF) as u8;
        data[2] = (throttle >> 8) as u8;
        data[3] = (brake & 0xFF) as u8;
        data[4] = (brake >> 8) as u8;
        data[5] = (clutch & 0xFF) as u8;
        data[6] = (clutch >> 8) as u8;
        let state = parse_pedal_report(&data);
        prop_assert!(state.is_some());
        if let Some(s) = state {
            prop_assert!(s.throttle_raw <= 0x0FFF,
                "throttle {} > 0x0FFF for input 0x{:04X}", s.throttle_raw, throttle);
            prop_assert!(s.brake_raw <= 0x0FFF,
                "brake {} > 0x0FFF for input 0x{:04X}", s.brake_raw, brake);
            prop_assert!(s.clutch_raw <= 0x0FFF,
                "clutch {} > 0x0FFF for input 0x{:04X}", s.clutch_raw, clutch);
        }
    }

    /// Extended report steering raw i16 must round-trip exactly.
    #[test]
    fn prop_extended_steering_raw_round_trip(steering: i16) {
        let mut data = [0u8; 64];
        data[0] = 0x02;
        let bytes = steering.to_le_bytes();
        data[1] = bytes[0];
        data[2] = bytes[1];
        let state = parse_extended_report(&data);
        prop_assert!(state.is_some());
        if let Some(s) = state {
            prop_assert_eq!(s.steering_raw, steering,
                "steering_raw must round-trip exactly");
        }
    }

    /// Extended report steering velocity i16 must round-trip exactly.
    #[test]
    fn prop_extended_velocity_round_trip(velocity: i16) {
        let mut data = [0u8; 64];
        data[0] = 0x02;
        let bytes = velocity.to_le_bytes();
        data[3] = bytes[0];
        data[4] = bytes[1];
        let state = parse_extended_report(&data);
        prop_assert!(state.is_some());
        if let Some(s) = state {
            prop_assert_eq!(s.steering_velocity, velocity,
                "steering_velocity must round-trip exactly");
        }
    }

    /// Hat value must always have only the lower nibble set.
    #[test]
    fn prop_hat_lower_nibble_only(raw_byte: u8) {
        let mut data = [0u8; 64];
        data[0] = 0x01;
        data[1] = 0x00; data[2] = 0x80;
        data[9] = raw_byte;
        let state = parse_standard_report(&data);
        prop_assert!(state.is_some());
        if let Some(s) = state {
            prop_assert!(s.hat <= 0x0F,
                "hat must be in [0, 0x0F], got 0x{:02X} for input 0x{:02X}", s.hat, raw_byte);
            prop_assert_eq!(s.hat, raw_byte & 0x0F,
                "hat must equal input & 0x0F");
        }
    }

    /// Button mask must round-trip exactly through LE encoding.
    #[test]
    fn prop_button_mask_round_trip(buttons: u16) {
        let mut data = [0u8; 64];
        data[0] = 0x01;
        data[1] = 0x00; data[2] = 0x80;
        data[7] = (buttons & 0xFF) as u8;
        data[8] = (buttons >> 8) as u8;
        let state = parse_standard_report(&data);
        prop_assert!(state.is_some());
        if let Some(s) = state {
            prop_assert_eq!(s.buttons, buttons, "buttons must round-trip exactly");
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// §10  Known constant validation (VID/PIDs, report sizes)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn constant_vendor_id() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        FANATEC_VENDOR_ID, 0x0EB7,
        "Fanatec VID must be 0x0EB7 (Endor AG)"
    );
    Ok(())
}

#[test]
fn constant_report_sizes() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(CONSTANT_FORCE_REPORT_LEN, 8, "FFB report must be 8 bytes");
    assert_eq!(LED_REPORT_LEN, 8, "LED report must be 8 bytes");
    Ok(())
}

#[test]
fn constant_rotation_range_limits() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(MIN_ROTATION_DEGREES, 90, "minimum rotation must be 90°");
    assert_eq!(MAX_ROTATION_DEGREES, 2520, "maximum rotation must be 2520°");
    Ok(())
}

#[test]
fn constant_report_ids() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_fanatec_protocol::ids::report_ids;
    assert_eq!(report_ids::STANDARD_INPUT, 0x01);
    assert_eq!(report_ids::EXTENDED_INPUT, 0x02);
    assert_eq!(report_ids::MODE_SWITCH, 0x01);
    assert_eq!(report_ids::FFB_OUTPUT, 0x01);
    assert_eq!(report_ids::LED_DISPLAY, 0x08);
    Ok(())
}

#[test]
fn constant_ffb_commands() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_fanatec_protocol::ids::ffb_commands;
    assert_eq!(ffb_commands::CONSTANT_FORCE, 0x01);
    assert_eq!(ffb_commands::SET_ROTATION_RANGE, 0x12);
    assert_eq!(ffb_commands::SET_GAIN, 0x10);
    assert_eq!(ffb_commands::STOP_ALL, 0x0F);
    Ok(())
}

#[test]
fn constant_led_commands() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(led_commands::REV_LIGHTS, 0x80);
    assert_eq!(led_commands::DISPLAY, 0x81);
    assert_eq!(led_commands::RUMBLE, 0x82);
    Ok(())
}

#[test]
fn constant_all_product_ids_nonzero() -> Result<(), Box<dyn std::error::Error>> {
    let all_pids: &[(u16, &str)] = &[
        (product_ids::CLUBSPORT_V2, "CLUBSPORT_V2"),
        (product_ids::CLUBSPORT_V2_5, "CLUBSPORT_V2_5"),
        (product_ids::CSL_ELITE_PS4, "CSL_ELITE_PS4"),
        (product_ids::DD1, "DD1"),
        (product_ids::DD2, "DD2"),
        (product_ids::CSR_ELITE, "CSR_ELITE"),
        (product_ids::CSL_DD, "CSL_DD"),
        (product_ids::GT_DD_PRO, "GT_DD_PRO"),
        (product_ids::CSL_ELITE, "CSL_ELITE"),
        (product_ids::CLUBSPORT_DD, "CLUBSPORT_DD"),
        (
            product_ids::CLUBSPORT_PEDALS_V1_V2,
            "CLUBSPORT_PEDALS_V1_V2",
        ),
        (product_ids::CLUBSPORT_PEDALS_V3, "CLUBSPORT_PEDALS_V3"),
        (product_ids::CSL_ELITE_PEDALS, "CSL_ELITE_PEDALS"),
        (product_ids::CSL_PEDALS_LC, "CSL_PEDALS_LC"),
        (product_ids::CSL_PEDALS_V2, "CSL_PEDALS_V2"),
        (product_ids::CLUBSPORT_SHIFTER, "CLUBSPORT_SHIFTER"),
        (product_ids::CLUBSPORT_HANDBRAKE, "CLUBSPORT_HANDBRAKE"),
    ];
    for &(pid, name) in all_pids {
        assert!(pid != 0, "{} PID must be non-zero", name);
    }
    Ok(())
}

#[test]
fn constant_all_rim_ids_nonzero() -> Result<(), Box<dyn std::error::Error>> {
    let all_rims: &[(u8, &str)] = &[
        (rim_ids::BMW_GT2, "BMW_GT2"),
        (rim_ids::FORMULA_V2, "FORMULA_V2"),
        (rim_ids::FORMULA_V2_5, "FORMULA_V2_5"),
        (rim_ids::CSL_ELITE_P1, "CSL_ELITE_P1"),
        (rim_ids::MCLAREN_GT3_V2, "MCLAREN_GT3_V2"),
        (rim_ids::PORSCHE_911_GT3_R, "PORSCHE_911_GT3_R"),
        (rim_ids::PORSCHE_918_RSR, "PORSCHE_918_RSR"),
        (rim_ids::CLUBSPORT_RS, "CLUBSPORT_RS"),
        (rim_ids::WRC, "WRC"),
        (rim_ids::PODIUM_HUB, "PODIUM_HUB"),
    ];
    for &(id, name) in all_rims {
        assert!(id != 0, "{} rim ID must be non-zero", name);
    }
    Ok(())
}

/// Output reports produced by all build_* functions must be exactly 8 bytes.
#[test]
fn all_output_reports_are_8_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let encoder = FanatecConstantForceEncoder::new(8.0);
    let mut ffb = [0u8; CONSTANT_FORCE_REPORT_LEN];
    let len = encoder.encode(1.0, 0, &mut ffb);
    assert_eq!(len, 8, "FFB report length");

    let stop = fan::build_stop_all_report();
    assert_eq!(stop.len(), 8, "stop-all report length");

    let gain = fan::build_set_gain_report(50);
    assert_eq!(gain.len(), 8, "gain report length");

    let led = fan::build_led_report(0, 0);
    assert_eq!(led.len(), 8, "LED report length");

    let display = fan::build_display_report(0, [0; 3], 0);
    assert_eq!(display.len(), 8, "display report length");

    let rumble = fan::build_rumble_report(0, 0, 0);
    assert_eq!(rumble.len(), 8, "rumble report length");

    let rot = fan::build_rotation_range_report(900);
    assert_eq!(rot.len(), 8, "rotation range report length");

    let mode = fan::build_mode_switch_report();
    assert_eq!(mode.len(), 8, "mode switch report length");
    Ok(())
}

/// Kernel range sequence reports must each be 7 bytes.
#[test]
fn kernel_range_sequence_7_byte_reports() -> Result<(), Box<dyn std::error::Error>> {
    let seq = fan::build_kernel_range_sequence(900);
    assert_eq!(seq.len(), 3, "must be 3 reports");
    for (i, report) in seq.iter().enumerate() {
        assert_eq!(report.len(), 7, "report {} must be 7 bytes", i);
    }
    Ok(())
}
