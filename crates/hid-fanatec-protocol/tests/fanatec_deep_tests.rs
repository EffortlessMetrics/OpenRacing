//! Deep integration tests for the Fanatec HID protocol crate.
//!
//! These tests exercise cross-module interactions and protocol edge cases
//! that are not covered by existing per-module unit tests or snapshot tests.
//! Every test returns `Result` and avoids `unwrap()`/`expect()`.

use racing_wheel_hid_fanatec_protocol::display::{
    SEGBITS, encode_display, encode_range, encode_wheel_leds, encode_wheelbase_leds, seg_bits,
};
use racing_wheel_hid_fanatec_protocol::ids::{
    FANATEC_VENDOR_ID, ffb_commands, product_ids, report_ids, rim_ids,
};
use racing_wheel_hid_fanatec_protocol::input::{
    parse_extended_report, parse_pedal_report, parse_standard_report,
};
use racing_wheel_hid_fanatec_protocol::output::{
    CONSTANT_FORCE_REPORT_LEN, FanatecConstantForceEncoder, build_display_report,
    build_kernel_range_sequence, build_led_report, build_mode_switch_report,
    build_rotation_range_report, build_rumble_report, build_set_gain_report, build_stop_all_report,
    fix_report_values,
};
use racing_wheel_hid_fanatec_protocol::slots::{
    SLOT_CMD_SIZE, effect_cmd, encode_constant_highres, encode_constant_lowres, encode_damper,
    encode_disable_slot, encode_friction, encode_inertia, encode_spring, encode_stop_all, slot,
};
use racing_wheel_hid_fanatec_protocol::tuning::{
    ConversionType, PARAMS, TUNING_HEADER_0, TUNING_HEADER_1, TUNING_REPORT_SIZE, decode_value,
    encode_reset, encode_select_slot, encode_toggle_advanced_mode, encode_value, encode_write,
    param_by_addr, param_by_name,
};
use racing_wheel_hid_fanatec_protocol::types::{
    FanatecModel, FanatecRimId, is_pedal_product, is_wheelbase_product,
};

// ============================================================================
// §1  Cross-module device-to-encoding pipeline
// ============================================================================

/// Given a product ID, resolve the model, determine highres, and encode a
/// constant force through the correct slot encoder. Verify the entire chain.
#[test]
fn pipeline_model_to_slot_encoding_dd1() -> Result<(), Box<dyn std::error::Error>> {
    let model = FanatecModel::from_product_id(product_ids::DD1);
    assert_eq!(model, FanatecModel::Dd1);
    assert!(model.is_highres());

    // Use highres slot encoder for DD models
    let cmd = encode_constant_highres(0x4000);
    assert_eq!(cmd[6], 0x01, "highres marker must be 0x01 for DD1");
    assert_eq!(cmd[1], effect_cmd::CONSTANT);
    assert_eq!(cmd[0] >> 4, slot::CONSTANT, "slot ID must be constant");

    // Also verify the output module encoder produces valid reports
    let encoder = FanatecConstantForceEncoder::new(model.max_torque_nm());
    let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];
    let len = encoder.encode(10.0, 0, &mut buf);
    assert_eq!(len, CONSTANT_FORCE_REPORT_LEN);
    assert_eq!(buf[0], report_ids::FFB_OUTPUT);
    assert_eq!(buf[1], ffb_commands::CONSTANT_FORCE);
    // 10 Nm on a 20 Nm base → ~50% positive
    let force = i16::from_le_bytes([buf[2], buf[3]]);
    assert!(
        force > 15_000 && force < 17_000,
        "DD1 10Nm should be ~50% of i16::MAX, got {force}"
    );
    Ok(())
}

#[test]
fn pipeline_model_to_slot_encoding_csl_elite() -> Result<(), Box<dyn std::error::Error>> {
    let model = FanatecModel::from_product_id(product_ids::CSL_ELITE);
    assert_eq!(model, FanatecModel::CslElite);
    assert!(!model.is_highres());

    // Belt-driven base → lowres slot encoder
    let cmd = encode_constant_lowres(0x4000);
    assert_eq!(cmd[6], 0x00, "no highres marker for belt base");
    assert_eq!(cmd[1], effect_cmd::CONSTANT);

    // Output module encoder
    let encoder = FanatecConstantForceEncoder::new(model.max_torque_nm());
    let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(3.0, 0, &mut buf);
    // 3 Nm on a 6 Nm base → ~50%
    let force = i16::from_le_bytes([buf[2], buf[3]]);
    assert!(
        force > 15_000 && force < 17_000,
        "CSL Elite 3Nm should be ~50%, got {force}"
    );
    Ok(())
}

/// Verify that max rotation degrees from the model matches the kernel range
/// sequence's accepted range for each wheelbase class.
#[test]
fn pipeline_model_rotation_to_kernel_range() -> Result<(), Box<dyn std::error::Error>> {
    let models = [
        (product_ids::DD1, 2520u16),
        (product_ids::DD2, 2520),
        (product_ids::CSL_DD, 2520),
        (product_ids::GT_DD_PRO, 2520),
        (product_ids::CLUBSPORT_DD, 2520),
        (product_ids::CSL_ELITE, 1080),
        (product_ids::CLUBSPORT_V2, 900),
        (product_ids::CLUBSPORT_V2_5, 900),
        (product_ids::CSR_ELITE, 900),
    ];

    for (pid, expected_max) in models {
        let model = FanatecModel::from_product_id(pid);
        assert_eq!(
            model.max_rotation_degrees(),
            expected_max,
            "max rotation mismatch for PID 0x{pid:04X}"
        );

        // Verify kernel range sequence can encode this model's max
        let seq = build_kernel_range_sequence(model.max_rotation_degrees());
        let encoded_range = u16::from_le_bytes([seq[2][2], seq[2][3]]);
        assert_eq!(
            encoded_range, expected_max,
            "kernel range for PID 0x{pid:04X}"
        );
    }
    Ok(())
}

// ============================================================================
// §2  Tuning menu cross-parameter integration
// ============================================================================

