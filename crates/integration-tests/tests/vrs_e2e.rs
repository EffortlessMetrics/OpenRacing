//! BDD end-to-end tests for the VRS DirectForce Pro protocol stack.
//!
//! Each test follows a Given/When/Then pattern to verify observable
//! hardware-ready behaviours without real USB hardware.
//! VRS uses encoder structs (no DeviceWriter trait), so all tests exercise
//! the pure encoding/decoding API directly.

use racing_wheel_hid_vrs_protocol::{
    CONSTANT_FORCE_REPORT_LEN, DAMPER_REPORT_LEN, FRICTION_REPORT_LEN, SPRING_REPORT_LEN,
    VRS_PRODUCT_ID, VRS_VENDOR_ID, VrsConstantForceEncoder, VrsDamperEncoder,
    VrsFrictionEncoder, VrsSpringEncoder, identify_device, parse_input_report, product_ids,
};
use racing_wheel_hid_vrs_protocol::types::VrsDeviceCategory;

// ─── Scenario 1: zero torque via encode_zero ────────────────────────────────

#[test]
fn given_dfp_encoder_when_encode_zero_then_magnitude_bytes_are_zero(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: a DFP constant-force encoder at 20 Nm
    let encoder = VrsConstantForceEncoder::new(20.0);
    let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];

    // When: encoding an explicit zero-force report
    let len = encoder.encode_zero(&mut buf);

    // Then: magnitude bytes (3-4) are zero and length is correct
    assert_eq!(len, CONSTANT_FORCE_REPORT_LEN);
    let mag = i16::from_le_bytes([buf[3], buf[4]]);
    assert_eq!(mag, 0, "encode_zero must produce zero magnitude");
    Ok(())
}

// ─── Scenario 2: full-scale positive torque saturates at +10000 ─────────────

#[test]
fn given_dfp_encoder_when_torque_exceeds_max_then_saturates_positive(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: a DFP encoder at 20 Nm
    let encoder = VrsConstantForceEncoder::new(20.0);
    let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];

    // When: encoding torque well beyond max
    let _ = encoder.encode(100.0, &mut buf);

    // Then: magnitude clamps to +10000
    let mag = i16::from_le_bytes([buf[3], buf[4]]);
    assert_eq!(mag, 10000, "positive torque beyond max must saturate at +10000");
    Ok(())
}

// ─── Scenario 3: full-scale negative torque saturates at -10000 ─────────────

#[test]
fn given_dfp_encoder_when_torque_exceeds_max_negative_then_saturates_negative(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: a DFP encoder at 20 Nm
    let encoder = VrsConstantForceEncoder::new(20.0);
    let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];

    // When: encoding large negative torque
    let _ = encoder.encode(-100.0, &mut buf);

    // Then: magnitude clamps to -10000
    let mag = i16::from_le_bytes([buf[3], buf[4]]);
    assert_eq!(mag, -10000, "negative torque beyond max must saturate at -10000");
    Ok(())
}

// ─── Scenario 4: sign preservation (positive vs negative) ───────────────────

#[test]
fn given_dfp_encoder_when_positive_and_negative_torque_then_sign_preserved(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: a DFP encoder at 20 Nm
    let encoder = VrsConstantForceEncoder::new(20.0);
    let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];

    // When: encoding +10 Nm
    let _ = encoder.encode(10.0, &mut buf);
    let mag_pos = i16::from_le_bytes([buf[3], buf[4]]);

    // When: encoding -10 Nm
    let _ = encoder.encode(-10.0, &mut buf);
    let mag_neg = i16::from_le_bytes([buf[3], buf[4]]);

    // Then: positive yields positive magnitude, negative yields negative
    assert!(mag_pos > 0, "positive torque must produce positive magnitude");
    assert!(mag_neg < 0, "negative torque must produce negative magnitude");
    assert_eq!(mag_pos, -mag_neg, "magnitudes must be symmetric");
    Ok(())
}

// ─── Scenario 5: report byte layout (PIDFF header) ─────────────────────────

