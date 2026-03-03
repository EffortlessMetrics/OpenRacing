//! Tests for RT device communication layer
//!
//! Covers HID report construction per vendor, report parsing from raw bytes,
//! device capability negotiation, malformed-report error handling, and edge cases.

use racing_wheel_engine::{
    CapabilityNegotiator, GameCompatibility, HidDevice, HidPort, ModeSelectionPolicy,
    VirtualDevice, VirtualHidPort,
};
use racing_wheel_engine::hid::{
    self, DeviceCapabilitiesReport, DeviceTelemetryReport, TorqueCommand, MAX_TORQUE_REPORT_SIZE,
};
use racing_wheel_engine::hid::quirks::DeviceQuirks;
use racing_wheel_engine::hid::vendor;
use racing_wheel_engine::protocol::{
    self, DeviceCapabilitiesReport as ProtocolCapabilitiesReport,
    DeviceTelemetryReport as ProtocolTelemetryReport, SafetyInterlockAck,
    SafetyInterlockChallenge, TorqueCommand as ProtocolTorqueCommand,
};
use racing_wheel_engine::rt::FFBMode;
use racing_wheel_schemas::prelude::*;


// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_device_id(name: &str) -> Result<DeviceId, Box<dyn std::error::Error>> {
    Ok(name.parse::<DeviceId>()?)
}

fn test_caps(
    pid: bool,
    raw_torque: bool,
    nm: f32,
) -> Result<DeviceCapabilities, Box<dyn std::error::Error>> {
    Ok(DeviceCapabilities::new(
        pid,
        raw_torque,
        true,
        true,
        TorqueNm::new(nm)?,
        10000,
        1000,
    ))
}

// ===================================================================
// 1. HID report construction per vendor
// ===================================================================

#[test]
fn torque_report_generic_device_uses_owp1_layout() -> Result<(), Box<dyn std::error::Error>> {
    let mut out = [0u8; MAX_TORQUE_REPORT_SIZE];
    let len = hid::encode_torque_report_for_device(0x046D, 0xC294, 5.0, 2.0, 77, &mut out);

    assert_eq!(len, std::mem::size_of::<TorqueCommand>());
    assert_eq!(out[0], TorqueCommand::REPORT_ID);
    let encoded = i16::from_le_bytes([out[1], out[2]]);
    assert_eq!(encoded, (2.0_f32 * 256.0) as i16);
    let seq = u16::from_le_bytes([out[4], out[5]]);
    assert_eq!(seq, 77);
    Ok(())
}

#[test]
fn torque_report_fanatec_encodes_constant_force() -> Result<(), Box<dyn std::error::Error>> {
    let mut out = [0u8; MAX_TORQUE_REPORT_SIZE];
    let len = hid::encode_torque_report_for_device(0x0EB7, 0x0024, 8.0, 8.0, 0, &mut out);

    assert_eq!(len, vendor::fanatec::CONSTANT_FORCE_REPORT_LEN);
    assert_eq!(out[0], 0x01); // FFB output report ID
    assert_eq!(out[1], 0x01); // constant force command
    assert_eq!(i16::from_le_bytes([out[2], out[3]]), i16::MAX);
    Ok(())
}

#[test]
fn torque_report_fanatec_zero_produces_zero_payload() -> Result<(), Box<dyn std::error::Error>> {
    let mut out = [0u8; MAX_TORQUE_REPORT_SIZE];
    let len = hid::encode_torque_report_for_device(0x0EB7, 0x0020, 8.0, 0.0, 0, &mut out);

    assert_eq!(len, vendor::fanatec::CONSTANT_FORCE_REPORT_LEN);
    assert_eq!(out[2], 0x00);
    assert_eq!(out[3], 0x00);
    Ok(())
}

#[test]
fn torque_report_moza_uses_direct_layout() -> Result<(), Box<dyn std::error::Error>> {
    let mut out = [0u8; MAX_TORQUE_REPORT_SIZE];
    let len = hid::encode_torque_report_for_device(0x346E, 0x0004, 5.5, 5.5, 11, &mut out);

    assert_eq!(len, MAX_TORQUE_REPORT_SIZE);
    assert_eq!(out[0], vendor::moza::report_ids::DIRECT_TORQUE);
    assert_eq!(i16::from_le_bytes([out[1], out[2]]), i16::MAX);
    Ok(())
}

