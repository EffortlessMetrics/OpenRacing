//! BDD end-to-end tests for the FFBeast protocol stack.
//!
//! Each test follows a Given/When/Then pattern to verify observable hardware-ready
//! behaviors without real USB hardware.

use racing_wheel_hid_ffbeast_protocol::output::ENABLE_FFB_REPORT_ID;
use racing_wheel_hid_ffbeast_protocol::{
    FFBEAST_PRODUCT_ID_JOYSTICK, FFBEAST_PRODUCT_ID_RUDDER, FFBEAST_PRODUCT_ID_WHEEL,
    FFBEAST_VENDOR_ID, GAIN_REPORT_ID,
};
use racing_wheel_integration_tests::ffbeast_virtual::FFBeastScenario;

// ─── Scenario 1: wheel initialize sends enable-FFB and gain ──────────────────

#[test]
fn scenario_wheel_initialize_sends_enable_ffb_and_gain() -> Result<(), Box<dyn std::error::Error>> {
    // Given: FFBeast wheel
    let mut s = FFBeastScenario::wheel();

    // When: initialized
    s.initialize()?;

    // Then: exactly two feature reports (enable + gain)
    assert_eq!(
        s.device.feature_reports().len(),
        2,
        "expected exactly two feature reports on init"
    );
    assert!(s.device.sent_feature_report_id(ENABLE_FFB_REPORT_ID));
    assert!(s.device.sent_feature_report_id(GAIN_REPORT_ID));

    Ok(())
}

// ─── Scenario 2: enable FFB byte is 0x01 ─────────────────────────────────────

#[test]
fn scenario_enable_ffb_byte_is_0x01() -> Result<(), Box<dyn std::error::Error>> {
    // Given: FFBeast wheel
    let mut s = FFBeastScenario::wheel();

    // When: initialized
    s.initialize()?;

    // Then: first report enables FFB
    let enable_report = &s.device.feature_reports()[0];
    assert_eq!(enable_report[0], ENABLE_FFB_REPORT_ID);
    assert_eq!(
        enable_report[1], 0x01,
        "FFB must be enabled (byte 1 = 0x01)"
    );

    Ok(())
}

// ─── Scenario 3: gain set to maximum (0xFF) on init ──────────────────────────

#[test]
fn scenario_gain_set_to_maximum_on_init() -> Result<(), Box<dyn std::error::Error>> {
    // Given: FFBeast wheel
    let mut s = FFBeastScenario::wheel();

    // When: initialized
    s.initialize()?;

    // Then: gain report carries maximum scale
    let gain_reports = s.device.feature_reports_with_id(GAIN_REPORT_ID);
    assert_eq!(gain_reports.len(), 1);
    assert_eq!(
        gain_reports[0][1], 0xFF,
        "gain must be set to maximum (0xFF)"
    );

    Ok(())
}

// ─── Scenario 4: joystick and rudder PIDs also enumerate correctly ────────────

#[test]
fn scenario_joystick_pid_initializes() -> Result<(), Box<dyn std::error::Error>> {
    // Given: FFBeast joystick
    let mut s = FFBeastScenario::joystick();

    // When: initialized
    s.initialize()?;

    // Then: same two reports as wheel
    assert_eq!(s.device.feature_reports().len(), 2);

    Ok(())
}

#[test]
fn scenario_rudder_pid_initializes() -> Result<(), Box<dyn std::error::Error>> {
    // Given: FFBeast rudder
    let mut s = FFBeastScenario::rudder();

    // When: initialized
    s.initialize()?;

    // Then: same two reports
    assert_eq!(s.device.feature_reports().len(), 2);

    Ok(())
}

// ─── Scenario 5: initialize returns Err on I/O failure ───────────────────────

#[test]
fn scenario_initialize_returns_error_on_io_failure() {
    // Given: always-failing device
    let mut s = FFBeastScenario::wheel_failing();

    // Then: initialize propagates the error
    assert!(
        s.initialize().is_err(),
        "I/O failure must propagate from initialize"
    );
}

// ─── Scenario 6: shutdown disables FFB ───────────────────────────────────────

#[test]
fn scenario_shutdown_disables_ffb() -> Result<(), Box<dyn std::error::Error>> {
    // Given: successfully initialized wheel
    let mut s = FFBeastScenario::wheel();
    s.initialize()?;
    s.device.clear_records();

    // When: shutdown
    s.shutdown()?;

    // Then: exactly one feature report (disable FFB)
    assert_eq!(s.device.feature_reports().len(), 1);
    let report = &s.device.feature_reports()[0];
    assert_eq!(report[0], ENABLE_FFB_REPORT_ID);
    assert_eq!(report[1], 0x00, "byte 1 must be 0x00 to disable FFB");

    Ok(())
}

// ─── Scenario 7: disconnect → reconnect → reinitialize ───────────────────────

