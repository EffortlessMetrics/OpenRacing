//! Comprehensive AccuForce protocol hardening tests.
//!
//! Covers VID/PID validation, device classification, PIDFF encode roundtrips,
//! report constants, and proptest fuzzing.

use racing_wheel_hid_accuforce_protocol::*;

// ─── VID / PID golden values ────────────────────────────────────────────

#[test]
fn vid_golden_value() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(VENDOR_ID, 0x1FC9, "AccuForce VID must be NXP 0x1FC9");
    Ok(())
}

#[test]
fn pid_golden_value() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(PID_ACCUFORCE_PRO, 0x804C);
    Ok(())
}

#[test]
fn is_accuforce_accepts_known_device() -> Result<(), Box<dyn std::error::Error>> {
    assert!(is_accuforce(VENDOR_ID, PID_ACCUFORCE_PRO));
    assert!(is_accuforce_pid(PID_ACCUFORCE_PRO));
    Ok(())
}

#[test]
fn is_accuforce_rejects_wrong_vid() -> Result<(), Box<dyn std::error::Error>> {
    assert!(!is_accuforce(0x0000, PID_ACCUFORCE_PRO));
    assert!(!is_accuforce(0x0483, PID_ACCUFORCE_PRO)); // STM VID
    assert!(!is_accuforce(0x1DD2, PID_ACCUFORCE_PRO)); // Leo Bodnar VID
    assert!(!is_accuforce(0x045B, PID_ACCUFORCE_PRO)); // FFBeast VID
    assert!(!is_accuforce(0xFFFF, PID_ACCUFORCE_PRO));
    Ok(())
}

#[test]
fn is_accuforce_rejects_unknown_pid() -> Result<(), Box<dyn std::error::Error>> {
    assert!(!is_accuforce(VENDOR_ID, 0x0000));
    assert!(!is_accuforce(VENDOR_ID, 0xFFFF));
    assert!(!is_accuforce_pid(0x0000));
    assert!(!is_accuforce_pid(0xFFFF));
    assert!(!is_accuforce_pid(PID_ACCUFORCE_PRO + 1));
    assert!(!is_accuforce_pid(PID_ACCUFORCE_PRO - 1));
    Ok(())
}

// ─── Model classification ───────────────────────────────────────────────

#[test]
fn model_from_product_id_pro() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        AccuForceModel::from_product_id(PID_ACCUFORCE_PRO),
        AccuForceModel::Pro
    );
    Ok(())
}

#[test]
fn model_from_product_id_unknown() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        AccuForceModel::from_product_id(0x0000),
        AccuForceModel::Unknown
    );
    assert_eq!(
        AccuForceModel::from_product_id(0xFFFF),
        AccuForceModel::Unknown
    );
    Ok(())
}

#[test]
fn model_display_names() -> Result<(), Box<dyn std::error::Error>> {
    let pro_name = AccuForceModel::Pro.display_name();
    assert!(
        pro_name.contains("AccuForce"),
        "Pro name must contain brand"
    );
    assert!(!pro_name.is_empty());

    let unknown_name = AccuForceModel::Unknown.display_name();
    assert!(!unknown_name.is_empty());
    assert!(unknown_name.contains("unknown") || unknown_name.contains("AccuForce"));
    Ok(())
}

#[test]
fn model_max_torque() -> Result<(), Box<dyn std::error::Error>> {
    assert!(
        (AccuForceModel::Pro.max_torque_nm() - 7.0).abs() < 0.001,
        "Pro = 7 Nm"
    );
    assert!(
        AccuForceModel::Unknown.max_torque_nm() > 0.0,
        "Unknown has safe fallback"
    );
    Ok(())
}

// ─── DeviceInfo ─────────────────────────────────────────────────────────

#[test]
fn device_info_from_vid_pid_pro() -> Result<(), Box<dyn std::error::Error>> {
    let info = DeviceInfo::from_vid_pid(VENDOR_ID, PID_ACCUFORCE_PRO);
    assert_eq!(info.vendor_id, VENDOR_ID);
    assert_eq!(info.product_id, PID_ACCUFORCE_PRO);
    assert_eq!(info.model, AccuForceModel::Pro);
    Ok(())
}

#[test]
fn device_info_from_vid_pid_unknown() -> Result<(), Box<dyn std::error::Error>> {
    let info = DeviceInfo::from_vid_pid(VENDOR_ID, 0xFFFF);
    assert_eq!(info.model, AccuForceModel::Unknown);
    Ok(())
}

#[test]
fn device_info_preserves_vid_even_if_wrong() -> Result<(), Box<dyn std::error::Error>> {
    let info = DeviceInfo::from_vid_pid(0xBEEF, PID_ACCUFORCE_PRO);
    assert_eq!(info.vendor_id, 0xBEEF);
    assert_eq!(info.product_id, PID_ACCUFORCE_PRO);
    assert_eq!(info.model, AccuForceModel::Pro);
    Ok(())
}

// ─── Report constants ───────────────────────────────────────────────────

#[test]
fn report_constants_golden() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(MAX_REPORT_BYTES, 64);
    assert_eq!(HID_PID_USAGE_PAGE, 0x000F);
    assert_eq!(RECOMMENDED_B_INTERVAL_MS, 8);
    Ok(())
}

// ─── PIDFF encode roundtrips ────────────────────────────────────────────

#[test]
fn pidff_constant_force_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_set_constant_force(1, 7000);
    assert_eq!(report[0], 0x05, "constant force report ID");
    assert_eq!(report[1], 1, "block index");
    let mag = i16::from_le_bytes([report[2], report[3]]);
    assert_eq!(mag, 7000);
    Ok(())
}

