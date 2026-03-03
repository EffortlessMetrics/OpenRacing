//! BDD end-to-end tests for the Thrustmaster protocol stack.
//!
//! Each test follows a Given/When/Then pattern to verify observable hardware-ready
//! behaviors without real USB hardware.

use racing_wheel_hid_thrustmaster_protocol::{
    EFFECT_REPORT_LEN, ThrustmasterConstantForceEncoder, build_actuator_enable, build_device_gain,
    build_set_range_report, output::report_ids, product_ids,
};
use racing_wheel_integration_tests::thrustmaster_virtual::ThrustmasterScenario;

// ─── Scenario 1: FFB wheel sends init sequence on initialize ─────────────────

#[test]
fn scenario_ffb_wheel_sends_init_on_initialize() -> Result<(), Box<dyn std::error::Error>> {
    // Given: T300RS wheelbase
    let mut s = ThrustmasterScenario::wheelbase(product_ids::T300_RS);

    // When: initialized
    s.initialize()?;

    // Then: exactly 4 feature reports sent (gain reset, gain full, enable, range)
    assert_eq!(
        s.device.feature_reports().len(),
        4,
        "T300RS init must send exactly 4 feature reports"
    );

    Ok(())
}

// ─── Scenario 2: non-FFB wheel skips init ────────────────────────────────────

#[test]
fn scenario_non_ffb_wheel_skips_init() -> Result<(), Box<dyn std::error::Error>> {
    // Given: T80 (no FFB support)
    let mut s = ThrustmasterScenario::wheelbase(product_ids::T80);

    // When: initialized
    s.initialize()?;

    // Then: no feature reports sent
    assert_eq!(
        s.device.feature_reports().len(),
        0,
        "T80 (no FFB) must not send any init reports"
    );

    Ok(())
}

// ─── Scenario 3: unknown PID skips init ──────────────────────────────────────

#[test]
fn scenario_unknown_pid_skips_init() -> Result<(), Box<dyn std::error::Error>> {
    // Given: unknown product ID
    let mut s = ThrustmasterScenario::wheelbase(0xFF00);

    // When: initialized
    s.initialize()?;

    // Then: no reports sent
    assert_eq!(s.device.feature_reports().len(), 0);
    Ok(())
}

// ─── Scenario 4: init reports have correct wire bytes ────────────────────────

#[test]
fn scenario_init_reports_correct_bytes() -> Result<(), Box<dyn std::error::Error>> {
    // Given: T300RS
    let mut s = ThrustmasterScenario::wheelbase(product_ids::T300_RS);

    // When: initialized
    s.initialize()?;

    let reports = s.device.feature_reports();

    // Then: first report is gain reset (0x81 = DEVICE_GAIN, 0x00)
    assert_eq!(
        reports[0],
        build_device_gain(0).to_vec(),
        "first report must be gain-reset"
    );

    // Then: second report is full gain (0x81, 0xFF)
    assert_eq!(
        reports[1],
        build_device_gain(0xFF).to_vec(),
        "second report must be full-gain"
    );

    // Then: third report is actuator enable (0x82, 0x01)
    assert_eq!(
        reports[2],
        build_actuator_enable(true).to_vec(),
        "third report must be actuator-enable"
    );

    // Then: fourth report is set-range
    let expected_range = build_set_range_report(1080).to_vec();
    assert_eq!(
        reports[3], expected_range,
        "fourth report must be set-range for 1080°"
    );

    Ok(())
}

// ─── Scenario 5: I/O failure propagates from init ────────────────────────────

#[test]
fn scenario_initialize_returns_error_on_io_failure() {
    // Given: always-failing device
    let mut s = ThrustmasterScenario::wheelbase_failing(product_ids::T300_RS);

    // When: initialized
    let result = s.initialize();

    // Then: returns Err
    assert!(result.is_err(), "I/O failure must propagate from init");
}

// ─── Scenario 6: shutdown sends actuator disable ─────────────────────────────

#[test]
fn scenario_shutdown_sends_actuator_disable() -> Result<(), Box<dyn std::error::Error>> {
    // Given: initialized T300RS
    let mut s = ThrustmasterScenario::wheelbase(product_ids::T300_RS);
    s.initialize()?;
    s.device.clear_records();

    // When: shutdown
    s.shutdown()?;

    // Then: exactly one feature report (actuator disable)
    assert_eq!(
        s.device.feature_reports().len(),
        1,
        "shutdown must send exactly one disable report"
    );
    assert_eq!(
        s.device.feature_reports()[0],
        build_actuator_enable(false).to_vec(),
        "shutdown must send actuator-disable"
    );

    Ok(())
}

// ─── Scenario 7: non-FFB shutdown is no-op ───────────────────────────────────

#[test]
fn scenario_non_ffb_shutdown_is_noop() -> Result<(), Box<dyn std::error::Error>> {
    // Given: T80 (no FFB)
    let mut s = ThrustmasterScenario::wheelbase(product_ids::T80);

    // When: shutdown
    s.shutdown()?;

    // Then: no reports
    assert_eq!(s.device.feature_reports().len(), 0);
    assert_eq!(s.device.output_reports().len(), 0);
    Ok(())
}

// ─── Scenario 8: constant force encoder max torque ───────────────────────────

#[test]
fn scenario_constant_force_max_torque_encoding() {
    // Given: T300RS encoder at 3.9 Nm
    let max_nm: f32 = 3.9;
    let encoder = ThrustmasterConstantForceEncoder::new(max_nm);
    let mut out = [0u8; EFFECT_REPORT_LEN];

    // When: encoding maximum positive torque
    let written = encoder.encode(max_nm, &mut out);

    // Then: report length is correct
    assert_eq!(written, EFFECT_REPORT_LEN);

    // Then: report ID byte is CONSTANT_FORCE
    assert_eq!(out[0], report_ids::CONSTANT_FORCE);
}

// ─── Scenario 9: zero torque produces neutral output ─────────────────────────

#[test]
fn scenario_constant_force_zero_torque_encoding() {
    // Given: T300RS encoder at 3.9 Nm
    let encoder = ThrustmasterConstantForceEncoder::new(3.9);
    let mut out = [0u8; EFFECT_REPORT_LEN];

    // When: encoding zero torque
    encoder.encode(0.0, &mut out);

    // Then: force value bytes are zero (bytes 2-3 in i16 LE)
    let force = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(force, 0, "zero torque must encode to 0");
}

// ─── Scenario 10: disconnect, reconnect, and reinitialize ────────────────────

#[test]
fn scenario_disconnect_reconnect_reinitialize() -> Result<(), Box<dyn std::error::Error>> {
    // Given: initialized T300RS
    let mut s = ThrustmasterScenario::wheelbase(product_ids::T300_RS);
    s.initialize()?;
    assert_eq!(s.device.feature_reports().len(), 4);

    // When: disconnect
    s.device.disconnect();
    assert!(!s.device.is_connected());

    // Then: init fails while disconnected
    assert!(s.initialize().is_err());

    // When: reconnect
    s.device.reconnect();
    s.device.clear_records();

    // Then: reinitialize succeeds
    s.initialize()?;
    assert_eq!(
        s.device.feature_reports().len(),
        4,
        "reinitialize after reconnect must send 4 init reports again"
    );

    Ok(())
}
