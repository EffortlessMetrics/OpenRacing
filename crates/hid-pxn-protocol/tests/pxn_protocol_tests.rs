//! Comprehensive PXN protocol hardening tests.
//!
//! Covers VID/PID validation, PIDFF encode roundtrips, known byte sequences,
//! effect encoding, and proptest fuzzing for all public API surfaces.

use racing_wheel_hid_pxn_protocol::*;

// ─── VID / PID golden values ────────────────────────────────────────────

#[test]
fn vid_golden_value() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(VENDOR_ID, 0x11FF, "PXN VID must be Lite Star 0x11FF");
    Ok(())
}

#[test]
fn pid_golden_values() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(PRODUCT_V10, 0x3245);
    assert_eq!(PRODUCT_V12, 0x1212);
    assert_eq!(PRODUCT_V12_LITE, 0x1112);
    assert_eq!(PRODUCT_V12_LITE_2, 0x1211);
    assert_eq!(PRODUCT_GT987, 0x2141);
    Ok(())
}

#[test]
fn all_pids_distinct() -> Result<(), Box<dyn std::error::Error>> {
    let pids = [
        PRODUCT_V10,
        PRODUCT_V12,
        PRODUCT_V12_LITE,
        PRODUCT_V12_LITE_2,
        PRODUCT_GT987,
    ];
    for i in 0..pids.len() {
        for j in (i + 1)..pids.len() {
            assert_ne!(
                pids[i], pids[j],
                "PIDs at index {i} and {j} must be distinct"
            );
        }
    }
    Ok(())
}

#[test]
fn is_pxn_accepts_all_known_products() -> Result<(), Box<dyn std::error::Error>> {
    let pids = [
        PRODUCT_V10,
        PRODUCT_V12,
        PRODUCT_V12_LITE,
        PRODUCT_V12_LITE_2,
        PRODUCT_GT987,
    ];
    for pid in pids {
        assert!(
            is_pxn(VENDOR_ID, pid),
            "is_pxn must accept known PID 0x{pid:04X}"
        );
    }
    Ok(())
}

#[test]
fn is_pxn_rejects_wrong_vid() -> Result<(), Box<dyn std::error::Error>> {
    assert!(!is_pxn(0x0000, PRODUCT_V10));
    assert!(!is_pxn(0x0483, PRODUCT_V10)); // STM VID
    assert!(!is_pxn(0x045B, PRODUCT_V10)); // FFBeast VID
    assert!(!is_pxn(0xFFFF, PRODUCT_V10));
    Ok(())
}

#[test]
fn is_pxn_rejects_unknown_pids() -> Result<(), Box<dyn std::error::Error>> {
    assert!(!is_pxn(VENDOR_ID, 0x0000));
    assert!(!is_pxn(VENDOR_ID, 0xFFFF));
    assert!(!is_pxn(VENDOR_ID, 0x0001));
    // Adjacent PIDs should not match
    assert!(!is_pxn(VENDOR_ID, PRODUCT_V10 + 1));
    assert!(!is_pxn(VENDOR_ID, PRODUCT_V10 - 1));
    Ok(())
}

#[test]
fn product_name_returns_correct_names() -> Result<(), Box<dyn std::error::Error>> {
    let expected = [
        (PRODUCT_V10, "PXN V10"),
        (PRODUCT_V12, "PXN V12"),
        (PRODUCT_V12_LITE, "PXN V12 Lite"),
        (PRODUCT_V12_LITE_2, "PXN V12 Lite (SE)"),
        (PRODUCT_GT987, "Lite Star GT987 FF"),
    ];
    for (pid, name) in expected {
        let result =
            product_name(pid).ok_or(format!("product_name returned None for 0x{pid:04X}"))?;
        assert_eq!(result, name);
    }
    Ok(())
}

#[test]
fn product_name_none_for_unknown() -> Result<(), Box<dyn std::error::Error>> {
    assert!(product_name(0x0000).is_none());
    assert!(product_name(0xFFFF).is_none());
    assert!(product_name(VENDOR_ID).is_none()); // VID is not a PID
    Ok(())
}

// ─── PIDFF effect encode roundtrips ─────────────────────────────────────

#[test]
fn constant_force_report_structure() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_set_constant_force(1, 7500);
    assert_eq!(report[0], 0x05, "constant force report ID");
    assert_eq!(report[1], 1, "block index");
    let mag = i16::from_le_bytes([report[2], report[3]]);
    assert_eq!(mag, 7500);
    Ok(())
}

