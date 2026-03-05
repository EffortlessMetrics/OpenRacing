//! Comprehensive Leo Bodnar protocol hardening tests.
//!
//! Covers VID/PID validation, device classification, FFB capability,
//! PIDFF encode roundtrips, report constants, and proptest fuzzing.

use racing_wheel_hid_leo_bodnar_protocol::*;

// ─── VID / PID golden values ────────────────────────────────────────────

#[test]
fn vid_golden_value() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(VENDOR_ID, 0x1DD2, "Leo Bodnar VID");
    Ok(())
}

#[test]
fn confirmed_pid_golden_values() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(PID_USB_JOYSTICK, 0x0001);
    assert_eq!(PID_BBI32, 0x000C);
    assert_eq!(PID_WHEEL_INTERFACE, 0x000E);
    assert_eq!(PID_FFB_JOYSTICK, 0x000F);
    assert_eq!(PID_SLI_M, 0x1301);
    Ok(())
}

#[test]
fn estimated_pid_golden_values() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(PID_BU0836A, 0x000B);
    assert_eq!(PID_BU0836X, 0x0030);
    assert_eq!(PID_BU0836_16BIT, 0x0031);
    Ok(())
}

#[test]
fn all_pids_nonzero_and_distinct() -> Result<(), Box<dyn std::error::Error>> {
    let pids = [
        PID_USB_JOYSTICK,
        PID_BBI32,
        PID_WHEEL_INTERFACE,
        PID_FFB_JOYSTICK,
        PID_SLI_M,
        PID_BU0836A,
        PID_BU0836X,
        PID_BU0836_16BIT,
    ];
    for &pid in &pids {
        assert_ne!(pid, 0, "PID must not be zero");
    }
    for i in 0..pids.len() {
        for j in (i + 1)..pids.len() {
            assert_ne!(pids[i], pids[j], "PIDs at [{i}] and [{j}] must be distinct");
        }
    }
    Ok(())
}

// ─── VID/PID matching ───────────────────────────────────────────────────

#[test]
fn is_leo_bodnar_all_known_pids() -> Result<(), Box<dyn std::error::Error>> {
    let pids = [
        PID_USB_JOYSTICK,
        PID_BBI32,
        PID_WHEEL_INTERFACE,
        PID_FFB_JOYSTICK,
        PID_SLI_M,
        PID_BU0836A,
        PID_BU0836X,
        PID_BU0836_16BIT,
    ];
    for pid in pids {
        assert!(
            is_leo_bodnar(VENDOR_ID, pid),
            "should accept PID 0x{pid:04X}"
        );
        assert!(
            is_leo_bodnar_device(pid),
            "device check for PID 0x{pid:04X}"
        );
    }
    Ok(())
}

#[test]
fn is_leo_bodnar_rejects_wrong_vid() -> Result<(), Box<dyn std::error::Error>> {
    assert!(!is_leo_bodnar(0x0000, PID_WHEEL_INTERFACE));
    assert!(!is_leo_bodnar(0x0483, PID_WHEEL_INTERFACE)); // STM VID
    assert!(!is_leo_bodnar(0x1FC9, PID_WHEEL_INTERFACE)); // AccuForce VID
    assert!(!is_leo_bodnar(0xFFFF, PID_WHEEL_INTERFACE));
    Ok(())
}

#[test]
fn is_leo_bodnar_rejects_unknown_pids() -> Result<(), Box<dyn std::error::Error>> {
    assert!(!is_leo_bodnar(VENDOR_ID, 0x0000));
    assert!(!is_leo_bodnar(VENDOR_ID, 0xFFFF));
    assert!(!is_leo_bodnar_device(0x0000));
    assert!(!is_leo_bodnar_device(0xFFFF));
    Ok(())
}

#[test]
fn ffb_pid_identification() -> Result<(), Box<dyn std::error::Error>> {
    assert!(is_leo_bodnar_ffb_pid(PID_WHEEL_INTERFACE));
    assert!(is_leo_bodnar_ffb_pid(PID_FFB_JOYSTICK));

    // Non-FFB PIDs
    assert!(!is_leo_bodnar_ffb_pid(PID_USB_JOYSTICK));
    assert!(!is_leo_bodnar_ffb_pid(PID_BBI32));
    assert!(!is_leo_bodnar_ffb_pid(PID_SLI_M));
    assert!(!is_leo_bodnar_ffb_pid(PID_BU0836A));
    assert!(!is_leo_bodnar_ffb_pid(PID_BU0836X));
    assert!(!is_leo_bodnar_ffb_pid(PID_BU0836_16BIT));
    assert!(!is_leo_bodnar_ffb_pid(0x0000));
    assert!(!is_leo_bodnar_ffb_pid(0xFFFF));
    Ok(())
}

