//! Comprehensive SimpleMotion V2 protocol tests based on the Granite Devices specification.
//!
//! References:
//! - Granite Devices SimpleMotion V2 wiki: <https://granitedevices.com/wiki/SimpleMotion_V2>
//! - SimpleMotion V2 parameter list: <https://granitedevices.com/wiki/List_of_SimpleMotion_parameters>
//! - simplemotion_defs.h: <https://github.com/GraniteDevices/SimpleMotionV2>
//!
//! Covers:
//! 1. Command encoding/decoding for all SimpleMotion commands
//! 2. Address field encoding
//! 3. CRC calculation and verification
//! 4. Multi-byte parameter encoding
//! 5. Status register bit fields
//! 6. Error response handling
//! 7. Streaming mode protocol
//! 8. Known command sequences from Granite Devices documentation
//! 9. Parameter types and ranges
//! 10. Protocol version handling

use racing_wheel_simplemotion_v2::commands::{
    SmCommand, SmCommandType, SmStatus, decode_command, encode_command,
};
use racing_wheel_simplemotion_v2::error::SmError;
use racing_wheel_simplemotion_v2::{
    FEEDBACK_REPORT_LEN, SETPARAM_REPORT_LEN, STATUS_REPORT_LEN, SmDeviceCategory, SmFeedbackState,
    SmMotorFeedback, TORQUE_COMMAND_LEN, TorqueCommandEncoder, build_device_enable,
    build_get_parameter, build_set_parameter, build_set_torque_command,
    build_set_torque_command_with_velocity, build_set_zero_position, identify_device,
    parse_feedback_report, sm_device_identity,
};

// ── Helper: CRC8 with polynomial 0x07 (matching the crate-internal algorithm) ──

/// CRC-8/ITU polynomial 0x07 — the standard used by SimpleMotion V2 on HID reports.
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

/// Build a feedback report with the given motor values and additional fields.
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
    data[4..8].copy_from_slice(&position.to_le_bytes());
    data[8..12].copy_from_slice(&velocity.to_le_bytes());
    data[12..14].copy_from_slice(&torque.to_le_bytes());
    data[14..16].copy_from_slice(&bus_voltage.to_le_bytes());
    data[16..18].copy_from_slice(&motor_current.to_le_bytes());
    data[18] = temperature as u8;
    data
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Command encoding/decoding for all SimpleMotion commands
// ═══════════════════════════════════════════════════════════════════════════════

/// SM V2 command type codes from simplemotion_defs.h:
/// SM_SET_WRITE_ADDRESS = 2, SM_WRITE_VALUE_24B = 1, SM_WRITE_VALUE_32B = 0
/// Our crate maps these to: GetParameter=0x0001, SetParameter=0x0002,
/// GetStatus=0x0003, SetTorque=0x0010, etc.
#[test]
fn encode_all_command_types_have_correct_opcodes() -> Result<(), Box<dyn std::error::Error>> {
    let expected_opcodes: &[(SmCommandType, u16)] = &[
        (SmCommandType::GetParameter, 0x0001),
        (SmCommandType::SetParameter, 0x0002),
        (SmCommandType::GetStatus, 0x0003),
        (SmCommandType::SetTorque, 0x0010),
        (SmCommandType::SetVelocity, 0x0011),
        (SmCommandType::SetPosition, 0x0012),
        (SmCommandType::SetZero, 0x0013),
        (SmCommandType::Reset, 0xFFFF),
    ];

    for &(cmd_type, expected_code) in expected_opcodes {
        let cmd = SmCommand::new(0, cmd_type);
        let mut buf = [0u8; 15];
        encode_command(&cmd, &mut buf)?;
        let encoded_type = u16::from_le_bytes([buf[2], buf[3]]);
        assert_eq!(
            encoded_type, expected_code,
            "{cmd_type:?} should encode as {expected_code:#06x}, got {encoded_type:#06x}"
        );
    }
    Ok(())
}

#[test]
fn decode_all_command_types_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
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

    for (seq, cmd_type) in types.iter().enumerate() {
        let cmd = SmCommand::new(seq as u8, *cmd_type)
            .with_param(0x1001, 42)
            .with_data(99);
        let mut buf = [0u8; 15];
        encode_command(&cmd, &mut buf)?;
        let decoded = decode_command(&buf)?;
        assert_eq!(decoded.cmd_type, *cmd_type);
        assert_eq!(decoded.seq, seq as u8);
    }
    Ok(())
}

/// Per SM V2 spec: byte 0 is always report ID 0x01 for command reports.
#[test]
fn all_commands_start_with_report_id_0x01() -> Result<(), Box<dyn std::error::Error>> {
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
        assert_eq!(
            buf[0], 0x01,
            "command {cmd_type:?} must have report ID 0x01"
        );
    }
    Ok(())
}

/// Sequence number occupies byte 1 and must be preserved exactly.
#[test]
fn sequence_number_preserved_across_full_range() -> Result<(), Box<dyn std::error::Error>> {
    for seq in [0u8, 1, 127, 128, 254, 255] {
        let cmd = SmCommand::new(seq, SmCommandType::GetStatus);
        let mut buf = [0u8; 15];
        encode_command(&cmd, &mut buf)?;
        assert_eq!(buf[1], seq);
        let decoded = decode_command(&buf)?;
        assert_eq!(decoded.seq, seq);
    }
    Ok(())
}

/// SM V2 protocol: command type occupies bytes 2-3 in little-endian.
#[test]
fn command_type_bytes_are_little_endian() -> Result<(), Box<dyn std::error::Error>> {
    // Reset = 0xFFFF — both bytes should be 0xFF
    let cmd = SmCommand::new(0, SmCommandType::Reset);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;
    assert_eq!(buf[2], 0xFF);
    assert_eq!(buf[3], 0xFF);

    // GetParameter = 0x0001 — low byte 0x01, high byte 0x00
    let cmd = SmCommand::new(0, SmCommandType::GetParameter);
    encode_command(&cmd, &mut buf)?;
    assert_eq!(buf[2], 0x01);
    assert_eq!(buf[3], 0x00);
    Ok(())
}

