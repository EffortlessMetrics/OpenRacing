//! Snapshot tests for engine safety states, device reports, error messages,
//! and configuration display.
//!
//! Note: Types containing `Instant` fields (SafetyState, EngineStats, etc.)
//! are not snapshot-testable directly; we test the deterministic types.

use racing_wheel_engine::{
    DeviceCapabilitiesReport, DeviceInputs, DeviceTelemetryReport, EngineConfig, FFBMode, RTError,
    RTSetup,
};
use racing_wheel_engine::ports::{NormalizedTelemetry, TelemetryFlags};
use racing_wheel_engine::safety::{ButtonCombo, ConsentRequirements, FaultType};

// ---------------------------------------------------------------------------
// Safety state snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshot_safety_state_safe_torque() {
    // SafetyState contains Instant, so snapshot the deterministic enum via Debug
    insta::assert_debug_snapshot!("safety_safe_torque", racing_wheel_engine::safety::SafetyState::SafeTorque);
}

#[test]
fn snapshot_fault_type_usb_stall_display() {
    let output = format!("{}", FaultType::UsbStall);
    insta::assert_snapshot!("fault_type_usb_stall", output);
}

#[test]
fn snapshot_fault_type_thermal_limit_display() {
    let output = format!("{}", FaultType::ThermalLimit);
    insta::assert_snapshot!("fault_type_thermal_limit", output);
}

#[test]
fn snapshot_fault_type_overcurrent_display() {
    let output = format!("{}", FaultType::Overcurrent);
    insta::assert_snapshot!("fault_type_overcurrent", output);
}

#[test]
fn snapshot_consent_full() {
    let consent = ConsentRequirements {
        max_torque_nm: 25.0,
        warnings: vec![
            "High torque mode can cause injury".to_string(),
            "Ensure wheel is firmly mounted".to_string(),
        ],
        disclaimers: vec!["Use at your own risk".to_string()],
        requires_explicit_consent: true,
    };
    insta::assert_json_snapshot!("consent_full", consent);
}

#[test]
fn snapshot_button_combo_debug_all() {
    let combos = vec![
        ButtonCombo::BothClutchPaddles,
        ButtonCombo::CustomSequence(7),
        ButtonCombo::CustomSequence(0),
    ];
    insta::assert_debug_snapshot!("button_combo_all_variants", combos);
}

// ---------------------------------------------------------------------------
// Device report snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshot_telemetry_report_normal() {
    let report = DeviceTelemetryReport::new(45.5, 2.5, 65, 0x00, true, 100);
    insta::assert_debug_snapshot!("telemetry_report_normal", report);
}

#[test]
fn snapshot_telemetry_report_faulted() {
    let report = DeviceTelemetryReport::new(-90.0, -1.0, 95, 0x05, false, 999);
    insta::assert_debug_snapshot!("telemetry_report_faulted", report);
}

#[test]
fn snapshot_capabilities_report_full() {
    let report = DeviceCapabilitiesReport::new(
        true,  // supports_pid
        true,  // supports_raw_torque_1khz
        true,  // supports_health_stream
        true,  // supports_led_bus
        800,   // max_torque_cnm (8 Nm)
        2048,  // encoder_cpr
        1000,  // min_report_period_us
    );
    insta::assert_debug_snapshot!("capabilities_report_full", report);
}

#[test]
fn snapshot_capabilities_report_minimal() {
    let report = DeviceCapabilitiesReport::new(false, false, false, false, 0, 1024, 5000);
    insta::assert_debug_snapshot!("capabilities_report_minimal", report);
}

#[test]
fn snapshot_device_inputs_default() {
    insta::assert_debug_snapshot!("device_inputs_default", DeviceInputs::default());
}

#[test]
fn snapshot_device_inputs_populated() {
    let mut inputs = DeviceInputs::default();
    inputs.tick = 42;
    inputs.steering = Some(32768);
    inputs.throttle = Some(65535);
    inputs.brake = Some(0);
    inputs.clutch_left = Some(16000);
    inputs.clutch_right = Some(16000);
    inputs.hat = 3;
    inputs.buttons[0] = 0xFF;
    inputs.rotaries[0] = 127;
    insta::assert_debug_snapshot!("device_inputs_populated", inputs);
}

