//! BDD end-to-end tests for the Logitech protocol stack.
//!
//! Each test follows a Given/When/Then pattern to verify observable hardware-ready
//! behaviors without real USB hardware.

use racing_wheel_hid_logitech_protocol::{
    build_gain_report, build_native_mode_report, build_set_leds_report, build_set_range_report,
    ids::report_ids, parse_input_report, product_ids, LogitechConstantForceEncoder,
    CONSTANT_FORCE_REPORT_LEN,
};
use racing_wheel_integration_tests::logitech_virtual::LogitechScenario;

// ─── Scenario 1: wheel sends native mode on initialize ───────────────────────

#[test]
fn scenario_wheel_sends_native_mode_on_initialize() -> Result<(), Box<dyn std::error::Error>> {
    // Given: G920 wheel
    let mut s = LogitechScenario::wheel(product_ids::G920);

    // When: initialized
    s.initialize()?;

    // Then: exactly two feature reports are sent (native mode + set range)
    assert_eq!(
        s.device.feature_reports().len(),
        2,
        "should send native mode + set range"
    );

    // Then: report ID 0xF8 is present
    assert!(s.device.sent_feature_report_id(report_ids::VENDOR));

    Ok(())
}

// ─── Scenario 2: unknown PID skips init ──────────────────────────────────────

#[test]
fn scenario_unknown_pid_skips_init() -> Result<(), Box<dyn std::error::Error>> {
    // Given: unknown product ID
    let mut s = LogitechScenario::wheel(0xFF00);

    // When: initialized
    s.initialize()?;

    // Then: no feature reports are sent
    assert_eq!(
        s.device.feature_reports().len(),
        0,
        "unknown PID must not send init reports"
    );

    Ok(())
}

// ─── Scenario 3: native mode report has correct bytes ────────────────────────

#[test]
fn scenario_native_mode_report_bytes_are_correct() -> Result<(), Box<dyn std::error::Error>> {
    // Given: G923 Xbox wheel
    let mut s = LogitechScenario::wheel(product_ids::G923_XBOX);

    // When: initialized
    s.initialize()?;

    // Then: first feature report is native mode command [0xF8, 0x0A, 0x00 x5]
    let reports = s.device.feature_reports_with_id(report_ids::VENDOR);
    assert!(!reports.is_empty(), "at least one vendor report required");
    assert_eq!(
        reports[0].as_slice(),
        &[0xF8u8, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00],
        "native mode payload must match Logitech protocol spec"
    );

    Ok(())
}

// ─── Scenario 4: initialize returns error on I/O failure ─────────────────────

#[test]
fn scenario_initialize_returns_error_on_io_failure() {
    // Given: always-failing device
    let mut s = LogitechScenario::wheel_failing(product_ids::G920);

    // When: initialized
    let result = s.initialize();

    // Then: error is propagated
    assert!(result.is_err(), "I/O failure must propagate as error");
}

// ─── Scenario 5: set range report encodes 900° correctly ─────────────────────

#[test]
fn scenario_set_range_encodes_900_degrees() -> Result<(), Box<dyn std::error::Error>> {
    // Given/When
    let report = build_set_range_report(900);

    // Then: correct command bytes
    assert_eq!(report[0], 0xF8, "report ID");
    assert_eq!(report[1], 0x81, "SET_RANGE command");
    // 900 dec = 0x0384 little-endian = [0x84, 0x03]
    assert_eq!(report[2], 0x84, "LSB of 900");
    assert_eq!(report[3], 0x03, "MSB of 900");

    Ok(())
}

// ─── Scenario 6: Pro Racing Wheel uses 1080° range ───────────────────────────

#[test]
fn scenario_pro_racing_uses_1080_degree_range() -> Result<(), Box<dyn std::error::Error>> {
    // Given: Pro Racing Wheel
    let mut s = LogitechScenario::wheel(product_ids::PRO_RACING);

    // When: initialized
    s.initialize()?;

    // Then: second feature report sets range to 1080° (0x0438 = [0x38, 0x04])
    let reports = s.device.feature_reports_with_id(report_ids::VENDOR);
    assert_eq!(reports.len(), 2, "expected native mode + set range");
    assert_eq!(reports[1][1], 0x81, "second report is SET_RANGE");
    assert_eq!(reports[1][2], 0x38, "LSB of 1080°");
    assert_eq!(reports[1][3], 0x04, "MSB of 1080°");

    Ok(())
}

