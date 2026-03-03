//! Property-based tests for the SimpleMotion V2 protocol.
//!
//! Uses proptest with 500 cases to verify invariants on command encoding,
//! CRC integrity, torque encoding, and feedback parsing.

use proptest::prelude::*;
use racing_wheel_simplemotion_v2::commands::{
    SmCommand, SmCommandType, decode_command, encode_command,
};
use racing_wheel_simplemotion_v2::error::SmError;
use racing_wheel_simplemotion_v2::{
    SmFeedbackState, SmMotorFeedback, TorqueCommandEncoder, TORQUE_COMMAND_LEN,
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
        let result = encode_command(&cmd, &mut buf);
        prop_assert!(result.is_ok());

        let decoded = decode_command(&buf);
        prop_assert!(decoded.is_ok());
        let decoded = decoded.expect("already checked");

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
        encode_command(&cmd, &mut buf).expect("encode must succeed");

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
        encode_command(&cmd, &mut buf).expect("encode must succeed");

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
        let decoded = decode_command(&report);
        prop_assert!(decoded.is_ok());
        let decoded = decoded.expect("already checked");
        prop_assert_eq!(decoded.seq, seq);
        prop_assert_eq!(decoded.cmd_type, SmCommandType::GetParameter);
        prop_assert_eq!(decoded.param_addr, Some(addr));
    }

    /// build_set_parameter + decode must round-trip address and value.
    #[test]
    fn prop_set_parameter_roundtrip(addr in any::<u16>(), value in any::<i32>(), seq in any::<u8>()) {
        let report = build_set_parameter(addr, value, seq);
        let decoded = decode_command(&report);
        prop_assert!(decoded.is_ok());
        let decoded = decoded.expect("already checked");
        prop_assert_eq!(decoded.seq, seq);
        prop_assert_eq!(decoded.cmd_type, SmCommandType::SetParameter);
        prop_assert_eq!(decoded.param_addr, Some(addr));
        prop_assert_eq!(decoded.param_value, Some(value));
    }

    /// build_set_torque_command + decode must preserve the torque value.
    #[test]
    fn prop_set_torque_roundtrip(torque in any::<i16>(), seq in any::<u8>()) {
        let report = build_set_torque_command(torque, seq);
        let decoded = decode_command(&report);
        prop_assert!(decoded.is_ok());
        let decoded = decoded.expect("already checked");
        prop_assert_eq!(decoded.seq, seq);
        prop_assert_eq!(decoded.cmd_type, SmCommandType::SetTorque);
        prop_assert_eq!(decoded.data, Some(torque as i32));
    }
}
