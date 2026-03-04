//! Deep protocol tests for SimpleMotion V2 command encoding, response parsing,
//! CRC validation, address management, error handling, property tests, and snapshots.

use racing_wheel_simplemotion_v2::commands::{
    SmCommand, SmCommandType, SmStatus, decode_command, encode_command,
};
use racing_wheel_simplemotion_v2::error::SmError;
use racing_wheel_simplemotion_v2::{
    SmDeviceCategory, SmFeedbackState, SmMotorFeedback, TORQUE_COMMAND_LEN, TorqueCommandEncoder,
    build_device_enable, build_get_parameter, build_set_parameter, build_set_torque_command,
    build_set_torque_command_with_velocity, build_set_zero_position, identify_device,
    is_wheelbase_product, parse_feedback_report,
};

// ── Helper: CRC8 matching the crate-internal algorithm ──────────────────────

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

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Command encoding — all SM-V2 command types
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn encode_get_parameter_has_correct_layout() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(1, SmCommandType::GetParameter).with_param(0x2001, 0);
    let mut buf = [0u8; 15];
    let len = encode_command(&cmd, &mut buf)?;
    assert_eq!(len, 15);
    assert_eq!(buf[0], 0x01); // report id
    assert_eq!(buf[1], 1); // seq
    assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 0x0001); // cmd type
    assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), 0x2001); // param addr
    Ok(())
}

#[test]
fn encode_set_parameter_embeds_value() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(2, SmCommandType::SetParameter).with_param(0x3000, -42);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;
    assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 0x0002);
    assert_eq!(i32::from_le_bytes([buf[6], buf[7], buf[8], buf[9]]), -42);
    Ok(())
}

#[test]
fn encode_get_status_zeroes_param_fields() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(3, SmCommandType::GetStatus);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;
    assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 0x0003);
    // No param_addr / param_value set → bytes 4-9 should be 0
    assert_eq!(&buf[4..10], &[0; 6]);
    Ok(())
}

#[test]
fn encode_set_torque_packs_data_in_bytes_10_13() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(4, SmCommandType::SetTorque).with_data(0x0000_1234);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;
    assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 0x0010);
    assert_eq!(
        i32::from_le_bytes([buf[10], buf[11], buf[12], buf[13]]),
        0x1234
    );
    Ok(())
}

#[test]
fn encode_set_velocity_preserves_negative_data() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(5, SmCommandType::SetVelocity).with_data(-7777);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;
    let decoded = decode_command(&buf)?;
    assert_eq!(decoded.cmd_type, SmCommandType::SetVelocity);
    assert_eq!(decoded.data, Some(-7777));
    Ok(())
}

#[test]
fn encode_set_position_large_value() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(6, SmCommandType::SetPosition).with_data(i32::MAX);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;
    let decoded = decode_command(&buf)?;
    assert_eq!(decoded.data, Some(i32::MAX));
    Ok(())
}

#[test]
fn encode_set_zero_roundtrips() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(7, SmCommandType::SetZero).with_data(0);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;
    let decoded = decode_command(&buf)?;
    assert_eq!(decoded.cmd_type, SmCommandType::SetZero);
    assert_eq!(decoded.data, Some(0));
    Ok(())
}

#[test]
fn encode_reset_roundtrips() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(8, SmCommandType::Reset);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;
    let decoded = decode_command(&buf)?;
    assert_eq!(decoded.cmd_type, SmCommandType::Reset);
    assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 0xFFFF);
    Ok(())
}

#[test]
fn encode_all_types_produce_15_bytes() -> Result<(), Box<dyn std::error::Error>> {
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
    for ct in types {
        let cmd = SmCommand::new(0, ct);
        let mut buf = [0u8; 15];
        let len = encode_command(&cmd, &mut buf)?;
        assert_eq!(len, 15, "unexpected length for {ct:?}");
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Response parsing — status, position, velocity, torque
// ═══════════════════════════════════════════════════════════════════════════════

fn make_feedback(position: i32, velocity: i32, torque: i16, status: u8) -> Vec<u8> {
    let mut data = vec![0u8; 64];
    data[0] = 0x02; // feedback report id
    data[1] = 0x01; // seq
    data[2] = status;
    let pos_bytes = position.to_le_bytes();
    data[4..8].copy_from_slice(&pos_bytes);
    let vel_bytes = velocity.to_le_bytes();
    data[8..12].copy_from_slice(&vel_bytes);
    let torque_bytes = torque.to_le_bytes();
    data[12..14].copy_from_slice(&torque_bytes);
    data
}

#[test]
fn parse_status_ok() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_feedback(0, 0, 0, 0);
    let fb = parse_feedback_report(&data)?;
    assert_eq!(fb.status, SmStatus::Ok);
    Ok(())
}

#[test]
fn parse_status_error() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_feedback(0, 0, 0, 1);
    let fb = parse_feedback_report(&data)?;
    assert_eq!(fb.status, SmStatus::Error);
    Ok(())
}