#[test]
fn torque_report_fanatec_negative_torque_encodes_correctly() -> Result<(), Box<dyn std::error::Error>>
{
    let mut out = [0u8; MAX_TORQUE_REPORT_SIZE];
    let len = hid::encode_torque_report_for_device(0x0EB7, 0x0024, 8.0, -8.0, 1, &mut out);

    assert_eq!(len, vendor::fanatec::CONSTANT_FORCE_REPORT_LEN);
    let force = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(force, i16::MIN);
    Ok(())
}

#[test]
fn torque_report_moza_negative_torque_encodes_correctly() -> Result<(), Box<dyn std::error::Error>> {
    let mut out = [0u8; MAX_TORQUE_REPORT_SIZE];
    let len = hid::encode_torque_report_for_device(0x346E, 0x0004, 5.5, -5.5, 0, &mut out);

    assert_eq!(len, MAX_TORQUE_REPORT_SIZE);
    let force = i16::from_le_bytes([out[1], out[2]]);
    assert_eq!(force, i16::MIN);
    Ok(())
}

// ===================================================================
// 2. Report parsing from raw bytes (OWP-1 protocol structs)
// ===================================================================

#[test]
fn protocol_torque_command_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = ProtocolTorqueCommand::new(10.5, protocol::torque_flags::HANDS_ON_HINT, 1234);
    let bytes = cmd.to_bytes();
    let parsed = ProtocolTorqueCommand::from_bytes(&bytes)?;

    assert!((parsed.torque_nm() - 10.5).abs() < f32::EPSILON);
    assert_eq!(parsed.flags, protocol::torque_flags::HANDS_ON_HINT);
    let seq = parsed.sequence;
    assert_eq!(seq, 1234);
    assert!(parsed.validate_crc());
    Ok(())
}

#[test]
fn protocol_telemetry_report_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let report = ProtocolTelemetryReport::new(45.5, 2.5, 42, 0, true, 1234);
    let bytes = report.to_bytes();
    let parsed = ProtocolTelemetryReport::from_bytes(&bytes)?;

    assert!((parsed.wheel_angle_deg() - 45.5).abs() < f32::EPSILON);
    assert!((parsed.wheel_speed_rad_s() - 2.5).abs() < f32::EPSILON);
    assert_eq!(parsed.temp_c, 42);
    assert_eq!(parsed.hands_on(), Some(true));
    assert!(parsed.validate_crc());
    Ok(())
}

#[test]
fn protocol_capabilities_report_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let report = ProtocolCapabilitiesReport::new(true, true, true, true, 2500, 10000, 1000);
    let bytes = report.to_bytes();
    let parsed = ProtocolCapabilitiesReport::from_bytes(&bytes)?;

    assert!(parsed.supports_pid());
    assert!(parsed.supports_raw_torque_1khz());
    assert!(parsed.supports_health_stream());
    assert!(parsed.supports_led_bus());
    assert!((parsed.max_torque_nm() - 25.0).abs() < f32::EPSILON);
    let encoder_cpr = parsed.encoder_cpr;
    assert_eq!(encoder_cpr, 10000);
    assert!((parsed.max_update_rate_hz() - 1000.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn protocol_safety_challenge_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let challenge = SafetyInterlockChallenge::new(
        0xDEADBEEF,
        SafetyInterlockChallenge::COMBO_BOTH_CLUTCH,
        3000,
        1700000000,
    );
    let bytes = challenge.to_bytes();
    let parsed = SafetyInterlockChallenge::from_bytes(&bytes)?;

    let token = parsed.challenge_token;
    let combo = parsed.combo_type;
    let hold = parsed.hold_duration_ms;
    let expires = parsed.expires_unix_secs;
    assert_eq!(token, 0xDEADBEEF);
    assert_eq!(combo, SafetyInterlockChallenge::COMBO_BOTH_CLUTCH);
    assert_eq!(hold, 3000);
    assert_eq!(expires, 1700000000);
    Ok(())
}

#[test]
fn protocol_safety_ack_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let ack = SafetyInterlockAck::new(0xCAFEBABE, 0x12345678, 0, 2500, 999999);
    let bytes = ack.to_bytes();
    let parsed = SafetyInterlockAck::from_bytes(&bytes)?;

    assert!(parsed.validate_crc());
    let ct = parsed.challenge_token;
    let dt = parsed.device_token;
    assert_eq!(ct, 0xCAFEBABE);
    assert_eq!(dt, 0x12345678);
    Ok(())
}