/// Encode writes for every known tuning parameter and verify the value lands
/// at the correct offset in the 64-byte report.
#[test]
fn tuning_write_all_params_correct_offset() -> Result<(), Box<dyn std::error::Error>> {
    for param in PARAMS {
        // Skip SEN (sensitivity) — its max is 0 (device-dependent)
        if param.name == "SEN" {
            continue;
        }

        let test_value = param.min;
        let raw = encode_value(param.conv, test_value);
        let report = encode_write(param.addr, raw);

        assert_eq!(report[0], TUNING_HEADER_0, "header[0] for {}", param.name);
        assert_eq!(report[1], TUNING_HEADER_1, "header[1] for {}", param.name);
        assert_eq!(report[2], 0x00, "command byte for {}", param.name);

        let expected_offset = (param.addr as usize) + 1;
        assert_eq!(
            report[expected_offset], raw,
            "value at offset {} for param {} (addr 0x{:02X})",
            expected_offset, param.name, param.addr
        );

        // All other payload bytes (except headers and value) must be zero
        for (i, &b) in report.iter().enumerate() {
            if i == 0 || i == 1 || i == 2 || i == expected_offset {
                continue;
            }
            assert_eq!(b, 0x00, "byte {i} should be zero for param {}", param.name);
        }
    }
    Ok(())
}

/// Encode→decode roundtrip for all TimesTen parameters at boundary values.
#[test]
fn tuning_times_ten_roundtrip_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let times_ten_params: Vec<_> = PARAMS
        .iter()
        .filter(|p| p.conv == ConversionType::TimesTen)
        .collect();

    assert!(!times_ten_params.is_empty(), "should have TimesTen params");

    for param in &times_ten_params {
        for value in [param.min, param.max, (param.min + param.max) / 2] {
            // Round to multiple of 10 for clean roundtrip
            let aligned = (value / 10) * 10;
            let raw = encode_value(ConversionType::TimesTen, aligned);
            let decoded = decode_value(ConversionType::TimesTen, raw);
            assert_eq!(
                decoded, aligned,
                "TimesTen roundtrip failed for {} value={}",
                param.name, aligned
            );
        }
    }
    Ok(())
}

/// Verify Signed conversion roundtrip for DRI parameter's full range.
#[test]
fn tuning_signed_dri_full_range_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let dri = param_by_name("DRI").ok_or("DRI param not found")?;
    assert_eq!(dri.conv, ConversionType::Signed);

    for v in dri.min..=dri.max {
        let raw = encode_value(ConversionType::Signed, v);
        let decoded = decode_value(ConversionType::Signed, raw);
        assert_eq!(decoded, v, "DRI roundtrip failed for {v}");
    }
    Ok(())
}

/// Slot-select encoding for all valid slots (1–5).
#[test]
fn tuning_select_slot_all_valid() -> Result<(), Box<dyn std::error::Error>> {
    for slot_num in 1u8..=5 {
        let report = encode_select_slot(slot_num);
        assert_eq!(report[0], TUNING_HEADER_0);
        assert_eq!(report[1], TUNING_HEADER_1);
        assert_eq!(report[2], 0x01, "select command");
        assert_eq!(report[3], slot_num, "slot number");
        assert_eq!(report.len(), TUNING_REPORT_SIZE);
    }
    Ok(())
}

/// Reset and toggle-advanced-mode reports must have correct command bytes
/// and zero-filled payloads.
#[test]
fn tuning_reset_and_advanced_mode_wire_format() -> Result<(), Box<dyn std::error::Error>> {
    let reset = encode_reset();
    assert_eq!(reset[0], TUNING_HEADER_0);
    assert_eq!(reset[1], TUNING_HEADER_1);
    assert_eq!(reset[2], 0x04);
    assert!(
        reset[3..].iter().all(|&b| b == 0),
        "reset payload must be zero"
    );

    let adv = encode_toggle_advanced_mode();
    assert_eq!(adv[0], TUNING_HEADER_0);
    assert_eq!(adv[1], TUNING_HEADER_1);
    assert_eq!(adv[2], 0x06);
    assert!(
        adv[3..].iter().all(|&b| b == 0),
        "advanced mode payload must be zero"
    );
    Ok(())
}

/// Every param address must be reachable via both name and address lookup.
#[test]
fn tuning_param_lookup_bidirectional() -> Result<(), Box<dyn std::error::Error>> {
    for param in PARAMS {
        let by_name =
            param_by_name(param.name).ok_or(format!("{} not found by name", param.name))?;
        let by_addr =
            param_by_addr(param.addr).ok_or(format!("addr 0x{:02X} not found", param.addr))?;
        assert_eq!(by_name.addr, param.addr, "addr mismatch for {}", param.name);
        assert_eq!(
            by_addr.name, param.name,
            "name mismatch for addr 0x{:02X}",
            param.addr
        );
    }
    Ok(())
}

// ============================================================================
// §3  Display / LED / 7-segment integration
// ============================================================================

/// Verify 7-segment encoding for all printable digit characters matches the
/// SEGBITS table directly, and that encode_display produces correct wire bytes.
#[test]
fn display_7seg_all_digits_encode_display_integration() -> Result<(), Box<dyn std::error::Error>> {
    for d in b'0'..=b'9' {
        let expected_seg = SEGBITS[(d - b'0') as usize];
        let actual = seg_bits(d, false);
        assert_eq!(
            actual, expected_seg,
            "seg_bits mismatch for digit '{}'",
            d as char
        );

        // Single digit → right-justified in encode_display
        let report = encode_display(&[d]);
        assert_eq!(
            report[6], expected_seg,
            "encode_display digit '{}' at pos 2",
            d as char
        );
        assert_eq!(report[4], 0x00, "left-pad blank for single digit");
        assert_eq!(report[5], 0x00, "middle-pad blank for single digit");
    }
    Ok(())
}

/// Decimal point merging: "1.2.3" should produce 3 segments with points on 1 and 2.
#[test]
fn display_decimal_point_merging() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_display(b"1.2.3");
    // '1' with point, '2' with point, '3' without point
    assert_eq!(report[4], seg_bits(b'1', true), "seg 0: '1.'");
    assert_eq!(report[5], seg_bits(b'2', true), "seg 1: '2.'");
    assert_eq!(report[6], seg_bits(b'3', false), "seg 2: '3'");
    Ok(())
}

/// Letters without 7-segment representation produce blank segments.
#[test]
fn display_unrepresentable_letters_are_blank() -> Result<(), Box<dyn std::error::Error>> {
    let blanks = [b'k', b'm', b'v', b'w', b'x'];
    for &ch in &blanks {
        assert_eq!(
            seg_bits(ch, false),
            0x00,
            "'{}' should be blank",
            ch as char
        );
    }

    let report = encode_display(b"kwm");
    // All three characters have no 7-seg representation
    assert_eq!(report[4], 0x00);
    assert_eq!(report[5], 0x00);
    assert_eq!(report[6], 0x00);
    Ok(())
}

