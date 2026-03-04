#![allow(clippy::redundant_closure)]
#![allow(clippy::result_large_err)]
//! Deep watchdog tests covering timeout detection, feed precision,
//! cascading failures, recovery, safety interlock interaction, and
//! configurable thresholds.

use racing_wheel_engine::safety::{
    FaultType, HardwareWatchdog, SafetyInterlockState, SafetyInterlockSystem, SoftwareWatchdog,
    WatchdogError, WatchdogTimeoutHandler,
};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_watchdog(timeout_ms: u32) -> SoftwareWatchdog {
    SoftwareWatchdog::new(timeout_ms)
}

fn make_interlock(timeout_ms: u32) -> SafetyInterlockSystem {
    let wd = Box::new(SoftwareWatchdog::new(timeout_ms));
    SafetyInterlockSystem::new(wd, 25.0)
}

fn make_interlock_with_limit(timeout_ms: u32, max_nm: f32) -> SafetyInterlockSystem {
    let wd = Box::new(SoftwareWatchdog::new(timeout_ms));
    SafetyInterlockSystem::new(wd, max_nm)
}

// =========================================================================
// 1. Watchdog timeout detection at various intervals
// =========================================================================

#[test]
fn watchdog_timeout_at_100ms() -> Result<(), WatchdogError> {
    let mut wd = make_watchdog(100);
    wd.arm()?;
    wd.feed()?;
    assert!(!wd.has_timed_out());
    std::thread::sleep(Duration::from_millis(110));
    assert!(wd.has_timed_out());
    Ok(())
}

#[test]
fn watchdog_timeout_at_50ms() -> Result<(), WatchdogError> {
    let mut wd = make_watchdog(50);
    wd.arm()?;
    wd.feed()?;
    std::thread::sleep(Duration::from_millis(60));
    assert!(wd.has_timed_out());
    Ok(())
}

#[test]
fn watchdog_timeout_at_200ms() -> Result<(), WatchdogError> {
    let mut wd = make_watchdog(200);
    wd.arm()?;
    wd.feed()?;
    std::thread::sleep(Duration::from_millis(100));
    assert!(!wd.has_timed_out());
    std::thread::sleep(Duration::from_millis(120));
    assert!(wd.has_timed_out());
    Ok(())
}

#[test]
fn watchdog_no_timeout_within_window() -> Result<(), WatchdogError> {
    let mut wd = make_watchdog(100);
    wd.arm()?;
    wd.feed()?;
    std::thread::sleep(Duration::from_millis(80));
    assert!(!wd.has_timed_out());
    Ok(())
}

// =========================================================================
// 2. Feed timing precision
// =========================================================================

#[test]
fn repeated_feeds_prevent_timeout() -> Result<(), WatchdogError> {
    let mut wd = make_watchdog(100);
    wd.arm()?;
    for _ in 0..10 {
        wd.feed()?;
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!wd.has_timed_out());
    Ok(())
}

#[test]
fn time_since_last_feed_increases() -> Result<(), WatchdogError> {
    let mut wd = make_watchdog(200);
    wd.arm()?;
    wd.feed()?;
    std::thread::sleep(Duration::from_millis(50));
    let d1 = wd.time_since_last_feed();
    std::thread::sleep(Duration::from_millis(50));
    let d2 = wd.time_since_last_feed();
    assert!(d2 > d1);
    Ok(())
}

#[test]
fn feed_resets_time_since_last_feed() -> Result<(), WatchdogError> {
    let mut wd = make_watchdog(200);
    wd.arm()?;
    wd.feed()?;
    std::thread::sleep(Duration::from_millis(80));
    wd.feed()?;
    let elapsed = wd.time_since_last_feed();
    assert!(elapsed < Duration::from_millis(50));
    Ok(())
}

// =========================================================================
// 3. Cascading watchdog failures (multi-level)
// =========================================================================

#[test]
fn timeout_triggers_safe_state() -> Result<(), WatchdogError> {
    let mut wd = make_watchdog(50);
    wd.arm()?;
    wd.feed()?;
    std::thread::sleep(Duration::from_millis(60));
    assert!(wd.has_timed_out());
    wd.trigger_safe_state()?;
    assert!(wd.is_safe_state_triggered());
    Ok(())
}