// ─── Scenario 7: constant force encoder encodes half torque ──────────────────

#[test]
fn scenario_constant_force_encoder_half_torque() -> Result<(), Box<dyn std::error::Error>> {
    // Given: G920 encoder (2.2 Nm max)
    let enc = LogitechConstantForceEncoder::new(2.2);

    // When: encoding 1.1 Nm (50% of max)
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(1.1, &mut out);

    // Then: magnitude = 5000 (50% of ±10000 range)
    let magnitude = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(magnitude, 5000, "50% torque = 5000 magnitude units");
    assert_eq!(out[0], 0x12, "constant force report ID");
    assert_eq!(out[1], 1, "effect block index 1");

    Ok(())
}

// ─── Scenario 8: LED report encodes bitmask correctly ────────────────────────

#[test]
fn scenario_led_report_encodes_bitmask() -> Result<(), Box<dyn std::error::Error>> {
    // Given: all 5 LEDs on
    let report = build_set_leds_report(0b00011111);

    // Then: correct encoding
    assert_eq!(report[0], 0xF8, "vendor report ID");
    assert_eq!(report[1], 0x12, "SET_LEDS command");
    assert_eq!(report[2], 0x1F, "all 5 LEDs = 0x1F");
    assert_eq!(&report[3..], &[0u8; 4], "trailing bytes zero");

    Ok(())
}

// ─── Scenario 9: gain report encodes full and zero gain ──────────────────────

#[test]
fn scenario_gain_report_encodes_correctly() -> Result<(), Box<dyn std::error::Error>> {
    // Full gain
    let full = build_gain_report(0xFF);
    assert_eq!(full[0], 0x16, "Device Gain report ID");
    assert_eq!(full[1], 0xFF, "full gain");

    // Zero gain
    let zero = build_gain_report(0);
    assert_eq!(zero[0], 0x16);
    assert_eq!(zero[1], 0, "zero gain");

    Ok(())
}

// ─── Scenario 10: input report parsing — centered steering ───────────────────

#[test]
fn scenario_input_report_parse_center() -> Result<(), Box<dyn std::error::Error>> {
    // Given: input report with centered steering (0x8000 LE)
    let mut data = [0u8; 12];
    data[0] = 0x01;
    data[1] = 0x00;
    data[2] = 0x80;

    // When: parsed
    let state = parse_input_report(&data).ok_or("parse failed")?;

    // Then: steering is ~0.0
    assert!(
        state.steering.abs() < 0.001,
        "centered steering must be ~0.0"
    );

    Ok(())
}

// ─── Scenario 11: G923 PS variant initializes correctly ──────────────────────

#[test]
fn scenario_g923_ps_initializes() -> Result<(), Box<dyn std::error::Error>> {
    // Given: G923 PlayStation wheel
    let mut s = LogitechScenario::wheel(product_ids::G923_PS);

    // When: initialized
    s.initialize()?;

    // Then: 2 feature reports with correct commands
    assert_eq!(s.device.feature_reports().len(), 2);
    assert_eq!(
        s.device.feature_reports()[0][1],
        0x0A,
        "native mode command"
    );
    assert_eq!(
        s.device.feature_reports()[1][1],
        0x81,
        "set range command"
    );

    Ok(())
}

// ─── Scenario 12: native mode report standalone encoding ─────────────────────

#[test]
fn scenario_native_mode_report_standalone() -> Result<(), Box<dyn std::error::Error>> {
    // Given/When
    let r = build_native_mode_report();

    // Then: full expected wire bytes
    assert_eq!(
        r,
        [0xF8u8, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00],
        "native mode report must exactly match Logitech spec"
    );

    Ok(())
}
