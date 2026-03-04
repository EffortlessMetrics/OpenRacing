#![allow(clippy::redundant_closure)]

use openracing_shifter::*;

// ── Gear Position Detection (H-pattern) ─────────────────────────────────────

#[test]
fn gear_position_from_raw_all_valid_gears() {
    for gear in 1u8..=7 {
        let pos = GearPosition::from_raw(gear);
        assert_eq!(pos.gear, gear as i32);
        assert!(!pos.is_neutral);
        assert!(!pos.is_reverse);
    }
}

#[test]
fn gear_position_from_raw_invalid_values_map_to_neutral() {
    for raw in [8u8, 50, 100, 200, 254] {
        let pos = GearPosition::from_raw(raw);
        assert!(pos.is_neutral, "raw {raw} should map to neutral");
    }
}

#[test]
fn gear_position_from_raw_0xff_is_neutral() {
    let pos = GearPosition::from_raw(0xFF);
    assert!(pos.is_neutral);
    assert_eq!(pos.gear, 0);
}

#[test]
fn gear_position_from_raw_zero_is_neutral() {
    let pos = GearPosition::from_raw(0);
    assert!(pos.is_neutral);
    assert_eq!(pos.gear, NEUTRAL_GEAR);
}

// ── Sequential Shifting ─────────────────────────────────────────────────────

#[test]
fn sequential_upshift_increments() {
    let input = ShifterInput::from_sequential(true, false, 3);
    assert_eq!(input.gear(), 4);
    assert!(input.paddle_up);
    assert!(!input.paddle_down);
}

#[test]
fn sequential_downshift_decrements() {
    let input = ShifterInput::from_sequential(false, true, 5);
    assert_eq!(input.gear(), 4);
    assert!(!input.paddle_up);
    assert!(input.paddle_down);
}

#[test]
fn sequential_no_shift_preserves_gear() {
    let input = ShifterInput::from_sequential(false, false, 5);
    assert_eq!(input.gear(), 5);
    assert!(!input.is_shifting());
}

#[test]
fn sequential_both_paddles_up_takes_priority() {
    let input = ShifterInput::from_sequential(true, true, 3);
    assert_eq!(input.gear(), 4);
    assert!(input.is_shifting());
}

#[test]
fn sequential_upshift_clamped_at_max() {
    let input = ShifterInput::from_sequential(true, false, MAX_GEARS as i32);
    assert_eq!(input.gear(), MAX_GEARS as i32);
}

#[test]
fn sequential_downshift_clamped_at_one() {
    let input = ShifterInput::from_sequential(false, true, 1);
    assert_eq!(input.gear(), 1);
}

// ── State Transitions ───────────────────────────────────────────────────────

#[test]
fn upshift_through_all_gears() {
    let mut current = 1i32;
    for expected in 2..=MAX_GEARS as i32 {
        let input = ShifterInput::from_sequential(true, false, current);
        current = input.gear();
        assert_eq!(current, expected);
    }
    // At max, further upshift stays at max
    let input = ShifterInput::from_sequential(true, false, current);
    assert_eq!(input.gear(), MAX_GEARS as i32);
}

#[test]
fn downshift_through_all_gears() {
    let mut current = MAX_GEARS as i32;
    for expected in (1..MAX_GEARS as i32).rev() {
        let input = ShifterInput::from_sequential(false, true, current);
        current = input.gear();
        assert_eq!(current, expected);
    }
    // At 1, further downshift stays at 1
    let input = ShifterInput::from_sequential(false, true, current);
    assert_eq!(input.gear(), 1);
}

#[test]
fn rapid_up_down_returns_to_original() {
    let start = 4;
    let up = ShifterInput::from_sequential(true, false, start);
    let down = ShifterInput::from_sequential(false, true, up.gear());
    assert_eq!(down.gear(), start);
}

// ── Neutral / Reverse Detection ─────────────────────────────────────────────

#[test]
fn neutral_gear_properties() {
    let neutral = GearPosition::neutral();
    assert!(neutral.is_neutral);
    assert!(!neutral.is_reverse);
    assert_eq!(neutral.gear, NEUTRAL_GEAR);
}

#[test]
fn reverse_gear_properties() {
    let reverse = GearPosition::reverse();
    assert!(!reverse.is_neutral);
    assert!(reverse.is_reverse);
    assert_eq!(reverse.gear, -1);
}

#[test]
fn negative_gear_values_are_reverse() {
    for g in [-1, -2, -3] {
        let pos = GearPosition::new(g);
        assert!(pos.is_reverse, "gear {g} should be reverse");
        assert!(!pos.is_neutral);
    }
}

#[test]
fn zero_gear_is_neutral_not_reverse() {
    let pos = GearPosition::new(0);
    assert!(pos.is_neutral);
    assert!(!pos.is_reverse);
}

#[test]
fn default_gear_position_is_neutral() {
    let pos = GearPosition::default();
    assert!(pos.is_neutral);
    assert_eq!(pos.gear, NEUTRAL_GEAR);
}

