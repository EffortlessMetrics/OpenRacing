//! Protocol round-trip integration tests for Fanatec, Logitech, and Thrustmaster.
//!
//! Verifies that constant-force encoding produces correct wire bytes and that
//! encoded torque values round-trip through encode→extract→decode with minimal
//! quantization error across all three vendor protocol stacks.

use racing_wheel_hid_fanatec_protocol::CONSTANT_FORCE_REPORT_LEN as FANATEC_REPORT_LEN;
use racing_wheel_hid_fanatec_protocol::FanatecConstantForceEncoder;
use racing_wheel_hid_fanatec_protocol::ids::report_ids as fanatec_ids;

use racing_wheel_hid_logitech_protocol::CONSTANT_FORCE_REPORT_LEN as LOGITECH_REPORT_LEN;
use racing_wheel_hid_logitech_protocol::LogitechConstantForceEncoder;

use racing_wheel_hid_thrustmaster_protocol::EFFECT_REPORT_LEN as THRUSTMASTER_REPORT_LEN;
use racing_wheel_hid_thrustmaster_protocol::ThrustmasterConstantForceEncoder;
use racing_wheel_hid_thrustmaster_protocol::output::report_ids as tm_ids;

/// Extract the signed 16-bit LE force value from bytes 2–3 of an encoded report.
fn extract_force_i16(buf: &[u8]) -> i16 {
    i16::from_le_bytes([buf[2], buf[3]])
}

// ─── Fanatec constant-force encoding round-trips ──────────────────────────────

#[test]
fn fanatec_encode_half_torque_produces_correct_wire_bytes() -> Result<(), Box<dyn std::error::Error>>
{
    // Given: encoder at 8 Nm max
    let encoder = FanatecConstantForceEncoder::new(8.0);
    let mut buf = [0u8; FANATEC_REPORT_LEN];

    // When: encoding 4.0 Nm (50% of max)
    let written = encoder.encode(4.0, 0, &mut buf);

    // Then: full report written with correct header
    assert_eq!(written, FANATEC_REPORT_LEN);
    assert_eq!(buf[0], fanatec_ids::FFB_OUTPUT, "byte 0 must be FFB_OUTPUT");
    assert_eq!(buf[1], 0x01, "byte 1 must be CONSTANT_FORCE command");

    // Then: force = round(0.5 × 32767) = 16384
    let force = extract_force_i16(&buf);
    assert_eq!(force, 16384, "50% torque must encode to 16384");

    // Then: trailing bytes are zero
    assert_eq!(&buf[4..], &[0u8; 4], "trailing bytes must be zero");

    Ok(())
}

#[test]
fn fanatec_encode_zero_torque_produces_zero_force() -> Result<(), Box<dyn std::error::Error>> {
    let encoder = FanatecConstantForceEncoder::new(8.0);
    let mut buf = [0u8; FANATEC_REPORT_LEN];

    encoder.encode(0.0, 0, &mut buf);

    let force = extract_force_i16(&buf);
    assert_eq!(force, 0, "zero torque must encode to 0");
    assert_eq!(buf[0], fanatec_ids::FFB_OUTPUT);
    assert_eq!(buf[1], 0x01);

    Ok(())
}

#[test]
fn fanatec_encode_max_torque_produces_i16_max() -> Result<(), Box<dyn std::error::Error>> {
    let encoder = FanatecConstantForceEncoder::new(8.0);
    let mut buf = [0u8; FANATEC_REPORT_LEN];

    encoder.encode(8.0, 0, &mut buf);

    let force = extract_force_i16(&buf);
    assert_eq!(
        force,
        i16::MAX,
        "100% torque must encode to i16::MAX (32767)"
    );

    Ok(())
}

#[test]
fn fanatec_encode_negative_torque_produces_negative_force() -> Result<(), Box<dyn std::error::Error>>
{
    let encoder = FanatecConstantForceEncoder::new(8.0);
    let mut buf = [0u8; FANATEC_REPORT_LEN];

    // When: encoding -4.0 Nm (−50%)
    encoder.encode(-4.0, 0, &mut buf);

    // Then: force = round(−0.5 × 32768) = −16384
    let force = extract_force_i16(&buf);
    assert_eq!(force, -16384, "−50% torque must encode to −16384");

    Ok(())
}

