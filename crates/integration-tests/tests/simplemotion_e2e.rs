//! BDD end-to-end tests for the SimpleMotion V2 protocol stack.
//!
//! Each test follows a Given/When/Then pattern to verify observable hardware-ready
//! behaviors without real USB hardware.

use racing_wheel_integration_tests::simplemotion_virtual::SimpleMotionScenario;
use racing_wheel_simplemotion_v2::{
    ARGON_PRODUCT_ID, IONI_PRODUCT_ID, IONI_PRODUCT_ID_PREMIUM, IONI_VENDOR_ID,
    is_wheelbase_product,
};

/// Command type byte for `SetParameter` used by `build_device_enable`.
///
/// Verified by the existing unit test in simplemotion-v2 output.rs.
const SET_PARAMETER_CMD_TYPE: u8 = 0x02;

/// Report ID for all SimpleMotion V2 output reports.
const SM_OUTPUT_REPORT_ID: u8 = 0x01;

// ─── Scenario 1: IONI initialize sends one output report ─────────────────────

#[test]
fn scenario_ioni_initialize_sends_output_report() -> Result<(), Box<dyn std::error::Error>> {
    // Given: IONI (Simucube 1) scenario
    let mut s = SimpleMotionScenario::ioni();

    // When: initialized
    s.initialize()?;

    // Then: exactly one output report (device enable)
    assert_eq!(
        s.device.output_reports().len(),
        1,
        "IONI initialize must send exactly one output report"
    );

    // Then: no feature reports (SimpleMotion uses output reports for init)
    assert_eq!(
        s.device.feature_reports().len(),
        0,
        "IONI initialize must NOT send feature reports"
    );

    Ok(())
}

// ─── Scenario 2: output report starts with 0x01 ───────────────────────────────

#[test]
fn scenario_output_report_starts_with_report_id() -> Result<(), Box<dyn std::error::Error>> {
    // Given: IONI scenario
    let mut s = SimpleMotionScenario::ioni();

    // When: initialized
    s.initialize()?;

    // Then: first byte of output report is 0x01 (report ID)
    let report = &s.device.output_reports()[0];
    assert_eq!(
        report[0], SM_OUTPUT_REPORT_ID,
        "output report must start with report ID 0x01"
    );

    Ok(())
}

// ─── Scenario 3: output report is SetParameter command ───────────────────────

#[test]
fn scenario_output_report_is_set_parameter() -> Result<(), Box<dyn std::error::Error>> {
    // Given: IONI scenario
    let mut s = SimpleMotionScenario::ioni();

    // When: initialized
    s.initialize()?;

    // Then: byte[2] of output report is SetParameter (0x02)
    let report = &s.device.output_reports()[0];
    assert_eq!(
        report[2], SET_PARAMETER_CMD_TYPE,
        "initialization command must be SetParameter (0x02)"
    );

    Ok(())
}

// ─── Scenario 4: IONI_PREMIUM initializes with one output report ──────────────

#[test]
fn scenario_ioni_premium_initializes() -> Result<(), Box<dyn std::error::Error>> {
    // Given: IONI Premium (Simucube 2) scenario
    let mut s = SimpleMotionScenario::ioni_premium();

    // When: initialized
    s.initialize()?;

    // Then: one output report
    assert_eq!(s.device.output_reports().len(), 1);
    assert_eq!(s.device.output_reports()[0][0], SM_OUTPUT_REPORT_ID);

    Ok(())
}

// ─── Scenario 5: ARGON initializes with one output report ─────────────────────

#[test]
fn scenario_argon_initializes() -> Result<(), Box<dyn std::error::Error>> {
    // Given: ARGON (Simucube Sport) scenario
    let mut s = SimpleMotionScenario::argon();

    // When: initialized
    s.initialize()?;

    // Then: one output report
    assert_eq!(s.device.output_reports().len(), 1);

    Ok(())
}

// ─── Scenario 6: initialize returns Err on I/O failure ───────────────────────