#[test]
fn parse_status_busy() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_feedback(0, 0, 0, 2);
    let fb = parse_feedback_report(&data)?;
    assert_eq!(fb.status, SmStatus::Busy);
    Ok(())
}

#[test]
fn parse_status_not_ready() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_feedback(0, 0, 0, 3);
    let fb = parse_feedback_report(&data)?;
    assert_eq!(fb.status, SmStatus::NotReady);
    Ok(())
}

#[test]
fn parse_status_unknown_high_value() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_feedback(0, 0, 0, 200);
    let fb = parse_feedback_report(&data)?;
    assert_eq!(fb.status, SmStatus::Unknown);
    Ok(())
}

#[test]
fn parse_position_positive() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_feedback(50000, 0, 0, 0);
    let fb = parse_feedback_report(&data)?;
    assert_eq!(fb.motor.position, 50000);
    Ok(())
}

#[test]
fn parse_position_negative() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_feedback(-12345, 0, 0, 0);
    let fb = parse_feedback_report(&data)?;
    assert_eq!(fb.motor.position, -12345);
    Ok(())
}

#[test]
fn parse_velocity_positive() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_feedback(0, 8000, 0, 0);
    let fb = parse_feedback_report(&data)?;
    assert_eq!(fb.motor.velocity, 8000);
    Ok(())
}

#[test]
fn parse_velocity_negative() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_feedback(0, -3000, 0, 0);
    let fb = parse_feedback_report(&data)?;
    assert_eq!(fb.motor.velocity, -3000);
    Ok(())
}

#[test]
fn parse_torque_positive() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_feedback(0, 0, 1500, 0);
    let fb = parse_feedback_report(&data)?;
    assert_eq!(fb.motor.torque, 1500);
    Ok(())
}

#[test]
fn parse_torque_negative() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_feedback(0, 0, -2000, 0);
    let fb = parse_feedback_report(&data)?;
    assert_eq!(fb.motor.torque, -2000);
    Ok(())
}

#[test]
fn parse_torque_max_i16() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_feedback(0, 0, i16::MAX, 0);
    let fb = parse_feedback_report(&data)?;
    assert_eq!(fb.motor.torque, i16::MAX);
    Ok(())
}

#[test]
fn parse_torque_min_i16() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_feedback(0, 0, i16::MIN, 0);
    let fb = parse_feedback_report(&data)?;
    assert_eq!(fb.motor.torque, i16::MIN);
    Ok(())
}

#[test]
fn parse_combined_position_velocity_torque() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_feedback(14400, -500, 256, 0);
    let fb = parse_feedback_report(&data)?;
    assert_eq!(fb.motor.position, 14400);
    assert_eq!(fb.motor.velocity, -500);
    assert_eq!(fb.motor.torque, 256);
    // Unit conversions
    let deg = fb.position_degrees(14400);
    assert!((deg - 360.0).abs() < 0.01);
    let nm = fb.torque_nm(0.1);
    assert!((nm - 0.1).abs() < 0.01);
    Ok(())
}

#[test]
fn parse_bus_voltage_and_temperature() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = make_feedback(0, 0, 0, 0);
    data[14] = 0xE8;
    data[15] = 0x03; // 1000
    data[16] = 0x00;
    data[17] = 0x10; // motor_current = 0x1000
    data[18] = 45; // temperature
    let fb = parse_feedback_report(&data)?;
    assert_eq!(fb.bus_voltage, 1000);
    assert_eq!(fb.motor_current, 0x1000);
    assert_eq!(fb.temperature, 45);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. CRC calculation — verify CRC for known packets
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn crc_of_all_zeros_is_deterministic() {
    let data = [0u8; 14];
    let crc = compute_crc8(&data);
    // Re-compute to verify determinism
    assert_eq!(crc, compute_crc8(&data));
}