#[test]
fn hid_mod_telemetry_from_bytes_valid() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; std::mem::size_of::<DeviceTelemetryReport>()];
    data[0] = DeviceTelemetryReport::REPORT_ID;
    let angle_bytes = 90_000i32.to_le_bytes();
    data[1..5].copy_from_slice(&angle_bytes);

    let report =
        DeviceTelemetryReport::from_bytes(&data).ok_or("telemetry deserialization failed")?;
    let tel = report.to_telemetry_data();
    assert!((tel.wheel_angle_deg - 90.0).abs() < 0.01);
    Ok(())
}

#[test]
fn hid_mod_capabilities_from_bytes_all_features() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; std::mem::size_of::<DeviceCapabilitiesReport>()];
    data[0] = DeviceCapabilitiesReport::REPORT_ID;
    data[1] = 0x01; // supports_pid
    data[2] = 0x01; // supports_raw_torque_1khz
    data[3] = 0x01; // supports_health_stream
    data[4] = 0x01; // supports_led_bus
    data[5..7].copy_from_slice(&2500u16.to_le_bytes()); // 25 Nm
    data[7..9].copy_from_slice(&4096u16.to_le_bytes()); // encoder CPR

    let report =
        DeviceCapabilitiesReport::from_bytes(&data).ok_or("capabilities deserialization failed")?;
    let caps = report.to_device_capabilities();
    assert!(caps.supports_pid);
    assert!(caps.supports_raw_torque_1khz);
    assert!((caps.max_torque.value() - 25.0).abs() < 0.01);
    assert_eq!(caps.encoder_cpr, 4096);
    Ok(())
}

// ===================================================================
// 3. Device capability negotiation
// ===================================================================

#[test]
fn negotiation_prefers_raw_torque_for_full_caps() -> Result<(), Box<dyn std::error::Error>> {
    let caps = test_caps(true, true, 25.0)?;
    let result = CapabilityNegotiator::negotiate_capabilities(&caps, None);

    assert_eq!(result.mode, FFBMode::RawTorque);
    assert!((result.update_rate_hz - 1000.0).abs() < f32::EPSILON);
    assert!(result.is_optimal());
    Ok(())
}

