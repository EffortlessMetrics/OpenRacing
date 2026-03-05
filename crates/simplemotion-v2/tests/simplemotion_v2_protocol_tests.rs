//! Comprehensive SimpleMotion V2 protocol verification tests.
//!
//! Tests cover:
//! 1. Command encoding/decoding roundtrips for every command type
//! 2. CRC-8/ITU calculation correctness against known vectors
//! 3. Address validation and multi-axis parameter addressing
//! 4. Response parsing for all feedback report fields and status codes
//! 5. Error handling: invalid CRC, truncated packets, invalid addresses, bad report IDs
//! 6. Protocol state machine: sequence numbering, encoder lifecycle
//! 7. Proptest fuzzing for packet construction robustness
//! 8. Known-good byte sequences derived from Granite Devices documentation
//! 9. Q8.8 fixed-point torque encoding precision
//! 10. Cross-builder consistency: every build_* helper produces decodeable packets
//!
//! References:
//! - Granite Devices SimpleMotion V2: <https://granitedevices.com/wiki/SimpleMotion_V2>
//! - IONI / ARGON servo drive datasheets

use proptest::prelude::*;
use racing_wheel_simplemotion_v2::commands::{
    SmCommand, SmCommandType, SmStatus, decode_command, encode_command,
};
use racing_wheel_simplemotion_v2::error::SmError;
use racing_wheel_simplemotion_v2::{
    FEEDBACK_REPORT_LEN, SETPARAM_REPORT_LEN, STATUS_REPORT_LEN, SmDeviceCategory, SmFeedbackState,
    SmMotorFeedback, TORQUE_COMMAND_LEN, TorqueCommandEncoder, build_device_enable,
    build_get_parameter, build_get_status, build_set_parameter, build_set_torque_command,
    build_set_torque_command_with_velocity, build_set_zero_position, identify_device,
    is_wheelbase_product, parse_feedback_report, sm_device_identity,
};

// ── Helper: CRC-8/ITU polynomial 0x07 (matching the crate's internal algorithm) ─

