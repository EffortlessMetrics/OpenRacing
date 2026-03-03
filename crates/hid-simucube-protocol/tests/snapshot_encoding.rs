//! Extended snapshot tests for Simucube wire-format encoding.
//!
//! These tests supplement `snapshot_tests.rs` by covering VID/PID constants,
//! torque encoding at boundary values, device identification, wheel
//! capabilities, and device status parsing that would detect wire-format
//! regressions.

use hid_simucube_protocol as sc;
use insta::{assert_debug_snapshot, assert_snapshot};

// ── Individual VID/PID constants ─────────────────────────────────────────────

#[test]
fn snapshot_vendor_id() {
    assert_snapshot!(format!("{:#06X}", sc::VENDOR_ID));
}

#[test]
fn snapshot_product_id_sport() {
    assert_snapshot!(format!("{:#06X}", sc::PRODUCT_ID_SPORT));
}

#[test]
fn snapshot_product_id_pro() {
    assert_snapshot!(format!("{:#06X}", sc::PRODUCT_ID_PRO));
}

#[test]
fn snapshot_product_id_ultimate() {
    assert_snapshot!(format!("{:#06X}", sc::PRODUCT_ID_ULTIMATE));
}

#[test]
fn snapshot_simucube_1_pid() {
    assert_snapshot!(format!("{:#06X}", sc::SIMUCUBE_1_PID));
}

#[test]
fn snapshot_active_pedal_pid() {
    assert_snapshot!(format!("{:#06X}", sc::SIMUCUBE_ACTIVE_PEDAL_PID));
}

#[test]
fn snapshot_bootloader_pids() {
    assert_snapshot!(format!(
        "sc1_boot={:#06X}, sc2_boot={:#06X}",
        sc::SIMUCUBE_1_BOOTLOADER_PID,
        sc::SIMUCUBE_2_BOOTLOADER_PID,
    ));
}

// ── is_simucube_device ───────────────────────────────────────────────────────

#[test]
fn snapshot_is_simucube_device_known_vid() {
    assert_snapshot!(format!(
        "known_vid={}, wrong_vid={}",
        sc::is_simucube_device(sc::SIMUCUBE_VENDOR_ID),
        sc::is_simucube_device(0x0000),
    ));
}

// ── SimucubeModel from PID ───────────────────────────────────────────────────

#[test]
fn snapshot_model_from_all_known_pids() {
    let entries = [
        ("SC1", sc::SIMUCUBE_1_PID),
        ("Sport", sc::SIMUCUBE_2_SPORT_PID),
        ("Pro", sc::SIMUCUBE_2_PRO_PID),
        ("Ultimate", sc::SIMUCUBE_2_ULTIMATE_PID),
        ("ActivePedal", sc::SIMUCUBE_ACTIVE_PEDAL_PID),
        ("WirelessWheel", sc::SIMUCUBE_WIRELESS_WHEEL_PID),
    ];
    let summary: Vec<String> = entries
        .iter()
        .map(|(label, pid)| {
            let model = sc::SimucubeModel::from_product_id(*pid);
            format!(
                "{}: model={:?}, torque={:.1}, name={}",
                label,
                model,
                model.max_torque_nm(),
                model.display_name(),
            )
        })
        .collect();
    assert_snapshot!(summary.join("\n"));
}

#[test]
fn snapshot_model_from_unknown_pid() {
    let model = sc::SimucubeModel::from_product_id(0xFFFF);
    assert_snapshot!(format!(
        "model={:?}, torque={:.1}, name={}",
        model,
        model.max_torque_nm(),
        model.display_name(),
    ));
}

// ── simucube_model_from_info (VID + PID) ─────────────────────────────────────

#[test]
fn snapshot_model_from_info_wrong_vid() {
    let model = sc::simucube_model_from_info(0x1234, sc::SIMUCUBE_2_PRO_PID);
    assert_snapshot!(format!("{model:?}"));
}

// ── Torque encoding boundary values ──────────────────────────────────────────