#[test]
fn crc_changes_with_any_payload_byte() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(10, SmCommandType::SetTorque).with_data(999);
    let mut original = [0u8; 15];
    encode_command(&cmd, &mut original)?;
    let original_crc = original[14];

    for i in 0..14 {
        let mut modified = original;
        modified[i] ^= 0x01; // flip one bit
        let new_crc = compute_crc8(&modified[..14]);
        assert_ne!(
            new_crc, original_crc,
            "CRC should change when byte {i} is modified"
        );
    }
    Ok(())
}

#[test]
fn crc_embedded_in_encoded_packet_is_correct() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(77, SmCommandType::SetParameter).with_param(0xABCD, 0x12345678);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;
    let expected_crc = compute_crc8(&buf[..14]);
    assert_eq!(buf[14], expected_crc);
    Ok(())
}

#[test]
fn crc_known_vector_get_status_seq0() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(0, SmCommandType::GetStatus);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;
    // The payload is 0x01, 0x00, 0x03, 0x00, then 10 zero bytes
    let expected_payload: [u8; 14] = [0x01, 0x00, 0x03, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(&buf[..14], &expected_payload);
    let crc = compute_crc8(&expected_payload);
    assert_eq!(buf[14], crc);
    assert_ne!(crc, 0, "CRC should be non-zero for GetStatus seq=0");
    Ok(())
}

#[test]
fn decode_detects_single_bit_flip_in_each_byte() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(33, SmCommandType::SetParameter).with_param(0x1001, 500);
    let mut original = [0u8; 15];
    encode_command(&cmd, &mut original)?;

    for i in 0..14 {
        for bit in 0..8 {
            let mut corrupted = original;
            corrupted[i] ^= 1 << bit;
            let result = decode_command(&corrupted);
            assert!(
                result.is_err(),
                "single bit flip at byte {i} bit {bit} was not detected"
            );
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Address management — valid and invalid addresses
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn address_zero_roundtrips() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_set_parameter(0x0000, 1, 0);
    let decoded = decode_command(&report)?;
    assert_eq!(decoded.param_addr, Some(0x0000));
    Ok(())
}

#[test]
fn address_max_roundtrips() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_set_parameter(0xFFFF, 1, 0);
    let decoded = decode_command(&report)?;
    assert_eq!(decoded.param_addr, Some(0xFFFF));
    Ok(())
}

#[test]
fn address_device_enable_is_0x1001() -> Result<(), Box<dyn std::error::Error>> {
    let enable = build_device_enable(true, 0);
    let decoded = decode_command(&enable)?;
    assert_eq!(decoded.param_addr, Some(0x1001));
    Ok(())
}

#[test]
fn get_parameter_address_preserved() -> Result<(), Box<dyn std::error::Error>> {
    for addr in [0x0000, 0x0001, 0x1001, 0x2000, 0x7FFF, 0xFFFF] {
        let report = build_get_parameter(addr, 0);
        let decoded = decode_command(&report)?;
        assert_eq!(
            decoded.param_addr,
            Some(addr),
            "address {addr:#06x} not preserved"
        );
    }
    Ok(())
}

#[test]
fn device_identity_known_pids_are_wheelbases() {
    for pid in [0x6050, 0x6051, 0x6052] {
        let id = identify_device(pid);
        assert_eq!(id.category, SmDeviceCategory::Wheelbase);
        assert!(id.supports_ffb);
        assert!(is_wheelbase_product(pid));
    }
}