#[test]
fn negotiation_falls_back_to_pid() -> Result<(), Box<dyn std::error::Error>> {
    let caps = test_caps(true, false, 15.0)?;
    let result = CapabilityNegotiator::negotiate_capabilities(&caps, None);

    assert_eq!(result.mode, FFBMode::PidPassthrough);
    assert!((result.update_rate_hz - 60.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn negotiation_falls_back_to_telemetry_synth() -> Result<(), Box<dyn std::error::Error>> {
    let caps = test_caps(false, false, 10.0)?;
    let result = CapabilityNegotiator::negotiate_capabilities(&caps, None);

    assert_eq!(result.mode, FFBMode::TelemetrySynth);
    assert!(!result.is_optimal());
    assert!(!result.warnings.is_empty());
    Ok(())
}

#[test]
fn negotiation_respects_game_telemetry_preference() -> Result<(), Box<dyn std::error::Error>> {
    let caps = test_caps(true, true, 25.0)?;
    let game = GameCompatibility {
        game_id: "arcade-racer".to_string(),
        supports_robust_ffb: false,
        supports_telemetry: true,
        preferred_mode: FFBMode::TelemetrySynth,
    };

    let mode = ModeSelectionPolicy::select_mode(&caps, Some(&game));
    assert_eq!(mode, FFBMode::TelemetrySynth);
    Ok(())
}

#[test]
fn negotiation_respects_game_robust_ffb() -> Result<(), Box<dyn std::error::Error>> {
    let caps = test_caps(true, true, 25.0)?;
    let game = GameCompatibility {
        game_id: "sim-racer".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: FFBMode::RawTorque,
    };

    let mode = ModeSelectionPolicy::select_mode(&caps, Some(&game));
    assert_eq!(mode, FFBMode::RawTorque);
    Ok(())
}

#[test]
fn capabilities_report_round_trip_through_negotiator() -> Result<(), Box<dyn std::error::Error>> {
    let caps = test_caps(true, true, 25.0)?;
    let report = CapabilityNegotiator::create_capabilities_report(&caps);
    let parsed = CapabilityNegotiator::parse_capabilities_report(&report)?;

    assert_eq!(caps.supports_pid, parsed.supports_pid);
    assert_eq!(
        caps.supports_raw_torque_1khz,
        parsed.supports_raw_torque_1khz
    );
    assert!((caps.max_torque.value() - parsed.max_torque.value()).abs() < f32::EPSILON);
    assert_eq!(caps.encoder_cpr, parsed.encoder_cpr);
    Ok(())
}

#[test]
fn mode_compatibility_matrix() -> Result<(), Box<dyn std::error::Error>> {
    let full = test_caps(true, true, 25.0)?;
    assert!(ModeSelectionPolicy::is_mode_compatible(
        FFBMode::PidPassthrough,
        &full
    ));
    assert!(ModeSelectionPolicy::is_mode_compatible(
        FFBMode::RawTorque,
        &full
    ));
    assert!(ModeSelectionPolicy::is_mode_compatible(
        FFBMode::TelemetrySynth,
        &full
    ));

    let no_pid = test_caps(false, true, 25.0)?;
    assert!(!ModeSelectionPolicy::is_mode_compatible(
        FFBMode::PidPassthrough,
        &no_pid
    ));
    assert!(ModeSelectionPolicy::is_mode_compatible(
        FFBMode::RawTorque,
        &no_pid
    ));

    let no_raw = test_caps(true, false, 25.0)?;
    assert!(!ModeSelectionPolicy::is_mode_compatible(
        FFBMode::RawTorque,
        &no_raw
    ));
    assert!(ModeSelectionPolicy::is_mode_compatible(
        FFBMode::PidPassthrough,
        &no_raw
    ));
    Ok(())
}

// ===================================================================
// 4. Error handling for malformed reports
// ===================================================================

#[test]
fn protocol_torque_too_short_rejected() {
    let short = [0x20, 0x00];
    let result = ProtocolTorqueCommand::from_bytes(&short);
    assert!(result.is_err());
}

#[test]
fn protocol_torque_wrong_report_id_rejected() {
    let mut bytes = ProtocolTorqueCommand::new(1.0, 0, 0).to_bytes();
    bytes[0] = 0xFF;
    let result = ProtocolTorqueCommand::from_bytes(&bytes);
    assert!(result.is_err());
}

#[test]
fn protocol_torque_corrupted_crc_rejected() {
    let mut bytes = ProtocolTorqueCommand::new(5.0, 0, 100).to_bytes();
    bytes[1] = 0xFF; // corrupt torque
    let result = ProtocolTorqueCommand::from_bytes(&bytes);
    assert!(result.is_err());
}

#[test]
fn protocol_telemetry_too_short_rejected() {
    let short = [0x21, 0x00, 0x00];
    let result = ProtocolTelemetryReport::from_bytes(&short);
    assert!(result.is_err());
}

#[test]
fn protocol_telemetry_wrong_report_id_rejected() {
    let mut bytes = ProtocolTelemetryReport::new(0.0, 0.0, 25, 0, false, 0).to_bytes();
    bytes[0] = 0xAA;
    let result = ProtocolTelemetryReport::from_bytes(&bytes);
    assert!(result.is_err());
}

#[test]
fn protocol_telemetry_corrupted_crc_rejected() {
    let mut bytes = ProtocolTelemetryReport::new(0.0, 0.0, 25, 0, false, 0).to_bytes();
    bytes[5] = 0xFF;
    let result = ProtocolTelemetryReport::from_bytes(&bytes);
    assert!(result.is_err());
}

#[test]
fn protocol_capabilities_too_short_rejected() {
    let short = [0x01, 0x0F, 0x00];
    let result = ProtocolCapabilitiesReport::from_bytes(&short);
    assert!(result.is_err());
}

#[test]
fn protocol_capabilities_wrong_report_id_rejected() {
    let mut bytes = ProtocolCapabilitiesReport::new(true, true, true, true, 2500, 10000, 1000)
        .to_bytes();
    bytes[0] = 0x99;
    let result = ProtocolCapabilitiesReport::from_bytes(&bytes);
    assert!(result.is_err());
}

#[test]
fn negotiator_parse_rejects_short_report() {
    let short = [0x01, 0x0F, 0x00, 0x00];
    let result = CapabilityNegotiator::parse_capabilities_report(&short);
    assert!(result.is_err());
}

#[test]
fn negotiator_parse_rejects_wrong_report_id() {
    let report = vec![0xFF, 0x0F, 0xC4, 0x09, 0x10, 0x27, 0xE8, 0x03];
    let result = CapabilityNegotiator::parse_capabilities_report(&report);
    assert!(result.is_err());
}

#[test]
fn protocol_safety_challenge_too_short_rejected() {
    let short = [0x03, 0x00, 0x00];
    let result = SafetyInterlockChallenge::from_bytes(&short);
    assert!(result.is_err());
}

#[test]
fn protocol_safety_ack_corrupted_crc_rejected() {
    let mut bytes = SafetyInterlockAck::new(1, 2, 0, 2000, 100000).to_bytes();
    bytes[5] = 0xFF;
    let result = SafetyInterlockAck::from_bytes(&bytes);
    assert!(result.is_err());
}

// ===================================================================
// 5. Edge cases: zero-length, oversized, boundary values
// ===================================================================

#[test]
fn protocol_torque_from_empty_slice_rejected() {
    let result = ProtocolTorqueCommand::from_bytes(&[]);
    assert!(result.is_err());
}

#[test]
fn protocol_telemetry_from_empty_slice_rejected() {
    let result = ProtocolTelemetryReport::from_bytes(&[]);
    assert!(result.is_err());
}

#[test]
fn protocol_capabilities_from_empty_slice_rejected() {
    let result = ProtocolCapabilitiesReport::from_bytes(&[]);
    assert!(result.is_err());
}

#[test]
fn protocol_safety_challenge_from_empty_slice_rejected() {
    let result = SafetyInterlockChallenge::from_bytes(&[]);
    assert!(result.is_err());
}

#[test]
fn protocol_safety_ack_from_empty_slice_rejected() {
    let result = SafetyInterlockAck::from_bytes(&[]);
    assert!(result.is_err());
}

#[test]
fn hid_mod_telemetry_from_empty_slice_returns_none() {
    let result = DeviceTelemetryReport::from_bytes(&[]);
    assert!(result.is_none());
}

#[test]
fn hid_mod_capabilities_from_empty_slice_returns_none() {
    let result = DeviceCapabilitiesReport::from_bytes(&[]);
    assert!(result.is_none());
}

#[test]
fn hid_mod_telemetry_wrong_report_id_returns_none() {
    let mut data = vec![0u8; std::mem::size_of::<DeviceTelemetryReport>()];
    data[0] = 0xFF;
    let result = DeviceTelemetryReport::from_bytes(&data);
    assert!(result.is_none());
}

#[test]
fn hid_mod_capabilities_wrong_report_id_returns_none() {
    let mut data = vec![0u8; std::mem::size_of::<DeviceCapabilitiesReport>()];
    data[0] = 0xFF;
    let result = DeviceCapabilitiesReport::from_bytes(&data);
    assert!(result.is_none());
}

#[test]
fn protocol_torque_command_max_positive_clamp() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = ProtocolTorqueCommand::new(50.0, 0, 0);
    let torque_mnm = cmd.torque_mnm;
    assert_eq!(torque_mnm, 32767); // clamped to i16::MAX
    assert!(cmd.validate_crc());
    Ok(())
}

#[test]
fn protocol_torque_command_max_negative_clamp() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = ProtocolTorqueCommand::new(-50.0, 0, 0);
    let torque_mnm = cmd.torque_mnm;
    assert_eq!(torque_mnm, -32768); // clamped to i16::MIN
    assert!(cmd.validate_crc());
    Ok(())
}

#[test]
fn protocol_torque_command_zero() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = ProtocolTorqueCommand::new(0.0, 0, 0);
    assert!((cmd.torque_nm()).abs() < f32::EPSILON);
    assert!(cmd.validate_crc());
    Ok(())
}

