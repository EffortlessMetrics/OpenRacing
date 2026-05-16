//! Additional coverage tests for `openracing-shifter`.
//!
//! Covers behaviour that the original suite leaves uncovered:
//!
//! * Sequential shift saturating arithmetic at `i32::MAX` / `i32::MIN`.
//!   Without `saturating_add` / `saturating_sub` the previous
//!   implementation would panic in debug builds; these tests pin the new
//!   non-panicking behaviour.
//! * `GearPosition::new(i32::MAX)` and `GearPosition::new(i32::MIN)`
//!   field values.
//! * `parse_gamepad` ignores `data[0]` and `data[1]` (header bytes).
//! * `parse_gamepad` button-mask isolation — bits other than `0x10`
//!   (paddle up) and `0x20` (paddle down) do not influence the paddle
//!   flags.
//! * `ShifterError` `std::error::Error` impl: `source()` is `None`,
//!   the value coerces to `&dyn std::error::Error`, and the
//!   `InvalidGear` payload is recoverable via pattern match.
//! * `ShifterCapabilities::default()` complete field-by-field
//!   assertion.

use openracing_shifter::{
    GearPosition, MAX_GEARS, ShifterCapabilities, ShifterError, ShifterInput, ShifterType,
};
use std::error::Error;

// ---------------------------------------------------------------------------
// Sequential shift saturating arithmetic
// ---------------------------------------------------------------------------

#[test]
fn from_sequential_upshift_at_i32_max_saturates_to_max_gears() {
    // Pre-fix this would panic in debug builds via integer overflow.
    let input = ShifterInput::from_sequential(true, false, i32::MAX);
    assert_eq!(input.gear(), MAX_GEARS as i32);
    assert!(input.is_shifting());
}

#[test]
fn from_sequential_downshift_at_i32_min_saturates_to_one() {
    let input = ShifterInput::from_sequential(false, true, i32::MIN);
    assert_eq!(input.gear(), 1);
    assert!(input.is_shifting());
}

#[test]
fn from_sequential_upshift_just_below_clamp() {
    // current=7, up=true → gear=8 (= MAX_GEARS as i32). Boundary one
    // below the clamp, never directly asserted by the original suite.
    let input = ShifterInput::from_sequential(true, false, 7);
    assert_eq!(input.gear(), MAX_GEARS as i32);
}

#[test]
fn from_sequential_downshift_just_above_clamp() {
    let input = ShifterInput::from_sequential(false, true, 2);
    assert_eq!(input.gear(), 1);
}

#[test]
fn from_sequential_upshift_above_max_clamps() {
    // current=MAX (8), up=true → stays at 8 (saturating + clamp combine).
    let input = ShifterInput::from_sequential(true, false, MAX_GEARS as i32);
    assert_eq!(input.gear(), MAX_GEARS as i32);
}

#[test]
fn from_sequential_no_shift_passes_through_extremes() {
    // No clamp is applied when neither paddle is pressed; document that
    // contract holds even at the i32 extremes.
    let input_max = ShifterInput::from_sequential(false, false, i32::MAX);
    assert_eq!(input_max.gear(), i32::MAX);
    let input_min = ShifterInput::from_sequential(false, false, i32::MIN);
    assert_eq!(input_min.gear(), i32::MIN);
}

// ---------------------------------------------------------------------------
// GearPosition::new at i32 extremes
// ---------------------------------------------------------------------------

#[test]
fn gear_position_new_with_i32_max_is_forward_only() {
    let pos = GearPosition::new(i32::MAX);
    assert_eq!(pos.gear, i32::MAX);
    assert!(!pos.is_neutral);
    assert!(!pos.is_reverse);
}

#[test]
fn gear_position_new_with_i32_min_is_reverse() {
    let pos = GearPosition::new(i32::MIN);
    assert_eq!(pos.gear, i32::MIN);
    assert!(!pos.is_neutral);
    assert!(pos.is_reverse);
}

