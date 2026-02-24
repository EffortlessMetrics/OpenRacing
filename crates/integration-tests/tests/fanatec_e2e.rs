//! BDD end-to-end tests for the Fanatec protocol stack.
//!
//! Each test follows a Given/When/Then pattern to verify observable hardware-ready
//! behaviors without real USB hardware.

use racing_wheel_hid_fanatec_protocol::{
    ids::report_ids, product_ids, FanatecConstantForceEncoder, CONSTANT_FORCE_REPORT_LEN,
};
use racing_wheel_integration_tests::fanatec_virtual::FanatecScenario;

// ─── Scenario 1: wheelbase sends mode-switch on initialize ───────────────────

#[test]
fn scenario_wheelbase_sends_mode_switch_on_initialize() -> Result<(), Box<dyn std::error::Error>> {
    // Given: GT DD Pro wheelbase
    let mut s = FanatecScenario::wheelbase(product_ids::GT_DD_PRO);

    // When: initialized
    s.initialize()?;

    // Then: exactly one feature report was sent
    assert_eq!(s.device.feature_reports().len(), 1);

    // Then: the report carries the MODE_SWITCH report ID
    assert!(s.device.sent_feature_report_id(report_ids::MODE_SWITCH));

    Ok(())
}

// ─── Scenario 2: non-wheelbase skips mode-switch ─────────────────────────────

#[test]
fn scenario_non_wheelbase_skips_mode_switch() -> Result<(), Box<dyn std::error::Error>> {
    // Given: unknown/accessory product ID that is not a wheelbase
    let mut s = FanatecScenario::wheelbase(0xFF00);

    // When: initialized
    s.initialize()?;

    // Then: no feature reports are sent
    assert_eq!(
        s.device.feature_reports().len(),
        0,
        "non-wheelbase must not send a mode-switch report"
    );

    Ok(())
}

// ─── Scenario 3: mode-switch report has correct wire bytes ───────────────────

#[test]
fn scenario_mode_switch_report_bytes_are_correct() -> Result<(), Box<dyn std::error::Error>> {
    // Given: CSL DD wheelbase
    let mut s = FanatecScenario::wheelbase(product_ids::CSL_DD);

    // When: initialized
    s.initialize()?;

    // Then: mode-switch payload is exactly [0x01, 0x01, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00]
    let reports = s.device.feature_reports_with_id(report_ids::MODE_SWITCH);
    assert_eq!(reports.len(), 1);
    assert_eq!(
        reports[0].as_slice(),
        &[0x01u8, 0x01, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00],
        "mode-switch payload must match Fanatec Advanced/PC mode command"
    );

    Ok(())
}

// ─── Scenario 4: initialize returns error on I/O failure ─────────────────────

#[test]
fn scenario_initialize_returns_error_on_io_failure() {
    // Given: always-failing device
    let mut s = FanatecScenario::wheelbase_failing(product_ids::GT_DD_PRO);

    // When: initialized
    let result = s.initialize();

    // Then: returns Err (write failure propagated)
    assert!(result.is_err(), "I/O failure must propagate from initialize");
}

// ─── Scenario 5: disconnect, reconnect, and reinitialize ─────────────────────

#[test]
fn scenario_disconnect_reconnect_and_reinitialize() -> Result<(), Box<dyn std::error::Error>> {
    // Given: GT DD Pro after successful init
    let mut s = FanatecScenario::wheelbase(product_ids::GT_DD_PRO);
    s.initialize()?;
    assert_eq!(s.device.feature_reports().len(), 1);

    // When: device disconnects
    s.device.disconnect();
    assert!(!s.device.is_connected());

    // Then: reinitialize while disconnected returns Err
    assert!(
        s.initialize().is_err(),
        "initialize must fail when device is disconnected"
    );

    // When: device reconnects
    s.device.reconnect();
    s.device.clear_records();

    // Then: reinitialize succeeds and sends exactly one mode-switch report again
    s.initialize()?;
    assert_eq!(
        s.device.feature_reports().len(),
        1,
        "reinitialize after reconnect must send mode-switch again"
    );

    Ok(())
}

// ─── Scenario 6: constant-force encoder encodes max torque correctly ─────────

