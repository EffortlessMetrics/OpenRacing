//! Tests for game integration layer
//!
//! Covers game detection and adapter selection, telemetry data flow through
//! normalization to FFB calculation, game disconnection handling, and
//! multi-game switching.

use racing_wheel_engine::{
    CapabilityNegotiator, GameCompatibility, GameInput, HidDevice, HidPort,
    ModeSelectionPolicy, NormalizedTelemetry, TelemetryFlags, TelemetryStatistics,
    VirtualDevice, VirtualHidPort,
};
use racing_wheel_engine::ports::{ConfigChange, ConfigurationStatus, ProfileContext};
use racing_wheel_engine::rt::FFBMode;
use racing_wheel_schemas::prelude::*;
use std::time::{Duration, Instant};

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

fn make_telemetry(ffb: f32, rpm: f32, speed: f32, slip: f32, gear: i8) -> NormalizedTelemetry {
    NormalizedTelemetry {
        ffb_scalar: ffb,
        rpm,
        speed_ms: speed,
        slip_ratio: slip,
        gear,
        flags: TelemetryFlags::default(),
        car_id: None,
        track_id: None,
        timestamp: Instant::now(),
    }
}

fn make_game_input(ffb: f32, telemetry: Option<NormalizedTelemetry>) -> GameInput {
    GameInput {
        ffb_scalar: ffb,
        telemetry,
        timestamp: Instant::now(),
    }
}

// ===================================================================
// 1. Game detection and adapter selection
// ===================================================================

#[test]
fn mode_selection_sim_with_robust_ffb_prefers_raw_torque() -> Result<(), Box<dyn std::error::Error>>
{
    let caps = test_caps(true, true, 25.0)?;
    let game = GameCompatibility {
        game_id: "iracing".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: FFBMode::RawTorque,
    };

    let mode = ModeSelectionPolicy::select_mode(&caps, Some(&game));
    assert_eq!(mode, FFBMode::RawTorque);
    Ok(())
}