#[test]
fn fanatec_encode_over_range_clamps_to_max() -> Result<(), Box<dyn std::error::Error>> {
    let encoder = FanatecConstantForceEncoder::new(8.0);
    let mut buf = [0u8; FANATEC_REPORT_LEN];

    // When: encoding 12.0 Nm (150% of 8 Nm max → clamped to 1.0)
    encoder.encode(12.0, 0, &mut buf);

    let force = extract_force_i16(&buf);
    assert_eq!(force, i16::MAX, "over-range torque must clamp to i16::MAX");

    Ok(())
}

#[test]
fn fanatec_encode_round_trip_preserves_torque() -> Result<(), Box<dyn std::error::Error>> {
    // Encode a torque value, extract the i16, decode back, verify round-trip accuracy
    let max_nm: f32 = 8.0;
    let input_nm: f32 = 4.0;
    let encoder = FanatecConstantForceEncoder::new(max_nm);
    let mut buf = [0u8; FANATEC_REPORT_LEN];

    encoder.encode(input_nm, 0, &mut buf);

    let force = extract_force_i16(&buf);
    // Decode: force / 32767.0 × max_torque
    let decoded_nm = (force as f32 / i16::MAX as f32) * max_nm;
    let error = (decoded_nm - input_nm).abs();

    assert!(
        error < 0.001,
        "round-trip error must be < 0.001 Nm, got {} (decoded {} from force {})",
        error,
        decoded_nm,
        force
    );

    Ok(())
}

// ─── Logitech constant-force encoding round-trips ─────────────────────────────

#[test]
fn logitech_encode_half_torque_produces_correct_wire_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: G920 encoder at 2.2 Nm max
    let encoder = LogitechConstantForceEncoder::new(2.2);
    let mut buf = [0u8; LOGITECH_REPORT_LEN];

    // When: encoding 1.1 Nm (50%)
    encoder.encode(1.1, &mut buf);

    // Then: magnitude = truncate(0.5 × 10000) = 5000
    let magnitude = extract_force_i16(&buf);
    assert_eq!(magnitude, 5000, "50% torque must produce 5000 magnitude");
    assert_eq!(buf[0], 0x12, "byte 0 must be constant-force report ID");
    assert_eq!(buf[1], 1, "byte 1 must be effect block index 1");

    Ok(())
}

#[test]
fn logitech_encode_zero_torque_produces_zero_magnitude() -> Result<(), Box<dyn std::error::Error>> {
    let encoder = LogitechConstantForceEncoder::new(2.2);
    let mut buf = [0u8; LOGITECH_REPORT_LEN];

    encoder.encode(0.0, &mut buf);

    let magnitude = extract_force_i16(&buf);
    assert_eq!(magnitude, 0, "zero torque must encode to 0");

    Ok(())
}

#[test]
fn logitech_encode_max_torque_produces_10000() -> Result<(), Box<dyn std::error::Error>> {
    let encoder = LogitechConstantForceEncoder::new(2.2);
    let mut buf = [0u8; LOGITECH_REPORT_LEN];

    encoder.encode(2.2, &mut buf);

    let magnitude = extract_force_i16(&buf);
    assert_eq!(magnitude, 10000, "full torque must encode to 10000");

    Ok(())
}

#[test]
fn logitech_encode_negative_torque_produces_negative_magnitude()
-> Result<(), Box<dyn std::error::Error>> {
    let encoder = LogitechConstantForceEncoder::new(2.2);
    let mut buf = [0u8; LOGITECH_REPORT_LEN];

    // When: encoding −1.1 Nm (−50%)
    encoder.encode(-1.1, &mut buf);

    let magnitude = extract_force_i16(&buf);
    assert_eq!(magnitude, -5000, "−50% torque must produce −5000 magnitude");

    Ok(())
}

#[test]
fn logitech_encode_over_range_clamps_to_10000() -> Result<(), Box<dyn std::error::Error>> {
    let encoder = LogitechConstantForceEncoder::new(2.2);
    let mut buf = [0u8; LOGITECH_REPORT_LEN];

    // When: encoding 5.0 Nm (way beyond 2.2 Nm max)
    encoder.encode(5.0, &mut buf);

    let magnitude = extract_force_i16(&buf);
    assert_eq!(magnitude, 10000, "over-range torque must clamp to 10000");

    Ok(())
}

