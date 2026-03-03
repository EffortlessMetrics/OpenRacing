//! Integration tests for SimpleMotion V2 protocol encoding, decoding, and validation.
//!
//! Covers:
//! - Packet encoding/decoding round-trips for all command types
//! - Command construction and validation
//! - Register read/write operations
//! - Boundary value testing for motor parameters
//! - CRC8 checksum validation

use racing_wheel_simplemotion_v2::commands::{
    SmCommand, SmCommandType, SmStatus, decode_command, encode_command,
};
use racing_wheel_simplemotion_v2::error::SmError;
use racing_wheel_simplemotion_v2::{
    SmFeedbackState, SmMotorFeedback, TorqueCommandEncoder, TORQUE_COMMAND_LEN,
    build_device_enable, build_get_parameter, build_set_parameter, build_set_torque_command,
    build_set_torque_command_with_velocity, build_set_zero_position, identify_device,
    is_wheelbase_product, parse_feedback_report, sm_device_identity,
};

// ── Encoding / decoding round-trips ─────────────────────────────────────────

#[test]
fn roundtrip_get_parameter() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(42, SmCommandType::GetParameter).with_param(0x1001, 0);
    let mut buf = [0u8; 15];
    let len = encode_command(&cmd, &mut buf)?;
    assert_eq!(len, 15);

    let decoded = decode_command(&buf)?;
    assert_eq!(decoded.seq, 42);
    assert_eq!(decoded.cmd_type, SmCommandType::GetParameter);
    assert_eq!(decoded.param_addr, Some(0x1001));
    assert_eq!(decoded.param_value, Some(0));
    Ok(())
}

#[test]
fn roundtrip_set_parameter() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(7, SmCommandType::SetParameter).with_param(0x2000, -500);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;

    let decoded = decode_command(&buf)?;
    assert_eq!(decoded.seq, 7);
    assert_eq!(decoded.cmd_type, SmCommandType::SetParameter);
    assert_eq!(decoded.param_addr, Some(0x2000));
    assert_eq!(decoded.param_value, Some(-500));
    Ok(())
}

#[test]
fn roundtrip_set_torque() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(0, SmCommandType::SetTorque).with_data(12345);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;

    let decoded = decode_command(&buf)?;
    assert_eq!(decoded.seq, 0);
    assert_eq!(decoded.cmd_type, SmCommandType::SetTorque);
    assert_eq!(decoded.data, Some(12345));
    Ok(())
}

#[test]
fn roundtrip_set_velocity() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(99, SmCommandType::SetVelocity).with_data(-9999);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;

    let decoded = decode_command(&buf)?;
    assert_eq!(decoded.seq, 99);
    assert_eq!(decoded.cmd_type, SmCommandType::SetVelocity);
    assert_eq!(decoded.data, Some(-9999));
    Ok(())
}

#[test]
fn roundtrip_set_position() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(128, SmCommandType::SetPosition).with_data(100_000);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;

    let decoded = decode_command(&buf)?;
    assert_eq!(decoded.seq, 128);
    assert_eq!(decoded.cmd_type, SmCommandType::SetPosition);
    assert_eq!(decoded.data, Some(100_000));
    Ok(())
}

#[test]
fn roundtrip_set_zero() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(1, SmCommandType::SetZero).with_data(0);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;

    let decoded = decode_command(&buf)?;
    assert_eq!(decoded.seq, 1);
    assert_eq!(decoded.cmd_type, SmCommandType::SetZero);
    Ok(())
}

#[test]
fn roundtrip_get_status() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(255, SmCommandType::GetStatus);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;

    let decoded = decode_command(&buf)?;
    assert_eq!(decoded.seq, 255);
    assert_eq!(decoded.cmd_type, SmCommandType::GetStatus);
    Ok(())
}

#[test]
fn roundtrip_reset() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(0, SmCommandType::Reset);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;

    let decoded = decode_command(&buf)?;
    assert_eq!(decoded.seq, 0);
    assert_eq!(decoded.cmd_type, SmCommandType::Reset);
    Ok(())
}

