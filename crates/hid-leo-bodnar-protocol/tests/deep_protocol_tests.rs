//! Deep protocol tests for Leo Bodnar HID protocol crate.

use racing_wheel_hid_leo_bodnar_protocol::{
    HID_PID_USAGE_PAGE, MAX_REPORT_BYTES, PID_BBI32, PID_BU0836A, PID_BU0836X,
    PID_BU0836_16BIT, PID_FFB_JOYSTICK, PID_SLI_M, PID_USB_JOYSTICK, PID_WHEEL_INTERFACE,
    VENDOR_ID, WHEEL_DEFAULT_MAX_TORQUE_NM, WHEEL_ENCODER_CPR, is_leo_bodnar,
    is_leo_bodnar_device, is_leo_bodnar_ffb_pid, LeoBodnarDevice,
};

// ── Device identification ────────────────────────────────────────────────────

#[test]
fn vendor_id_is_correct_usb_value() {
    assert_eq!(VENDOR_ID, 0x1DD2, "VID must be 0x1DD2 per USB-IF");
}

#[test]
fn all_confirmed_pids_are_nonzero() {
    let pids = [
        PID_USB_JOYSTICK,
        PID_BBI32,
        PID_WHEEL_INTERFACE,
        PID_FFB_JOYSTICK,
        PID_SLI_M,
    ];
    for pid in pids {
        assert_ne!(pid, 0, "confirmed PID must not be zero");
    }
}

#[test]
fn all_estimated_pids_are_nonzero() {
    let pids = [PID_BU0836A, PID_BU0836X, PID_BU0836_16BIT];
    for pid in pids {
        assert_ne!(pid, 0, "estimated PID must not be zero");
    }
}

#[test]
fn pids_are_unique() {
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
    for (i, &a) in pids.iter().enumerate() {
        for (j, &b) in pids.iter().enumerate() {
            if i != j {
                assert_ne!(a, b, "PIDs at index {i} and {j} must be unique");
            }
        }
    }
}

#[test]
fn wrong_vid_rejects_all_pids() {
    let pids = [
        PID_USB_JOYSTICK,
        PID_BBI32,
        PID_WHEEL_INTERFACE,
        PID_FFB_JOYSTICK,
    ];
    for pid in pids {
        assert!(!is_leo_bodnar(0x0000, pid));
        assert!(!is_leo_bodnar(0xFFFF, pid));
    }
}

#[test]
fn is_leo_bodnar_device_matches_is_leo_bodnar_with_correct_vid() {
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
        assert_eq!(
            is_leo_bodnar_device(pid),
            is_leo_bodnar(VENDOR_ID, pid),
            "is_leo_bodnar_device and is_leo_bodnar must agree for PID 0x{pid:04X}"
        );
    }
}

// ── LeoBodnarDevice classification ──────────────────────────────────────────

#[test]
fn from_product_id_returns_correct_variant_for_each_pid() -> Result<(), String> {
    let cases: &[(u16, LeoBodnarDevice)] = &[
        (PID_USB_JOYSTICK, LeoBodnarDevice::UsbJoystick),
        (PID_BU0836A, LeoBodnarDevice::Bu0836a),
        (PID_BBI32, LeoBodnarDevice::Bbi32),
        (PID_WHEEL_INTERFACE, LeoBodnarDevice::WheelInterface),
        (PID_FFB_JOYSTICK, LeoBodnarDevice::FfbJoystick),
        (PID_BU0836X, LeoBodnarDevice::Bu0836x),
        (PID_BU0836_16BIT, LeoBodnarDevice::Bu0836_16bit),
        (PID_SLI_M, LeoBodnarDevice::SlimShiftLight),
    ];
    for &(pid, expected) in cases {
        let device = LeoBodnarDevice::from_product_id(pid)
            .ok_or_else(|| format!("PID 0x{pid:04X} should resolve"))?;
        assert_eq!(device, expected, "PID 0x{pid:04X} mapped to wrong variant");
    }
    Ok(())
}

#[test]
fn from_product_id_returns_none_for_adjacent_pids() {
    let unknowns: &[u16] = &[0x0000, 0x0002, 0x000A, 0x000D, 0x0010, 0x002F, 0x0032];
    for &pid in unknowns {
        assert!(
            LeoBodnarDevice::from_product_id(pid).is_none(),
            "PID 0x{pid:04X} should not resolve"
        );
    }
}

// ── FFB identification ───────────────────────────────────────────────────────

#[test]
fn only_wheel_and_ffb_joystick_are_ffb_capable() {
    let all = [
        PID_USB_JOYSTICK,
        PID_BU0836A,
        PID_BBI32,
        PID_WHEEL_INTERFACE,
        PID_FFB_JOYSTICK,
        PID_BU0836X,
        PID_BU0836_16BIT,
        PID_SLI_M,
    ];
    let ffb_set = [PID_WHEEL_INTERFACE, PID_FFB_JOYSTICK];
    for pid in all {
        let expected = ffb_set.contains(&pid);
        assert_eq!(
            is_leo_bodnar_ffb_pid(pid),
            expected,
            "FFB status wrong for PID 0x{pid:04X}"
        );
    }
}

