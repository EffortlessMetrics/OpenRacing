//! Deep tests for openracing-shifter.
//!
//! Covers: H-pattern recognition (gears 1-7 + neutral), sequential mode
//! (up/down shift), debounce (rapid shifts filtered via clamping), state
//! transitions (valid and invalid gear changes), edge cases (diagonal
//! positions, boundary values), and property-based gear-validity invariants.

use openracing_shifter::*;
use proptest::prelude::*;

// ── H-pattern recognition: all gear positions ──────────────────────────────

mod h_pattern {
    use super::*;

    #[test]
    fn all_forward_gears_recognized() {
        for gear in 1u8..=7 {
            let pos = GearPosition::from_raw(gear);
            assert_eq!(pos.gear, gear as i32, "gear {gear} not recognized");
            assert!(!pos.is_neutral);
            assert!(!pos.is_reverse);
        }
    }

    #[test]
    fn neutral_from_raw_zero() {
        let pos = GearPosition::from_raw(0);
        assert!(pos.is_neutral);
        assert!(!pos.is_reverse);
        assert_eq!(pos.gear, NEUTRAL_GEAR);
    }

    #[test]
    fn neutral_from_raw_0xff() {
        let pos = GearPosition::from_raw(0xFF);
        assert!(pos.is_neutral);
        assert_eq!(pos.gear, 0);
    }

    #[test]
    fn reverse_gear_from_new() {
        let rev = GearPosition::reverse();
        assert!(rev.is_reverse);
        assert!(!rev.is_neutral);
        assert_eq!(rev.gear, -1);
    }

    #[test]
    fn negative_gear_values_are_reverse() {
        for g in [-1, -2, -5, -100] {
            let pos = GearPosition::new(g);
            assert!(pos.is_reverse, "gear {g} should be reverse");
            assert!(!pos.is_neutral);
        }
    }

    #[test]
    fn gear_position_equality() {
        assert_eq!(GearPosition::new(3), GearPosition::new(3));
        assert_ne!(GearPosition::new(3), GearPosition::new(4));
        assert_ne!(GearPosition::neutral(), GearPosition::reverse());
    }

    #[test]
    fn parse_gamepad_recognizes_all_h_pattern_gears() -> Result<(), Box<dyn std::error::Error>> {
        for gear in 1u8..=7 {
            let data = [0x00, 0x00, gear, 0x00];
            let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
            assert_eq!(input.gear(), gear as i32, "parsed gear {gear} mismatch");
        }
        Ok(())
    }

    #[test]
    fn parse_gamepad_neutral_gear() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x00, 0x00, 0x00, 0x00];
        let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert!(input.gear.is_neutral);
        assert_eq!(input.gear(), 0);
        Ok(())
    }
}

// ── Sequential mode: up/down shift detection ───────────────────────────────

mod sequential_mode {
    use super::*;

    #[test]
    fn upshift_increments_gear() {
        let input = ShifterInput::from_sequential(true, false, 3);
        assert_eq!(input.gear(), 4);
        assert!(input.paddle_up);
        assert!(!input.paddle_down);
        assert!(input.is_shifting());
    }

    #[test]
    fn downshift_decrements_gear() {
        let input = ShifterInput::from_sequential(false, true, 5);
        assert_eq!(input.gear(), 4);
        assert!(input.paddle_down);
        assert!(!input.paddle_up);
        assert!(input.is_shifting());
    }

    #[test]
    fn no_shift_preserves_gear() {
        let input = ShifterInput::from_sequential(false, false, 5);
        assert_eq!(input.gear(), 5);
        assert!(!input.is_shifting());
    }

    #[test]
    fn both_paddles_up_takes_priority() {
        let input = ShifterInput::from_sequential(true, true, 3);
        assert_eq!(input.gear(), 4);
        assert!(input.is_shifting());
    }

    #[test]
    fn paddle_buttons_from_gamepad() -> Result<(), Box<dyn std::error::Error>> {
        // paddle_up = bit 0x10, paddle_down = bit 0x20
        let data_up = [0x00, 0x00, 0x01, 0x10];
        let up = ShifterInput::parse_gamepad(&data_up).map_err(|e| e.to_string())?;
        assert!(up.paddle_up);
        assert!(!up.paddle_down);

        let data_down = [0x00, 0x00, 0x01, 0x20];
        let down = ShifterInput::parse_gamepad(&data_down).map_err(|e| e.to_string())?;
        assert!(!down.paddle_up);
        assert!(down.paddle_down);

        let data_both = [0x00, 0x00, 0x01, 0x30];
        let both = ShifterInput::parse_gamepad(&data_both).map_err(|e| e.to_string())?;
        assert!(both.paddle_up);
        assert!(both.paddle_down);
        Ok(())
    }

