//! Additional coverage tests for `racing-wheel-telemetry-contracts`.
//!
//! Pins behaviour the existing suite leaves implicit:
//!
//! * `rpm_fraction` with zero / negative / NaN / infinite redline.
//! * `with_ffb_scalar(NaN)` propagation through `f32::clamp`.
//! * `with_speed_ms(NEG_INFINITY)` (combined non-finite + non-negative
//!   branch).
//! * `with_gear` accepts `i8::MIN` and `i8::MAX` verbatim (no clamp).
//! * `has_active_flags` when `green_flag == false` but a hazard flag is set
//!   (green does not suppress hazards).
//! * `TelemetryValue` cross-variant inequality (`Boolean(true)` vs
//!   `Integer(1)`).
//! * `TelemetryValue::Float(NaN) != Float(NaN)` — IEEE NaN semantics
//!   through derived `PartialEq`.
//! * `TelemetryFieldCoverage` and `FlagCoverage` `Clone` / `Debug` are
//!   exercised so removing a derive is caught.
//! * `NormalizedTelemetry::default()` direct call (vs `new()`).
//! * Large finite speeds (`speed_kmh` / `speed_mph`).

use racing_wheel_telemetry_contracts::{
    FlagCoverage, NormalizedTelemetry, TelemetryFieldCoverage, TelemetryFlags, TelemetryFrame,
    TelemetryValue,
};

// ---------------------------------------------------------------------------
// rpm_fraction edge cases
// ---------------------------------------------------------------------------

#[test]
fn rpm_fraction_with_zero_redline_clamps_to_one() {
    // rpm / 0.0 == +inf → clamp to 1.0
    let t = NormalizedTelemetry::new().with_rpm(1000.0);
    assert_eq!(t.rpm_fraction(0.0), Some(1.0));
}

#[test]
fn rpm_fraction_with_zero_redline_and_zero_rpm_yields_nan_passthrough() {
    // 0.0 / 0.0 = NaN; clamp on NaN returns NaN. Document propagation.
    let t = NormalizedTelemetry::new().with_rpm(0.0);
    let frac = t.rpm_fraction(0.0).expect("rpm is set");
    assert!(frac.is_nan(), "0/0 → NaN must propagate");
}

#[test]
fn rpm_fraction_with_negative_redline_clamps_to_zero() {
    // 1000 / -8000 = -0.125 → clamp to 0.0
    let t = NormalizedTelemetry::new().with_rpm(1000.0);
    assert_eq!(t.rpm_fraction(-8000.0), Some(0.0));
}

#[test]
fn rpm_fraction_with_infinite_redline_yields_zero() {
    let t = NormalizedTelemetry::new().with_rpm(5000.0);
    assert_eq!(t.rpm_fraction(f32::INFINITY), Some(0.0));
}

#[test]
fn rpm_fraction_with_nan_redline_propagates_nan() {
    let t = NormalizedTelemetry::new().with_rpm(5000.0);
    let frac = t.rpm_fraction(f32::NAN).expect("rpm is set");
    assert!(frac.is_nan());
}

// ---------------------------------------------------------------------------
// with_ffb_scalar NaN
// ---------------------------------------------------------------------------

#[test]
fn with_ffb_scalar_nan_value_remains_nan() {
    // `clamp` on NaN returns NaN; the builder wraps it in Some(NaN).
    let t = NormalizedTelemetry::new().with_ffb_scalar(f32::NAN);
    assert!(t.ffb_scalar.expect("set above").is_nan());
}

// ---------------------------------------------------------------------------
// with_speed_ms / with_rpm NEG_INFINITY
// ---------------------------------------------------------------------------

#[test]
fn with_speed_ms_rejects_neg_infinity() {
    let t = NormalizedTelemetry::new().with_speed_ms(f32::NEG_INFINITY);
    assert!(t.speed_ms.is_none());
}

#[test]
fn with_rpm_rejects_neg_infinity() {
    let t = NormalizedTelemetry::new().with_rpm(f32::NEG_INFINITY);
    assert!(t.rpm.is_none());
}

#[test]
fn with_slip_ratio_rejects_neg_infinity() {
    let t = NormalizedTelemetry::new().with_slip_ratio(f32::NEG_INFINITY);
    assert!(t.slip_ratio.is_none());
}

// ---------------------------------------------------------------------------
// with_gear accepts i8 extremes verbatim
// ---------------------------------------------------------------------------

#[test]
fn with_gear_accepts_i8_extremes_verbatim() {
    let min = NormalizedTelemetry::new().with_gear(i8::MIN);
    let max = NormalizedTelemetry::new().with_gear(i8::MAX);
    assert_eq!(min.gear, Some(i8::MIN));
    assert_eq!(max.gear, Some(i8::MAX));
}