fn compute_crc8(data: &[u8]) -> u8 {
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

/// Build a 64-byte feedback report with specified motor values.
fn make_feedback_report(
    seq: u8,
    status: u8,
    position: i32,
    velocity: i32,
    torque: i16,
    bus_voltage: u16,
    motor_current: i16,
    temperature: i8,
) -> Vec<u8> {
    let mut data = vec![0u8; 64];
    data[0] = 0x02; // feedback report ID
    data[1] = seq;
    data[2] = status;
    let pos = position.to_le_bytes();
    data[4..8].copy_from_slice(&pos);
    let vel = velocity.to_le_bytes();
    data[8..12].copy_from_slice(&vel);
    let torq = torque.to_le_bytes();
    data[12..14].copy_from_slice(&torq);
    let bv = bus_voltage.to_le_bytes();
    data[14..16].copy_from_slice(&bv);
    let mc = motor_current.to_le_bytes();
    data[16..18].copy_from_slice(&mc);
    data[18] = temperature as u8;
    data
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Command encoding/decoding roundtrips — every SmCommandType
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn roundtrip_all_command_types_preserve_seq_and_type() -> Result<(), Box<dyn std::error::Error>> {
    let all_types = [
        SmCommandType::GetParameter,
        SmCommandType::SetParameter,
        SmCommandType::GetStatus,
        SmCommandType::SetTorque,
        SmCommandType::SetVelocity,
        SmCommandType::SetPosition,
        SmCommandType::SetZero,
        SmCommandType::Reset,
    ];
    for (i, &cmd_type) in all_types.iter().enumerate() {
        let seq = (i as u8).wrapping_mul(37); // varied sequences
        let cmd = SmCommand::new(seq, cmd_type)
            .with_param(0x1001, 42)
            .with_data(99);
        let mut buf = [0u8; 15];
        let len = encode_command(&cmd, &mut buf)?;
        assert_eq!(len, 15, "all commands encode to 15 bytes");

        let decoded = decode_command(&buf)?;
        assert_eq!(decoded.seq, seq, "sequence preserved for {:?}", cmd_type);
        assert_eq!(
            decoded.cmd_type, cmd_type,
            "command type preserved for {:?}",
            cmd_type
        );
    }
    Ok(())
}

#[test]
fn roundtrip_set_parameter_preserves_param_addr_and_value() -> Result<(), Box<dyn std::error::Error>>
{
    let addrs: &[(u16, i32)] = &[
        (0x0000, 0),
        (0x1001, 1),
        (0x2000, -500),
        (0xFFFF, i32::MAX),
        (0x8000, i32::MIN),
    ];
    for &(addr, value) in addrs {
        let cmd = SmCommand::new(1, SmCommandType::SetParameter).with_param(addr, value);
        let mut buf = [0u8; 15];
        encode_command(&cmd, &mut buf)?;
        let decoded = decode_command(&buf)?;
        assert_eq!(decoded.param_addr, Some(addr));
        assert_eq!(decoded.param_value, Some(value));
    }
    Ok(())
}

#[test]
fn roundtrip_set_torque_preserves_data_field() -> Result<(), Box<dyn std::error::Error>> {
    let values: &[i32] = &[0, 1, -1, 1000, -1000, i32::MAX, i32::MIN, 32767, -32768];
    for &data_val in values {
        let cmd = SmCommand::new(10, SmCommandType::SetTorque).with_data(data_val);
        let mut buf = [0u8; 15];
        encode_command(&cmd, &mut buf)?;
        let decoded = decode_command(&buf)?;
        assert_eq!(decoded.data, Some(data_val));
    }
    Ok(())
}

#[test]
fn roundtrip_command_with_no_optional_fields() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(0, SmCommandType::GetStatus);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;
    let decoded = decode_command(&buf)?;
    assert_eq!(decoded.seq, 0);
    assert_eq!(decoded.cmd_type, SmCommandType::GetStatus);
    // Decoded always populates param fields from raw bytes (all zeros)
    assert_eq!(decoded.param_addr, Some(0));
    assert_eq!(decoded.param_value, Some(0));
    assert_eq!(decoded.data, Some(0));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. CRC-8/ITU calculation correctness — known vectors
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn crc8_empty_input_is_zero() {
    assert_eq!(compute_crc8(&[]), 0x00);
}

#[test]
fn crc8_single_byte_vectors() {
    // CRC-8/ITU with poly 0x07, init 0x00
    assert_eq!(compute_crc8(&[0x00]), 0x00);
    assert_eq!(compute_crc8(&[0x01]), 0x07);
    assert_eq!(compute_crc8(&[0x80]), 0x89); // 0x80 -> poly applied, result 0x89
}

#[test]
fn crc8_known_multi_byte_vectors() {
    // "123456789" is a standard CRC-8 test vector
    // CRC-8 with poly 0x07, init 0x00 gives 0xF4 for ASCII "123456789"
    let data = b"123456789";
    assert_eq!(compute_crc8(data), 0xF4);
}

#[test]
fn crc8_encoded_get_status_seq0_matches_packet() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(0, SmCommandType::GetStatus);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;
    // The CRC stored at byte 14 must match our independent computation
    let expected_crc = compute_crc8(&buf[..14]);
    assert_eq!(buf[14], expected_crc);
    Ok(())
}

#[test]
fn crc8_every_encoded_command_validates() -> Result<(), Box<dyn std::error::Error>> {
    let commands: Vec<SmCommand> = vec![
        SmCommand::new(0, SmCommandType::GetStatus),
        SmCommand::new(255, SmCommandType::Reset),
        SmCommand::new(128, SmCommandType::SetTorque).with_data(-1),
        SmCommand::new(1, SmCommandType::SetParameter).with_param(0xFFFF, i32::MAX),
        SmCommand::new(42, SmCommandType::GetParameter).with_param(0x1001, 0),
    ];
    for cmd in &commands {
        let mut buf = [0u8; 15];
        encode_command(cmd, &mut buf)?;
        let crc_computed = compute_crc8(&buf[..14]);
        assert_eq!(buf[14], crc_computed, "CRC mismatch for cmd {:?}", cmd);
        // Decoding must succeed (CRC check inside decode_command)
        let _decoded = decode_command(&buf)?;
    }
    Ok(())
}

#[test]
fn crc8_single_bit_flip_detected() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(42, SmCommandType::SetTorque).with_data(5000);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;

    // Flip each bit in the payload (bytes 0..14), verify CRC check fails
    for byte_idx in 0..14 {
        for bit in 0..8 {
            let mut corrupted = buf;
            corrupted[byte_idx] ^= 1 << bit;
            let result = decode_command(&corrupted);
            assert!(
                result.is_err(),
                "bit flip at byte {} bit {} not detected",
                byte_idx,
                bit
            );
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Address validation and multi-axis parameter addressing
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn parameter_address_full_u16_range() -> Result<(), Box<dyn std::error::Error>> {
    let addresses: &[u16] = &[0x0000, 0x0001, 0x1001, 0x7FFF, 0x8000, 0xFFFE, 0xFFFF];
    for &addr in addresses {
        let cmd = SmCommand::new(1, SmCommandType::GetParameter).with_param(addr, 0);
        let mut buf = [0u8; 15];
        encode_command(&cmd, &mut buf)?;
        let decoded = decode_command(&buf)?;
        assert_eq!(
            decoded.param_addr,
            Some(addr),
            "address 0x{:04X} not preserved",
            addr
        );
    }
    Ok(())
}

#[test]
fn multi_axis_commands_differ_by_address() -> Result<(), Box<dyn std::error::Error>> {
    // Simulate addressing two axes by using different parameter addresses
    let axis0_addr: u16 = 0x1001;
    let axis1_addr: u16 = 0x2001;

    let cmd0 = SmCommand::new(0, SmCommandType::SetParameter).with_param(axis0_addr, 100);
    let cmd1 = SmCommand::new(1, SmCommandType::SetParameter).with_param(axis1_addr, 200);

    let mut buf0 = [0u8; 15];
    let mut buf1 = [0u8; 15];
    encode_command(&cmd0, &mut buf0)?;
    encode_command(&cmd1, &mut buf1)?;

    // Bytes 4-5 carry the address — they must differ
    assert_ne!(buf0[4..6], buf1[4..6], "axis addresses should differ");

    let dec0 = decode_command(&buf0)?;
    let dec1 = decode_command(&buf1)?;
    assert_eq!(dec0.param_addr, Some(axis0_addr));
    assert_eq!(dec1.param_addr, Some(axis1_addr));
    assert_eq!(dec0.param_value, Some(100));
    assert_eq!(dec1.param_value, Some(200));
    Ok(())
}

#[test]
fn well_known_sm_parameter_addresses_encode() -> Result<(), Box<dyn std::error::Error>> {
    // SM parameter addresses from Granite Devices docs
    let well_known: &[(u16, &str)] = &[
        (0x0001, "SMP_SERIAL_NR"),
        (0x0005, "SMP_BUS_MODE"),
        (0x1001, "SMP_CONTROL_BITS1"),
        (0x0100, "SMP_POSITION_FB"),
        (0x0101, "SMP_VELOCITY_FB"),
    ];
    for &(addr, name) in well_known {
        let cmd = SmCommand::new(0, SmCommandType::GetParameter).with_param(addr, 0);
        let mut buf = [0u8; 15];
        encode_command(&cmd, &mut buf)?;
        let decoded = decode_command(&buf)?;
        assert_eq!(
            decoded.param_addr,
            Some(addr),
            "address for {} not preserved",
            name
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Response parsing — all feedback report fields and status codes
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn feedback_all_status_codes_parsed() -> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        (0u8, SmStatus::Ok),
        (1, SmStatus::Error),
        (2, SmStatus::Busy),
        (3, SmStatus::NotReady),
        (4, SmStatus::Unknown),
        (127, SmStatus::Unknown),
        (255, SmStatus::Unknown),
    ];
    for (status_byte, expected_status) in cases {
        let report = make_feedback_report(0, status_byte, 0, 0, 0, 0, 0, 0);
        let state = parse_feedback_report(&report)?;
        assert_eq!(
            state.status, expected_status,
            "status byte {} mismatch",
            status_byte
        );
    }
    Ok(())
}

#[test]
fn feedback_position_encoding_signed_i32() -> Result<(), Box<dyn std::error::Error>> {
    let positions: &[i32] = &[0, 1, -1, 100_000, -100_000, i32::MAX, i32::MIN];
    for &pos in positions {
        let report = make_feedback_report(1, 0, pos, 0, 0, 0, 0, 0);
        let state = parse_feedback_report(&report)?;
        assert_eq!(state.motor.position, pos, "position {} not preserved", pos);
    }
    Ok(())
}

#[test]
fn feedback_velocity_encoding_signed_i32() -> Result<(), Box<dyn std::error::Error>> {
    let velocities: &[i32] = &[0, 1, -1, 50_000, -50_000, i32::MAX, i32::MIN];
    for &vel in velocities {
        let report = make_feedback_report(1, 0, 0, vel, 0, 0, 0, 0);
        let state = parse_feedback_report(&report)?;
        assert_eq!(state.motor.velocity, vel, "velocity {} not preserved", vel);
    }
    Ok(())
}

#[test]
fn feedback_torque_encoding_signed_i16() -> Result<(), Box<dyn std::error::Error>> {
    let torques: &[i16] = &[0, 1, -1, 32767, -32768, 256, -256];
    for &torq in torques {
        let report = make_feedback_report(1, 0, 0, 0, torq, 0, 0, 0);
        let state = parse_feedback_report(&report)?;
        assert_eq!(state.motor.torque, torq, "torque {} not preserved", torq);
    }
    Ok(())
}

#[test]
fn feedback_bus_voltage_unsigned_u16() -> Result<(), Box<dyn std::error::Error>> {
    let voltages: &[u16] = &[0, 1, 480, 1000, 48000, u16::MAX];
    for &voltage in voltages {
        let report = make_feedback_report(1, 0, 0, 0, 0, voltage, 0, 0);
        let state = parse_feedback_report(&report)?;
        assert_eq!(
            state.bus_voltage, voltage,
            "bus_voltage {} not preserved",
            voltage
        );
    }
    Ok(())
}

#[test]
fn feedback_motor_current_signed_i16() -> Result<(), Box<dyn std::error::Error>> {
    let currents: &[i16] = &[0, 1, -1, 10000, -10000, i16::MAX, i16::MIN];
    for &current in currents {
        let report = make_feedback_report(1, 0, 0, 0, 0, 0, current, 0);
        let state = parse_feedback_report(&report)?;
        assert_eq!(
            state.motor_current, current,
            "motor_current {} not preserved",
            current
        );
    }
    Ok(())
}

#[test]
fn feedback_temperature_signed_i8() -> Result<(), Box<dyn std::error::Error>> {
    let temps: &[i8] = &[0, 25, 50, 80, 127, -1, -40, -128];
    for &temp in temps {
        let report = make_feedback_report(1, 0, 0, 0, 0, 0, 0, temp);
        let state = parse_feedback_report(&report)?;
        assert_eq!(
            state.temperature, temp,
            "temperature {} not preserved",
            temp
        );
    }
    Ok(())
}

#[test]
fn feedback_connected_flag_logic() -> Result<(), Box<dyn std::error::Error>> {
    // Connected: position bytes [4..6] are NOT both 0xFF
    let report_connected = make_feedback_report(1, 0, 0x00001000, 0, 0, 0, 0, 0);
    let state = parse_feedback_report(&report_connected)?;
    assert!(state.connected);

    // Disconnected: bytes[4]=0xFF, bytes[5]=0xFF (checks LE representation of position)
    let mut report_disc = vec![0u8; 64];
    report_disc[0] = 0x02;
    report_disc[4] = 0xFF;
    report_disc[5] = 0xFF;
    // bytes[6..8] can be anything — the check only looks at [4] and [5]
    let state_disc = parse_feedback_report(&report_disc)?;
    assert!(!state_disc.connected);

    // Edge case: only byte[4] is 0xFF, byte[5] is not
    let mut report_partial = vec![0u8; 64];
    report_partial[0] = 0x02;
    report_partial[4] = 0xFF;
    report_partial[5] = 0x00;
    let state_partial = parse_feedback_report(&report_partial)?;
    assert!(state_partial.connected);

    Ok(())
}

#[test]
fn feedback_seq_full_range() -> Result<(), Box<dyn std::error::Error>> {
    for seq in [0u8, 1, 127, 128, 254, 255] {
        let report = make_feedback_report(seq, 0, 0, 0, 0, 0, 0, 0);
        let state = parse_feedback_report(&report)?;
        assert_eq!(state.seq, seq);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Error handling — invalid CRC, truncated packets, bad report IDs
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn error_decode_truncated_0_bytes() {
    let result = decode_command(&[]);
    assert!(matches!(
        result,
        Err(SmError::InvalidLength {
            expected: 15,
            actual: 0
        })
    ));
}

#[test]
fn error_decode_truncated_14_bytes() {
    let result = decode_command(&[0u8; 14]);
    assert!(matches!(
        result,
        Err(SmError::InvalidLength {
            expected: 15,
            actual: 14
        })
    ));
}

#[test]
fn error_decode_truncated_1_byte() {
    let result = decode_command(&[0x01]);
    assert!(matches!(
        result,
        Err(SmError::InvalidLength {
            expected: 15,
            actual: 1
        })
    ));
}

#[test]
fn error_encode_buffer_too_small() {
    let cmd = SmCommand::new(0, SmCommandType::GetStatus);
    let mut buf = [0u8; 14];
    let result = encode_command(&cmd, &mut buf);
    assert!(matches!(
        result,
        Err(SmError::InvalidLength {
            expected: 15,
            actual: 14
        })
    ));
}

#[test]
fn error_encode_buffer_zero_length() {
    let cmd = SmCommand::new(0, SmCommandType::GetStatus);
    let mut buf = [0u8; 0];
    let result = encode_command(&cmd, &mut buf);
    assert!(matches!(
        result,
        Err(SmError::InvalidLength {
            expected: 15,
            actual: 0
        })
    ));
}

#[test]
fn error_crc_mismatch_corrupted_crc_byte() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(5, SmCommandType::SetTorque).with_data(1000);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;

    // Corrupt CRC
    buf[14] ^= 0xFF;
    let result = decode_command(&buf);
    assert!(matches!(result, Err(SmError::CrcMismatch { .. })));
    Ok(())
}

#[test]
fn error_crc_mismatch_corrupted_payload() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(5, SmCommandType::SetTorque).with_data(1000);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;

    // Corrupt payload byte
    buf[6] ^= 0x01;
    let result = decode_command(&buf);
    assert!(matches!(result, Err(SmError::CrcMismatch { .. })));
    Ok(())
}

#[test]
fn error_invalid_command_type_in_decode() -> Result<(), Box<dyn std::error::Error>> {
    // Construct a packet with an unknown command type (0x0099)
    let mut buf = [0u8; 15];
    buf[0] = 0x01;
    buf[2] = 0x99;
    buf[3] = 0x00;
    // Compute valid CRC
    let crc = compute_crc8(&buf[..14]);
    buf[14] = crc;

    let result = decode_command(&buf);
    assert!(matches!(result, Err(SmError::InvalidCommandType(_))));
    Ok(())
}

#[test]
fn error_feedback_wrong_report_id() {
    let mut data = vec![0u8; 64];
    data[0] = 0x01; // command report ID instead of feedback
    let result = parse_feedback_report(&data);
    assert!(matches!(result, Err(SmError::InvalidCommandType(0x01))));
}

#[test]
fn error_feedback_report_too_short() {
    for len in [0, 1, 32, 63] {
        let data = vec![0u8; len];
        let result = parse_feedback_report(&data);
        assert!(
            matches!(result, Err(SmError::InvalidLength { expected: 64, .. })),
            "expected InvalidLength for len {}",
            len
        );
    }
}

#[test]
fn error_all_variants_display_non_empty() {
    let errors: Vec<SmError> = vec![
        SmError::InvalidLength {
            expected: 15,
            actual: 10,
        },
        SmError::InvalidCommandType(0x99),
        SmError::InvalidParameter(0x1001),
        SmError::DeviceError("test device error".to_string()),
        SmError::CommunicationError("test comm error".to_string()),
        SmError::CrcMismatch {
            expected: 0xAA,
            actual: 0xBB,
        },
        SmError::ParseError("test parse error".to_string()),
        SmError::EncodeError("test encode error".to_string()),
    ];
    for err in &errors {
        let msg = err.to_string();
        assert!(
            !msg.is_empty(),
            "Error display for {:?} should not be empty",
            err
        );
    }
}

#[test]
fn error_io_error_converts_to_communication_error() {
    let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe broken");
    let sm_err: SmError = io_err.into();
    assert!(matches!(sm_err, SmError::CommunicationError(_)));
    let msg = sm_err.to_string();
    assert!(msg.contains("pipe broken"));
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Protocol state machine — sequence numbering, encoder lifecycle
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn torque_encoder_sequence_starts_at_zero() {
    let enc = TorqueCommandEncoder::new(20.0);
    assert_eq!(enc.sequence(), 0);
}

#[test]
fn torque_encoder_sequence_increments_per_encode() {
    let mut enc = TorqueCommandEncoder::new(20.0);
    let mut out = [0u8; TORQUE_COMMAND_LEN];
    for expected_seq in 0u8..10 {
        assert_eq!(enc.sequence(), expected_seq);
        enc.encode(1.0, &mut out);
    }
    assert_eq!(enc.sequence(), 10);
}

#[test]
fn torque_encoder_sequence_wraps_at_255() {
    let mut enc = TorqueCommandEncoder::new(20.0);
    let mut out = [0u8; TORQUE_COMMAND_LEN];
    // Advance to 255
    for _ in 0..255 {
        enc.encode(0.0, &mut out);
    }
    assert_eq!(enc.sequence(), 255);
    enc.encode(0.0, &mut out);
    assert_eq!(enc.sequence(), 0, "sequence should wrap to 0 after 255");
}

#[test]
fn torque_encoder_encode_with_velocity_also_increments_seq() {
    let mut enc = TorqueCommandEncoder::new(20.0);
    let mut out = [0u8; TORQUE_COMMAND_LEN];
    enc.encode_with_velocity(1.0, 100.0, &mut out);
    assert_eq!(enc.sequence(), 1);
    enc.encode_with_velocity(2.0, 200.0, &mut out);
    assert_eq!(enc.sequence(), 2);
}

#[test]
fn torque_encoder_encode_zero_increments_seq() {
    let mut enc = TorqueCommandEncoder::new(20.0);
    let mut out = [0u8; TORQUE_COMMAND_LEN];
    enc.encode_zero(&mut out);
    assert_eq!(enc.sequence(), 1);
}

#[test]
fn torque_encoder_all_encode_methods_produce_valid_report_id() {
    let mut enc = TorqueCommandEncoder::new(20.0);
    let mut out = [0u8; TORQUE_COMMAND_LEN];

    enc.encode(5.0, &mut out);
    assert_eq!(out[0], 0x01);

    enc.encode_with_velocity(5.0, 100.0, &mut out);
    assert_eq!(out[0], 0x01);

    enc.encode_zero(&mut out);
    assert_eq!(out[0], 0x01);
}

#[test]
fn torque_encoder_with_custom_torque_constant() {
    let mut enc = TorqueCommandEncoder::new(35.0).with_torque_constant(0.15);
    let mut out = [0u8; TORQUE_COMMAND_LEN];
    let len = enc.encode(17.5, &mut out);
    assert_eq!(len, 15);
    assert_eq!(out[0], 0x01);
}

#[test]
fn torque_encoder_clamps_to_max_torque() -> Result<(), Box<dyn std::error::Error>> {
    let mut enc = TorqueCommandEncoder::new(20.0);
    let mut out1 = [0u8; TORQUE_COMMAND_LEN];
    let mut out2 = [0u8; TORQUE_COMMAND_LEN];

    // Encoding max exactly and over-max should produce same torque value
    enc.encode(20.0, &mut out1);

    let mut enc2 = TorqueCommandEncoder::new(20.0);
    enc2.encode(100.0, &mut out2);

    // Data bytes [10..14] carry the torque value; should be identical
    assert_eq!(out1[10..14], out2[10..14], "over-max should clamp to max");
    Ok(())
}

#[test]
fn torque_encoder_zero_output_is_deterministic() {
    let mut enc = TorqueCommandEncoder::new(20.0);
    let mut out1 = [0u8; TORQUE_COMMAND_LEN];
    let mut out2 = [0u8; TORQUE_COMMAND_LEN];

    enc.encode(0.0, &mut out1);
    // Reset encoder to same initial state
    let mut enc2 = TorqueCommandEncoder::new(20.0);
    enc2.encode(0.0, &mut out2);

    assert_eq!(out1, out2, "zero torque should be deterministic");
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Conversion helpers — position_degrees, velocity_rpm, torque_nm
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn position_degrees_full_rotation() -> Result<(), Box<dyn std::error::Error>> {
    let state = SmFeedbackState {
        motor: SmMotorFeedback {
            position: 14400,
            ..Default::default()
        },
        ..Default::default()
    };
    let degrees = state.position_degrees(14400);
    assert!(
        (degrees - 360.0).abs() < 0.01,
        "expected 360.0, got {}",
        degrees
    );
    Ok(())
}

#[test]
fn position_degrees_half_rotation() -> Result<(), Box<dyn std::error::Error>> {
    let state = SmFeedbackState {
        motor: SmMotorFeedback {
            position: 2048,
            ..Default::default()
        },
        ..Default::default()
    };
    let degrees = state.position_degrees(4096);
    assert!(
        (degrees - 180.0).abs() < 0.01,
        "expected 180.0, got {}",
        degrees
    );
    Ok(())
}

#[test]
fn position_degrees_negative() -> Result<(), Box<dyn std::error::Error>> {
    let state = SmFeedbackState {
        motor: SmMotorFeedback {
            position: -7200,
            ..Default::default()
        },
        ..Default::default()
    };
    let degrees = state.position_degrees(14400);
    assert!(
        (degrees - (-180.0)).abs() < 0.01,
        "expected -180.0, got {}",
        degrees
    );
    Ok(())
}

#[test]
fn velocity_rpm_one_rotation_per_second() -> Result<(), Box<dyn std::error::Error>> {
    // 1 RPS = 60 RPM. If encoder CPR is 4096, velocity of 4096 counts/sec = 60 RPM
    let state = SmFeedbackState {
        motor: SmMotorFeedback {
            velocity: 4096,
            ..Default::default()
        },
        ..Default::default()
    };
    let rpm = state.velocity_rpm(4096);
    assert!((rpm - 60.0).abs() < 0.01, "expected 60.0, got {}", rpm);
    Ok(())
}

#[test]
fn torque_nm_with_q8_8_scaling() -> Result<(), Box<dyn std::error::Error>> {
    // torque_nm = torque_raw * torque_constant / 256
    // torque_raw=256, constant=0.1 => 256 * 0.1 / 256 = 0.1 Nm
    let state = SmFeedbackState {
        motor: SmMotorFeedback {
            torque: 256,
            ..Default::default()
        },
        ..Default::default()
    };
    let nm = state.torque_nm(0.1);
    assert!((nm - 0.1).abs() < 0.001, "expected 0.1, got {}", nm);

    // torque_raw=512, constant=0.1 => 512 * 0.1 / 256 = 0.2 Nm
    let state2 = SmFeedbackState {
        motor: SmMotorFeedback {
            torque: 512,
            ..Default::default()
        },
        ..Default::default()
    };
    let nm2 = state2.torque_nm(0.1);
    assert!((nm2 - 0.2).abs() < 0.001, "expected 0.2, got {}", nm2);
    Ok(())
}

#[test]
fn feedback_empty_has_defaults() {
    let state = SmFeedbackState::empty();
    assert_eq!(state.seq, 0);
    assert_eq!(state.motor.position, 0);
    assert_eq!(state.motor.velocity, 0);
    assert_eq!(state.motor.torque, 0);
    assert_eq!(state.bus_voltage, 0);
    assert_eq!(state.motor_current, 0);
    assert_eq!(state.temperature, 0);
    assert!(!state.connected);
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Known-good byte sequences — derived from protocol spec
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn known_bytes_get_status_seq0() -> Result<(), Box<dyn std::error::Error>> {
    let buf = build_get_status(0);
    assert_eq!(buf[0], 0x01, "report ID");
    assert_eq!(buf[1], 0x00, "seq");
    assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 0x0003, "GetStatus");
    // All param/data bytes should be zero
    for i in 4..14 {
        assert_eq!(buf[i], 0x00, "byte {} should be zero for GetStatus", i);
    }
    // CRC must match
    let expected_crc = compute_crc8(&buf[..14]);
    assert_eq!(buf[14], expected_crc);
    Ok(())
}

#[test]
fn known_bytes_set_torque_positive_256() -> Result<(), Box<dyn std::error::Error>> {
    let buf = build_set_torque_command(256, 5);
    assert_eq!(buf[0], 0x01);
    assert_eq!(buf[1], 0x05);
    assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 0x0010); // SetTorque
    // data field at bytes 10..14 carries 256 as i32 LE
    let data_val = i32::from_le_bytes([buf[10], buf[11], buf[12], buf[13]]);
    assert_eq!(data_val, 256);
    let expected_crc = compute_crc8(&buf[..14]);
    assert_eq!(buf[14], expected_crc);
    Ok(())
}

#[test]
fn known_bytes_set_torque_negative() -> Result<(), Box<dyn std::error::Error>> {
    let buf = build_set_torque_command(-1000, 10);
    let data_val = i32::from_le_bytes([buf[10], buf[11], buf[12], buf[13]]);
    assert_eq!(data_val, -1000);
    Ok(())
}

#[test]
fn known_bytes_set_torque_with_velocity_packing() -> Result<(), Box<dyn std::error::Error>> {
    // Torque=256 (0x0100), velocity=128 (0x0080)
    // Combined: (torque << 16) | (velocity & 0xFFFF) = 0x01000080
    let buf = build_set_torque_command_with_velocity(256, 128, 0);
    let combined = i32::from_le_bytes([buf[10], buf[11], buf[12], buf[13]]);
    let expected = (256i32 << 16) | (128i32 & 0xFFFF);
    assert_eq!(combined, expected);
    Ok(())
}

#[test]
fn known_bytes_get_parameter_0x1001() -> Result<(), Box<dyn std::error::Error>> {
    let buf = build_get_parameter(0x1001, 0);
    assert_eq!(buf[0], 0x01);
    assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 0x0001); // GetParameter
    assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), 0x1001); // param addr
    Ok(())
}

#[test]
fn known_bytes_set_parameter_0x1001_value_1000() -> Result<(), Box<dyn std::error::Error>> {
    let buf = build_set_parameter(0x1001, 1000, 3);
    assert_eq!(buf[0], 0x01);
    assert_eq!(buf[1], 0x03); // seq
    assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 0x0002); // SetParameter
    assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), 0x1001);
    let value = i32::from_le_bytes([buf[6], buf[7], buf[8], buf[9]]);
    assert_eq!(value, 1000);
    Ok(())
}