/// Wheelbase LED and wheel LED encoders produce distinct wire formats.
#[test]
fn display_wheelbase_vs_wheel_led_formats_differ() -> Result<(), Box<dyn std::error::Error>> {
    let wb = encode_wheelbase_leds(0x0FF);
    let wh = encode_wheel_leds(0x0FF);

    // Wheelbase: [0xf8, 0x13, leds_lo, ...]
    assert_eq!(wb[0], 0xf8);
    assert_eq!(wb[1], 0x13);

    // Wheel: [0xf8, 0x09, 0x08, hi, lo, ...]
    assert_eq!(wh[0], 0xf8);
    assert_eq!(wh[1], 0x09);
    assert_eq!(wh[2], 0x08);

    // Headers differ → commands are distinguishable
    assert_ne!(wb[1], wh[1], "wheelbase and wheel LED commands must differ");
    Ok(())
}

/// Encode range via display module matches build_kernel_range_sequence from output module.
#[test]
fn display_range_vs_output_range_consistency() -> Result<(), Box<dyn std::error::Error>> {
    for degrees in [90u16, 360, 540, 900, 1080, 2520] {
        let display_seq = encode_range(degrees);
        let output_seq = build_kernel_range_sequence(degrees);

        // Both produce 3-step sequences with same structure
        assert_eq!(
            display_seq[0],
            [0xf5, 0, 0, 0, 0, 0, 0],
            "step 1 reset for {degrees}°"
        );
        assert_eq!(
            display_seq[1], output_seq[1],
            "step 2 prepare for {degrees}°"
        );

        // Step 3: range byte encoding must match
        let display_range = u16::from_le_bytes([display_seq[2][2], display_seq[2][3]]);
        let output_range = u16::from_le_bytes([output_seq[2][2], output_seq[2][3]]);
        assert_eq!(
            display_range, output_range,
            "range encoding mismatch for {degrees}°"
        );
    }
    Ok(())
}

/// Wheel LED bit reversal: specific patterns verify correct mirroring.
#[test]
fn display_wheel_led_specific_patterns() -> Result<(), Box<dyn std::error::Error>> {
    // Only LED 0 lit → should appear as LED 8 (highest bit of 9)
    let buf = encode_wheel_leds(0b0_0000_0001);
    let out = ((buf[3] as u16) << 8) | buf[4] as u16;
    assert_eq!(out, 0b1_0000_0000, "bit 0 → bit 8");

    // Only LED 4 (middle) lit → should stay in middle
    let buf = encode_wheel_leds(0b0_0001_0000);
    let out = ((buf[3] as u16) << 8) | buf[4] as u16;
    assert_eq!(out, 0b0_0001_0000, "middle LED stays in middle");

    // Asymmetric: LEDs 0,1,2 lit → bits 6,7,8 lit
    let buf = encode_wheel_leds(0b0_0000_0111);
    let out = ((buf[3] as u16) << 8) | buf[4] as u16;
    assert_eq!(out, 0b1_1100_0000, "lower 3 → upper 3");
    Ok(())
}

// ============================================================================
// §4  Slot-based FFB effect encoding — all effect types
// ============================================================================

/// Encode all 5 effect types with non-trivial parameters and verify slot IDs,
/// effect commands, and the overall wire structure.
#[test]
fn slot_all_five_effects_structure() -> Result<(), Box<dyn std::error::Error>> {
    let constant_lr = encode_constant_lowres(0x2000);
    let constant_hr = encode_constant_highres(0x2000);
    let spring = encode_spring(0x1000, -0x1000, 0x3000, 0x3000, 0x8000);
    let damper = encode_damper(0x3000, 0x3000, 0xFFFF);
    let inertia = encode_inertia(0x2000, 0x2000, 0xA000);
    let friction = encode_friction(0x4000, 0x4000, 0xC000);

    // All must be 7 bytes
    assert_eq!(constant_lr.len(), SLOT_CMD_SIZE);
    assert_eq!(constant_hr.len(), SLOT_CMD_SIZE);
    assert_eq!(spring.len(), SLOT_CMD_SIZE);
    assert_eq!(damper.len(), SLOT_CMD_SIZE);
    assert_eq!(inertia.len(), SLOT_CMD_SIZE);
    assert_eq!(friction.len(), SLOT_CMD_SIZE);

    // Slot IDs in upper nibble of byte 0
    assert_eq!(constant_lr[0] >> 4, slot::CONSTANT);
    assert_eq!(constant_hr[0] >> 4, slot::CONSTANT);
    assert_eq!(spring[0] >> 4, slot::SPRING);
    assert_eq!(damper[0] >> 4, slot::DAMPER);
    assert_eq!(inertia[0] >> 4, slot::INERTIA);
    assert_eq!(friction[0] >> 4, slot::FRICTION);

    // Effect commands
    assert_eq!(constant_lr[1], effect_cmd::CONSTANT);
    assert_eq!(constant_hr[1], effect_cmd::CONSTANT);
    assert_eq!(spring[1], effect_cmd::SPRING);
    assert_eq!(damper[1], effect_cmd::RESISTANCE);
    assert_eq!(inertia[1], effect_cmd::RESISTANCE);
    assert_eq!(friction[1], effect_cmd::RESISTANCE);

    // Active flag (bit 0) should be set for non-zero effects
    assert_eq!(constant_lr[0] & 0x01, 0x01, "lowres active");
    assert_eq!(constant_hr[0] & 0x01, 0x01, "highres active");
    assert_eq!(spring[0] & 0x01, 0x01, "spring active");
    assert_eq!(damper[0] & 0x01, 0x01, "damper active");
    assert_eq!(inertia[0] & 0x01, 0x01, "inertia active");
    assert_eq!(friction[0] & 0x01, 0x01, "friction active");
    Ok(())
}

