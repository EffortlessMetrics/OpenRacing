//! BDD end-to-end tests for the Simagic protocol stack.
//!
//! Each test follows a Given/When/Then pattern to verify observable hardware-ready
//! behaviors without real USB hardware.

use racing_wheel_engine::hid::vendor::simagic::product_ids as engine_product_ids;
use racing_wheel_hid_simagic_protocol::{
    CONSTANT_FORCE_REPORT_LEN, SimagicConstantForceEncoder, product_ids,
};
use racing_wheel_integration_tests::simagic_virtual::SimagicScenario;

// ─── Scenario 1: EVO device sends gain + rotation range on init ──────────────

#[test]
fn scenario_evo_sends_gain_and_range_on_init() -> Result<(), Box<dyn std::error::Error>> {
    // Given: EVO Sport (VID 0x3670)
    let mut s = SimagicScenario::evo(product_ids::EVO_SPORT);

    // When: initialized
    s.initialize()?;

    // Then: exactly 2 feature reports (gain + rotation range)
    assert_eq!(
        s.device.feature_reports().len(),
        2,
        "EVO init must send gain and rotation range"
    );

    Ok(())
}

// ─── Scenario 2: EVO Pro also sends 2 init reports ──────────────────────────

#[test]
fn scenario_evo_pro_sends_init() -> Result<(), Box<dyn std::error::Error>> {
    // Given: EVO Pro
    let mut s = SimagicScenario::evo(product_ids::EVO_PRO);

    // When: initialized
    s.initialize()?;

    // Then: 2 feature reports
    assert_eq!(s.device.feature_reports().len(), 2);

    Ok(())
}

// ─── Scenario 3: legacy Alpha skips active init (passive mode) ──────────────

#[test]
fn scenario_legacy_alpha_passive_mode() -> Result<(), Box<dyn std::error::Error>> {
    // Given: Alpha (legacy VID 0x0483)
    let mut s = SimagicScenario::legacy(engine_product_ids::ALPHA);

    // When: initialized
    s.initialize()?;

    // Then: no feature reports (passive mode for legacy devices)
    assert_eq!(
        s.device.feature_reports().len(),
        0,
        "Legacy Simagic must use passive mode (no init reports)"
    );

    Ok(())
}

// ─── Scenario 4: legacy Alpha Mini passive ──────────────────────────────────

#[test]
fn scenario_legacy_alpha_mini_passive() -> Result<(), Box<dyn std::error::Error>> {
    // Given: Alpha Mini (legacy VID 0x0483)
    let mut s = SimagicScenario::legacy(engine_product_ids::ALPHA_MINI);

    // When: initialized
    s.initialize()?;

    // Then: no feature reports
    assert_eq!(s.device.feature_reports().len(), 0);

    Ok(())
}

// ─── Scenario 5: I/O failure propagates from EVO init ───────────────────────

#[test]
fn scenario_evo_init_error_on_io_failure() {
    // Given: failing EVO device
    let mut s = SimagicScenario::evo_failing(product_ids::EVO_SPORT);

    // When: initialized
    let result = s.initialize();

    // Then: returns Err
    assert!(result.is_err(), "I/O failure must propagate from EVO init");
}

// ─── Scenario 6: legacy init does not fail even with failing device ─────────

#[test]
fn scenario_legacy_init_no_writes_no_failure() -> Result<(), Box<dyn std::error::Error>> {
    // Given: failing legacy device (but init sends nothing in passive mode)
    let mut s = SimagicScenario::legacy_failing(engine_product_ids::ALPHA);

    // When: initialized
    s.initialize()?;

    // Then: no reports and no error (passive mode writes nothing)
    assert_eq!(s.device.feature_reports().len(), 0);

    Ok(())
}

// ─── Scenario 7: constant force encoder max torque ──────────────────────────

#[test]
fn scenario_constant_force_max_torque_encoding() {
    // Given: Alpha Ultimate encoder at 25 Nm
    let max_nm: f32 = 25.0;
    let encoder = SimagicConstantForceEncoder::new(max_nm);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];

    // When: encoding maximum positive torque
    let written = encoder.encode(max_nm, &mut out);

    // Then: report length is correct
    assert_eq!(written, CONSTANT_FORCE_REPORT_LEN);
}

// ─── Scenario 8: zero torque produces neutral output ────────────────────────

#[test]
fn scenario_constant_force_zero_torque() {
    // Given: Alpha encoder at 10 Nm
    let encoder = SimagicConstantForceEncoder::new(10.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];

    // When: encoding zero torque
    encoder.encode(0.0, &mut out);

    // Then: force bytes are zero
    let force = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(force, 0, "zero torque must encode to 0");
}

// ─── Scenario 9: EVO unknown device sends conservative init ─────────────────

#[test]
fn scenario_evo_unknown_sends_conservative_init() -> Result<(), Box<dyn std::error::Error>> {
    // Given: unknown EVO PID
    let mut s = SimagicScenario::evo(0xFFFF);

    // When: initialized
    s.initialize()?;

    // Then: still sends 2 reports (conservative mode)
    assert_eq!(
        s.device.feature_reports().len(),
        2,
        "Unknown EVO device must still send conservative init"
    );

    Ok(())
}

// ─── Scenario 10: disconnect, reconnect, reinitialize ──────────────────────

#[test]
fn scenario_disconnect_reconnect_reinitialize() -> Result<(), Box<dyn std::error::Error>> {
    // Given: initialized EVO Sport
    let mut s = SimagicScenario::evo(product_ids::EVO_SPORT);
    s.initialize()?;
    assert_eq!(s.device.feature_reports().len(), 2);

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
        2,
        "reinitialize after reconnect must send 2 reports again"
    );

    Ok(())
}