#[test]
fn roundtrip_all_command_types_preserve_type() -> Result<(), Box<dyn std::error::Error>> {
    let types = [
        SmCommandType::GetParameter,
        SmCommandType::SetParameter,
        SmCommandType::GetStatus,
        SmCommandType::SetTorque,
        SmCommandType::SetVelocity,
        SmCommandType::SetPosition,
        SmCommandType::SetZero,
        SmCommandType::Reset,
    ];

    for cmd_type in types {
        let cmd = SmCommand::new(0, cmd_type);
        let mut buf = [0u8; 15];
        encode_command(&cmd, &mut buf)?;
        let decoded = decode_command(&buf)?;
        assert_eq!(decoded.cmd_type, cmd_type, "round-trip failed for {cmd_type:?}");
    }
    Ok(())
}

// ── Command construction and validation ─────────────────────────────────────

#[test]
fn command_type_from_u16_returns_none_for_invalid() {
    assert!(SmCommandType::from_u16(0x0000).is_none());
    assert!(SmCommandType::from_u16(0x0004).is_none());
    assert!(SmCommandType::from_u16(0x000F).is_none());
    assert!(SmCommandType::from_u16(0x0014).is_none());
    assert!(SmCommandType::from_u16(0xFFFE).is_none());
}

#[test]
fn command_type_to_u16_roundtrip_all_variants() {
    let types = [
        (SmCommandType::GetParameter, 0x0001),
        (SmCommandType::SetParameter, 0x0002),
        (SmCommandType::GetStatus, 0x0003),
        (SmCommandType::SetTorque, 0x0010),
        (SmCommandType::SetVelocity, 0x0011),
        (SmCommandType::SetPosition, 0x0012),
        (SmCommandType::SetZero, 0x0013),
        (SmCommandType::Reset, 0xFFFF),
    ];

    for (cmd_type, expected_val) in types {
        assert_eq!(cmd_type.to_u16(), expected_val);
        assert_eq!(SmCommandType::from_u16(expected_val), Some(cmd_type));
    }
}

#[test]
fn sm_status_from_u8_covers_all_variants() {
    assert_eq!(SmStatus::from_u8(0), SmStatus::Ok);
    assert_eq!(SmStatus::from_u8(1), SmStatus::Error);
    assert_eq!(SmStatus::from_u8(2), SmStatus::Busy);
    assert_eq!(SmStatus::from_u8(3), SmStatus::NotReady);
    assert_eq!(SmStatus::from_u8(4), SmStatus::Unknown);
    assert_eq!(SmStatus::from_u8(128), SmStatus::Unknown);
    assert_eq!(SmStatus::from_u8(255), SmStatus::Unknown);
}

#[test]
fn sm_command_builder_with_param_and_data() {
    let cmd = SmCommand::new(10, SmCommandType::SetParameter)
        .with_param(0x3000, 42)
        .with_data(99);

    assert_eq!(cmd.seq, 10);
    assert_eq!(cmd.cmd_type, SmCommandType::SetParameter);
    assert_eq!(cmd.param_addr, Some(0x3000));
    assert_eq!(cmd.param_value, Some(42));
    assert_eq!(cmd.data, Some(99));
}

#[test]
fn sm_command_new_has_no_optional_fields() {
    let cmd = SmCommand::new(0, SmCommandType::GetStatus);
    assert!(cmd.param_addr.is_none());
    assert!(cmd.param_value.is_none());
    assert!(cmd.data.is_none());
}

#[test]
fn encode_rejects_buffer_too_small() {
    let cmd = SmCommand::new(0, SmCommandType::GetStatus);
    let mut buf = [0u8; 14];
    let result = encode_command(&cmd, &mut buf);
    assert!(matches!(
        result,
        Err(SmError::InvalidLength {
            expected: 15,
            actual: 14,
        })
    ));
}

#[test]
fn encode_accepts_buffer_larger_than_15() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(0, SmCommandType::GetStatus);
    let mut buf = [0u8; 64];
    let len = encode_command(&cmd, &mut buf)?;
    assert_eq!(len, 15);
    Ok(())
}

