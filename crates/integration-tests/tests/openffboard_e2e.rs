//! BDD end-to-end tests for the OpenFFBoard protocol stack.
//!
//! Each test follows a Given/When/Then pattern to verify observable hardware-ready
//! behaviors without real USB hardware.

use racing_wheel_hid_openffboard_protocol::output::ENABLE_FFB_REPORT_ID;
use racing_wheel_hid_openffboard_protocol::{
    GAIN_REPORT_ID, OPENFFBOARD_PRODUCT_ID, OPENFFBOARD_PRODUCT_ID_ALT, OPENFFBOARD_VENDOR_ID,
};
use racing_wheel_integration_tests::openffboard_virtual::OpenFFBoardScenario;

// ─── Scenario 1: initialize sends enable-FFB and gain reports ─────────────────

#[test]
fn scenario_initialize_sends_enable_ffb_and_gain() -> Result<(), Box<dyn std::error::Error>> {
    // Given: OpenFFBoard main firmware
    let mut s = OpenFFBoardScenario::wheelbase(OPENFFBOARD_PRODUCT_ID);

    // When: initialized
    s.initialize()?;

    // Then: exactly two feature reports sent (enable + gain)
    assert_eq!(
        s.device.feature_reports().len(),
        2,
        "expected exactly two feature reports on init"
    );

    // Then: first report enables FFB (ENABLE_FFB_REPORT_ID with 0x01)
    assert!(
        s.device.sent_feature_report_id(ENABLE_FFB_REPORT_ID),
        "must send ENABLE_FFB_REPORT_ID"
    );
    let enable_report = &s.device.feature_reports()[0];
    assert_eq!(
        enable_report[0], ENABLE_FFB_REPORT_ID,
        "byte 0 must be ENABLE_FFB_REPORT_ID"
    );
    assert_eq!(enable_report[1], 0x01, "byte 1 must enable FFB (0x01)");

    Ok(())
}

// ─── Scenario 2: gain is set to maximum on initialize ─────────────────────────

#[test]
fn scenario_initialize_sets_maximum_gain() -> Result<(), Box<dyn std::error::Error>> {
    // Given: OpenFFBoard main firmware
    let mut s = OpenFFBoardScenario::wheelbase(OPENFFBOARD_PRODUCT_ID);

    // When: initialized
    s.initialize()?;

    // Then: second feature report is a gain report at full scale (0xFF)
    let gain_reports = s.device.feature_reports_with_id(GAIN_REPORT_ID);
    assert_eq!(gain_reports.len(), 1, "exactly one gain report expected");
    assert_eq!(
        gain_reports[0][1], 0xFF,
        "gain must be set to maximum (0xFF)"
    );

    Ok(())
}

// ─── Scenario 3: alternate PID also initializes correctly ─────────────────────

#[test]
fn scenario_alt_pid_initializes_correctly() -> Result<(), Box<dyn std::error::Error>> {
    // Given: OpenFFBoard alternate firmware PID
    let mut s = OpenFFBoardScenario::wheelbase(OPENFFBOARD_PRODUCT_ID_ALT);

    // When: initialized
    s.initialize()?;

    // Then: same two reports as the main PID
    assert_eq!(s.device.feature_reports().len(), 2);
    assert!(s.device.sent_feature_report_id(ENABLE_FFB_REPORT_ID));
    assert!(s.device.sent_feature_report_id(GAIN_REPORT_ID));

    Ok(())
}

// ─── Scenario 4: initialize returns Err on I/O failure ────────────────────────

#[test]
fn scenario_initialize_returns_error_on_io_failure() {
    // Given: always-failing device
    let mut s = OpenFFBoardScenario::wheelbase_failing(OPENFFBOARD_PRODUCT_ID);

    // When / Then: initialize must propagate the error
    assert!(
        s.initialize().is_err(),
        "I/O failure must propagate from initialize"
    );
}

// ─── Scenario 5: shutdown sends FFB-disable report ────────────────────────────

#[test]
fn scenario_shutdown_sends_ffb_disable() -> Result<(), Box<dyn std::error::Error>> {
    // Given: successfully initialized wheelbase
    let mut s = OpenFFBoardScenario::wheelbase(OPENFFBOARD_PRODUCT_ID);
    s.initialize()?;
    s.device.clear_records();

    // When: shutdown
    s.shutdown()?;

    // Then: exactly one feature report (disable FFB)
    assert_eq!(
        s.device.feature_reports().len(),
        1,
        "shutdown must send exactly one feature report"
    );
    let report = &s.device.feature_reports()[0];
    assert_eq!(report[0], ENABLE_FFB_REPORT_ID);
    assert_eq!(report[1], 0x00, "byte 1 must be 0x00 to disable FFB");

    Ok(())
}

// ─── Scenario 6: disconnect → initialize fails; reconnect → succeeds ──────────

#[test]
fn scenario_disconnect_reconnect_reinitialize() -> Result<(), Box<dyn std::error::Error>> {
    // Given: successfully initialized wheelbase
    let mut s = OpenFFBoardScenario::wheelbase(OPENFFBOARD_PRODUCT_ID);
    s.initialize()?;

    // When: disconnect
    s.device.disconnect();
    assert!(!s.device.is_connected());

    // Then: reinitialize while disconnected must fail
    assert!(
        s.initialize().is_err(),
        "initialize must fail when device is disconnected"
    );

    // When: reconnect
    s.device.reconnect();
    s.device.clear_records();

    // Then: reinitialize succeeds again with the expected two reports
    s.initialize()?;
    assert_eq!(
        s.device.feature_reports().len(),
        2,
        "reinitialize after reconnect must send two feature reports"
    );

    Ok(())
}

