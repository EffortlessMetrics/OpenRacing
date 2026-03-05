//! Deep integration tests for the Logitech HID protocol crate.
//!
//! These tests exercise cross-module interactions, per-variant behaviour,
//! multi-step command sequences, and edge cases that are not covered by
//! existing unit, snapshot, or property-based tests.
//!
//! # Coverage focus
//!
//! - Per-variant input report simulation (G25, G27, G29, G920, G923, PRO)
//! - Slot-based FFB command sequences (start → update → stop)
//! - Spring deadband threshold boundary behaviour
//! - Damper/friction coefficient extremes
//! - Multi-model capability-gated operations
//! - Pedal 3-axis parsing with realistic per-variant values
//! - H-pattern shifter button detection
//! - Combined autocenter + range + mode-switch command sequences
//! - Proptest for slot encoding robustness
//! - Error handling and rejection of malformed data

use racing_wheel_hid_logitech_protocol::ids::{LOGITECH_VENDOR_ID, product_ids, report_ids};
use racing_wheel_hid_logitech_protocol::input::parse_input_report;
use racing_wheel_hid_logitech_protocol::mode::{
    G923_PS_REPORT_ID, LED_COUNT, MAX_RANGE, MIN_RANGE, REPORT_SIZE, TargetMode, encode_autocenter,
    encode_autocenter_off, encode_dfp_native_mode, encode_g25_native_mode,
    encode_g923_ps_mode_switch, encode_leds, encode_mode_switch, encode_range,
};
use racing_wheel_hid_logitech_protocol::output::{
    CONSTANT_FORCE_REPORT_LEN, LogitechConstantForceEncoder, build_gain_report,
    build_mode_switch_report, build_native_mode_report, build_set_leds_report,
    build_set_range_dfp_reports, build_set_range_report,
};
use racing_wheel_hid_logitech_protocol::slots::{
    SLOT_CMD_SIZE, effect_type, encode_constant, encode_damper, encode_friction, encode_slot_stop,
    encode_spring, op, slot, translate_force,
};
use racing_wheel_hid_logitech_protocol::types::{LogitechModel, is_wheel_product};

// ═══════════════════════════════════════════════════════════════════════════════
// Helper: build a synthetic input report for a given wheel variant
// ═══════════════════════════════════════════════════════════════════════════════