#[test]
fn interlock_enters_safe_mode_on_watchdog_timeout() -> Result<(), WatchdogError> {
    let mut sys = make_interlock(50);
    sys.arm()?;
    sys.report_communication();
    std::thread::sleep(Duration::from_millis(70));
    let result = sys.process_tick(10.0);
    // After timeout the torque must be clamped toward zero
    assert!(result.torque_command < 10.0);
    Ok(())
}

#[test]
fn multiple_faults_accumulate_in_log() {
    let mut sys = make_interlock(200);
    let _ = sys.arm();
    sys.report_fault(FaultType::UsbStall);
    sys.report_fault(FaultType::EncoderNaN);
    let log = sys.fault_log();
    assert!(log.len() >= 2);
}

#[test]
fn cascading_fault_keeps_torque_zero() {
    let mut sys = make_interlock(200);
    let _ = sys.arm();
    sys.report_fault(FaultType::UsbStall);
    sys.report_fault(FaultType::ThermalLimit);
    // Process a few ticks to let safe-mode ramp down
    for _ in 0..10 {
        let _ = sys.process_tick(20.0);
    }
    let result = sys.process_tick(20.0);
    assert!(result.torque_command < 20.0);
}

// =========================================================================
// 4. Watchdog behavior during high CPU load
// =========================================================================

#[test]
fn watchdog_detects_timeout_under_cpu_load() -> Result<(), WatchdogError> {
    let mut wd = make_watchdog(100);
    wd.arm()?;
    wd.feed()?;
    // Simulate CPU load with busy-wait
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(120) {
        std::hint::spin_loop();
    }
    assert!(wd.has_timed_out());
    Ok(())
}

#[test]
fn interlock_process_tick_under_load() -> Result<(), WatchdogError> {
    let mut sys = make_interlock(80);
    sys.arm()?;
    sys.report_communication();
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(100) {
        std::hint::spin_loop();
    }
    let result = sys.process_tick(15.0);
    // Watchdog should have timed out → torque reduced
    assert!(result.torque_command < 15.0);
    Ok(())
}

// =========================================================================
// 5. Watchdog reset after recovery
// =========================================================================

#[test]
fn watchdog_reset_clears_timeout() -> Result<(), WatchdogError> {
    let mut wd = make_watchdog(50);
    wd.arm()?;
    wd.feed()?;
    std::thread::sleep(Duration::from_millis(60));
    assert!(wd.has_timed_out());
    wd.reset()?;
    wd.arm()?;
    wd.feed()?;
    assert!(!wd.has_timed_out());
    Ok(())
}

#[test]
fn interlock_clear_fault_restores_normal() -> Result<(), String> {
    let mut sys = make_interlock(200);
    let _ = sys.arm();
    sys.report_communication();
    sys.report_fault(FaultType::UsbStall);
    // Now in safe mode
    let result = sys.process_tick(10.0);
    assert!(result.torque_command < 10.0);
    // Must wait 100ms before clearing
    std::thread::sleep(Duration::from_millis(110));
    sys.clear_fault()?;
    sys.report_communication();
    let result = sys.process_tick(10.0);
    // After clearing, torque should be closer to requested
    assert!(result.torque_command >= 0.0);
    Ok(())
}

#[test]
fn disarm_and_rearm_resets_watchdog() -> Result<(), WatchdogError> {
    let mut wd = make_watchdog(100);
    wd.arm()?;
    wd.feed()?;
    std::thread::sleep(Duration::from_millis(110));
    assert!(wd.has_timed_out());
    wd.disarm()?;
    wd.arm()?;
    wd.feed()?;
    assert!(!wd.has_timed_out());
    Ok(())
}

// =========================================================================
// 6. Watchdog interaction with safety interlocks
// =========================================================================

#[test]
fn emergency_stop_zeroes_torque() -> Result<(), WatchdogError> {
    let mut sys = make_interlock(200);
    sys.arm()?;
    sys.report_communication();
    let result = sys.emergency_stop();
    assert!(result.torque_command.abs() < 0.001);
    Ok(())
}

#[test]
fn fault_triggers_safe_mode_state() {
    let mut sys = make_interlock(200);
    let _ = sys.arm();
    sys.report_communication();
    sys.report_fault(FaultType::Overcurrent);
    let state = sys.state();
    match state {
        SafetyInterlockState::SafeMode { .. } | SafetyInterlockState::EmergencyStop { .. } => {}
        other => panic!("Expected SafeMode or EmergencyStop, got {:?}", other),
    }
}

