//! Additional insta snapshot tests for the Cammus HID protocol.
//!
//! Complements `snapshot_tests.rs` with model-from-PID lookup, full-left
//! steering, multi-axis input, half-negative torque, and mode constants.

use insta::assert_debug_snapshot;
use racing_wheel_hid_cammus_protocol as cammus;

#[test]
fn snapshot_model_from_pid_all() {
    let pids: &[u16] = &[
        cammus::PRODUCT_C5,
        cammus::PRODUCT_C12,
        cammus::PRODUCT_CP5_PEDALS,
        cammus::PRODUCT_LC100_PEDALS,
        0xFFFF,
    ];
    let summary: Vec<String> = pids
        .iter()
        .map(|pid| {
            let model = cammus::CammusModel::from_pid(*pid);
            match model {
                Some(m) => format!("PID={pid:#06X}: {:?} ({})", m, m.name()),
                None => format!("PID={pid:#06X}: None"),
            }
        })
        .collect();
    assert_debug_snapshot!(summary);
}

#[test]
fn snapshot_parse_full_left_steering() -> Result<(), String> {
    let mut data = [0u8; 64];
    let val: i16 = -i16::MAX;
    let bytes = val.to_le_bytes();
    data[0] = bytes[0];
    data[1] = bytes[1];
    let report = cammus::parse(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(format!(
        "steering={:.4}, throttle={:.4}, brake={:.4}",
        report.steering, report.throttle, report.brake
    ));
    Ok(())
}

#[test]
fn snapshot_parse_all_axes_max() -> Result<(), String> {
    let mut data = [0u8; 64];
    // steering = i16::MAX
    let steer_bytes = i16::MAX.to_le_bytes();
    data[0] = steer_bytes[0];
    data[1] = steer_bytes[1];
    // throttle = u16::MAX
    data[2] = 0xFF;
    data[3] = 0xFF;
    // brake = u16::MAX
    data[4] = 0xFF;
    data[5] = 0xFF;
    // buttons = 0xFFFF
    data[6] = 0xFF;
    data[7] = 0xFF;
    // clutch = u16::MAX
    data[8] = 0xFF;
    data[9] = 0xFF;
    // handbrake = u16::MAX
    data[10] = 0xFF;
    data[11] = 0xFF;
    let report = cammus::parse(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(format!(
        "steering={:.4}, throttle={:.4}, brake={:.4}, clutch={:.4}, handbrake={:.4}, buttons={:#06X}",
        report.steering, report.throttle, report.brake,
        report.clutch, report.handbrake, report.buttons
    ));
    Ok(())
}

#[test]
fn snapshot_encode_torque_half_negative() {
    assert_debug_snapshot!(cammus::encode_torque(-0.5));
}

#[test]
fn snapshot_mode_constants() {
    assert_debug_snapshot!(format!(
        "MODE_GAME={:#04X}, MODE_CONFIG={:#04X}, FFB_REPORT_ID={:#04X}, FFB_REPORT_LEN={}",
        cammus::MODE_GAME,
        cammus::MODE_CONFIG,
        cammus::FFB_REPORT_ID,
        cammus::FFB_REPORT_LEN
    ));
}