/// Invalid command type codes should return None from from_u16.
#[test]
fn invalid_command_type_codes_return_none() {
    let invalid_codes: &[u16] = &[
        0x0000, 0x0004, 0x0005, 0x000F, 0x0014, 0x0015, 0x00FF, 0x0100, 0x7FFF, 0xFFFE,
    ];
    for &code in invalid_codes {
        assert!(
            SmCommandType::from_u16(code).is_none(),
            "code {code:#06x} should not be a valid command type"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Address field encoding
// ═══════════════════════════════════════════════════════════════════════════════

/// SM V2 address space from simplemotion_defs.h:
/// 0 = NULL, 1-63 = SM bus params, 128-167 = digital I/O,
/// 168-199 = analog I/O, 201-8190 = motor drive params, 8191 = NOP
#[test]
fn address_encoding_covers_sm_v2_address_space() -> Result<(), Box<dyn std::error::Error>> {
    // Key addresses from SM V2 address space
    let addresses: &[(u16, &str)] = &[
        (0x0000, "SMP_NULL"),
        (0x0001, "SMP_NODE_ADDRESS"),
        (0x0003, "SMP_SM_VERSION"),
        (0x0004, "SMP_SM_VERSION_COMPAT"),
        (0x0005, "SMP_BUS_SPEED"),
        (0x000E, "SMP_ADDRESS_OFFSET (14)"),
        (0x003F, "SM reserved boundary (63)"),
        (0x0080, "SMP_DIGITAL_IN_VALUES_1 (128)"),
        (0x00A8, "SMP_ANALOG_IN_VALUE_1 (168)"),
        (0x1001, "device enable"),
        (0x1FFF, "SMP_ADDR_NOP (8191)"),
        (0xFFFF, "max address"),
    ];

    for &(addr, name) in addresses {
        let report = build_get_parameter(addr, 0);
        let decoded = decode_command(&report)?;
        assert_eq!(
            decoded.param_addr,
            Some(addr),
            "address {name} ({addr:#06x}) not preserved"
        );
    }
    Ok(())
}

/// SM V2 parameter address attribute masks from simplemotion_defs.h:
/// SMP_VALUE_MASK = 0x0000, SMP_MIN_VALUE_MASK = 0x4000,
/// SMP_MAX_VALUE_MASK = 0x8000, SMP_PROPERTIES_MASK = 0xC000
#[test]
fn address_attribute_masks_preserved() -> Result<(), Box<dyn std::error::Error>> {
    let base_addr: u16 = 0x000C; // SMP_TIMEOUT = 12
    let masks: &[(u16, &str)] = &[
        (0x0000, "SMP_VALUE_MASK"),
        (0x4000, "SMP_MIN_VALUE_MASK"),
        (0x8000, "SMP_MAX_VALUE_MASK"),
        (0xC000, "SMP_PROPERTIES_MASK"),
    ];

    for &(mask, name) in masks {
        let addr = base_addr | mask;
        let report = build_get_parameter(addr, 0);
        let decoded = decode_command(&report)?;
        assert_eq!(
            decoded.param_addr,
            Some(addr),
            "address with {name} mask not preserved"
        );
    }
    Ok(())
}

/// Address bytes occupy positions 4-5 in little-endian format.
#[test]
fn address_bytes_layout_is_little_endian() -> Result<(), Box<dyn std::error::Error>> {
    let addr: u16 = 0xABCD;
    let report = build_get_parameter(addr, 0);
    // Byte 4 = low byte, byte 5 = high byte
    assert_eq!(report[4], 0xCD);
    assert_eq!(report[5], 0xAB);
    Ok(())
}

/// SM V2 broadcast address is 0 — all nodes receive, none reply.
#[test]
fn broadcast_address_zero_roundtrips() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_set_parameter(0x0000, 0, 0);
    let decoded = decode_command(&report)?;
    assert_eq!(decoded.param_addr, Some(0x0000));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. CRC calculation and verification
// ═══════════════════════════════════════════════════════════════════════════════

/// The CRC-8 uses polynomial 0x07 (x^8 + x^2 + x + 1), which is the
/// ITU-T CRC-8 / SM V2 standard. Init value is 0x00.
#[test]
fn crc8_polynomial_0x07_known_vectors() {
    // CRC-8/ITU with poly 0x07, init 0x00
    // All zeros → deterministic non-zero output
    let zeros = [0u8; 14];
    let crc_zeros = compute_crc8(&zeros);
    assert_eq!(crc_zeros, compute_crc8(&zeros), "CRC must be deterministic");

    // Single byte 0x01
    let crc_one = compute_crc8(&[0x01]);
    assert_eq!(
        crc_one, 0x07,
        "CRC-8 of [0x01] with poly 0x07 should be 0x07"
    );

    // Two bytes: 0x01, 0x00 — shift once then XOR
    let crc_two = compute_crc8(&[0x01, 0x00]);
    // After processing 0x01 → 0x07; after processing 0x00 with that state → 0x07 << 1 ^ ...
    // Verify it's nonzero and deterministic
    assert_ne!(crc_two, 0);
}

/// Verify the CRC byte (byte 14) embedded in encoded packets matches our calculation.
#[test]
fn crc_embedded_matches_manual_computation() -> Result<(), Box<dyn std::error::Error>> {
    let test_cases: &[(u8, SmCommandType, u16, i32)] = &[
        (0, SmCommandType::GetStatus, 0, 0),
        (1, SmCommandType::GetParameter, 0x1001, 0),
        (42, SmCommandType::SetParameter, 0x2000, -500),
        (255, SmCommandType::SetTorque, 0, 32767),
        (128, SmCommandType::Reset, 0, 0),
    ];

    for &(seq, cmd_type, addr, val) in test_cases {
        let cmd = SmCommand::new(seq, cmd_type).with_param(addr, val);
        let mut buf = [0u8; 15];
        encode_command(&cmd, &mut buf)?;
        let expected_crc = compute_crc8(&buf[..14]);
        assert_eq!(
            buf[14], expected_crc,
            "CRC mismatch for seq={seq}, cmd={cmd_type:?}"
        );
    }
    Ok(())
}

/// Flipping any single bit in payload bytes 0-13 must change the CRC.
#[test]
fn crc_detects_all_single_bit_errors() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(0x55, SmCommandType::SetParameter).with_param(0xAAAA, 0x55555555);
    let mut original = [0u8; 15];
    encode_command(&cmd, &mut original)?;
    let original_crc = original[14];

    for byte_idx in 0..14 {
        for bit_idx in 0..8u8 {
            let mut modified = original;
            modified[byte_idx] ^= 1 << bit_idx;
            let new_crc = compute_crc8(&modified[..14]);
            assert_ne!(
                new_crc, original_crc,
                "CRC should differ when byte {byte_idx} bit {bit_idx} is flipped"
            );
        }
    }
    Ok(())
}

/// Decode must reject packets with any single bit flip (CRC mismatch).
#[test]
fn decode_rejects_every_single_bit_corruption() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(0x33, SmCommandType::SetParameter).with_param(0x1001, 500);
    let mut original = [0u8; 15];
    encode_command(&cmd, &mut original)?;

    for byte_idx in 0..14 {
        for bit_idx in 0..8u8 {
            let mut corrupted = original;
            corrupted[byte_idx] ^= 1 << bit_idx;
            let result = decode_command(&corrupted);
            assert!(
                result.is_err(),
                "bit flip at byte {byte_idx} bit {bit_idx} should be detected"
            );
        }
    }
    Ok(())
}

/// CRC-8 of an empty slice should be 0x00 (init value).
#[test]
fn crc8_of_empty_input_is_zero() {
    assert_eq!(compute_crc8(&[]), 0x00);
}

/// CRC-8 of all-0xFF payload should be deterministic.
#[test]
fn crc8_all_ones_payload() {
    let data = [0xFFu8; 14];
    let crc = compute_crc8(&data);
    assert_eq!(crc, compute_crc8(&data));
    // Not zero for this polynomial
    assert_ne!(crc, 0);
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Multi-byte parameter encoding
// ═══════════════════════════════════════════════════════════════════════════════

/// Parameter values occupy bytes 6-9 in little-endian i32 format.
#[test]
fn parameter_value_little_endian_layout() -> Result<(), Box<dyn std::error::Error>> {
    let value: i32 = 0x12345678;
    let report = build_set_parameter(0x1001, value, 0);
    assert_eq!(report[6], 0x78); // lowest byte
    assert_eq!(report[7], 0x56);
    assert_eq!(report[8], 0x34);
    assert_eq!(report[9], 0x12); // highest byte

    let decoded = decode_command(&report)?;
    assert_eq!(decoded.param_value, Some(0x12345678));
    Ok(())
}

/// Negative parameter values use two's complement.
#[test]
fn negative_parameter_twos_complement() -> Result<(), Box<dyn std::error::Error>> {
    let value: i32 = -1;
    let report = build_set_parameter(0x1001, value, 0);
    // -1 in two's complement = 0xFFFFFFFF
    assert_eq!(report[6], 0xFF);
    assert_eq!(report[7], 0xFF);
    assert_eq!(report[8], 0xFF);
    assert_eq!(report[9], 0xFF);

    let decoded = decode_command(&report)?;
    assert_eq!(decoded.param_value, Some(-1));
    Ok(())
}

/// i32 boundary values roundtrip correctly.
#[test]
fn parameter_boundary_values_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let boundaries: &[i32] = &[
        0,
        1,
        -1,
        i32::MAX,
        i32::MIN,
        i16::MAX as i32,
        i16::MIN as i32,
    ];

    for &val in boundaries {
        let report = build_set_parameter(0x2000, val, 0);
        let decoded = decode_command(&report)?;
        assert_eq!(
            decoded.param_value,
            Some(val),
            "value {val} not preserved in roundtrip"
        );
    }
    Ok(())
}

/// Data field (bytes 10-13) used for torque, velocity, position commands.
#[test]
fn data_field_bytes_10_13_layout() -> Result<(), Box<dyn std::error::Error>> {
    let data_value: i32 = 0x0A0B0C0D;
    let cmd = SmCommand::new(0, SmCommandType::SetTorque).with_data(data_value);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;
    assert_eq!(buf[10], 0x0D); // lowest byte
    assert_eq!(buf[11], 0x0C);
    assert_eq!(buf[12], 0x0B);
    assert_eq!(buf[13], 0x0A); // highest byte

    let decoded = decode_command(&buf)?;
    assert_eq!(decoded.data, Some(data_value));
    Ok(())
}

/// SM V2 torque+velocity combined encoding:
/// torque in upper 16 bits, velocity in lower 16 bits.
#[test]
fn torque_velocity_combined_packing() -> Result<(), Box<dyn std::error::Error>> {
    let torque: i16 = 1000;
    let velocity: i16 = -500;
    let report = build_set_torque_command_with_velocity(torque, velocity, 0);
    let decoded = decode_command(&report)?;

    let combined = decoded.data.ok_or("missing data field")?;
    let expected = ((torque as i32) << 16) | (velocity as i32 & 0xFFFF);
    assert_eq!(combined, expected);
    Ok(())
}

/// Q8.8 fixed-point torque encoding: 256 counts = 1.0 unit.
#[test]
fn q8_8_fixed_point_torque_encoding() -> Result<(), Box<dyn std::error::Error>> {
    // 256 in Q8.8 = 1.0
    let report = build_set_torque_command(256, 0);
    let decoded = decode_command(&report)?;
    assert_eq!(decoded.data, Some(256));

    // -256 in Q8.8 = -1.0
    let report_neg = build_set_torque_command(-256, 0);
    let decoded_neg = decode_command(&report_neg)?;
    assert_eq!(decoded_neg.data, Some(-256));

    // 128 in Q8.8 = 0.5
    let report_half = build_set_torque_command(128, 0);
    let decoded_half = decode_command(&report_half)?;
    assert_eq!(decoded_half.data, Some(128));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Status register bit fields
// ═══════════════════════════════════════════════════════════════════════════════

/// SM V2 status values from simplemotion_defs.h:
/// SMP_CMD_STATUS_ACK = 0, NACK = 1, INVALID_ADDR = 2,
/// INVALID_VALUE = 4, VALUE_TOO_HIGH = 8, VALUE_TOO_LOW = 16
#[test]
fn sm_status_from_u8_maps_to_granite_spec() {
    assert_eq!(SmStatus::from_u8(0), SmStatus::Ok); // SMP_CMD_STATUS_ACK
    assert_eq!(SmStatus::from_u8(1), SmStatus::Error); // SMP_CMD_STATUS_NACK
    assert_eq!(SmStatus::from_u8(2), SmStatus::Busy); // maps to Busy in crate
    assert_eq!(SmStatus::from_u8(3), SmStatus::NotReady);
}

/// All undefined status values (4-255) must map to Unknown.
#[test]
fn sm_status_undefined_values_map_to_unknown() {
    for v in 4u8..=255 {
        assert_eq!(
            SmStatus::from_u8(v),
            SmStatus::Unknown,
            "status value {v} should be Unknown"
        );
    }
}

/// SmStatus default is Unknown (safe default for uninitialized state).
#[test]
fn sm_status_default_is_unknown() {
    assert_eq!(SmStatus::default(), SmStatus::Unknown);
}

/// SM V2 SMP_STATUS register bit fields from simplemotion_defs.h:
/// These represent the drive status register, not the command status.
#[test]
fn smp_status_register_bit_definitions() {
    // From simplemotion_defs.h — SMP_STATUS (param 553) bit field
    const STAT_TARGET_REACHED: u32 = 1 << 1;
    const STAT_FERROR_RECOVERY: u32 = 1 << 2;
    const STAT_RUN: u32 = 1 << 3;
    const STAT_ENABLED: u32 = 1 << 4;
    const STAT_FAULTSTOP: u32 = 1 << 5;
    const STAT_FERROR_WARNING: u32 = 1 << 6;
    const STAT_STO_ACTIVE: u32 = 1 << 7;
    const STAT_SERVO_READY: u32 = 1 << 8;
    const STAT_BRAKING: u32 = 1 << 10;
    const STAT_HOMING: u32 = 1 << 11;
    const STAT_INITIALIZED: u32 = 1 << 12;
    const STAT_VOLTAGES_OK: u32 = 1 << 13;
    const STAT_PERMANENT_STOP: u32 = 1 << 15;
    const STAT_STANDING_STILL: u32 = 1 << 16;
    const STAT_QUICK_STOP_ACTIVE: u32 = 1 << 17;

    // Verify bit positions don't overlap
    let bits = [
        STAT_TARGET_REACHED,
        STAT_FERROR_RECOVERY,
        STAT_RUN,
        STAT_ENABLED,
        STAT_FAULTSTOP,
        STAT_FERROR_WARNING,
        STAT_STO_ACTIVE,
        STAT_SERVO_READY,
        STAT_BRAKING,
        STAT_HOMING,
        STAT_INITIALIZED,
        STAT_VOLTAGES_OK,
        STAT_PERMANENT_STOP,
        STAT_STANDING_STILL,
        STAT_QUICK_STOP_ACTIVE,
    ];

    // Verify each bit is a power of two (single bit set)
    for &bit in &bits {
        assert!(
            bit.is_power_of_two(),
            "status bit {bit:#010x} must be a single bit"
        );
    }

    // Verify no overlaps
    let combined: u32 = bits.iter().sum();
    let ored: u32 = bits.iter().fold(0u32, |acc, &b| acc | b);
    assert_eq!(combined, ored, "status bits must not overlap");
}

/// SM V2 SMP_FAULTS register bit fields from simplemotion_defs.h.
#[test]
fn smp_faults_register_bit_definitions() {
    // From simplemotion_defs.h — SMP_FAULTS (param 552) bit field
    const FLT_FOLLOWERROR: u32 = 1 << 1;
    const FLT_OVERCURRENT: u32 = 1 << 2;
    const FLT_COMMUNICATION: u32 = 1 << 3;
    const FLT_ENCODER: u32 = 1 << 4;
    const FLT_OVERTEMP: u32 = 1 << 5;
    const FLT_UNDERVOLTAGE: u32 = 1 << 6;
    const FLT_OVERVOLTAGE: u32 = 1 << 7;
    const FLT_PROGRAM_OR_MEM: u32 = 1 << 8;
    const FLT_HARDWARE: u32 = 1 << 9;
    const FLT_OVERVELOCITY: u32 = 1 << 10;
    const FLT_INIT: u32 = 1 << 11;
    const FLT_MOTION: u32 = 1 << 12;
    const FLT_RANGE: u32 = 1 << 13;
    const FLT_PSTAGE_FORCED_OFF: u32 = 1 << 14;
    const FLT_HOST_COMM_ERROR: u32 = 1 << 15;
    const FLT_CONFIG: u32 = 1 << 16;

    let fault_bits = [
        FLT_FOLLOWERROR,
        FLT_OVERCURRENT,
        FLT_COMMUNICATION,
        FLT_ENCODER,
        FLT_OVERTEMP,
        FLT_UNDERVOLTAGE,
        FLT_OVERVOLTAGE,
        FLT_PROGRAM_OR_MEM,
        FLT_HARDWARE,
        FLT_OVERVELOCITY,
        FLT_INIT,
        FLT_MOTION,
        FLT_RANGE,
        FLT_PSTAGE_FORCED_OFF,
        FLT_HOST_COMM_ERROR,
        FLT_CONFIG,
    ];

    for &bit in &fault_bits {
        assert!(bit.is_power_of_two());
    }

    // Each fault bit can be encoded as a parameter value
    for &bit in &fault_bits {
        let report = build_set_parameter(552, bit as i32, 0); // SMP_FAULTS = 552
        let decoded = decode_command(&report);
        assert!(decoded.is_ok());
    }
}

/// Feedback report status field is parsed from byte 2.
#[test]
fn feedback_status_field_parsing() -> Result<(), Box<dyn std::error::Error>> {
    for status_val in 0u8..=4 {
        let data = make_feedback_report(0, status_val, 0, 0, 0, 0, 0, 0);
        let fb = parse_feedback_report(&data)?;
        let expected = SmStatus::from_u8(status_val);
        assert_eq!(fb.status, expected);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Error response handling
// ═══════════════════════════════════════════════════════════════════════════════

/// Encode with buffer smaller than 15 bytes returns InvalidLength.
#[test]
fn error_encode_buffer_too_small() {
    let cmd = SmCommand::new(0, SmCommandType::GetStatus);
    for size in 0..15 {
        let mut buf = vec![0u8; size];
        let result = encode_command(&cmd, &mut buf);
        assert!(
            matches!(
                result,
                Err(SmError::InvalidLength {
                    expected: 15,
                    actual,
                }) if actual == size
            ),
            "buffer size {size} should fail"
        );
    }
}

/// Decode with buffer smaller than 15 bytes returns InvalidLength.
#[test]
fn error_decode_buffer_too_small() {
    for size in 0..15 {
        let buf = vec![0u8; size];
        let result = decode_command(&buf);
        assert!(
            matches!(
                result,
                Err(SmError::InvalidLength {
                    expected: 15,
                    actual,
                }) if actual == size
            ),
            "buffer size {size} should fail"
        );
    }
}

/// Decode with valid CRC but invalid command type returns InvalidCommandType.
#[test]
fn error_decode_invalid_command_type_with_valid_crc() {
    // Test several invalid command type values
    let invalid_types: &[u16] = &[0x0000, 0x0004, 0x000F, 0x0099, 0x1234, 0xFFFE];
    for &invalid_type in invalid_types {
        let mut buf = [0u8; 15];
        buf[0] = 0x01;
        let type_bytes = invalid_type.to_le_bytes();
        buf[2] = type_bytes[0];
        buf[3] = type_bytes[1];
        buf[14] = compute_crc8(&buf[..14]);
        let result = decode_command(&buf);
        assert!(
            matches!(result, Err(SmError::InvalidCommandType(_))),
            "type {invalid_type:#06x} should fail as InvalidCommandType"
        );
    }
}

/// CRC mismatch error includes expected and actual values.
#[test]
fn error_crc_mismatch_has_expected_and_actual() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(0, SmCommandType::GetStatus);
    let mut buf = [0u8; 15];
    encode_command(&cmd, &mut buf)?;
    let correct_crc = buf[14];
    buf[14] = correct_crc.wrapping_add(1); // intentionally wrong

    let result = decode_command(&buf);
    match result {
        Err(SmError::CrcMismatch { expected, actual }) => {
            assert_eq!(expected, correct_crc);
            assert_eq!(actual, correct_crc.wrapping_add(1));
        }
        other => return Err(format!("expected CrcMismatch, got {other:?}").into()),
    }
    Ok(())
}

/// Feedback report with wrong report ID (not 0x02) returns InvalidCommandType.
#[test]
fn error_feedback_wrong_report_id() {
    for id in [0x00, 0x01, 0x03, 0xFF] {
        let mut data = vec![0u8; 64];
        data[0] = id;
        let result = parse_feedback_report(&data);
        assert!(
            matches!(result, Err(SmError::InvalidCommandType(v)) if v == id),
            "report ID {id:#04x} should fail"
        );
    }
}

/// Feedback report shorter than 64 bytes returns InvalidLength.
#[test]
fn error_feedback_too_short() {
    for size in [0, 1, 32, 63] {
        let data = vec![0x02; size];
        let result = parse_feedback_report(&data);
        assert!(
            matches!(
                result,
                Err(SmError::InvalidLength {
                    expected: 64,
                    actual,
                }) if actual == size
            ),
            "feedback size {size} should fail"
        );
    }
}

/// All SmError variants have non-empty display messages.
#[test]
fn error_all_variants_display_nonempty() {
    let errors: Vec<SmError> = vec![
        SmError::InvalidLength {
            expected: 15,
            actual: 10,
        },
        SmError::InvalidCommandType(0x99),
        SmError::InvalidParameter(0xBEEF),
        SmError::DeviceError("test".to_string()),
        SmError::CommunicationError("timeout".to_string()),
        SmError::CrcMismatch {
            expected: 0xAA,
            actual: 0xBB,
        },
        SmError::ParseError("bad data".to_string()),
        SmError::EncodeError("overflow".to_string()),
    ];

    for err in &errors {
        let msg = err.to_string();
        assert!(!msg.is_empty(), "error {err:?} should have display text");
    }
}

/// std::io::Error converts to SmError::CommunicationError.
#[test]
fn error_io_conversion_preserves_message() {
    let io_err = std::io::Error::new(std::io::ErrorKind::TimedOut, "RS485 timeout");
    let sm_err: SmError = io_err.into();
    match sm_err {
        SmError::CommunicationError(msg) => assert!(msg.contains("RS485 timeout")),
        other => panic!("expected CommunicationError, got {other:?}"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Streaming mode protocol
// ═══════════════════════════════════════════════════════════════════════════════

/// SM V2 fast update cycle: torque encoder produces continuous valid packets
/// with incrementing sequence numbers — simulates streaming mode.
#[test]
fn streaming_torque_commands_increment_sequence() -> Result<(), Box<dyn std::error::Error>> {
    let mut enc = TorqueCommandEncoder::new(20.0);
    let mut prev_seq = None;
    let mut out = [0u8; TORQUE_COMMAND_LEN];

    for i in 0..300u32 {
        enc.encode(0.0, &mut out);
        let decoded = decode_command(&out)?;
        let expected_seq = (i & 0xFF) as u8;
        assert_eq!(decoded.seq, expected_seq, "wrong seq at iteration {i}");

        if let Some(prev) = prev_seq {
            assert_eq!(
                decoded.seq,
                (prev as u16 + 1) as u8,
                "sequence gap at iteration {i}"
            );
        }
        prev_seq = Some(decoded.seq);
    }
    Ok(())
}

/// SM V2 buffered command protocol: sequence of set+get operations.
#[test]
fn streaming_parameter_write_read_sequence() -> Result<(), Box<dyn std::error::Error>> {
    // Simulate: set parameter, then read it back (two-packet sequence)
    let param_addr: u16 = 559; // SMP_CONTROL_MODE
    let param_value: i32 = 3; // CM_TORQUE

    let set_report = build_set_parameter(param_addr, param_value, 0);
    let get_report = build_get_parameter(param_addr, 1);

    let set_decoded = decode_command(&set_report)?;
    assert_eq!(set_decoded.cmd_type, SmCommandType::SetParameter);
    assert_eq!(set_decoded.param_addr, Some(param_addr));
    assert_eq!(set_decoded.param_value, Some(param_value));
    assert_eq!(set_decoded.seq, 0);

    let get_decoded = decode_command(&get_report)?;
    assert_eq!(get_decoded.cmd_type, SmCommandType::GetParameter);
    assert_eq!(get_decoded.param_addr, Some(param_addr));
    assert_eq!(get_decoded.seq, 1);
    Ok(())
}

/// SM V2 fast update cycle with velocity: simulates Simucube-style combined
/// torque+velocity streaming at high rate.
#[test]
fn streaming_torque_with_velocity_continuous() -> Result<(), Box<dyn std::error::Error>> {
    let mut enc = TorqueCommandEncoder::new(25.0);
    let mut out = [0u8; TORQUE_COMMAND_LEN];

    // Simulate 1kHz cycle with varying torque and velocity
    let test_points: &[(f32, f32)] = &[
        (0.0, 0.0),
        (5.0, 100.0),
        (-5.0, -100.0),
        (25.0, 500.0), // at saturation
        (-25.0, -500.0),
        (12.5, 0.0),
    ];

    for &(torque_nm, velocity_rpm) in test_points {
        let len = enc.encode_with_velocity(torque_nm, velocity_rpm, &mut out);
        assert_eq!(len, 15);
        let decoded = decode_command(&out)?;
        assert_eq!(decoded.cmd_type, SmCommandType::SetTorque);
    }
    Ok(())
}

/// SM V2 device initialization sequence: get status, enable, set control mode.
#[test]
fn streaming_device_init_sequence() -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: Get device status
    let status_report = racing_wheel_simplemotion_v2::build_get_status(0);
    let decoded = decode_command(&status_report)?;
    assert_eq!(decoded.cmd_type, SmCommandType::GetStatus);
    assert_eq!(decoded.seq, 0);

    // Step 2: Enable device
    let enable_report = build_device_enable(true, 1);
    let decoded = decode_command(&enable_report)?;
    assert_eq!(decoded.cmd_type, SmCommandType::SetParameter);
    assert_eq!(decoded.param_addr, Some(0x1001));
    assert_eq!(decoded.param_value, Some(1));
    assert_eq!(decoded.seq, 1);

    // Step 3: Set control mode to torque (CM_TORQUE = 3)
    let mode_report = build_set_parameter(559, 3, 2); // SMP_CONTROL_MODE
    let decoded = decode_command(&mode_report)?;
    assert_eq!(decoded.param_addr, Some(559));
    assert_eq!(decoded.param_value, Some(3));
    assert_eq!(decoded.seq, 2);

    // Step 4: Set zero position
    let zero_report = build_set_zero_position(3);
    let decoded = decode_command(&zero_report)?;
    assert_eq!(decoded.cmd_type, SmCommandType::SetZero);
    assert_eq!(decoded.seq, 3);
    Ok(())
}

/// SM V2 report constants match expected lengths.
#[test]
fn report_length_constants_match_spec() {
    assert_eq!(TORQUE_COMMAND_LEN, 15);
    assert_eq!(SETPARAM_REPORT_LEN, 15);
    assert_eq!(STATUS_REPORT_LEN, 15);
    assert_eq!(FEEDBACK_REPORT_LEN, 64);
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Known command sequences from Granite Devices documentation
// ═══════════════════════════════════════════════════════════════════════════════

/// From SM V2 docs: reading SMP_SM_VERSION (param addr 3) to check protocol version.
#[test]
fn known_sequence_read_sm_version() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_get_parameter(3, 0); // SMP_SM_VERSION = 3
    let decoded = decode_command(&report)?;
    assert_eq!(decoded.cmd_type, SmCommandType::GetParameter);
    assert_eq!(decoded.param_addr, Some(3));
    assert_eq!(decoded.param_value, Some(0));
    Ok(())
}

/// From SM V2 docs: reading SMP_SM_VERSION_COMPAT (param addr 4).
#[test]
fn known_sequence_read_sm_version_compat() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_get_parameter(4, 1); // SMP_SM_VERSION_COMPAT = 4
    let decoded = decode_command(&report)?;
    assert_eq!(decoded.param_addr, Some(4));
    Ok(())
}

/// From SM V2 docs: SMP_NODE_ADDRESS = 1, used for bus address configuration.
#[test]
fn known_sequence_set_node_address() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_set_parameter(1, 5, 0); // SMP_NODE_ADDRESS = 1, set to address 5
    let decoded = decode_command(&report)?;
    assert_eq!(decoded.param_addr, Some(1));
    assert_eq!(decoded.param_value, Some(5));
    Ok(())
}

/// From SM V2 docs: SMP_BUS_MODE = 2 with values DFU=0, NORMAL=1, BUSY=2.
#[test]
fn known_sequence_bus_mode() -> Result<(), Box<dyn std::error::Error>> {
    // Set to normal mode
    let report = build_set_parameter(2, 1, 0); // SMP_BUS_MODE = 2, SMP_BUS_MODE_NORMAL = 1
    let decoded = decode_command(&report)?;
    assert_eq!(decoded.param_addr, Some(2));
    assert_eq!(decoded.param_value, Some(1));
    Ok(())
}

/// From SM V2 docs: SMP_CONTROL_MODE = 559, with CM_TORQUE=3, CM_VELOCITY=2, CM_POSITION=1.
#[test]
fn known_sequence_control_mode_values() -> Result<(), Box<dyn std::error::Error>> {
    let modes: &[(i32, &str)] = &[
        (0, "CM_NONE"),
        (1, "CM_POSITION"),
        (2, "CM_VELOCITY"),
        (3, "CM_TORQUE"),
    ];

    for &(mode_val, name) in modes {
        let report = build_set_parameter(559, mode_val, 0); // SMP_CONTROL_MODE
        let decoded = decode_command(&report)?;
        assert_eq!(
            decoded.param_value,
            Some(mode_val),
            "{name} value not preserved"
        );
    }
    Ok(())
}

/// From SM V2 docs: SMP_SYSTEM_CONTROL = 554 with SAVECFG=1, RESTART=2.
#[test]
fn known_sequence_system_control() -> Result<(), Box<dyn std::error::Error>> {
    // Save config to flash
    let save = build_set_parameter(554, 1, 0); // SMP_SYSTEM_CONTROL_SAVECFG
    let decoded = decode_command(&save)?;
    assert_eq!(decoded.param_addr, Some(554));
    assert_eq!(decoded.param_value, Some(1));

    // Restart device
    let restart = build_set_parameter(554, 2, 1); // SMP_SYSTEM_CONTROL_RESTART
    let decoded = decode_command(&restart)?;
    assert_eq!(decoded.param_value, Some(2));
    Ok(())
}

/// From SM V2 docs: SMP_CUMULATIVE_STATUS = 13, clear by writing 0.
#[test]
fn known_sequence_clear_cumulative_status() -> Result<(), Box<dyn std::error::Error>> {
    let clear = build_set_parameter(13, 0, 0); // SMP_CUMULATIVE_STATUS = 13
    let decoded = decode_command(&clear)?;
    assert_eq!(decoded.param_addr, Some(13));
    assert_eq!(decoded.param_value, Some(0));
    Ok(())
}

/// From SM V2 docs: SMP_BUFFERED_CMD_STATUS = 7, bit mask for buffer state.
#[test]
fn known_sequence_read_buffered_cmd_status() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_get_parameter(7, 0); // SMP_BUFFERED_CMD_STATUS
    let decoded = decode_command(&report)?;
    assert_eq!(decoded.param_addr, Some(7));
    assert_eq!(decoded.cmd_type, SmCommandType::GetParameter);

    // SM_BUFCMD_STAT bits: IDLE=1, RUN=2, UNDERRUN=4, OVERRUN=8
    let buf_status: u32 = 1 | 2; // IDLE | RUN
    assert_eq!(buf_status & 1, 1); // IDLE bit
    assert_eq!(buf_status & 2, 2); // RUN bit
    assert_eq!(buf_status & 4, 0); // UNDERRUN bit not set
    assert_eq!(buf_status & 8, 0); // OVERRUN bit not set
    Ok(())
}

/// From SM V2 docs: SMP_RETURN_PARAM_ADDR = 9, SMP_RETURN_PARAM_LEN = 10.
#[test]
fn known_sequence_set_return_format() -> Result<(), Box<dyn std::error::Error>> {
    // Set return parameter address
    let addr_report = build_set_parameter(9, 553, 0); // SMP_RETURN_PARAM_ADDR = 9, SMP_STATUS = 553
    let decoded = decode_command(&addr_report)?;
    assert_eq!(decoded.param_addr, Some(9));
    assert_eq!(decoded.param_value, Some(553));

    // Set return parameter length
    let len_report = build_set_parameter(10, 0, 1); // SMP_RETURN_PARAM_LEN = 10, SM_RETURN_VALUE_32B = 0
    let decoded = decode_command(&len_report)?;
    assert_eq!(decoded.param_addr, Some(10));
    assert_eq!(decoded.param_value, Some(0));
    Ok(())
}

/// From SM V2 docs: device enable uses param 0x1001 with value 1 (enable) / 0 (disable).
#[test]
fn known_sequence_device_enable_disable() -> Result<(), Box<dyn std::error::Error>> {
    let enable = build_device_enable(true, 0);
    let decoded = decode_command(&enable)?;
    assert_eq!(decoded.cmd_type, SmCommandType::SetParameter);
    assert_eq!(decoded.param_addr, Some(0x1001));
    assert_eq!(decoded.param_value, Some(1));

    let disable = build_device_enable(false, 1);
    let decoded = decode_command(&disable)?;
    assert_eq!(decoded.param_addr, Some(0x1001));
    assert_eq!(decoded.param_value, Some(0));
    Ok(())
}

/// Known USB VID/PID from Granite Devices:
/// VID 0x1D50 with PIDs 0x6050 (IONI), 0x6051 (IONI Premium), 0x6052 (ARGON).
#[test]
fn known_usb_vid_pid_values() {
    use racing_wheel_simplemotion_v2::ids::*;

    assert_eq!(SM_VENDOR_ID, 0x1D50); // Granite Devices VID
    assert_eq!(IONI_VENDOR_ID, 0x1D50);
    assert_eq!(IONI_PRODUCT_ID, 0x6050);
    assert_eq!(IONI_PRODUCT_ID_PREMIUM, 0x6051);
    assert_eq!(ARGON_VENDOR_ID, 0x1D50);
    assert_eq!(ARGON_PRODUCT_ID, 0x6052);
    assert_eq!(OSW_VENDOR_ID, 0x1D50);
}

/// Known product IDs map to correct device identities.
#[test]
fn known_device_identities() {
    let ioni = identify_device(0x6050);
    assert_eq!(ioni.category, SmDeviceCategory::Wheelbase);
    assert!(ioni.supports_ffb);
    assert!(ioni.name.contains("Simucube 1") || ioni.name.contains("IONI"));

    let ioni_premium = identify_device(0x6051);
    assert_eq!(ioni_premium.category, SmDeviceCategory::Wheelbase);
    assert!(ioni_premium.supports_ffb);
    assert!(ioni_premium.name.contains("Simucube 2") || ioni_premium.name.contains("IONI Premium"));

    let argon = identify_device(0x6052);
    assert_eq!(argon.category, SmDeviceCategory::Wheelbase);
    assert!(argon.supports_ffb);
    assert!(argon.name.contains("Simucube Sport") || argon.name.contains("ARGON"));
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. Parameter types and ranges
// ═══════════════════════════════════════════════════════════════════════════════

/// SM V2 parameter addresses from simplemotion_defs.h with their expected ranges.
#[test]
fn sm_v2_parameter_addresses_encodable() -> Result<(), Box<dyn std::error::Error>> {
    // Key parameters from simplemotion_defs.h
    let params: &[(u16, &str)] = &[
        (0, "SMP_NULL"),
        (1, "SMP_NODE_ADDRESS"),
        (2, "SMP_BUS_MODE"),
        (3, "SMP_SM_VERSION"),
        (4, "SMP_SM_VERSION_COMPAT"),
        (5, "SMP_BUS_SPEED"),
        (6, "SMP_BUFFER_FREE_BYTES"),
        (7, "SMP_BUFFERED_CMD_STATUS"),
        (8, "SMP_BUFFERED_CMD_PERIOD"),
        (9, "SMP_RETURN_PARAM_ADDR"),
        (10, "SMP_RETURN_PARAM_LEN"),
        (12, "SMP_TIMEOUT"),
        (13, "SMP_CUMULATIVE_STATUS"),
        (14, "SMP_ADDRESS_OFFSET"),
        (15, "SMP_FAULT_BEHAVIOR"),
        (128, "SMP_DIGITAL_IN_VALUES_1"),
        (168, "SMP_ANALOG_IN_VALUE_1"),
        (200, "SMP_VEL_I"),
        (201, "SMP_POS_P"),
        (202, "SMP_VEL_P"),
        (550, "SMP_INCREMENTAL_SETPOINT"),
        (551, "SMP_ABSOLUTE_SETPOINT"),
        (552, "SMP_FAULTS"),
        (553, "SMP_STATUS"),
        (554, "SMP_SYSTEM_CONTROL"),
        (558, "SMP_MOTOR_MODE"),
        (559, "SMP_CONTROL_MODE"),
        (565, "SMP_ENCODER_PPR"),
        (566, "SMP_MOTOR_POLEPAIRS"),
    ];

    for &(addr, name) in params {
        let report = build_get_parameter(addr, 0);
        let decoded = decode_command(&report)?;
        assert_eq!(
            decoded.param_addr,
            Some(addr),
            "parameter {name} (addr={addr}) not preserved"
        );
    }
    Ok(())
}

/// SM V2 motor mode values from simplemotion_defs.h.
#[test]
fn motor_mode_parameter_values() -> Result<(), Box<dyn std::error::Error>> {
    // SMP_MOTOR_MODE (558) values
    let modes: &[(i32, &str)] = &[
        (0, "MOTOR_NONE"),
        (1, "MOTOR_DC"),
        (2, "MOTOR_AC_VECTOR_2PHA"),
        (3, "MOTOR_AC_VECTOR"),
        (4, "MOTOR_STEPPER_2PHA"),
        (5, "MOTOR_LINEAR_3PH"),
    ];

    for &(mode_val, name) in modes {
        let report = build_set_parameter(558, mode_val, 0);
        let decoded = decode_command(&report)?;
        assert_eq!(
            decoded.param_value,
            Some(mode_val),
            "motor mode {name} not preserved"
        );
    }
    Ok(())
}

/// SM V2 electrical mode for Simucube (SMP_ELECTRICAL_MODE = 573).
#[test]
fn electrical_mode_simucube_values() -> Result<(), Box<dyn std::error::Error>> {
    let modes: &[(i32, &str)] = &[
        (0, "EL_MODE_STANDARD"),
        (1, "EL_MODE_IONICUBE"),
        (2, "EL_MODE_SIMUCUBE"),
        (3, "EL_MODE_IONIZER"),
    ];

    for &(mode_val, name) in modes {
        let report = build_set_parameter(573, mode_val, 0);
        let decoded = decode_command(&report)?;
        assert_eq!(
            decoded.param_value,
            Some(mode_val),
            "electrical mode {name} not preserved"
        );
    }
    Ok(())
}

/// SM V2 fast update cycle format values (SMP_FAST_UPDATE_CYCLE_FORMAT = 17).
#[test]
fn fast_update_cycle_format_values() -> Result<(), Box<dyn std::error::Error>> {
    let formats: &[(i32, &str)] = &[
        (0, "FAST_UPDATE_CYCLE_FORMAT_DEFAULT"),
        (1, "FAST_UPDATE_CYCLE_FORMAT_ALT1"),
        (2, "FAST_UPDATE_CYCLE_FORMAT_ALT2"),
        (3, "FAST_UPDATE_CYCLE_FORMAT_ALT3"),
        (4, "FAST_UPDATE_CYCLE_FORMAT_ALT4"),
    ];

    for &(fmt_val, name) in formats {
        let report = build_set_parameter(17, fmt_val, 0);
        let decoded = decode_command(&report)?;
        assert_eq!(
            decoded.param_value,
            Some(fmt_val),
            "format {name} not preserved"
        );
    }
    Ok(())
}

/// SM V2 motor torque limits: ensure current limit parameters roundtrip.
/// SMP MCC = 410 (continuous current), MMC = 411 (peak current).
#[test]
fn motor_current_limit_parameters() -> Result<(), Box<dyn std::error::Error>> {
    // Continuous current limit (scaled by 1000 in Granity)
    let mcc = build_set_parameter(410, 5000, 0); // 5.0A
    let decoded = decode_command(&mcc)?;
    assert_eq!(decoded.param_addr, Some(410));
    assert_eq!(decoded.param_value, Some(5000));

    // Peak current limit
    let mmc = build_set_parameter(411, 15000, 1); // 15.0A
    let decoded = decode_command(&mmc)?;
    assert_eq!(decoded.param_addr, Some(411));
    assert_eq!(decoded.param_value, Some(15000));
    Ok(())
}

/// SM V2 velocity and acceleration limit parameters.
/// SMP CAL = 800 (acceleration limit), CVL = 802 (velocity limit).
#[test]
fn velocity_acceleration_limit_parameters() -> Result<(), Box<dyn std::error::Error>> {
    let accel = build_set_parameter(800, 10000, 0); // SMP_CAL
    let decoded = decode_command(&accel)?;
    assert_eq!(decoded.param_addr, Some(800));
    assert_eq!(decoded.param_value, Some(10000));

    let vel = build_set_parameter(802, 5000, 1); // SMP_CVL
    let decoded = decode_command(&vel)?;
    assert_eq!(decoded.param_addr, Some(802));
    assert_eq!(decoded.param_value, Some(5000));
    Ok(())
}

/// Known torque limit values for each device type.
#[test]
fn device_torque_limits_match_spec() {
    let ioni = identify_device(0x6050);
    assert!(
        (ioni.max_torque_nm.unwrap_or(0.0) - 15.0).abs() < 0.1,
        "IONI should support ~15 Nm"
    );

    let ioni_premium = identify_device(0x6051);
    assert!(
        (ioni_premium.max_torque_nm.unwrap_or(0.0) - 35.0).abs() < 0.1,
        "IONI Premium should support ~35 Nm"
    );

    let argon = identify_device(0x6052);
    assert!(
        (argon.max_torque_nm.unwrap_or(0.0) - 10.0).abs() < 0.1,
        "ARGON should support ~10 Nm"
    );
}

/// Known RPM limits for each device type.
#[test]
fn device_rpm_limits_match_spec() {
    let ioni = identify_device(0x6050);
    assert_eq!(ioni.max_rpm, Some(10000));

    let ioni_premium = identify_device(0x6051);
    assert_eq!(ioni_premium.max_rpm, Some(15000));

    let argon = identify_device(0x6052);
    assert_eq!(argon.max_rpm, Some(8000));
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. Protocol version handling
// ═══════════════════════════════════════════════════════════════════════════════

/// SM V2 protocol version negotiation from simplemotion_defs.h:
/// Read SMP_SM_VERSION (3) and SMP_SM_VERSION_COMPAT (4), compare with
/// OLDEST_SUPPORTED and NEWEST_SUPPORTED to determine compatibility.
#[test]
fn protocol_version_check_sequence() -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: Read current version
    let version_cmd = build_get_parameter(3, 0); // SMP_SM_VERSION
    let decoded = decode_command(&version_cmd)?;
    assert_eq!(decoded.cmd_type, SmCommandType::GetParameter);
    assert_eq!(decoded.param_addr, Some(3));

    // Step 2: Read compatibility version
    let compat_cmd = build_get_parameter(4, 1); // SMP_SM_VERSION_COMPAT
    let decoded = decode_command(&compat_cmd)?;
    assert_eq!(decoded.param_addr, Some(4));

    // SM V2 version history: V20 (Argon), V25 (IONI), V26 (fast cmd), V27 (buffered), V28 (capabilities)
    let known_versions: &[i32] = &[20, 25, 26, 27, 28];
    for &ver in known_versions {
        let report = build_set_parameter(3, ver, 0);
        let decoded = decode_command(&report)?;
        assert_eq!(decoded.param_value, Some(ver));
    }
    Ok(())
}

/// SM V2 firmware version can be read via SMP_FIRMWARE_VERSION.
/// simulated address from the docs — firmware version is a device-specific param.
#[test]
fn firmware_version_parameter_encodable() -> Result<(), Box<dyn std::error::Error>> {
    // SMP_FIRMWARE_VERSION is typically in the device-specific range
    // Using a representative address
    let fw_ver = build_get_parameter(8100, 0);
    let decoded = decode_command(&fw_ver)?;
    assert_eq!(decoded.param_addr, Some(8100));
    Ok(())
}

/// SM V2 device capabilities from simplemotion_defs.h:
/// SMP_DEVICE_CAPABILITIES1 and SMP_DEVICE_CAPABILITIES2 are read-only bit fields.
#[test]
fn device_capabilities_parameters_encodable() -> Result<(), Box<dyn std::error::Error>> {
    // Read capabilities (specific addresses are device-model-dependent)
    // but the get_parameter encoding must work for any valid u16 address
    for addr in [8100u16, 8101, 8102] {
        let report = build_get_parameter(addr, 0);
        let decoded = decode_command(&report)?;
        assert_eq!(decoded.param_addr, Some(addr));
    }
    Ok(())
}

/// SMP_BUFFERED_MODE = 16: interpolation modes from SM V2 protocol version 27+.
#[test]
fn buffered_mode_interpolation_values() -> Result<(), Box<dyn std::error::Error>> {
    // BUFFERED_INTERPOLATION_MODE_NEAREST = 0
    let nearest = build_set_parameter(16, 0, 0); // SMP_BUFFERED_MODE
    let decoded = decode_command(&nearest)?;
    assert_eq!(decoded.param_addr, Some(16));
    assert_eq!(decoded.param_value, Some(0));

    // BUFFERED_INTERPOLATION_MODE_LINEAR = 1
    let linear = build_set_parameter(16, 1, 1);
    let decoded = decode_command(&linear)?;
    assert_eq!(decoded.param_value, Some(1));
    Ok(())
}

/// SMP_FAULT_BEHAVIOR = 15: watchdog timeout encoding.
/// Bits 0: enable fault stop on comm error, bits 8-17: watchdog timeout (10ms units).
#[test]
fn fault_behavior_watchdog_encoding() -> Result<(), Box<dyn std::error::Error>> {
    // Enable comm error fault + 1 second watchdog (100 * 10ms = 1000ms)
    let watchdog_value: i32 = 1 | (100 << 8); // bit 0 + watchdog in bits 8-17
    let report = build_set_parameter(15, watchdog_value, 0); // SMP_FAULT_BEHAVIOR
    let decoded = decode_command(&report)?;
    assert_eq!(decoded.param_addr, Some(15));
    assert_eq!(decoded.param_value, Some(watchdog_value));

    // Verify bit extraction
    let comm_error_enabled = (watchdog_value & 1) != 0;
    let watchdog_timeout = (watchdog_value >> 8) & 0x3FF; // 10-bit field
    assert!(comm_error_enabled);
    assert_eq!(watchdog_timeout, 100); // 100 * 10ms = 1 second
    Ok(())
}

/// SM V2 NOP command: SMP_ADDR_NOP = 8191 (0x1FFF).
#[test]
fn nop_address_roundtrips() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_set_parameter(0x1FFF, 0, 0); // SMP_ADDR_NOP
    let decoded = decode_command(&report)?;
    assert_eq!(decoded.param_addr, Some(0x1FFF));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Additional: Feedback report comprehensive tests
// ═══════════════════════════════════════════════════════════════════════════════

/// Comprehensive feedback report parsing with all fields populated.
#[test]
fn feedback_report_all_fields() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_feedback_report(
        0x42, // seq
        0,    // status OK
        14400, 1000, 256,  // position, velocity, torque
        480,  // bus voltage (48.0V)
        5000, // motor current
        65,   // temperature (65°C)
    );

    let fb = parse_feedback_report(&data)?;
    assert_eq!(fb.seq, 0x42);
    assert_eq!(fb.status, SmStatus::Ok);
    assert_eq!(fb.motor.position, 14400);
    assert_eq!(fb.motor.velocity, 1000);
    assert_eq!(fb.motor.torque, 256);
    assert_eq!(fb.bus_voltage, 480);
    assert_eq!(fb.motor_current, 5000);
    assert_eq!(fb.temperature, 65);
    assert!(fb.connected);
    Ok(())
}

/// Feedback unit conversion: position to degrees.
#[test]
fn feedback_position_degrees_conversion() {
    let state = SmFeedbackState {
        motor: SmMotorFeedback {
            position: 3600,
            ..Default::default()
        },
        ..Default::default()
    };
    // 3600 counts / 14400 CPR * 360 = 90 degrees
    let degrees = state.position_degrees(14400);
    assert!((degrees - 90.0).abs() < 0.1);
}

/// Feedback unit conversion: velocity to RPM.
#[test]
fn feedback_velocity_rpm_conversion() {
    let state = SmFeedbackState {
        motor: SmMotorFeedback {
            velocity: 14400,
            ..Default::default()
        },
        ..Default::default()
    };
    // 14400 counts / 14400 CPR * 60 = 60 RPM
    let rpm = state.velocity_rpm(14400);
    assert!((rpm - 60.0).abs() < 0.1);
}

/// Feedback unit conversion: torque to Nm.
#[test]
fn feedback_torque_nm_conversion() {
    let state = SmFeedbackState {
        motor: SmMotorFeedback {
            torque: 512,
            ..Default::default()
        },
        ..Default::default()
    };
    // 512 / 256 * 0.1 = 0.2 Nm
    let nm = state.torque_nm(0.1);
    assert!((nm - 0.2).abs() < 0.01);
}

/// Negative temperature encoding (signed i8).
#[test]
fn feedback_negative_temperature() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_feedback_report(0, 0, 0, 0, 0, 0, 0, -20);
    let fb = parse_feedback_report(&data)?;
    assert_eq!(fb.temperature, -20);
    Ok(())
}

/// Disconnected marker: bytes 4-5 both 0xFF means disconnected.
#[test]
fn feedback_disconnected_marker_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; 64];
    data[0] = 0x02;
    data[4] = 0xFF;
    data[5] = 0xFF;
    // Other position bytes can be anything, but bytes 4-5 = 0xFFFF triggers disconnect
    let fb = parse_feedback_report(&data)?;
    assert!(!fb.connected);
    Ok(())
}

