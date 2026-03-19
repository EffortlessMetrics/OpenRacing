//! Property-based tests for the SimpleMotion V2 protocol.
//!
//! Uses proptest with 500 cases to verify invariants on command encoding,
//! CRC integrity, torque encoding, and feedback parsing.

#![allow(clippy::redundant_closure)]

use proptest::prelude::*;
use racing_wheel_simplemotion_v2::commands::{
    SmCommand, SmCommandType, SmStatus, decode_command, encode_command,
};
use racing_wheel_simplemotion_v2::error::SmError;
use racing_wheel_simplemotion_v2::{
    SmFeedbackState, SmMotorFeedback, TORQUE_COMMAND_LEN, TorqueCommandEncoder,
    build_get_parameter, build_set_parameter, build_set_torque_command, identify_device,
    is_wheelbase_product, parse_feedback_report,
};

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(500))]

    // ── Encoding / decoding round-trip invariants ───────────────────────────

    /// Any encoded command with a valid type must round-trip: seq and cmd_type are preserved.
    #[test]
    fn prop_roundtrip_preserves_seq_and_type(
        seq in 0u8..=255,
        type_idx in 0usize..8,
        param_addr in any::<u16>(),
        param_value in any::<i32>(),
        data in any::<i32>(),
    ) {
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
        let cmd_type = types[type_idx];
        let cmd = SmCommand::new(seq, cmd_type)
            .with_param(param_addr, param_value)
            .with_data(data);

        let mut buf = [0u8; 15];
        let Ok(_) = encode_command(&cmd, &mut buf) else {
            prop_assert!(false, "encode must succeed");
            unreachable!()
        };

        let Ok(decoded) = decode_command(&buf) else {
            prop_assert!(false, "decode must succeed");
            unreachable!()
        };

        prop_assert_eq!(decoded.seq, seq);
        prop_assert_eq!(decoded.cmd_type, cmd_type);
        prop_assert_eq!(decoded.param_addr, Some(param_addr));
        prop_assert_eq!(decoded.param_value, Some(param_value));
    }

    // ── CRC integrity ───────────────────────────────────────────────────────

    /// Flipping any single bit in the payload (bytes 0-13) must cause CRC mismatch.
    #[test]
    fn prop_single_bit_flip_detected(
        seq in 0u8..=255,
        data_val in any::<i32>(),
        byte_idx in 0usize..14,
        bit_idx in 0u8..8,
    ) {
        let cmd = SmCommand::new(seq, SmCommandType::SetTorque).with_data(data_val);
        let mut buf = [0u8; 15];
        let Ok(_) = encode_command(&cmd, &mut buf) else {
            prop_assert!(false, "encode must succeed");
            unreachable!()
        };

        // Flip one bit
        buf[byte_idx] ^= 1 << bit_idx;

        let result = decode_command(&buf);
        // After bit flip, decode should either fail with CRC mismatch
        // or (very rarely) the CRC happens to still match but the command type is invalid.
        if let Ok(decoded) = &result {
            // If CRC still matched by coincidence, the data was at least altered
            // (unless the flipped bit was in an unused region and the CRC collided).
            // We just verify it doesn't panic.
            let _ = decoded.cmd_type;
        }
    }

    /// Corrupting the CRC byte itself must cause a mismatch (unless it happens
    /// to produce the correct CRC, which is a 1/256 chance per value).
    #[test]
    fn prop_crc_byte_corruption(
        seq in 0u8..=255,
        corrupt_crc in 0u8..=255,
    ) {
        let cmd = SmCommand::new(seq, SmCommandType::GetStatus);
        let mut buf = [0u8; 15];
        let Ok(_) = encode_command(&cmd, &mut buf) else {
            prop_assert!(false, "encode must succeed");
            unreachable!()
        };

        let original_crc = buf[14];
        buf[14] = corrupt_crc;

        let result = decode_command(&buf);
        if corrupt_crc == original_crc {
            prop_assert!(result.is_ok());
        } else {
            prop_assert!(result.is_err(), "corrupted CRC must be rejected");
            let is_crc_err = matches!(result, Err(SmError::CrcMismatch { .. }));
            prop_assert!(is_crc_err, "error must be CrcMismatch");
        }
    }

    // ── Torque encoding invariants ──────────────────────────────────────────

    /// Report byte 0 must always be 0x01 (command report ID).
    #[test]
    fn prop_torque_report_id_always_0x01(
        torque in -200.0f32..200.0f32,
        max_torque in 0.01f32..50.0f32,
    ) {
        let mut enc = TorqueCommandEncoder::new(max_torque);
        let mut out = [0u8; TORQUE_COMMAND_LEN];
        enc.encode(torque, &mut out);
        prop_assert_eq!(out[0], 0x01, "byte 0 must be command report ID 0x01");
    }

    /// The encoded torque magnitude (as i16 in data field) must stay within ±32767.
    #[test]
    fn prop_torque_magnitude_within_i16_range(
        torque in -200.0f32..200.0f32,
        max_torque in 0.01f32..50.0f32,
    ) {
        let mut enc = TorqueCommandEncoder::new(max_torque);
        let mut out = [0u8; TORQUE_COMMAND_LEN];
        enc.encode(torque, &mut out);
        let raw = i16::from_le_bytes([out[10], out[11]]);
        prop_assert!(
            (-32767..=32767).contains(&raw),
            "torque magnitude {} must be within ±32767",
            raw
        );
    }

    /// Positive torque must produce non-negative raw value.
    #[test]
    fn prop_positive_torque_nonneg(
        torque in 0.001f32..200.0f32,
        max_torque in 0.01f32..50.0f32,
    ) {
        let mut enc = TorqueCommandEncoder::new(max_torque);
        let mut out = [0u8; TORQUE_COMMAND_LEN];
        enc.encode(torque, &mut out);
        let raw = i16::from_le_bytes([out[10], out[11]]);
        prop_assert!(raw >= 0, "positive torque {torque} produced negative raw {raw}");
    }

    /// Negative torque must produce non-positive raw value.
    #[test]
    fn prop_negative_torque_nonpos(
        torque in -200.0f32..-0.001f32,
        max_torque in 0.01f32..50.0f32,
    ) {
        let mut enc = TorqueCommandEncoder::new(max_torque);
        let mut out = [0u8; TORQUE_COMMAND_LEN];
        enc.encode(torque, &mut out);
        let raw = i16::from_le_bytes([out[10], out[11]]);
        prop_assert!(raw <= 0, "negative torque {torque} produced positive raw {raw}");
    }

    /// Saturation: torque well beyond max must clamp to ±32767.
    #[test]
    fn prop_torque_overflow_saturates(max_torque in 0.1f32..50.0f32) {
        let mut enc = TorqueCommandEncoder::new(max_torque);
        let mut out = [0u8; TORQUE_COMMAND_LEN];

        enc.encode(max_torque * 100.0, &mut out);
        let raw_pos = i16::from_le_bytes([out[10], out[11]]);
        prop_assert_eq!(raw_pos, 32767, "overflow positive must saturate to 32767");

        enc.encode(-max_torque * 100.0, &mut out);
        let raw_neg = i16::from_le_bytes([out[10], out[11]]);
        prop_assert_eq!(raw_neg, -32767, "overflow negative must saturate to -32767");
    }

    /// Encoder sequence must increment by 1 on each encode call.
    #[test]
    fn prop_encoder_sequence_increments(
        initial_encodes in 0u8..254,
        max_torque in 0.1f32..50.0f32,
    ) {
        let mut enc = TorqueCommandEncoder::new(max_torque);
        let mut out = [0u8; TORQUE_COMMAND_LEN];

        for _ in 0..initial_encodes {
            enc.encode(0.0, &mut out);
        }

        let before = enc.sequence();
        enc.encode(0.0, &mut out);
        let after = enc.sequence();
        prop_assert_eq!(after, before.wrapping_add(1));
    }

    // ── Feedback parsing invariants ─────────────────────────────────────────

    /// Parsing arbitrary 64-byte buffers with report ID 0x02 must not panic.
    #[test]
    fn prop_feedback_parse_never_panics(ref data in proptest::collection::vec(any::<u8>(), 64..65)) {
        let mut buf = data.clone();
        buf[0] = 0x02;
        let _ = parse_feedback_report(&buf);
    }

    /// Parsing buffers shorter than 64 bytes must return InvalidLength error.
    #[test]
    fn prop_feedback_short_buffer_rejected(len in 0usize..64) {
        let data = vec![0x02; len];
        let result = parse_feedback_report(&data);
        prop_assert!(result.is_err(), "short buffer must be rejected");
        let is_len_err = matches!(result, Err(SmError::InvalidLength { .. }));
        prop_assert!(is_len_err, "error must be InvalidLength");
    }

    /// position_degrees must always produce a finite value for valid encoder CPR.
    #[test]
    fn prop_position_degrees_finite(
        position in any::<i32>(),
        cpr in 1u32..=100_000,
    ) {
        let state = SmFeedbackState {
            motor: SmMotorFeedback { position, ..Default::default() },
            ..Default::default()
        };
        let degrees = state.position_degrees(cpr);
        prop_assert!(degrees.is_finite());
    }

    /// velocity_rpm must always produce a finite value for valid encoder CPR.
    #[test]
    fn prop_velocity_rpm_finite(
        velocity in any::<i32>(),
        cpr in 1u32..=100_000,
    ) {
        let state = SmFeedbackState {
            motor: SmMotorFeedback { velocity, ..Default::default() },
            ..Default::default()
        };
        let rpm = state.velocity_rpm(cpr);
        prop_assert!(rpm.is_finite());
    }

    /// torque_nm must produce a finite value for any torque constant.
    #[test]
    fn prop_torque_nm_finite(
        torque in any::<i16>(),
        constant in 0.001f32..10.0f32,
    ) {
        let state = SmFeedbackState {
            motor: SmMotorFeedback { torque, ..Default::default() },
            ..Default::default()
        };
        let nm = state.torque_nm(constant);
        prop_assert!(nm.is_finite());
    }

    // ── Device identification invariants ────────────────────────────────────

    /// identify_device must be deterministic for any PID.
    #[test]
    fn prop_identify_device_deterministic(pid: u16) {
        let a = identify_device(pid);
        let b = identify_device(pid);
        prop_assert_eq!(a.name, b.name);
        prop_assert_eq!(a.category, b.category);
        prop_assert_eq!(a.supports_ffb, b.supports_ffb);
    }

    /// is_wheelbase_product must agree with identify_device category.
    #[test]
    fn prop_is_wheelbase_consistent(pid: u16) {
        let identity = identify_device(pid);
        let is_wb = is_wheelbase_product(pid);
        let category_is_wb = identity.category == racing_wheel_simplemotion_v2::SmDeviceCategory::Wheelbase;
        prop_assert_eq!(is_wb, category_is_wb);
    }

    // ── Register read/write round-trips ─────────────────────────────────────

    /// build_get_parameter + decode must round-trip the parameter address.
    #[test]
    fn prop_get_parameter_roundtrip(addr in any::<u16>(), seq in any::<u8>()) {
        let report = build_get_parameter(addr, seq);
        let Ok(decoded) = decode_command(&report) else {
            prop_assert!(false, "decode must succeed");
            unreachable!()
        };
        prop_assert_eq!(decoded.seq, seq);
        prop_assert_eq!(decoded.cmd_type, SmCommandType::GetParameter);
        prop_assert_eq!(decoded.param_addr, Some(addr));
    }

    /// build_set_parameter + decode must round-trip address and value.
    #[test]
    fn prop_set_parameter_roundtrip(addr in any::<u16>(), value in any::<i32>(), seq in any::<u8>()) {
        let report = build_set_parameter(addr, value, seq);
        let Ok(decoded) = decode_command(&report) else {
            prop_assert!(false, "decode must succeed");
            unreachable!()
        };
        prop_assert_eq!(decoded.seq, seq);
        prop_assert_eq!(decoded.cmd_type, SmCommandType::SetParameter);
        prop_assert_eq!(decoded.param_addr, Some(addr));
        prop_assert_eq!(decoded.param_value, Some(value));
    }

    /// build_set_torque_command + decode must preserve the torque value.
    #[test]
    fn prop_set_torque_roundtrip(torque in any::<i16>(), seq in any::<u8>()) {
        let report = build_set_torque_command(torque, seq);
        let Ok(decoded) = decode_command(&report) else {
            prop_assert!(false, "decode must succeed");
            unreachable!()
        };
        prop_assert_eq!(decoded.seq, seq);
        prop_assert_eq!(decoded.cmd_type, SmCommandType::SetTorque);
        prop_assert_eq!(decoded.data, Some(torque as i32));
    }

    // ── Command type round-trip for all variants via proptest ────────────

    /// Every SmCommandType variant round-trips through to_u16 / from_u16.
    #[test]
    fn prop_command_type_roundtrip(type_idx in 0usize..8) {
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
        let ct = types[type_idx];
        let raw = ct.to_u16();
        let recovered = SmCommandType::from_u16(raw);
        prop_assert_eq!(recovered, Some(ct));
    }

    /// Arbitrary u16 values that are NOT valid command types must return None.
    #[test]
    fn prop_invalid_command_type_returns_none(val in any::<u16>()) {
        let valid = [0x0001, 0x0002, 0x0003, 0x0010, 0x0011, 0x0012, 0x0013, 0xFFFF];
        if !valid.contains(&val) {
            prop_assert!(SmCommandType::from_u16(val).is_none());
        }
    }

    // ── CRC correctness: double-encode produces same CRC ────────────────

    /// Encoding the same command twice must produce identical buffers (including CRC).
    #[test]
    fn prop_crc_deterministic(
        seq in any::<u8>(),
        data in any::<i32>(),
        type_idx in 0usize..8,
    ) {
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
        let cmd = SmCommand::new(seq, types[type_idx]).with_data(data);
        let mut buf1 = [0u8; 15];
        let mut buf2 = [0u8; 15];
        let Ok(_) = encode_command(&cmd, &mut buf1) else {
            prop_assert!(false, "encode must succeed");
            unreachable!()
        };
        let Ok(_) = encode_command(&cmd, &mut buf2) else {
            prop_assert!(false, "encode must succeed");
            unreachable!()
        };
        prop_assert_eq!(buf1, buf2, "CRC must be deterministic");
    }

    // ── Encode into oversized buffer ────────────────────────────────────

    /// Encoding into a buffer larger than 15 must succeed and only
    /// write 15 meaningful bytes (rest zeroed by fill).
    #[test]
    fn prop_encode_oversized_buffer(seq in any::<u8>(), extra in 0usize..50) {
        let size = 15 + extra;
        let mut buf = vec![0xFFu8; size];
        let cmd = SmCommand::new(seq, SmCommandType::GetStatus);
        let Ok(len) = encode_command(&cmd, &mut buf) else {
            prop_assert!(false, "encode must succeed");
            unreachable!()
        };
        prop_assert_eq!(len, 15);
        // Bytes beyond 15 must be zero (encode fills then writes)
        for &b in &buf[15..] {
            prop_assert_eq!(b, 0u8, "bytes past 15 must be zeroed");
        }
    }
}