    #[test]
    fn clutch_data_parsed_when_6_bytes() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x00, 0x00, 0x03, 0x00, 0x34, 0x12];
        let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.clutch, Some(0x1234));
        Ok(())
    }

    #[test]
    fn no_clutch_when_4_bytes() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x00, 0x00, 0x03, 0x00];
        let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.clutch, None);
        Ok(())
    }

    #[test]
    fn no_clutch_when_5_bytes() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x00, 0x00, 0x03, 0x00, 0xFF];
        let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.clutch, None);
        Ok(())
    }
}

// ── Debounce: rapid shifts filtered by clamping ────────────────────────────

mod debounce {
    use super::*;

    #[test]
    fn rapid_upshifts_clamped_at_max() {
        let mut current = 1;
        for _ in 0..20 {
            let input = ShifterInput::from_sequential(true, false, current);
            current = input.gear();
        }
        assert_eq!(current, MAX_GEARS as i32);
    }

    #[test]
    fn rapid_downshifts_clamped_at_one() {
        let mut current = MAX_GEARS as i32;
        for _ in 0..20 {
            let input = ShifterInput::from_sequential(false, true, current);
            current = input.gear();
        }
        assert_eq!(current, 1);
    }

    #[test]
    fn up_then_down_returns_to_original() {
        let start = 4;
        let up = ShifterInput::from_sequential(true, false, start);
        let back = ShifterInput::from_sequential(false, true, up.gear());
        assert_eq!(back.gear(), start);
    }

    #[test]
    fn alternating_shifts_stay_bounded() {
        let mut current = 4;
        for i in 0..100 {
            let up = i % 2 == 0;
            let input = ShifterInput::from_sequential(up, !up, current);
            current = input.gear();
            assert!(current >= 1);
            assert!(current <= MAX_GEARS as i32);
        }
    }
}

// ── State transitions: valid and invalid gear changes ──────────────────────

mod state_transitions {
    use super::*;

    #[test]
    fn upshift_through_all_gears_sequentially() {
        let mut current = 1;
        for expected in 2..=MAX_GEARS as i32 {
            let input = ShifterInput::from_sequential(true, false, current);
            current = input.gear();
            assert_eq!(current, expected);
        }
    }

    #[test]
    fn downshift_through_all_gears_sequentially() {
        let mut current = MAX_GEARS as i32;
        for expected in (1..MAX_GEARS as i32).rev() {
            let input = ShifterInput::from_sequential(false, true, current);
            current = input.gear();
            assert_eq!(current, expected);
        }
    }

    #[test]
    fn upshift_from_beyond_max_stays_clamped() {
        let input = ShifterInput::from_sequential(true, false, 100);
        assert_eq!(input.gear(), MAX_GEARS as i32);
    }

    #[test]
    fn downshift_from_negative_clamps_to_one() {
        let input = ShifterInput::from_sequential(false, true, -5);
        assert_eq!(input.gear(), 1);
    }

    #[test]
    fn no_shift_with_negative_gear_preserves_it() {
        let input = ShifterInput::from_sequential(false, false, -3);
        assert_eq!(input.gear(), -3);
    }

    #[test]
    fn no_shift_with_zero_gear_preserves_it() {
        let input = ShifterInput::from_sequential(false, false, 0);
        assert_eq!(input.gear(), 0);
    }

    #[test]
    fn shifter_input_default_is_neutral() {
        let input = ShifterInput::default();
        assert!(input.gear.is_neutral);
        assert_eq!(input.gear(), 0);
        assert_eq!(input.clutch, None);
        assert!(!input.paddle_up);
        assert!(!input.paddle_down);
        assert!(!input.is_shifting());
    }
}

// ── Edge cases: diagonal positions, boundary values ────────────────────────

mod edge_cases {
    use super::*;

    #[test]
    fn raw_values_above_7_map_to_neutral() {
        for raw in [8u8, 9, 50, 100, 200, 254] {
            let pos = GearPosition::from_raw(raw);
            assert!(pos.is_neutral, "raw {raw} should map to neutral");
            assert_eq!(pos.gear, 0);
        }
    }

