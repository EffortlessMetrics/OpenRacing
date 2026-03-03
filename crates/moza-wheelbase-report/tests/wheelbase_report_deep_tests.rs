//! Deep tests for the Moza wheelbase report micro-crate.
//!
//! Covers: report parsing, field extraction, multi-byte endianness,
//! all report fields, and edge cases.

use racing_wheel_moza_wheelbase_report::{
    MIN_REPORT_LEN, RawWheelbaseReport, WheelbaseInputRaw, WheelbasePedalAxesRaw, input_report,
    parse_axis, parse_wheelbase_input_report, parse_wheelbase_pedal_axes, parse_wheelbase_report,
};

type R = Result<(), Box<dyn std::error::Error>>;

/// Build a full-length report with all fields set, returning a stack-allocated buffer.
#[allow(clippy::too_many_arguments)]
fn build_full_report(
    steering: u16,
    throttle: u16,
    brake: u16,
    clutch: u16,
    handbrake: u16,
    buttons: &[u8; input_report::BUTTONS_LEN],
    hat: u8,
    funky: u8,
    rotary: [u8; input_report::ROTARY_LEN],
) -> [u8; input_report::ROTARY_START + input_report::ROTARY_LEN] {
    let mut report = [0u8; input_report::ROTARY_START + input_report::ROTARY_LEN];
    report[0] = input_report::REPORT_ID;
    report[input_report::STEERING_START..input_report::STEERING_START + 2]
        .copy_from_slice(&steering.to_le_bytes());
    report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
        .copy_from_slice(&throttle.to_le_bytes());
    report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
        .copy_from_slice(&brake.to_le_bytes());
    report[input_report::CLUTCH_START..input_report::CLUTCH_START + 2]
        .copy_from_slice(&clutch.to_le_bytes());
    report[input_report::HANDBRAKE_START..input_report::HANDBRAKE_START + 2]
        .copy_from_slice(&handbrake.to_le_bytes());
    report[input_report::BUTTONS_START..input_report::BUTTONS_START + input_report::BUTTONS_LEN]
        .copy_from_slice(buttons);
    report[input_report::HAT_START] = hat;
    report[input_report::FUNKY_START] = funky;
    report[input_report::ROTARY_START..input_report::ROTARY_START + input_report::ROTARY_LEN]
        .copy_from_slice(&rotary);
    report
}

// ═══════════════════════════════════════════════════════════════════════
// § Report parsing — validation and rejection
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn rejects_empty_report() {
    assert!(parse_wheelbase_report(&[]).is_none());
    assert!(parse_wheelbase_pedal_axes(&[]).is_none());
    assert!(parse_wheelbase_input_report(&[]).is_none());
}

#[test]
fn rejects_report_id_zero() {
    let mut report = [0u8; MIN_REPORT_LEN];
    report[0] = 0x00;
    assert!(parse_wheelbase_report(&report).is_none());
}

#[test]
fn rejects_every_wrong_report_id() {
    for id in 2u8..=255 {
        let mut report = [0u8; MIN_REPORT_LEN];
        report[0] = id;
        assert!(
            parse_wheelbase_report(&report).is_none(),
            "id={id:#04x} should be rejected"
        );
    }
}

#[test]
fn rejects_one_byte_short_of_minimum() {
    let mut report = vec![0u8; MIN_REPORT_LEN - 1];
    report[0] = input_report::REPORT_ID;
    assert!(parse_wheelbase_report(&report).is_none());
}

#[test]
fn accepts_exactly_minimum_length() -> R {
    let mut report = [0u8; MIN_REPORT_LEN];
    report[0] = input_report::REPORT_ID;
    let parsed = parse_wheelbase_report(&report).ok_or("exact min should parse")?;
    assert_eq!(parsed.report_id(), input_report::REPORT_ID);
    Ok(())
}