#[test]
fn pidff_constant_force_negative() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_set_constant_force(1, -7000);
    let mag = i16::from_le_bytes([report[2], report[3]]);
    assert_eq!(mag, -7000);
    Ok(())
}

#[test]
fn pidff_device_control_commands() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(encode_device_control(0x01), [0x0C, 0x01]); // enable
    assert_eq!(encode_device_control(0x02), [0x0C, 0x02]); // disable
    assert_eq!(encode_device_control(0x03), [0x0C, 0x03]); // stop
    Ok(())
}

#[test]
fn pidff_device_gain_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_device_gain(7000);
    assert_eq!(report[0], 0x0D);
    let gain = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(gain, 7000);
    Ok(())
}

#[test]
fn pidff_device_gain_clamps_to_10000() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_device_gain(15000);
    let gain = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(gain, 10000);
    Ok(())
}

#[test]
fn pidff_block_free_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_block_free(7);
    assert_eq!(report, [0x0B, 7]);
    Ok(())
}

#[test]
fn pidff_set_effect_constant() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_set_effect(1, EffectType::Constant, DURATION_INFINITE, 255, 0);
    assert_eq!(report[0], 0x01);
    assert_eq!(report[2], EffectType::Constant as u8);
    Ok(())
}

#[test]
fn pidff_set_effect_spring() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_set_effect(1, EffectType::Spring, 5000, 200, 0);
    assert_eq!(report[2], EffectType::Spring as u8);
    Ok(())
}

#[test]
fn pidff_set_envelope_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_set_envelope(1, 5000, 3000, 100, 200);
    assert_eq!(report[0], 0x02);
    let attack = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(attack, 5000);
    Ok(())
}

#[test]
fn pidff_set_periodic_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_set_periodic(1, 8000, 0, 500, 0);
    assert_eq!(report[0], 0x04);
    let mag = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(mag, 8000);
    Ok(())
}

#[test]
fn pidff_set_ramp_force_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_set_ramp_force(1, -3500, 3500);
    assert_eq!(report[0], 0x06);
    let start = i16::from_le_bytes([report[2], report[3]]);
    let end = i16::from_le_bytes([report[4], report[5]]);
    assert_eq!(start, -3500);
    assert_eq!(end, 3500);
    Ok(())
}

#[test]
fn pidff_set_condition_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_set_condition(1, 0, 50, -50, 3000, 2000, 100, 10);
    assert_eq!(report[0], 0x03);
    assert_eq!(report[1], 1);
    Ok(())
}

// ─── Effect type values ─────────────────────────────────────────────────

#[test]
fn effect_type_values_match_pid_spec() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(EffectType::Constant as u8, 1);
    assert_eq!(EffectType::Sine as u8, 4);
    assert_eq!(EffectType::Spring as u8, 8);
    assert_eq!(EffectType::Damper as u8, 9);
    assert_eq!(EffectType::Friction as u8, 11);
    Ok(())
}

#[test]
fn duration_infinite_is_ffff() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(DURATION_INFINITE, 0xFFFF);
    Ok(())
}

// ─── Proptest fuzzing ───────────────────────────────────────────────────

mod proptest_accuforce {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_is_accuforce_requires_correct_vid(pid in 0u16..=0xFFFF) {
            prop_assert!(!is_accuforce(0x0000, pid));
            prop_assert!(!is_accuforce(0xFFFF, pid));
        }

        #[test]
        fn prop_is_accuforce_pid_deterministic(pid in 0u16..=0xFFFF) {
            let a = is_accuforce_pid(pid);
            let b = is_accuforce_pid(pid);
            prop_assert_eq!(a, b);
        }

        #[test]
        fn prop_model_from_pid_deterministic(pid in 0u16..=0xFFFF) {
            let a = AccuForceModel::from_product_id(pid);
            let b = AccuForceModel::from_product_id(pid);
            prop_assert_eq!(a, b);
        }

        #[test]
        fn prop_model_max_torque_positive(pid in 0u16..=0xFFFF) {
            let model = AccuForceModel::from_product_id(pid);
            prop_assert!(model.max_torque_nm() > 0.0);
        }

        #[test]
        fn prop_model_display_name_nonempty(pid in 0u16..=0xFFFF) {
            let model = AccuForceModel::from_product_id(pid);
            prop_assert!(!model.display_name().is_empty());
        }

        #[test]
        fn prop_device_info_consistent(vid in 0u16..=0xFFFF, pid in 0u16..=0xFFFF) {
            let info = DeviceInfo::from_vid_pid(vid, pid);
            prop_assert_eq!(info.vendor_id, vid);
            prop_assert_eq!(info.product_id, pid);
            prop_assert_eq!(info.model, AccuForceModel::from_product_id(pid));
        }

        #[test]
        fn prop_constant_force_magnitude_roundtrips(block in 1u8..=255, mag in -10000i16..=10000) {
            let report = encode_set_constant_force(block, mag);
            prop_assert_eq!(report[0], 0x05);
            prop_assert_eq!(report[1], block);
            let decoded = i16::from_le_bytes([report[2], report[3]]);
            prop_assert_eq!(decoded, mag);
        }

        #[test]
        fn prop_device_gain_clamps(gain in 0u16..=30000) {
            let report = encode_device_gain(gain);
            let decoded = u16::from_le_bytes([report[2], report[3]]);
            prop_assert!(decoded <= 10000);
        }

        #[test]
        fn prop_ramp_force_roundtrips(block in 1u8..=255, start in -10000i16..=10000, end in -10000i16..=10000) {
            let report = encode_set_ramp_force(block, start, end);
            let decoded_start = i16::from_le_bytes([report[2], report[3]]);
            let decoded_end = i16::from_le_bytes([report[4], report[5]]);
            prop_assert_eq!(decoded_start, start);
            prop_assert_eq!(decoded_end, end);
        }
    }
}
