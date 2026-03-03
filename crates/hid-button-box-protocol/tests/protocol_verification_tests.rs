//! Protocol verification tests for the generic button box HID protocol.
//!
//! These tests cross-reference our constants and report format against the
//! pid.codes open-source hardware VID registry and standard USB HID gamepad
//! report conventions.
//!
//! ## Sources cited
//!
//! | # | Source | What it confirms |
//! |---|--------|------------------|
//! | 1 | pid.codes registry (https://pid.codes/1209/) | VID `0x1209` = open source hardware allocation |
//! | 2 | USB HID Usage Tables 1.4, §4 Generic Desktop | Gamepad usage page conventions |
//! | 3 | USB HID spec §6.2.2.4 | Standard report descriptor format |

use hid_button_box_protocol::{
    ButtonBoxInputReport, HatDirection, MAX_AXES, MAX_BUTTONS, PRODUCT_ID_BUTTON_BOX,
    REPORT_SIZE_GAMEPAD, VENDOR_ID_GENERIC,
};

// ════════════════════════════════════════════════════════════════════════════
// § 1. VID / PID verification
// ════════════════════════════════════════════════════════════════════════════

/// VID `0x1209` = pid.codes open-source hardware allocation.
/// Source [1]: pid.codes registry — VID `0x1209` reserved for open-source projects.
/// This is the standard VID used by DIY/open-source USB devices.
#[test]
fn vid_is_pid_codes_open_source() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        VENDOR_ID_GENERIC, 0x1209,
        "Button box VID must be 0x1209 (pid.codes open-source allocation)"
    );
    Ok(())
}

/// PID `0x1BBD` is the assigned product ID for generic button boxes.
#[test]
fn pid_is_button_box() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        PRODUCT_ID_BUTTON_BOX, 0x1BBD,
        "Button box PID must be 0x1BBD"
    );
    Ok(())
}

/// VID must be in the pid.codes range (0x1209).
/// Source [1]: pid.codes assigns VID 0x1209 to open-source hardware.
/// PIDs in 0x1000-0x1FFF are InterBiometrics reserved; 0x1BBD is outside
/// the 0x0000-0x0FFF test range, making it a valid allocation.
#[test]
#[allow(clippy::assertions_on_constants)]
fn pid_is_outside_reserved_test_range() -> Result<(), Box<dyn std::error::Error>> {
    assert!(
        PRODUCT_ID_BUTTON_BOX > 0x0FFF,
        "PID must be outside the pid.codes 0x0000-0x0FFF test-only range"
    );
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 2. Report format constants
// ════════════════════════════════════════════════════════════════════════════

/// Standard HID gamepad report size is 8 bytes.
/// Source [3]: USB HID spec — standard gamepad reports use compact formats.
#[test]
fn gamepad_report_size_is_8_bytes() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        REPORT_SIZE_GAMEPAD, 8,
        "Standard gamepad report must be 8 bytes"
    );
    Ok(())
}

/// Maximum button count for the extended format is 32 (4 bytes of bitmask).
/// Source [2]: USB HID Usage Tables — joystick/gamepad buttons up to 128,
/// but 32 is the practical limit for standard descriptors.
#[test]
fn max_buttons_is_32() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(MAX_BUTTONS, 32, "Max button count must be 32");
    Ok(())
}

/// Maximum axes count is 4 (X, Y, Z, RZ — standard gamepad layout).
/// Source [2]: USB HID Usage Tables §4 — Generic Desktop X/Y/Z/Rz axes.
#[test]
fn max_axes_is_4() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(MAX_AXES, 4, "Max axes must be 4 (X, Y, Z, RZ)");
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 3. Input report parsing verification
// ════════════════════════════════════════════════════════════════════════════

/// Gamepad parse requires exactly 10 bytes (8-byte minimum check + 2 extra
/// bytes read by the parser for hat + padding).
#[test]
fn gamepad_parse_minimum_10_bytes() -> Result<(), Box<dyn std::error::Error>> {
    // 9 bytes: size check passes but parser runs short
    let short = [0u8; 9];
    assert!(
        ButtonBoxInputReport::parse_gamepad(&short).is_err(),
        "9 bytes must fail"
    );

    // 10 bytes: should succeed
    let exact = [0u8; 10];
    let report = ButtonBoxInputReport::parse_gamepad(&exact).map_err(|e| e.to_string())?;
    assert_eq!(report.buttons, 0, "all-zero report must have 0 buttons");
    Ok(())
}

/// Extended parse requires 13 bytes (4-byte buttons + 4×i16 axes + 1 hat).
#[test]
fn extended_parse_minimum_13_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let short = [0u8; 12];
    assert!(
        ButtonBoxInputReport::parse_extended(&short).is_err(),
        "12 bytes must fail"
    );

    let exact = [0u8; 13];
    let report = ButtonBoxInputReport::parse_extended(&exact).map_err(|e| e.to_string())?;
    assert_eq!(report.buttons, 0);
    Ok(())
}