// ---------------------------------------------------------------------------
// parse_gamepad ignores header bytes 0 and 1
// ---------------------------------------------------------------------------

#[test]
fn parse_gamepad_ignores_header_bytes_zero_and_one()
-> Result<(), Box<dyn std::error::Error>> {
    let a = ShifterInput::parse_gamepad(&[0x00, 0x00, 0x03, 0x10, 0xAB, 0xCD])?;
    let b = ShifterInput::parse_gamepad(&[0xFF, 0xFF, 0x03, 0x10, 0xAB, 0xCD])?;
    assert_eq!(a.gear(), b.gear());
    assert_eq!(a.paddle_up, b.paddle_up);
    assert_eq!(a.paddle_down, b.paddle_down);
    assert_eq!(a.clutch, b.clutch);
    Ok(())
}

// ---------------------------------------------------------------------------
// parse_gamepad button-mask isolation
// ---------------------------------------------------------------------------

#[test]
fn parse_gamepad_unrelated_button_bits_do_not_set_paddles()
-> Result<(), Box<dyn std::error::Error>> {
    // 0x10 = paddle up, 0x20 = paddle down. Bits 0x01, 0x02, 0x04, 0x08,
    // 0x40, 0x80 must be ignored.
    let buttons = 0x01 | 0x02 | 0x04 | 0x08 | 0x40 | 0x80;
    let input = ShifterInput::parse_gamepad(&[0x00, 0x00, 0x01, buttons])?;
    assert!(!input.paddle_up);
    assert!(!input.paddle_down);
    Ok(())
}

#[test]
fn parse_gamepad_paddle_bits_isolated_with_other_bits()
-> Result<(), Box<dyn std::error::Error>> {
    // All bits set → both paddles must be true.
    let input = ShifterInput::parse_gamepad(&[0x00, 0x00, 0x01, 0xFF])?;
    assert!(input.paddle_up);
    assert!(input.paddle_down);
    Ok(())
}

#[test]
fn parse_gamepad_paddle_up_only_with_neighbouring_bits()
-> Result<(), Box<dyn std::error::Error>> {
    // 0x10 plus neighbouring bits but not 0x20.
    let input = ShifterInput::parse_gamepad(&[0x00, 0x00, 0x01, 0x10 | 0x40 | 0x80])?;
    assert!(input.paddle_up);
    assert!(!input.paddle_down);
    Ok(())
}

// ---------------------------------------------------------------------------
// ShifterError trait implementations
// ---------------------------------------------------------------------------

#[test]
fn shifter_error_implements_std_error_trait() {
    let err = ShifterError::InvalidReport;
    let as_dyn: &dyn Error = &err;
    assert!(as_dyn.source().is_none());
    let _ = format!("{err}");
}

#[test]
fn shifter_error_invalid_gear_payload_is_recoverable() {
    for v in [i32::MIN, -1, 0, 100, i32::MAX] {
        let err = ShifterError::InvalidGear(v);
        match err {
            ShifterError::InvalidGear(carried) => assert_eq!(carried, v),
            other => panic!("expected InvalidGear, got {other:?}"),
        }
    }
}

#[test]
fn shifter_error_disconnected_implements_error() {
    let err = ShifterError::Disconnected;
    let as_dyn: &dyn Error = &err;
    assert!(as_dyn.source().is_none());
    assert!(format!("{err}").contains("disconnect") || !format!("{err}").is_empty());
}

// ---------------------------------------------------------------------------
// ShifterCapabilities::default()
// ---------------------------------------------------------------------------

#[test]
fn shifter_capabilities_default_complete_field_assertion() {
    let caps = ShifterCapabilities::default();
    assert_eq!(caps.shifter_type, ShifterType::Sequential);
    assert_eq!(caps.max_gears, MAX_GEARS);
    assert!(!caps.has_clutch);
    assert!(caps.has_paddle_shifters);
}