#[test]
fn decode_rejects_buffer_too_small() {
    let buf = [0u8; 14];
    let result = decode_command(&buf);
    assert!(matches!(
        result,
        Err(SmError::InvalidLength {
            expected: 15,
            actual: 14,
        })
    ));
}

#[test]
fn decode_rejects_invalid_command_type() -> Result<(), Box<dyn std::error::Error>> {
    // Build a valid packet then overwrite the command type with an invalid value
    let cmd = SmCommand::new(0, SmCommandType::GetStatus);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;

    // Set command type to invalid 0x0099 and recompute CRC
    buf[2] = 0x99;
    buf[3] = 0x00;
    // CRC must be recomputed; just set byte 14 to the wrong value to trigger CRC error first.
    // To test invalid command type, we need a valid CRC for the corrupted payload.
    // Compute CRC manually using the same algorithm.
    let crc = compute_crc8_for_test(&buf[..14]);
    buf[14] = crc;

    let result = decode_command(&buf);
    assert!(matches!(result, Err(SmError::InvalidCommandType(_))));
    Ok(())
}

// ── Register read/write operations ──────────────────────────────────────────

#[test]
fn build_get_parameter_encodes_address() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_get_parameter(0x2001, 5);
    let decoded = decode_command(&report)?;

    assert_eq!(decoded.seq, 5);
    assert_eq!(decoded.cmd_type, SmCommandType::GetParameter);
    assert_eq!(decoded.param_addr, Some(0x2001));
    assert_eq!(decoded.param_value, Some(0));
    Ok(())
}

#[test]
fn build_set_parameter_encodes_address_and_value() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_set_parameter(0x1001, 500, 10);
    let decoded = decode_command(&report)?;

    assert_eq!(decoded.seq, 10);
    assert_eq!(decoded.cmd_type, SmCommandType::SetParameter);
    assert_eq!(decoded.param_addr, Some(0x1001));
    assert_eq!(decoded.param_value, Some(500));
    Ok(())
}

#[test]
fn build_set_parameter_negative_value() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_set_parameter(0x1002, -1000, 20);
    let decoded = decode_command(&report)?;

    assert_eq!(decoded.param_addr, Some(0x1002));
    assert_eq!(decoded.param_value, Some(-1000));
    Ok(())
}

#[test]
fn build_device_enable_sets_param_0x1001() -> Result<(), Box<dyn std::error::Error>> {
    let enable = build_device_enable(true, 0);
    let decoded = decode_command(&enable)?;
    assert_eq!(decoded.cmd_type, SmCommandType::SetParameter);
    assert_eq!(decoded.param_addr, Some(0x1001));
    assert_eq!(decoded.param_value, Some(1));

    let disable = build_device_enable(false, 0);
    let decoded = decode_command(&disable)?;
    assert_eq!(decoded.param_value, Some(0));
    Ok(())
}

#[test]
fn build_get_status_is_get_status_type() -> Result<(), Box<dyn std::error::Error>> {
    let report = racing_wheel_simplemotion_v2::build_get_status(3);
    let decoded = decode_command(&report)?;
    assert_eq!(decoded.seq, 3);
    assert_eq!(decoded.cmd_type, SmCommandType::GetStatus);
    Ok(())
}

#[test]
fn build_set_zero_position_is_set_zero_type() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_set_zero_position(7);
    let decoded = decode_command(&report)?;
    assert_eq!(decoded.seq, 7);
    assert_eq!(decoded.cmd_type, SmCommandType::SetZero);
    assert_eq!(decoded.data, Some(0));
    Ok(())
}

#[test]
fn build_set_torque_command_decodable() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_set_torque_command(2560, 1);
    let decoded = decode_command(&report)?;
    assert_eq!(decoded.seq, 1);
    assert_eq!(decoded.cmd_type, SmCommandType::SetTorque);
    assert_eq!(decoded.data, Some(2560));
    Ok(())
}

#[test]
fn build_set_torque_command_negative_torque() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_set_torque_command(-5000, 2);
    let decoded = decode_command(&report)?;
    assert_eq!(decoded.data, Some(-5000));
    Ok(())
}