#[test]
fn mode_selection_arcade_game_prefers_telemetry_synth() -> Result<(), Box<dyn std::error::Error>> {
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
fn mode_selection_game_no_telemetry_no_robust_falls_to_raw() -> Result<(), Box<dyn std::error::Error>>
{
    let caps = test_caps(true, true, 25.0)?;
    let game = GameCompatibility {
        game_id: "old-racer".to_string(),
        supports_robust_ffb: false,
        supports_telemetry: false,
        preferred_mode: FFBMode::PidPassthrough,
    };

    // Device supports raw torque, game doesn't support either, falls to raw torque default
    let mode = ModeSelectionPolicy::select_mode(&caps, Some(&game));
    assert_eq!(mode, FFBMode::RawTorque);
    Ok(())
}

#[test]
fn mode_selection_commodity_wheel_pid_only() -> Result<(), Box<dyn std::error::Error>> {
    let caps = test_caps(true, false, 5.0)?;
    let game = GameCompatibility {
        game_id: "any-game".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: FFBMode::RawTorque,
    };

    let mode = ModeSelectionPolicy::select_mode(&caps, Some(&game));
    assert_eq!(mode, FFBMode::PidPassthrough);
    Ok(())
}

#[test]
fn mode_selection_no_game_info_defaults_to_device_best() -> Result<(), Box<dyn std::error::Error>> {
    let caps = test_caps(true, true, 25.0)?;
    let mode = ModeSelectionPolicy::select_mode(&caps, None);
    assert_eq!(mode, FFBMode::RawTorque);

    let caps_pid = test_caps(true, false, 10.0)?;
    let mode = ModeSelectionPolicy::select_mode(&caps_pid, None);
    assert_eq!(mode, FFBMode::PidPassthrough);

    let caps_basic = test_caps(false, false, 5.0)?;
    let mode = ModeSelectionPolicy::select_mode(&caps_basic, None);
    assert_eq!(mode, FFBMode::TelemetrySynth);
    Ok(())
}

#[test]
fn negotiation_with_game_produces_summary() -> Result<(), Box<dyn std::error::Error>> {
    let caps = test_caps(true, true, 25.0)?;
    let game = GameCompatibility {
        game_id: "assetto-corsa".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: FFBMode::RawTorque,
    };

    let result = CapabilityNegotiator::negotiate_capabilities(&caps, Some(&game));
    assert_eq!(result.mode, FFBMode::RawTorque);

    let summary = result.summary();
    assert!(summary.contains("Raw Torque"));
    assert!(summary.contains("1000Hz"));
    Ok(())
}

#[test]
fn negotiation_telemetry_synth_warns() -> Result<(), Box<dyn std::error::Error>> {
    let caps = test_caps(false, false, 5.0)?;
    let result = CapabilityNegotiator::negotiate_capabilities(&caps, None);

    assert_eq!(result.mode, FFBMode::TelemetrySynth);
    assert!(!result.warnings.is_empty());
    let has_quality_warning = result
        .warnings
        .iter()
        .any(|w| w.contains("telemetry synthesis") || w.contains("reduced"));
    assert!(has_quality_warning);
    Ok(())
}

// ===================================================================
// 2. Telemetry data flow: game → normalization → FFB calculation
// ===================================================================

#[test]
fn normalized_telemetry_defaults_are_safe() {
    let flags = TelemetryFlags::default();
    assert!(!flags.yellow_flag);
    assert!(!flags.red_flag);
    assert!(!flags.blue_flag);
    assert!(!flags.checkered_flag);
    assert!(!flags.pit_limiter);
    assert!(!flags.drs_enabled);
    assert!(!flags.ers_available);
    assert!(!flags.in_pit);
}

#[test]
fn normalized_telemetry_carries_car_and_track() {
    let mut tel = make_telemetry(0.5, 6000.0, 50.0, 0.1, 3);
    tel.car_id = Some("porsche_911_gt3".to_string());
    tel.track_id = Some("spa_francorchamps".to_string());

    assert_eq!(tel.car_id.as_deref(), Some("porsche_911_gt3"));
    assert_eq!(tel.track_id.as_deref(), Some("spa_francorchamps"));
}

#[test]
fn game_input_with_telemetry_carries_ffb_scalar() {
    let tel = make_telemetry(0.75, 7000.0, 60.0, 0.05, 4);
    let input = make_game_input(0.75, Some(tel));

    assert!((input.ffb_scalar - 0.75).abs() < f32::EPSILON);
    assert!(input.telemetry.is_some());
    let t = input.telemetry.as_ref().unwrap();
    assert!((t.rpm - 7000.0).abs() < f32::EPSILON);
}

#[test]
fn game_input_without_telemetry_still_has_ffb() {
    let input = make_game_input(0.5, None);

    assert!((input.ffb_scalar - 0.5).abs() < f32::EPSILON);
    assert!(input.telemetry.is_none());
}

#[test]
fn telemetry_ffb_scalar_range_boundaries() {
    // Valid range -1.0 to 1.0
    let tel_max = make_telemetry(1.0, 8000.0, 80.0, 0.0, 5);
    assert!((tel_max.ffb_scalar - 1.0).abs() < f32::EPSILON);

    let tel_min = make_telemetry(-1.0, 1000.0, 10.0, 0.0, 1);
    assert!((tel_min.ffb_scalar - (-1.0)).abs() < f32::EPSILON);

    let tel_zero = make_telemetry(0.0, 0.0, 0.0, 0.0, 0);
    assert!(tel_zero.ffb_scalar.abs() < f32::EPSILON);
}

#[test]
fn telemetry_flags_can_combine() {
    let flags = TelemetryFlags {
        yellow_flag: true,
        pit_limiter: true,
        drs_enabled: true,
        ..Default::default()
    };

    assert!(flags.yellow_flag);
    assert!(flags.pit_limiter);
    assert!(flags.drs_enabled);
    assert!(!flags.red_flag);
    assert!(!flags.blue_flag);
}

#[test]
fn telemetry_gear_covers_all_states() {
    let reverse = make_telemetry(0.0, 2000.0, 5.0, 0.0, -1);
    assert_eq!(reverse.gear, -1);

    let neutral = make_telemetry(0.0, 800.0, 0.0, 0.0, 0);
    assert_eq!(neutral.gear, 0);

    let top_gear = make_telemetry(0.8, 8500.0, 80.0, 0.0, 7);
    assert_eq!(top_gear.gear, 7);
}

#[test]
fn telemetry_slip_ratio_bounds() {
    let no_slip = make_telemetry(0.5, 6000.0, 50.0, 0.0, 3);
    assert!(no_slip.slip_ratio.abs() < f32::EPSILON);

    let full_slip = make_telemetry(0.5, 6000.0, 50.0, 1.0, 3);
    assert!((full_slip.slip_ratio - 1.0).abs() < f32::EPSILON);
}

#[test]
fn telemetry_to_ffb_flow_virtual_device() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_device_id("ffb-flow-test")?;
    let mut device = VirtualDevice::new(id, "FFB Flow Test".to_string());

    // Simulate telemetry data arriving and being converted to torque
    let telemetry = make_telemetry(0.5, 6000.0, 45.0, 0.1, 3);
    let torque_nm = telemetry.ffb_scalar * device.capabilities().max_torque.value();

    assert!(device.write_ffb_report(torque_nm, 1).is_ok());

    device.simulate_physics(Duration::from_millis(10));
    let device_tel = device.read_telemetry().ok_or("no telemetry")?;
    assert!(device_tel.wheel_angle_deg.is_finite());
    assert!(device_tel.wheel_speed_rad_s.is_finite());
    Ok(())
}