#[test]
fn scenario_disconnect_reconnect_reinitialize() -> Result<(), Box<dyn std::error::Error>> {
    // Given: initialized wheel
    let mut s = FFBeastScenario::wheel();
    s.initialize()?;

    // When: disconnect
    s.device.disconnect();
    assert!(!s.device.is_connected());

    // Then: initialize fails
    assert!(
        s.initialize().is_err(),
        "initialize must fail when disconnected"
    );

    // When: reconnect
    s.device.reconnect();
    s.device.clear_records();

    // Then: reinitialize succeeds
    s.initialize()?;
    assert_eq!(s.device.feature_reports().len(), 2);

    Ok(())
}

// ─── Scenario 8: torque encoder end-to-end ────────────────────────────────────

#[test]
fn scenario_torque_encoder_end_to_end() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_ffbeast_protocol::output::MAX_TORQUE_SCALE;
    use racing_wheel_hid_ffbeast_protocol::{
        CONSTANT_FORCE_REPORT_ID, CONSTANT_FORCE_REPORT_LEN, FFBeastTorqueEncoder,
    };

    // Given: FFBeast torque encoder
    let encoder = FFBeastTorqueEncoder;

    // When: encoding full positive, zero, and full negative
    let full_pos = encoder.encode(1.0);
    let zero = encoder.encode(0.0);
    let full_neg = encoder.encode(-1.0);

    // Then: correct report IDs and values
    assert_eq!(full_pos[0], CONSTANT_FORCE_REPORT_ID);
    assert_eq!(full_pos.len(), CONSTANT_FORCE_REPORT_LEN);
    assert_eq!(
        i16::from_le_bytes([full_pos[1], full_pos[2]]),
        MAX_TORQUE_SCALE
    );
    assert_eq!(i16::from_le_bytes([zero[1], zero[2]]), 0);
    assert_eq!(
        i16::from_le_bytes([full_neg[1], full_neg[2]]),
        -MAX_TORQUE_SCALE
    );

    Ok(())
}

// ─── Scenario 9: is_ffbeast_product correctly identifies all PIDs ─────────────

#[test]
fn scenario_product_id_detection() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_ffbeast_protocol::is_ffbeast_product;

    // Then: all known PIDs are recognised
    assert!(is_ffbeast_product(FFBEAST_PRODUCT_ID_WHEEL));
    assert!(is_ffbeast_product(FFBEAST_PRODUCT_ID_JOYSTICK));
    assert!(is_ffbeast_product(FFBEAST_PRODUCT_ID_RUDDER));

    // Then: unknown PID not recognised
    assert!(!is_ffbeast_product(0x0000));
    assert!(!is_ffbeast_product(0xFFFF));

    Ok(())
}

// ─── Scenario 10: get_vendor_protocol returns FFBeast for correct VID/PID ─────

#[test]
fn scenario_get_vendor_protocol_returns_ffbeast() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_engine::hid::vendor::get_vendor_protocol;

    // Given: all FFBeast PIDs
    for pid in [
        FFBEAST_PRODUCT_ID_WHEEL,
        FFBEAST_PRODUCT_ID_JOYSTICK,
        FFBEAST_PRODUCT_ID_RUDDER,
    ] {
        let protocol = get_vendor_protocol(FFBEAST_VENDOR_ID, pid);
        assert!(
            protocol.is_some(),
            "VID=0x045B / PID=0x{pid:04X} must be recognised"
        );
    }

    Ok(())
}

// ─── Scenario 11: unknown PID under FFBeast VID returns None ──────────────────

#[test]
fn scenario_unknown_pid_under_ffbeast_vid_returns_none() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_engine::hid::vendor::get_vendor_protocol;

    // Given: FFBeast VID with unknown PID
    let protocol = get_vendor_protocol(FFBEAST_VENDOR_ID, 0x0000);

    // Then: not recognised
    assert!(
        protocol.is_none(),
        "unknown PID under FFBeast VID must return None"
    );

    Ok(())
}

// ─── Scenario 12: feature report too large returns Err ────────────────────────

#[test]
fn scenario_feature_report_too_large_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_engine::hid::vendor::ffbeast::FFBeastHandler;
    use racing_wheel_hid_moza_protocol::VendorProtocol;

    // Given: handler
    let handler = FFBeastHandler::new(FFBEAST_VENDOR_ID, FFBEAST_PRODUCT_ID_WHEEL);
    let mut device = racing_wheel_integration_tests::ffbeast_virtual::VirtualFFBeastDevice::new(
        FFBEAST_VENDOR_ID,
        FFBEAST_PRODUCT_ID_WHEEL,
    );

    // When: 64 data bytes + 1 report ID = 65 bytes (exceeds limit)
    let oversized = vec![0u8; 64];
    let result = handler.send_feature_report(&mut device, 0x70, &oversized);

    // Then: Err returned
    assert!(result.is_err(), "oversized report must return Err");

    Ok(())
}