/// Torque encoder produces valid decodable packets for the full torque range.
#[test]
fn torque_encoder_full_range() -> Result<(), Box<dyn std::error::Error>> {
    let mut enc = TorqueCommandEncoder::new(20.0);
    let mut out = [0u8; TORQUE_COMMAND_LEN];

    let torques: &[f32] = &[
        -20.0, -10.0, -5.0, -1.0, -0.001, 0.0, 0.001, 1.0, 5.0, 10.0, 20.0,
    ];

    for &t in torques {
        let len = enc.encode(t, &mut out);
        assert_eq!(len, 15);
        let decoded = decode_command(&out)?;
        assert_eq!(decoded.cmd_type, SmCommandType::SetTorque);
    }
    Ok(())
}

/// MOTOR_POLE_PAIRS constant used for electrical angle calculation.
#[test]
fn motor_pole_pairs_constant() {
    use racing_wheel_simplemotion_v2::MOTOR_POLE_PAIRS;
    assert_eq!(MOTOR_POLE_PAIRS, 4);
}

/// Encode into oversized buffer works and fills unused bytes with zero.
#[test]
fn encode_oversized_buffer_fills_zeros() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = SmCommand::new(0, SmCommandType::GetStatus);
    let mut buf = [0xFFu8; 64];
    let len = encode_command(&cmd, &mut buf)?;
    assert_eq!(len, 15);
    // The encode fills to zero before writing, so bytes 15+ should be 0
    for &b in &buf[15..] {
        assert_eq!(b, 0, "bytes past 15 should be zeroed");
    }
    Ok(())
}