#[test]
fn scenario_constant_force_max_torque_encoding() {
    // Given: GT DD Pro encoder at 8 Nm
    let max_nm: f32 = 8.0;
    let encoder = FanatecConstantForceEncoder::new(max_nm);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];

    // When: encoding maximum positive torque
    let written = encoder.encode(max_nm, 0, &mut out);

    // Then: report ID and command byte are correct
    assert_eq!(written, CONSTANT_FORCE_REPORT_LEN);
    assert_eq!(out[0], report_ids::FFB_OUTPUT, "byte 0 must be FFB_OUTPUT report ID");
    assert_eq!(out[1], 0x01, "byte 1 must be CONSTANT_FORCE command");

    // Then: signed i16 LE at bytes 2–3 must be +32767 (maximum positive)
    let force = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(force, i16::MAX, "max torque must encode to i16::MAX");
}

// ─── Scenario 7: zero-torque produces a neutral output report ────────────────

#[test]
fn scenario_constant_force_zero_torque_encoding() {
    // Given: CSL DD encoder at 8 Nm
    let encoder = FanatecConstantForceEncoder::new(8.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];

    // When: encoding zero torque
    encoder.encode(0.0, 0, &mut out);

    // Then: force bytes are zero
    let force = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(force, 0, "zero torque must encode to 0");

    // Then: report ID and command still set
    assert_eq!(out[0], report_ids::FFB_OUTPUT);
    assert_eq!(out[1], 0x01);
}

// ─── Scenario 8: DD1 also sends mode-switch ───────────────────────────────────

#[test]
fn scenario_dd1_sends_mode_switch_on_initialize() -> Result<(), Box<dyn std::error::Error>> {
    // Given: Podium DD1 (20 Nm direct drive)
    let mut s = FanatecScenario::wheelbase(product_ids::DD1);

    // When: initialized
    s.initialize()?;

    // Then: exactly one mode-switch report sent
    assert_eq!(s.device.feature_reports().len(), 1);
    assert!(s.device.sent_feature_report_id(report_ids::MODE_SWITCH));

    Ok(())
}

// ─── Scenario 9: graceful shutdown sends stop-all ─────────────────────────────

#[test]
fn scenario_shutdown_sends_stop_all_output_report() -> Result<(), Box<dyn std::error::Error>> {
    // Given: initialized GT DD Pro wheelbase
    let mut s = FanatecScenario::wheelbase(product_ids::GT_DD_PRO);
    s.initialize()?;
    s.device.clear_records();

    // When: graceful shutdown
    s.shutdown()?;

    // Then: exactly one output report sent (stop-all)
    assert_eq!(
        s.device.output_reports().len(),
        1,
        "shutdown must send exactly one stop-all output report"
    );

    let report = s.device.last_output_report().expect("report must be present");
    // stop-all layout: [FFB_OUTPUT=0x01, STOP_ALL=0x0F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
    assert_eq!(report[0], report_ids::FFB_OUTPUT, "byte 0 must be FFB_OUTPUT");
    assert_eq!(report[1], 0x0F, "byte 1 must be STOP_ALL command (0x0F)");
    assert_eq!(&report[2..], &[0x00u8; 6], "trailing bytes must be zero");

    // Then: no feature reports during shutdown
    assert_eq!(
        s.device.feature_reports().len(),
        0,
        "shutdown must not send any feature reports"
    );

    Ok(())
}

// ─── Scenario 10: non-wheelbase shutdown is a no-op ───────────────────────────

#[test]
fn scenario_non_wheelbase_shutdown_is_noop() -> Result<(), Box<dyn std::error::Error>> {
    // Given: unknown/accessory PID
    let mut s = FanatecScenario::wheelbase(0xFF00);

    // When: shutdown
    s.shutdown()?;

    // Then: no reports sent at all
    assert_eq!(s.device.output_reports().len(), 0);
    assert_eq!(s.device.feature_reports().len(), 0);

    Ok(())
}

// ─── Scenario 11: extended telemetry report parses temperature and faults ─────