    #[test]
    fn max_gears_constant() {
        assert_eq!(MAX_GEARS, 8);
    }

    #[test]
    fn neutral_gear_constant() {
        assert_eq!(NEUTRAL_GEAR, 0);
    }

    #[test]
    fn parse_gamepad_invalid_report_for_short_data() {
        for len in 0..4 {
            let data = vec![0u8; len];
            let result = ShifterInput::parse_gamepad(&data);
            assert!(
                matches!(result, Err(ShifterError::InvalidReport)),
                "length {len} should give InvalidReport"
            );
        }
    }

    #[test]
    fn error_display_invalid_gear() {
        let err = ShifterError::InvalidGear(999);
        assert!(err.to_string().contains("999"));
    }

    #[test]
    fn error_display_disconnected() {
        let err = ShifterError::Disconnected;
        let msg = err.to_string().to_lowercase();
        assert!(msg.contains("disconnected"));
    }

    #[test]
    fn error_display_invalid_report() {
        let err = ShifterError::InvalidReport;
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn error_debug_format_not_empty() {
        let errors: Vec<ShifterError> = vec![
            ShifterError::InvalidGear(0),
            ShifterError::InvalidReport,
            ShifterError::Disconnected,
        ];
        for err in errors {
            assert!(!format!("{err:?}").is_empty());
        }
    }

    #[test]
    fn shifter_type_all_variants_distinct() {
        let types = [
            ShifterType::Sequential,
            ShifterType::HPattern,
            ShifterType::SequentialWithReverse,
        ];
        for i in 0..types.len() {
            for j in (i + 1)..types.len() {
                assert_ne!(types[i], types[j]);
            }
        }
    }

    #[test]
    fn shifter_type_default_is_sequential() {
        assert_eq!(ShifterType::default(), ShifterType::Sequential);
    }

    #[test]
    fn capabilities_h_pattern_details() {
        let caps = ShifterCapabilities::h_pattern();
        assert_eq!(caps.shifter_type, ShifterType::HPattern);
        assert_eq!(caps.max_gears, 6);
        assert!(caps.has_clutch);
        assert!(!caps.has_paddle_shifters);
    }

    #[test]
    fn capabilities_sequential_details() {
        let caps = ShifterCapabilities::sequential();
        assert_eq!(caps.shifter_type, ShifterType::Sequential);
        assert_eq!(caps.max_gears, MAX_GEARS);
        assert!(!caps.has_clutch);
        assert!(caps.has_paddle_shifters);
    }

    #[test]
    fn capabilities_default_matches_sequential() {
        let def = ShifterCapabilities::default();
        let seq = ShifterCapabilities::sequential();
        assert_eq!(def.shifter_type, seq.shifter_type);
        assert_eq!(def.max_gears, seq.max_gears);
        assert_eq!(def.has_clutch, seq.has_clutch);
        assert_eq!(def.has_paddle_shifters, seq.has_paddle_shifters);
    }

    #[test]
    fn gear_position_clone_copy() {
        let a = GearPosition::new(5);
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn gear_position_default_is_neutral() {
        let pos = GearPosition::default();
        assert!(pos.is_neutral);
        assert_eq!(pos.gear, 0);
    }
}

// ── Serde round-trip ───────────────────────────────────────────────────────

mod serde_roundtrip {
    use super::*;

    #[test]
    fn gear_position_serde_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let positions = [
            GearPosition::neutral(),
            GearPosition::reverse(),
            GearPosition::new(1),
            GearPosition::new(7),
        ];
        for pos in &positions {
            let json = serde_json::to_string(pos).map_err(|e| e.to_string())?;
            let back: GearPosition = serde_json::from_str(&json).map_err(|e| e.to_string())?;
            assert_eq!(*pos, back);
        }
        Ok(())
    }

    #[test]
    fn shifter_type_serde_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let types = [
            ShifterType::Sequential,
            ShifterType::HPattern,
            ShifterType::SequentialWithReverse,
        ];
        for &t in &types {
            let json = serde_json::to_string(&t).map_err(|e| e.to_string())?;
            let back: ShifterType = serde_json::from_str(&json).map_err(|e| e.to_string())?;
            assert_eq!(t, back);
        }
        Ok(())
    }