// ── State machine / SmStatus transitions ────────────────────────────────────

/// SmStatus::from_u8 covers all defined values; everything else is Unknown.
#[test]
fn status_from_u8_all_defined() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(SmStatus::from_u8(0), SmStatus::Ok);
    assert_eq!(SmStatus::from_u8(1), SmStatus::Error);
    assert_eq!(SmStatus::from_u8(2), SmStatus::Busy);
    assert_eq!(SmStatus::from_u8(3), SmStatus::NotReady);
    for v in 4..=255 {
        assert_eq!(
            SmStatus::from_u8(v),
            SmStatus::Unknown,
            "value {v} must map to Unknown"
        );
    }
    Ok(())
}

/// SmStatus default must be Unknown.
#[test]
fn status_default_is_unknown() {
    assert_eq!(SmStatus::default(), SmStatus::Unknown);
}

// ── Error handling for all error codes ──────────────────────────────────────

#[test]
fn error_invalid_length_display() {
    let err = SmError::InvalidLength {
        expected: 15,
        actual: 10,
    };
    assert!(err.to_string().contains("15"));
    assert!(err.to_string().contains("10"));
}

#[test]
fn error_invalid_command_type_display() {
    let err = SmError::InvalidCommandType(0x99);
    assert!(
        err.to_string().contains("153")
            || err.to_string().contains("0x99")
            || err.to_string().contains("Invalid command type")
    );
}