#[test]
fn protocol_torque_command_sequence_wraps() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = ProtocolTorqueCommand::new(1.0, 0, u16::MAX);
    let bytes = cmd.to_bytes();
    let parsed = ProtocolTorqueCommand::from_bytes(&bytes)?;
    let seq = parsed.sequence;
    assert_eq!(seq, u16::MAX);
    Ok(())
}

#[test]
fn protocol_report_size_constraints() {
    assert_eq!(std::mem::size_of::<protocol::TorqueCommand>(), 7);
    assert_eq!(std::mem::size_of::<protocol::DeviceTelemetryReport>(), 13);
    assert_eq!(
        std::mem::size_of::<protocol::DeviceCapabilitiesReport>(),
        9
    );
    assert_eq!(std::mem::size_of::<SafetyInterlockChallenge>(), 12);
    assert_eq!(std::mem::size_of::<SafetyInterlockAck>(), 17);
}

#[test]
fn protocol_telemetry_hands_on_unknown_state() {
    let mut report = ProtocolTelemetryReport::new(0.0, 0.0, 25, 0, false, 0);
    report.hands_on = 255;
    assert_eq!(report.hands_on(), None);
}

#[test]
fn protocol_telemetry_all_fault_flags() {
    let all_faults = 0xFF;
    let report = ProtocolTelemetryReport::new(0.0, 0.0, 80, all_faults, true, 0);
    assert!(report.has_faults());
    assert_eq!(report.faults, 0xFF);
}