    #[test]
    fn shifter_capabilities_serde_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let caps = [
            ShifterCapabilities::sequential(),
            ShifterCapabilities::h_pattern(),
        ];
        for cap in &caps {
            let json = serde_json::to_string(cap).map_err(|e| e.to_string())?;
            let back: ShifterCapabilities =
                serde_json::from_str(&json).map_err(|e| e.to_string())?;
            assert_eq!(*cap, back);
        }
        Ok(())
    }
}

// ── Property tests ─────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(512))]

    #[test]
    fn prop_gear_output_always_valid_for_sequential_up(current in -50i32..=100i32) {
        let input = ShifterInput::from_sequential(true, false, current);
        let g = input.gear();
        prop_assert!(g <= MAX_GEARS as i32,
            "gear {} exceeds max for upshift from {}", g, current);
    }

    #[test]
    fn prop_gear_output_always_valid_for_sequential_down(current in -50i32..=100i32) {
        let input = ShifterInput::from_sequential(false, true, current);
        let g = input.gear();
        prop_assert!(g >= 1,
            "gear {} below minimum for downshift from {}", g, current);
    }

    #[test]
    fn prop_from_raw_never_panics_and_gear_is_bounded(raw in 0u8..=255u8) {
        let pos = GearPosition::from_raw(raw);
        // Gear is either in [1,7] or neutral (0)
        let valid = pos.gear == 0 || (1..=7).contains(&pos.gear);
        prop_assert!(valid, "raw {} gave invalid gear {}", raw, pos.gear);
    }

    #[test]
    fn prop_parse_gamepad_valid_length_always_ok(
        data in proptest::collection::vec(any::<u8>(), 4..=128),
    ) {
        let result = ShifterInput::parse_gamepad(&data);
        prop_assert!(result.is_ok());
    }

    #[test]
    fn prop_parse_gamepad_short_data_always_err(
        data in proptest::collection::vec(any::<u8>(), 0..4usize),
    ) {
        let result = ShifterInput::parse_gamepad(&data);
        prop_assert!(result.is_err());
    }

    #[test]
    fn prop_no_shift_preserves_gear_exactly(current in -100i32..=100i32) {
        let input = ShifterInput::from_sequential(false, false, current);
        prop_assert_eq!(input.gear(), current);
    }

    #[test]
    fn prop_upshift_never_exceeds_max(current in 1i32..=100i32) {
        let input = ShifterInput::from_sequential(true, false, current);
        prop_assert!(input.gear() <= MAX_GEARS as i32);
    }

    #[test]
    fn prop_downshift_never_below_one(current in -100i32..=100i32) {
        let input = ShifterInput::from_sequential(false, true, current);
        prop_assert!(input.gear() >= 1);
    }

    #[test]
    fn prop_parsed_gear_from_raw_matches_direct(raw in 0u8..=255u8) {
        let data = [0x00, 0x00, raw, 0x00];
        if let Ok(input) = ShifterInput::parse_gamepad(&data) {
            let direct = GearPosition::from_raw(raw);
            prop_assert_eq!(input.gear.gear, direct.gear);
            prop_assert_eq!(input.gear.is_neutral, direct.is_neutral);
            prop_assert_eq!(input.gear.is_reverse, direct.is_reverse);
        }
    }
}

// ── Gear position encoding: systematic from_raw coverage ──────────────────

mod gear_position_encoding {
    use super::*;

    #[test]
    fn from_raw_all_256_values_never_panic() {
        for raw in 0u8..=255 {
            let pos = GearPosition::from_raw(raw);
            // Every value must produce a valid GearPosition
            let _gear = pos.gear;
            let _neutral = pos.is_neutral;
            let _reverse = pos.is_reverse;
        }
    }

    #[test]
    fn from_raw_valid_range_1_to_7_are_forward_gears() {
        for raw in 1u8..=7 {
            let pos = GearPosition::from_raw(raw);
            assert_eq!(pos.gear, raw as i32);
            assert!(!pos.is_neutral, "gear {raw} should not be neutral");
            assert!(!pos.is_reverse, "gear {raw} should not be reverse");
        }
    }

    #[test]
    fn from_raw_8_through_254_all_neutral() {
        for raw in 8u8..=254 {
            let pos = GearPosition::from_raw(raw);
            assert!(pos.is_neutral, "raw {raw} should map to neutral");
            assert_eq!(pos.gear, 0);
        }
    }

