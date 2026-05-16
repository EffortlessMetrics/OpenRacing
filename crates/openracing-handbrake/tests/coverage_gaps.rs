//! Additional coverage tests for `openracing-handbrake`.
//!
//! Pins behaviour the original suite did not exercise:
//!
//! * `HandbrakeCalibration::apply` reads `min`/`max` but never `center`;
//!   the `center` field is therefore a no-op for `apply`. Document it.
//! * `HandbrakeCalibration::apply` on a freshly-constructed (zero-sample)
//!   calibration leaves the input at the default [0, MAX_ANALOG_VALUE]
//!   range.
//! * `HandbrakeInput::normalized` with extreme inverted full-range
//!   calibration (`min == u16::MAX, max == 0`).
//! * `HandbrakeInput::with_calibration` stores `(min, max)` verbatim,
//!   even when callers pass them in inverted order.
//! * `HandbrakeInput::normalized` with raw below an inverted calibration
//!   produces 0.0 (clamped through `saturating_sub`).
//! * `HandbrakeCapabilities::load_cell` permissive handling of negative,
//!   NaN, and infinite loads (documents current contract).
//! * `HandbrakeCapabilities::Debug` carries the type and field names.
//! * `HandbrakeError::Disconnected` `Display` does not carry a payload.
//! * `parse_gamepad` ignores header bytes 0/1 even when both are 0xFF.

use openracing_handbrake::{
    HandbrakeCalibration, HandbrakeCapabilities, HandbrakeError, HandbrakeInput, HandbrakeType,
    MAX_ANALOG_VALUE,
};

// ---------------------------------------------------------------------------
// HandbrakeCalibration: center is unused, zero-sample apply is a no-op
// ---------------------------------------------------------------------------

#[test]
fn calibration_apply_does_not_propagate_center_field() {
    let mut cal = HandbrakeCalibration::new();
    cal.sample(100);
    cal.sample(5000);
    cal.center = Some(2500);

    let mut input = HandbrakeInput::default();
    cal.apply(&mut input);
    // `apply` writes only min/max. The center field is currently a
    // dangling public field — pin the contract so a future change is
    // explicit.
    assert_eq!(input.calibration_min, 100);
    assert_eq!(input.calibration_max, 5000);
}

#[test]
fn calibration_apply_after_zero_samples_uses_constructor_defaults() {
    let cal = HandbrakeCalibration::new();
    let mut input = HandbrakeInput::default();
    cal.apply(&mut input);
    assert_eq!(input.calibration_min, 0);
    assert_eq!(input.calibration_max, MAX_ANALOG_VALUE);
}

// ---------------------------------------------------------------------------
// normalized: extreme inverted full-range
// ---------------------------------------------------------------------------

#[test]
fn normalized_fully_inverted_full_range_at_midpoint() {
    let input = HandbrakeInput {
        raw_value: u16::MAX / 2,
        is_engaged: true,
        calibration_min: u16::MAX,
        calibration_max: 0,
    };
    // After internal min/max ordering: min=0, max=u16::MAX → midpoint
    // raw normalises to ~0.5.
    let norm = input.normalized();
    assert!((norm - 0.5).abs() < 0.001, "got {norm}");
}

#[test]
fn normalized_raw_below_inverted_calibration_min_is_zero() {
    let input = HandbrakeInput {
        raw_value: 0,
        is_engaged: false,
        // Inverted order: builder stores verbatim; normalized() reorders
        // and uses the lower of the two as the floor.
        calibration_min: 9000,
        calibration_max: 1000,
    };
    // After reorder: min=1000, max=9000. raw_value=0 < min → offset 0.
    assert!(input.normalized().abs() < f32::EPSILON);
}

// ---------------------------------------------------------------------------
// with_calibration stores values verbatim
// ---------------------------------------------------------------------------