#[test]
fn build_set_torque_with_velocity_packs_both_values() {
    let torque: i16 = 1000;
    let velocity: i16 = 500;
    let report = build_set_torque_command_with_velocity(torque, velocity, 0);

    // Verify report ID and command type
    assert_eq!(report[0], 0x01);
    let cmd_type = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(cmd_type, SmCommandType::SetTorque.to_u16());

    // Verify the combined data field packing
    let data = i32::from_le_bytes([report[10], report[11], report[12], report[13]]);
    let expected = ((torque as i32) << 16) | (velocity as i32 & 0xFFFF);
    assert_eq!(data, expected);
}

// ── Boundary value testing for motor parameters ─────────────────────────────

#[test]
fn torque_encoder_clamps_at_max() {
    let mut enc = TorqueCommandEncoder::new(10.0);
    let mut out = [0u8; TORQUE_COMMAND_LEN];
    enc.encode(100.0, &mut out);
    let data = i32::from_le_bytes([out[10], out[11], out[12], out[13]]);
    // Normalized to 1.0 => 32767
    assert_eq!(data as i16, 32767_i16);
}

#[test]
fn torque_encoder_clamps_at_negative_max() {
    let mut enc = TorqueCommandEncoder::new(10.0);
    let mut out = [0u8; TORQUE_COMMAND_LEN];
    enc.encode(-100.0, &mut out);
    let data = i32::from_le_bytes([out[10], out[11], out[12], out[13]]);
    assert_eq!(data as i16, -32767_i16);
}

#[test]
fn torque_encoder_zero_produces_zero_data() {
    let mut enc = TorqueCommandEncoder::new(20.0);
    let mut out = [0u8; TORQUE_COMMAND_LEN];
    enc.encode_zero(&mut out);
    let data = i32::from_le_bytes([out[10], out[11], out[12], out[13]]);
    assert_eq!(data as i16, 0);
}

#[test]
fn torque_encoder_half_max_produces_half_range() {
    let mut enc = TorqueCommandEncoder::new(20.0);
    let mut out = [0u8; TORQUE_COMMAND_LEN];
    enc.encode(10.0, &mut out);
    let data = i32::from_le_bytes([out[10], out[11], out[12], out[13]]);
    let raw = data as i16;
    // 10.0 / 20.0 = 0.5 => 0.5 * 32767 = 16383
    assert_eq!(raw, 16383);
}

#[test]
fn torque_encoder_sequence_wraps_at_255() {
    let mut enc = TorqueCommandEncoder::new(10.0);
    let mut out = [0u8; TORQUE_COMMAND_LEN];

    // Advance sequence to 255
    for _ in 0..255 {
        enc.encode(0.0, &mut out);
    }
    assert_eq!(enc.sequence(), 255);

    // Next encode should wrap to 0
    enc.encode(0.0, &mut out);
    assert_eq!(enc.sequence(), 0);
}

#[test]
fn torque_encoder_tiny_max_torque_floors_to_minimum() {
    // max_torque_nm is clamped to 0.01 minimum
    let mut enc = TorqueCommandEncoder::new(0.0);
    let mut out = [0u8; TORQUE_COMMAND_LEN];
    let len = enc.encode(0.005, &mut out);
    assert_eq!(len, 15);
    assert_eq!(out[0], 0x01);
}

#[test]
fn set_torque_command_boundary_i16_max() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_set_torque_command(i16::MAX, 0);
    let decoded = decode_command(&report)?;
    assert_eq!(decoded.data, Some(i16::MAX as i32));
    Ok(())
}

#[test]
fn set_torque_command_boundary_i16_min() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_set_torque_command(i16::MIN, 0);
    let decoded = decode_command(&report)?;
    assert_eq!(decoded.data, Some(i16::MIN as i32));
    Ok(())
}

#[test]
fn set_parameter_boundary_i32_max() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_set_parameter(0xFFFF, i32::MAX, 0);
    let decoded = decode_command(&report)?;
    assert_eq!(decoded.param_addr, Some(0xFFFF));
    assert_eq!(decoded.param_value, Some(i32::MAX));
    Ok(())
}