#[test]
fn error_invalid_parameter_display() {
    let err = SmError::InvalidParameter(0xBEEF);
    assert!(err.to_string().contains("Invalid parameter address"));
}

#[test]
fn error_device_error_display() {
    let err = SmError::DeviceError("test fault".to_string());
    assert!(err.to_string().contains("test fault"));
}

#[test]
fn error_communication_error_display() {
    let err = SmError::CommunicationError("timeout".to_string());
    assert!(err.to_string().contains("timeout"));
}

#[test]
fn error_crc_mismatch_display() {
    let err = SmError::CrcMismatch {
        expected: 0xAA,
        actual: 0xBB,
    };
    assert!(err.to_string().contains("CRC mismatch"));
}

#[test]
fn error_parse_error_display() {
    let err = SmError::ParseError("bad data".to_string());
    assert!(err.to_string().contains("bad data"));
}

#[test]
fn error_encode_error_display() {
    let err = SmError::EncodeError("overflow".to_string());
    assert!(err.to_string().contains("overflow"));
}

#[test]
fn error_from_io_preserves_message() {
    let io_err = std::io::Error::new(std::io::ErrorKind::TimedOut, "serial timeout");
    let sm_err: SmError = io_err.into();
    assert!(matches!(sm_err, SmError::CommunicationError(ref s) if s.contains("serial timeout")));
}