/// Build a 12-byte standard input report with specified axis and button values.
fn build_input_report(
    steering_raw: u16,
    throttle: u8,
    brake: u8,
    clutch: u8,
    buttons: u16,
    hat: u8,
    paddles: u8,
) -> [u8; 12] {
    let [s_lo, s_hi] = steering_raw.to_le_bytes();
    let [b_lo, b_hi] = buttons.to_le_bytes();
    [
        report_ids::STANDARD_INPUT,
        s_lo,
        s_hi,
        throttle,
        brake,
        clutch,
        b_lo,
        b_hi,
        hat & 0x0F,
        paddles & 0x03,
        0x00,
        0x00,
    ]
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Per-variant input report simulation
// ═══════════════════════════════════════════════════════════════════════════════

/// Simulate a G25 in a realistic driving scenario: half-right steering,
/// partial throttle, light braking, clutch released.
#[test]
fn g25_realistic_driving_input() -> Result<(), Box<dyn std::error::Error>> {
    let model = LogitechModel::from_product_id(product_ids::G25);
    assert_eq!(model, LogitechModel::G25);

    let report = build_input_report(0xC000, 0x80, 0x30, 0x00, 0x0000, 0x08, 0x00);
    let state = parse_input_report(&report).ok_or("G25 report parse failed")?;

    // Steering: 0xC000 → (0xC000 - 32768) / 32768 ≈ 0.5
    assert!(
        (state.steering - 0.5).abs() < 0.01,
        "G25 half-right steering: got {}",
        state.steering
    );
    assert!(state.throttle > 0.4 && state.throttle < 0.6);
    assert!(state.brake > 0.1 && state.brake < 0.3);
    assert!(state.clutch < 0.01, "clutch should be released");
    assert_eq!(state.hat, 0x08, "hat neutral");
    Ok(())
}

/// Simulate a G27 with H-pattern shifter engaged (buttons encode gear).
/// G27 H-pattern gears map to button bits in the 16-bit field.
#[test]
fn g27_hpattern_shifter_buttons() -> Result<(), Box<dyn std::error::Error>> {
    let model = LogitechModel::from_product_id(product_ids::G27);
    assert_eq!(model, LogitechModel::G27);

    // Gear 3 mapped to bit 10 in our 16-bit button field
    let gear3_button: u16 = 1 << 10;
    let report = build_input_report(0x8000, 0x00, 0x00, 0x00, gear3_button, 0x08, 0x00);
    let state = parse_input_report(&report).ok_or("G27 shifter parse failed")?;

    assert_eq!((state.buttons >> 10) & 1, 1, "gear 3 button must be set");
    // Other nearby gear buttons should be clear
    for bit in [8, 9, 11] {
        assert_eq!(
            (state.buttons >> bit) & 1,
            0,
            "gear button bit {} must be clear",
            bit
        );
    }
    Ok(())
}

/// Simulate a G29 with full-lock left steering and maximum braking.
#[test]
fn g29_full_lock_left_heavy_braking() -> Result<(), Box<dyn std::error::Error>> {
    let model = LogitechModel::from_product_id(product_ids::G29_PS);
    assert_eq!(model, LogitechModel::G29);

    let report = build_input_report(0x0000, 0x00, 0xFF, 0x00, 0x0000, 0x08, 0x00);
    let state = parse_input_report(&report).ok_or("G29 report parse failed")?;

    assert!(
        (state.steering + 1.0).abs() < 0.001,
        "full left: got {}",
        state.steering
    );
    assert!((state.brake - 1.0).abs() < 0.01, "full brake");
    assert!(state.throttle < 0.01, "throttle released");
    Ok(())
}

/// Simulate a G920 with both paddle shifters pulled.
#[test]
fn g920_both_paddles_pulled() -> Result<(), Box<dyn std::error::Error>> {
    let model = LogitechModel::from_product_id(product_ids::G920);
    assert_eq!(model, LogitechModel::G920);

    let report = build_input_report(0x8000, 0x00, 0x00, 0x00, 0x0000, 0x08, 0x03);
    let state = parse_input_report(&report).ok_or("G920 paddle parse failed")?;

    assert_eq!(state.paddles, 0x03, "both paddles engaged");
    assert_eq!(state.paddles & 0x01, 1, "right/upshift paddle");
    assert_eq!((state.paddles >> 1) & 1, 1, "left/downshift paddle");
    Ok(())
}

/// Simulate a G923 with partial throttle blip and D-pad right.
#[test]
fn g923_throttle_blip_dpad_right() -> Result<(), Box<dyn std::error::Error>> {
    let model = LogitechModel::from_product_id(product_ids::G923);
    assert_eq!(model, LogitechModel::G923);

    // D-pad right = 0x02 in standard HID hat encoding
    let report = build_input_report(0x8000, 0x40, 0x00, 0x00, 0x0000, 0x02, 0x00);
    let state = parse_input_report(&report).ok_or("G923 report parse failed")?;

    assert!(
        state.throttle > 0.2 && state.throttle < 0.3,
        "partial throttle blip"
    );
    assert_eq!(state.hat, 0x02, "D-pad right");
    Ok(())
}

/// Simulate a PRO Racing Wheel with 3-pedal input at various positions.
#[test]
fn gpro_three_pedal_input() -> Result<(), Box<dyn std::error::Error>> {
    let model = LogitechModel::from_product_id(product_ids::G_PRO);
    assert_eq!(model, LogitechModel::GPro);

    // Full throttle, half brake (trail braking), quarter clutch
    let report = build_input_report(0x8000, 0xFF, 0x80, 0x40, 0x0000, 0x08, 0x00);
    let state = parse_input_report(&report).ok_or("G PRO parse failed")?;

    assert!((state.throttle - 1.0).abs() < 0.01, "full throttle");
    assert!(
        (state.brake - 0.502).abs() < 0.01,
        "half brake: got {}",
        state.brake
    );
    assert!(
        (state.clutch - 0.251).abs() < 0.01,
        "quarter clutch: got {}",
        state.clutch
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Slot-based FFB command sequences
// ═══════════════════════════════════════════════════════════════════════════════

/// Full constant force lifecycle: start → update → stop.
#[test]
fn constant_force_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    let start = encode_constant(op::START, 0);
    assert_eq!(start[0] & 0x0F, op::START, "start operation");
    assert_eq!(start[0] >> 4, slot::CONSTANT, "constant slot");
    assert_eq!(start[2], 0x80, "center force at start");

    let update = encode_constant(op::UPDATE, 0x4000);
    assert_eq!(update[0] & 0x0F, op::UPDATE, "update operation");
    assert!(update[2] > 0x80, "positive force offset from center");

    let stop = encode_slot_stop(slot::CONSTANT);
    assert_eq!(stop[0] & 0x0F, op::STOP, "stop operation");
    assert_eq!(&stop[1..], &[0u8; 6], "stop payload zeroed");
    Ok(())
}

/// Spring + damper running simultaneously in different slots.
#[test]
fn spring_and_damper_simultaneous() -> Result<(), Box<dyn std::error::Error>> {
    let spring = encode_spring(op::START, 0, 0, 0x4000, 0x4000, 0xFFFF);
    let damper = encode_damper(op::START, 0x2000, 0x2000, 0x8000);

    // Verify they target different slots
    let spring_slot = spring[0] >> 4;
    let damper_slot = damper[0] >> 4;
    assert_eq!(spring_slot, slot::SPRING);
    assert_eq!(damper_slot, slot::DAMPER);
    assert_ne!(spring_slot, damper_slot, "must be different slots");

    // Verify effect type bytes
    assert_eq!(spring[1], effect_type::SPRING);
    assert_eq!(damper[1], effect_type::DAMPER);
    Ok(())
}

/// All four slots can be active simultaneously.
#[test]
fn all_four_slots_active() -> Result<(), Box<dyn std::error::Error>> {
    let constant = encode_constant(op::START, 0x2000);
    let spring = encode_spring(op::START, 0, 0, 0x3000, 0x3000, 0x8000);
    let damper = encode_damper(op::START, 0x1000, 0x1000, 0x4000);
    let friction = encode_friction(op::START, 0x1000, 0x1000, 0x4000);

    let slots_used: [u8; 4] = [
        constant[0] >> 4,
        spring[0] >> 4,
        damper[0] >> 4,
        friction[0] >> 4,
    ];

    // All four slots (0, 1, 2, 3) must be distinct
    for i in 0..4 {
        for j in (i + 1)..4 {
            assert_ne!(
                slots_used[i], slots_used[j],
                "slots {} and {} collide: both use slot {}",
                i, j, slots_used[i]
            );
        }
    }

    // All should be START operations
    for cmd in [&constant, &spring, &damper, &friction] {
        assert_eq!(cmd[0] & 0x0F, op::START);
    }
    Ok(())
}

/// Stopping each slot produces correctly formed stop commands.
#[test]
fn stop_all_slots() -> Result<(), Box<dyn std::error::Error>> {
    for slot_id in [slot::CONSTANT, slot::SPRING, slot::DAMPER, slot::FRICTION] {
        let cmd = encode_slot_stop(slot_id);
        assert_eq!(cmd[0], (slot_id << 4) | op::STOP);
        assert_eq!(
            &cmd[1..],
            &[0u8; 6],
            "stop payload for slot {} must be all zeros",
            slot_id
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Spring deadband threshold boundary behaviour
// ═══════════════════════════════════════════════════════════════════════════════

/// Spring with coefficients below the 2048 deadband threshold.
#[test]
fn spring_below_deadband_threshold() -> Result<(), Box<dyn std::error::Error>> {
    // k1=1000, k2=1000 — both below 2048 threshold
    let cmd = encode_spring(op::START, 0, 0, 1000, 1000, 0xFFFF);
    assert_eq!(cmd[0], (slot::SPRING << 4) | op::START);
    assert_eq!(cmd[1], effect_type::SPRING);
    // Below threshold: d1 should be 0, d2 should be 2047 (0x7FF)
    // d1_final=0 → d1_final >> 3 = 0
    assert_eq!(cmd[2], 0x00, "d1 zeroed below threshold");
    // d2_final=2047 → 2047 >> 3 = 255
    assert_eq!(cmd[3], 0xFF, "d2 maxed below threshold");
    Ok(())
}

/// Spring with coefficients at exactly the 2048 deadband boundary.
#[test]
fn spring_at_deadband_boundary() -> Result<(), Box<dyn std::error::Error>> {
    let cmd_below = encode_spring(op::START, 0, 0, 2047, 2047, 0xFFFF);
    let cmd_at = encode_spring(op::START, 0, 0, 2048, 2048, 0xFFFF);

    // Below threshold: d1=0, d2=2047
    assert_eq!(cmd_below[2], 0x00, "below threshold: d1 zeroed");
    assert_eq!(cmd_below[3], 0xFF, "below threshold: d2=2047>>3=255");

    // At/above threshold: deadband positions come from actual d1/d2
    // The behaviour changes at 2048 — the deadband positions are no longer forced
    assert_ne!(
        cmd_below[2..=3],
        cmd_at[2..=3],
        "crossing the 2048 threshold should change deadband encoding"
    );
    Ok(())
}

/// Spring with negative coefficients produces correct sign bits.
#[test]
fn spring_negative_coefficients() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = encode_spring(op::START, 0, 0, -5000, -5000, 0xFFFF);
    // s1 and s2 should both be 1 (negative)
    let s1 = cmd[5] & 0x01;
    let s2 = (cmd[5] >> 4) & 0x01;
    assert_eq!(s1, 1, "k1 negative → s1=1");
    assert_eq!(s2, 1, "k2 negative → s2=1");
    Ok(())
}

/// Spring with mixed sign coefficients.
#[test]
fn spring_mixed_sign_coefficients() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = encode_spring(op::START, 0, 0, 5000, -5000, 0xFFFF);
    let s1 = cmd[5] & 0x01;
    let s2 = (cmd[5] >> 4) & 0x01;
    assert_eq!(s1, 0, "k1 positive → s1=0");
    assert_eq!(s2, 1, "k2 negative → s2=1");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Damper and friction coefficient extremes
// ═══════════════════════════════════════════════════════════════════════════════

/// Damper with maximum positive coefficients.
#[test]
fn damper_max_positive_coefficients() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = encode_damper(op::START, i16::MAX, i16::MAX, 0xFFFF);
    assert_eq!(cmd[0], (slot::DAMPER << 4) | op::START);
    assert_eq!(cmd[1], effect_type::DAMPER);
    assert_eq!(cmd[3], 0, "k1 positive → s1=0");
    assert_eq!(cmd[5], 0, "k2 positive → s2=0");
    assert_eq!(cmd[6], 0xFF, "full clip");
    Ok(())
}

/// Damper with maximum negative coefficients.
#[test]
fn damper_max_negative_coefficients() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = encode_damper(op::START, i16::MIN, i16::MIN, 0xFFFF);
    assert_eq!(cmd[3], 1, "k1 negative → s1=1");
    assert_eq!(cmd[5], 1, "k2 negative → s2=1");
    Ok(())
}

/// Damper with zero coefficients and zero clip produces benign output.
#[test]
fn damper_zero_coefficients() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = encode_damper(op::UPDATE, 0, 0, 0);
    assert_eq!(cmd[2], 0, "k1 scaled = 0");
    assert_eq!(cmd[3], 0, "s1 = 0");
    assert_eq!(cmd[4], 0, "k2 scaled = 0");
    assert_eq!(cmd[5], 0, "s2 = 0");
    assert_eq!(cmd[6], 0, "clip = 0");
    Ok(())
}

/// Friction with maximum coefficients saturates to expected range.
#[test]
fn friction_max_coefficients() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = encode_friction(op::START, i16::MAX, i16::MAX, 0xFFFF);
    assert_eq!(cmd[0], (slot::FRICTION << 4) | op::START);
    assert_eq!(cmd[1], effect_type::FRICTION);
    // k1 and k2 are 8-bit scaled, so max should be 0xFF
    assert_eq!(cmd[2], 0xFF, "k1 max scaled to 0xFF");
    assert_eq!(cmd[3], 0xFF, "k2 max scaled to 0xFF");
    assert_eq!(cmd[4], 0xFF, "full clip");
    let s1 = cmd[5] & 0x01;
    let s2 = (cmd[5] >> 4) & 0x01;
    assert_eq!(s1, 0, "k1 positive");
    assert_eq!(s2, 0, "k2 positive");
    assert_eq!(cmd[6], 0x00, "friction trailing byte always 0");
    Ok(())
}