#[test]
fn telemetry_statistics_default_is_zeroed() {
    let stats = TelemetryStatistics::default();
    assert_eq!(stats.packets_received, 0);
    assert_eq!(stats.packets_dropped, 0);
    assert!(stats.last_packet_time.is_none());
    assert!(stats.average_rate_hz.abs() < f32::EPSILON);
    assert_eq!(stats.connection_errors, 0);
}

// ===================================================================
// 3. Game disconnection handling
// ===================================================================

#[test]
fn device_disconnection_stops_ffb() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_device_id("disconnect-ffb")?;
    let mut device = VirtualDevice::new(id, "Disconnect FFB Test".to_string());

    // Normal operation
    assert!(device.write_ffb_report(10.0, 1).is_ok());
    assert!(device.read_telemetry().is_some());

    // Disconnect
    device.disconnect();
    assert!(!device.is_connected());
    assert_eq!(
        device.write_ffb_report(10.0, 2),
        Err(racing_wheel_engine::RTError::DeviceDisconnected)
    );
    assert!(device.read_telemetry().is_none());
    Ok(())
}

#[test]
fn device_reconnection_restores_communication() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_device_id("reconnect-comm")?;
    let mut device = VirtualDevice::new(id, "Reconnect Comm Test".to_string());

    device.disconnect();
    assert!(device.write_ffb_report(5.0, 1).is_err());

    device.reconnect();
    assert!(device.is_connected());
    assert!(device.write_ffb_report(5.0, 2).is_ok());
    assert!(device.read_telemetry().is_some());
    Ok(())
}

#[test]
fn multiple_disconnect_reconnect_cycles() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_device_id("cycle-test")?;
    let mut device = VirtualDevice::new(id, "Cycle Test".to_string());

    for i in 0..5u16 {
        device.disconnect();
        assert!(!device.is_connected());
        assert!(device.write_ffb_report(1.0, i * 2).is_err());

        device.reconnect();
        assert!(device.is_connected());
        assert!(device.write_ffb_report(1.0, i * 2 + 1).is_ok());
    }
    Ok(())
}

#[test]
fn fault_injection_during_operation() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_device_id("fault-op")?;
    let mut device = VirtualDevice::new(id, "Fault Operation".to_string());

    // Normal operation
    assert!(device.write_ffb_report(5.0, 1).is_ok());
    let tel = device.read_telemetry().ok_or("no telemetry")?;
    assert_eq!(tel.fault_flags, 0);

    // Inject thermal fault — device still operates but reports fault
    device.inject_fault(0x04);
    assert!(device.write_ffb_report(5.0, 2).is_ok());
    let tel = device.read_telemetry().ok_or("no telemetry")?;
    assert_eq!(tel.fault_flags, 0x04);

    // Inject multiple faults
    device.inject_fault(0x01); // USB fault
    device.inject_fault(0x08); // overcurrent
    let tel = device.read_telemetry().ok_or("no telemetry")?;
    assert_eq!(tel.fault_flags, 0x04 | 0x01 | 0x08);

    // Clear and verify clean
    device.clear_faults();
    let tel = device.read_telemetry().ok_or("no telemetry")?;
    assert_eq!(tel.fault_flags, 0);
    Ok(())
}

