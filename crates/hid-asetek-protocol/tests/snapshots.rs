//! Additional insta snapshot tests for the Asetek HID protocol.
//!
//! Complements `snapshot_tests.rs` with wheel capabilities, non-zero input
//! report parsing, output sequence numbers, error display, and a full model
//! capabilities table.

use hid_asetek_protocol as asetek;
use insta::assert_debug_snapshot;

#[test]
fn snapshot_wheel_capabilities_all_models() {
    let models = [
        asetek::WheelModel::Forte,
        asetek::WheelModel::Invicta,
        asetek::WheelModel::LaPrima,
        asetek::WheelModel::Unknown,
    ];
    let summary: Vec<String> = models
        .iter()
        .map(|m| {
            let caps = asetek::WheelCapabilities::for_model(*m);
            format!(
                "{m:?}: torque={:.1}Nm, speed={}rpm, qr={}",
                caps.max_torque_nm, caps.max_speed_rpm, caps.supports_quick_release
            )
        })
        .collect();
    assert_debug_snapshot!(summary);
}

#[test]
fn snapshot_wheel_capabilities_default() {
    let caps = asetek::WheelCapabilities::default();
    assert_debug_snapshot!(format!(
        "torque={:.1}Nm, speed={}rpm, qr={}",
        caps.max_torque_nm, caps.max_speed_rpm, caps.supports_quick_release
    ));
}

#[test]
fn snapshot_input_report_nonzero() -> Result<(), String> {
    // Build a report with known non-zero values:
    // seq=1, angle=90000 (90°), speed=500, torque=1500 (15Nm), temp=40, status=0x03
    let mut data = [0u8; 32];
    data[0..2].copy_from_slice(&1u16.to_le_bytes());
    data[2..6].copy_from_slice(&90_000i32.to_le_bytes());
    data[6..8].copy_from_slice(&500i16.to_le_bytes());
    data[8..10].copy_from_slice(&1500i16.to_le_bytes());
    data[10] = 40; // temperature
    data[11] = 0x03; // connected + enabled
    let report = asetek::AsetekInputReport::parse(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(format!(
        "angle={:.3}deg, speed={:.4}rad_s, torque={:.3}Nm, temp={}, connected={}, enabled={}",
        report.wheel_angle_degrees(),
        report.wheel_speed_rad_s(),
        report.applied_torque_nm(),
        report.temperature,
        report.is_connected(),
        report.is_enabled()
    ));
    Ok(())
}

#[test]
fn snapshot_output_report_with_sequence() -> Result<(), String> {
    let report = asetek::AsetekOutputReport::new(42)
        .with_torque(10.5)
        .with_led(0x02, 0xFF);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

#[test]
fn snapshot_all_model_display_names() {
    let models = [
        asetek::AsetekModel::Forte,
        asetek::AsetekModel::Invicta,
        asetek::AsetekModel::LaPrima,
        asetek::AsetekModel::TonyKanaan,
        asetek::AsetekModel::InvictaPedals,
        asetek::AsetekModel::FortePedals,
        asetek::AsetekModel::LaPrimaPedals,
        asetek::AsetekModel::Unknown,
    ];
    let summary: Vec<String> = models
        .iter()
        .map(|m| {
            format!(
                "{m:?}: name='{}', torque={:.1}Nm",
                m.display_name(),
                m.max_torque_nm()
            )
        })
        .collect();
    assert_debug_snapshot!(summary);
}

#[test]
fn snapshot_input_report_short_error() {
    let data = [0u8; 10];
    let result = asetek::AsetekInputReport::parse(&data);
    assert!(result.is_err());
    if let Err(e) = result {
        assert_debug_snapshot!(format!("{e}"));
    }
}