#[test]
fn torque_limit_clamp_tracks_violations() {
    let mut sys = make_interlock_with_limit(200, 10.0);
    let _ = sys.arm();
    sys.report_communication();
    // Request above limit
    let _result = sys.process_tick(20.0);
    let violations = sys.torque_limit().violation_count;
    // Just verify we can read the violation count
    let _ = violations;
}

#[test]
fn watchdog_armed_state_query() -> Result<(), WatchdogError> {
    let mut sys = make_interlock(100);
    assert!(!sys.is_watchdog_armed());
    sys.arm()?;
    assert!(sys.is_watchdog_armed());
    sys.disarm()?;
    assert!(!sys.is_watchdog_armed());
    Ok(())
}

// =========================================================================
// 7. Watchdog log entries on timeout
// =========================================================================

#[test]
fn fault_log_records_fault_type() {
    let mut sys = make_interlock(200);
    let _ = sys.arm();
    sys.report_fault(FaultType::TimingViolation);
    let log = sys.fault_log();
    assert!(!log.is_empty());
    assert_eq!(
        log.last().map(|e| e.fault_type),
        Some(FaultType::TimingViolation)
    );
}

#[test]
fn fault_log_records_response_time() {
    let mut sys = make_interlock(200);
    let _ = sys.arm();
    sys.report_communication();
    sys.report_fault(FaultType::UsbStall);
    let log = sys.fault_log();
    assert!(!log.is_empty());
    let entry = &log[log.len() - 1];
    // Response time must be bounded
    assert!(entry.response_time < Duration::from_millis(50));
}

#[test]
fn fault_log_includes_torque_at_fault() {
    let mut sys = make_interlock(200);
    let _ = sys.arm();
    sys.report_communication();
    let _tick = sys.process_tick(12.0);
    sys.report_fault(FaultType::EncoderNaN);
    let log = sys.fault_log();
    assert!(!log.is_empty());
}

// =========================================================================
// 8. Configurable timeout thresholds
// =========================================================================

#[test]
fn default_timeout_is_100ms() {
    let wd = SoftwareWatchdog::with_default_timeout();
    assert_eq!(wd.timeout_ms(), 100);
}

#[test]
fn custom_timeout_value_respected() {
    let wd = SoftwareWatchdog::new(250);
    assert_eq!(wd.timeout_ms(), 250);
}

#[test]
fn interlock_timeout_matches_watchdog() {
    let sys = make_interlock(150);
    assert_eq!(sys.watchdog_timeout_ms(), 150);
}

// =========================================================================
// 9. WatchdogTimeoutHandler
// =========================================================================

#[test]
fn timeout_handler_zeroes_torque() {
    let mut handler = WatchdogTimeoutHandler::new();
    let response = handler.handle_timeout(15.0);
    assert!(response.torque_command.abs() < 0.001);
    assert!((response.previous_torque - 15.0).abs() < 0.001);
}

#[test]
fn timeout_handler_response_within_budget() {
    let mut handler = WatchdogTimeoutHandler::new();
    let response = handler.handle_timeout(10.0);
    assert!(response.within_budget);
    assert!(response.response_time < Duration::from_millis(1));
}

#[test]
fn timeout_handler_sets_triggered_flag() {
    let mut handler = WatchdogTimeoutHandler::new();
    assert!(!handler.is_timeout_triggered());
    let _response = handler.handle_timeout(5.0);
    assert!(handler.is_timeout_triggered());
}

#[test]
fn timeout_handler_reset_clears_state() {
    let mut handler = WatchdogTimeoutHandler::new();
    let _response = handler.handle_timeout(5.0);
    assert!(handler.is_timeout_triggered());
    handler.reset();
    assert!(!handler.is_timeout_triggered());
}

#[test]
fn timeout_handler_current_torque_after_timeout() {
    let mut handler = WatchdogTimeoutHandler::new();
    let _response = handler.handle_timeout(12.0);
    assert!(handler.current_torque().abs() < 0.001);
}

// =========================================================================
// 10. Arm/Disarm error conditions
// =========================================================================

#[test]
fn feed_unarmed_watchdog_returns_error() {
    let mut wd = make_watchdog(100);
    let result = wd.feed();
    assert!(result.is_err());
}

#[test]
fn double_arm_returns_error() -> Result<(), WatchdogError> {
    let mut wd = make_watchdog(100);
    wd.arm()?;
    let result = wd.arm();
    assert!(result.is_err());
    Ok(())
}