#[test]
fn test_snapshot_output_half_positive_torque() -> Result<(), String> {
    let report = sc::SimucubeOutputReport::new(0).with_torque(sc::MAX_TORQUE_NM * 0.5);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

#[test]
fn test_snapshot_output_half_negative_torque() -> Result<(), String> {
    let report = sc::SimucubeOutputReport::new(0).with_torque(-sc::MAX_TORQUE_NM * 0.5);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

#[test]
fn test_snapshot_output_clamp_above_max() -> Result<(), String> {
    let report = sc::SimucubeOutputReport::new(0).with_torque(100.0);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

#[test]
fn test_snapshot_output_clamp_below_neg_max() -> Result<(), String> {
    let report = sc::SimucubeOutputReport::new(0).with_torque(-100.0);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

#[test]
fn test_snapshot_output_tiny_positive_torque() -> Result<(), String> {
    let report = sc::SimucubeOutputReport::new(0).with_torque(0.01);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

// ── Sequence number in output report ─────────────────────────────────────────

#[test]
fn test_snapshot_output_with_sequence_42() -> Result<(), String> {
    let report = sc::SimucubeOutputReport::new(42).with_torque(5.0);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

#[test]
fn test_snapshot_output_max_sequence() -> Result<(), String> {
    let report = sc::SimucubeOutputReport::new(u16::MAX).with_torque(0.0);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

// ── Effect types in output report ────────────────────────────────────────────

#[test]
fn test_snapshot_output_with_sine_effect() -> Result<(), String> {
    let report = sc::SimucubeOutputReport::new(0)
        .with_torque(0.0)
        .with_effect(sc::EffectType::Sine, 1000);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

#[test]
fn test_snapshot_output_with_friction_effect() -> Result<(), String> {
    let report = sc::SimucubeOutputReport::new(0)
        .with_torque(0.0)
        .with_effect(sc::EffectType::Friction, 300);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

#[test]
fn test_snapshot_output_with_constant_effect() -> Result<(), String> {
    let report = sc::SimucubeOutputReport::new(1)
        .with_torque(10.0)
        .with_effect(sc::EffectType::Constant, 0);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

// ── WheelCapabilities per model ──────────────────────────────────────────────

#[test]
fn snapshot_wheel_capabilities_sport() {
    let caps = sc::WheelCapabilities::for_model(sc::WheelModel::Simucube2Sport);
    assert_debug_snapshot!(format!(
        "max_torque={:.1}, encoder_bits={}, wireless={}, active_pedal={}, max_rpm={}",
        caps.max_torque_nm,
        caps.encoder_resolution_bits,
        caps.supports_wireless,
        caps.supports_active_pedal,
        caps.max_speed_rpm,
    ));
}

#[test]
fn snapshot_wheel_capabilities_pro() {
    let caps = sc::WheelCapabilities::for_model(sc::WheelModel::Simucube2Pro);
    assert_debug_snapshot!(format!(
        "max_torque={:.1}, encoder_bits={}, wireless={}, active_pedal={}, max_rpm={}",
        caps.max_torque_nm,
        caps.encoder_resolution_bits,
        caps.supports_wireless,
        caps.supports_active_pedal,
        caps.max_speed_rpm,
    ));
}

#[test]
fn snapshot_wheel_capabilities_ultimate() {
    let caps = sc::WheelCapabilities::for_model(sc::WheelModel::Simucube2Ultimate);
    assert_debug_snapshot!(format!(
        "max_torque={:.1}, encoder_bits={}, wireless={}, active_pedal={}, max_rpm={}",
        caps.max_torque_nm,
        caps.encoder_resolution_bits,
        caps.supports_wireless,
        caps.supports_active_pedal,
        caps.max_speed_rpm,
    ));
}

#[test]
fn snapshot_wheel_capabilities_active_pedal() {
    let caps = sc::WheelCapabilities::for_model(sc::WheelModel::SimucubeActivePedal);
    assert_debug_snapshot!(format!(
        "max_torque={:.1}, encoder_bits={}, wireless={}, active_pedal={}, max_rpm={}",
        caps.max_torque_nm,
        caps.encoder_resolution_bits,
        caps.supports_wireless,
        caps.supports_active_pedal,
        caps.max_speed_rpm,
    ));
}

#[test]
fn snapshot_wheel_capabilities_unknown() {
    let caps = sc::WheelCapabilities::for_model(sc::WheelModel::Unknown);
    assert_debug_snapshot!(format!(
        "max_torque={:.1}, encoder_bits={}, wireless={}, active_pedal={}, max_rpm={}",
        caps.max_torque_nm,
        caps.encoder_resolution_bits,
        caps.supports_wireless,
        caps.supports_active_pedal,
        caps.max_speed_rpm,
    ));
}

// ── DeviceStatus from flags ──────────────────────────────────────────────────

#[test]
fn snapshot_device_status_all_flag_combos() {
    let flag_combos: &[(u8, &str)] = &[
        (0x00, "disconnected"),
        (0x01, "connected_only"),
        (0x03, "connected+enabled"),
        (0x05, "connected+calibrating"),
        (0x09, "connected+error_no_enable"),
        (0x0B, "connected+enabled+error"),
    ];
    let summary: Vec<String> = flag_combos
        .iter()
        .map(|(flags, label)| {
            format!(
                "0x{:02X}({}): {:?}",
                flags,
                label,
                sc::DeviceStatus::from_flags(*flags),
            )
        })
        .collect();
    assert_snapshot!(summary.join("\n"));
}

// ── HID layout constants ─────────────────────────────────────────────────────

#[test]
fn snapshot_hid_layout_constants() {
    assert_snapshot!(format!(
        "additional_axes={}, button_count={}, button_bytes={}, joystick_min_bytes={}, angle_sensor_bits={}, angle_sensor_max=0x{:X}",
        sc::HID_ADDITIONAL_AXES,
        sc::HID_BUTTON_COUNT,
        sc::HID_BUTTON_BYTES,
        sc::HID_JOYSTICK_REPORT_MIN_BYTES,
        sc::ANGLE_SENSOR_BITS,
        sc::ANGLE_SENSOR_MAX,
    ));
}

// ── Per-model torque constants ───────────────────────────────────────────────

#[test]
fn snapshot_max_torque_constants() {
    assert_snapshot!(format!(
        "default={:.1}, sport={:.1}, pro={:.1}, ultimate={:.1}",
        sc::MAX_TORQUE_NM,
        sc::MAX_TORQUE_SPORT,
        sc::MAX_TORQUE_PRO,
        sc::MAX_TORQUE_ULTIMATE,
    ));
}
