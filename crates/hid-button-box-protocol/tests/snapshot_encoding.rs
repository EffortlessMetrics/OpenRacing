//! Extended snapshot tests for button box wire-format encoding.
//!
//! These tests supplement `snapshot_tests.rs` by covering byte-level parsing
//! boundary values, normalized axis encoding, rotary encoder delta clamping,
//! and serialized capabilities that would detect wire-format regressions.

use hid_button_box_protocol as button_box;
use insta::assert_snapshot;

// ── Gamepad report byte-level parsing ────────────────────────────────────────

#[test]
fn test_snapshot_parse_gamepad_max_axes() -> Result<(), String> {
    // axis_x=0x7FFF (max i16), axis_y=0x8000 (min i16)
    let data: [u8; 10] = [0x00, 0x00, 0xFF, 0x7F, 0x00, 0x80, 0x00, 0x00, 0xFF, 0x00];
    let report =
        button_box::ButtonBoxInputReport::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert_snapshot!(format!(
        "axis_x={}, axis_y={}, norm_x={:.4}, norm_y={:.4}",
        report.axis_x,
        report.axis_y,
        report.axis_normalized(0),
        report.axis_normalized(1)
    ));
    Ok(())
}

#[test]
fn test_snapshot_parse_gamepad_all_buttons_low_word() -> Result<(), String> {
    // buttons = 0xFFFF (16 bits via u16 LE)
    let data: [u8; 10] = [0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0x00];
    let report =
        button_box::ButtonBoxInputReport::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert_snapshot!(format!(
        "buttons=0x{:08X}, count={}",
        report.buttons,
        report.button_count()
    ));
    Ok(())
}

// ── Extended report byte-level parsing ───────────────────────────────────────

#[test]
fn test_snapshot_parse_extended_full_axes() -> Result<(), String> {
    let mut data = [0u8; 13];
    // buttons = 0
    // axis_x = i16::MAX
    data[4] = 0xFF;
    data[5] = 0x7F;
    // axis_y = i16::MIN
    data[6] = 0x00;
    data[7] = 0x80;
    // axis_z = 1000
    data[8] = 0xE8;
    data[9] = 0x03;
    // axis_rz = -1000
    data[10] = 0x18;
    data[11] = 0xFC;
    // hat = neutral
    data[12] = 0xFF;
    let report =
        button_box::ButtonBoxInputReport::parse_extended(&data).map_err(|e| e.to_string())?;
    assert_snapshot!(format!(
        "x={}, y={}, z={}, rz={}, hat={:?}",
        report.axis_x,
        report.axis_y,
        report.axis_z,
        report.axis_rz,
        report.hat_direction()
    ));
    Ok(())
}

#[test]
fn test_snapshot_parse_extended_button_pattern() -> Result<(), String> {
    let mut data = [0u8; 13];
    // buttons = 0xAAAA_5555 (alternating bit pattern)
    data[0] = 0x55;
    data[1] = 0x55;
    data[2] = 0xAA;
    data[3] = 0xAA;
    let report =
        button_box::ButtonBoxInputReport::parse_extended(&data).map_err(|e| e.to_string())?;
    assert_snapshot!(format!(
        "buttons=0x{:08X}, count={}, b0={}, b1={}, b16={}, b17={}",
        report.buttons,
        report.button_count(),
        report.button(0),
        report.button(1),
        report.button(16),
        report.button(17)
    ));
    Ok(())
}

// ── Normalized axis boundary values ──────────────────────────────────────────

#[test]
fn test_snapshot_axis_normalized_boundaries() {
    let cases = [
        ("max", i16::MAX),
        ("min", i16::MIN),
        ("zero", 0i16),
        ("one", 1i16),
        ("neg_one", -1i16),
    ];
    let results: Vec<String> = cases
        .iter()
        .map(|(label, val)| {
            let report = button_box::ButtonBoxInputReport {
                axis_x: *val,
                ..Default::default()
            };
            format!("{label}: raw={val}, norm={:.6}", report.axis_normalized(0))
        })
        .collect();
    assert_snapshot!(results.join("\n"));
}

// ── Rotary encoder delta clamping ────────────────────────────────────────────

#[test]
fn test_snapshot_rotary_encoder_clamp_positive() {
    let mut encoder = button_box::RotaryEncoderState::new();
    encoder.update(200);
    assert_snapshot!(format!("pos={}, delta={}", encoder.position, encoder.delta));
}

#[test]
fn test_snapshot_rotary_encoder_clamp_negative() {
    let mut encoder = button_box::RotaryEncoderState::new();
    encoder.update(-300);
    assert_snapshot!(format!("pos={}, delta={}", encoder.position, encoder.delta));
}

#[test]
fn test_snapshot_rotary_encoder_sequential() {
    let mut encoder = button_box::RotaryEncoderState::new();
    let mut steps: Vec<String> = Vec::new();
    for pos in [10, 20, 15, 0, -5] {
        encoder.update(pos);
        steps.push(format!(
            "pos={}, delta={}, btn={}",
            encoder.position, encoder.delta, encoder.button_pressed
        ));
    }
    assert_snapshot!(steps.join("\n"));
}

// ── Capabilities YAML serialization ──────────────────────────────────────────

#[test]
fn test_snapshot_capabilities_yaml_basic() {
    let caps = button_box::ButtonBoxCapabilities::basic();
    assert_snapshot!(format!("{caps:?}"));
}

#[test]
fn test_snapshot_capabilities_yaml_extended() {
    let caps = button_box::ButtonBoxCapabilities::extended();
    assert_snapshot!(format!("{caps:?}"));
}

// ── Error message formatting ─────────────────────────────────────────────────

#[test]
fn test_snapshot_all_error_variants() {
    let errors = [
        button_box::ButtonBoxError::InvalidReportSize {
            expected: 8,
            actual: 3,
        },
        button_box::ButtonBoxError::InvalidButtonIndex(33),
        button_box::ButtonBoxError::InvalidAxisIndex(5),
        button_box::ButtonBoxError::HidError("timeout".into()),
    ];
    let messages: Vec<String> = errors.iter().map(|e| format!("{e}")).collect();
    assert_snapshot!(messages.join("\n"));
}

// ── Hat direction exhaustive ─────────────────────────────────────────────────

#[test]
fn test_snapshot_hat_direction_full_sweep() {
    let results: Vec<String> = (0u8..=15)
        .map(|hat| {
            let report = button_box::ButtonBoxInputReport {
                hat,
                ..Default::default()
            };
            format!("0x{hat:02X} -> {:?}", report.hat_direction())
        })
        .collect();
    assert_snapshot!(results.join("\n"));
}