#[test]
fn logitech_encode_round_trip_preserves_torque() -> Result<(), Box<dyn std::error::Error>> {
    let max_nm: f32 = 2.2;
    let input_nm: f32 = 1.1;
    let encoder = LogitechConstantForceEncoder::new(max_nm);
    let mut buf = [0u8; LOGITECH_REPORT_LEN];

    encoder.encode(input_nm, &mut buf);

    let magnitude = extract_force_i16(&buf);
    let decoded_nm = (magnitude as f32 / 10_000.0) * max_nm;
    let error = (decoded_nm - input_nm).abs();

    assert!(
        error < 0.001,
        "round-trip error must be < 0.001 Nm, got {} (decoded {} from magnitude {})",
        error,
        decoded_nm,
        magnitude
    );

    Ok(())
}

// ─── Thrustmaster constant-force encoding round-trips ─────────────────────────

#[test]
fn thrustmaster_encode_half_torque_produces_correct_wire_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: T300RS encoder at 3.9 Nm max
    let encoder = ThrustmasterConstantForceEncoder::new(3.9);
    let mut buf = [0u8; THRUSTMASTER_REPORT_LEN];

    // When: encoding 1.95 Nm (50%)
    encoder.encode(1.95, &mut buf);

    // Then: magnitude = truncate(0.5 × 10000) = 5000
    let magnitude = extract_force_i16(&buf);
    assert_eq!(magnitude, 5000, "50% torque must produce 5000");
    assert_eq!(
        buf[0],
        tm_ids::CONSTANT_FORCE,
        "byte 0 must be CONSTANT_FORCE report ID"
    );

    // Then: trailing bytes are zero
    assert_eq!(&buf[4..], &[0u8; 4], "trailing bytes must be zero");

    Ok(())
}

#[test]
fn thrustmaster_encode_zero_torque_produces_zero_magnitude()
-> Result<(), Box<dyn std::error::Error>> {
    let encoder = ThrustmasterConstantForceEncoder::new(3.9);
    let mut buf = [0u8; THRUSTMASTER_REPORT_LEN];

    encoder.encode(0.0, &mut buf);

    let magnitude = extract_force_i16(&buf);
    assert_eq!(magnitude, 0, "zero torque must encode to 0");

    Ok(())
}

#[test]
fn thrustmaster_encode_max_torque_produces_10000() -> Result<(), Box<dyn std::error::Error>> {
    let encoder = ThrustmasterConstantForceEncoder::new(3.9);
    let mut buf = [0u8; THRUSTMASTER_REPORT_LEN];

    encoder.encode(3.9, &mut buf);

    let magnitude = extract_force_i16(&buf);
    assert_eq!(magnitude, 10000, "full torque must encode to 10000");

    Ok(())
}

#[test]
fn thrustmaster_encode_negative_torque_produces_negative_magnitude()
-> Result<(), Box<dyn std::error::Error>> {
    let encoder = ThrustmasterConstantForceEncoder::new(3.9);
    let mut buf = [0u8; THRUSTMASTER_REPORT_LEN];

    encoder.encode(-1.95, &mut buf);

    let magnitude = extract_force_i16(&buf);
    assert_eq!(magnitude, -5000, "−50% torque must produce −5000");

    Ok(())
}

#[test]
fn thrustmaster_encode_round_trip_preserves_torque() -> Result<(), Box<dyn std::error::Error>> {
    let max_nm: f32 = 3.9;
    let input_nm: f32 = 1.95;
    let encoder = ThrustmasterConstantForceEncoder::new(max_nm);
    let mut buf = [0u8; THRUSTMASTER_REPORT_LEN];

    encoder.encode(input_nm, &mut buf);

    let magnitude = extract_force_i16(&buf);
    let decoded_nm = (magnitude as f32 / 10_000.0) * max_nm;
    let error = (decoded_nm - input_nm).abs();

    assert!(
        error < 0.001,
        "round-trip error must be < 0.001 Nm, got {} (decoded {} from magnitude {})",
        error,
        decoded_nm,
        magnitude
    );

    Ok(())
}
