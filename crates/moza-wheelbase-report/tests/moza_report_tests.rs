//! Comprehensive tests for the Moza wheelbase report micro-crate.
//!
//! Covers: HID report parsing (all fields), known-good byte sequences from
//! Moza R9/R12/R16/R21 wheelbases, field scaling, report ID handling,
//! malformed report handling, and proptest fuzzing.

use racing_wheel_moza_wheelbase_report::{
    MIN_REPORT_LEN, RawWheelbaseReport, WheelbaseInputRaw, WheelbasePedalAxesRaw, input_report,
    parse_axis, parse_wheelbase_input_report, parse_wheelbase_pedal_axes, parse_wheelbase_report,
};

type R = Result<(), Box<dyn std::error::Error>>;

/// Build a full-length report with all fields populated.
#[allow(clippy::too_many_arguments)]
fn build_report(
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
// § HID report parsing — all fields
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn parse_all_fields_round_trip() -> R {
    let mut buttons = [0u8; input_report::BUTTONS_LEN];
    for (i, b) in buttons.iter_mut().enumerate() {
        *b = (i as u8).wrapping_add(0x10);
    }
    let report = build_report(
        0x1234,
        0x5678,
        0x9ABC,
        0xDEF0,
        0x2468,
        &buttons,
        0x0F,
        0xA5,
        [0x42, 0x99],
    );
    let parsed = parse_wheelbase_input_report(&report).ok_or("full fields round-trip")?;

    assert_eq!(parsed.steering, 0x1234);
    assert_eq!(parsed.pedals.throttle, 0x5678);
    assert_eq!(parsed.pedals.brake, 0x9ABC);
    assert_eq!(parsed.pedals.clutch, Some(0xDEF0));
    assert_eq!(parsed.pedals.handbrake, Some(0x2468));
    assert_eq!(parsed.buttons, buttons);
    assert_eq!(parsed.hat, 0x0F);
    assert_eq!(parsed.funky, 0xA5);
    assert_eq!(parsed.rotary, [0x42, 0x99]);
    Ok(())
}

#[test]
fn parse_all_zeros_report() -> R {
    let buttons = [0u8; input_report::BUTTONS_LEN];
    let report = build_report(0, 0, 0, 0, 0, &buttons, 0, 0, [0, 0]);
    let parsed = parse_wheelbase_input_report(&report).ok_or("all-zeros")?;
    assert_eq!(parsed.steering, 0);
    assert_eq!(parsed.pedals.throttle, 0);
    assert_eq!(parsed.pedals.brake, 0);
    assert_eq!(parsed.pedals.clutch, Some(0));
    assert_eq!(parsed.pedals.handbrake, Some(0));
    assert_eq!(parsed.hat, 0);
    assert_eq!(parsed.funky, 0);
    assert_eq!(parsed.rotary, [0, 0]);
    Ok(())
}

#[test]
fn parse_all_max_report() -> R {
    let buttons = [0xFF; input_report::BUTTONS_LEN];
    let report = build_report(
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
    let parsed = parse_wheelbase_input_report(&report).ok_or("all-max")?;
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
// § Known-good byte sequences — Moza R9/R12/R16/R21 style reports
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn moza_r9_center_position_idle() -> R {
    // R9: steering centered (0x8000), pedals released, no buttons
    let buttons = [0u8; input_report::BUTTONS_LEN];
    let report = build_report(
        0x8000,
        0x0000,
        0x0000,
        0x0000,
        0x0000,
        &buttons,
        0x00,
        0x00,
        [0x00, 0x00],
    );
    let parsed = parse_wheelbase_input_report(&report).ok_or("R9 idle")?;
    assert_eq!(parsed.steering, 0x8000); // center
    assert_eq!(parsed.pedals.throttle, 0);
    assert_eq!(parsed.pedals.brake, 0);
    Ok(())
}

#[test]
fn moza_r12_full_lock_left_with_throttle() -> R {
    // R12: steering full left (0x0000), full throttle (0xFFFF)
    let buttons = [0u8; input_report::BUTTONS_LEN];
    let report = build_report(
        0x0000,
        0xFFFF,
        0x0000,
        0x0000,
        0x0000,
        &buttons,
        0x00,
        0x00,
        [0x00, 0x00],
    );
    let parsed = parse_wheelbase_input_report(&report).ok_or("R12 full left+throttle")?;
    assert_eq!(parsed.steering, 0x0000);
    assert_eq!(parsed.pedals.throttle, 0xFFFF);
    assert_eq!(parsed.pedals.brake, 0);
    Ok(())
}

#[test]
fn moza_r16_full_lock_right_with_braking() -> R {
    // R16: steering full right (0xFFFF), heavy braking (0xC000)
    let buttons = [0u8; input_report::BUTTONS_LEN];
    let report = build_report(
        0xFFFF,
        0x0000,
        0xC000,
        0x0000,
        0x0000,
        &buttons,
        0x00,
        0x00,
        [0x00, 0x00],
    );
    let parsed = parse_wheelbase_input_report(&report).ok_or("R16 full right+brake")?;
    assert_eq!(parsed.steering, 0xFFFF);
    assert_eq!(parsed.pedals.brake, 0xC000);
    Ok(())
}

#[test]
fn moza_r21_all_pedals_with_buttons_and_hat() -> R {
    // R21: mid-steering, partial throttle, partial brake, clutch engaged, hat up-right
    let mut buttons = [0u8; input_report::BUTTONS_LEN];
    buttons[0] = 0x03; // buttons 0 and 1 pressed
    buttons[1] = 0x80; // button 15 pressed
    let report = build_report(
        0x6000,
        0x4000,
        0x2000,
        0x8000,
        0x0000,
        &buttons,
        0x01,
        0x00,
        [0x00, 0x00],
    );
    let parsed = parse_wheelbase_input_report(&report).ok_or("R21 full pedals")?;
    assert_eq!(parsed.steering, 0x6000);
    assert_eq!(parsed.pedals.throttle, 0x4000);
    assert_eq!(parsed.pedals.brake, 0x2000);
    assert_eq!(parsed.pedals.clutch, Some(0x8000));
    assert_eq!(parsed.hat, 0x01);
    assert_eq!(parsed.buttons[0], 0x03);
    assert_eq!(parsed.buttons[1], 0x80);
    Ok(())
}

#[test]
fn moza_r9_quick_steering_sweep() -> R {
    // Simulate a rapid steering sweep: left → center → right
    let sweep_values: &[u16] = &[
        0x0000, 0x2000, 0x4000, 0x6000, 0x8000, 0xA000, 0xC000, 0xFFFF,
    ];
    let buttons = [0u8; input_report::BUTTONS_LEN];
    for &steer in sweep_values {
        let report = build_report(steer, 0, 0, 0, 0, &buttons, 0, 0, [0, 0]);
        let parsed =
            parse_wheelbase_input_report(&report).ok_or(format!("sweep steer={steer:#06x}"))?;
        assert_eq!(parsed.steering, steer);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// § Field scaling and unit conversion
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn steering_axis_le_byte_order() -> R {
    // 0x5678: low=0x78, high=0x56
    let buttons = [0u8; input_report::BUTTONS_LEN];
    let report = build_report(0x5678, 0, 0, 0, 0, &buttons, 0, 0, [0, 0]);
    assert_eq!(report[input_report::STEERING_START], 0x78);
    assert_eq!(report[input_report::STEERING_START + 1], 0x56);
    let parsed = parse_wheelbase_input_report(&report).ok_or("LE steering")?;
    assert_eq!(parsed.steering, 0x5678);
    Ok(())
}

#[test]
fn all_axis_le_byte_order_verified() -> R {
    let buttons = [0u8; input_report::BUTTONS_LEN];
    let report = build_report(
        0xAABB,
        0xCCDD,
        0xEEFF,
        0x1122,
        0x3344,
        &buttons,
        0,
        0,
        [0, 0],
    );

    // Verify raw bytes are little-endian
    assert_eq!(report[input_report::STEERING_START], 0xBB);
    assert_eq!(report[input_report::STEERING_START + 1], 0xAA);
    assert_eq!(report[input_report::THROTTLE_START], 0xDD);
    assert_eq!(report[input_report::THROTTLE_START + 1], 0xCC);
    assert_eq!(report[input_report::BRAKE_START], 0xFF);
    assert_eq!(report[input_report::BRAKE_START + 1], 0xEE);
    assert_eq!(report[input_report::CLUTCH_START], 0x22);
    assert_eq!(report[input_report::CLUTCH_START + 1], 0x11);
    assert_eq!(report[input_report::HANDBRAKE_START], 0x44);
    assert_eq!(report[input_report::HANDBRAKE_START + 1], 0x33);

    let parsed = parse_wheelbase_input_report(&report).ok_or("LE all axes")?;
    assert_eq!(parsed.steering, 0xAABB);
    assert_eq!(parsed.pedals.throttle, 0xCCDD);
    assert_eq!(parsed.pedals.brake, 0xEEFF);
    assert_eq!(parsed.pedals.clutch, Some(0x1122));
    assert_eq!(parsed.pedals.handbrake, Some(0x3344));
    Ok(())
}

#[test]
fn axis_value_one_is_minimal_nonzero() -> R {
    let buttons = [0u8; input_report::BUTTONS_LEN];
    let report = build_report(1, 1, 1, 1, 1, &buttons, 0, 0, [0, 0]);
    let parsed = parse_wheelbase_input_report(&report).ok_or("minimal nonzero axes")?;
    assert_eq!(parsed.steering, 1);
    assert_eq!(parsed.pedals.throttle, 1);
    assert_eq!(parsed.pedals.brake, 1);
    assert_eq!(parsed.pedals.clutch, Some(1));
    assert_eq!(parsed.pedals.handbrake, Some(1));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// § Report ID handling
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn correct_report_id_accepted() -> R {
    let mut report = [0u8; MIN_REPORT_LEN];
    report[0] = input_report::REPORT_ID;
    let parsed = parse_wheelbase_report(&report).ok_or("correct ID")?;
    assert_eq!(parsed.report_id(), input_report::REPORT_ID);
    Ok(())
}

#[test]
fn report_id_zero_rejected() {
    let mut report = [0u8; MIN_REPORT_LEN];
    report[0] = 0x00;
    assert!(parse_wheelbase_report(&report).is_none());
}

#[test]
fn every_wrong_report_id_rejected() {
    for id in 0u8..=255 {
        if id == input_report::REPORT_ID {
            continue;
        }
        let mut report = [0u8; MIN_REPORT_LEN + 10];
        report[0] = id;
        assert!(
            parse_wheelbase_report(&report).is_none(),
            "id={id:#04x} should be rejected"
        );
        assert!(parse_wheelbase_pedal_axes(&report).is_none());
        assert!(parse_wheelbase_input_report(&report).is_none());
    }
}

#[test]
fn report_id_constant_is_0x01() {
    assert_eq!(input_report::REPORT_ID, 0x01);
}

// ═══════════════════════════════════════════════════════════════════════
// § Malformed report handling
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn empty_report_rejected_by_all_parsers() {
    assert!(parse_wheelbase_report(&[]).is_none());
    assert!(parse_wheelbase_pedal_axes(&[]).is_none());
    assert!(parse_wheelbase_input_report(&[]).is_none());
}

#[test]
fn single_byte_report_rejected() {
    assert!(parse_wheelbase_report(&[input_report::REPORT_ID]).is_none());
}

#[test]
fn one_byte_short_of_minimum_rejected() {
    let mut report = vec![0u8; MIN_REPORT_LEN - 1];
    report[0] = input_report::REPORT_ID;
    assert!(parse_wheelbase_report(&report).is_none());
}

#[test]
fn two_bytes_short_of_minimum_rejected() {
    if MIN_REPORT_LEN >= 3 {
        let mut report = vec![0u8; MIN_REPORT_LEN - 2];
        report[0] = input_report::REPORT_ID;
        assert!(parse_wheelbase_report(&report).is_none());
    }
}

#[test]
fn correct_id_but_too_short_for_pedal_axes() {
    // MIN_REPORT_LEN is needed for throttle+brake, which pedal_axes requires
    let mut report = vec![0u8; MIN_REPORT_LEN - 1];
    report[0] = input_report::REPORT_ID;
    assert!(parse_wheelbase_pedal_axes(&report).is_none());
}

#[test]
fn handbrake_needs_two_bytes_single_byte_is_none() -> R {
    let mut report = [0u8; input_report::HANDBRAKE_START + 1];
    report[0] = input_report::REPORT_ID;
    report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
        .copy_from_slice(&0x1111u16.to_le_bytes());
    report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
        .copy_from_slice(&0x2222u16.to_le_bytes());
    report[input_report::CLUTCH_START..input_report::CLUTCH_START + 2]
        .copy_from_slice(&0x3333u16.to_le_bytes());
    report[input_report::HANDBRAKE_START] = 0xFF;

    let parsed = parse_wheelbase_pedal_axes(&report).ok_or("single handbrake byte")?;
    assert_eq!(parsed.handbrake, None);
    assert_eq!(parsed.clutch, Some(0x3333));
    Ok(())
}

#[test]
fn clutch_needs_two_bytes() -> R {
    // Report ends after brake — no clutch bytes at all
    let report = [input_report::REPORT_ID, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
    let parsed = parse_wheelbase_pedal_axes(&report).ok_or("no clutch")?;
    assert_eq!(parsed.clutch, None);
    assert_eq!(parsed.handbrake, None);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// § Partial report zero-fill behavior
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn minimal_report_zero_fills_optional_fields() -> R {
    let report = [input_report::REPORT_ID, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
    let parsed = parse_wheelbase_input_report(&report).ok_or("minimal zero-fill")?;
    assert_eq!(parsed.pedals.clutch, None);
    assert_eq!(parsed.pedals.handbrake, None);
    assert_eq!(parsed.buttons, [0u8; input_report::BUTTONS_LEN]);
    assert_eq!(parsed.hat, 0);
    assert_eq!(parsed.funky, 0);
    assert_eq!(parsed.rotary, [0u8; input_report::ROTARY_LEN]);
    Ok(())
}

#[test]
fn partial_buttons_zero_filled() -> R {
    let n_partial = 7;
    let mut report = [0u8; input_report::BUTTONS_START + 7];
    report[0] = input_report::REPORT_ID;
    report[input_report::STEERING_START..input_report::STEERING_START + 2]
        .copy_from_slice(&0x1000u16.to_le_bytes());
    report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
        .copy_from_slice(&0x2000u16.to_le_bytes());
    report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
        .copy_from_slice(&0x3000u16.to_le_bytes());
    for i in 0..n_partial {
        report[input_report::BUTTONS_START + i] = (0xB0 + i) as u8;
    }

    let parsed = parse_wheelbase_input_report(&report).ok_or("partial buttons")?;
    for i in 0..n_partial {
        assert_eq!(parsed.buttons[i], (0xB0 + i) as u8, "button[{i}]");
    }
    for i in n_partial..input_report::BUTTONS_LEN {
        assert_eq!(parsed.buttons[i], 0, "button[{i}] should be zero");
    }
    Ok(())
}

#[test]
fn hat_and_funky_present_rotary_absent() -> R {
    let mut report = [0u8; input_report::ROTARY_START]; // ends before rotary
    report[0] = input_report::REPORT_ID;
    report[input_report::STEERING_START..input_report::STEERING_START + 2]
        .copy_from_slice(&0x1000u16.to_le_bytes());
    report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
        .copy_from_slice(&0x2000u16.to_le_bytes());
    report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
        .copy_from_slice(&0x3000u16.to_le_bytes());
    report[input_report::HAT_START] = 0x0F;
    report[input_report::FUNKY_START] = 0xAA;

    let parsed = parse_wheelbase_input_report(&report).ok_or("hat+funky no rotary")?;
    assert_eq!(parsed.hat, 0x0F);
    assert_eq!(parsed.funky, 0xAA);
    assert_eq!(parsed.rotary, [0, 0]);
    Ok(())
}

#[test]
fn partial_rotary_single_byte() -> R {
    let mut report = [0u8; input_report::ROTARY_START + 1];
    report[0] = input_report::REPORT_ID;
    report[input_report::STEERING_START..input_report::STEERING_START + 2]
        .copy_from_slice(&0x1000u16.to_le_bytes());
    report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
        .copy_from_slice(&0x2000u16.to_le_bytes());
    report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
        .copy_from_slice(&0x3000u16.to_le_bytes());
    report[input_report::ROTARY_START] = 0x77;

    let parsed = parse_wheelbase_input_report(&report).ok_or("partial rotary")?;
    assert_eq!(parsed.rotary, [0x77, 0x00]);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// § RawWheelbaseReport accessor edge cases
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
fn raw_report_byte_iteration() -> R {
    let data: Vec<u8> = (0u8..20).collect();
    let view = RawWheelbaseReport::new(&data);
    for (i, &expected) in data.iter().enumerate() {
        let got = view.byte(i).ok_or(format!("byte({i}) missing"))?;
        assert_eq!(got, expected);
    }
    assert_eq!(view.byte(data.len()), None);
    assert_eq!(view.byte(usize::MAX), None);
    Ok(())
}

#[test]
fn raw_report_axis_agrees_with_parse_axis() {
    let data = [0x01, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
    let view = RawWheelbaseReport::new(&data);
    for start in 0..data.len() {
        assert_eq!(
            view.axis_u16_le(start),
            parse_axis(&data, start),
            "mismatch at {start}"
        );
    }
}

#[test]
fn axis_u16_or_zero_out_of_bounds() {
    let view = RawWheelbaseReport::new(&[0x01, 0x02]);
    assert_eq!(view.axis_u16_or_zero(0), u16::from_le_bytes([0x01, 0x02]));
    assert_eq!(view.axis_u16_or_zero(1), 0);
    assert_eq!(view.axis_u16_or_zero(100), 0);
    assert_eq!(view.axis_u16_or_zero(usize::MAX), 0);
}

// ═══════════════════════════════════════════════════════════════════════
// § parse_axis edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn parse_axis_empty_none() {
    assert_eq!(parse_axis(&[], 0), None);
}

#[test]
fn parse_axis_single_byte_none() {
    assert_eq!(parse_axis(&[0xFF], 0), None);
}

#[test]
fn parse_axis_usize_max_none() {
    assert_eq!(parse_axis(&[0; 100], usize::MAX), None);
}

#[test]
fn parse_axis_exact_two_bytes() -> R {
    let val = parse_axis(&[0xAB, 0xCD], 0).ok_or("exact 2 bytes")?;
    assert_eq!(val, 0xCDAB);
    Ok(())
}

#[test]
fn parse_axis_at_offset() -> R {
    let data = [0x00, 0x34, 0x12, 0x00];
    let val = parse_axis(&data, 1).ok_or("offset 1")?;
    assert_eq!(val, 0x1234);
    Ok(())
}

#[test]
fn parse_axis_boundary_values_comprehensive() -> R {
    assert_eq!(parse_axis(&0u16.to_le_bytes(), 0).ok_or("zero")?, 0u16);
    assert_eq!(parse_axis(&1u16.to_le_bytes(), 0).ok_or("one")?, 1u16);
    assert_eq!(parse_axis(&255u16.to_le_bytes(), 0).ok_or("255")?, 255u16);
    assert_eq!(parse_axis(&256u16.to_le_bytes(), 0).ok_or("256")?, 256u16);
    assert_eq!(
        parse_axis(&u16::MAX.to_le_bytes(), 0).ok_or("max")?,
        u16::MAX
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// § Derive trait smoke tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn pedal_axes_raw_eq_and_ne() {
    let a = WheelbasePedalAxesRaw {
        throttle: 100,
        brake: 200,
        clutch: Some(300),
        handbrake: Some(400),
    };
    let b = a;
    assert_eq!(a, b);

    let c = WheelbasePedalAxesRaw {
        throttle: 101,
        brake: 200,
        clutch: Some(300),
        handbrake: Some(400),
    };
    assert_ne!(a, c);
}

#[test]
fn wheelbase_input_raw_copy_semantics() {
    let a = WheelbaseInputRaw {
        steering: 0xBBBB,
        pedals: WheelbasePedalAxesRaw {
            throttle: 1,
            brake: 2,
            clutch: Some(3),
            handbrake: Some(4),
        },
        buttons: [0xAA; input_report::BUTTONS_LEN],
        hat: 0x07,
        funky: 0xCC,
        rotary: [0x12, 0x34],
    };
    let b = a;
    let c = a;
    assert_eq!(a, b);
    assert_eq!(b, c);
}

#[test]
fn wheelbase_input_raw_debug_contains_field_names() {
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
// § Constant offset consistency
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn offsets_monotonically_increase() {
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
        assert!(pair[0] < pair[1], "{} >= {}", pair[0], pair[1]);
    }
}

#[test]
fn axis_fields_spaced_two_bytes() {
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
fn hat_funky_rotary_adjacency() {
    assert_eq!(
        input_report::HAT_START,
        input_report::BUTTONS_START + input_report::BUTTONS_LEN
    );
    assert_eq!(input_report::FUNKY_START, input_report::HAT_START + 1);
    assert_eq!(input_report::ROTARY_START, input_report::FUNKY_START + 1);
}

#[test]
fn min_report_len_matches_brake_end() {
    assert_eq!(MIN_REPORT_LEN, input_report::BRAKE_START + 2);
}

#[test]
fn buttons_len_and_rotary_len() {
    assert_eq!(input_report::BUTTONS_LEN, 16);
    assert_eq!(input_report::ROTARY_LEN, 2);
}

// ═══════════════════════════════════════════════════════════════════════
// § Proptest — arbitrary report bytes
// ═══════════════════════════════════════════════════════════════════════

use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(1024))]

    #[test]
    fn prop_steering_round_trips(value: u16) {
        let buttons = [0u8; input_report::BUTTONS_LEN];
        let report = build_report(value, 0, 0, 0, 0, &buttons, 0, 0, [0, 0]);
        if let Some(parsed) = parse_wheelbase_input_report(&report) {
            prop_assert_eq!(parsed.steering, value);
        }
    }

    #[test]
    fn prop_throttle_round_trips(value: u16) {
        let buttons = [0u8; input_report::BUTTONS_LEN];
        let report = build_report(0, value, 0, 0, 0, &buttons, 0, 0, [0, 0]);
        if let Some(parsed) = parse_wheelbase_input_report(&report) {
            prop_assert_eq!(parsed.pedals.throttle, value);
        }
    }

    #[test]
    fn prop_brake_round_trips(value: u16) {
        let buttons = [0u8; input_report::BUTTONS_LEN];
        let report = build_report(0, 0, value, 0, 0, &buttons, 0, 0, [0, 0]);
        if let Some(parsed) = parse_wheelbase_input_report(&report) {
            prop_assert_eq!(parsed.pedals.brake, value);
        }
    }

    #[test]
    fn prop_clutch_round_trips(value: u16) {
        let buttons = [0u8; input_report::BUTTONS_LEN];
        let report = build_report(0, 0, 0, value, 0, &buttons, 0, 0, [0, 0]);
        if let Some(parsed) = parse_wheelbase_input_report(&report) {
            prop_assert_eq!(parsed.pedals.clutch, Some(value));
        }
    }

    #[test]
    fn prop_handbrake_round_trips(value: u16) {
        let buttons = [0u8; input_report::BUTTONS_LEN];
        let report = build_report(0, 0, 0, 0, value, &buttons, 0, 0, [0, 0]);
        if let Some(parsed) = parse_wheelbase_input_report(&report) {
            prop_assert_eq!(parsed.pedals.handbrake, Some(value));
        }
    }

    #[test]
    fn prop_hat_round_trips(hat: u8) {
        let buttons = [0u8; input_report::BUTTONS_LEN];
        let report = build_report(0, 0, 0, 0, 0, &buttons, hat, 0, [0, 0]);
        if let Some(parsed) = parse_wheelbase_input_report(&report) {
            prop_assert_eq!(parsed.hat, hat);
        }
    }

    #[test]
    fn prop_funky_round_trips(funky: u8) {
        let buttons = [0u8; input_report::BUTTONS_LEN];
        let report = build_report(0, 0, 0, 0, 0, &buttons, 0, funky, [0, 0]);
        if let Some(parsed) = parse_wheelbase_input_report(&report) {
            prop_assert_eq!(parsed.funky, funky);
        }
    }

    #[test]
    fn prop_rotary_round_trips(r0: u8, r1: u8) {
        let buttons = [0u8; input_report::BUTTONS_LEN];
        let report = build_report(0, 0, 0, 0, 0, &buttons, 0, 0, [r0, r1]);
        if let Some(parsed) = parse_wheelbase_input_report(&report) {
            prop_assert_eq!(parsed.rotary, [r0, r1]);
        }
    }

    #[test]
    fn prop_wrong_report_id_always_rejected(id in 2u8..=255u8) {
        let mut report = [0u8; MIN_REPORT_LEN + 20];
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
    fn prop_parse_axis_oob_never_panics(len in 0usize..=32usize, start in 0usize..=40usize) {
        let buf = vec![0xDD; len];
        let result = parse_axis(&buf, start);
        if start.checked_add(2).is_some_and(|end| end <= len) {
            prop_assert!(result.is_some());
        } else {
            prop_assert!(result.is_none());
        }
    }

    #[test]
    fn prop_axis_u16_or_zero_agrees(lo in 0u8..=255u8, hi in 0u8..=255u8) {
        let data = [0x01, lo, hi];
        let view = RawWheelbaseReport::new(&data);
        let opt_val = view.axis_u16_le(1).unwrap_or(0);
        prop_assert_eq!(view.axis_u16_or_zero(1), opt_val);
    }

    #[test]
    fn prop_full_report_preserves_all_axes(
        steer: u16,
        throttle: u16,
        brake: u16,
        clutch: u16,
        handbrake: u16,
    ) {
        let buttons = [0u8; input_report::BUTTONS_LEN];
        let report = build_report(steer, throttle, brake, clutch, handbrake, &buttons, 0, 0, [0, 0]);
        if let Some(parsed) = parse_wheelbase_input_report(&report) {
            prop_assert_eq!(parsed.steering, steer);
            prop_assert_eq!(parsed.pedals.throttle, throttle);
            prop_assert_eq!(parsed.pedals.brake, brake);
            prop_assert_eq!(parsed.pedals.clutch, Some(clutch));
            prop_assert_eq!(parsed.pedals.handbrake, Some(handbrake));
        }
    }

    #[test]
    fn prop_arbitrary_length_report_never_panics(len in 0usize..=128usize) {
        let mut buf = vec![0u8; len];
        if !buf.is_empty() {
            buf[0] = input_report::REPORT_ID;
        }
        let _ = parse_wheelbase_report(&buf);
        let _ = parse_wheelbase_pedal_axes(&buf);
        let _ = parse_wheelbase_input_report(&buf);
    }
}
