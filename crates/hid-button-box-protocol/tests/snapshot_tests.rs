//! Snapshot tests for the button box HID protocol.
//!
//! These tests lock in parsed report values to catch accidental protocol regressions.

use hid_button_box_protocol as button_box;
use insta::assert_debug_snapshot;

// parse_gamepad needs 10 bytes: 2 (buttons) + 2+2+2 (axes) + 1 (hat) + 1 (padding)

#[test]
fn test_snapshot_parse_gamepad_zeros() -> Result<(), String> {
    let data = [0u8; 10];
    let report =
        button_box::ButtonBoxInputReport::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(format!(
        "buttons={:#010x}, axis_x={}, axis_y={}, axis_z={}, axis_rz={}, hat={:?}",
        report.buttons,
        report.axis_x,
        report.axis_y,
        report.axis_z,
        report.axis_rz,
        report.hat_direction()
    ));
    Ok(())
}

#[test]
fn test_snapshot_parse_gamepad_button0_pressed() -> Result<(), String> {
    // byte 0 = 0x01 → button 0 set; byte 8 = 0xFF → hat=Neutral
    let data = [0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0x00];
    let report =
        button_box::ButtonBoxInputReport::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(format!(
        "button0={}, button1={}, button_count={}, hat={:?}",
        report.button(0),
        report.button(1),
        report.button_count(),
        report.hat_direction()
    ));
    Ok(())
}

#[test]
fn test_snapshot_parse_gamepad_hat_right() -> Result<(), String> {
    // byte 8 = 2 → HatDirection::Right
    let data = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00];
    let report =
        button_box::ButtonBoxInputReport::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(format!("hat={:?}", report.hat_direction()));
    Ok(())
}

// parse_extended needs 13 bytes: 4 (buttons) + 2+2+2+2 (axes) + 1 (hat)

#[test]
fn test_snapshot_parse_extended_zeros() -> Result<(), String> {
    let data = [0u8; 13];
    let report =
        button_box::ButtonBoxInputReport::parse_extended(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(format!(
        "buttons={:#010x}, axis_x={}, axis_y={}, axis_z={}, axis_rz={}, hat={:?}",
        report.buttons,
        report.axis_x,
        report.axis_y,
        report.axis_z,
        report.axis_rz,
        report.hat_direction()
    ));
    Ok(())
}

#[test]
fn test_snapshot_parse_extended_all_buttons() -> Result<(), String> {
    // bytes 0-3 = 0xFF → all 32 buttons pressed
    let mut data = [0u8; 13];
    data[0] = 0xFF;
    data[1] = 0xFF;
    data[2] = 0xFF;
    data[3] = 0xFF;
    let report =
        button_box::ButtonBoxInputReport::parse_extended(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(format!(
        "buttons={:#010x}, button_count={}",
        report.buttons,
        report.button_count()
    ));
    Ok(())
}

#[test]
fn test_snapshot_hat_directions_all() {
    let directions: Vec<_> = (0u8..=8)
        .map(|hat| {
            let mut report = button_box::ButtonBoxInputReport::default();
            report.hat = hat;
            format!("hat_byte={} -> {:?}", hat, report.hat_direction())
        })
        .collect();
    assert_debug_snapshot!(directions.join("\n"));
}

#[test]
fn test_snapshot_capabilities_basic() {
    let caps = button_box::ButtonBoxCapabilities::basic();
    assert_debug_snapshot!(format!(
        "buttons={}, axes={}, pov={}, rotary={}, rotary_count={}",
        caps.button_count,
        caps.analog_axis_count,
        caps.has_pov_hat,
        caps.has_rotary_encoders,
        caps.rotary_encoder_count
    ));
}

#[test]
fn test_snapshot_capabilities_extended() {
    let caps = button_box::ButtonBoxCapabilities::extended();
    assert_debug_snapshot!(format!(
        "buttons={}, axes={}, pov={}, rotary={}, rotary_count={}",
        caps.button_count,
        caps.analog_axis_count,
        caps.has_pov_hat,
        caps.has_rotary_encoders,
        caps.rotary_encoder_count
    ));
}

#[test]
fn test_snapshot_button_box_types() {
    let types = [
        ("simple", button_box::ButtonBoxType::Simple),
        ("standard", button_box::ButtonBoxType::Standard),
        ("extended", button_box::ButtonBoxType::Extended),
    ];
    assert_debug_snapshot!(format!("{:?}", types));
}

#[test]
fn test_snapshot_constants() {
    assert_debug_snapshot!(format!(
        "VENDOR_ID={:#06x}, PRODUCT_ID={:#06x}, REPORT_SIZE={}, MAX_BUTTONS={}, MAX_AXES={}",
        button_box::VENDOR_ID_GENERIC,
        button_box::PRODUCT_ID_BUTTON_BOX,
        button_box::REPORT_SIZE_GAMEPAD,
        button_box::MAX_BUTTONS,
        button_box::MAX_AXES,
    ));
}