#[test]
fn snapshot_normalized_telemetry_typical() {
    let telemetry = NormalizedTelemetry {
        ffb_scalar: 0.75,
        rpm: 7200.0,
        speed_ms: 55.5,
        slip_ratio: 0.12,
        gear: 4,
        flags: TelemetryFlags {
            yellow_flag: false,
            red_flag: false,
            blue_flag: true,
            checkered_flag: false,
            pit_limiter: false,
            drs_enabled: true,
            ers_available: true,
            in_pit: false,
        },
        car_id: Some("gt3_992".to_string()),
        track_id: Some("spa".to_string()),
        timestamp: std::time::Instant::now(),
    };
    // Use json_snapshot for the serializable fields (timestamp is skipped by serde)
    insta::assert_json_snapshot!("normalized_telemetry_typical", telemetry);
}

#[test]
fn snapshot_telemetry_flags_default() {
    insta::assert_debug_snapshot!("telemetry_flags_default", TelemetryFlags::default());
}

#[test]
fn snapshot_telemetry_flags_race_condition() {
    let flags = TelemetryFlags {
        yellow_flag: true,
        red_flag: false,
        blue_flag: false,
        checkered_flag: false,
        pit_limiter: true,
        drs_enabled: false,
        ers_available: false,
        in_pit: true,
    };
    insta::assert_json_snapshot!("telemetry_flags_race_condition", flags);
}

// ---------------------------------------------------------------------------
// Error message snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshot_rt_error_all_display() {
    let errors = [
        RTError::DeviceDisconnected,
        RTError::TorqueLimit,
        RTError::PipelineFault,
        RTError::TimingViolation,
        RTError::RTSetupFailed,
        RTError::InvalidConfig,
        RTError::SafetyInterlock,
        RTError::BufferOverflow,
        RTError::DeadlineMissed,
        RTError::ResourceUnavailable,
    ];
    let output: Vec<String> = errors.iter().map(|e| format!("{e}")).collect();
    insta::assert_debug_snapshot!("rt_error_all_display", output);
}

#[test]
fn snapshot_rt_error_debug() {
    insta::assert_debug_snapshot!("rt_error_debug_device_disconnected", RTError::DeviceDisconnected);
}

#[test]
fn snapshot_rt_error_debug_safety_interlock() {
    insta::assert_debug_snapshot!("rt_error_debug_safety_interlock", RTError::SafetyInterlock);
}

// ---------------------------------------------------------------------------
// Configuration display snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshot_ffb_mode_display_all() {
    let modes = [FFBMode::PidPassthrough, FFBMode::RawTorque, FFBMode::TelemetrySynth];
    let output: Vec<String> = modes.iter().map(|m| format!("{m}")).collect();
    insta::assert_debug_snapshot!("ffb_mode_display_all", output);
}

#[test]
fn snapshot_engine_config_debug() -> Result<(), Box<dyn std::error::Error>> {
    let device_id = "moza-r9-001".parse()?;
    let config = EngineConfig {
        device_id,
        mode: FFBMode::RawTorque,
        max_safe_torque_nm: 8.0,
        max_high_torque_nm: 20.0,
        enable_blackbox: false,
        rt_setup: RTSetup::default(),
    };
    insta::assert_debug_snapshot!("engine_config_typical", config);
    Ok(())
}

#[test]
fn snapshot_engine_config_blackbox_enabled() -> Result<(), Box<dyn std::error::Error>> {
    let device_id = "fanatec-dd-pro".parse()?;
    let config = EngineConfig {
        device_id,
        mode: FFBMode::TelemetrySynth,
        max_safe_torque_nm: 5.0,
        max_high_torque_nm: 12.0,
        enable_blackbox: true,
        rt_setup: RTSetup {
            high_priority: true,
            lock_memory: true,
            disable_power_throttling: false,
            cpu_affinity: Some(0x0C),
        },
    };
    insta::assert_debug_snapshot!("engine_config_blackbox", config);
    Ok(())
}

#[test]
fn snapshot_rt_setup_default() {
    insta::assert_debug_snapshot!("rt_setup_default", RTSetup::default());
}