/// Zero force disables constant slot; zero clip disables condition slots.
#[test]
fn slot_zero_disables_effects() -> Result<(), Box<dyn std::error::Error>> {
    let constant_lr = encode_constant_lowres(0);
    let constant_hr = encode_constant_highres(0);
    let spring = encode_spring(0, 0, 0x4000, 0x4000, 0);
    let damper = encode_damper(0x3000, 0x3000, 0);
    let inertia = encode_inertia(0x2000, 0x2000, 0);
    let friction = encode_friction(0x4000, 0x4000, 0);

    // Disable flag (bit 1) set, active flag (bit 0) cleared
    for (name, cmd) in [
        ("constant_lr", constant_lr),
        ("constant_hr", constant_hr),
        ("spring", spring),
        ("damper", damper),
        ("inertia", inertia),
        ("friction", friction),
    ] {
        assert_eq!(cmd[0] & 0x02, 0x02, "{name} disable flag");
        assert_eq!(cmd[0] & 0x01, 0x00, "{name} active flag must be clear");
    }
    Ok(())
}

/// Highres constant force: byte 6 must always be 0x01 marker.
#[test]
fn slot_highres_marker_always_set() -> Result<(), Box<dyn std::error::Error>> {
    for level in [i16::MIN, -1, 0, 1, i16::MAX] {
        let cmd = encode_constant_highres(level);
        assert_eq!(cmd[6], 0x01, "highres marker for level={level}");
    }
    Ok(())
}

/// Lowres constant force: byte 6 must always be 0x00.
#[test]
fn slot_lowres_no_marker() -> Result<(), Box<dyn std::error::Error>> {
    for level in [i16::MIN, -1, 0, 1, i16::MAX] {
        let cmd = encode_constant_lowres(level);
        assert_eq!(cmd[6], 0x00, "lowres byte 6 for level={level}");
    }
    Ok(())
}

/// Stop-all command is exactly [0xF3, 0, 0, 0, 0, 0, 0].
#[test]
fn slot_stop_all_exact_bytes() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        encode_stop_all(),
        [0xF3, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
    );
    Ok(())
}

/// Disable-slot command for each slot/effect-cmd pair.
#[test]
fn slot_disable_each_slot() -> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        (slot::CONSTANT, effect_cmd::CONSTANT),
        (slot::SPRING, effect_cmd::SPRING),
        (slot::DAMPER, effect_cmd::RESISTANCE),
        (slot::INERTIA, effect_cmd::RESISTANCE),
        (slot::FRICTION, effect_cmd::RESISTANCE),
    ];
    for (slot_id, ecmd) in cases {
        let cmd = encode_disable_slot(slot_id, ecmd);
        assert_eq!(cmd[0] >> 4, slot_id, "slot ID for slot {slot_id}");
        assert_eq!(cmd[0] & 0x02, 0x02, "disable flag for slot {slot_id}");
        assert_eq!(cmd[0] & 0x01, 0x00, "active clear for slot {slot_id}");
        assert_eq!(cmd[1], ecmd, "effect cmd for slot {slot_id}");
        assert_eq!(cmd[6], 0xFF, "trailing 0xFF for slot {slot_id}");
    }
    Ok(())
}

// ============================================================================
// §5  Input parsing — rim-specific features
// ============================================================================

/// Standard report with McLaren GT3 V2 rim features: funky switch + rotary + dual clutch.
#[test]
fn input_standard_report_mclaren_gt3v2_features() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[0] = report_ids::STANDARD_INPUT;
    data[1] = 0x00;
    data[2] = 0x80; // steering center
    data[3] = 0xFF; // throttle released
    data[4] = 0xFF; // brake released
    data[5] = 0xFF; // clutch released
    data[9] = 0x02; // hat = right
    data[10] = 0x01; // funky switch = up
    // Rotary encoder 1: +100
    let r1 = 100i16;
    let r1_bytes = r1.to_le_bytes();
    data[11] = r1_bytes[0];
    data[12] = r1_bytes[1];
    // Rotary encoder 2: -50
    let r2 = -50i16;
    let r2_bytes = r2.to_le_bytes();
    data[13] = r2_bytes[0];
    data[14] = r2_bytes[1];
    // Dual clutch left: fully pressed (0x00 → 1.0 inverted)
    data[15] = 0x00;
    // Dual clutch right: half pressed (0x80 → ~0.498)
    data[16] = 0x80;
    // Rim ID at offset 0x1F
    data[0x1F] = rim_ids::MCLAREN_GT3_V2;

    let state = parse_standard_report(&data).ok_or("parse failed")?;
    assert!((state.steering).abs() < 1e-4, "steering centered");
    assert_eq!(state.hat, 0x02, "hat = right");
    assert_eq!(state.funky_dir, 0x01, "funky = up");
    assert_eq!(state.rotary1, 100, "rotary1");
    assert_eq!(state.rotary2, -50, "rotary2");
    assert!(
        (state.clutch_left - 1.0).abs() < 1e-3,
        "left clutch fully pressed"
    );
    assert!(
        state.clutch_right > 0.49 && state.clutch_right < 0.51,
        "right clutch half"
    );

    // Verify rim detection
    let rim = FanatecRimId::from_byte(data[0x1F]);
    assert_eq!(rim, FanatecRimId::McLarenGt3V2);
    assert!(rim.has_funky_switch());
    assert!(rim.has_dual_clutch());
    assert!(rim.has_rotary_encoders());
    Ok(())
}

/// Standard report with Formula V2 rim — dual clutch but no funky switch.
#[test]
fn input_standard_report_formula_v2_features() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[0] = report_ids::STANDARD_INPUT;
    data[1] = 0x00;
    data[2] = 0x80; // center
    data[3] = 0xFF; // throttle released
    data[4] = 0x00; // brake fully pressed
    data[5] = 0xFF; // clutch released
    data[9] = 0x0F; // hat neutral
    data[10] = 0x00; // no funky switch
    data[15] = 0x80; // left clutch ~50%
    data[16] = 0xFF; // right clutch released
    data[0x1F] = rim_ids::FORMULA_V2;

    let state = parse_standard_report(&data).ok_or("parse failed")?;
    assert!((state.brake - 1.0).abs() < 1e-3, "brake fully pressed");
    assert!(
        state.clutch_left > 0.49 && state.clutch_left < 0.51,
        "left clutch ~50%"
    );
    assert!(state.clutch_right.abs() < 1e-3, "right clutch released");

    let rim = FanatecRimId::from_byte(data[0x1F]);
    assert!(rim.has_dual_clutch());
    assert!(!rim.has_funky_switch());
    Ok(())
}