#[test]
fn known_bytes_device_enable_on() -> Result<(), Box<dyn std::error::Error>> {
    let buf = build_device_enable(true, 0);
    assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 0x0002); // SetParameter
    assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), 0x1001); // SMP_CONTROL_BITS1
    let value = i32::from_le_bytes([buf[6], buf[7], buf[8], buf[9]]);
    assert_eq!(value, 1);
    Ok(())
}

#[test]
fn known_bytes_device_enable_off() -> Result<(), Box<dyn std::error::Error>> {
    let buf = build_device_enable(false, 0);
    let value = i32::from_le_bytes([buf[6], buf[7], buf[8], buf[9]]);
    assert_eq!(value, 0);
    Ok(())
}

#[test]
fn known_bytes_set_zero_position() -> Result<(), Box<dyn std::error::Error>> {
    let buf = build_set_zero_position(7);
    assert_eq!(buf[0], 0x01);
    assert_eq!(buf[1], 7);
    assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 0x0013); // SetZero
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. Device identity and product ID verification
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn device_identity_all_known_pids() {
    let known = [
        (0x6050u16, "IONI", SmDeviceCategory::Wheelbase, true),
        (0x6051, "IONI Premium", SmDeviceCategory::Wheelbase, true),
        (0x6052, "ARGON", SmDeviceCategory::Wheelbase, true),
    ];
    for (pid, label, expected_cat, expected_ffb) in known {
        let identity = identify_device(pid);
        assert_eq!(
            identity.product_id, pid,
            "product_id mismatch for {}",
            label
        );
        assert_eq!(
            identity.category, expected_cat,
            "category mismatch for {}",
            label
        );
        assert_eq!(
            identity.supports_ffb, expected_ffb,
            "supports_ffb mismatch for {}",
            label
        );
        assert!(
            identity.max_torque_nm.is_some(),
            "max_torque_nm should be Some for {}",
            label
        );
        assert!(
            identity.max_rpm.is_some(),
            "max_rpm should be Some for {}",
            label
        );
    }
}