#[test]
fn accepts_longer_than_minimum() -> R {
    let mut report = [0u8; MIN_REPORT_LEN + 50];
    report[0] = input_report::REPORT_ID;
    let parsed = parse_wheelbase_report(&report).ok_or("long report should parse")?;
    assert_eq!(parsed.report_bytes().len(), MIN_REPORT_LEN + 50);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// § Field extraction — multi-byte LE endianness
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn steering_le_byte_order() -> R {
    let buttons = [0u8; input_report::BUTTONS_LEN];
    // 0x5678: low byte 0x78, high byte 0x56
    let report = build_full_report(0x5678, 0, 0, 0, 0, &buttons, 0, 0, [0, 0]);
    let parsed = parse_wheelbase_input_report(&report).ok_or("steering LE test")?;
    assert_eq!(parsed.steering, 0x5678);
    // Verify raw bytes at the steering position
    assert_eq!(report[input_report::STEERING_START], 0x78); // low byte first
    assert_eq!(report[input_report::STEERING_START + 1], 0x56); // high byte second
    Ok(())
}

#[test]
fn throttle_le_byte_order() -> R {
    let buttons = [0u8; input_report::BUTTONS_LEN];
    let report = build_full_report(0, 0xABCD, 0, 0, 0, &buttons, 0, 0, [0, 0]);
    let parsed = parse_wheelbase_input_report(&report).ok_or("throttle LE test")?;
    assert_eq!(parsed.pedals.throttle, 0xABCD);
    assert_eq!(report[input_report::THROTTLE_START], 0xCD);
    assert_eq!(report[input_report::THROTTLE_START + 1], 0xAB);
    Ok(())
}

#[test]
fn brake_le_byte_order() -> R {
    let buttons = [0u8; input_report::BUTTONS_LEN];
    let report = build_full_report(0, 0, 0xDEAD, 0, 0, &buttons, 0, 0, [0, 0]);
    let parsed = parse_wheelbase_input_report(&report).ok_or("brake LE test")?;
    assert_eq!(parsed.pedals.brake, 0xDEAD);
    assert_eq!(report[input_report::BRAKE_START], 0xAD);
    assert_eq!(report[input_report::BRAKE_START + 1], 0xDE);
    Ok(())
}

#[test]
fn clutch_le_byte_order() -> R {
    let buttons = [0u8; input_report::BUTTONS_LEN];
    let report = build_full_report(0, 0, 0, 0xFACE, 0, &buttons, 0, 0, [0, 0]);
    let parsed = parse_wheelbase_input_report(&report).ok_or("clutch LE test")?;
    assert_eq!(parsed.pedals.clutch, Some(0xFACE));
    assert_eq!(report[input_report::CLUTCH_START], 0xCE);
    assert_eq!(report[input_report::CLUTCH_START + 1], 0xFA);
    Ok(())
}

#[test]
fn handbrake_le_byte_order() -> R {
    let buttons = [0u8; input_report::BUTTONS_LEN];
    let report = build_full_report(0, 0, 0, 0, 0xBEEF, &buttons, 0, 0, [0, 0]);
    let parsed = parse_wheelbase_input_report(&report).ok_or("handbrake LE test")?;
    assert_eq!(parsed.pedals.handbrake, Some(0xBEEF));
    assert_eq!(report[input_report::HANDBRAKE_START], 0xEF);
    assert_eq!(report[input_report::HANDBRAKE_START + 1], 0xBE);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// § All report fields — full round-trip
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn full_report_all_fields_round_trip() -> R {
    let mut buttons = [0u8; input_report::BUTTONS_LEN];
    for (i, b) in buttons.iter_mut().enumerate() {
        *b = (i as u8).wrapping_mul(17); // deterministic pattern
    }
    let report = build_full_report(
        0x1234,
        0x5678,
        0x9ABC,
        0xDEF0,
        0x1357,
        &buttons,
        0x0F,
        0xA5,
        [0x42, 0x99],
    );
    let parsed = parse_wheelbase_input_report(&report).ok_or("full round-trip")?;
    assert_eq!(parsed.steering, 0x1234);
    assert_eq!(parsed.pedals.throttle, 0x5678);
    assert_eq!(parsed.pedals.brake, 0x9ABC);
    assert_eq!(parsed.pedals.clutch, Some(0xDEF0));
    assert_eq!(parsed.pedals.handbrake, Some(0x1357));
    assert_eq!(parsed.buttons, buttons);
    assert_eq!(parsed.hat, 0x0F);
    assert_eq!(parsed.funky, 0xA5);
    assert_eq!(parsed.rotary, [0x42, 0x99]);
    Ok(())
}

#[test]
fn full_report_all_zeros() -> R {
    let buttons = [0u8; input_report::BUTTONS_LEN];
    let report = build_full_report(0, 0, 0, 0, 0, &buttons, 0, 0, [0, 0]);
    let parsed = parse_wheelbase_input_report(&report).ok_or("all-zero round-trip")?;
    assert_eq!(parsed.steering, 0);
    assert_eq!(parsed.pedals.throttle, 0);
    assert_eq!(parsed.pedals.brake, 0);
    assert_eq!(parsed.pedals.clutch, Some(0));
    assert_eq!(parsed.pedals.handbrake, Some(0));
    assert_eq!(parsed.buttons, buttons);
    assert_eq!(parsed.hat, 0);
    assert_eq!(parsed.funky, 0);
    assert_eq!(parsed.rotary, [0, 0]);
    Ok(())
}

#[test]
fn full_report_all_max() -> R {
    let buttons = [0xFF; input_report::BUTTONS_LEN];
    let report = build_full_report(
        u16::MAX,
        u16::MAX,
        u16::MAX,
        u16::MAX,
        u16::MAX,
        &buttons,
        0xFF,
        0xFF,
        [0xFF, 0xFF],
    );
    let parsed = parse_wheelbase_input_report(&report).ok_or("all-max round-trip")?;
    assert_eq!(parsed.steering, u16::MAX);
    assert_eq!(parsed.pedals.throttle, u16::MAX);
    assert_eq!(parsed.pedals.brake, u16::MAX);
    assert_eq!(parsed.pedals.clutch, Some(u16::MAX));
    assert_eq!(parsed.pedals.handbrake, Some(u16::MAX));
    assert_eq!(parsed.buttons, buttons);
    assert_eq!(parsed.hat, 0xFF);
    assert_eq!(parsed.funky, 0xFF);
    assert_eq!(parsed.rotary, [0xFF, 0xFF]);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// § Edge cases — partial reports and zero-fill behavior
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn minimal_report_has_no_optional_axes() -> R {
    let report = [input_report::REPORT_ID, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
    let parsed = parse_wheelbase_input_report(&report).ok_or("minimal input parse")?;
    assert_eq!(parsed.steering, 0x2211);
    assert_eq!(parsed.pedals.throttle, 0x4433);
    assert_eq!(parsed.pedals.brake, 0x6655);
    assert_eq!(parsed.pedals.clutch, None);
    assert_eq!(parsed.pedals.handbrake, None);
    assert_eq!(parsed.buttons, [0u8; input_report::BUTTONS_LEN]);
    assert_eq!(parsed.hat, 0);
    assert_eq!(parsed.funky, 0);
    assert_eq!(parsed.rotary, [0u8; input_report::ROTARY_LEN]);
    Ok(())
}

#[test]
fn report_with_clutch_but_no_handbrake() -> R {
    let mut report = [0u8; input_report::HANDBRAKE_START]; // ends just before handbrake
    report[0] = input_report::REPORT_ID;
    report[input_report::STEERING_START..input_report::STEERING_START + 2]
        .copy_from_slice(&0x1111u16.to_le_bytes());
    report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
        .copy_from_slice(&0x2222u16.to_le_bytes());
    report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
        .copy_from_slice(&0x3333u16.to_le_bytes());
    report[input_report::CLUTCH_START..input_report::CLUTCH_START + 2]
        .copy_from_slice(&0x4444u16.to_le_bytes());

    let parsed = parse_wheelbase_pedal_axes(&report).ok_or("clutch-only pedals")?;
    assert_eq!(parsed.clutch, Some(0x4444));
    assert_eq!(parsed.handbrake, None);
    Ok(())
}

#[test]
fn report_with_handbrake_but_no_buttons() -> R {
    let mut report = [0u8; input_report::BUTTONS_START]; // ends at buttons start
    report[0] = input_report::REPORT_ID;
    report[input_report::STEERING_START..input_report::STEERING_START + 2]
        .copy_from_slice(&0x1000u16.to_le_bytes());
    report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
        .copy_from_slice(&0x2000u16.to_le_bytes());
    report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
        .copy_from_slice(&0x3000u16.to_le_bytes());
    report[input_report::CLUTCH_START..input_report::CLUTCH_START + 2]
        .copy_from_slice(&0x4000u16.to_le_bytes());
    report[input_report::HANDBRAKE_START..input_report::HANDBRAKE_START + 2]
        .copy_from_slice(&0x5000u16.to_le_bytes());

    let parsed = parse_wheelbase_input_report(&report).ok_or("no buttons parse")?;
    assert_eq!(parsed.pedals.handbrake, Some(0x5000));
    assert_eq!(parsed.buttons, [0u8; input_report::BUTTONS_LEN]);
    Ok(())
}

#[test]
fn partial_button_bytes_are_zero_filled() -> R {
    let count = 5;
    let mut report = [0u8; input_report::BUTTONS_START + 5];
    report[0] = input_report::REPORT_ID;
    report[input_report::STEERING_START..input_report::STEERING_START + 2]
        .copy_from_slice(&0x0001u16.to_le_bytes());
    report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
        .copy_from_slice(&0x0002u16.to_le_bytes());
    report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
        .copy_from_slice(&0x0003u16.to_le_bytes());
    for i in 0..count {
        report[input_report::BUTTONS_START + i] = (0xA0 + i) as u8;
    }

    let parsed = parse_wheelbase_input_report(&report).ok_or("partial buttons")?;
    for i in 0..count {
        assert_eq!(parsed.buttons[i], (0xA0 + i) as u8, "button[{i}] mismatch");
    }
    for i in count..input_report::BUTTONS_LEN {
        assert_eq!(parsed.buttons[i], 0, "button[{i}] should be zero-filled");
    }
    Ok(())
}

#[test]
fn hat_and_funky_present_but_no_rotary() -> R {
    let mut report = [0u8; input_report::ROTARY_START]; // ends at rotary start
    report[0] = input_report::REPORT_ID;
    report[input_report::STEERING_START..input_report::STEERING_START + 2]
        .copy_from_slice(&0x1000u16.to_le_bytes());
    report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
        .copy_from_slice(&0x2000u16.to_le_bytes());
    report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
        .copy_from_slice(&0x3000u16.to_le_bytes());
    report[input_report::HAT_START] = 0x07;
    report[input_report::FUNKY_START] = 0xCC;

    let parsed = parse_wheelbase_input_report(&report).ok_or("hat+funky no rotary")?;
    assert_eq!(parsed.hat, 0x07);
    assert_eq!(parsed.funky, 0xCC);
    assert_eq!(parsed.rotary, [0, 0]);
    Ok(())
}

#[test]
fn partial_rotary_one_byte_only() -> R {
    let mut report = [0u8; input_report::ROTARY_START + 1];
    report[0] = input_report::REPORT_ID;
    report[input_report::STEERING_START..input_report::STEERING_START + 2]
        .copy_from_slice(&0x1000u16.to_le_bytes());
    report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
        .copy_from_slice(&0x2000u16.to_le_bytes());
    report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
        .copy_from_slice(&0x3000u16.to_le_bytes());
    report[input_report::ROTARY_START] = 0xEE;

    let parsed = parse_wheelbase_input_report(&report).ok_or("partial rotary")?;
    assert_eq!(parsed.rotary, [0xEE, 0x00]);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// § RawWheelbaseReport accessors
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn raw_report_empty_defaults() {
    let view = RawWheelbaseReport::new(&[]);
    assert_eq!(view.report_id(), 0);
    assert_eq!(view.byte(0), None);
    assert_eq!(view.axis_u16_le(0), None);
    assert_eq!(view.axis_u16_or_zero(0), 0);
    assert_eq!(view.report_bytes().len(), 0);
}

#[test]
fn raw_report_byte_at_every_position() -> R {
    let data: Vec<u8> = (0..=10).collect();
    let view = RawWheelbaseReport::new(&data);
    for (i, &expected) in data.iter().enumerate() {
        let got = view.byte(i).ok_or(format!("byte({i}) should exist"))?;
        assert_eq!(got, expected);
    }
    assert_eq!(view.byte(data.len()), None);
    Ok(())
}

#[test]
fn raw_report_axis_u16_le_matches_parse_axis() {
    let data = [0x01, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE];
    let view = RawWheelbaseReport::new(&data);
    for start in 0..data.len() {
        assert_eq!(
            view.axis_u16_le(start),
            parse_axis(&data, start),
            "mismatch at offset {start}"
        );
    }
}

#[test]
fn raw_report_axis_u16_or_zero_on_oob() {
    let view = RawWheelbaseReport::new(&[0x01, 0x02]);
    assert_eq!(view.axis_u16_or_zero(0), u16::from_le_bytes([0x01, 0x02]));
    assert_eq!(view.axis_u16_or_zero(1), 0); // only 1 byte left
    assert_eq!(view.axis_u16_or_zero(100), 0);
}

// ═══════════════════════════════════════════════════════════════════════
// § parse_axis — edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn parse_axis_empty_slice() {
    assert_eq!(parse_axis(&[], 0), None);
}

#[test]
fn parse_axis_single_byte_at_zero() {
    assert_eq!(parse_axis(&[0xFF], 0), None);
}

#[test]
fn parse_axis_usize_max_offset() {
    assert_eq!(parse_axis(&[0; 10], usize::MAX), None);
}

#[test]
fn parse_axis_usize_max_minus_one_offset() {
    assert_eq!(parse_axis(&[0; 10], usize::MAX - 1), None);
}

#[test]
fn parse_axis_boundary_values() -> R {
    let zero = parse_axis(&[0x00, 0x00], 0).ok_or("zero")?;
    assert_eq!(zero, 0);
    let max = parse_axis(&[0xFF, 0xFF], 0).ok_or("max")?;
    assert_eq!(max, u16::MAX);
    let one = parse_axis(&[0x01, 0x00], 0).ok_or("one")?;
    assert_eq!(one, 1);
    let high = parse_axis(&[0x00, 0x01], 0).ok_or("256")?;
    assert_eq!(high, 256);
    Ok(())
}

#[test]
fn parse_axis_at_nonzero_offset() -> R {
    let data = [0xFF, 0x34, 0x12, 0xFF];
    let val = parse_axis(&data, 1).ok_or("offset 1")?;
    assert_eq!(val, 0x1234);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// § Derive trait / struct verification
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn pedal_axes_raw_equality() {
    let a = WheelbasePedalAxesRaw {
        throttle: 100,
        brake: 200,
        clutch: Some(300),
        handbrake: Some(400),
    };
    let b = a;
    assert_eq!(a, b);
}

#[test]
fn pedal_axes_raw_inequality() {
    let a = WheelbasePedalAxesRaw {
        throttle: 100,
        brake: 200,
        clutch: Some(300),
        handbrake: Some(400),
    };
    let b = WheelbasePedalAxesRaw {
        throttle: 100,
        brake: 201,
        clutch: Some(300),
        handbrake: Some(400),
    };
    assert_ne!(a, b);
}

#[test]
fn pedal_axes_raw_optional_field_inequality() {
    let a = WheelbasePedalAxesRaw {
        throttle: 0,
        brake: 0,
        clutch: Some(0),
        handbrake: None,
    };
    let b = WheelbasePedalAxesRaw {
        throttle: 0,
        brake: 0,
        clutch: None,
        handbrake: None,
    };
    assert_ne!(a, b);
}

#[test]
fn wheelbase_input_raw_copy_semantics() {
    let a = WheelbaseInputRaw {
        steering: 0xAAAA,
        pedals: WheelbasePedalAxesRaw {
            throttle: 1,
            brake: 2,
            clutch: Some(3),
            handbrake: Some(4),
        },
        buttons: [0xFF; input_report::BUTTONS_LEN],
        hat: 0x0F,
        funky: 0xA5,
        rotary: [0x12, 0x34],
    };
    let b = a; // Copy
    let c = a; // still usable
    assert_eq!(a, b);
    assert_eq!(b, c);
}

#[test]
fn wheelbase_input_raw_debug_contains_fields() {
    let input = WheelbaseInputRaw {
        steering: 0x1234,
        pedals: WheelbasePedalAxesRaw {
            throttle: 10,
            brake: 20,
            clutch: None,
            handbrake: None,
        },
        buttons: [0u8; input_report::BUTTONS_LEN],
        hat: 5,
        funky: 6,
        rotary: [7, 8],
    };
    let dbg = format!("{input:?}");
    assert!(dbg.contains("steering"));
    assert!(dbg.contains("pedals"));
    assert!(dbg.contains("hat"));
    assert!(dbg.contains("funky"));
    assert!(dbg.contains("rotary"));
}

// ═══════════════════════════════════════════════════════════════════════
// § Constant consistency
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn offset_ordering_is_monotonic() {
    let offsets = [
        input_report::STEERING_START,
        input_report::THROTTLE_START,
        input_report::BRAKE_START,
        input_report::CLUTCH_START,
        input_report::HANDBRAKE_START,
        input_report::BUTTONS_START,
        input_report::HAT_START,
        input_report::FUNKY_START,
        input_report::ROTARY_START,
    ];
    for pair in offsets.windows(2) {
        assert!(
            pair[0] < pair[1],
            "offset order violation: {} >= {}",
            pair[0],
            pair[1]
        );
    }
}

#[test]
fn axis_fields_are_two_bytes_apart() {
    assert_eq!(
        input_report::THROTTLE_START - input_report::STEERING_START,
        2
    );
    assert_eq!(input_report::BRAKE_START - input_report::THROTTLE_START, 2);
    assert_eq!(input_report::CLUTCH_START - input_report::BRAKE_START, 2);
    assert_eq!(
        input_report::HANDBRAKE_START - input_report::CLUTCH_START,
        2
    );
}

#[test]
fn hat_follows_buttons_block() {
    assert_eq!(
        input_report::HAT_START,
        input_report::BUTTONS_START + input_report::BUTTONS_LEN
    );
}

#[test]
fn funky_follows_hat() {
    assert_eq!(input_report::FUNKY_START, input_report::HAT_START + 1);
}

#[test]
fn rotary_follows_funky() {
    assert_eq!(input_report::ROTARY_START, input_report::FUNKY_START + 1);
}

#[test]
fn min_report_len_is_brake_end() {
    assert_eq!(MIN_REPORT_LEN, input_report::BRAKE_START + 2);
}

#[test]
fn report_id_constant() {
    assert_eq!(input_report::REPORT_ID, 0x01);
}

#[test]
fn buttons_len_is_16() {
    assert_eq!(input_report::BUTTONS_LEN, 16);
}

#[test]
fn rotary_len_is_2() {
    assert_eq!(input_report::ROTARY_LEN, 2);
}

// ═══════════════════════════════════════════════════════════════════════
// § Edge case: single-byte handbrake offset (only low byte present)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn handbrake_single_byte_is_none() -> R {
    // Report extends only 1 byte into handbrake region
    let mut report = [0u8; input_report::HANDBRAKE_START + 1];
    report[0] = input_report::REPORT_ID;
    report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
        .copy_from_slice(&0x1111u16.to_le_bytes());
    report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
        .copy_from_slice(&0x2222u16.to_le_bytes());
    report[input_report::CLUTCH_START..input_report::CLUTCH_START + 2]
        .copy_from_slice(&0x3333u16.to_le_bytes());
    report[input_report::HANDBRAKE_START] = 0xAA; // only 1 byte — not enough for u16

    let parsed = parse_wheelbase_pedal_axes(&report).ok_or("single-byte handbrake")?;
    assert_eq!(parsed.clutch, Some(0x3333));
    assert_eq!(parsed.handbrake, None); // need 2 bytes for u16
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// § Edge case: button array exactly full
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn full_button_array_round_trip() -> R {
    let mut buttons = [0u8; input_report::BUTTONS_LEN];
    for (i, b) in buttons.iter_mut().enumerate() {
        *b = (i as u8) ^ 0xAA; // unique pattern per slot
    }

    let report = build_full_report(0, 0, 0, 0, 0, &buttons, 0, 0, [0, 0]);
    let parsed = parse_wheelbase_input_report(&report).ok_or("full buttons")?;
    assert_eq!(parsed.buttons, buttons);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// § Proptest — deep property-based coverage
// ═══════════════════════════════════════════════════════════════════════

use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(512))]

    #[test]
    fn prop_steering_round_trips(value: u16) {
        let buttons = [0u8; input_report::BUTTONS_LEN];
        let report = build_full_report(value, 0, 0, 0, 0, &buttons, 0, 0, [0, 0]);
        if let Some(parsed) = parse_wheelbase_input_report(&report) {
            prop_assert_eq!(parsed.steering, value);
        }
    }

    #[test]
    fn prop_throttle_round_trips(value: u16) {
        let buttons = [0u8; input_report::BUTTONS_LEN];
        let report = build_full_report(0, value, 0, 0, 0, &buttons, 0, 0, [0, 0]);
        if let Some(parsed) = parse_wheelbase_input_report(&report) {
            prop_assert_eq!(parsed.pedals.throttle, value);
        }
    }

    #[test]
    fn prop_brake_round_trips(value: u16) {
        let buttons = [0u8; input_report::BUTTONS_LEN];
        let report = build_full_report(0, 0, value, 0, 0, &buttons, 0, 0, [0, 0]);
        if let Some(parsed) = parse_wheelbase_input_report(&report) {
            prop_assert_eq!(parsed.pedals.brake, value);
        }
    }

    #[test]
    fn prop_clutch_round_trips(value: u16) {
        let buttons = [0u8; input_report::BUTTONS_LEN];
        let report = build_full_report(0, 0, 0, value, 0, &buttons, 0, 0, [0, 0]);
        if let Some(parsed) = parse_wheelbase_input_report(&report) {
            prop_assert_eq!(parsed.pedals.clutch, Some(value));
        }
    }

    #[test]
    fn prop_handbrake_round_trips(value: u16) {
        let buttons = [0u8; input_report::BUTTONS_LEN];
        let report = build_full_report(0, 0, 0, 0, value, &buttons, 0, 0, [0, 0]);
        if let Some(parsed) = parse_wheelbase_input_report(&report) {
            prop_assert_eq!(parsed.pedals.handbrake, Some(value));
        }
    }

    #[test]
    fn prop_hat_round_trips(hat: u8) {
        let buttons = [0u8; input_report::BUTTONS_LEN];
        let report = build_full_report(0, 0, 0, 0, 0, &buttons, hat, 0, [0, 0]);
        if let Some(parsed) = parse_wheelbase_input_report(&report) {
            prop_assert_eq!(parsed.hat, hat);
        }
    }

    #[test]
    fn prop_funky_round_trips(funky: u8) {
        let buttons = [0u8; input_report::BUTTONS_LEN];
        let report = build_full_report(0, 0, 0, 0, 0, &buttons, 0, funky, [0, 0]);
        if let Some(parsed) = parse_wheelbase_input_report(&report) {
            prop_assert_eq!(parsed.funky, funky);
        }
    }

    #[test]
    fn prop_rotary_round_trips(r0: u8, r1: u8) {
        let buttons = [0u8; input_report::BUTTONS_LEN];
        let report = build_full_report(0, 0, 0, 0, 0, &buttons, 0, 0, [r0, r1]);
        if let Some(parsed) = parse_wheelbase_input_report(&report) {
            prop_assert_eq!(parsed.rotary, [r0, r1]);
        }
    }

    #[test]
    fn prop_wrong_report_id_always_rejected(id in 2u8..=255u8) {
        let mut report = [0u8; MIN_REPORT_LEN + 10];
        report[0] = id;
        prop_assert!(parse_wheelbase_report(&report).is_none());
        prop_assert!(parse_wheelbase_pedal_axes(&report).is_none());
        prop_assert!(parse_wheelbase_input_report(&report).is_none());
    }

    #[test]
    fn prop_parse_axis_round_trips(lo in 0u8..=255u8, hi in 0u8..=255u8) {
        let expected = u16::from_le_bytes([lo, hi]);
        prop_assert_eq!(parse_axis(&[lo, hi], 0), Some(expected));
    }

    #[test]
    fn prop_parse_axis_oob_returns_none(
        len in 0usize..=16usize,
        start in 0usize..=20usize,
    ) {
        let buf = vec![0xBB; len];
        let result = parse_axis(&buf, start);
        if start.checked_add(2).is_some_and(|end| end <= len) {
            prop_assert!(result.is_some());
        } else {
            prop_assert!(result.is_none());
        }
    }

    #[test]
    fn prop_axis_u16_or_zero_agrees_with_option(
        lo in 0u8..=255u8,
        hi in 0u8..=255u8,
    ) {
        let data = [0x01, lo, hi];
        let view = RawWheelbaseReport::new(&data);
        let opt_val = view.axis_u16_le(1).unwrap_or(0);
        prop_assert_eq!(view.axis_u16_or_zero(1), opt_val);
    }

    #[test]
    fn prop_full_report_all_fields_preserved(
        steer: u16,
        throttle: u16,
        brake: u16,
        hat: u8,
        funky: u8,
    ) {
        let buttons = [0u8; input_report::BUTTONS_LEN];
        let report = build_full_report(steer, throttle, brake, 0, 0, &buttons, hat, funky, [0, 0]);
        if let Some(parsed) = parse_wheelbase_input_report(&report) {
            prop_assert_eq!(parsed.steering, steer);
            prop_assert_eq!(parsed.pedals.throttle, throttle);
            prop_assert_eq!(parsed.pedals.brake, brake);
            prop_assert_eq!(parsed.hat, hat);
            prop_assert_eq!(parsed.funky, funky);
        }
    }
}