#[test]
fn scenario_initialize_returns_error_on_io_failure() {
    // Given: always-failing device
    let mut s = SimpleMotionScenario::ioni_failing();

    // When / Then: I/O failure propagates
    assert!(
        s.initialize().is_err(),
        "I/O failure must propagate from initialize"
    );
}

// ─── Scenario 7: shutdown sends no output reports ─────────────────────────────

#[test]
fn scenario_shutdown_is_no_op() -> Result<(), Box<dyn std::error::Error>> {
    // Given: initialized IONI scenario
    let mut s = SimpleMotionScenario::ioni();
    s.initialize()?;
    s.device.clear_records();

    // When: shutdown
    s.shutdown()?;

    // Then: shutdown is a no-op (no output reports sent)
    assert_eq!(
        s.device.output_reports().len(),
        0,
        "shutdown must not send any reports"
    );
    assert_eq!(s.device.feature_reports().len(), 0);

    Ok(())
}

// ─── Scenario 8: device identity – IONI is Simucube 1 ────────────────────────

#[test]
fn scenario_ioni_identity_is_simucube_1() -> Result<(), Box<dyn std::error::Error>> {
    // Given: IONI handler
    let s = SimpleMotionScenario::ioni();

    // When: identity queried
    let id = s.protocol.identity();

    // Then: name contains Simucube and max torque is 15 Nm
    assert!(
        id.name.contains("Simucube") || id.name.contains("IONI"),
        "IONI name must reference Simucube or IONI, got: {}",
        id.name
    );
    assert_eq!(id.max_torque_nm, Some(15.0), "IONI max torque must be 15 Nm");
    assert!(id.supports_ffb, "IONI must support FFB");

    Ok(())
}

// ─── Scenario 9: device identity – IONI Premium is Simucube 2 ────────────────

#[test]
fn scenario_ioni_premium_identity_is_simucube_2() -> Result<(), Box<dyn std::error::Error>> {
    // Given: IONI Premium handler
    let s = SimpleMotionScenario::ioni_premium();

    // When: identity queried
    let id = s.protocol.identity();

    // Then: max torque is 35 Nm
    assert_eq!(
        id.max_torque_nm,
        Some(35.0),
        "IONI Premium max torque must be 35 Nm"
    );
    assert!(id.supports_ffb);

    Ok(())
}

// ─── Scenario 10: device identity – ARGON is Simucube Sport ──────────────────

#[test]
fn scenario_argon_identity_is_simucube_sport() -> Result<(), Box<dyn std::error::Error>> {
    // Given: ARGON handler
    let s = SimpleMotionScenario::argon();

    // When: identity queried
    let id = s.protocol.identity();

    // Then: max torque is 10 Nm
    assert_eq!(
        id.max_torque_nm,
        Some(10.0),
        "ARGON max torque must be 10 Nm"
    );
    assert!(id.supports_ffb);

    Ok(())
}

// ─── Scenario 11: is_wheelbase_product identifies all known PIDs ─────────────

#[test]
fn scenario_is_wheelbase_product_known_pids() -> Result<(), Box<dyn std::error::Error>> {
    // Then: all known PIDs are wheelbases
    assert!(is_wheelbase_product(IONI_PRODUCT_ID));
    assert!(is_wheelbase_product(IONI_PRODUCT_ID_PREMIUM));
    assert!(is_wheelbase_product(ARGON_PRODUCT_ID));

    // Then: unknown PID is not a wheelbase
    assert!(!is_wheelbase_product(0x0000));
    assert!(!is_wheelbase_product(0xFFFF));

    Ok(())
}

// ─── Scenario 12: torque encoder produces output reports ─────────────────────

#[test]
fn scenario_torque_encoder_sends_output_report() -> Result<(), Box<dyn std::error::Error>> {
    // Given: initialized IONI scenario
    let mut s = SimpleMotionScenario::ioni();
    s.initialize()?;
    s.device.clear_records();

    // When: torque command sent
    s.write_torque(5.0)?;

    // Then: one output report with report ID 0x01
    assert_eq!(
        s.device.output_reports().len(),
        1,
        "write_torque must produce one output report"
    );
    assert_eq!(s.device.output_reports()[0][0], SM_OUTPUT_REPORT_ID);

    Ok(())
}