/// Friction with zero coefficients.
#[test]
fn friction_zero_coefficients() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = encode_friction(op::UPDATE, 0, 0, 0);
    assert_eq!(cmd[2], 0, "k1 zero");
    assert_eq!(cmd[3], 0, "k2 zero");
    assert_eq!(cmd[4], 0, "clip zero");
    assert_eq!(cmd[5], 0, "signs zero");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Model capability-gated operations
// ═══════════════════════════════════════════════════════════════════════════════

/// Only specific models support hardware friction.
#[test]
fn hardware_friction_model_gating() -> Result<(), Box<dyn std::error::Error>> {
    let friction_models = [
        (product_ids::DRIVING_FORCE_PRO, true),
        (product_ids::G25, true),
        (product_ids::DRIVING_FORCE_GT, true),
        (product_ids::G27, true),
        (product_ids::G29_PS, false),
        (product_ids::G920, false),
        (product_ids::G923, false),
        (product_ids::G_PRO, false),
    ];

    for (pid, expected) in friction_models {
        let model = LogitechModel::from_product_id(pid);
        assert_eq!(
            model.supports_hardware_friction(),
            expected,
            "PID 0x{:04X} ({:?}) friction support mismatch",
            pid,
            model
        );
    }
    Ok(())
}

/// Only G923 supports TrueForce.
#[test]
fn trueforce_exclusive_to_g923() -> Result<(), Box<dyn std::error::Error>> {
    let all_pids = [
        product_ids::MOMO,
        product_ids::DRIVING_FORCE_PRO,
        product_ids::DRIVING_FORCE_GT,
        product_ids::G25,
        product_ids::G27,
        product_ids::G29_PS,
        product_ids::G920,
        product_ids::G923,
        product_ids::G923_PS,
        product_ids::G923_XBOX,
        product_ids::G923_XBOX_ALT,
        product_ids::G_PRO,
        product_ids::G_PRO_XBOX,
    ];

    for pid in all_pids {
        let model = LogitechModel::from_product_id(pid);
        let expected = matches!(model, LogitechModel::G923);
        assert_eq!(
            model.supports_trueforce(),
            expected,
            "PID 0x{:04X} ({:?}) TrueForce mismatch",
            pid,
            model
        );
    }
    Ok(())
}