#[test]
fn given_dfp_encoder_when_encoding_then_pidff_header_correct(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: a DFP encoder at 20 Nm
    let encoder = VrsConstantForceEncoder::new(20.0);
    let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];

    // When: encoding any torque
    let _ = encoder.encode(5.0, &mut buf);

    // Then: byte 0 = report ID 0x11, byte 1 = effect block index 1, byte 2 = 0
    assert_eq!(buf[0], 0x11, "report ID must be 0x11 (constant force)");
    assert_eq!(buf[1], 0x01, "effect block index must be 1");
    assert_eq!(buf[2], 0x00, "effect block index high byte must be 0");
    // Reserved bytes 5-7 must be zero
    assert_eq!(buf[5], 0x00, "reserved byte 5 must be 0");
    assert_eq!(buf[6], 0x00, "reserved byte 6 must be 0");
    assert_eq!(buf[7], 0x00, "reserved byte 7 must be 0");
    Ok(())
}

// ─── Scenario 6: multiple encoder instances with different max torque ───────

#[test]
fn given_two_encoders_with_different_max_when_same_torque_then_different_magnitude(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: DFP (20 Nm) and DFP V2 (25 Nm) encoders
    let enc_20 = VrsConstantForceEncoder::new(20.0);
    let enc_25 = VrsConstantForceEncoder::new(25.0);
    let mut buf_20 = [0u8; CONSTANT_FORCE_REPORT_LEN];
    let mut buf_25 = [0u8; CONSTANT_FORCE_REPORT_LEN];

    // When: encoding 10 Nm on each
    let _ = enc_20.encode(10.0, &mut buf_20);
    let _ = enc_25.encode(10.0, &mut buf_25);

    // Then: 10/20 = 50% → 5000, 10/25 = 40% → 4000
    let mag_20 = i16::from_le_bytes([buf_20[3], buf_20[4]]);
    let mag_25 = i16::from_le_bytes([buf_25[3], buf_25[4]]);
    assert_eq!(mag_20, 5000);
    assert_eq!(mag_25, 4000);
    assert!(
        mag_20 > mag_25,
        "same torque on lower-max encoder must produce larger magnitude"
    );
    Ok(())
}

// ─── Scenario 7: device identification for known product IDs ────────────────

#[test]
fn given_known_product_ids_when_identified_then_correct_metadata(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given/When: identifying DirectForce Pro
    let dfp = identify_device(product_ids::DIRECTFORCE_PRO);
    assert_eq!(dfp.product_id, 0xA355);
    assert_eq!(dfp.category, VrsDeviceCategory::Wheelbase);
    assert!(dfp.supports_ffb);
    assert_eq!(dfp.max_torque_nm, Some(20.0));

    // Given/When: identifying DirectForce Pro V2
    let dfp_v2 = identify_device(product_ids::DIRECTFORCE_PRO_V2);
    assert_eq!(dfp_v2.product_id, 0xA356);
    assert_eq!(dfp_v2.category, VrsDeviceCategory::Wheelbase);
    assert!(dfp_v2.supports_ffb);
    assert_eq!(dfp_v2.max_torque_nm, Some(25.0));

    // Given/When: identifying R295
    let r295 = identify_device(product_ids::R295);
    assert_eq!(r295.product_id, 0xA44C);
    assert_eq!(r295.category, VrsDeviceCategory::Wheelbase);
    assert!(r295.supports_ffb);

    Ok(())
}

// ─── Scenario 8: unknown product ID returns Unknown category ────────────────

#[test]
fn given_unknown_product_id_when_identified_then_unknown_category(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: an unrecognised PID
    let unknown = identify_device(0xFFFF);

    // Then: category is Unknown, no FFB, no torque
    assert_eq!(unknown.category, VrsDeviceCategory::Unknown);
    assert!(!unknown.supports_ffb);
    assert_eq!(unknown.max_torque_nm, None);
    Ok(())
}

// ─── Scenario 9: encoder monotonicity (larger torque → larger magnitude) ────