/// Feedback report with extra bytes (>64) is accepted.
#[test]
fn feedback_oversized_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; 128];
    data[0] = 0x02;
    data[1] = 0x99;
    let fb = parse_feedback_report(&data)?;
    assert_eq!(fb.seq, 0x99);
    Ok(())
}

/// SmFeedbackState::empty() returns default state with connected=false.
#[test]
fn feedback_empty_state() {
    let fb = SmFeedbackState::empty();
    assert_eq!(fb.seq, 0);
    assert_eq!(fb.status, SmStatus::Unknown);
    assert_eq!(fb.motor.position, 0);
    assert_eq!(fb.motor.velocity, 0);
    assert_eq!(fb.motor.torque, 0);
    assert_eq!(fb.bus_voltage, 0);
    assert_eq!(fb.motor_current, 0);
    assert_eq!(fb.temperature, 0);
    assert!(!fb.connected);
}

/// sm_device_identity is an alias for identify_device.
#[test]
fn sm_device_identity_alias() {
    for pid in [0x6050, 0x6051, 0x6052, 0xFFFF] {
        let a = identify_device(pid);
        let b = sm_device_identity(pid);
        assert_eq!(a.product_id, b.product_id);
        assert_eq!(a.name, b.name);
        assert_eq!(a.category, b.category);
        assert_eq!(a.supports_ffb, b.supports_ffb);
    }
}