// ─── Scenario 13: torque command type byte is SetTorque ───────────────────────

#[test]
fn scenario_torque_command_type_is_set_torque() -> Result<(), Box<dyn std::error::Error>> {
    // SetTorque = 0x10, as confirmed by the unit test in simplemotion-v2/output.rs
    const SET_TORQUE_CMD_TYPE: u8 = 0x10;

    // Given: initialized scenario
    let mut s = SimpleMotionScenario::ioni();
    s.initialize()?;
    s.device.clear_records();

    // When: torque command sent
    s.write_torque(0.0)?;

    // Then: command type byte is SetTorque (0x10)
    assert_eq!(
        s.device.output_reports()[0][2],
        SET_TORQUE_CMD_TYPE,
        "torque command must have SetTorque (0x10) at byte[2]"
    );

    Ok(())
}

// ─── Scenario 14: torque encoder sequence increments ─────────────────────────

#[test]
fn scenario_torque_encoder_sequence_increments() -> Result<(), Box<dyn std::error::Error>> {
    // Given: IONI scenario
    let mut s = SimpleMotionScenario::ioni();

    // When: two torque commands sent
    s.write_torque(0.0)?;
    s.write_torque(0.0)?;

    // Then: sequence bytes in successive reports differ by 1
    let seq0 = s.device.output_reports()[0][1];
    let seq1 = s.device.output_reports()[1][1];
    assert_eq!(
        seq0.wrapping_add(1),
        seq1,
        "sequence must increment by 1 between consecutive commands"
    );

    Ok(())
}

// ─── Scenario 15: get_vendor_protocol returns SimpleMotion for known PIDs ─────

#[test]
fn scenario_get_vendor_protocol_returns_simplemotion() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_engine::hid::vendor::get_vendor_protocol;

    // Given: all known SimpleMotion V2 PIDs
    for pid in [IONI_PRODUCT_ID, IONI_PRODUCT_ID_PREMIUM, ARGON_PRODUCT_ID] {
        let protocol = get_vendor_protocol(IONI_VENDOR_ID, pid);
        assert!(
            protocol.is_some(),
            "VID=0x1D50 / PID=0x{pid:04X} must be recognised"
        );
    }

    Ok(())
}

// ─── Scenario 16: is_v2_hardware returns true for IONI Premium and ARGON ──────

#[test]
fn scenario_is_v2_hardware() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_hid_moza_protocol::VendorProtocol;

    // Given: all three protocol variants
    let ioni = SimpleMotionScenario::ioni();
    let ioni_premium = SimpleMotionScenario::ioni_premium();
    let argon = SimpleMotionScenario::argon();

    // Then: IONI is V1 hardware; IONI Premium and ARGON are V2
    assert!(!ioni.protocol.is_v2_hardware(), "IONI must not be V2 hardware");
    assert!(
        ioni_premium.protocol.is_v2_hardware(),
        "IONI Premium must be V2 hardware"
    );
    assert!(argon.protocol.is_v2_hardware(), "ARGON must be V2 hardware");

    Ok(())
}

// ─── Scenario 17: disconnect → initialize fails; reconnect → succeeds ─────────

#[test]
fn scenario_disconnect_reconnect_reinitialize() -> Result<(), Box<dyn std::error::Error>> {
    // Given: initialized scenario
    let mut s = SimpleMotionScenario::ioni();
    s.initialize()?;

    // When: disconnect
    s.device.disconnect();

    // Then: initialize fails
    assert!(
        s.initialize().is_err(),
        "initialize must fail when disconnected"
    );

    // When: reconnect
    s.device.reconnect();
    s.device.clear_records();

    // Then: initialize succeeds again
    s.initialize()?;
    assert_eq!(s.device.output_reports().len(), 1);

    Ok(())
}