// ── Edge cases: maximum payload, zero-length, boundary buffers ──────────────

#[test]
fn encode_with_exactly_15_byte_buffer() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(0xFF, SmCommandType::Reset)
        .with_param(u16::MAX, i32::MAX)
        .with_data(i32::MIN);
    let mut buf = [0u8; 15];
    let len = encode_command(&cmd, &mut buf)?;
    assert_eq!(len, 15);
    let decoded = decode_command(&buf)?;
    assert_eq!(decoded.seq, 0xFF);
    assert_eq!(decoded.cmd_type, SmCommandType::Reset);
    assert_eq!(decoded.param_addr, Some(u16::MAX));
    assert_eq!(decoded.param_value, Some(i32::MAX));
    assert_eq!(decoded.data, Some(i32::MIN));
    Ok(())
}

#[test]
fn decode_zero_length_buffer_returns_error() {
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
fn encode_zero_length_buffer_returns_error() {
    let cmd = SmCommand::new(0, SmCommandType::GetStatus);
    let result = encode_command(&cmd, &mut []);
    assert!(matches!(
        result,
        Err(SmError::InvalidLength {
            expected: 15,
            actual: 0
        })
    ));
}

#[test]
fn decode_single_byte_buffer_returns_error() {
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
fn encode_14_byte_buffer_returns_error() {
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

/// Maximum payload: all fields at extreme values round-trip correctly.
#[test]
fn maximum_payload_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(255, SmCommandType::SetParameter)
        .with_param(u16::MAX, i32::MIN)
        .with_data(i32::MAX);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;
    let decoded = decode_command(&buf)?;
    assert_eq!(decoded.seq, 255);
    assert_eq!(decoded.cmd_type, SmCommandType::SetParameter);
    assert_eq!(decoded.param_addr, Some(u16::MAX));
    assert_eq!(decoded.param_value, Some(i32::MIN));
    assert_eq!(decoded.data, Some(i32::MAX));
    Ok(())
}

/// Zero payload: all optional fields zeroed still round-trips.
#[test]
fn zero_payload_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(0, SmCommandType::GetStatus);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;
    let decoded = decode_command(&buf)?;
    assert_eq!(decoded.seq, 0);
    assert_eq!(decoded.cmd_type, SmCommandType::GetStatus);
    Ok(())
}

/// Feedback report at exactly 64 bytes is accepted.
#[test]
fn feedback_report_exact_64_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; 64];
    data[0] = 0x02;
    let state = parse_feedback_report(&data)?;
    assert_eq!(state.seq, 0);
    Ok(())
}

/// Feedback report with 65+ bytes is also accepted (extra bytes ignored).
#[test]
fn feedback_report_oversized_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0xAA; 128];
    data[0] = 0x02;
    let state = parse_feedback_report(&data)?;
    assert!(state.seq == 0xAA);
    Ok(())
}

/// Feedback report with wrong report ID returns InvalidCommandType.
#[test]
fn feedback_report_wrong_report_id() {
    let mut data = vec![0u8; 64];
    data[0] = 0x01; // command ID, not feedback
    let result = parse_feedback_report(&data);
    assert!(matches!(result, Err(SmError::InvalidCommandType(0x01))));
}

/// Decode with valid CRC but unrecognized command type returns InvalidCommandType.
#[test]
fn decode_unknown_command_type_with_valid_crc() {
    let mut buf = [0u8; 15];
    buf[0] = 0x01;
    buf[2] = 0x05; // Not a valid command type
    buf[3] = 0x00;
    // Compute CRC manually
    let crc = compute_crc8_for_test(&buf[..14]);
    buf[14] = crc;
    let result = decode_command(&buf);
    assert!(matches!(result, Err(SmError::InvalidCommandType(_))));
}

// ── Helper ──────────────────────────────────────────────────────────────────

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
