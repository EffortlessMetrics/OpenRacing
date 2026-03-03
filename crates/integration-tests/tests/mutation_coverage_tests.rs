//! Mutation coverage tests.
//!
//! Each test is specifically designed to catch a common mutation pattern:
//! off-by-one, wrong operator, missing negation, wrong return value,
//! missing boundary check, or wrong enum variant.
//!
//! These tests complement property-based tests by targeting the exact
//! code patterns that mutation testing tools (cargo-mutants) modify.

// ── Imports ─────────────────────────────────────────────────────────────────

use openracing_atomic::AtomicCounters;
use openracing_calibration::AxisCalibration;
use openracing_curves::CurveLut;
use openracing_errors::{ErrorSeverity, RTError};
use openracing_filters::{
    DamperState, Frame as FilterFrame, FrictionState, SlewRateState, damper_filter,
    friction_filter, slew_rate_filter, torque_cap_filter,
};
use openracing_fmea::{FaultAction, FaultType};

// ═════════════════════════════════════════════════════════════════════════════
// 1. Off-by-one errors
// ═════════════════════════════════════════════════════════════════════════════

/// Torque exactly at the cap boundary must be preserved (not clipped to cap − ε).
#[test]
fn torque_cap_at_exact_boundary_is_preserved() {
    let mut frame = FilterFrame::from_torque(0.5);
    torque_cap_filter(&mut frame, 0.5);
    assert!(
        (frame.torque_out - 0.5).abs() < f32::EPSILON,
        "Torque at exact cap boundary was altered: {}",
        frame.torque_out,
    );
}

/// Torque one ULP above the cap must be clamped *down* to the cap.
#[test]
fn torque_cap_just_above_boundary_is_clamped() {
    let cap = 0.5_f32;
    let just_above = f32::from_bits(cap.to_bits() + 1);
    let mut frame = FilterFrame::from_torque(just_above);
    torque_cap_filter(&mut frame, cap);
    assert!(
        frame.torque_out <= cap,
        "Torque just above cap was not clamped: {} > {}",
        frame.torque_out,
        cap,
    );
}

/// Calibration at raw == min must produce 0.0, not a small positive number.
#[test]
fn calibration_at_min_produces_zero() {
    let cal = AxisCalibration::new(0, 1000).with_deadzone(0, 1000);
    let value = cal.apply(0);
    assert!(
        value.abs() < f32::EPSILON,
        "apply(min) should be 0.0, got {}",
        value,
    );
}

/// Calibration at raw == max must produce 1.0, not slightly below.
#[test]
fn calibration_at_max_produces_one() {
    let cal = AxisCalibration::new(0, 1000).with_deadzone(0, 1000);
    let value = cal.apply(1000);
    assert!(
        (value - 1.0).abs() < f32::EPSILON,
        "apply(max) should be 1.0, got {}",
        value,
    );
}

/// LUT lookup at input=0.0 must return the first table entry exactly.
#[test]
fn lut_lookup_at_zero_returns_first_entry() {
    let lut = CurveLut::linear();
    let value = lut.lookup(0.0);
    let table = lut.table();
    assert!(
        (value - table[0]).abs() < f32::EPSILON,
        "lookup(0.0) = {}, table[0] = {}",
        value,
        table[0],
    );
}

/// LUT lookup at input=1.0 must return the last table entry exactly.
#[test]
fn lut_lookup_at_one_returns_last_entry() {
    let lut = CurveLut::linear();
    let value = lut.lookup(1.0);
    let table = lut.table();
    assert!(
        (value - table[CurveLut::SIZE - 1]).abs() < f32::EPSILON,
        "lookup(1.0) = {}, table[last] = {}",
        value,
        table[CurveLut::SIZE - 1],
    );
}