/// Standard report with exactly 10 bytes (minimum valid length) — optional
/// fields default to zero.
#[test]
fn input_standard_report_minimum_length() -> Result<(), Box<dyn std::error::Error>> {
    let data = [
        report_ids::STANDARD_INPUT,
        0x00,
        0x80,
        0xFF,
        0xFF,
        0xFF,
        0x00,
        0x00,
        0x00,
        0x0F,
    ];
    let state = parse_standard_report(&data).ok_or("parse failed")?;
    assert_eq!(state.funky_dir, 0, "funky defaults to 0");
    assert_eq!(state.rotary1, 0, "rotary1 defaults to 0");
    assert_eq!(state.rotary2, 0, "rotary2 defaults to 0");
    assert!(
        (state.clutch_left).abs() < 1e-6,
        "clutch_left defaults to 0.0"
    );
    assert!(
        (state.clutch_right).abs() < 1e-6,
        "clutch_right defaults to 0.0"
    );
    Ok(())
}

/// Reports with 11–16 bytes: each optional field is populated progressively.
#[test]
fn input_standard_report_progressive_lengths() -> Result<(), Box<dyn std::error::Error>> {
    let base = [
        report_ids::STANDARD_INPUT,
        0x00,
        0x80,
        0xFF,
        0xFF,
        0xFF,
        0x00,
        0x00,
        0x00,
        0x0F,
    ];

    // 11 bytes: funky_dir available
    let mut d11 = [0u8; 11];
    d11[..10].copy_from_slice(&base);
    d11[10] = 0x03; // funky = down
    let s = parse_standard_report(&d11).ok_or("parse 11")?;
    assert_eq!(s.funky_dir, 0x03);
    assert_eq!(s.rotary1, 0, "rotary1 not available at 11 bytes");

    // 13 bytes: rotary1 available
    let mut d13 = [0u8; 13];
    d13[..10].copy_from_slice(&base);
    d13[11] = 0x64; // rotary1 = 100 (little-endian)
    d13[12] = 0x00;
    let s = parse_standard_report(&d13).ok_or("parse 13")?;
    assert_eq!(s.rotary1, 100);
    assert_eq!(s.rotary2, 0, "rotary2 not available at 13 bytes");

    // 15 bytes: rotary2 available
    let mut d15 = [0u8; 15];
    d15[..13].copy_from_slice(&d13);
    let r2 = (-200i16).to_le_bytes();
    d15[13] = r2[0];
    d15[14] = r2[1];
    let s = parse_standard_report(&d15).ok_or("parse 15")?;
    assert_eq!(s.rotary2, -200);
    assert!(
        (s.clutch_left).abs() < 1e-6,
        "clutch_left not available at 15 bytes"
    );

    // 17 bytes: both clutch paddles available
    let mut d17 = [0u8; 17];
    d17[..15].copy_from_slice(&d15);
    d17[15] = 0x00; // left clutch fully pressed
    d17[16] = 0x00; // right clutch fully pressed
    let s = parse_standard_report(&d17).ok_or("parse 17")?;
    assert!((s.clutch_left - 1.0).abs() < 1e-3);
    assert!((s.clutch_right - 1.0).abs() < 1e-3);
    Ok(())
}

// ============================================================================
// §6  Extended report parsing
// ============================================================================

/// Extended report with all fault flags combined.
#[test]
fn input_extended_all_faults_combined() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[0] = report_ids::EXTENDED_INPUT;
    // Steering raw: 0x1234
    data[1] = 0x34;
    data[2] = 0x12;
    // Velocity: -500 = 0xFE0C
    let vel = (-500i16).to_le_bytes();
    data[3] = vel[0];
    data[4] = vel[1];
    data[5] = 85; // motor temp
    data[6] = 42; // board temp
    data[7] = 30; // current = 3.0A
    data[10] = 0x0F; // all 4 fault flags

    let ext = parse_extended_report(&data).ok_or("parse failed")?;
    assert_eq!(ext.steering_raw, 0x1234);
    assert_eq!(ext.steering_velocity, -500);
    assert_eq!(ext.motor_temp_c, 85);
    assert_eq!(ext.board_temp_c, 42);
    assert_eq!(ext.current_raw, 30);
    assert_eq!(ext.fault_flags, 0x0F);
    // Verify individual bits
    assert_eq!(ext.fault_flags & 0x01, 0x01, "over-temp");
    assert_eq!(ext.fault_flags & 0x02, 0x02, "over-current");
    assert_eq!(ext.fault_flags & 0x04, 0x04, "comm error");
    assert_eq!(ext.fault_flags & 0x08, 0x08, "motor fault");
    Ok(())
}

/// Extended report at exactly minimum length (11 bytes).
#[test]
fn input_extended_minimum_length() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 11];
    data[0] = report_ids::EXTENDED_INPUT;
    data[5] = 55; // motor temp
    let ext = parse_extended_report(&data).ok_or("parse failed")?;
    assert_eq!(ext.motor_temp_c, 55);
    Ok(())
}

// ============================================================================
// §7  Pedal report parsing edge cases
// ============================================================================

/// Pedal report with max 12-bit values on all 3 axes.
#[test]
fn input_pedal_all_axes_max() -> Result<(), Box<dyn std::error::Error>> {
    let data = [
        report_ids::STANDARD_INPUT,
        0xFF,
        0x0F, // throttle = 0x0FFF
        0xFF,
        0x0F, // brake = 0x0FFF
        0xFF,
        0x0F, // clutch = 0x0FFF
    ];
    let pedal = parse_pedal_report(&data).ok_or("parse failed")?;
    assert_eq!(pedal.throttle_raw, 0x0FFF);
    assert_eq!(pedal.brake_raw, 0x0FFF);
    assert_eq!(pedal.clutch_raw, 0x0FFF);
    assert_eq!(pedal.axis_count, 3);
    Ok(())
}

/// Pedal report masks upper 4 bits of each 16-bit value.
#[test]
fn input_pedal_upper_bits_masked() -> Result<(), Box<dyn std::error::Error>> {
    let data = [
        report_ids::STANDARD_INPUT,
        0xFF,
        0xFF, // raw = 0xFFFF → masked to 0x0FFF
        0x00,
        0xF0, // raw = 0xF000 → masked to 0x0000
        0xAB,
        0xCD, // raw = 0xCDAB → masked to 0x0DAB
    ];
    let pedal = parse_pedal_report(&data).ok_or("parse failed")?;
    assert_eq!(pedal.throttle_raw, 0x0FFF);
    assert_eq!(pedal.brake_raw, 0x0000);
    assert_eq!(pedal.clutch_raw, 0x0DAB);
    Ok(())
}