#[test]
fn given_dfp_encoder_when_increasing_torque_then_magnitude_monotonic(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: a DFP encoder at 20 Nm
    let encoder = VrsConstantForceEncoder::new(20.0);
    let torques: [f32; 5] = [0.0, 5.0, 10.0, 15.0, 20.0];
    let mut prev_mag: i16 = i16::MIN;

    for torque in torques {
        let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];
        // When: encoding increasing torques
        let _ = encoder.encode(torque, &mut buf);
        let mag = i16::from_le_bytes([buf[3], buf[4]]);

        // Then: each magnitude >= previous
        assert!(
            mag >= prev_mag,
            "magnitude must be monotonically non-decreasing: {mag} < {prev_mag} at {torque} Nm"
        );
        prev_mag = mag;
    }
    Ok(())
}

// ─── Scenario 10: spring encoder basic validation ───────────────────────────

#[test]
fn given_spring_encoder_when_encoding_then_correct_report_layout(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: a spring encoder
    let encoder = VrsSpringEncoder::new(20.0);
    let mut buf = [0u8; SPRING_REPORT_LEN];

    // When: encoding a spring effect
    let len = encoder.encode(5000, 1000, 0, 500, &mut buf);

    // Then: report ID = 0x19, effect block index = 1, correct length
    assert_eq!(len, SPRING_REPORT_LEN);
    assert_eq!(buf[0], 0x19, "spring report ID must be 0x19");
    assert_eq!(buf[1], 0x01, "effect block index must be 1");
    // Coefficient = 5000 little-endian
    let coeff = u16::from_le_bytes([buf[2], buf[3]]);
    assert_eq!(coeff, 5000);
    // Steering position = 1000 little-endian
    let steering = i16::from_le_bytes([buf[4], buf[5]]);
    assert_eq!(steering, 1000);
    Ok(())
}

// ─── Scenario 11: damper encoder basic validation ───────────────────────────

#[test]
fn given_damper_encoder_when_encoding_then_correct_report_layout(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: a damper encoder
    let encoder = VrsDamperEncoder::new(20.0);
    let mut buf = [0u8; DAMPER_REPORT_LEN];

    // When: encoding a damper effect
    let len = encoder.encode(7500, 5000, &mut buf);

    // Then: report ID = 0x1A, effect block index = 1
    assert_eq!(len, DAMPER_REPORT_LEN);
    assert_eq!(buf[0], 0x1A, "damper report ID must be 0x1A");
    assert_eq!(buf[1], 0x01);
    let coeff = u16::from_le_bytes([buf[2], buf[3]]);
    assert_eq!(coeff, 7500);
    let velocity = u16::from_le_bytes([buf[4], buf[5]]);
    assert_eq!(velocity, 5000);
    Ok(())
}

// ─── Scenario 12: friction encoder basic validation ─────────────────────────

#[test]
fn given_friction_encoder_when_encoding_then_correct_report_layout(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: a friction encoder
    let encoder = VrsFrictionEncoder::new(20.0);
    let mut buf = [0u8; FRICTION_REPORT_LEN];

    // When: encoding a friction effect
    let len = encoder.encode(3000, 2000, &mut buf);

    // Then: report ID = 0x1B, effect block index = 1
    assert_eq!(len, FRICTION_REPORT_LEN);
    assert_eq!(buf[0], 0x1B, "friction report ID must be 0x1B");
    assert_eq!(buf[1], 0x01);
    let coeff = u16::from_le_bytes([buf[2], buf[3]]);
    assert_eq!(coeff, 3000);
    let velocity = u16::from_le_bytes([buf[4], buf[5]]);
    assert_eq!(velocity, 2000);
    Ok(())
}

// ─── Scenario 13: product ID constants match expected hex values ────────────

#[test]
fn given_product_id_constants_when_checked_then_match_expected_hex(
) -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(VRS_VENDOR_ID, 0x0483, "VRS vendor ID must be 0x0483 (STMicroelectronics)");
    assert_eq!(VRS_PRODUCT_ID, 0xA355, "default VRS product ID must be DFP 0xA355");
    assert_eq!(product_ids::DIRECTFORCE_PRO, 0xA355);
    assert_eq!(product_ids::DIRECTFORCE_PRO_V2, 0xA356);
    assert_eq!(product_ids::R295, 0xA44C);
    assert_eq!(product_ids::PEDALS, 0xA3BE);
    Ok(())
}

// ─── Scenario 14: input report parsing round-trip ───────────────────────────

