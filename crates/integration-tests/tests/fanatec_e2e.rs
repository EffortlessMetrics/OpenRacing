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