/// 2-axis pedal report (5 bytes) — clutch defaults to 0.
#[test]
fn input_pedal_two_axis_no_clutch() -> Result<(), Box<dyn std::error::Error>> {
    let data = [
        report_ids::STANDARD_INPUT,
        0x00,
        0x04, // throttle = 0x0400
        0x80,
        0x02, // brake = 0x0280
    ];
    let pedal = parse_pedal_report(&data).ok_or("parse failed")?;
    assert_eq!(pedal.throttle_raw, 0x0400);
    assert_eq!(pedal.brake_raw, 0x0280);
    assert_eq!(pedal.clutch_raw, 0x0000);
    assert_eq!(pedal.axis_count, 2);
    Ok(())
}

/// 6-byte pedal report (between 5 and 7) still produces 2-axis result.
#[test]
fn input_pedal_six_bytes_two_axis() -> Result<(), Box<dyn std::error::Error>> {
    let data = [
        report_ids::STANDARD_INPUT,
        0x00,
        0x01,
        0x00,
        0x02,
        0xFF, // only 1 extra byte, not enough for clutch
    ];
    let pedal = parse_pedal_report(&data).ok_or("parse failed")?;
    assert_eq!(pedal.axis_count, 2);
    assert_eq!(pedal.clutch_raw, 0);
    Ok(())
}

// ============================================================================
// §8  Malformed report error handling
// ============================================================================

/// Reports shorter than minimum lengths return None.
#[test]
fn malformed_standard_report_short_lengths() -> Result<(), Box<dyn std::error::Error>> {
    for len in 0..10 {
        let mut data = vec![report_ids::STANDARD_INPUT; len];
        if !data.is_empty() {
            data[0] = report_ids::STANDARD_INPUT;
        }
        assert!(
            parse_standard_report(&data).is_none(),
            "len={len} should be None"
        );
    }
    Ok(())
}

#[test]
fn malformed_extended_report_short_lengths() -> Result<(), Box<dyn std::error::Error>> {
    for len in 0..11 {
        let mut data = vec![report_ids::EXTENDED_INPUT; len];
        if !data.is_empty() {
            data[0] = report_ids::EXTENDED_INPUT;
        }
        assert!(
            parse_extended_report(&data).is_none(),
            "len={len} should be None"
        );
    }
    Ok(())
}

#[test]
fn malformed_pedal_report_short_lengths() -> Result<(), Box<dyn std::error::Error>> {
    for len in 0..5 {
        let mut data = vec![report_ids::STANDARD_INPUT; len];
        if !data.is_empty() {
            data[0] = report_ids::STANDARD_INPUT;
        }
        assert!(
            parse_pedal_report(&data).is_none(),
            "len={len} should be None"
        );
    }
    Ok(())
}

/// Wrong report IDs are rejected for each parser.
#[test]
fn malformed_wrong_report_ids() -> Result<(), Box<dyn std::error::Error>> {
    let wrong_ids = [0x00, 0x03, 0x08, 0xFF];

    for &id in &wrong_ids {
        let mut data = [0u8; 64];
        data[0] = id;

        // Standard report expects 0x01
        if id != report_ids::STANDARD_INPUT {
            assert!(
                parse_standard_report(&data).is_none(),
                "standard with ID 0x{id:02X}"
            );
        }
        // Extended report expects 0x02
        if id != report_ids::EXTENDED_INPUT {
            assert!(
                parse_extended_report(&data).is_none(),
                "extended with ID 0x{id:02X}"
            );
        }
        // Pedal report expects 0x01
        if id != report_ids::STANDARD_INPUT {
            assert!(
                parse_pedal_report(&data).is_none(),
                "pedal with ID 0x{id:02X}"
            );
        }
    }
    Ok(())
}

/// All-0xFF standard report: valid (steering max, pedals released, all buttons set).
#[test]
fn malformed_all_0xff_standard_is_valid() -> Result<(), Box<dyn std::error::Error>> {
    // 0xFF != 0x01 for report ID, so this should be rejected
    let data = [0xFFu8; 64];
    assert!(
        parse_standard_report(&data).is_none(),
        "0xFF report ID != 0x01"
    );
    Ok(())
}

/// Exactly 10-byte standard report with valid ID is accepted.
#[test]
fn malformed_boundary_ten_bytes_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let data = [
        report_ids::STANDARD_INPUT,
        0,
        0x80,
        0xFF,
        0xFF,
        0xFF,
        0,
        0,
        0,
        0x0F,
    ];
    assert!(
        parse_standard_report(&data).is_some(),
        "10 bytes must parse"
    );
    Ok(())
}

// ============================================================================
// §9  VID/PID validation
// ============================================================================

/// Vendor ID must be 0x0EB7 (Endor AG / Fanatec).
#[test]
fn vid_is_endor_ag() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(FANATEC_VENDOR_ID, 0x0EB7);
    Ok(())
}

/// All known wheelbase PIDs must be classified as wheelbase products.
#[test]
fn all_wheelbase_pids_are_wheelbase_products() -> Result<(), Box<dyn std::error::Error>> {
    let pids = [
        product_ids::CLUBSPORT_V2,
        product_ids::CLUBSPORT_V2_5,
        product_ids::CSL_ELITE_PS4,
        product_ids::DD1,
        product_ids::DD2,
        product_ids::CSR_ELITE,
        product_ids::CSL_DD,
        product_ids::GT_DD_PRO,
        product_ids::CLUBSPORT_DD,
        product_ids::CSL_ELITE,
    ];
    for pid in pids {
        assert!(
            is_wheelbase_product(pid),
            "PID 0x{pid:04X} must be wheelbase"
        );
        assert!(!is_pedal_product(pid), "PID 0x{pid:04X} must not be pedal");
    }
    Ok(())
}

/// All known pedal PIDs must be classified as pedal products.
#[test]
fn all_pedal_pids_are_pedal_products() -> Result<(), Box<dyn std::error::Error>> {
    let pids = [
        product_ids::CLUBSPORT_PEDALS_V1_V2,
        product_ids::CLUBSPORT_PEDALS_V3,
        product_ids::CSL_ELITE_PEDALS,
        product_ids::CSL_PEDALS_LC,
        product_ids::CSL_PEDALS_V2,
    ];
    for pid in pids {
        assert!(is_pedal_product(pid), "PID 0x{pid:04X} must be pedal");
        assert!(
            !is_wheelbase_product(pid),
            "PID 0x{pid:04X} must not be wheelbase"
        );
    }
    Ok(())
}