#[test]
fn ffb_devices_report_supports_ffb() -> Result<(), String> {
    let dev_wheel = LeoBodnarDevice::from_product_id(PID_WHEEL_INTERFACE)
        .ok_or("wheel should resolve")?;
    let dev_ffb = LeoBodnarDevice::from_product_id(PID_FFB_JOYSTICK)
        .ok_or("ffb joystick should resolve")?;
    assert!(dev_wheel.supports_ffb());
    assert!(dev_ffb.supports_ffb());
    Ok(())
}

#[test]
fn non_ffb_devices_do_not_report_supports_ffb() -> Result<(), String> {
    let non_ffb_pids = [PID_USB_JOYSTICK, PID_BBI32, PID_SLI_M, PID_BU0836A];
    for pid in non_ffb_pids {
        let dev = LeoBodnarDevice::from_product_id(pid)
            .ok_or_else(|| format!("PID 0x{pid:04X} should resolve"))?;
        assert!(!dev.supports_ffb(), "{dev:?} should not support FFB");
    }
    Ok(())
}

// ── Input channel counts ─────────────────────────────────────────────────────

#[test]
fn joystick_interfaces_have_32_buttons() -> Result<(), String> {
    let pids = [
        PID_USB_JOYSTICK,
        PID_BU0836A,
        PID_BU0836X,
        PID_BU0836_16BIT,
        PID_BBI32,
        PID_WHEEL_INTERFACE,
        PID_FFB_JOYSTICK,
    ];
    for pid in pids {
        let dev = LeoBodnarDevice::from_product_id(pid)
            .ok_or_else(|| format!("PID 0x{pid:04X} should resolve"))?;
        assert_eq!(
            dev.max_input_channels(),
            32,
            "{dev:?} should have 32 channels"
        );
    }
    Ok(())
}

#[test]
fn output_only_device_has_zero_channels() {
    assert_eq!(LeoBodnarDevice::SlimShiftLight.max_input_channels(), 0);
}

#[test]
fn pedal_devices_have_zero_buttons() {
    assert_eq!(LeoBodnarDevice::Pedals.max_input_channels(), 0);
    assert_eq!(LeoBodnarDevice::LcPedals.max_input_channels(), 0);
}

// ── Device names ─────────────────────────────────────────────────────────────

#[test]
fn every_device_variant_has_nonempty_name() {
    let all = [
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
    for dev in all {
        assert!(!dev.name().is_empty(), "{dev:?} name must not be empty");
    }
}

#[test]
fn device_names_contain_leo_bodnar() {
    let all = [
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
    for dev in all {
        assert!(
            dev.name().contains("Leo Bodnar"),
            "{dev:?} name '{}' must contain 'Leo Bodnar'",
            dev.name()
        );
    }
}

// ── Report constants ─────────────────────────────────────────────────────────

#[test]
fn max_report_bytes_fits_usb_full_speed() {
    assert_eq!(MAX_REPORT_BYTES, 64);
}

#[test]
fn hid_pid_usage_page_is_standard() {
    assert_eq!(HID_PID_USAGE_PAGE, 0x000F);
}

#[test]
fn wheel_encoder_cpr_is_full_16bit_range() {
    assert_eq!(WHEEL_ENCODER_CPR, 65_535);
}

#[test]
fn wheel_default_torque_is_positive_and_reasonable() {
    let torque = WHEEL_DEFAULT_MAX_TORQUE_NM;
    assert!(torque > 0.0, "torque must be positive, got {torque}");
    assert!(torque <= 50.0, "torque must be reasonable, got {torque}");
}

// ── Cross-consistency checks ─────────────────────────────────────────────────

#[test]
fn every_recognised_pid_has_a_device_variant() {
    let all_pids = [
        PID_USB_JOYSTICK,
        PID_BBI32,
        PID_WHEEL_INTERFACE,
        PID_FFB_JOYSTICK,
        PID_SLI_M,
        PID_BU0836A,
        PID_BU0836X,
        PID_BU0836_16BIT,
    ];
    for pid in all_pids {
        assert!(
            is_leo_bodnar_device(pid),
            "PID 0x{pid:04X} must be recognised"
        );
        assert!(
            LeoBodnarDevice::from_product_id(pid).is_some(),
            "PID 0x{pid:04X} must map to a variant"
        );
    }
}

#[test]
fn device_variant_round_trips_through_product_id() -> Result<(), String> {
    let table: &[(u16, LeoBodnarDevice)] = &[
        (PID_USB_JOYSTICK, LeoBodnarDevice::UsbJoystick),
        (PID_BBI32, LeoBodnarDevice::Bbi32),
        (PID_WHEEL_INTERFACE, LeoBodnarDevice::WheelInterface),
        (PID_FFB_JOYSTICK, LeoBodnarDevice::FfbJoystick),
        (PID_SLI_M, LeoBodnarDevice::SlimShiftLight),
        (PID_BU0836A, LeoBodnarDevice::Bu0836a),
        (PID_BU0836X, LeoBodnarDevice::Bu0836x),
        (PID_BU0836_16BIT, LeoBodnarDevice::Bu0836_16bit),
    ];
    for &(pid, expected) in table {
        let resolved = LeoBodnarDevice::from_product_id(pid)
            .ok_or_else(|| format!("PID 0x{pid:04X} should resolve"))?;
        assert_eq!(resolved, expected);
    }
    Ok(())
}