// ===================================================================
// 6. Virtual device communication through HidDevice trait
// ===================================================================

#[test]
fn virtual_device_write_within_torque_limit() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_device_id("comm-test")?;
    let mut device = VirtualDevice::new(id, "Comm Test".to_string());

    assert!(device.write_ffb_report(0.0, 0).is_ok());
    assert!(device.write_ffb_report(25.0, 1).is_ok());
    assert!(device.write_ffb_report(-25.0, 2).is_ok());
    Ok(())
}

#[test]
fn virtual_device_write_exceeding_limit_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_device_id("limit-test")?;
    let mut device = VirtualDevice::new(id, "Limit Test".to_string());

    let result = device.write_ffb_report(30.0, 0);
    assert!(result.is_err());
    assert_eq!(result, Err(racing_wheel_engine::RTError::TorqueLimit));
    Ok(())
}

#[test]
fn virtual_device_disconnected_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_device_id("disc-test")?;
    let mut device = VirtualDevice::new(id, "Disc Test".to_string());
    device.disconnect();

    assert!(!device.is_connected());
    let result = device.write_ffb_report(1.0, 0);
    assert_eq!(result, Err(racing_wheel_engine::RTError::DeviceDisconnected));
    assert!(device.read_telemetry().is_none());
    Ok(())
}

#[test]
fn virtual_device_reconnect_restores_operations() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_device_id("reconnect-test")?;
    let mut device = VirtualDevice::new(id, "Reconnect Test".to_string());

    device.disconnect();
    assert!(!device.is_connected());

    device.reconnect();
    assert!(device.is_connected());
    assert!(device.write_ffb_report(5.0, 0).is_ok());
    assert!(device.read_telemetry().is_some());
    Ok(())
}