/// Range command support matrix.
#[test]
fn range_command_support_matrix() -> Result<(), Box<dyn std::error::Error>> {
    let with_range = [
        product_ids::DRIVING_FORCE_PRO,
        product_ids::G25,
        product_ids::DRIVING_FORCE_GT,
        product_ids::G27,
        product_ids::G29_PS,
        product_ids::G920,
        product_ids::G923,
        product_ids::G_PRO,
    ];
    let without_range = [
        product_ids::MOMO,
        product_ids::MOMO_2,
        product_ids::WINGMAN_FORMULA_FORCE,
        product_ids::WINGMAN_FORMULA_FORCE_GP,
        product_ids::VIBRATION_WHEEL,
        product_ids::SPEED_FORCE_WIRELESS,
        product_ids::DRIVING_FORCE_EX,
    ];

    for pid in with_range {
        let model = LogitechModel::from_product_id(pid);
        assert!(
            model.supports_range_command(),
            "PID 0x{:04X} ({:?}) should support range",
            pid,
            model
        );
    }
    for pid in without_range {
        let model = LogitechModel::from_product_id(pid);
        assert!(
            !model.supports_range_command(),
            "PID 0x{:04X} ({:?}) should NOT support range",
            pid,
            model
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Pedal 3-axis parsing — realistic per-variant values
// ═══════════════════════════════════════════════════════════════════════════════

/// Full throttle + full brake + full clutch (heel-toe extreme).
#[test]
fn three_pedal_all_maxed() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_input_report(0x8000, 0xFF, 0xFF, 0xFF, 0, 0x08, 0);
    let state = parse_input_report(&report).ok_or("max pedals parse failed")?;

    assert!((state.throttle - 1.0).abs() < 0.01);
    assert!((state.brake - 1.0).abs() < 0.01);
    assert!((state.clutch - 1.0).abs() < 0.01);
    Ok(())
}

/// All pedals released.
#[test]
fn three_pedal_all_released() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_input_report(0x8000, 0x00, 0x00, 0x00, 0, 0x08, 0);
    let state = parse_input_report(&report).ok_or("released pedals parse failed")?;

    assert!(state.throttle < 0.01);
    assert!(state.brake < 0.01);
    assert!(state.clutch < 0.01);
    Ok(())
}

/// Pedal mid-range values produce correctly normalized output.
#[test]
fn three_pedal_mid_range() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_input_report(0x8000, 0x80, 0x80, 0x80, 0, 0x08, 0);
    let state = parse_input_report(&report).ok_or("mid pedals parse failed")?;

    // 0x80 / 255.0 ≈ 0.502
    let expected = 0x80 as f32 / 255.0;
    assert!(
        (state.throttle - expected).abs() < 0.01,
        "throttle mid: got {}",
        state.throttle
    );
    assert!(
        (state.brake - expected).abs() < 0.01,
        "brake mid: got {}",
        state.brake
    );
    assert!(
        (state.clutch - expected).abs() < 0.01,
        "clutch mid: got {}",
        state.clutch
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. H-pattern shifter report parsing
// ═══════════════════════════════════════════════════════════════════════════════

/// Each gear position on the H-pattern shifter sets exactly one button bit.
/// The 16-bit button field supports bits 0–15; we use bits 8–13 for gears 1–6.
#[test]
fn hpattern_each_gear_sets_one_button() -> Result<(), Box<dyn std::error::Error>> {
    // Gears 1–6 mapped to bits 8–13 within the 16-bit button field
    for gear in 0u16..6 {
        let bit = 8 + gear;
        let buttons: u16 = 1 << bit;
        let report = build_input_report(0x8000, 0, 0, 0, buttons, 0x08, 0);
        let state = parse_input_report(&report).ok_or("shifter parse failed")?;

        // Only the expected bit should be set among gear bits 8–13
        for check_bit in 8u16..14 {
            let expected = if check_bit == bit { 1 } else { 0 };
            assert_eq!(
                (state.buttons >> check_bit) & 1,
                expected,
                "gear {} (bit {}): bit {} should be {}",
                gear + 1,
                bit,
                check_bit,
                expected
            );
        }
    }
    Ok(())
}

/// Multiple gear bits can appear simultaneously in the button field.
#[test]
fn hpattern_multiple_gear_bits_possible() -> Result<(), Box<dyn std::error::Error>> {
    // Hardware can report multiple bits; software must handle it
    let buttons: u16 = (1 << 8) | (1 << 11); // gear 1 + gear 4 simultaneously
    let report = build_input_report(0x8000, 0, 0, 0, buttons, 0x08, 0);
    let state = parse_input_report(&report).ok_or("multi-gear parse failed")?;

    assert_eq!((state.buttons >> 8) & 1, 1, "gear 1 set");
    assert_eq!((state.buttons >> 11) & 1, 1, "gear 4 set");
    assert_eq!((state.buttons >> 9) & 1, 0, "gear 2 clear");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Combined command sequences
// ═══════════════════════════════════════════════════════════════════════════════

/// Full initialization sequence: native mode → range → gain → constant force.
#[test]
fn full_init_command_sequence() -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: Native mode (revert)
    let native = build_native_mode_report();
    assert_eq!(native[0], report_ids::VENDOR);
    assert_eq!(native[1], 0x0A);

    // Step 2: Mode switch to G29
    let mode = build_mode_switch_report(0x05, true);
    assert_eq!(mode[0], report_ids::VENDOR);
    assert_eq!(mode[1], 0x09);
    assert_eq!(mode[2], 0x05, "G29 mode");
    assert_eq!(mode[4], 0x01, "detach flag");

    // Step 3: Set range to 900°
    let range = build_set_range_report(900);
    assert_eq!(range[1], 0x81);
    let range_val = u16::from_le_bytes([range[2], range[3]]);
    assert_eq!(range_val, 900);

    // Step 4: Set gain to 100%
    let gain = build_gain_report(0xFF);
    assert_eq!(gain[0], report_ids::DEVICE_GAIN);
    assert_eq!(gain[1], 0xFF);

    // Step 5: Start constant force at zero (neutral)
    let cf = encode_constant(op::START, 0);
    assert_eq!(cf[2], 0x80, "neutral force");

    Ok(())
}

/// Autocenter activation sequence matches kernel protocol.
#[test]
fn autocenter_activation_sequence() -> Result<(), Box<dyn std::error::Error>> {
    // Full autocenter: spring params + activate
    let cmds = encode_autocenter(0x80, 0xC0);

    // First report: spring parameters
    assert_eq!(cmds[0][0], 0xFE, "spring param prefix");
    assert_eq!(cmds[0][1], 0x0D, "spring param command");
    assert_eq!(cmds[0][2], 0x80, "spring k");
    assert_eq!(cmds[0][3], 0x80, "spring k (duplicated)");
    assert_eq!(cmds[0][4], 0xC0, "strength");

    // Second report: activate
    assert_eq!(cmds[1][0], 0x14, "activate command");
    assert_eq!(&cmds[1][1..], &[0u8; 6], "activate payload zeroed");

    // Deactivate
    let off = encode_autocenter_off();
    assert_eq!(off[0], 0xF5, "deactivate prefix");
    assert_eq!(&off[1..], &[0u8; 6], "deactivate payload zeroed");
    Ok(())
}

/// G923 PS mode switch requires report ID 0x30.
#[test]
fn g923_ps_mode_switch_sequence() -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: revert mode
    let revert = encode_mode_switch(TargetMode::G923);
    assert_eq!(revert[0], [0xF8, 0x0A, 0, 0, 0, 0, 0]);

    // Step 2: The PS-specific switch payload
    let ps_cmd = encode_g923_ps_mode_switch();
    assert_eq!(ps_cmd, [0xF8, 0x09, 0x07, 0x01, 0x01, 0x00, 0x00]);

    // Caller must use report ID 0x30
    assert_eq!(G923_PS_REPORT_ID, 0x30);

    // This differs from the normal EXT_CMD9 path only in report ID
    assert_eq!(revert[1], ps_cmd, "payload matches EXT_CMD9 G923");
    Ok(())
}

/// DFP native mode is a single-step command (no two-step EXT_CMD9).
#[test]
fn dfp_native_mode_single_step() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = encode_dfp_native_mode();
    assert_eq!(cmd[0], 0xF8);
    assert_eq!(cmd[1], 0x01, "DFP EXT_CMD1");
    assert_eq!(&cmd[2..], &[0u8; 5]);
    Ok(())
}