// ===================================================================
// 4. Multi-game switching
// ===================================================================

#[test]
fn profile_context_game_switching() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_device_id("game-switch")?;

    // Simulate switching between games
    let ctx_iracing = ProfileContext::new(id.clone())
        .with_game("iracing".to_string())
        .with_car("porsche_911_gt3".to_string())
        .with_track("spa".to_string());

    assert_eq!(ctx_iracing.game, Some("iracing".to_string()));
    assert_eq!(ctx_iracing.car, Some("porsche_911_gt3".to_string()));
    assert_eq!(ctx_iracing.track, Some("spa".to_string()));

    // Switch to different game
    let ctx_acc = ProfileContext::new(id.clone())
        .with_game("acc".to_string())
        .with_car("lamborghini_huracan_gt3".to_string())
        .with_track("monza".to_string());

    assert_eq!(ctx_acc.game, Some("acc".to_string()));
    assert_eq!(ctx_acc.car, Some("lamborghini_huracan_gt3".to_string()));
    assert_eq!(ctx_acc.track, Some("monza".to_string()));

    // Context should use same device
    assert_eq!(ctx_iracing.device_id, ctx_acc.device_id);
    Ok(())
}

#[test]
fn negotiation_changes_with_game_switch() -> Result<(), Box<dyn std::error::Error>> {
    let caps = test_caps(true, true, 25.0)?;

    // Game 1: sim with robust FFB
    let game1 = GameCompatibility {
        game_id: "iracing".to_string(),
        supports_robust_ffb: true,
        supports_telemetry: true,
        preferred_mode: FFBMode::RawTorque,
    };
    let result1 = CapabilityNegotiator::negotiate_capabilities(&caps, Some(&game1));
    assert_eq!(result1.mode, FFBMode::RawTorque);
    assert!((result1.update_rate_hz - 1000.0).abs() < f32::EPSILON);

    // Game 2: arcade with telemetry only
    let game2 = GameCompatibility {
        game_id: "nfs-heat".to_string(),
        supports_robust_ffb: false,
        supports_telemetry: true,
        preferred_mode: FFBMode::TelemetrySynth,
    };
    let result2 = CapabilityNegotiator::negotiate_capabilities(&caps, Some(&game2));
    assert_eq!(result2.mode, FFBMode::TelemetrySynth);

    // Game 3: no game info (standalone device test)
    let result3 = CapabilityNegotiator::negotiate_capabilities(&caps, None);
    assert_eq!(result3.mode, FFBMode::RawTorque);
    Ok(())
}

#[test]
fn update_rate_matches_mode() {
    assert!((ModeSelectionPolicy::get_update_rate_hz(FFBMode::RawTorque) - 1000.0).abs() < f32::EPSILON);
    assert!((ModeSelectionPolicy::get_update_rate_hz(FFBMode::PidPassthrough) - 60.0).abs() < f32::EPSILON);
    assert!((ModeSelectionPolicy::get_update_rate_hz(FFBMode::TelemetrySynth) - 60.0).abs() < f32::EPSILON);
}

#[test]
fn configuration_status_validation() {
    let status = ConfigurationStatus {
        is_valid: true,
        game_version: Some("1.8.0".to_string()),
        telemetry_enabled: true,
        expected_config_changes: vec![ConfigChange {
            file_path: std::path::PathBuf::from("config/telemetry.ini"),
            section: Some("Output".to_string()),
            key: "enabled".to_string(),
            expected_value: "1".to_string(),
            current_value: Some("1".to_string()),
        }],
        issues: Vec::new(),
    };

    assert!(status.is_valid);
    assert!(status.telemetry_enabled);
    assert!(status.issues.is_empty());
    assert_eq!(status.expected_config_changes.len(), 1);
    assert_eq!(status.game_version, Some("1.8.0".to_string()));
}