#[test]
fn virtual_device_health_status_reflects_state() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_device_id("health-test")?;
    let mut device = VirtualDevice::new(id, "Health Test".to_string());

    let health = device.health_status();
    assert_eq!(health.fault_flags, 0);
    assert!(health.temperature_c >= 20);
    assert_eq!(health.communication_errors, 0);

    device.inject_fault(0x04);
    let health = device.health_status();
    assert_eq!(health.fault_flags, 0x04);

    device.clear_faults();
    let health = device.health_status();
    assert_eq!(health.fault_flags, 0);
    Ok(())
}

// ===================================================================
// 7. Quirks detection per vendor
// ===================================================================

#[test]
fn quirks_moza_wheelbase_has_conditional_fix() {
    let quirks = DeviceQuirks::for_device(0x346E, 0x0005);
    assert!(quirks.fix_conditional_direction);
    assert!(quirks.requires_init_handshake);
    assert!(quirks.has_quirks());
}

#[test]
fn quirks_moza_pedals_no_ffb_quirks() {
    let quirks = DeviceQuirks::for_device(0x346E, 0x0003);
    assert!(!quirks.fix_conditional_direction);
    assert!(!quirks.requires_init_handshake);
}

#[test]
fn quirks_fanatec_wheelbase_needs_init() {
    let quirks = DeviceQuirks::for_device(0x0EB7, 0x0024);
    assert!(quirks.requires_init_handshake);
    assert_eq!(quirks.required_b_interval, Some(1));
}

#[test]
fn quirks_unknown_device_has_none() {
    let quirks = DeviceQuirks::for_device(0x1234, 0x5678);
    assert!(!quirks.has_quirks());
}

// ===================================================================
// 8. Vendor protocol dispatch
// ===================================================================

#[test]
fn vendor_protocol_dispatch_fanatec() {
    let handler = vendor::get_vendor_protocol(0x0EB7, 0x0024);
    assert!(handler.is_some());
}

#[test]
fn vendor_protocol_dispatch_moza() {
    let handler = vendor::get_vendor_protocol(0x346E, 0x0004);
    assert!(handler.is_some());
}

#[test]
fn vendor_protocol_dispatch_logitech() {
    let handler = vendor::get_vendor_protocol(0x046D, 0xC294);
    assert!(handler.is_some());
}

#[test]
fn vendor_protocol_dispatch_thrustmaster() {
    let handler = vendor::get_vendor_protocol(0x044F, 0xB66E);
    assert!(handler.is_some());
}

#[test]
fn vendor_protocol_dispatch_unknown_returns_none() {
    let handler = vendor::get_vendor_protocol(0x0000, 0x0000);
    assert!(handler.is_none());
}

#[test]
fn vendor_protocol_with_hid_pid_fallback_uses_generic() {
    let handler =
        vendor::get_vendor_protocol_with_hid_pid_fallback(0x9999, 0x0001, true);
    assert!(handler.is_some());
}

#[test]
fn vendor_protocol_with_hid_pid_fallback_no_pid_returns_none() {
    let handler =
        vendor::get_vendor_protocol_with_hid_pid_fallback(0x9999, 0x0001, false);
    assert!(handler.is_none());
}

// ===================================================================
// 9. VirtualHidPort async operations
// ===================================================================

#[tokio::test]
async fn virtual_hid_port_add_list_remove() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();
    let id = make_device_id("port-test")?;
    let device = VirtualDevice::new(id.clone(), "Port Test".to_string());
    port.add_device(device)?;

    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].id.as_str(), "port-test");

    port.remove_device(&id)?;
    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), 0);
    Ok(())
}

#[tokio::test]
async fn virtual_hid_port_open_nonexistent_fails() -> Result<(), Box<dyn std::error::Error>> {
    let port = VirtualHidPort::new();
    let id = make_device_id("does-not-exist")?;
    let result = port.open_device(&id).await;
    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn virtual_hid_port_open_shares_state() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();
    let id = make_device_id("shared-state")?;
    let device = VirtualDevice::new(id.clone(), "Shared State".to_string());
    port.add_device(device)?;

    let mut opened = port.open_device(&id).await?;
    assert!(opened.is_connected());
    assert!(opened.write_ffb_report(5.0, 1).is_ok());
    assert!(opened.read_telemetry().is_some());
    Ok(())
}