/// G25 native mode is a single-step command.
#[test]
fn g25_native_mode_single_step() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = encode_g25_native_mode();
    assert_eq!(cmd[0], 0xF8);
    assert_eq!(cmd[1], 0x10, "G25 EXT_CMD16");
    assert_eq!(&cmd[2..], &[0u8; 5]);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. LED indicator control
// ═══════════════════════════════════════════════════════════════════════════════

/// Each individual LED can be controlled.
#[test]
fn led_individual_control() -> Result<(), Box<dyn std::error::Error>> {
    for led in 0..LED_COUNT {
        let mask = 1u8 << led;
        let cmd = encode_leds(mask);
        assert_eq!(cmd[0], 0xF8);
        assert_eq!(cmd[1], 0x12);
        assert_eq!(cmd[2], mask, "LED {} mask", led);
    }
    Ok(())
}

/// Progressive LED pattern (RPM indicator style).
#[test]
fn led_progressive_pattern() -> Result<(), Box<dyn std::error::Error>> {
    // 1 LED lit, 2 LEDs, 3 LEDs, 4 LEDs, all 5 LEDs
    let patterns: [u8; 5] = [0x01, 0x03, 0x07, 0x0F, 0x1F];
    for (i, &pattern) in patterns.iter().enumerate() {
        let cmd = encode_leds(pattern);
        assert_eq!(cmd[2], pattern, "progressive pattern {} LEDs", i + 1);
    }
    Ok(())
}