#[test]
fn device_identity_unknown_pid() {
    for pid in [0x0000u16, 0x0001, 0x6053, 0xFFFF] {
        let identity = identify_device(pid);
        assert_eq!(identity.category, SmDeviceCategory::Unknown);
        assert!(!identity.supports_ffb);
        assert!(identity.max_torque_nm.is_none());
        assert!(identity.max_rpm.is_none());
    }
}

#[test]
fn is_wheelbase_product_known_devices() {
    assert!(is_wheelbase_product(0x6050));
    assert!(is_wheelbase_product(0x6051));
    assert!(is_wheelbase_product(0x6052));
    assert!(!is_wheelbase_product(0x0000));
    assert!(!is_wheelbase_product(0xFFFF));
}

#[test]
fn sm_device_identity_matches_identify_device() {
    for pid in [0x6050u16, 0x6051, 0x6052, 0xFFFF] {
        let a = identify_device(pid);
        let b = sm_device_identity(pid);
        assert_eq!(a.product_id, b.product_id);
        assert_eq!(a.name, b.name);
        assert_eq!(a.category, b.category);
        assert_eq!(a.supports_ffb, b.supports_ffb);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. Protocol constants verification
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn protocol_constants_have_expected_values() {
    assert_eq!(TORQUE_COMMAND_LEN, 15);
    assert_eq!(FEEDBACK_REPORT_LEN, 64);
    assert_eq!(SETPARAM_REPORT_LEN, 15);
    assert_eq!(STATUS_REPORT_LEN, 15);
}

#[test]
fn command_type_u16_values_match_spec() {
    assert_eq!(SmCommandType::GetParameter.to_u16(), 0x0001);
    assert_eq!(SmCommandType::SetParameter.to_u16(), 0x0002);
    assert_eq!(SmCommandType::GetStatus.to_u16(), 0x0003);
    assert_eq!(SmCommandType::SetTorque.to_u16(), 0x0010);
    assert_eq!(SmCommandType::SetVelocity.to_u16(), 0x0011);
    assert_eq!(SmCommandType::SetPosition.to_u16(), 0x0012);
    assert_eq!(SmCommandType::SetZero.to_u16(), 0x0013);
    assert_eq!(SmCommandType::Reset.to_u16(), 0xFFFF);
}

#[test]
fn command_type_from_u16_returns_none_for_unrecognized() {
    let invalid_vals: &[u16] = &[
        0x0000, 0x0004, 0x000F, 0x0014, 0x0099, 0x7FFF, 0x8000, 0xFFFE,
    ];
    for &val in invalid_vals {
        assert!(
            SmCommandType::from_u16(val).is_none(),
            "0x{:04X} should not parse to a command type",
            val
        );
    }
}

#[test]
fn command_type_from_u16_roundtrip_all_valid() {
    let all_types = [
        SmCommandType::GetParameter,
        SmCommandType::SetParameter,
        SmCommandType::GetStatus,
        SmCommandType::SetTorque,
        SmCommandType::SetVelocity,
        SmCommandType::SetPosition,
        SmCommandType::SetZero,
        SmCommandType::Reset,
    ];
    for cmd_type in all_types {
        let val = cmd_type.to_u16();
        let recovered = SmCommandType::from_u16(val);
        assert_eq!(recovered, Some(cmd_type));
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 11. Cross-builder consistency: every build_* produces decodeable packets
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn all_builders_produce_decodeable_packets() -> Result<(), Box<dyn std::error::Error>> {
    let packets: Vec<(&str, [u8; 15])> = vec![
        ("build_set_torque_command", build_set_torque_command(100, 0)),
        (
            "build_set_torque_command_with_velocity",
            build_set_torque_command_with_velocity(100, 50, 0),
        ),
        ("build_get_parameter", build_get_parameter(0x1001, 0)),
        ("build_set_parameter", build_set_parameter(0x1001, 1000, 0)),
        ("build_get_status", build_get_status(0)),
        ("build_set_zero_position", build_set_zero_position(0)),
        ("build_device_enable(true)", build_device_enable(true, 0)),
        ("build_device_enable(false)", build_device_enable(false, 0)),
    ];

    for (name, buf) in &packets {
        let decoded = decode_command(buf);
        assert!(
            decoded.is_ok(),
            "builder '{}' produced invalid packet",
            name
        );
    }
    Ok(())
}

#[test]
fn all_builders_have_correct_report_id() {
    let packets: Vec<[u8; 15]> = vec![
        build_set_torque_command(0, 0),
        build_set_torque_command_with_velocity(0, 0, 0),
        build_get_parameter(0, 0),
        build_set_parameter(0, 0, 0),
        build_get_status(0),
        build_set_zero_position(0),
        build_device_enable(true, 0),
        build_device_enable(false, 0),
    ];
    for (i, buf) in packets.iter().enumerate() {
        assert_eq!(buf[0], 0x01, "builder {} report ID mismatch", i);
    }
}

#[test]
fn builders_with_different_seqs_produce_different_packets() {
    let buf0 = build_get_status(0);
    let buf1 = build_get_status(1);
    assert_ne!(
        buf0, buf1,
        "different seqs should produce different packets"
    );
    assert_eq!(buf0[0], buf1[0]); // same report ID
    assert_ne!(buf0[1], buf1[1]); // different seq
    assert_ne!(buf0[14], buf1[14]); // different CRC (because seq differs)
}

// ═══════════════════════════════════════════════════════════════════════════════
// 12. Feedback report with exact 64-byte boundary
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn feedback_report_exactly_64_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let data = vec![0x02; 64]; // all 0x02 — first byte is valid report ID
    let _state = parse_feedback_report(&data)?;
    Ok(())
}

#[test]
fn feedback_report_larger_than_64_still_parses() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; 128];
    data[0] = 0x02;
    let state = parse_feedback_report(&data)?;
    assert_eq!(state.seq, 0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 13. Proptest — fuzzing packet construction and parsing
// ═══════════════════════════════════════════════════════════════════════════════

fn arb_command_type() -> impl Strategy<Value = SmCommandType> {
    prop_oneof![
        Just(SmCommandType::GetParameter),
        Just(SmCommandType::SetParameter),
        Just(SmCommandType::GetStatus),
        Just(SmCommandType::SetTorque),
        Just(SmCommandType::SetVelocity),
        Just(SmCommandType::SetPosition),
        Just(SmCommandType::SetZero),
        Just(SmCommandType::Reset),
    ]
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(500))]

    /// Any command with a valid type encodes to 15 bytes and round-trips.
    #[test]
    fn prop_encode_decode_roundtrip_all_types(
        seq in any::<u8>(),
        cmd_type in arb_command_type(),
        param_addr in any::<u16>(),
        param_value in any::<i32>(),
        data in any::<i32>(),
    ) {
        let cmd = SmCommand::new(seq, cmd_type)
            .with_param(param_addr, param_value)
            .with_data(data);
        let mut buf = [0u8; 15];
        let result = encode_command(&cmd, &mut buf);
        prop_assert!(result.is_ok());
        let len = result.map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(len, 15);

        let decoded = decode_command(&buf)
            .map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(decoded.seq, seq);
        prop_assert_eq!(decoded.cmd_type, cmd_type);
        prop_assert_eq!(decoded.param_addr, Some(param_addr));
        prop_assert_eq!(decoded.param_value, Some(param_value));
        prop_assert_eq!(decoded.data, Some(data));
    }

    /// CRC byte (index 14) matches independent computation for any valid command.
    #[test]
    fn prop_crc_matches_independent_computation(
        seq in any::<u8>(),
        cmd_type in arb_command_type(),
        data in any::<i32>(),
    ) {
        let cmd = SmCommand::new(seq, cmd_type).with_data(data);
        let mut buf = [0u8; 15];
        let _ = encode_command(&cmd, &mut buf);
        let expected_crc = compute_crc8(&buf[..14]);
        prop_assert_eq!(buf[14], expected_crc);
    }

    /// Corrupting any single byte in the payload is always detected by CRC.
    #[test]
    fn prop_single_byte_corruption_detected(
        seq in any::<u8>(),
        data in any::<i32>(),
        corrupt_idx in 0usize..14,
        corrupt_mask in 1u8..=255,
    ) {
        let cmd = SmCommand::new(seq, SmCommandType::SetTorque).with_data(data);
        let mut buf = [0u8; 15];
        let _ = encode_command(&cmd, &mut buf);
        buf[corrupt_idx] ^= corrupt_mask;
        let result = decode_command(&buf);
        // Must be CRC error or some other error — never Ok
        prop_assert!(result.is_err());
    }

    /// Feedback report parsing never panics on arbitrary 64+ byte inputs.
    #[test]
    fn prop_feedback_parse_never_panics(ref data in proptest::collection::vec(any::<u8>(), 64..128)) {
        let mut report = data.clone();
        report[0] = 0x02; // valid report ID so we test parsing logic
        let _ = parse_feedback_report(&report);
    }

    /// Feedback parsing with valid report ID always succeeds for 64+ byte input.
    #[test]
    fn prop_feedback_parse_valid_report_id_succeeds(ref data in proptest::collection::vec(any::<u8>(), 64..128)) {
        let mut report = data.clone();
        report[0] = 0x02;
        let result = parse_feedback_report(&report);
        prop_assert!(result.is_ok());
    }

    /// Feedback parsing with invalid report ID always fails.
    #[test]
    fn prop_feedback_parse_invalid_report_id_fails(
        report_id in (0u8..=0xFF).prop_filter("not feedback ID", |&id| id != 0x02),
        ref data in proptest::collection::vec(any::<u8>(), 64..128),
    ) {
        let mut report = data.clone();
        report[0] = report_id;
        let result = parse_feedback_report(&report);
        prop_assert!(result.is_err());
    }

    /// Torque encoder always produces valid 15-byte packets.
    #[test]
    fn prop_torque_encoder_valid_output(
        max_torque in 0.01f32..100.0,
        torque in -200.0f32..200.0,
    ) {
        let mut enc = TorqueCommandEncoder::new(max_torque);
        let mut out = [0u8; TORQUE_COMMAND_LEN];
        let len = enc.encode(torque, &mut out);
        prop_assert_eq!(len, 15);
        prop_assert_eq!(out[0], 0x01);
        // CRC must be valid
        let expected_crc = compute_crc8(&out[..14]);
        prop_assert_eq!(out[14], expected_crc);
    }

    /// Torque encoder with velocity always produces valid packets.
    #[test]
    fn prop_torque_encoder_with_velocity_valid(
        torque in -100.0f32..100.0,
        velocity in -10000.0f32..10000.0,
    ) {
        let mut enc = TorqueCommandEncoder::new(20.0);
        let mut out = [0u8; TORQUE_COMMAND_LEN];
        let len = enc.encode_with_velocity(torque, velocity, &mut out);
        prop_assert_eq!(len, 15);
        prop_assert_eq!(out[0], 0x01);
    }

    /// position_degrees is always finite for valid encoder CPR.
    #[test]
    fn prop_position_degrees_finite(
        position in any::<i32>(),
        encoder_cpr in 1u32..=100_000,
    ) {
        let state = SmFeedbackState {
            motor: SmMotorFeedback { position, ..Default::default() },
            ..Default::default()
        };
        let degrees = state.position_degrees(encoder_cpr);
        prop_assert!(degrees.is_finite());
    }

    /// velocity_rpm is always finite for valid encoder CPR.
    #[test]
    fn prop_velocity_rpm_finite(
        velocity in any::<i32>(),
        encoder_cpr in 1u32..=100_000,
    ) {
        let state = SmFeedbackState {
            motor: SmMotorFeedback { velocity, ..Default::default() },
            ..Default::default()
        };
        let rpm = state.velocity_rpm(encoder_cpr);
        prop_assert!(rpm.is_finite());
    }

    /// torque_nm is always finite for reasonable torque constants.
    #[test]
    fn prop_torque_nm_finite(
        torque in any::<i16>(),
        constant in 0.001f32..10.0,
    ) {
        let state = SmFeedbackState {
            motor: SmMotorFeedback { torque, ..Default::default() },
            ..Default::default()
        };
        let nm = state.torque_nm(constant);
        prop_assert!(nm.is_finite());
    }

    /// build_set_torque_command produces decodeable SetTorque.
    #[test]
    fn prop_build_set_torque_decodeable(torque in any::<i16>(), seq in any::<u8>()) {
        let buf = build_set_torque_command(torque, seq);
        let decoded = decode_command(&buf);
        prop_assert!(decoded.is_ok());
        let decoded = decoded.map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(decoded.seq, seq);
        prop_assert_eq!(decoded.cmd_type, SmCommandType::SetTorque);
    }

    /// build_set_torque_command_with_velocity produces decodeable SetTorque.
    #[test]
    fn prop_build_set_torque_velocity_decodeable(
        torque in any::<i16>(),
        velocity in any::<i16>(),
        seq in any::<u8>(),
    ) {
        let buf = build_set_torque_command_with_velocity(torque, velocity, seq);
        let decoded = decode_command(&buf);
        prop_assert!(decoded.is_ok());
    }

    /// build_get_parameter produces decodeable GetParameter with correct address.
    #[test]
    fn prop_build_get_parameter_decodeable(addr in any::<u16>(), seq in any::<u8>()) {
        let buf = build_get_parameter(addr, seq);
        let decoded = decode_command(&buf)
            .map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(decoded.cmd_type, SmCommandType::GetParameter);
        prop_assert_eq!(decoded.param_addr, Some(addr));
        prop_assert_eq!(decoded.seq, seq);
    }

    /// build_set_parameter produces decodeable SetParameter with correct address and value.
    #[test]
    fn prop_build_set_parameter_decodeable(
        addr in any::<u16>(),
        value in any::<i32>(),
        seq in any::<u8>(),
    ) {
        let buf = build_set_parameter(addr, value, seq);
        let decoded = decode_command(&buf)
            .map_err(|e| proptest::test_runner::TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(decoded.cmd_type, SmCommandType::SetParameter);
        prop_assert_eq!(decoded.param_addr, Some(addr));
        prop_assert_eq!(decoded.param_value, Some(value));
    }

    /// identify_device returns correct product_id for any input.
    #[test]
    fn prop_identify_device_echoes_pid(pid in any::<u16>()) {
        let identity = identify_device(pid);
        prop_assert_eq!(identity.product_id, pid);
    }

    /// is_wheelbase_product is consistent with identify_device category.
    #[test]
    fn prop_is_wheelbase_consistent(pid in any::<u16>()) {
        let identity = identify_device(pid);
        let is_wb = is_wheelbase_product(pid);
        let expected = matches!(identity.category, SmDeviceCategory::Wheelbase);
        prop_assert_eq!(is_wb, expected);
    }

    /// Arbitrary 15-byte packet decoding never panics.
    #[test]
    fn prop_decode_never_panics(ref data in any::<[u8; 15]>()) {
        let _ = decode_command(data);
    }

    /// Sub-15-byte buffers always produce InvalidLength on decode.
    #[test]
    fn prop_short_buffer_decode_fails(len in 0usize..15) {
        let data = vec![0u8; len];
        let result = decode_command(&data);
        let is_invalid_len = matches!(result, Err(SmError::InvalidLength { .. }));
        prop_assert!(is_invalid_len);
    }

    /// Sub-15-byte buffers always produce InvalidLength on encode.
    #[test]
    fn prop_short_buffer_encode_fails(len in 0usize..15) {
        let cmd = SmCommand::new(0, SmCommandType::GetStatus);
        let mut buf = vec![0u8; len];
        let result = encode_command(&cmd, &mut buf);
        let is_invalid_len = matches!(result, Err(SmError::InvalidLength { .. }));
        prop_assert!(is_invalid_len);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 14. Vendor and product ID constants
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn vendor_product_id_constants() {
    use racing_wheel_simplemotion_v2::{
        ARGON_PRODUCT_ID, ARGON_VENDOR_ID, IONI_PRODUCT_ID, IONI_PRODUCT_ID_PREMIUM,
        IONI_VENDOR_ID, OSW_VENDOR_ID, sm_product_ids,
    };

    // All vendors share the Granite Devices VID
    assert_eq!(IONI_VENDOR_ID, 0x1D50);
    assert_eq!(ARGON_VENDOR_ID, 0x1D50);
    assert_eq!(OSW_VENDOR_ID, 0x1D50);

    // Product IDs
    assert_eq!(IONI_PRODUCT_ID, 0x6050);
    assert_eq!(IONI_PRODUCT_ID_PREMIUM, 0x6051);
    assert_eq!(ARGON_PRODUCT_ID, 0x6052);

    // Aliased constants in product_ids module
    assert_eq!(sm_product_ids::IONI, IONI_PRODUCT_ID);
    assert_eq!(sm_product_ids::IONI_PREMIUM, IONI_PRODUCT_ID_PREMIUM);
    assert_eq!(sm_product_ids::ARGON, ARGON_PRODUCT_ID);
    assert_eq!(sm_product_ids::SIMUCUBE_1, IONI_PRODUCT_ID);
    assert_eq!(sm_product_ids::SIMUCUBE_2, IONI_PRODUCT_ID_PREMIUM);
    assert_eq!(sm_product_ids::SIMUCUBE_SPORT, ARGON_PRODUCT_ID);
    assert_eq!(sm_product_ids::SIMUCUBE_PRO, IONI_PRODUCT_ID_PREMIUM);
}

// ═══════════════════════════════════════════════════════════════════════════════
// 15. SmCommand builder pattern
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn sm_command_new_defaults() {
    let cmd = SmCommand::new(0, SmCommandType::GetStatus);
    assert_eq!(cmd.seq, 0);
    assert_eq!(cmd.cmd_type, SmCommandType::GetStatus);
    assert!(cmd.param_addr.is_none());
    assert!(cmd.param_value.is_none());
    assert!(cmd.data.is_none());
}

#[test]
fn sm_command_with_param_sets_fields() {
    let cmd = SmCommand::new(5, SmCommandType::SetParameter).with_param(0x2000, -500);
    assert_eq!(cmd.param_addr, Some(0x2000));
    assert_eq!(cmd.param_value, Some(-500));
}

#[test]
fn sm_command_with_data_sets_field() {
    let cmd = SmCommand::new(5, SmCommandType::SetTorque).with_data(9999);
    assert_eq!(cmd.data, Some(9999));
}

#[test]
fn sm_command_chained_builders() {
    let cmd = SmCommand::new(10, SmCommandType::SetParameter)
        .with_param(0x1001, 42)
        .with_data(99);
    assert_eq!(cmd.seq, 10);
    assert_eq!(cmd.cmd_type, SmCommandType::SetParameter);
    assert_eq!(cmd.param_addr, Some(0x1001));
    assert_eq!(cmd.param_value, Some(42));
    assert_eq!(cmd.data, Some(99));
}

// ═══════════════════════════════════════════════════════════════════════════════
// 16. Encoding buffer size — larger buffers still work
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn encode_with_oversized_buffer() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(0, SmCommandType::GetStatus);
    let mut buf = [0u8; 64];
    let len = encode_command(&cmd, &mut buf)?;
    assert_eq!(len, 15);
    // Bytes beyond 15 should be zeroed (encode fills then writes)
    for i in 15..64 {
        assert_eq!(buf[i], 0, "byte {} beyond packet should be zero", i);
    }
    Ok(())
}