    #[test]
    fn gear_position_new_encodes_flags_correctly() {
        // Positive gears: not neutral, not reverse
        for g in 1..=10 {
            let pos = GearPosition::new(g);
            assert!(!pos.is_neutral);
            assert!(!pos.is_reverse);
            assert_eq!(pos.gear, g);
        }
        // Zero: neutral, not reverse
        let zero = GearPosition::new(0);
        assert!(zero.is_neutral);
        assert!(!zero.is_reverse);

        // Negative: not neutral, is reverse
        for g in [-1, -2, -10, i32::MIN] {
            let pos = GearPosition::new(g);
            assert!(!pos.is_neutral);
            assert!(pos.is_reverse);
        }
    }

    #[test]
    fn gear_position_serde_all_special_values() -> Result<(), Box<dyn std::error::Error>> {
        let positions = [
            GearPosition::neutral(),
            GearPosition::reverse(),
            GearPosition::new(0),
            GearPosition::new(1),
            GearPosition::new(7),
            GearPosition::new(-1),
            GearPosition::new(-100),
            GearPosition::from_raw(0),
            GearPosition::from_raw(7),
            GearPosition::from_raw(0xFF),
        ];
        for pos in &positions {
            let json = serde_json::to_string(pos).map_err(|e| e.to_string())?;
            let back: GearPosition = serde_json::from_str(&json).map_err(|e| e.to_string())?;
            assert_eq!(*pos, back, "serde roundtrip failed for gear={}", pos.gear);
        }
        Ok(())
    }
}

// ── Multi-gate patterns: different H-pattern layouts ──────────────────────

mod multi_gate_patterns {
    use super::*;

    #[test]
    fn four_speed_plus_reverse_pattern() {
        // 4+R: gears 1-4, reverse at -1, everything else neutral
        let valid_gears = [1, 2, 3, 4];
        for &g in &valid_gears {
            let pos = GearPosition::from_raw(g);
            assert_eq!(pos.gear, g as i32);
            assert!(!pos.is_neutral);
        }
        let rev = GearPosition::reverse();
        assert!(rev.is_reverse);
    }

    #[test]
    fn five_speed_pattern() {
        for g in 1u8..=5 {
            let pos = GearPosition::from_raw(g);
            assert_eq!(pos.gear, g as i32);
        }
    }

    #[test]
    fn six_speed_pattern() {
        for g in 1u8..=6 {
            let pos = GearPosition::from_raw(g);
            assert_eq!(pos.gear, g as i32);
        }
    }

    #[test]
    fn seven_speed_pattern() {
        for g in 1u8..=7 {
            let pos = GearPosition::from_raw(g);
            assert_eq!(pos.gear, g as i32);
        }
    }

    #[test]
    fn h_pattern_caps_reflect_6_gears() {
        let caps = ShifterCapabilities::h_pattern();
        assert_eq!(caps.max_gears, 6);
        assert!(caps.has_clutch);
    }

    #[test]
    fn sequential_caps_reflect_8_gears() {
        let caps = ShifterCapabilities::sequential();
        assert_eq!(caps.max_gears, MAX_GEARS);
        assert!(!caps.has_clutch);
    }

    #[test]
    fn gate_transitions_neutral_to_gear_to_neutral() -> Result<(), Box<dyn std::error::Error>> {
        // Simulate H-pattern: neutral → 1st → neutral → 2nd
        let neutral_data = [0x00, 0x00, 0x00, 0x00];
        let gear1_data = [0x00, 0x00, 0x01, 0x00];
        let gear2_data = [0x00, 0x00, 0x02, 0x00];

        let n1 = ShifterInput::parse_gamepad(&neutral_data).map_err(|e| e.to_string())?;
        assert!(n1.gear.is_neutral);

        let g1 = ShifterInput::parse_gamepad(&gear1_data).map_err(|e| e.to_string())?;
        assert_eq!(g1.gear(), 1);

        let n2 = ShifterInput::parse_gamepad(&neutral_data).map_err(|e| e.to_string())?;
        assert!(n2.gear.is_neutral);

        let g2 = ShifterInput::parse_gamepad(&gear2_data).map_err(|e| e.to_string())?;
        assert_eq!(g2.gear(), 2);
        Ok(())
    }
}

// ── SequentialWithReverse type ─────────────────────────────────────────────