/// LED command from output module also masks to 5 bits.
#[test]
fn led_output_module_masking() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = build_set_leds_report(0xFF);
    assert_eq!(cmd[2], 0x1F, "high bits masked off");
    assert_eq!(cmd[0], report_ids::VENDOR);
    assert_eq!(cmd[1], 0x12);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. VID/PID validation
// ═══════════════════════════════════════════════════════════════════════════════

/// Logitech VID is always 0x046D.
#[test]
fn vendor_id_constant() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(LOGITECH_VENDOR_ID, 0x046D);
    Ok(())
}

/// All G923 variants (4 PIDs) map to the same model.
#[test]
fn g923_all_variants_same_model() -> Result<(), Box<dyn std::error::Error>> {
    let g923_pids = [
        product_ids::G923,
        product_ids::G923_PS,
        product_ids::G923_XBOX,
        product_ids::G923_XBOX_ALT,
    ];
    for pid in g923_pids {
        let model = LogitechModel::from_product_id(pid);
        assert_eq!(
            model,
            LogitechModel::G923,
            "PID 0x{:04X} should map to G923",
            pid
        );
        assert!(is_wheel_product(pid), "PID 0x{:04X} should be a wheel", pid);
    }
    Ok(())
}

/// G PRO PS and Xbox variants map to the same model.
#[test]
fn gpro_both_variants_same_model() -> Result<(), Box<dyn std::error::Error>> {
    let ps = LogitechModel::from_product_id(product_ids::G_PRO);
    let xbox = LogitechModel::from_product_id(product_ids::G_PRO_XBOX);
    assert_eq!(ps, LogitechModel::GPro);
    assert_eq!(xbox, LogitechModel::GPro);
    assert_eq!(ps, xbox);
    Ok(())
}

/// Unknown PIDs should not be classified as wheels and should map to Unknown.
#[test]
fn unknown_pids_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let unknown_pids: [u16; 5] = [0x0000, 0xFFFF, 0xDEAD, 0xBEEF, 0x1234];
    for pid in unknown_pids {
        assert!(
            !is_wheel_product(pid),
            "PID 0x{:04X} should not be a wheel",
            pid
        );
        assert_eq!(
            LogitechModel::from_product_id(pid),
            LogitechModel::Unknown,
            "PID 0x{:04X} should map to Unknown",
            pid
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 11. Constant-force encoder integration
// ═══════════════════════════════════════════════════════════════════════════════

/// Encoder for each wheel model produces valid reports within torque bounds.
#[test]
fn constant_force_encoder_per_model() -> Result<(), Box<dyn std::error::Error>> {
    let models = [
        (product_ids::G25, 2.5f32),
        (product_ids::G27, 2.5),
        (product_ids::G29_PS, 2.2),
        (product_ids::G920, 2.2),
        (product_ids::G923, 2.2),
        (product_ids::G_PRO, 11.0),
    ];

    for (pid, expected_torque) in models {
        let model = LogitechModel::from_product_id(pid);
        let max_nm = model.max_torque_nm();
        assert!(
            (max_nm - expected_torque).abs() < 0.05,
            "{:?} torque mismatch",
            model
        );

        let encoder = LogitechConstantForceEncoder::new(max_nm);
        let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];

        // Encode positive max torque
        let len = encoder.encode(max_nm, &mut buf);
        assert_eq!(len, CONSTANT_FORCE_REPORT_LEN);
        assert_eq!(buf[0], report_ids::CONSTANT_FORCE);
        let mag = i16::from_le_bytes([buf[2], buf[3]]);
        assert!(
            mag > 9000,
            "{:?} max torque magnitude {} too low",
            model,
            mag
        );

        // Encode zero torque
        let len = encoder.encode(0.0, &mut buf);
        assert_eq!(len, CONSTANT_FORCE_REPORT_LEN);
        let mag = i16::from_le_bytes([buf[2], buf[3]]);
        assert_eq!(mag, 0, "{:?} zero torque should produce magnitude 0", model);

        // Encode zero via dedicated method
        let len = encoder.encode_zero(&mut buf);
        assert_eq!(len, CONSTANT_FORCE_REPORT_LEN);
        assert_eq!(buf[0], report_ids::CONSTANT_FORCE);
        assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), 0);
    }
    Ok(())
}

