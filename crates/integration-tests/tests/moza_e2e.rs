//! BDD end-to-end tests for the Moza protocol stack.
//!
//! Each test follows a Given/When/Then pattern to verify observable hardware-ready
//! behaviors without real USB hardware.

use racing_wheel_hid_moza_protocol::{FfbMode, MozaInitState, product_ids, report_ids};
use racing_wheel_integration_tests::moza_virtual::MozaScenario;

// ─── Scenario 1: wheelbase handshake ─────────────────────────────────────────

#[test]
fn scenario_wheelbase_only_handshake_with_high_torque() -> Result<(), Box<dyn std::error::Error>> {
    // Given: R5 V1 wheelbase with high_torque enabled
    let mut s = MozaScenario::wheelbase_with_config(product_ids::R5_V1, FfbMode::Standard, true);

    // When: initialized
    s.initialize()?;

    // Then: state is Ready
    assert_eq!(s.protocol.init_state(), MozaInitState::Ready);

    // Then: exact handshake sequence [HIGH_TORQUE, START_REPORTS, FFB_MODE]
    let reports = s.device.feature_reports();
    assert_eq!(reports.len(), 3);
    assert_eq!(reports[0][0], report_ids::HIGH_TORQUE);
    assert_eq!(reports[1][0], report_ids::START_REPORTS);
    assert_eq!(reports[2][0], report_ids::FFB_MODE);
    assert_eq!(reports[2][1], FfbMode::Standard as u8);

    Ok(())
}

// ─── Scenario 2: high torque not sent by default ─────────────────────────────

#[test]
fn scenario_high_torque_not_sent_by_default() -> Result<(), Box<dyn std::error::Error>> {
    // Given: R9 V2, default config (high_torque_enabled = false)
    let mut s = MozaScenario::wheelbase(product_ids::R9_V2);
    assert!(!s.protocol.is_high_torque_enabled());

    // When: initialized
    s.initialize()?;

    // Then: HIGH_TORQUE report is NOT in the sequence
    assert!(!s.device.sent_feature_report_id(report_ids::HIGH_TORQUE));

    // Then: START_REPORTS and FFB_MODE are still sent
    assert!(s.device.sent_feature_report_id(report_ids::START_REPORTS));
    assert!(s.device.sent_feature_report_id(report_ids::FFB_MODE));

    // Then: only 2 reports total
    assert_eq!(s.device.feature_reports().len(), 2);

    Ok(())
}

// ─── Scenario 3: high torque sent when explicitly enabled ────────────────────

#[test]
fn scenario_high_torque_sent_when_explicitly_enabled() -> Result<(), Box<dyn std::error::Error>> {
    // Given: R9 V2, high_torque_enabled = true
    let mut s = MozaScenario::wheelbase_with_config(product_ids::R9_V2, FfbMode::Standard, true);
    assert!(s.protocol.is_high_torque_enabled());

    // When: initialized
    s.initialize()?;

    // Then: HIGH_TORQUE IS first in the sequence
    let reports = s.device.feature_reports();
    assert_eq!(reports.len(), 3);
    assert_eq!(reports[0][0], report_ids::HIGH_TORQUE);

    Ok(())
}

// ─── Scenario 4: FFB not ready before handshake ──────────────────────────────

#[test]
fn scenario_ffb_not_ready_before_handshake() {
    // Given: wheelbase, not yet initialized
    let s = MozaScenario::wheelbase(product_ids::R5_V2);

    // Then: is_ffb_ready returns false
    assert!(!s.protocol.is_ffb_ready());
    assert_eq!(s.protocol.init_state(), MozaInitState::Uninitialized);
}

// ─── Scenario 5: FFB becomes ready after handshake ───────────────────────────

#[test]
fn scenario_ffb_ready_after_successful_handshake() -> Result<(), Box<dyn std::error::Error>> {
    // Given: R5 V1 wheelbase
    let mut s = MozaScenario::wheelbase(product_ids::R5_V1);
    assert!(!s.protocol.is_ffb_ready());

    // When: initialized
    s.initialize()?;

    // Then: is_ffb_ready returns true
    assert!(s.protocol.is_ffb_ready());

    Ok(())
}