#[test]
fn with_calibration_preserves_inverted_argument_order() {
    let input = HandbrakeInput::default().with_calibration(9000, 1000);
    assert_eq!(input.calibration_min, 9000, "min field stored verbatim");
    assert_eq!(input.calibration_max, 1000, "max field stored verbatim");
    // And normalize() still produces a value in the unit range.
    let n = input.normalized();
    assert!((0.0..=1.0).contains(&n));
}

// ---------------------------------------------------------------------------
// HandbrakeCapabilities: permissive load values
// ---------------------------------------------------------------------------

#[test]
fn load_cell_capabilities_accept_negative_load() {
    let caps = HandbrakeCapabilities::load_cell(-5.0);
    assert_eq!(caps.handbrake_type, HandbrakeType::LoadCell);
    assert_eq!(caps.max_load_kg, Some(-5.0));
    assert!(caps.supports_calibration);
}

#[test]
fn load_cell_capabilities_accept_nan_load() {
    let caps = HandbrakeCapabilities::load_cell(f32::NAN);
    assert_eq!(caps.handbrake_type, HandbrakeType::LoadCell);
    let max = caps.max_load_kg.expect("set above");
    assert!(max.is_nan());
}

#[test]
fn load_cell_capabilities_accept_infinity_load() {
    let caps = HandbrakeCapabilities::load_cell(f32::INFINITY);
    let max = caps.max_load_kg.expect("set above");
    assert!(max.is_infinite() && max.is_sign_positive());

    let caps = HandbrakeCapabilities::load_cell(f32::NEG_INFINITY);
    let max = caps.max_load_kg.expect("set above");
    assert!(max.is_infinite() && max.is_sign_negative());
}

#[test]
fn handbrake_capabilities_debug_format_carries_type_and_fields() {
    let caps = HandbrakeCapabilities::load_cell(50.0);
    let dbg = format!("{caps:?}");
    assert!(dbg.contains("HandbrakeCapabilities"), "got {dbg}");
    assert!(dbg.contains("LoadCell"));
    assert!(dbg.contains("max_load_kg"));
}

// ---------------------------------------------------------------------------
// HandbrakeError::Disconnected has no payload in Display
// ---------------------------------------------------------------------------

#[test]
fn disconnected_error_display_does_not_carry_payload() {
    let err = HandbrakeError::Disconnected;
    let display = err.to_string();
    // Regression guard: if someone accidentally adds a payload to
    // Disconnected, the Display message would start carrying digits.
    assert!(
        !display.chars().any(|c| c.is_ascii_digit()),
        "Disconnected display should not contain digits; got {display:?}"
    );
}

// ---------------------------------------------------------------------------
// parse_gamepad ignores header bytes 0 and 1
// ---------------------------------------------------------------------------

#[test]
fn parse_gamepad_header_bytes_do_not_affect_raw_value()
-> Result<(), Box<dyn std::error::Error>> {
    let a = HandbrakeInput::parse_gamepad(&[0x00, 0x00, 0x10, 0x27])?;
    let b = HandbrakeInput::parse_gamepad(&[0xFF, 0xFF, 0x10, 0x27])?;
    assert_eq!(a.raw_value, b.raw_value);
    assert_eq!(a.is_engaged, b.is_engaged);
    Ok(())
}

#[test]
fn parse_gamepad_all_0xff_payload_engages() -> Result<(), Box<dyn std::error::Error>> {
    let input = HandbrakeInput::parse_gamepad(&[0xFF; 4])?;
    assert_eq!(input.raw_value, 0xFFFF);
    assert!(input.is_engaged);
    Ok(())
}

// ---------------------------------------------------------------------------
// HandbrakeType + HandbrakeCapabilities Eq sanity
// ---------------------------------------------------------------------------

#[test]
fn handbrake_capabilities_partial_eq_inequality_via_load() {
    let a = HandbrakeCapabilities::load_cell(50.0);
    let b = HandbrakeCapabilities::load_cell(100.0);
    assert_ne!(a, b);
}