#[test]
fn set_parameter_boundary_i32_min() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_set_parameter(0x0000, i32::MIN, 0);
    let decoded = decode_command(&report)?;
    assert_eq!(decoded.param_addr, Some(0x0000));
    assert_eq!(decoded.param_value, Some(i32::MIN));
    Ok(())
}

#[test]
fn feedback_position_degrees_boundary_full_rotation() {
    let state = SmFeedbackState {
        motor: SmMotorFeedback {
            position: 14400,
            ..Default::default()
        },
        ..Default::default()
    };
    let degrees = state.position_degrees(14400);
    assert!((degrees - 360.0).abs() < 0.01);
}

#[test]
fn feedback_position_degrees_zero() {
    let state = SmFeedbackState {
        motor: SmMotorFeedback {
            position: 0,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!((state.position_degrees(14400) - 0.0).abs() < f32::EPSILON);
}

#[test]
fn feedback_position_degrees_negative() {
    let state = SmFeedbackState {
        motor: SmMotorFeedback {
            position: -7200,
            ..Default::default()
        },
        ..Default::default()
    };
    let degrees = state.position_degrees(14400);
    assert!((degrees - (-180.0)).abs() < 0.01);
}

#[test]
fn feedback_velocity_rpm_boundary() {
    let state = SmFeedbackState {
        motor: SmMotorFeedback {
            velocity: 0,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!((state.velocity_rpm(14400) - 0.0).abs() < f32::EPSILON);
}

#[test]
fn feedback_torque_nm_zero() {
    let state = SmFeedbackState {
        motor: SmMotorFeedback {
            torque: 0,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!((state.torque_nm(0.1) - 0.0).abs() < f32::EPSILON);
}

#[test]
fn feedback_torque_nm_negative() {
    let state = SmFeedbackState {
        motor: SmMotorFeedback {
            torque: -256,
            ..Default::default()
        },
        ..Default::default()
    };
    let torque = state.torque_nm(0.1);
    assert!((torque - (-0.1)).abs() < 0.001);
}

#[test]
fn parse_feedback_disconnected_marker() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; 64];
    data[0] = 0x02;
    data[4] = 0xFF;
    data[5] = 0xFF;
    // When bytes 4-5 are both 0xFF, connected should be false
    let state = parse_feedback_report(&data)?;
    assert!(!state.connected);
    Ok(())
}

#[test]
fn parse_feedback_connected_when_position_nonzero() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; 64];
    data[0] = 0x02;
    data[4] = 0x01;
    data[5] = 0x00;
    let state = parse_feedback_report(&data)?;
    assert!(state.connected);
    Ok(())
}

#[test]
fn parse_feedback_temperature_signed() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; 64];
    data[0] = 0x02;
    data[18] = 0xCE; // -50 as i8
    let state = parse_feedback_report(&data)?;
    assert_eq!(state.temperature, -50);
    Ok(())
}

#[test]
fn parse_feedback_max_bus_voltage() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; 64];
    data[0] = 0x02;
    data[14] = 0xFF;
    data[15] = 0xFF;
    let state = parse_feedback_report(&data)?;
    assert_eq!(state.bus_voltage, u16::MAX);
    Ok(())
}

// ── CRC8 checksum validation ────────────────────────────────────────────────

#[test]
fn crc_mismatch_detected_on_single_bit_flip() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(0, SmCommandType::SetTorque).with_data(1000);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;

    // Flip a single bit in byte 6
    buf[6] ^= 0x01;
    let result = decode_command(&buf);
    assert!(matches!(result, Err(SmError::CrcMismatch { .. })));
    Ok(())
}

#[test]
fn crc_mismatch_detected_on_sequence_corruption() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(42, SmCommandType::GetStatus);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;

    // Corrupt the sequence byte
    buf[1] = 43;
    let result = decode_command(&buf);
    assert!(matches!(result, Err(SmError::CrcMismatch { .. })));
    Ok(())
}