// ─── Scenario 6: handshake retry on transient IO failure ─────────────────────

#[test]
fn scenario_handshake_retry_on_transient_io_failure() -> Result<(), Box<dyn std::error::Error>> {
    // Given: device with IO failures (simulated)
    let mut s = MozaScenario::wheelbase_failing(product_ids::R5_V1);

    // When: first attempt fails
    s.initialize()?; // returns Ok even on failure (graceful)
    assert_eq!(s.protocol.init_state(), MozaInitState::Failed);
    assert!(
        s.protocol.can_retry(),
        "should be able to retry after first failure"
    );

    // Given: device recovers
    s.device.reconnect();

    // When: retry succeeds
    s.initialize()?;

    // Then: state is Ready
    assert_eq!(s.protocol.init_state(), MozaInitState::Ready);
    assert!(s.protocol.is_ffb_ready());

    Ok(())
}

// ─── Scenario 7: retries bounded by max_retries ──────────────────────────────

#[test]
fn scenario_retries_bounded_no_deadlock() -> Result<(), Box<dyn std::error::Error>> {
    // Given: always-failing device
    let mut s = MozaScenario::wheelbase_failing(product_ids::R5_V1);

    // When: exhausting retries (DEFAULT_MAX_RETRIES = 3)
    for _ in 0..3 {
        s.initialize()?;
    }

    // Then: state is PermanentFailure (not deadlocked)
    assert_eq!(s.protocol.init_state(), MozaInitState::PermanentFailure);
    assert!(!s.protocol.can_retry());

    // Then: further calls are no-ops (no new reports)
    let report_count_before = s.device.feature_reports().len();
    s.initialize()?;
    assert_eq!(s.device.feature_reports().len(), report_count_before);

    Ok(())
}

// ─── Scenario 8: disconnect resets handshake ─────────────────────────────────

#[test]
fn scenario_disconnect_resets_handshake() -> Result<(), Box<dyn std::error::Error>> {
    // Given: Ready device
    let mut s = MozaScenario::wheelbase(product_ids::R5_V1);
    s.initialize()?;
    assert_eq!(s.protocol.init_state(), MozaInitState::Ready);

    // When: disconnect
    s.device.disconnect();
    s.protocol.reset_to_uninitialized();

    // Then: state is Uninitialized
    assert_eq!(s.protocol.init_state(), MozaInitState::Uninitialized);
    assert!(!s.protocol.is_ffb_ready());

    // Then: reconnect + re-initialize succeeds
    s.device.reconnect();
    s.initialize()?;
    assert_eq!(s.protocol.init_state(), MozaInitState::Ready);

    Ok(())
}

// ─── Scenario 9: peripheral skips initialization ─────────────────────────────

#[test]
fn scenario_peripheral_device_skips_handshake() -> Result<(), Box<dyn std::error::Error>> {
    // Given: HBP handbrake (peripheral)
    let mut s = MozaScenario::wheelbase(product_ids::HBP_HANDBRAKE);

    // When: initialize called
    s.initialize()?;

    // Then: no feature reports sent
    assert!(
        s.device.feature_reports().is_empty(),
        "peripheral should not receive handshake"
    );

    // Then: state stays Uninitialized (peripheral not tracked)
    assert_eq!(s.protocol.init_state(), MozaInitState::Uninitialized);

    Ok(())
}

// ─── Scenario 10: direct torque FFB mode ─────────────────────────────────────

#[test]
fn scenario_direct_torque_ffb_mode_sets_mode_byte() -> Result<(), Box<dyn std::error::Error>> {
    // Given: R12 V2 in direct torque mode
    let mut s = MozaScenario::wheelbase_with_config(product_ids::R12_V2, FfbMode::Direct, false);

    // When: initialized
    s.initialize()?;

    // Then: FFB_MODE report contains Direct mode byte (0x02)
    let ffb_reports = s.device.feature_reports_with_id(report_ids::FFB_MODE);
    assert_eq!(ffb_reports.len(), 1);
    assert_eq!(ffb_reports[0][1], FfbMode::Direct as u8);

    Ok(())
}