#[test]
fn given_center_steering_input_when_parsed_then_steering_near_zero(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: a 64-byte input report with center steering (0x0000)
    let mut data = vec![0u8; 64];
    data[0] = 0x00;
    data[1] = 0x00;

    // When: parsed
    let state = parse_input_report(&data).ok_or("parse_input_report returned None")?;

    // Then: steering is near zero
    assert!(
        state.steering.abs() < 0.001,
        "center steering must parse near 0.0, got {}",
        state.steering
    );
    Ok(())
}

// ─── Scenario 15: input report too short returns None ───────────────────────

#[test]
fn given_short_input_report_when_parsed_then_returns_none(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: a report shorter than the minimum 17 bytes
    let data = vec![0u8; 10];

    // When: parsed
    let result = parse_input_report(&data);

    // Then: returns None
    assert!(result.is_none(), "input shorter than 17 bytes must return None");
    Ok(())
}

// ─── Scenario 16: non-wheelbase devices report no FFB support ───────────────

#[test]
fn given_pedals_product_id_when_identified_then_no_ffb(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given/When: identifying pedals, handbrake, shifter
    let pedals = identify_device(product_ids::PEDALS);
    let handbrake = identify_device(product_ids::HANDBRAKE);
    let shifter = identify_device(product_ids::SHIFTER);

    // Then: none support FFB
    assert!(!pedals.supports_ffb, "pedals must not support FFB");
    assert!(!handbrake.supports_ffb, "handbrake must not support FFB");
    assert!(!shifter.supports_ffb, "shifter must not support FFB");
    assert_eq!(pedals.max_torque_nm, None);
    assert_eq!(handbrake.max_torque_nm, None);
    assert_eq!(shifter.max_torque_nm, None);
    Ok(())
}

// ─── Scenario 17: spring/damper/friction encode_zero produce neutral output ─

#[test]
fn given_condition_encoders_when_encode_zero_then_coefficients_are_zero(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: spring, damper, friction encoders
    let spring = VrsSpringEncoder::new(20.0);
    let damper = VrsDamperEncoder::new(20.0);
    let friction = VrsFrictionEncoder::new(20.0);

    // When: encoding zero for each
    let mut sbuf = [0u8; SPRING_REPORT_LEN];
    let _ = spring.encode_zero(&mut sbuf);
    let mut dbuf = [0u8; DAMPER_REPORT_LEN];
    let _ = damper.encode_zero(&mut dbuf);
    let mut fbuf = [0u8; FRICTION_REPORT_LEN];
    let _ = friction.encode_zero(&mut fbuf);

    // Then: coefficient bytes are all zero
    assert_eq!(u16::from_le_bytes([sbuf[2], sbuf[3]]), 0, "spring zero coeff");
    assert_eq!(u16::from_le_bytes([dbuf[2], dbuf[3]]), 0, "damper zero coeff");
    assert_eq!(u16::from_le_bytes([fbuf[2], fbuf[3]]), 0, "friction zero coeff");
    Ok(())
}

// ─── Scenario 18: encode_zero header matches encode header ──────────────────

#[test]
fn given_constant_force_encoder_when_encode_zero_then_header_matches_encode(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: a DFP encoder
    let encoder = VrsConstantForceEncoder::new(20.0);
    let mut buf_zero = [0u8; CONSTANT_FORCE_REPORT_LEN];
    let mut buf_enc = [0u8; CONSTANT_FORCE_REPORT_LEN];

    // When: encoding via both paths
    let _ = encoder.encode_zero(&mut buf_zero);
    let _ = encoder.encode(0.0, &mut buf_enc);

    // Then: report ID and effect block index match
    assert_eq!(buf_zero[0], buf_enc[0], "report IDs must match");
    assert_eq!(buf_zero[1], buf_enc[1], "effect block indices must match");
    // Both should produce zero magnitude
    let mag_zero = i16::from_le_bytes([buf_zero[3], buf_zero[4]]);
    let mag_enc = i16::from_le_bytes([buf_enc[3], buf_enc[4]]);
    assert_eq!(mag_zero, mag_enc, "both zero paths must produce same magnitude");
    Ok(())
}