#[test]
fn constant_force_negative_magnitude() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_set_constant_force(2, -10000);
    assert_eq!(report[1], 2);
    let mag = i16::from_le_bytes([report[2], report[3]]);
    assert_eq!(mag, -10000);
    Ok(())
}

#[test]
fn constant_force_zero() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_set_constant_force(1, 0);
    let mag = i16::from_le_bytes([report[2], report[3]]);
    assert_eq!(mag, 0);
    Ok(())
}

#[test]
fn set_effect_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_set_effect(1, EffectType::Sine, DURATION_INFINITE, 255, 0);
    assert_eq!(report[0], 0x01, "set_effect report ID");
    assert_eq!(report[1], 1, "block index");
    assert_eq!(report[2], EffectType::Sine as u8);
    Ok(())
}

#[test]
fn device_control_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let enable = encode_device_control(0x01); // enable actuators
    assert_eq!(enable, [0x0C, 0x01]);

    let disable = encode_device_control(0x02); // disable actuators
    assert_eq!(disable, [0x0C, 0x02]);

    let reset = encode_device_control(0x03); // stop all effects
    assert_eq!(reset, [0x0C, 0x03]);
    Ok(())
}

#[test]
fn device_gain_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_device_gain(10000);
    assert_eq!(report[0], 0x0D, "device gain report ID");
    assert_eq!(report[1], 0x00); // reserved
    let gain = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(gain, 10000);
    Ok(())
}

#[test]
fn device_gain_clamps_to_10000() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_device_gain(20000);
    let gain = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(gain, 10000, "gain must clamp to 10000");
    Ok(())
}

#[test]
fn device_gain_zero() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_device_gain(0);
    let gain = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(gain, 0);
    Ok(())
}

#[test]
fn block_free_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_block_free(5);
    assert_eq!(report[0], 0x0B, "block free report ID");
    assert_eq!(report[1], 5, "block index");
    Ok(())
}

#[test]
fn effect_operation_start_solo() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_effect_operation(1, EffectOp::Start, 0);
    assert_eq!(report[0], 0x0A, "effect operation report ID");
    assert_eq!(report[1], 1, "block index");
    assert_eq!(report[2], EffectOp::Start as u8);
    Ok(())
}

#[test]
fn set_envelope_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_set_envelope(1, 5000, 3000, 100, 200);
    assert_eq!(report[0], 0x02, "envelope report ID");
    assert_eq!(report[1], 1);
    let attack = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(attack, 5000);
    let fade = u16::from_le_bytes([report[4], report[5]]);
    assert_eq!(fade, 3000);
    Ok(())
}

#[test]
fn set_condition_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_set_condition(1, 0, 100, -100, 5000, 3000, 500, 10);
    assert_eq!(report[0], 0x03, "condition report ID");
    assert_eq!(report[1], 1);
    Ok(())
}

#[test]
fn set_periodic_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_set_periodic(1, 8000, 0, 1000, 0);
    assert_eq!(report[0], 0x04, "periodic report ID");
    assert_eq!(report[1], 1);
    let mag = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(mag, 8000);
    Ok(())
}

#[test]
fn set_ramp_force_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_set_ramp_force(1, -5000, 5000);
    assert_eq!(report[0], 0x06, "ramp force report ID");
    assert_eq!(report[1], 1);
    let start = i16::from_le_bytes([report[2], report[3]]);
    let end = i16::from_le_bytes([report[4], report[5]]);
    assert_eq!(start, -5000);
    assert_eq!(end, 5000);
    Ok(())
}

// ─── Effect type values ─────────────────────────────────────────────────

#[test]
fn effect_type_discriminants() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(EffectType::Constant as u8, 1);
    assert_eq!(EffectType::Ramp as u8, 2);
    assert_eq!(EffectType::Square as u8, 3);
    assert_eq!(EffectType::Sine as u8, 4);
    assert_eq!(EffectType::Triangle as u8, 5);
    assert_eq!(EffectType::SawtoothUp as u8, 6);
    assert_eq!(EffectType::SawtoothDown as u8, 7);
    assert_eq!(EffectType::Spring as u8, 8);
    assert_eq!(EffectType::Damper as u8, 9);
    assert_eq!(EffectType::Inertia as u8, 10);
    assert_eq!(EffectType::Friction as u8, 11);
    Ok(())
}