// ---------------------------------------------------------------------------
// has_active_flags: green_flag does not suppress hazards
// ---------------------------------------------------------------------------

#[test]
fn has_active_flags_true_when_yellow_set_even_if_green_cleared() {
    let t = NormalizedTelemetry::new().with_flags(TelemetryFlags {
        green_flag: false,
        yellow_flag: true,
        ..TelemetryFlags::default()
    });
    assert!(t.has_active_flags());
}

// ---------------------------------------------------------------------------
// TelemetryValue cross-variant inequality and NaN semantics
// ---------------------------------------------------------------------------

#[test]
fn telemetry_value_cross_variant_pairs_are_unequal() {
    assert_ne!(TelemetryValue::Boolean(true), TelemetryValue::Integer(1));
    assert_ne!(TelemetryValue::Float(0.0), TelemetryValue::Boolean(false));
    assert_ne!(TelemetryValue::Float(0.0), TelemetryValue::Integer(0));
    assert_ne!(
        TelemetryValue::Integer(0),
        TelemetryValue::String("0".to_string())
    );
}

#[test]
fn telemetry_value_float_nan_not_equal_to_itself() {
    // IEEE-754: NaN != NaN. Pin that the derived PartialEq inherits this.
    let a = TelemetryValue::Float(f32::NAN);
    let b = TelemetryValue::Float(f32::NAN);
    assert_ne!(a, b);
}

// ---------------------------------------------------------------------------
// TelemetryFieldCoverage / FlagCoverage derives
// ---------------------------------------------------------------------------

#[test]
fn telemetry_field_coverage_clone_and_debug_visible() {
    let cov = TelemetryFieldCoverage {
        game_id: "g".to_string(),
        game_version: "1".to_string(),
        ffb_scalar: true,
        rpm: false,
        speed: true,
        slip_ratio: false,
        gear: true,
        flags: FlagCoverage {
            yellow_flag: true,
            red_flag: false,
            blue_flag: false,
            checkered_flag: false,
            green_flag: true,
            pit_limiter: false,
            in_pits: false,
            drs_available: false,
            drs_active: false,
            ers_available: false,
            launch_control: false,
            traction_control: false,
            abs_active: false,
        },
        car_id: true,
        track_id: false,
        extended_fields: vec!["fuel".to_string()],
    };
    let cloned = cov.clone();
    assert_eq!(cloned.game_id, cov.game_id);
    assert_eq!(cloned.flags.yellow_flag, cov.flags.yellow_flag);
    assert_eq!(cloned.extended_fields, cov.extended_fields);
    let dbg = format!("{cov:?}");
    assert!(dbg.contains("TelemetryFieldCoverage"));
    let flag_dbg = format!("{:?}", cov.flags);
    assert!(flag_dbg.contains("FlagCoverage"));
}

// ---------------------------------------------------------------------------
// NormalizedTelemetry::default() vs new()
// ---------------------------------------------------------------------------

#[test]
fn normalized_telemetry_default_direct_call_matches_new() {
    let d: NormalizedTelemetry = NormalizedTelemetry::default();
    let n = NormalizedTelemetry::new();
    assert_eq!(d, n);
    assert!(d.flags.green_flag);
    assert!(d.ffb_scalar.is_none());
}

// ---------------------------------------------------------------------------
// Large finite speed conversions
// ---------------------------------------------------------------------------

#[test]
fn speed_conversions_handle_large_finite_speed() {
    // 1000 m/s ≈ 3600 km/h ≈ 2237 mph (within f32 precision).
    let t = NormalizedTelemetry::new().with_speed_ms(1000.0);
    let kmh = t.speed_kmh().expect("set above");
    let mph = t.speed_mph().expect("set above");
    assert!((kmh - 3600.0).abs() < 0.5, "got {kmh}");
    assert!((mph - 2237.0).abs() < 0.5, "got {mph}");
}

// ---------------------------------------------------------------------------
// TelemetryFrame derives
// ---------------------------------------------------------------------------

#[test]
fn telemetry_frame_clone_preserves_all_fields() {
    let frame = TelemetryFrame::new(
        NormalizedTelemetry::new().with_rpm(2500.0).with_gear(2),
        42,
        7,
        16,
    );
    let cloned = frame.clone();
    assert_eq!(cloned.timestamp_ns, frame.timestamp_ns);
    assert_eq!(cloned.sequence, frame.sequence);
    assert_eq!(cloned.raw_size, frame.raw_size);
    assert_eq!(cloned.data, frame.data);
    let dbg = format!("{frame:?}");
    assert!(dbg.contains("TelemetryFrame"));
}