// ── Gamepad Parsing ─────────────────────────────────────────────────────────

#[test]
fn parse_gamepad_gear_and_paddles() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0x00, 0x00, 0x03, 0x10, 0x00, 0x00];
    let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert_eq!(input.gear(), 3);
    assert!(input.paddle_up);
    assert!(!input.paddle_down);
    Ok(())
}

#[test]
fn parse_gamepad_both_paddles() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0x00, 0x00, 0x04, 0x30, 0x00, 0x00];
    let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert_eq!(input.gear(), 4);
    assert!(input.paddle_up);
    assert!(input.paddle_down);
    Ok(())
}

#[test]
fn parse_gamepad_with_clutch_data() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0x00, 0x00, 0x01, 0x00, 0xCD, 0xAB];
    let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert_eq!(input.clutch, Some(0xABCD));
    Ok(())
}

#[test]
fn parse_gamepad_no_clutch_when_short() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0x00, 0x00, 0x01, 0x00];
    let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert_eq!(input.clutch, None);
    Ok(())
}

#[test]
fn parse_gamepad_5_bytes_no_clutch() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0x00, 0x00, 0x03, 0x00, 0xFF];
    let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert_eq!(input.clutch, None);
    Ok(())
}

#[test]
fn parse_gamepad_too_short_gives_invalid_report() {
    for len in 0..4 {
        let data = vec![0u8; len];
        let result = ShifterInput::parse_gamepad(&data);
        assert!(matches!(result, Err(ShifterError::InvalidReport)));
    }
}

#[test]
fn parse_gamepad_neutral_gear() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0x00, 0x00, 0x00, 0x00];
    let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
    assert!(input.gear.is_neutral);
    assert_eq!(input.gear(), 0);
    Ok(())
}

// ── Capabilities ────────────────────────────────────────────────────────────

#[test]
fn h_pattern_capabilities() {
    let caps = ShifterCapabilities::h_pattern();
    assert_eq!(caps.shifter_type, ShifterType::HPattern);
    assert_eq!(caps.max_gears, 6);
    assert!(caps.has_clutch);
    assert!(!caps.has_paddle_shifters);
}

#[test]
fn sequential_capabilities() {
    let caps = ShifterCapabilities::sequential();
    assert_eq!(caps.shifter_type, ShifterType::Sequential);
    assert_eq!(caps.max_gears, MAX_GEARS);
    assert!(!caps.has_clutch);
    assert!(caps.has_paddle_shifters);
}

#[test]
fn default_capabilities_are_sequential() {
    let caps = ShifterCapabilities::default();
    assert_eq!(caps.shifter_type, ShifterType::Sequential);
    assert_eq!(caps.max_gears, MAX_GEARS);
}

#[test]
fn all_shifter_types_distinct() {
    assert_ne!(ShifterType::Sequential, ShifterType::HPattern);
    assert_ne!(ShifterType::Sequential, ShifterType::SequentialWithReverse);
    assert_ne!(ShifterType::HPattern, ShifterType::SequentialWithReverse);
}

// ── Error Display ───────────────────────────────────────────────────────────

#[test]
fn error_display_messages() {
    let err = ShifterError::InvalidGear(99);
    assert!(err.to_string().contains("99"));

    let err = ShifterError::InvalidReport;
    assert!(!err.to_string().is_empty());

    let err = ShifterError::Disconnected;
    let msg = err.to_string().to_lowercase();
    assert!(msg.contains("disconnected"));
}

// ── ShifterInput Default ────────────────────────────────────────────────────

#[test]
fn shifter_input_default() {
    let input = ShifterInput::default();
    assert!(input.gear.is_neutral);
    assert_eq!(input.clutch, None);
    assert!(!input.paddle_up);
    assert!(!input.paddle_down);
    assert!(!input.is_shifting());
    assert_eq!(input.gear(), 0);
}

// ── Proptest ────────────────────────────────────────────────────────────────

use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(256))]

    #[test]
    fn prop_sequential_gear_always_bounded(current in -100i32..=100i32) {
        let up = ShifterInput::from_sequential(true, false, current);
        prop_assert!(up.gear() <= MAX_GEARS as i32);

        let down = ShifterInput::from_sequential(false, true, current);
        prop_assert!(down.gear() >= 1);
    }

    #[test]
    fn prop_from_raw_never_panics(raw in 0u8..=255u8) {
        let pos = GearPosition::from_raw(raw);
        let _ = pos.gear;
        let _ = pos.is_neutral;
        let _ = pos.is_reverse;
    }

    #[test]
    fn prop_parse_gamepad_succeeds_for_valid_length(
        data in proptest::collection::vec(any::<u8>(), 4..=64),
    ) {
        let result = ShifterInput::parse_gamepad(&data);
        prop_assert!(result.is_ok());
    }

    #[test]
    fn prop_parse_gamepad_fails_for_short_data(
        data in proptest::collection::vec(any::<u8>(), 0..4usize),
    ) {
        let result = ShifterInput::parse_gamepad(&data);
        prop_assert!(result.is_err());
    }
}