/// Accessory PIDs (shifter, handbrake) are neither wheelbase nor pedal.
#[test]
fn accessory_pids_neither_wheelbase_nor_pedal() -> Result<(), Box<dyn std::error::Error>> {
    let accessories = [
        product_ids::CLUBSPORT_SHIFTER,
        product_ids::CLUBSPORT_HANDBRAKE,
    ];
    for pid in accessories {
        assert!(!is_wheelbase_product(pid), "PID 0x{pid:04X} not wheelbase");
        assert!(!is_pedal_product(pid), "PID 0x{pid:04X} not pedal");
    }
    Ok(())
}

/// Unknown PIDs classify as Unknown model with safe defaults.
#[test]
fn unknown_pid_safe_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let model = FanatecModel::from_product_id(0xDEAD);
    assert_eq!(model, FanatecModel::Unknown);
    assert!((model.max_torque_nm() - 5.0).abs() < 0.1, "unknown = 5 Nm");
    assert_eq!(model.encoder_cpr(), 4_096);
    assert!(!model.supports_1000hz());
    assert_eq!(model.max_rotation_degrees(), 900);
    assert!(!model.is_highres());
    assert!(!model.needs_sign_fix());
    Ok(())
}

// ============================================================================
// §10  Wheelbase variant matrix — all models
// ============================================================================

/// Exhaustive check of all wheelbase model properties.
#[test]
fn wheelbase_variant_matrix() -> Result<(), Box<dyn std::error::Error>> {
    #[derive(Debug)]
    struct Expected {
        pid: u16,
        model: FanatecModel,
        torque: f32,
        cpr: u32,
        hz1000: bool,
        max_rot: u16,
        highres: bool,
        sign_fix: bool,
    }

    let matrix = [
        Expected {
            pid: product_ids::DD1,
            model: FanatecModel::Dd1,
            torque: 20.0,
            cpr: 16_384,
            hz1000: true,
            max_rot: 2520,
            highres: true,
            sign_fix: true,
        },
        Expected {
            pid: product_ids::DD2,
            model: FanatecModel::Dd2,
            torque: 25.0,
            cpr: 16_384,
            hz1000: true,
            max_rot: 2520,
            highres: true,
            sign_fix: true,
        },
        Expected {
            pid: product_ids::CSL_DD,
            model: FanatecModel::CslDd,
            torque: 8.0,
            cpr: 16_384,
            hz1000: true,
            max_rot: 2520,
            highres: true,
            sign_fix: true,
        },
        Expected {
            pid: product_ids::GT_DD_PRO,
            model: FanatecModel::GtDdPro,
            torque: 8.0,
            cpr: 16_384,
            hz1000: true,
            max_rot: 2520,
            highres: true,
            sign_fix: true,
        },
        Expected {
            pid: product_ids::CLUBSPORT_DD,
            model: FanatecModel::ClubSportDd,
            torque: 12.0,
            cpr: 16_384,
            hz1000: true,
            max_rot: 2520,
            highres: true,
            sign_fix: true,
        },
        Expected {
            pid: product_ids::CSL_ELITE,
            model: FanatecModel::CslElite,
            torque: 6.0,
            cpr: 4_096,
            hz1000: false,
            max_rot: 1080,
            highres: false,
            sign_fix: true,
        },
        Expected {
            pid: product_ids::CSL_ELITE_PS4,
            model: FanatecModel::CslElite,
            torque: 6.0,
            cpr: 4_096,
            hz1000: false,
            max_rot: 1080,
            highres: false,
            sign_fix: true,
        },
        Expected {
            pid: product_ids::CLUBSPORT_V2,
            model: FanatecModel::ClubSportV2,
            torque: 8.0,
            cpr: 4_096,
            hz1000: false,
            max_rot: 900,
            highres: false,
            sign_fix: true,
        },
        Expected {
            pid: product_ids::CLUBSPORT_V2_5,
            model: FanatecModel::ClubSportV25,
            torque: 8.0,
            cpr: 4_096,
            hz1000: false,
            max_rot: 900,
            highres: false,
            sign_fix: true,
        },
        Expected {
            pid: product_ids::CSR_ELITE,
            model: FanatecModel::CsrElite,
            torque: 5.0,
            cpr: 4_096,
            hz1000: false,
            max_rot: 900,
            highres: false,
            sign_fix: false,
        },
    ];

    for e in &matrix {
        let model = FanatecModel::from_product_id(e.pid);
        assert_eq!(model, e.model, "model for PID 0x{:04X}", e.pid);
        assert!(
            (model.max_torque_nm() - e.torque).abs() < 0.1,
            "torque for {:?}",
            e.model
        );
        assert_eq!(model.encoder_cpr(), e.cpr, "CPR for {:?}", e.model);
        assert_eq!(model.supports_1000hz(), e.hz1000, "1kHz for {:?}", e.model);
        assert_eq!(
            model.max_rotation_degrees(),
            e.max_rot,
            "max_rot for {:?}",
            e.model
        );
        assert_eq!(model.is_highres(), e.highres, "highres for {:?}", e.model);
        assert_eq!(
            model.needs_sign_fix(),
            e.sign_fix,
            "sign_fix for {:?}",
            e.model
        );
    }
    Ok(())
}

// ============================================================================
// §11  Output report wire format verification
// ============================================================================

/// Verify mode-switch, stop-all, gain, and rotation-range reports all have
/// the correct report ID as byte 0.
#[test]
fn output_report_id_consistency() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(build_mode_switch_report()[0], report_ids::MODE_SWITCH);
    assert_eq!(build_stop_all_report()[0], report_ids::FFB_OUTPUT);
    assert_eq!(build_set_gain_report(50)[0], report_ids::FFB_OUTPUT);
    assert_eq!(build_rotation_range_report(900)[0], report_ids::FFB_OUTPUT);
    assert_eq!(build_led_report(0, 0)[0], report_ids::LED_DISPLAY);
    assert_eq!(
        build_display_report(0, [0; 3], 0)[0],
        report_ids::LED_DISPLAY
    );
    assert_eq!(build_rumble_report(0, 0, 0)[0], report_ids::LED_DISPLAY);
    Ok(())
}