// ─── Device classification ──────────────────────────────────────────────

#[test]
fn from_product_id_all_known() -> Result<(), Box<dyn std::error::Error>> {
    let mapping = [
        (PID_USB_JOYSTICK, LeoBodnarDevice::UsbJoystick),
        (PID_BU0836A, LeoBodnarDevice::Bu0836a),
        (PID_BBI32, LeoBodnarDevice::Bbi32),
        (PID_WHEEL_INTERFACE, LeoBodnarDevice::WheelInterface),
        (PID_FFB_JOYSTICK, LeoBodnarDevice::FfbJoystick),
        (PID_BU0836X, LeoBodnarDevice::Bu0836x),
        (PID_BU0836_16BIT, LeoBodnarDevice::Bu0836_16bit),
        (PID_SLI_M, LeoBodnarDevice::SlimShiftLight),
    ];
    for (pid, expected) in mapping {
        let device = LeoBodnarDevice::from_product_id(pid)
            .ok_or(format!("from_product_id returned None for 0x{pid:04X}"))?;
        assert_eq!(device, expected);
    }
    Ok(())
}

#[test]
fn from_product_id_unknown() -> Result<(), Box<dyn std::error::Error>> {
    assert!(LeoBodnarDevice::from_product_id(0x0000).is_none());
    assert!(LeoBodnarDevice::from_product_id(0xFFFF).is_none());
    assert!(LeoBodnarDevice::from_product_id(0x1234).is_none());
    Ok(())
}

#[test]
fn device_ffb_support() -> Result<(), Box<dyn std::error::Error>> {
    let ffb_devices = [
        LeoBodnarDevice::WheelInterface,
        LeoBodnarDevice::FfbJoystick,
    ];
    let non_ffb = [
        LeoBodnarDevice::UsbJoystick,
        LeoBodnarDevice::Bu0836a,
        LeoBodnarDevice::Bbi32,
        LeoBodnarDevice::Bu0836x,
        LeoBodnarDevice::Bu0836_16bit,
        LeoBodnarDevice::SlimShiftLight,
    ];
    for dev in ffb_devices {
        assert!(dev.supports_ffb(), "{dev:?} should support FFB");
    }
    for dev in non_ffb {
        assert!(!dev.supports_ffb(), "{dev:?} should not support FFB");
    }
    Ok(())
}

#[test]
fn device_input_channels() -> Result<(), Box<dyn std::error::Error>> {
    let joystick_types = [
        LeoBodnarDevice::UsbJoystick,
        LeoBodnarDevice::Bu0836a,
        LeoBodnarDevice::Bbi32,
        LeoBodnarDevice::WheelInterface,
        LeoBodnarDevice::FfbJoystick,
        LeoBodnarDevice::Bu0836x,
        LeoBodnarDevice::Bu0836_16bit,
    ];
    for dev in joystick_types {
        assert_eq!(
            dev.max_input_channels(),
            32,
            "{dev:?} should have 32 channels"
        );
    }
    assert_eq!(LeoBodnarDevice::SlimShiftLight.max_input_channels(), 0);
    Ok(())
}

#[test]
fn device_names_nonempty_and_contain_brand() -> Result<(), Box<dyn std::error::Error>> {
    let all_devices = [
        LeoBodnarDevice::UsbJoystick,
        LeoBodnarDevice::Bu0836a,
        LeoBodnarDevice::Bbi32,
        LeoBodnarDevice::WheelInterface,
        LeoBodnarDevice::FfbJoystick,
        LeoBodnarDevice::Bu0836x,
        LeoBodnarDevice::Bu0836_16bit,
        LeoBodnarDevice::SlimShiftLight,
        LeoBodnarDevice::Pedals,
        LeoBodnarDevice::LcPedals,
    ];
    for dev in all_devices {
        let name = dev.name();
        assert!(!name.is_empty(), "{dev:?} name must not be empty");
        assert!(
            name.contains("Leo Bodnar"),
            "{dev:?} name should contain brand"
        );
    }
    Ok(())
}

#[test]
fn pedals_have_zero_input_channels() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(LeoBodnarDevice::Pedals.max_input_channels(), 0);
    assert_eq!(LeoBodnarDevice::LcPedals.max_input_channels(), 0);
    Ok(())
}

#[test]
fn pedals_no_ffb() -> Result<(), Box<dyn std::error::Error>> {
    assert!(!LeoBodnarDevice::Pedals.supports_ffb());
    assert!(!LeoBodnarDevice::LcPedals.supports_ffb());
    Ok(())
}