#[test]
fn configuration_status_with_issues() {
    let status = ConfigurationStatus {
        is_valid: false,
        game_version: None,
        telemetry_enabled: false,
        expected_config_changes: Vec::new(),
        issues: vec![
            "Game not installed".to_string(),
            "Telemetry plugin missing".to_string(),
        ],
    };

    assert!(!status.is_valid);
    assert!(!status.telemetry_enabled);
    assert_eq!(status.issues.len(), 2);
}

// ===================================================================
// 5. End-to-end telemetry flow with virtual device
// ===================================================================

#[tokio::test]
async fn e2e_telemetry_through_virtual_device() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();
    let id = make_device_id("e2e-test")?;
    let device = VirtualDevice::new(id.clone(), "E2E Test Device".to_string());
    port.add_device(device)?;

    let mut opened = port.open_device(&id).await?;
    assert!(opened.is_connected());

    // Simulate a game providing telemetry at 60Hz for a short burst
    let game_tel = make_telemetry(0.6, 5500.0, 42.0, 0.05, 3);
    let max_torque = opened.capabilities().max_torque.value();
    let torque_nm = game_tel.ffb_scalar * max_torque;

    for seq in 0..10u16 {
        assert!(opened.write_ffb_report(torque_nm, seq).is_ok());
    }

    let device_tel = opened.read_telemetry().ok_or("no telemetry")?;
    assert!(device_tel.temperature_c >= 20);
    assert!(device_tel.temperature_c <= 100);
    Ok(())
}

#[tokio::test]
async fn e2e_multiple_devices_different_games() -> Result<(), Box<dyn std::error::Error>> {
    let mut port = VirtualHidPort::new();

    let id1 = make_device_id("device-1")?;
    let id2 = make_device_id("device-2")?;
    port.add_device(VirtualDevice::new(id1.clone(), "Wheel 1".to_string()))?;
    port.add_device(VirtualDevice::new(id2.clone(), "Wheel 2".to_string()))?;

    let devices = port.list_devices().await?;
    assert_eq!(devices.len(), 2);

    // Each device can be opened and used independently
    let mut dev1 = port.open_device(&id1).await?;
    let mut dev2 = port.open_device(&id2).await?;

    assert!(dev1.write_ffb_report(5.0, 0).is_ok());
    assert!(dev2.write_ffb_report(10.0, 0).is_ok());

    assert!(dev1.read_telemetry().is_some());
    assert!(dev2.read_telemetry().is_some());
    Ok(())
}

#[test]
fn profile_context_session_type_tracking() -> Result<(), Box<dyn std::error::Error>> {
    let id = make_device_id("session-type")?;

    let practice = ProfileContext::new(id.clone())
        .with_game("iracing".to_string())
        .with_session_type("practice".to_string());
    assert_eq!(practice.session_type, Some("practice".to_string()));

    let race = ProfileContext::new(id.clone())
        .with_game("iracing".to_string())
        .with_session_type("race".to_string());
    assert_eq!(race.session_type, Some("race".to_string()));
    Ok(())
}

#[test]
fn game_input_timestamp_is_recent() {
    let before = Instant::now();
    let input = make_game_input(0.5, None);
    let after = Instant::now();

    assert!(input.timestamp >= before);
    assert!(input.timestamp <= after);
}

#[test]
fn negotiation_result_is_optimal_only_for_raw_torque_no_warnings(
) -> Result<(), Box<dyn std::error::Error>> {
    let caps = test_caps(true, true, 25.0)?;
    let result = CapabilityNegotiator::negotiate_capabilities(&caps, None);
    assert!(result.is_optimal());

    // With a warning it's not optimal
    let caps_limited = test_caps(false, false, 5.0)?;
    let result = CapabilityNegotiator::negotiate_capabilities(&caps_limited, None);
    assert!(!result.is_optimal());
    Ok(())
}