// ─── PXN quirk: sine-only periodic ──────────────────────────────────────

#[test]
fn pxn_sine_only_quirk_documentation() -> Result<(), Box<dyn std::error::Error>> {
    // PXN firmware only reliably supports sine waveform for periodic effects.
    // Encode all periodic types and verify they produce valid reports.
    let types = [
        EffectType::Sine,
        EffectType::Square,
        EffectType::Triangle,
        EffectType::SawtoothUp,
        EffectType::SawtoothDown,
    ];
    for et in types {
        let report = encode_set_effect(1, et, DURATION_INFINITE, 255, 0);
        assert_eq!(report[0], 0x01);
        assert_eq!(report[2], et as u8);
    }
    Ok(())
}

// ─── Proptest fuzzing ───────────────────────────────────────────────────

mod proptest_effects {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_constant_force_magnitude_roundtrips(block in 1u8..=255, mag in -10000i16..=10000) {
            let report = encode_set_constant_force(block, mag);
            prop_assert_eq!(report[0], 0x05);
            prop_assert_eq!(report[1], block);
            let decoded = i16::from_le_bytes([report[2], report[3]]);
            prop_assert_eq!(decoded, mag);
        }

        #[test]
        fn prop_ramp_force_roundtrips(block in 1u8..=255, start in -10000i16..=10000, end in -10000i16..=10000) {
            let report = encode_set_ramp_force(block, start, end);
            prop_assert_eq!(report[0], 0x06);
            let decoded_start = i16::from_le_bytes([report[2], report[3]]);
            let decoded_end = i16::from_le_bytes([report[4], report[5]]);
            prop_assert_eq!(decoded_start, start);
            prop_assert_eq!(decoded_end, end);
        }

        #[test]
        fn prop_device_gain_clamps(gain in 0u16..=30000) {
            let report = encode_device_gain(gain);
            let decoded = u16::from_le_bytes([report[2], report[3]]);
            prop_assert!(decoded <= 10000, "gain must clamp to 10000, got {decoded}");
        }

        #[test]
        fn prop_block_free_roundtrips(block in 0u8..=255) {
            let report = encode_block_free(block);
            prop_assert_eq!(report[0], 0x0B);
            prop_assert_eq!(report[1], block);
        }

        #[test]
        fn prop_effect_operation_report_id_stable(block in 1u8..=255, loop_count in 0u8..=255) {
            let report = encode_effect_operation(block, EffectOp::Start, loop_count);
            prop_assert_eq!(report[0], 0x0A);
            prop_assert_eq!(report[1], block);
        }

        #[test]
        fn prop_set_effect_all_types(block in 1u8..=255, dur in 0u16..=65535u16, gain in 0u8..=255) {
            let types = [
                EffectType::Constant, EffectType::Ramp, EffectType::Sine,
                EffectType::Square, EffectType::Triangle,
                EffectType::SawtoothUp, EffectType::SawtoothDown,
                EffectType::Spring, EffectType::Damper,
                EffectType::Inertia, EffectType::Friction,
            ];
            for et in types {
                let report = encode_set_effect(block, et, dur, gain, 0);
                prop_assert_eq!(report[0], 0x01);
                prop_assert_eq!(report[1], block);
                prop_assert_eq!(report[2], et as u8);
            }
        }

        #[test]
        fn prop_is_pxn_only_with_correct_vid(pid in 0u16..=0xFFFF) {
            if is_pxn(VENDOR_ID, pid) {
                // Must be one of the known PIDs
                let known = [PRODUCT_V10, PRODUCT_V12, PRODUCT_V12_LITE, PRODUCT_V12_LITE_2, PRODUCT_GT987];
                prop_assert!(known.contains(&pid), "is_pxn accepted unknown PID 0x{pid:04X}");
            }
        }

        #[test]
        fn prop_product_name_some_iff_is_pxn(pid in 0u16..=0xFFFF) {
            let name_present = product_name(pid).is_some();
            let is_known = is_pxn(VENDOR_ID, pid);
            let msg = format!("product_name and is_pxn must agree for PID 0x{pid:04X}");
            prop_assert_eq!(name_present, is_known, "{}", msg);
        }
    }
}