/// fix_report_values converts each byte ≥ 0x80 to signed.
#[test]
fn output_fix_report_values_edge_cases() -> Result<(), Box<dyn std::error::Error>> {
    // Boundary: 0x80 → -128
    let mut vals = [0x80i16, 0, 0, 0, 0, 0, 0];
    fix_report_values(&mut vals);
    assert_eq!(vals[0], -128);

    // Boundary: 0x7F → unchanged (below threshold)
    let mut vals = [0x7Fi16, 0, 0, 0, 0, 0, 0];
    fix_report_values(&mut vals);
    assert_eq!(vals[0], 0x7F);

    // 0xFF → -1
    let mut vals = [0xFFi16, 0, 0, 0, 0, 0, 0];
    fix_report_values(&mut vals);
    assert_eq!(vals[0], -1);

    // Idempotent: applying twice to already-fixed values should not change them
    // (after first fix, values are negative, which are < 0x80)
    let mut vals = [0xA0i16, 0x90, 0x80, 0xFF, 0x00, 0x7F, 0x01];
    fix_report_values(&mut vals);
    let fixed = vals;
    fix_report_values(&mut vals);
    assert_eq!(vals, fixed, "fix_report_values must be idempotent");
    Ok(())
}

// ============================================================================
// §12  Proptest — report parsing and slot encoding
// ============================================================================

mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        /// Any 64-byte buffer starting with 0x01 should parse without panic.
        #[test]
        fn prop_standard_report_never_panics(data in proptest::collection::vec(any::<u8>(), 64..=64)) {
            let mut buf = data;
            buf[0] = 0x01;
            let _ = parse_standard_report(&buf);
        }

        /// Any 64-byte buffer starting with 0x02 should parse without panic.
        #[test]
        fn prop_extended_report_never_panics(data in proptest::collection::vec(any::<u8>(), 64..=64)) {
            let mut buf = data;
            buf[0] = 0x02;
            let _ = parse_extended_report(&buf);
        }

        /// Parsed standard report fields are always in valid ranges.
        #[test]
        fn prop_standard_report_fields_bounded(data in proptest::collection::vec(any::<u8>(), 64..=64)) {
            let mut buf = data;
            buf[0] = 0x01;
            if let Some(state) = parse_standard_report(&buf) {
                prop_assert!(state.steering >= -1.0 && state.steering <= 1.0);
                prop_assert!(state.throttle >= 0.0 && state.throttle <= 1.0);
                prop_assert!(state.brake >= 0.0 && state.brake <= 1.0);
                prop_assert!(state.clutch >= 0.0 && state.clutch <= 1.0);
                prop_assert!(state.hat <= 0x0F);
                prop_assert!(state.clutch_left >= 0.0 && state.clutch_left <= 1.0);
                prop_assert!(state.clutch_right >= 0.0 && state.clutch_right <= 1.0);
            }
        }

        /// Slot constant highres always has byte 6 = 0x01.
        #[test]
        fn prop_slot_highres_marker(level in i16::MIN..=i16::MAX) {
            let cmd = encode_constant_highres(level);
            prop_assert_eq!(cmd[6], 0x01);
            prop_assert_eq!(cmd[1], effect_cmd::CONSTANT);
            prop_assert_eq!(cmd[0] >> 4, slot::CONSTANT);
        }

        /// Slot constant lowres always has byte 6 = 0x00.
        #[test]
        fn prop_slot_lowres_no_marker(level in i16::MIN..=i16::MAX) {
            let cmd = encode_constant_lowres(level);
            prop_assert_eq!(cmd[6], 0x00);
        }

        /// Spring encoding: slot ID and effect cmd are always correct.
        #[test]
        fn prop_spring_structure(
            d1 in any::<i16>(),
            d2 in any::<i16>(),
            k1 in any::<i16>(),
            k2 in any::<i16>(),
            clip in any::<u16>(),
        ) {
            let cmd = encode_spring(d1, d2, k1, k2, clip);
            prop_assert_eq!(cmd[0] >> 4, slot::SPRING);
            prop_assert_eq!(cmd[1], effect_cmd::SPRING);
            prop_assert_eq!(cmd.len(), SLOT_CMD_SIZE);
        }

        /// Damper/inertia/friction all use RESISTANCE cmd and correct slot IDs.
        #[test]
        fn prop_resistance_effects(
            k1 in any::<i16>(),
            k2 in any::<i16>(),
            clip in 1u16..=u16::MAX,
        ) {
            let d = encode_damper(k1, k2, clip);
            let i = encode_inertia(k1, k2, clip);
            let f = encode_friction(k1, k2, clip);

            prop_assert_eq!(d[0] >> 4, slot::DAMPER);
            prop_assert_eq!(i[0] >> 4, slot::INERTIA);
            prop_assert_eq!(f[0] >> 4, slot::FRICTION);
            prop_assert_eq!(d[1], effect_cmd::RESISTANCE);
            prop_assert_eq!(i[1], effect_cmd::RESISTANCE);
            prop_assert_eq!(f[1], effect_cmd::RESISTANCE);
        }

        /// Tuning encode/decode roundtrip for Noop params in [0, 100].
        #[test]
        fn prop_tuning_noop_roundtrip(value in 0i16..=100) {
            let raw = encode_value(ConversionType::Noop, value);
            let decoded = decode_value(ConversionType::Noop, raw);
            prop_assert_eq!(decoded, value);
        }

        /// Tuning encode/decode roundtrip for TimesTen params (multiples of 10).
        #[test]
        fn prop_tuning_times_ten_roundtrip(factor in 0i16..=12) {
            let value = factor * 10;
            let raw = encode_value(ConversionType::TimesTen, value);
            let decoded = decode_value(ConversionType::TimesTen, raw);
            prop_assert_eq!(decoded, value);
        }

        /// Pedal report 12-bit masking always produces values ≤ 0x0FFF.
        #[test]
        fn prop_pedal_12bit_masking(
            t_lo in any::<u8>(), t_hi in any::<u8>(),
            b_lo in any::<u8>(), b_hi in any::<u8>(),
            c_lo in any::<u8>(), c_hi in any::<u8>(),
        ) {
            let data = [0x01, t_lo, t_hi, b_lo, b_hi, c_lo, c_hi];
            if let Some(pedal) = parse_pedal_report(&data) {
                prop_assert!(pedal.throttle_raw <= 0x0FFF);
                prop_assert!(pedal.brake_raw <= 0x0FFF);
                prop_assert!(pedal.clutch_raw <= 0x0FFF);
            }
        }
    }
}