// ─── Report constants ───────────────────────────────────────────────────

#[test]
fn report_constants_golden() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(MAX_REPORT_BYTES, 64);
    assert_eq!(HID_PID_USAGE_PAGE, 0x000F);
    assert_eq!(WHEEL_ENCODER_CPR, 65535);
    assert!(WHEEL_DEFAULT_MAX_TORQUE_NM > 0.0);
    assert!((WHEEL_DEFAULT_MAX_TORQUE_NM - 10.0).abs() < 0.001);
    Ok(())
}

// ─── PIDFF encode roundtrips ────────────────────────────────────────────

#[test]
fn pidff_constant_force_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_set_constant_force(1, 7500);
    assert_eq!(report[0], 0x05);
    assert_eq!(report[1], 1);
    let mag = i16::from_le_bytes([report[2], report[3]]);
    assert_eq!(mag, 7500);
    Ok(())
}

#[test]
fn pidff_device_control_enable() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_device_control(0x01);
    assert_eq!(report, [0x0C, 0x01]);
    Ok(())
}

#[test]
fn pidff_device_gain_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_device_gain(8000);
    assert_eq!(report[0], 0x0D);
    let gain = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(gain, 8000);
    Ok(())
}

#[test]
fn pidff_block_free_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_block_free(3);
    assert_eq!(report, [0x0B, 3]);
    Ok(())
}

#[test]
fn pidff_set_effect_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_set_effect(1, EffectType::Constant, DURATION_INFINITE, 200, 0);
    assert_eq!(report[0], 0x01);
    assert_eq!(report[1], 1);
    assert_eq!(report[2], EffectType::Constant as u8);
    Ok(())
}

#[test]
fn pidff_set_envelope_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_set_envelope(1, 5000, 3000, 100, 200);
    assert_eq!(report[0], 0x02);
    let attack = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(attack, 5000);
    Ok(())
}

#[test]
fn pidff_set_condition_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_set_condition(1, 0, 100, -100, 5000, 3000, 500, 10);
    assert_eq!(report[0], 0x03);
    assert_eq!(report[1], 1);
    Ok(())
}

#[test]
fn pidff_set_periodic_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_set_periodic(1, 8000, 0, 1000, 0);
    assert_eq!(report[0], 0x04);
    let mag = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(mag, 8000);
    Ok(())
}

#[test]
fn pidff_set_ramp_force_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let report = encode_set_ramp_force(1, -5000, 5000);
    assert_eq!(report[0], 0x06);
    let start = i16::from_le_bytes([report[2], report[3]]);
    let end = i16::from_le_bytes([report[4], report[5]]);
    assert_eq!(start, -5000);
    assert_eq!(end, 5000);
    Ok(())
}

// ─── Proptest fuzzing ───────────────────────────────────────────────────

mod proptest_leo_bodnar {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_is_leo_bodnar_requires_correct_vid(pid in 0u16..=0xFFFF) {
            if is_leo_bodnar(VENDOR_ID, pid) {
                prop_assert!(is_leo_bodnar_device(pid));
            }
            // Wrong VID never matches
            prop_assert!(!is_leo_bodnar(0x0000, pid));
        }

        #[test]
        fn prop_from_product_id_consistent_with_is_device(pid in 0u16..=0xFFFF) {
            let device = LeoBodnarDevice::from_product_id(pid);
            let is_known = is_leo_bodnar_device(pid);
            let msg = format!("from_product_id and is_leo_bodnar_device must agree for 0x{pid:04X}");
            prop_assert_eq!(device.is_some(), is_known, "{}", msg);
        }

        #[test]
        fn prop_ffb_pid_subset_of_known(pid in 0u16..=0xFFFF) {
            if is_leo_bodnar_ffb_pid(pid) {
                prop_assert!(is_leo_bodnar_device(pid),
                    "FFB PID 0x{pid:04X} must be a known device");
            }
        }

        #[test]
        fn prop_device_name_nonempty(pid in 0u16..=0xFFFF) {
            if let Some(dev) = LeoBodnarDevice::from_product_id(pid) {
                prop_assert!(!dev.name().is_empty());
            }
        }

        #[test]
        fn prop_ffb_support_consistent_with_ffb_pid(pid in 0u16..=0xFFFF) {
            if let Some(dev) = LeoBodnarDevice::from_product_id(pid) {
                prop_assert_eq!(dev.supports_ffb(), is_leo_bodnar_ffb_pid(pid));
            }
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
    }
}