#[test]
fn device_identity_unknown_pid_not_wheelbase() {
    for pid in [0x0000, 0x1234, 0xFFFF, 0x604F, 0x6053] {
        let id = identify_device(pid);
        assert_eq!(id.category, SmDeviceCategory::Unknown);
        assert!(!id.supports_ffb);
        assert!(!is_wheelbase_product(pid));
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Error response parsing
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn error_invalid_length_encode() {
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
fn error_invalid_length_decode_empty() {
    let result = decode_command(&[]);
    assert!(matches!(result, Err(SmError::InvalidLength { .. })));
}

#[test]
fn error_invalid_length_decode_short() {
    let result = decode_command(&[0u8; 7]);
    assert!(matches!(
        result,
        Err(SmError::InvalidLength {
            expected: 15,
            actual: 7,
        })
    ));
}

#[test]
fn error_crc_mismatch_on_zeroed_crc() {
    let mut buf = [0u8; 15];
    buf[0] = 0x01;
    buf[2] = 0x10; // SetTorque type
    buf[14] = 0x00;
    let result = decode_command(&buf);
    assert!(matches!(result, Err(SmError::CrcMismatch { .. })));
}

#[test]
fn error_invalid_command_type_in_decode() -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = [0u8; 15];
    buf[0] = 0x01;
    buf[2] = 0x99; // invalid type
    buf[3] = 0x00;
    let crc = compute_crc8(&buf[..14]);
    buf[14] = crc;
    let result = decode_command(&buf);
    assert!(matches!(result, Err(SmError::InvalidCommandType(_))));
    Ok(())
}

#[test]
fn error_feedback_wrong_report_id() {
    let mut data = vec![0u8; 64];
    data[0] = 0x01; // wrong — should be 0x02
    let result = parse_feedback_report(&data);
    assert!(matches!(result, Err(SmError::InvalidCommandType(0x01))));
}

#[test]
fn error_feedback_too_short() {
    let data = vec![0u8; 32];
    let result = parse_feedback_report(&data);
    assert!(matches!(
        result,
        Err(SmError::InvalidLength {
            expected: 64,
            actual: 32,
        })
    ));
}

#[test]
fn error_display_messages() {
    let err1 = SmError::InvalidLength {
        expected: 15,
        actual: 10,
    };
    assert!(err1.to_string().contains("15"));
    assert!(err1.to_string().contains("10"));

    let err2 = SmError::CrcMismatch {
        expected: 0xAA,
        actual: 0xBB,
    };
    assert!(err2.to_string().contains("CRC"));

    let err3 = SmError::InvalidCommandType(0x99);
    assert!(err3.to_string().contains("command type"));
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Property tests — any valid command encodes to valid bytes
// ═══════════════════════════════════════════════════════════════════════════════

mod property_tests {
    use super::*;
    use proptest::prelude::*;

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

        #[test]
        fn prop_any_command_encodes_to_valid_packet(
            seq in 0u8..=255,
            cmd_type in arb_command_type(),
            param_addr in 0u16..=0xFFFF,
            param_val in any::<i32>(),
            data in any::<i32>(),
        ) {
            let cmd = SmCommand::new(seq, cmd_type)
                .with_param(param_addr, param_val)
                .with_data(data);
            let mut buf = [0u8; 15];
            let len = encode_command(&cmd, &mut buf).map_err(|e| {
                proptest::test_runner::TestCaseError::Fail(format!("{e}").into())
            })?;
            prop_assert_eq!(len, 15);
            prop_assert_eq!(buf[0], 0x01);
            prop_assert_eq!(buf[1], seq);
            // CRC must be valid
            let expected_crc = compute_crc8(&buf[..14]);
            prop_assert_eq!(buf[14], expected_crc);
        }

        #[test]
        fn prop_encode_decode_roundtrip_all_types(
            seq in 0u8..=255,
            cmd_type in arb_command_type(),
            data in any::<i32>(),
        ) {
            let cmd = SmCommand::new(seq, cmd_type).with_data(data);
            let mut buf = [0u8; 15];
            encode_command(&cmd, &mut buf).map_err(|e| {
                proptest::test_runner::TestCaseError::Fail(format!("{e}").into())
            })?;
            let decoded = decode_command(&buf).map_err(|e| {
                proptest::test_runner::TestCaseError::Fail(format!("{e}").into())
            })?;
            prop_assert_eq!(decoded.seq, seq);
            prop_assert_eq!(decoded.cmd_type, cmd_type);
            prop_assert_eq!(decoded.data, Some(data));
        }

        #[test]
        fn prop_corrupted_packet_always_detected(
            seq in 0u8..=255,
            flip_byte in 0usize..14,
            flip_bit in 0u8..8,
        ) {
            let cmd = SmCommand::new(seq, SmCommandType::SetTorque).with_data(1000);
            let mut buf = [0u8; 15];
            encode_command(&cmd, &mut buf).map_err(|e| {
                proptest::test_runner::TestCaseError::Fail(format!("{e}").into())
            })?;
            buf[flip_byte] ^= 1 << flip_bit;
            let result = decode_command(&buf);
            // Must fail (CRC mismatch or invalid type)
            prop_assert!(result.is_err());
        }

        #[test]
        fn prop_torque_encoder_output_bounded(
            torque in -100.0f32..100.0,
            max_torque in 0.01f32..50.0,
        ) {
            let mut enc = TorqueCommandEncoder::new(max_torque);
            let mut out = [0u8; TORQUE_COMMAND_LEN];
            let len = enc.encode(torque, &mut out);
            prop_assert_eq!(len, 15);
            prop_assert_eq!(out[0], 0x01);
            // Verify decodable
            let decoded = decode_command(&out).map_err(|e| {
                proptest::test_runner::TestCaseError::Fail(format!("{e}").into())
            })?;
            prop_assert_eq!(decoded.cmd_type, SmCommandType::SetTorque);
        }

        #[test]
        fn prop_feedback_parse_never_panics(data in proptest::collection::vec(any::<u8>(), 0..128)) {
            // Should never panic regardless of input
            let _ = parse_feedback_report(&data);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Snapshot — typical command/response byte sequences
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn snapshot_set_torque_positive() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_set_torque_command(2560, 1);
    let decoded = decode_command(&report)?;
    assert_eq!(decoded.seq, 1);
    assert_eq!(decoded.cmd_type, SmCommandType::SetTorque);
    assert_eq!(decoded.data, Some(2560));
    insta::assert_yaml_snapshot!("set_torque_pos", report.to_vec());
    Ok(())
}

#[test]
fn snapshot_set_torque_negative() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_set_torque_command(-5000, 2);
    let decoded = decode_command(&report)?;
    assert_eq!(decoded.data, Some(-5000));
    insta::assert_yaml_snapshot!("set_torque_neg", report.to_vec());
    Ok(())
}

#[test]
fn snapshot_get_status() {
    let report = racing_wheel_simplemotion_v2::build_get_status(0);
    insta::assert_yaml_snapshot!("get_status_seq0", report.to_vec());
}

#[test]
fn snapshot_device_enable() {
    let report = build_device_enable(true, 0);
    insta::assert_yaml_snapshot!("device_enable", report.to_vec());
}

#[test]
fn snapshot_set_zero_position() {
    let report = build_set_zero_position(0);
    insta::assert_yaml_snapshot!("set_zero_pos", report.to_vec());
}

#[test]
fn snapshot_torque_with_velocity() {
    let report = build_set_torque_command_with_velocity(1000, 500, 0);
    insta::assert_yaml_snapshot!("torque_with_vel", report.to_vec());
}

#[test]
fn snapshot_feedback_centered() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_feedback(0, 0, 0, 0);
    let fb = parse_feedback_report(&data)?;
    insta::assert_yaml_snapshot!(
        "feedback_centered",
        format!(
            "seq={} status={:?} pos={} vel={} torque={}",
            fb.seq, fb.status, fb.motor.position, fb.motor.velocity, fb.motor.torque
        )
    );
    Ok(())
}

#[test]
fn snapshot_feedback_full_rotation() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_feedback(14400, 1000, 256, 0);
    let fb = parse_feedback_report(&data)?;
    insta::assert_yaml_snapshot!(
        "feedback_full_rot",
        format!(
            "pos={} deg={:.2} vel={} rpm={:.2} torque={} nm={:.4}",
            fb.motor.position,
            fb.position_degrees(14400),
            fb.motor.velocity,
            fb.velocity_rpm(14400),
            fb.motor.torque,
            fb.torque_nm(0.1)
        )
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Additional edge cases
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn torque_encoder_sequence_increments() {
    let mut enc = TorqueCommandEncoder::new(20.0);
    let mut out = [0u8; TORQUE_COMMAND_LEN];
    assert_eq!(enc.sequence(), 0);
    enc.encode(0.0, &mut out);
    assert_eq!(enc.sequence(), 1);
    enc.encode(0.0, &mut out);
    assert_eq!(enc.sequence(), 2);
}

#[test]
fn torque_encoder_sequence_wraps() {
    let mut enc = TorqueCommandEncoder::new(20.0);
    let mut out = [0u8; TORQUE_COMMAND_LEN];
    for _ in 0..256 {
        enc.encode(0.0, &mut out);
    }
    assert_eq!(enc.sequence(), 0);
}

#[test]
fn feedback_connected_when_position_not_0xffff() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_feedback(1, 0, 0, 0);
    let fb = parse_feedback_report(&data)?;
    assert!(fb.connected);
    Ok(())
}

#[test]
fn feedback_disconnected_when_marker_present() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; 64];
    data[0] = 0x02;
    data[4] = 0xFF;
    data[5] = 0xFF;
    let fb = parse_feedback_report(&data)?;
    assert!(!fb.connected);
    Ok(())
}

#[test]
fn feedback_state_empty_has_defaults() {
    let fb = SmFeedbackState::empty();
    assert_eq!(fb.motor, SmMotorFeedback::default());
    assert_eq!(fb.bus_voltage, 0);
    assert_eq!(fb.temperature, 0);
    assert!(!fb.connected);
}