/// RTError discriminant codes must match the declared repr(u8) values 1–10.
#[test]
fn rt_error_codes_match_discriminants() {
    let expected: &[(RTError, u8)] = &[
        (RTError::DeviceDisconnected, 1),
        (RTError::TorqueLimit, 2),
        (RTError::PipelineFault, 3),
        (RTError::TimingViolation, 4),
        (RTError::RTSetupFailed, 5),
        (RTError::InvalidConfig, 6),
        (RTError::SafetyInterlock, 7),
        (RTError::BufferOverflow, 8),
        (RTError::DeadlineMissed, 9),
        (RTError::ResourceUnavailable, 10),
    ];
    for (err, code) in expected {
        assert_eq!(
            err.code(),
            *code,
            "{:?} should have code {}, got {}",
            err,
            code,
            err.code(),
        );
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 2. Wrong operator (< vs <=, + vs -, * vs /)
// ═════════════════════════════════════════════════════════════════════════════

/// Negative torque exceeding the cap (in magnitude) must be clamped to −cap.
#[test]
fn torque_cap_negative_side_clamped() {
    let mut frame = FilterFrame::from_torque(-0.8);
    torque_cap_filter(&mut frame, 0.5);
    assert!(
        (frame.torque_out - (-0.5)).abs() < f32::EPSILON,
        "Negative torque not clamped to -cap: {}",
        frame.torque_out,
    );
}

/// Positive torque exceeding the cap must be clamped to +cap.
#[test]
fn torque_cap_positive_side_clamped() {
    let mut frame = FilterFrame::from_torque(0.8);
    torque_cap_filter(&mut frame, 0.5);
    assert!(
        (frame.torque_out - 0.5).abs() < f32::EPSILON,
        "Positive torque not clamped to +cap: {}",
        frame.torque_out,
    );
}

/// Slew-rate filter must limit the per-tick *change*, not the absolute value.
#[test]
fn slew_rate_limits_change_not_absolute() {
    let mut state = SlewRateState {
        max_change_per_tick: 0.1,
        prev_output: 0.4,
    };
    // Request a large jump from prev_output 0.4 → 1.0
    let mut frame = FilterFrame::from_torque(1.0);
    slew_rate_filter(&mut frame, &mut state);
    // Should advance by at most max_change_per_tick
    assert!(
        (frame.torque_out - 0.5).abs() < f32::EPSILON,
        "Slew-rate should limit output to prev + max_change (0.5), got {}",
        frame.torque_out,
    );
}

/// Slew-rate filter must also limit downward changes symmetrically.
#[test]
fn slew_rate_limits_downward_change() {
    let mut state = SlewRateState {
        max_change_per_tick: 0.1,
        prev_output: 0.5,
    };
    let mut frame = FilterFrame::from_torque(0.0);
    slew_rate_filter(&mut frame, &mut state);
    assert!(
        (frame.torque_out - 0.4).abs() < f32::EPSILON,
        "Slew-rate should limit output to prev - max_change (0.4), got {}",
        frame.torque_out,
    );
}

/// Damper must subtract from torque (opposing motion), not add.
#[test]
fn damper_opposes_motion() {
    let state = DamperState {
        coefficient: 0.5,
        speed_adaptation: false,
    };
    let mut frame = FilterFrame {
        ffb_in: 0.0,
        torque_out: 0.6,
        wheel_speed: 1.0,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    let original_torque = frame.torque_out;
    damper_filter(&mut frame, &state);
    // Damper should reduce torque magnitude when wheel is moving
    assert!(
        frame.torque_out.abs() <= original_torque.abs(),
        "Damper should reduce torque from {}, got {}",
        original_torque,
        frame.torque_out,
    );
}

/// Telemetry loss percent: denominator must include both received + lost.
#[test]
fn telemetry_loss_percent_uses_total_denominator() {
    let counters = AtomicCounters::new();
    for _ in 0..80 {
        counters.inc_telemetry_received();
    }
    for _ in 0..20 {
        counters.inc_telemetry_lost();
    }
    let pct = counters.telemetry_loss_percent();
    // 20 lost / (80 + 20) total = 20%
    assert!(
        (pct - 20.0).abs() < 0.1,
        "Expected 20% loss, got {}",
        pct,
    );
}

/// Saturation percent: numerator is saturated count, denominator is total samples.
#[test]
fn torque_saturation_percent_arithmetic() {
    let counters = AtomicCounters::new();
    // 30 saturated out of 100 samples → 30%
    for i in 0..100 {
        counters.record_torque_saturation(i < 30);
    }
    let pct = counters.torque_saturation_percent();
    assert!(
        (pct - 30.0).abs() < 0.1,
        "Expected 30% saturation, got {}",
        pct,
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// 3. Missing negation
// ═════════════════════════════════════════════════════════════════════════════

/// Torque cap with negative input within range must be preserved, not negated.
#[test]
fn negative_torque_within_cap_preserved() {
    let mut frame = FilterFrame::from_torque(-0.3);
    torque_cap_filter(&mut frame, 0.5);
    assert!(
        (frame.torque_out - (-0.3)).abs() < f32::EPSILON,
        "Negative torque within cap should be preserved: expected -0.3, got {}",
        frame.torque_out,
    );
}

/// Friction with negative wheel speed should still apply friction force.
#[test]
fn friction_with_negative_speed() {
    let state = FrictionState {
        coefficient: 0.5,
        speed_adaptation: false,
    };
    let mut frame = FilterFrame {
        ffb_in: 0.0,
        torque_out: 0.6,
        wheel_speed: -2.0,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    let original = frame.torque_out;
    friction_filter(&mut frame, &state);
    // Friction should modify torque output (not leave it unchanged)
    assert!(
        (frame.torque_out - original).abs() > f32::EPSILON,
        "Friction should modify torque, but output {} == original {}",
        frame.torque_out,
        original,
    );
    // Output must remain finite
    assert!(
        frame.torque_out.is_finite(),
        "Friction output must be finite, got {}",
        frame.torque_out,
    );
}

/// FaultType requiring immediate response must not be inverted with
/// those that do not.
#[test]
fn fault_type_immediate_response_not_inverted() {
    // These MUST require immediate response (safety-critical)
    let immediate = [
        FaultType::Overcurrent,
        FaultType::ThermalLimit,
        FaultType::UsbStall,
        FaultType::EncoderNaN,
        FaultType::SafetyInterlockViolation,
        FaultType::HandsOffTimeout,
    ];
    for ft in &immediate {
        assert!(
            ft.requires_immediate_response(),
            "{:?} must require immediate response",
            ft,
        );
    }

    // These must NOT require immediate response
    let non_immediate = [
        FaultType::PluginOverrun,
        FaultType::TimingViolation,
        FaultType::PipelineFault,
    ];
    for ft in &non_immediate {
        assert!(
            !ft.requires_immediate_response(),
            "{:?} must NOT require immediate response",
            ft,
        );
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 4. Wrong return value
// ═════════════════════════════════════════════════════════════════════════════

/// FaultType severity must return the documented values for every variant.
#[test]
fn fault_type_severity_exact_values() {
    // Critical severity (1)
    assert_eq!(FaultType::Overcurrent.severity(), 1);
    assert_eq!(FaultType::ThermalLimit.severity(), 1);

    // High severity (2)
    assert_eq!(FaultType::UsbStall.severity(), 2);
    assert_eq!(FaultType::EncoderNaN.severity(), 2);
    assert_eq!(FaultType::SafetyInterlockViolation.severity(), 2);
    assert_eq!(FaultType::HandsOffTimeout.severity(), 2);

    // Medium severity (3)
    assert_eq!(FaultType::PluginOverrun.severity(), 3);
    assert_eq!(FaultType::TimingViolation.severity(), 3);
    assert_eq!(FaultType::PipelineFault.severity(), 3);
}

/// RTError::severity must map each variant to the correct ErrorSeverity.
#[test]
fn rt_error_severity_mapping() {
    assert_eq!(RTError::DeviceDisconnected.severity(), ErrorSeverity::Critical);
    assert_eq!(RTError::TorqueLimit.severity(), ErrorSeverity::Critical);
    assert_eq!(RTError::PipelineFault.severity(), ErrorSeverity::Error);
    assert_eq!(RTError::TimingViolation.severity(), ErrorSeverity::Warning);
    assert_eq!(RTError::RTSetupFailed.severity(), ErrorSeverity::Critical);
    assert_eq!(RTError::InvalidConfig.severity(), ErrorSeverity::Error);
    assert_eq!(RTError::SafetyInterlock.severity(), ErrorSeverity::Critical);
    assert_eq!(RTError::BufferOverflow.severity(), ErrorSeverity::Warning);
    assert_eq!(RTError::DeadlineMissed.severity(), ErrorSeverity::Critical);
    assert_eq!(RTError::ResourceUnavailable.severity(), ErrorSeverity::Error);
}

/// FaultType::is_recoverable must return the correct value for every variant.
#[test]
fn fault_type_is_recoverable_exact() {
    let recoverable = [
        FaultType::UsbStall,
        FaultType::ThermalLimit,
        FaultType::PluginOverrun,
        FaultType::TimingViolation,
        FaultType::PipelineFault,
    ];
    for ft in &recoverable {
        assert!(ft.is_recoverable(), "{:?} must be recoverable", ft);
    }

    let non_recoverable = [
        FaultType::Overcurrent,
        FaultType::EncoderNaN,
        FaultType::SafetyInterlockViolation,
        FaultType::HandsOffTimeout,
    ];
    for ft in &non_recoverable {
        assert!(!ft.is_recoverable(), "{:?} must NOT be recoverable", ft);
    }
}

/// FaultType::default_max_response_time_ms must return distinct, correct values.
#[test]
fn fault_type_response_times_exact() {
    assert_eq!(FaultType::Overcurrent.default_max_response_time_ms(), 10);
    assert_eq!(FaultType::ThermalLimit.default_max_response_time_ms(), 50);
    assert_eq!(FaultType::UsbStall.default_max_response_time_ms(), 50);
    assert_eq!(FaultType::EncoderNaN.default_max_response_time_ms(), 50);
    assert_eq!(
        FaultType::SafetyInterlockViolation.default_max_response_time_ms(),
        10
    );
    assert_eq!(FaultType::HandsOffTimeout.default_max_response_time_ms(), 50);
    assert_eq!(FaultType::PluginOverrun.default_max_response_time_ms(), 1);
    assert_eq!(FaultType::TimingViolation.default_max_response_time_ms(), 1);
    assert_eq!(FaultType::PipelineFault.default_max_response_time_ms(), 10);
}

/// RTError::is_recoverable must return the correct boolean for each variant.
#[test]
fn rt_error_is_recoverable_exact() {
    let recoverable = [
        RTError::TimingViolation,
        RTError::BufferOverflow,
        RTError::ResourceUnavailable,
    ];
    for err in &recoverable {
        assert!(err.is_recoverable(), "{:?} should be recoverable", err);
    }

    let non_recoverable = [
        RTError::DeviceDisconnected,
        RTError::TorqueLimit,
        RTError::PipelineFault,
        RTError::RTSetupFailed,
        RTError::InvalidConfig,
        RTError::SafetyInterlock,
        RTError::DeadlineMissed,
    ];
    for err in &non_recoverable {
        assert!(!err.is_recoverable(), "{:?} should NOT be recoverable", err);
    }
}

/// RTError::requires_safety_action must return the correct boolean.
#[test]
fn rt_error_requires_safety_action_exact() {
    let safety_action = [
        RTError::DeviceDisconnected,
        RTError::TorqueLimit,
        RTError::SafetyInterlock,
        RTError::DeadlineMissed,
    ];
    for err in &safety_action {
        assert!(
            err.requires_safety_action(),
            "{:?} should require safety action",
            err,
        );
    }

    let no_safety_action = [
        RTError::PipelineFault,
        RTError::TimingViolation,
        RTError::RTSetupFailed,
        RTError::InvalidConfig,
        RTError::BufferOverflow,
        RTError::ResourceUnavailable,
    ];
    for err in &no_safety_action {
        assert!(
            !err.requires_safety_action(),
            "{:?} should NOT require safety action",
            err,
        );
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 5. Missing boundary checks
// ═════════════════════════════════════════════════════════════════════════════

/// Torque cap with NaN input must produce a finite, safe output.
#[test]
fn torque_cap_nan_produces_finite() {
    let mut frame = FilterFrame::from_torque(f32::NAN);
    torque_cap_filter(&mut frame, 0.5);
    assert!(
        frame.torque_out.is_finite(),
        "NaN input must produce finite output, got {}",
        frame.torque_out,
    );
}

/// Torque cap with infinity must clamp to the cap value.
#[test]
fn torque_cap_infinity_clamped() {
    let mut frame = FilterFrame::from_torque(f32::INFINITY);
    torque_cap_filter(&mut frame, 0.5);
    assert!(
        frame.torque_out <= 0.5,
        "Infinity must be clamped to cap (0.5), got {}",
        frame.torque_out,
    );
}

/// Torque cap with negative infinity must clamp to the negative cap value.
#[test]
fn torque_cap_neg_infinity_clamped() {
    let mut frame = FilterFrame::from_torque(f32::NEG_INFINITY);
    torque_cap_filter(&mut frame, 0.5);
    assert!(
        frame.torque_out >= -0.5,
        "Negative infinity must be clamped to -cap (-0.5), got {}",
        frame.torque_out,
    );
}

/// Calibration with raw value at min boundary must not underflow.
#[test]
fn calibration_at_min_boundary_no_underflow() {
    let cal = AxisCalibration::new(0, 1000).with_deadzone(0, 1000);
    // raw=0 is exactly min — must produce 0.0 cleanly
    let value = cal.apply(0);
    assert!(
        value >= 0.0 && value.is_finite(),
        "raw at min must produce non-negative finite value, got {}",
        value,
    );
}

/// Calibration with raw value above max must clamp to 1.0.
#[test]
fn calibration_above_max_clamps_to_one() {
    let cal = AxisCalibration::new(0, 1000).with_deadzone(0, 1000);
    let value = cal.apply(1500);
    assert!(
        value <= 1.0,
        "raw above max must not exceed 1.0, got {}",
        value,
    );
}

/// Calibration with min == max (zero range) must not divide by zero.
#[test]
fn calibration_zero_range_no_panic() {
    let cal = AxisCalibration::new(500, 500);
    let value = cal.apply(500);
    assert!(value.is_finite(), "Zero range must produce finite, got {}", value);
}

/// LUT lookup with negative input must clamp to 0.0, not go out of bounds.
#[test]
fn lut_lookup_negative_input_clamped() {
    let lut = CurveLut::linear();
    let value = lut.lookup(-1.0);
    let table = lut.table();
    assert!(
        (value - table[0]).abs() < f32::EPSILON,
        "Negative input must clamp to table[0] = {}, got {}",
        table[0],
        value,
    );
}

/// LUT lookup with input > 1.0 must clamp, not go out of bounds.
#[test]
fn lut_lookup_above_one_clamped() {
    let lut = CurveLut::linear();
    let value = lut.lookup(2.0);
    let table = lut.table();
    assert!(
        (value - table[CurveLut::SIZE - 1]).abs() < f32::EPSILON,
        "Input > 1.0 must clamp to table[last] = {}, got {}",
        table[CurveLut::SIZE - 1],
        value,
    );
}

/// Percentage calculations with zero counters must return 0.0, not NaN.
#[test]
fn percentage_with_zero_counters_is_zero() {
    let counters = AtomicCounters::new();
    let sat_pct = counters.torque_saturation_percent();
    let loss_pct = counters.telemetry_loss_percent();
    assert!(
        (sat_pct - 0.0).abs() < f32::EPSILON,
        "Zero samples must produce 0% saturation, got {}",
        sat_pct,
    );
    assert!(
        (loss_pct - 0.0).abs() < f32::EPSILON,
        "Zero packets must produce 0% loss, got {}",
        loss_pct,
    );
}

/// Snapshot percentage calculations must agree with live counter values.
#[test]
fn snapshot_percentages_match_live() {
    let counters = AtomicCounters::new();
    for _ in 0..50 {
        counters.inc_telemetry_received();
    }
    for _ in 0..50 {
        counters.inc_telemetry_lost();
    }
    for i in 0..100 {
        counters.record_torque_saturation(i < 25);
    }

    let live_loss = counters.telemetry_loss_percent();
    let live_sat = counters.torque_saturation_percent();

    let snapshot = counters.snapshot();
    let snap_loss = snapshot.telemetry_loss_percent();
    let snap_sat = snapshot.torque_saturation_percent();

    assert!(
        (live_loss - snap_loss).abs() < 0.1,
        "Snapshot loss {}% != live {}%",
        snap_loss,
        live_loss,
    );
    assert!(
        (live_sat - snap_sat).abs() < 0.1,
        "Snapshot sat {}% != live {}%",
        snap_sat,
        live_sat,
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// 6. Wrong enum variant
// ═════════════════════════════════════════════════════════════════════════════

/// FaultAction::affects_torque must be true for SoftStop and SafeMode only.
#[test]
fn fault_action_affects_torque_variants() {
    let torque_affecting = [FaultAction::SoftStop, FaultAction::SafeMode];
    for action in &torque_affecting {
        assert!(
            action.affects_torque(),
            "{:?} should affect torque",
            action,
        );
    }

    let non_torque = [
        FaultAction::Quarantine,
        FaultAction::LogAndContinue,
        FaultAction::Restart,
    ];
    for action in &non_torque {
        assert!(
            !action.affects_torque(),
            "{:?} should NOT affect torque",
            action,
        );
    }
}

/// FaultAction::allows_operation must be true for LogAndContinue,
/// Quarantine, and Restart only.
#[test]
fn fault_action_allows_operation_variants() {
    let allows = [
        FaultAction::LogAndContinue,
        FaultAction::Quarantine,
        FaultAction::Restart,
    ];
    for action in &allows {
        assert!(
            action.allows_operation(),
            "{:?} should allow operation",
            action,
        );
    }

    let disallows = [FaultAction::SoftStop, FaultAction::SafeMode];
    for action in &disallows {
        assert!(
            !action.allows_operation(),
            "{:?} should NOT allow operation",
            action,
        );
    }
}

/// FaultAction::affects_torque and allows_operation must be mutually exclusive.
#[test]
fn fault_action_torque_and_operation_exclusive() {
    let all_actions = [
        FaultAction::SoftStop,
        FaultAction::Quarantine,
        FaultAction::LogAndContinue,
        FaultAction::Restart,
        FaultAction::SafeMode,
    ];
    for action in &all_actions {
        // An action that affects torque must not allow normal operation
        assert!(
            !(action.affects_torque() && action.allows_operation()),
            "{:?} must not both affect torque AND allow operation",
            action,
        );
    }
}

/// ErrorSeverity ordering: Critical > Error > Warning > Info.
#[test]
fn error_severity_ordering() {
    assert!(ErrorSeverity::Critical > ErrorSeverity::Error);
    assert!(ErrorSeverity::Error > ErrorSeverity::Warning);
    assert!(ErrorSeverity::Warning > ErrorSeverity::Info);
}

/// FaultType severity values must be in range [1, 4].
#[test]
fn fault_type_severity_in_range() {
    let all_faults = [
        FaultType::UsbStall,
        FaultType::EncoderNaN,
        FaultType::ThermalLimit,
        FaultType::Overcurrent,
        FaultType::PluginOverrun,
        FaultType::TimingViolation,
        FaultType::SafetyInterlockViolation,
        FaultType::HandsOffTimeout,
        FaultType::PipelineFault,
    ];
    for ft in &all_faults {
        let sev = ft.severity();
        assert!(
            (1..=4).contains(&sev),
            "{:?} severity {} is outside [1, 4]",
            ft,
            sev,
        );
    }
}

/// Critical faults (severity 1) must always require immediate response.
#[test]
fn critical_faults_require_immediate_response() {
    let all_faults = [
        FaultType::UsbStall,
        FaultType::EncoderNaN,
        FaultType::ThermalLimit,
        FaultType::Overcurrent,
        FaultType::PluginOverrun,
        FaultType::TimingViolation,
        FaultType::SafetyInterlockViolation,
        FaultType::HandsOffTimeout,
        FaultType::PipelineFault,
    ];
    for ft in &all_faults {
        if ft.severity() == 1 {
            assert!(
                ft.requires_immediate_response(),
                "Critical fault {:?} must require immediate response",
                ft,
            );
        }
    }
}

/// RTError::from_code must round-trip correctly for all valid codes.
#[test]
fn rt_error_from_code_round_trip() {
    for code in 1..=10u8 {
        let err = RTError::from_code(code);
        assert!(err.is_some(), "Code {} must map to an RTError variant", code);
        if let Some(e) = err {
            assert_eq!(e.code(), code, "Round-trip failed for code {}", code);
        }
    }
}

/// RTError::from_code must return None for invalid codes.
#[test]
fn rt_error_from_code_invalid_returns_none() {
    assert!(RTError::from_code(0).is_none(), "Code 0 should be invalid");
    assert!(RTError::from_code(11).is_none(), "Code 11 should be invalid");
    assert!(RTError::from_code(255).is_none(), "Code 255 should be invalid");
}

/// record_torque_saturation(false) must increment samples but NOT count.
#[test]
fn record_saturation_false_increments_only_samples() {
    let counters = AtomicCounters::new();
    counters.record_torque_saturation(false);
    let snap = counters.snapshot();
    assert_eq!(
        snap.torque_saturation_samples, 1,
        "Samples should be 1 after one record",
    );
    assert_eq!(
        snap.torque_saturation_count, 0,
        "Count should be 0 when not saturated",
    );
}

/// record_torque_saturation(true) must increment both samples and count.
#[test]
fn record_saturation_true_increments_both() {
    let counters = AtomicCounters::new();
    counters.record_torque_saturation(true);
    let snap = counters.snapshot();
    assert_eq!(snap.torque_saturation_samples, 1);
    assert_eq!(snap.torque_saturation_count, 1);
}

/// LUT linear lookup must produce monotonically increasing values.
#[test]
fn lut_linear_is_monotonic() {
    let lut = CurveLut::linear();
    let mut prev = lut.lookup(0.0);
    let steps = 256;
    for i in 1..=steps {
        let input = i as f32 / steps as f32;
        let value = lut.lookup(input);
        assert!(
            value >= prev - f32::EPSILON,
            "LUT not monotonic at input {}: {} < {}",
            input,
            value,
            prev,
        );
        prev = value;
    }
}