// ─── Scenario 7: torque encoder full pipeline ─────────────────────────────────

#[test]
fn scenario_torque_encoder_full_pipeline() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_openffboard_protocol::output::MAX_TORQUE_SCALE;
    use racing_wheel_hid_openffboard_protocol::{
        CONSTANT_FORCE_REPORT_ID, CONSTANT_FORCE_REPORT_LEN, OpenFFBoardTorqueEncoder,
    };

    // Given: OpenFFBoard encoder
    let encoder = OpenFFBoardTorqueEncoder;

    // When: encoding maximum positive torque
    let report = encoder.encode(1.0);

    // Then: report layout is correct
    assert_eq!(report.len(), CONSTANT_FORCE_REPORT_LEN);
    assert_eq!(report[0], CONSTANT_FORCE_REPORT_ID);
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, MAX_TORQUE_SCALE, "full positive torque must be +10000");
    assert_eq!(report[3], 0x00);
    assert_eq!(report[4], 0x00);

    Ok(())
}

// ─── Scenario 8: torque encoder zero output ───────────────────────────────────

#[test]
fn scenario_torque_encoder_zero_output() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_openffboard_protocol::{
        CONSTANT_FORCE_REPORT_ID, OpenFFBoardTorqueEncoder,
    };

    // Given: encoder
    let encoder = OpenFFBoardTorqueEncoder;

    // When: encoding zero torque
    let report = encoder.encode(0.0);

    // Then: torque bytes are zero
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, 0, "zero torque must produce zero bytes");
    assert_eq!(report[0], CONSTANT_FORCE_REPORT_ID);

    Ok(())
}

// ─── Scenario 9: torque encoder negative (counter-steering) ───────────────────

#[test]
fn scenario_torque_encoder_negative_torque() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_openffboard_protocol::OpenFFBoardTorqueEncoder;
    use racing_wheel_hid_openffboard_protocol::output::MAX_TORQUE_SCALE;

    // Given: encoder
    let encoder = OpenFFBoardTorqueEncoder;

    // When: encoding maximum negative torque
    let report = encoder.encode(-1.0);

    // Then: raw i16 is -10000
    let raw = i16::from_le_bytes([report[1], report[2]]);
    assert_eq!(raw, -MAX_TORQUE_SCALE, "full negative must be -10000");

    Ok(())
}

// ─── Scenario 10: FFB config reports reasonable values ────────────────────────

#[test]
fn scenario_ffb_config_valid_ranges() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_engine::hid::vendor::openffboard::OpenFFBoardHandler;
    use racing_wheel_hid_moza_protocol::VendorProtocol;

    // Given: handler for main PID
    let handler = OpenFFBoardHandler::new(OPENFFBOARD_VENDOR_ID, OPENFFBOARD_PRODUCT_ID);

    // When: queried for FFB config
    let config = handler.get_ffb_config();

    // Then: sensible values
    assert!(
        config.max_torque_nm >= 1.0,
        "max torque should be at least 1 Nm, got {}",
        config.max_torque_nm
    );
    assert!(
        config.max_torque_nm <= 100.0,
        "max torque should be <= 100 Nm, got {}",
        config.max_torque_nm
    );
    assert!(
        config.encoder_cpr >= 100,
        "encoder CPR should be reasonable, got {}",
        config.encoder_cpr
    );

    Ok(())
}

// ─── Scenario 11: get_vendor_protocol returns OpenFFBoard for correct VID/PID ─

#[test]
fn scenario_get_vendor_protocol_returns_openffboard() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_engine::hid::vendor::get_vendor_protocol;

    // Given: VID/PID for OpenFFBoard main firmware
    let protocol = get_vendor_protocol(OPENFFBOARD_VENDOR_ID, OPENFFBOARD_PRODUCT_ID);

    // Then: protocol is recognised
    assert!(
        protocol.is_some(),
        "VID=0x1209 / PID=0xFFB0 must be recognised"
    );

    Ok(())
}

// ─── Scenario 12: get_vendor_protocol returns OpenFFBoard for alt PID ─────────

#[test]
fn scenario_get_vendor_protocol_returns_openffboard_alt_pid()
-> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_engine::hid::vendor::get_vendor_protocol;

    // Given: VID/PID for OpenFFBoard alt firmware
    let protocol = get_vendor_protocol(OPENFFBOARD_VENDOR_ID, OPENFFBOARD_PRODUCT_ID_ALT);

    // Then: protocol is recognised
    assert!(
        protocol.is_some(),
        "VID=0x1209 / PID=0xFFB1 must be recognised"
    );

    Ok(())
}

// ─── Scenario 13: feature report too large returns Err ────────────────────────

#[test]
fn scenario_feature_report_too_large_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_engine::hid::vendor::openffboard::OpenFFBoardHandler;
    use racing_wheel_hid_moza_protocol::VendorProtocol;

    // Given: handler
    let handler = OpenFFBoardHandler::new(OPENFFBOARD_VENDOR_ID, OPENFFBOARD_PRODUCT_ID);
    let mut device =
        racing_wheel_integration_tests::openffboard_virtual::VirtualOpenFFBoardDevice::new(
            OPENFFBOARD_VENDOR_ID,
            OPENFFBOARD_PRODUCT_ID,
        );

    // When: sending 64 data bytes (+ 1 report ID = 65, exceeds 64-byte max)
    let oversized = vec![0u8; 64];
    let result = handler.send_feature_report(&mut device, 0x70, &oversized);

    // Then: error returned
    assert!(result.is_err(), "oversized report must return Err");

    Ok(())
}