#[test]
fn scenario_extended_report_parses_telemetry() -> Result<(), Box<dyn std::error::Error>> {
    // Given: a raw 64-byte extended telemetry report (ID 0x02)
    let mut raw = [0u8; 64];
    raw[0] = report_ids::EXTENDED_INPUT; // report ID 0x02
    // Steering velocity (bytes 3–4): ignored in this test
    raw[5] = 82;   // motor temperature: 82 °C
    raw[6] = 41;   // board temperature: 41 °C
    raw[7] = 15;   // current draw: 1.5 A (in 0.1 A units)
    raw[10] = 0x03; // fault_flags: over-temp (bit 0) + over-current (bit 1)

    // When: parsed
    use racing_wheel_hid_fanatec_protocol::parse_extended_report;
    let state = parse_extended_report(&raw).ok_or("parse failed")?;

    // Then: fields match the raw bytes
    assert_eq!(state.motor_temp_c, 82, "motor temperature must match");
    assert_eq!(state.board_temp_c, 41, "board temperature must match");
    assert_eq!(state.current_raw, 15, "current draw must match");
    assert_eq!(
        state.fault_flags & 0x03,
        0x03,
        "over-temp and over-current bits must be set"
    );

    Ok(())
}

// ─── Scenario 12: LED report encodes bitmask and brightness correctly ─────────

#[test]
fn scenario_led_report_encoding() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_fanatec_protocol::build_led_report;

    // Given: 8 rev-lights lit (low byte = 0xFF) at 75 % brightness (≈ 191)
    let report = build_led_report(0x00FF, 191);

    // Then: wire layout is [0x08, 0x80, bitmask_lo, bitmask_hi, brightness, 0, 0, 0]
    assert_eq!(report[0], 0x08, "byte 0 must be LED_DISPLAY report ID");
    assert_eq!(report[1], 0x80, "byte 1 must be REV_LIGHTS command");
    assert_eq!(report[2], 0xFF, "byte 2 must be bitmask low byte");
    assert_eq!(report[3], 0x00, "byte 3 must be bitmask high byte");
    assert_eq!(report[4], 191, "byte 4 must be brightness");
    assert_eq!(&report[5..], &[0u8; 3], "trailing bytes must be zero");

    Ok(())
}

// ─── Scenario 13: display report carries gear digit and brightness ─────────────

#[test]
fn scenario_display_report_encoding() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_fanatec_protocol::build_display_report;

    // Given: display gear "3" at full brightness
    let report = build_display_report(0x00, [b'3', b' ', b' '], 255);

    // Then: layout is [0x08, 0x81, mode, d0, d1, d2, brightness, 0]
    assert_eq!(report[0], 0x08, "byte 0 must be LED_DISPLAY report ID");
    assert_eq!(report[1], 0x81, "byte 1 must be DISPLAY command");
    assert_eq!(report[2], 0x00, "byte 2 must be mode");
    assert_eq!(report[3], b'3', "byte 3 must be digit 0");
    assert_eq!(report[6], 255, "byte 6 must be brightness");
    assert_eq!(report[7], 0, "byte 7 must be reserved zero");

    Ok(())
}

// ─── Scenario 14: rumble report encodes left/right intensity and duration ──────

#[test]
fn scenario_rumble_report_encoding() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_fanatec_protocol::build_rumble_report;

    // Given: both motors at max for 200 ms (= 20 × 10 ms units)
    let report = build_rumble_report(255, 255, 20);

    // Then: layout is [0x08, 0x82, left, right, duration, 0, 0, 0]
    assert_eq!(report[0], 0x08, "byte 0 must be LED_DISPLAY report ID");
    assert_eq!(report[1], 0x82, "byte 1 must be RUMBLE command");
    assert_eq!(report[2], 255, "byte 2 must be left motor intensity");
    assert_eq!(report[3], 255, "byte 3 must be right motor intensity");
    assert_eq!(report[4], 20, "byte 4 must be duration (×10 ms)");
    assert_eq!(&report[5..], &[0u8; 3], "trailing bytes must be zero");

    Ok(())
}

// ─── Scenario 15: CSL DD sends mode-switch on initialize ──────────────────────

#[test]
fn scenario_csl_dd_sends_mode_switch_on_initialize() -> Result<(), Box<dyn std::error::Error>> {
    // Given: CSL DD wheelbase (primary PID 0x0020)
    let mut s = FanatecScenario::wheelbase(product_ids::CSL_DD);

    // When: initialized
    s.initialize()?;

    // Then: exactly one mode-switch feature report sent
    assert_eq!(
        s.device.feature_reports().len(),
        1,
        "CSL DD must send exactly one mode-switch on init"
    );
    assert!(
        s.device.sent_feature_report_id(report_ids::MODE_SWITCH),
        "CSL DD init report must carry MODE_SWITCH report ID"
    );

    Ok(())
}