/// Hat switch encoding: 0–7 = 8 cardinal/diagonal directions, ≥8 = neutral.
/// Source [2]: USB HID Usage Tables — Hat Switch logical values 0–7.
#[test]
fn hat_direction_encoding() -> Result<(), Box<dyn std::error::Error>> {
    let directions = [
        (0, HatDirection::Up),
        (1, HatDirection::UpRight),
        (2, HatDirection::Right),
        (3, HatDirection::DownRight),
        (4, HatDirection::Down),
        (5, HatDirection::DownLeft),
        (6, HatDirection::Left),
        (7, HatDirection::UpLeft),
        (0xFF, HatDirection::Neutral),
        (8, HatDirection::Neutral),
    ];
    for &(hat_val, expected_dir) in &directions {
        let report = ButtonBoxInputReport {
            hat: hat_val,
            ..Default::default()
        };
        assert_eq!(
            report.hat_direction(),
            expected_dir,
            "hat value {hat_val} must map to {expected_dir:?}"
        );
    }
    Ok(())
}

/// Button indices 0–31 must be settable; index 32+ must be a no-op.
#[test]
fn button_index_bounds() -> Result<(), Box<dyn std::error::Error>> {
    let mut report = ButtonBoxInputReport::default();

    // Set all 32 buttons
    for i in 0..MAX_BUTTONS {
        report.set_button(i, true);
        assert!(
            report.button(i),
            "button {i} must be readable after set_button"
        );
    }
    assert_eq!(report.button_count(), 32, "all 32 buttons must be set");

    // Out-of-range must be no-op
    report.set_button(32, true);
    assert!(!report.button(32), "button 32 must return false (out of range)");
    assert_eq!(report.button_count(), 32, "count must still be 32");
    Ok(())
}

/// Default report: all zeros, hat neutral.
#[test]
fn default_report_is_neutral() -> Result<(), Box<dyn std::error::Error>> {
    let report = ButtonBoxInputReport::default();
    assert_eq!(report.buttons, 0);
    assert_eq!(report.axis_x, 0);
    assert_eq!(report.axis_y, 0);
    assert_eq!(report.axis_z, 0);
    assert_eq!(report.axis_rz, 0);
    assert_eq!(report.hat, 0xFF, "default hat must be 0xFF (neutral)");
    assert_eq!(report.hat_direction(), HatDirection::Neutral);
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 4. Axis normalization
// ════════════════════════════════════════════════════════════════════════════

/// Axis normalization: i16::MAX → 1.0, 0 → 0.0, i16::MIN → approx −1.0.
#[test]
fn axis_normalization_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let report = ButtonBoxInputReport {
        axis_x: i16::MAX,
        axis_y: 0,
        axis_z: i16::MIN,
        axis_rz: 0,
        ..Default::default()
    };

    assert!(
        (report.axis_normalized(0) - 1.0).abs() < 0.001,
        "i16::MAX must normalize to ~1.0"
    );
    assert!(
        report.axis_normalized(1).abs() < 0.001,
        "0 must normalize to ~0.0"
    );
    assert!(
        report.axis_normalized(2) < -0.99,
        "i16::MIN must normalize to ~-1.0"
    );
    // Out-of-range axis returns 0
    assert!(
        report.axis_normalized(5).abs() < f32::EPSILON,
        "out-of-range axis must return 0.0"
    );
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 5. Extended report byte layout verification
// ════════════════════════════════════════════════════════════════════════════

/// Extended report layout: 4 bytes buttons (u32 LE), then i16 LE axes × 4,
/// then 1 byte hat.
#[test]
fn extended_report_byte_layout() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 13];
    // buttons = 0x00000005 (buttons 0 and 2 set)
    data[0] = 0x05;
    data[1] = 0x00;
    data[2] = 0x00;
    data[3] = 0x00;
    // axis_x = 1000 (0x03E8 LE)
    data[4] = 0xE8;
    data[5] = 0x03;
    // axis_y = -1000 (0xFC18 LE)
    data[6] = 0x18;
    data[7] = 0xFC;
    // axis_z = 0
    data[8] = 0x00;
    data[9] = 0x00;
    // axis_rz = 0
    data[10] = 0x00;
    data[11] = 0x00;
    // hat = 2 (Right)
    data[12] = 0x02;

    let report = ButtonBoxInputReport::parse_extended(&data).map_err(|e| e.to_string())?;
    assert_eq!(report.buttons, 0x00000005, "buttons must decode as u32 LE");
    assert!(report.button(0), "button 0 must be set");
    assert!(report.button(2), "button 2 must be set");
    assert!(!report.button(1), "button 1 must be clear");
    assert_eq!(report.axis_x, 1000, "axis_x must be 1000");
    assert_eq!(report.axis_y, -1000, "axis_y must be -1000");
    assert_eq!(
        report.hat_direction(),
        HatDirection::Right,
        "hat=2 must be Right"
    );
    Ok(())
}
