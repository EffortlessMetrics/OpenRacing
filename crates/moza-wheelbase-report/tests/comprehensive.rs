#![allow(clippy::redundant_closure)]

use racing_wheel_moza_wheelbase_report::{
    MIN_REPORT_LEN, RawWheelbaseReport, WheelbaseInputRaw, WheelbasePedalAxesRaw, input_report,
    parse_axis, parse_wheelbase_input_report, parse_wheelbase_pedal_axes, parse_wheelbase_report,
};

type R = Result<(), Box<dyn std::error::Error>>;

// ── Report validation ───────────────────────────────────────────────────

#[test]
fn rejects_empty_input() {
    assert!(parse_wheelbase_report(&[]).is_none());
}

#[test]
fn rejects_wrong_report_id() {
    let report = [0x02u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    assert!(parse_wheelbase_report(&report).is_none());
}

#[test]
fn rejects_too_short() {
    let mut report = vec![0u8; MIN_REPORT_LEN - 1];
    report[0] = input_report::REPORT_ID;
    assert!(parse_wheelbase_report(&report).is_none());
}

#[test]
fn accepts_minimal_valid() -> R {
    let mut report = [0u8; MIN_REPORT_LEN];
    report[0] = input_report::REPORT_ID;
    let parsed = parse_wheelbase_report(&report).ok_or("minimal valid should parse")?;
    assert_eq!(parsed.report_id(), input_report::REPORT_ID);
    Ok(())
}

// ── Pedal axes parsing ──────────────────────────────────────────────────

#[test]
fn pedal_axes_full_report() -> R {
    let mut report = [0u8; input_report::HANDBRAKE_START + 2];
    report[0] = input_report::REPORT_ID;
    report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
        .copy_from_slice(&0x1234u16.to_le_bytes());
    report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
        .copy_from_slice(&0x5678u16.to_le_bytes());
    report[input_report::CLUTCH_START..input_report::CLUTCH_START + 2]
        .copy_from_slice(&0x9ABCu16.to_le_bytes());
    report[input_report::HANDBRAKE_START..input_report::HANDBRAKE_START + 2]
        .copy_from_slice(&0xCDEFu16.to_le_bytes());

    let parsed = parse_wheelbase_pedal_axes(&report).ok_or("pedal axes should parse")?;
    assert_eq!(parsed.throttle, 0x1234);
    assert_eq!(parsed.brake, 0x5678);
    assert_eq!(parsed.clutch, Some(0x9ABC));
    assert_eq!(parsed.handbrake, Some(0xCDEF));
    Ok(())
}

#[test]
fn pedal_axes_clutch_present_handbrake_absent() -> R {
    let mut report = [0u8; input_report::HANDBRAKE_START];
    report[0] = input_report::REPORT_ID;
    report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
        .copy_from_slice(&0x1111u16.to_le_bytes());
    report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
        .copy_from_slice(&0x2222u16.to_le_bytes());
    report[input_report::CLUTCH_START..input_report::CLUTCH_START + 2]
        .copy_from_slice(&0x3333u16.to_le_bytes());

    let parsed = parse_wheelbase_pedal_axes(&report).ok_or("clutch-only parse")?;
    assert_eq!(parsed.clutch, Some(0x3333));
    assert_eq!(parsed.handbrake, None);
    Ok(())
}

#[test]
fn pedal_axes_rejects_wrong_id() {
    let report = [0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    assert!(parse_wheelbase_pedal_axes(&report).is_none());
}

// ── Full input report parsing ───────────────────────────────────────────

#[test]
fn full_input_round_trip() -> R {
    let steering: u16 = 0xBEEF;
    let throttle: u16 = 0xCAFE;
    let brake: u16 = 0xDEAD;

    let mut report = [0u8; input_report::ROTARY_START + input_report::ROTARY_LEN];
    report[0] = input_report::REPORT_ID;
    report[input_report::STEERING_START..input_report::STEERING_START + 2]
        .copy_from_slice(&steering.to_le_bytes());
    report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
        .copy_from_slice(&throttle.to_le_bytes());
    report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
        .copy_from_slice(&brake.to_le_bytes());
    report[input_report::HAT_START] = 0x07;
    report[input_report::FUNKY_START] = 0x0A;
    report[input_report::ROTARY_START] = 0x55;
    report[input_report::ROTARY_START + 1] = 0xAA;

    let parsed = parse_wheelbase_input_report(&report).ok_or("full input should parse")?;
    assert_eq!(parsed.steering, steering);
    assert_eq!(parsed.pedals.throttle, throttle);
    assert_eq!(parsed.pedals.brake, brake);
    assert_eq!(parsed.hat, 0x07);
    assert_eq!(parsed.funky, 0x0A);
    assert_eq!(parsed.rotary, [0x55, 0xAA]);
    Ok(())
}

#[test]
fn input_zero_fills_missing_controls() -> R {
    let report = [input_report::REPORT_ID, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
    let parsed = parse_wheelbase_input_report(&report).ok_or("minimal input should parse")?;
    assert_eq!(parsed.steering, 0x2211);
    assert_eq!(parsed.pedals.throttle, 0x4433);
    assert_eq!(parsed.pedals.brake, 0x6655);
    assert_eq!(parsed.buttons, [0u8; input_report::BUTTONS_LEN]);
    assert_eq!(parsed.hat, 0);
    assert_eq!(parsed.funky, 0);
    assert_eq!(parsed.rotary, [0u8; input_report::ROTARY_LEN]);
    Ok(())
}

#[test]
fn partial_buttons_preserved() -> R {
    let mut report = [0u8; input_report::BUTTONS_START + 3];
    report[0] = input_report::REPORT_ID;
    report[input_report::STEERING_START..input_report::STEERING_START + 2]
        .copy_from_slice(&0x2211u16.to_le_bytes());
    report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
        .copy_from_slice(&0x4433u16.to_le_bytes());
    report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
        .copy_from_slice(&0x6655u16.to_le_bytes());
    report[input_report::BUTTONS_START] = 0xA1;
    report[input_report::BUTTONS_START + 1] = 0xB2;
    report[input_report::BUTTONS_START + 2] = 0xC3;

    let parsed = parse_wheelbase_input_report(&report).ok_or("partial buttons should parse")?;
    assert_eq!(parsed.buttons[0], 0xA1);
    assert_eq!(parsed.buttons[1], 0xB2);
    assert_eq!(parsed.buttons[2], 0xC3);
    assert_eq!(parsed.buttons[3..], [0u8; input_report::BUTTONS_LEN - 3]);
    Ok(())
}

#[test]
fn partial_rotary() -> R {
    let mut report = [0u8; input_report::ROTARY_START + 1];
    report[0] = input_report::REPORT_ID;
    report[input_report::STEERING_START..input_report::STEERING_START + 2]
        .copy_from_slice(&0x1000u16.to_le_bytes());
    report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
        .copy_from_slice(&0x2000u16.to_le_bytes());
    report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
        .copy_from_slice(&0x3000u16.to_le_bytes());
    report[input_report::ROTARY_START] = 0x77;

    let parsed = parse_wheelbase_input_report(&report).ok_or("partial rotary should parse")?;
    assert_eq!(parsed.rotary, [0x77, 0x00]);
    Ok(())
}

// ── RawWheelbaseReport accessors ────────────────────────────────────────

#[test]
fn raw_report_accessors() -> R {
    let data = [0x01, 0xAA, 0xBB, 0xCC, 0xDD];
    let view = RawWheelbaseReport::new(&data);
    assert_eq!(view.report_id(), 0x01);
    assert_eq!(view.report_bytes(), &data);
    assert_eq!(view.byte(1), Some(0xAA));
    assert_eq!(view.byte(5), None);
    assert_eq!(view.axis_u16_le(1), Some(0xBBAA));
    assert_eq!(view.axis_u16_or_zero(100), 0);
    Ok(())
}

#[test]
fn raw_report_empty_id_defaults_to_zero() {
    let view = RawWheelbaseReport::new(&[]);
    assert_eq!(view.report_id(), 0);
}

// ── parse_axis edge cases ───────────────────────────────────────────────

#[test]
fn parse_axis_empty() {
    assert_eq!(parse_axis(&[], 0), None);
}

#[test]
fn parse_axis_single_byte() {
    assert_eq!(parse_axis(&[0x01], 0), None);
}

#[test]
fn parse_axis_usize_max() {
    assert_eq!(parse_axis(&[0x00, 0x00], usize::MAX), None);
}

#[test]
fn parse_axis_boundary() {
    assert_eq!(parse_axis(&[0x00, 0x00], 0), Some(0u16));
    assert_eq!(parse_axis(&[0xFF, 0xFF], 0), Some(u16::MAX));
}

// ── Derive/Eq smoke tests ───────────────────────────────────────────────

#[test]
fn pedal_axes_raw_eq() {
    let a = WheelbasePedalAxesRaw {
        throttle: 100,
        brake: 200,
        clutch: Some(300),
        handbrake: None,
    };
    let b = a;
    assert_eq!(a, b);
}

#[test]
fn wheelbase_input_raw_eq() {
    let a = WheelbaseInputRaw {
        steering: 0x1234,
        pedals: WheelbasePedalAxesRaw {
            throttle: 100,
            brake: 200,
            clutch: None,
            handbrake: None,
        },
        buttons: [0u8; input_report::BUTTONS_LEN],
        hat: 0,
        funky: 0,
        rotary: [0u8; input_report::ROTARY_LEN],
    };
    let b = a;
    assert_eq!(a, b);
}

// ── Constant consistency ────────────────────────────────────────────────

#[test]
fn input_report_constants_consistent() {
    // Use const assertions for compile-time-known values
    const _: () = assert!(input_report::STEERING_START < input_report::THROTTLE_START);
    const _: () = assert!(input_report::THROTTLE_START < input_report::BRAKE_START);
    const _: () = assert!(input_report::BRAKE_START < input_report::CLUTCH_START);
    const _: () = assert!(input_report::CLUTCH_START < input_report::HANDBRAKE_START);
    const _: () = assert!(input_report::HANDBRAKE_START < input_report::BUTTONS_START);
    assert_eq!(
        input_report::HAT_START,
        input_report::BUTTONS_START + input_report::BUTTONS_LEN
    );
    assert_eq!(input_report::FUNKY_START, input_report::HAT_START + 1);
    assert_eq!(input_report::ROTARY_START, input_report::FUNKY_START + 1);
    assert_eq!(MIN_REPORT_LEN, input_report::BRAKE_START + 2);
}

// ── Proptest ────────────────────────────────────────────────────────────

use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(256))]

    #[test]
    fn prop_parse_axis_round_trips(lo in 0u8..=255u8, hi in 0u8..=255u8) {
        let expected = u16::from_le_bytes([lo, hi]);
        prop_assert_eq!(parse_axis(&[lo, hi], 0), Some(expected));
    }

    #[test]
    fn prop_wrong_report_id_rejected(id in 2u8..=255u8) {
        let mut report = [0u8; MIN_REPORT_LEN + 4];
        report[0] = id;
        prop_assert!(parse_wheelbase_report(&report).is_none());
    }

    #[test]
    fn prop_steering_round_trips(lo in 0u8..=255u8, hi in 0u8..=255u8) {
        let steering = u16::from_le_bytes([lo, hi]);
        let mut report = [0u8; MIN_REPORT_LEN + 4];
        report[0] = input_report::REPORT_ID;
        report[input_report::STEERING_START] = lo;
        report[input_report::STEERING_START + 1] = hi;
        if let Some(parsed) = parse_wheelbase_input_report(&report) {
            prop_assert_eq!(parsed.steering, steering);
        }
    }

    #[test]
    fn prop_throttle_round_trips(lo in 0u8..=255u8, hi in 0u8..=255u8) {
        let throttle = u16::from_le_bytes([lo, hi]);
        let mut report = [0u8; MIN_REPORT_LEN + 4];
        report[0] = input_report::REPORT_ID;
        report[input_report::THROTTLE_START] = lo;
        report[input_report::THROTTLE_START + 1] = hi;
        if let Some(parsed) = parse_wheelbase_pedal_axes(&report) {
            prop_assert_eq!(parsed.throttle, throttle);
        }
    }

    #[test]
    fn prop_brake_round_trips(lo in 0u8..=255u8, hi in 0u8..=255u8) {
        let brake = u16::from_le_bytes([lo, hi]);
        let mut report = [0u8; MIN_REPORT_LEN + 4];
        report[0] = input_report::REPORT_ID;
        report[input_report::BRAKE_START] = lo;
        report[input_report::BRAKE_START + 1] = hi;
        if let Some(parsed) = parse_wheelbase_pedal_axes(&report) {
            prop_assert_eq!(parsed.brake, brake);
        }
    }

    #[test]
    fn prop_all_axes_preserved(
        steer in 0u16..=65535u16,
        throttle in 0u16..=65535u16,
        brake in 0u16..=65535u16,
    ) {
        let mut report = [0u8; MIN_REPORT_LEN + 4];
        report[0] = input_report::REPORT_ID;
        report[input_report::STEERING_START..input_report::STEERING_START + 2]
            .copy_from_slice(&steer.to_le_bytes());
        report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
            .copy_from_slice(&throttle.to_le_bytes());
        report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
            .copy_from_slice(&brake.to_le_bytes());
        if let Some(parsed) = parse_wheelbase_input_report(&report) {
            prop_assert_eq!(parsed.steering, steer);
            prop_assert_eq!(parsed.pedals.throttle, throttle);
            prop_assert_eq!(parsed.pedals.brake, brake);
        }
    }

    #[test]
    fn prop_axis_u16_or_zero_matches_option(lo in 0u8..=255u8, hi in 0u8..=255u8) {
        let data = [0x01, lo, hi];
        let view = RawWheelbaseReport::new(&data);
        let opt = view.axis_u16_le(1);
        let or_zero = view.axis_u16_or_zero(1);
        prop_assert_eq!(opt.unwrap_or(0), or_zero);
    }
}