#[test]
fn crc_mismatch_detected_on_every_byte_corruption() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(10, SmCommandType::SetParameter).with_param(0x1001, 42);
    let mut original = [0u8; 15];
    encode_command(&cmd, &mut original)?;

    // Corrupting any of bytes 0-13 (payload) should cause CRC mismatch
    for i in 0..14 {
        let mut corrupted = original;
        corrupted[i] ^= 0xFF;
        let result = decode_command(&corrupted);
        assert!(
            matches!(result, Err(SmError::CrcMismatch { .. })),
            "corruption at byte {i} was not detected"
        );
    }
    Ok(())
}

#[test]
fn crc_mismatch_on_zeroed_crc_byte() {
    let mut buf = [0u8; 15];
    buf[0] = 0x01;
    buf[2] = 0x03; // GetStatus command type
    buf[14] = 0x00; // Likely wrong CRC
    let result = decode_command(&buf);
    // If CRC happens to be 0x00, this would pass — but for GetStatus with seq=0 it won't.
    assert!(matches!(result, Err(SmError::CrcMismatch { .. })));
}

#[test]
fn valid_crc_passes_decode() -> Result<(), Box<dyn std::error::Error>> {
    // Encode then decode — CRC must match
    let cmd = SmCommand::new(100, SmCommandType::SetParameter).with_param(0xABCD, 0x12345678);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;

    // Decode should succeed
    let decoded = decode_command(&buf)?;
    assert_eq!(decoded.seq, 100);
    assert_eq!(decoded.param_addr, Some(0xABCD));
    assert_eq!(decoded.param_value, Some(0x12345678));
    Ok(())
}

#[test]
fn encoded_report_always_starts_with_report_id_0x01() -> Result<(), Box<dyn std::error::Error>> {
    let types = [
        SmCommandType::GetParameter,
        SmCommandType::SetParameter,
        SmCommandType::GetStatus,
        SmCommandType::SetTorque,
        SmCommandType::SetVelocity,
        SmCommandType::SetPosition,
        SmCommandType::SetZero,
        SmCommandType::Reset,
    ];

    for cmd_type in types {
        let cmd = SmCommand::new(0, cmd_type);
        let mut buf = [0u8; 15];
        encode_command(&cmd, &mut buf)?;
        assert_eq!(buf[0], 0x01, "report ID must be 0x01 for {cmd_type:?}");
    }
    Ok(())
}

// ── Device identification boundary tests ────────────────────────────────────

#[test]
fn identify_device_boundary_product_ids() {
    // Just below known PIDs
    let below = identify_device(0x604F);
    assert!(!below.supports_ffb);

    // Just above known PIDs
    let above = identify_device(0x6053);
    assert!(!above.supports_ffb);
}

#[test]
fn identify_device_zero_pid() {
    let identity = identify_device(0x0000);
    assert!(!identity.supports_ffb);
    assert!(identity.max_torque_nm.is_none());
}

#[test]
fn identify_device_max_pid() {
    let identity = identify_device(0xFFFF);
    assert!(!identity.supports_ffb);
    assert!(identity.max_torque_nm.is_none());
}

#[test]
fn sm_device_identity_matches_identify_device() {
    for pid in [0x6050, 0x6051, 0x6052, 0xFFFF] {
        let a = identify_device(pid);
        let b = sm_device_identity(pid);
        assert_eq!(a.product_id, b.product_id);
        assert_eq!(a.name, b.name);
        assert_eq!(a.supports_ffb, b.supports_ffb);
    }
}

#[test]
fn all_known_wheelbases_have_torque_and_rpm_limits() {
    for pid in [0x6050, 0x6051, 0x6052] {
        let identity = identify_device(pid);
        assert!(
            identity.max_torque_nm.is_some(),
            "wheelbase {:#06x} missing max_torque_nm",
            pid
        );
        assert!(
            identity.max_rpm.is_some(),
            "wheelbase {:#06x} missing max_rpm",
            pid
        );
        assert!(is_wheelbase_product(pid));
    }
}

// ── Helper: CRC8 computation for test use ───────────────────────────────────

/// Replicates the crate-internal CRC8 for test verification.
fn compute_crc8_for_test(data: &[u8]) -> u8 {
    let mut crc: u8 = 0x00;
    for &byte in data {
        crc ^= byte;
        for _ in 0..8 {
            if crc & 0x80 != 0 {
                crc = (crc << 1) ^ 0x07;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}