mod sequential_with_reverse {
    use super::*;

    #[test]
    fn type_variant_exists_and_distinct() {
        assert_ne!(ShifterType::SequentialWithReverse, ShifterType::Sequential);
        assert_ne!(ShifterType::SequentialWithReverse, ShifterType::HPattern);
    }

    #[test]
    fn serde_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let t = ShifterType::SequentialWithReverse;
        let json = serde_json::to_string(&t).map_err(|e| e.to_string())?;
        let back: ShifterType = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        assert_eq!(t, back);
        Ok(())
    }

    #[test]
    fn capabilities_can_be_built_with_reverse() {
        let caps = ShifterCapabilities {
            shifter_type: ShifterType::SequentialWithReverse,
            max_gears: 7,
            has_clutch: false,
            has_paddle_shifters: true,
        };
        assert_eq!(caps.shifter_type, ShifterType::SequentialWithReverse);
        assert_eq!(caps.max_gears, 7);
    }

    #[test]
    fn capabilities_serde_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let caps = ShifterCapabilities {
            shifter_type: ShifterType::SequentialWithReverse,
            max_gears: 7,
            has_clutch: false,
            has_paddle_shifters: true,
        };
        let json = serde_json::to_string(&caps).map_err(|e| e.to_string())?;
        let back: ShifterCapabilities = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        assert_eq!(caps, back);
        Ok(())
    }
}

// ── Clutch parsing edge cases ─────────────────────────────────────────────

mod clutch_parsing {
    use super::*;

    #[test]
    fn clutch_min_value() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x00, 0x00, 0x01, 0x00, 0x00, 0x00];
        let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.clutch, Some(0));
        Ok(())
    }

    #[test]
    fn clutch_max_value() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x00, 0x00, 0x01, 0x00, 0xFF, 0xFF];
        let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.clutch, Some(u16::MAX));
        Ok(())
    }

    #[test]
    fn clutch_mid_value() -> Result<(), Box<dyn std::error::Error>> {
        // 0x8000 = 32768
        let data = [0x00, 0x00, 0x03, 0x00, 0x00, 0x80];
        let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.clutch, Some(0x8000));
        Ok(())
    }

    #[test]
    fn clutch_little_endian_encoding() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x00, 0x00, 0x01, 0x00, 0xAB, 0xCD];
        let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.clutch, Some(0xCDAB));
        Ok(())
    }

    #[test]
    fn combined_gear_paddles_clutch() -> Result<(), Box<dyn std::error::Error>> {
        // gear=5, both paddles, clutch=0x1234
        let data = [0x00, 0x00, 0x05, 0x30, 0x34, 0x12];
        let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.gear(), 5);
        assert!(input.paddle_up);
        assert!(input.paddle_down);
        assert_eq!(input.clutch, Some(0x1234));
        assert!(input.is_shifting());
        Ok(())
    }
}

// ── Neutral detection: comprehensive scenarios ────────────────────────────

mod neutral_detection {
    use super::*;

    #[test]
    fn neutral_from_multiple_raw_sources() {
        // All these should produce neutral
        let neutral_raws: Vec<u8> = {
            let mut v = vec![0u8, 0xFF];
            v.extend(8u8..=254);
            v
        };
        for raw in neutral_raws {
            let pos = GearPosition::from_raw(raw);
            assert!(
                pos.is_neutral,
                "raw {raw} should be neutral but gear={}",
                pos.gear
            );
        }
    }

    #[test]
    fn neutral_gear_constant_is_zero() {
        assert_eq!(NEUTRAL_GEAR, 0);
        assert!(GearPosition::new(NEUTRAL_GEAR).is_neutral);
    }

    #[test]
    fn neutral_is_not_reverse() {
        let neutral = GearPosition::neutral();
        assert!(!neutral.is_reverse);
    }

    #[test]
    fn default_shifter_input_is_neutral() {
        let input = ShifterInput::default();
        assert!(input.gear.is_neutral);
        assert_eq!(input.gear(), NEUTRAL_GEAR);
    }

    #[test]
    fn parse_gamepad_0xff_gives_neutral() -> Result<(), Box<dyn std::error::Error>> {
        let data = [0x00, 0x00, 0xFF, 0x00];
        let input = ShifterInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert!(input.gear.is_neutral);
        assert_eq!(input.gear(), 0);
        Ok(())
    }
}