/// Encoder saturates at ±1.0 normalized regardless of over-torque input.
#[test]
fn constant_force_encoder_saturation() -> Result<(), Box<dyn std::error::Error>> {
    let encoder = LogitechConstantForceEncoder::new(2.2);
    let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];

    // Request 10x max torque
    encoder.encode(22.0, &mut buf);
    let mag_over = i16::from_le_bytes([buf[2], buf[3]]);

    // Request exactly max torque
    encoder.encode(2.2, &mut buf);
    let mag_max = i16::from_le_bytes([buf[2], buf[3]]);

    assert_eq!(
        mag_over, mag_max,
        "over-torque should clamp to same value as max torque"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 12. DFP range command dual-report structure
// ═══════════════════════════════════════════════════════════════════════════════

/// DFP range below 200° uses coarse command 0x02.
#[test]
fn dfp_range_below_200() -> Result<(), Box<dyn std::error::Error>> {
    let reports = build_set_range_dfp_reports(100);
    assert_eq!(reports[0][1], 0x02, "coarse cmd for ≤200°");
    assert_eq!(reports[1][0], 0x81, "fine cmd prefix");
    assert_eq!(reports[1][1], 0x0B, "fine cmd byte 1");
    Ok(())
}

/// DFP range above 200° uses coarse command 0x03.
#[test]
fn dfp_range_above_200() -> Result<(), Box<dyn std::error::Error>> {
    let reports = build_set_range_dfp_reports(540);
    assert_eq!(reports[0][1], 0x03, "coarse cmd for >200°");
    Ok(())
}

/// DFP range at boundary values (200, 900) produces zeroed fine command.
#[test]
fn dfp_range_boundary_zeroed_fine() -> Result<(), Box<dyn std::error::Error>> {
    for deg in [200, 900] {
        let reports = build_set_range_dfp_reports(deg);
        assert_eq!(
            reports[1],
            [0x81, 0x0B, 0, 0, 0, 0, 0],
            "DFP range {} should have zeroed fine command",
            deg
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 13. translate_force kernel compatibility
// ═══════════════════════════════════════════════════════════════════════════════

/// Verify translate_force matches the kernel macro for key values.
#[test]
fn translate_force_kernel_reference_values() -> Result<(), Box<dyn std::error::Error>> {
    // TRANSLATE_FORCE(x) = ((CLAMP_VALUE_S16(x) + 0x8000) >> 8)
    // x = 0       → (0 + 32768) >> 8 = 128 = 0x80
    assert_eq!(translate_force(0), 0x80);
    // x = -32767  → (-32767 + 32768) >> 8 = 1 >> 8 = 0
    assert_eq!(translate_force(-0x7FFF), 0x00);
    // x = 32767   → (32767 + 32768) >> 8 = 65535 >> 8 = 255
    assert_eq!(translate_force(0x7FFF), 0xFF);
    // x = -16384  → (-16384 + 32768) >> 8 = 16384 >> 8 = 64 = 0x40
    assert_eq!(translate_force(-0x4000), 0x40);
    // x = 16384   → (16384 + 32768) >> 8 = 49152 >> 8 = 192 = 0xC0
    assert_eq!(translate_force(0x4000), 0xC0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 14. Mode switch report size and structure invariants
// ═══════════════════════════════════════════════════════════════════════════════

/// All mode switch variants produce REPORT_SIZE-byte commands.
#[test]
fn mode_switch_all_targets_correct_size() -> Result<(), Box<dyn std::error::Error>> {
    let targets = [
        TargetMode::DfEx,
        TargetMode::Dfp,
        TargetMode::G25,
        TargetMode::Dfgt,
        TargetMode::G27,
        TargetMode::G29,
        TargetMode::G923,
    ];

    for target in targets {
        let cmds = encode_mode_switch(target);
        assert_eq!(cmds[0].len(), REPORT_SIZE, "{:?} revert size", target);
        assert_eq!(cmds[1].len(), REPORT_SIZE, "{:?} switch size", target);
        // First command is always the revert
        assert_eq!(cmds[0][0], 0xF8);
        assert_eq!(cmds[0][1], 0x0A);
        // Second command is always the switch
        assert_eq!(cmds[1][0], 0xF8);
        assert_eq!(cmds[1][1], 0x09);
        // Mode byte = target as u8
        assert_eq!(cmds[1][2], target as u8);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 15. Error handling — malformed input reports
// ═══════════════════════════════════════════════════════════════════════════════

/// Empty input is rejected.
#[test]
fn parse_empty_input() -> Result<(), Box<dyn std::error::Error>> {
    assert!(parse_input_report(&[]).is_none());
    Ok(())
}

/// Single-byte input is rejected.
#[test]
fn parse_single_byte_input() -> Result<(), Box<dyn std::error::Error>> {
    assert!(parse_input_report(&[0x01]).is_none());
    Ok(())
}

/// Report with wrong ID prefix is rejected even if length is valid.
#[test]
fn parse_wrong_id_valid_length() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 12];
    data[0] = 0xFF; // wrong report ID
    assert!(parse_input_report(&data).is_none());
    Ok(())
}

/// Report with correct ID but 9 bytes (one short of minimum) is rejected.
#[test]
fn parse_one_byte_short() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 9];
    data[0] = 0x01;
    assert!(parse_input_report(&data).is_none());
    Ok(())
}

/// Reports longer than 12 bytes still parse (only first 10 bytes used).
#[test]
fn parse_oversized_report() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[0] = 0x01;
    data[1] = 0x00;
    data[2] = 0x80; // center steering
    data[3] = 0xFF; // full throttle
    let state = parse_input_report(&data).ok_or("oversized report should parse")?;
    assert!(state.steering.abs() < 0.001);
    assert!((state.throttle - 1.0).abs() < 0.01);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 16. Max rotation consistency
// ═══════════════════════════════════════════════════════════════════════════════

/// Max rotation degrees are consistent across model classification.
#[test]
fn max_rotation_per_model() -> Result<(), Box<dyn std::error::Error>> {
    let cases: [(u16, u16); 8] = [
        (product_ids::WINGMAN_FORMULA_FORCE, 180),
        (product_ids::MOMO, 270),
        (product_ids::DRIVING_FORCE_EX, 270),
        (product_ids::G25, 900),
        (product_ids::G27, 900),
        (product_ids::G29_PS, 900),
        (product_ids::G923, 900),
        (product_ids::G_PRO, 1080),
    ];
    for (pid, expected_deg) in cases {
        let model = LogitechModel::from_product_id(pid);
        assert_eq!(
            model.max_rotation_deg(),
            expected_deg,
            "PID 0x{:04X} ({:?}) rotation mismatch",
            pid,
            model
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 17. Proptest — slot encoding robustness
// ═══════════════════════════════════════════════════════════════════════════════

mod slot_proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        /// Constant force encoding never panics and produces the right slot.
        #[test]
        fn prop_constant_always_slot_0(
            force in i16::MIN..=i16::MAX,
            op_val in prop_oneof![Just(op::START), Just(op::STOP), Just(op::UPDATE)],
        ) {
            let cmd = encode_constant(op_val, force);
            prop_assert_eq!(cmd[0] >> 4, slot::CONSTANT, "must be slot 0");
            prop_assert_eq!(cmd[0] & 0x0F, op_val);
            prop_assert_eq!(cmd.len(), SLOT_CMD_SIZE);
        }

        /// Spring encoding never panics for any i16 inputs.
        #[test]
        fn prop_spring_never_panics(
            d1 in i16::MIN..=i16::MAX,
            d2 in i16::MIN..=i16::MAX,
            k1 in i16::MIN..=i16::MAX,
            k2 in i16::MIN..=i16::MAX,
            clip in 0u16..=u16::MAX,
        ) {
            let cmd = encode_spring(op::START, d1, d2, k1, k2, clip);
            prop_assert_eq!(cmd[0] >> 4, slot::SPRING);
            prop_assert_eq!(cmd[1], effect_type::SPRING);
            prop_assert_eq!(cmd.len(), SLOT_CMD_SIZE);
        }

        /// Damper encoding produces correct slot and sign bits for all inputs.
        #[test]
        fn prop_damper_slot_and_signs(
            k1 in i16::MIN..=i16::MAX,
            k2 in i16::MIN..=i16::MAX,
            clip in 0u16..=u16::MAX,
        ) {
            let cmd = encode_damper(op::START, k1, k2, clip);
            prop_assert_eq!(cmd[0] >> 4, slot::DAMPER);
            prop_assert_eq!(cmd[1], effect_type::DAMPER);
            let s1 = cmd[3];
            let s2 = cmd[5];
            prop_assert_eq!(s1, if k1 < 0 { 1 } else { 0 }, "damper k1 sign");
            prop_assert_eq!(s2, if k2 < 0 { 1 } else { 0 }, "damper k2 sign");
        }

        /// Friction encoding produces correct slot and trailing zero.
        #[test]
        fn prop_friction_slot_and_trailing(
            k1 in i16::MIN..=i16::MAX,
            k2 in i16::MIN..=i16::MAX,
            clip in 0u16..=u16::MAX,
        ) {
            let cmd = encode_friction(op::START, k1, k2, clip);
            prop_assert_eq!(cmd[0] >> 4, slot::FRICTION);
            prop_assert_eq!(cmd[1], effect_type::FRICTION);
            prop_assert_eq!(cmd[6], 0x00, "friction trailing byte");
        }

        /// Input report parsing never panics for arbitrary data.
        #[test]
        fn prop_parse_input_no_panic(data in proptest::collection::vec(any::<u8>(), 0..=64)) {
            let _ = parse_input_report(&data);
        }

        /// When parse succeeds, all normalized values are in valid ranges.
        #[test]
        fn prop_parse_valid_ranges(
            steering in any::<u16>(),
            throttle in any::<u8>(),
            brake in any::<u8>(),
            clutch in any::<u8>(),
            buttons in any::<u16>(),
            hat in 0u8..=0x0F,
            paddles in 0u8..=0x03,
        ) {
            let report = build_input_report(steering, throttle, brake, clutch, buttons, hat, paddles);
            if let Some(state) = parse_input_report(&report) {
                prop_assert!(state.steering >= -1.0 && state.steering <= 1.0);
                prop_assert!(state.throttle >= 0.0 && state.throttle <= 1.0);
                prop_assert!(state.brake >= 0.0 && state.brake <= 1.0);
                prop_assert!(state.clutch >= 0.0 && state.clutch <= 1.0);
                prop_assert!(state.hat <= 0x0F);
                prop_assert!(state.paddles <= 0x03);
            }
        }

        /// Model classification is deterministic for any PID.
        #[test]
        fn prop_model_classification_deterministic(pid in any::<u16>()) {
            let m1 = LogitechModel::from_product_id(pid);
            let m2 = LogitechModel::from_product_id(pid);
            prop_assert_eq!(m1, m2, "model classification must be deterministic");
        }

        /// Encode range clamping: any u16 input produces valid wire data.
        #[test]
        fn prop_encode_range_always_valid(deg in any::<u16>()) {
            let cmd = encode_range(deg);
            prop_assert_eq!(cmd[0], 0xF8);
            prop_assert_eq!(cmd[1], 0x81);
            let val = u16::from_le_bytes([cmd[2], cmd[3]]);
            prop_assert!((MIN_RANGE..=MAX_RANGE).contains(&val), "range {} out of [{}, {}]", val, MIN_RANGE, MAX_RANGE);
        }
    }
}
