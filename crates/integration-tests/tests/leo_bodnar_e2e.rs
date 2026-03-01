//! BDD end-to-end tests for the Leo Bodnar HID protocol crate.
//!
//! Each test follows a Given/When/Then pattern to verify device classification,
//! FFB capability detection, product ID resolution, and report constants
//! without real USB hardware.

use racing_wheel_hid_leo_bodnar_protocol::{
    LeoBodnarDevice, PID_BBI32, PID_BU0836A, PID_BU0836X, PID_BU0836_16BIT, PID_FFB_JOYSTICK,
    PID_SLI_M, PID_USB_JOYSTICK, PID_WHEEL_INTERFACE, VENDOR_ID, is_leo_bodnar,
    is_leo_bodnar_device, is_leo_bodnar_ffb_pid, HID_PID_USAGE_PAGE, MAX_REPORT_BYTES,
    WHEEL_DEFAULT_MAX_TORQUE_NM, WHEEL_ENCODER_CPR,
};

/// All eight known Leo Bodnar product IDs.
const ALL_PIDS: &[u16] = &[
    PID_USB_JOYSTICK,
    PID_BU0836A,
    PID_BBI32,
    PID_WHEEL_INTERFACE,
    PID_FFB_JOYSTICK,
    PID_BU0836X,
    PID_BU0836_16BIT,
    PID_SLI_M,
];

// ─── Scenario 1: Vendor ID constant is correct ───────────────────────────────

#[test]
fn scenario_vendor_id_given_constant_when_checked_then_matches_leo_bodnar() {
    // Given: the VENDOR_ID constant
    // When: compared to the known Leo Bodnar VID
    // Then: it equals 0x1DD2
    assert_eq!(VENDOR_ID, 0x1DD2, "Leo Bodnar VID must be 0x1DD2");
}

// ─── Scenario 2: is_leo_bodnar recognises all known VID/PID pairs ────────────

#[test]
fn scenario_device_classification_given_valid_vid_pid_when_checked_then_recognised() {
    // Given: Leo Bodnar VID and all known PIDs
    for &pid in ALL_PIDS {
        // When: is_leo_bodnar is called with the correct VID
        let result = is_leo_bodnar(VENDOR_ID, pid);

        // Then: it returns true
        assert!(
            result,
            "is_leo_bodnar(0x{:04X}, 0x{:04X}) must return true",
            VENDOR_ID, pid
        );
    }
}

// ─── Scenario 3: is_leo_bodnar rejects wrong vendor ID ──────────────────────

#[test]
fn scenario_device_classification_given_wrong_vid_when_checked_then_rejected() {
    // Given: a non-Leo-Bodnar VID paired with a valid PID
    let wrong_vids: &[u16] = &[0x0000, 0x16D0, 0x0483, 0xFFFF];
    for &vid in wrong_vids {
        for &pid in ALL_PIDS {
            // When: is_leo_bodnar is called
            let result = is_leo_bodnar(vid, pid);

            // Then: it returns false
            assert!(
                !result,
                "is_leo_bodnar(0x{vid:04X}, 0x{pid:04X}) must return false for wrong VID"
            );
        }
    }
}

// ─── Scenario 4: is_leo_bodnar_device recognises all known PIDs ──────────────

#[test]
fn scenario_device_pid_check_given_known_pid_when_checked_then_recognised() {
    // Given: all known Leo Bodnar PIDs
    for &pid in ALL_PIDS {
        // When: is_leo_bodnar_device is called
        let result = is_leo_bodnar_device(pid);

        // Then: it returns true
        assert!(
            result,
            "is_leo_bodnar_device(0x{pid:04X}) must return true"
        );
    }
}

// ─── Scenario 5: is_leo_bodnar_device rejects unknown PIDs ──────────────────

#[test]
fn scenario_device_pid_check_given_unknown_pid_when_checked_then_rejected() {
    // Given: PIDs not belonging to Leo Bodnar
    let unknown_pids: &[u16] = &[0x0000, 0x0002, 0x0003, 0x0100, 0xDEAD, 0xFFFF];
    for &pid in unknown_pids {
        // When: is_leo_bodnar_device is called
        let result = is_leo_bodnar_device(pid);

        // Then: it returns false
        assert!(
            !result,
            "is_leo_bodnar_device(0x{pid:04X}) must return false for unknown PID"
        );
    }
}

// ─── Scenario 6: FFB PIDs identified correctly ──────────────────────────────

#[test]
fn scenario_ffb_detection_given_ffb_pid_when_checked_then_returns_true() {
    // Given: PIDs for FFB-capable devices
    let ffb_pids = [PID_WHEEL_INTERFACE, PID_FFB_JOYSTICK];

    for &pid in &ffb_pids {
        // When: is_leo_bodnar_ffb_pid is called
        let result = is_leo_bodnar_ffb_pid(pid);

        // Then: it returns true
        assert!(
            result,
            "is_leo_bodnar_ffb_pid(0x{pid:04X}) must return true"
        );
    }
}

// ─── Scenario 7: Non-FFB PIDs correctly rejected ────────────────────────────

#[test]
fn scenario_ffb_detection_given_non_ffb_pid_when_checked_then_returns_false() {
    // Given: PIDs for non-FFB devices
    let non_ffb_pids = [
        PID_USB_JOYSTICK,
        PID_BU0836A,
        PID_BBI32,
        PID_BU0836X,
        PID_BU0836_16BIT,
        PID_SLI_M,
    ];

    for &pid in &non_ffb_pids {
        // When: is_leo_bodnar_ffb_pid is called
        let result = is_leo_bodnar_ffb_pid(pid);

        // Then: it returns false
        assert!(
            !result,
            "is_leo_bodnar_ffb_pid(0x{pid:04X}) must return false for non-FFB device"
        );
    }
}

// ─── Scenario 8: from_product_id resolves all 8 variants ────────────────────

#[test]
fn scenario_product_id_resolution_given_known_pid_when_resolved_then_returns_correct_variant(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: all known PID-to-variant mappings
    let expected: &[(u16, LeoBodnarDevice)] = &[
        (PID_USB_JOYSTICK, LeoBodnarDevice::UsbJoystick),
        (PID_BU0836A, LeoBodnarDevice::Bu0836a),
        (PID_BBI32, LeoBodnarDevice::Bbi32),
        (PID_WHEEL_INTERFACE, LeoBodnarDevice::WheelInterface),
        (PID_FFB_JOYSTICK, LeoBodnarDevice::FfbJoystick),
        (PID_BU0836X, LeoBodnarDevice::Bu0836x),
        (PID_BU0836_16BIT, LeoBodnarDevice::Bu0836_16bit),
        (PID_SLI_M, LeoBodnarDevice::SlimShiftLight),
    ];

    for &(pid, ref variant) in expected {
        // When: from_product_id is called
        let device = LeoBodnarDevice::from_product_id(pid);

        // Then: it returns the expected variant
        assert_eq!(
            device.as_ref(),
            Some(variant),
            "PID 0x{pid:04X} must resolve to {variant:?}"
        );
    }

    // Then: we covered all 8 variants
    assert_eq!(expected.len(), 8, "must test all 8 device variants");

    Ok(())
}

// ─── Scenario 9: Unknown product ID returns None ────────────────────────────

#[test]
fn scenario_product_id_resolution_given_unknown_pid_when_resolved_then_returns_none() {
    // Given: PIDs that are not Leo Bodnar products
    let unknown_pids: &[u16] = &[0x0000, 0x0002, 0x0010, 0x0100, 0x1234, 0xDEAD, 0xFFFF];

    for &pid in unknown_pids {
        // When: from_product_id is called
        let result = LeoBodnarDevice::from_product_id(pid);

        // Then: it returns None
        assert_eq!(
            result, None,
            "PID 0x{pid:04X} must return None for unknown product"
        );
    }
}

// ─── Scenario 10: max_input_channels for joystick/button devices ────────────

#[test]
fn scenario_input_channels_given_joystick_device_when_queried_then_returns_32() {
    // Given: all devices that are joystick or button-box interfaces
    let joystick_devices = [
        LeoBodnarDevice::UsbJoystick,
        LeoBodnarDevice::Bu0836a,
        LeoBodnarDevice::Bbi32,
        LeoBodnarDevice::WheelInterface,
        LeoBodnarDevice::FfbJoystick,
        LeoBodnarDevice::Bu0836x,
        LeoBodnarDevice::Bu0836_16bit,
    ];

    for device in &joystick_devices {
        // When: max_input_channels is queried
        let channels = device.max_input_channels();

        // Then: it returns 32
        assert_eq!(
            channels, 32,
            "{:?} must report 32 input channels",
            device
        );
    }
}

// ─── Scenario 11: SLI-M has zero input channels (output-only) ───────────────

#[test]
fn scenario_input_channels_given_sli_device_when_queried_then_returns_zero() {
    // Given: the SLI-M shift light indicator (output-only)
    let device = LeoBodnarDevice::SlimShiftLight;

    // When: max_input_channels is queried
    let channels = device.max_input_channels();

    // Then: it returns 0
    assert_eq!(channels, 0, "SLI-M must report 0 input channels");
}

// ─── Scenario 12: supports_ffb for FFB devices ─────────────────────────────

#[test]
fn scenario_ffb_support_given_ffb_device_when_queried_then_returns_true() {
    // Given: devices that support force feedback
    let ffb_devices = [
        LeoBodnarDevice::WheelInterface,
        LeoBodnarDevice::FfbJoystick,
    ];

    for device in &ffb_devices {
        // When: supports_ffb is queried
        let result = device.supports_ffb();

        // Then: it returns true
        assert!(result, "{:?} must support FFB", device);
    }
}

// ─── Scenario 13: supports_ffb for non-FFB devices ─────────────────────────

#[test]
fn scenario_ffb_support_given_non_ffb_device_when_queried_then_returns_false() {
    // Given: devices that do not support force feedback
    let non_ffb_devices = [
        LeoBodnarDevice::UsbJoystick,
        LeoBodnarDevice::Bu0836a,
        LeoBodnarDevice::Bbi32,
        LeoBodnarDevice::Bu0836x,
        LeoBodnarDevice::Bu0836_16bit,
        LeoBodnarDevice::SlimShiftLight,
    ];

    for device in &non_ffb_devices {
        // When: supports_ffb is queried
        let result = device.supports_ffb();

        // Then: it returns false
        assert!(!result, "{:?} must not support FFB", device);
    }
}

// ─── Scenario 14: Device name strings are correct ───────────────────────────

#[test]
fn scenario_device_name_given_each_variant_when_queried_then_returns_expected_string() {
    // Given: all device variants and their expected human-readable names
    let expected_names: &[(LeoBodnarDevice, &str)] = &[
        (LeoBodnarDevice::UsbJoystick, "Leo Bodnar USB Joystick"),
        (LeoBodnarDevice::Bu0836a, "Leo Bodnar BU0836A"),
        (LeoBodnarDevice::Bbi32, "Leo Bodnar BBI-32"),
        (
            LeoBodnarDevice::WheelInterface,
            "Leo Bodnar USB Sim Racing Wheel Interface",
        ),
        (LeoBodnarDevice::FfbJoystick, "Leo Bodnar FFB Joystick"),
        (LeoBodnarDevice::Bu0836x, "Leo Bodnar BU0836X"),
        (LeoBodnarDevice::Bu0836_16bit, "Leo Bodnar BU0836 16-bit"),
        (LeoBodnarDevice::SlimShiftLight, "Leo Bodnar SLI-Pro"),
    ];

    for &(ref device, expected_name) in expected_names {
        // When: name() is called
        let name = device.name();

        // Then: it matches the expected string
        assert_eq!(
            name, expected_name,
            "{:?} name must be {:?}",
            device, expected_name
        );
    }
}

// ─── Scenario 15: All device names start with "Leo Bodnar" ──────────────────

#[test]
fn scenario_device_name_given_any_variant_when_queried_then_starts_with_leo_bodnar() {
    // Given: all known PIDs
    for &pid in ALL_PIDS {
        // When: the device is resolved and name() is called
        let device = LeoBodnarDevice::from_product_id(pid);
        assert!(device.is_some(), "PID 0x{pid:04X} must resolve");

        if let Some(dev) = device {
            let name = dev.name();

            // Then: the name starts with "Leo Bodnar"
            assert!(
                name.starts_with("Leo Bodnar"),
                "{:?} name {:?} must start with \"Leo Bodnar\"",
                dev,
                name
            );
        }
    }
}

// ─── Scenario 16: Report constants have valid values ────────────────────────

#[test]
fn scenario_report_constants_given_protocol_limits_when_checked_then_within_spec() {
    // Given: USB full-speed HID limits
    // Then: MAX_REPORT_BYTES is within the 64-byte USB full-speed limit
    assert!(
        MAX_REPORT_BYTES <= 64,
        "MAX_REPORT_BYTES must be ≤ 64 for USB full-speed"
    );
    assert!(MAX_REPORT_BYTES > 0, "MAX_REPORT_BYTES must be positive");

    // Then: HID PID usage page is the standard 0x000F
    assert_eq!(
        HID_PID_USAGE_PAGE, 0x000F,
        "HID PID usage page must be 0x000F"
    );

    // Then: encoder CPR is 16-bit range
    assert_eq!(
        WHEEL_ENCODER_CPR, 65_535,
        "encoder CPR must be 65535 (16-bit range)"
    );

    // Then: default max torque is positive and reasonable
    assert!(
        WHEEL_DEFAULT_MAX_TORQUE_NM > 0.0,
        "default max torque must be positive"
    );
    assert!(
        (WHEEL_DEFAULT_MAX_TORQUE_NM - 10.0).abs() < f32::EPSILON,
        "default max torque must be 10.0 Nm"
    );
}

// ─── Scenario 17: FFB support is consistent between device and PID check ────

#[test]
fn scenario_ffb_consistency_given_any_known_pid_when_both_apis_checked_then_agree(
) -> Result<(), Box<dyn std::error::Error>> {
    // Given: all known PIDs
    for &pid in ALL_PIDS {
        let device = LeoBodnarDevice::from_product_id(pid);
        assert!(device.is_some(), "PID 0x{pid:04X} must resolve");

        if let Some(dev) = device {
            // When: both supports_ffb() and is_leo_bodnar_ffb_pid() are checked
            let device_ffb = dev.supports_ffb();
            let pid_ffb = is_leo_bodnar_ffb_pid(pid);

            // Then: they agree
            assert_eq!(
                device_ffb, pid_ffb,
                "{:?} (PID 0x{:04X}): supports_ffb()={} but is_leo_bodnar_ffb_pid()={}",
                dev, pid, device_ffb, pid_ffb
            );
        }
    }

    Ok(())
}

// ─── Scenario 18: is_leo_bodnar is consistent with is_leo_bodnar_device ─────

#[test]
fn scenario_classification_consistency_given_correct_vid_when_both_apis_checked_then_agree() {
    // Given: all known PIDs with the correct VID
    for &pid in ALL_PIDS {
        // When: both classification functions are called
        let full_check = is_leo_bodnar(VENDOR_ID, pid);
        let pid_only = is_leo_bodnar_device(pid);

        // Then: they agree (since VID is correct)
        assert_eq!(
            full_check, pid_only,
            "is_leo_bodnar and is_leo_bodnar_device must agree for PID 0x{pid:04X} with correct VID"
        );
    }
}

// ─── Scenario 19: Unknown PID yields no FFB ────────────────────────────────

#[test]
fn scenario_ffb_detection_given_unknown_pid_when_checked_then_returns_false() {
    // Given: PIDs not belonging to any Leo Bodnar device
    let unknown_pids: &[u16] = &[0x0000, 0x0002, 0x0100, 0xDEAD, 0xFFFF];

    for &pid in unknown_pids {
        // When: is_leo_bodnar_ffb_pid is called
        let result = is_leo_bodnar_ffb_pid(pid);

        // Then: it returns false
        assert!(
            !result,
            "is_leo_bodnar_ffb_pid(0x{pid:04X}) must return false for unknown PID"
        );
    }
}

// ─── Scenario 20: Exact PID constant values match spec ──────────────────────

#[test]
fn scenario_pid_constants_given_usb_spec_when_checked_then_match_expected_values() {
    // Given: known USB product IDs from the Leo Bodnar catalogue
    // Then: each constant matches its documented hex value
    assert_eq!(PID_USB_JOYSTICK, 0x0001, "USB Joystick PID");
    assert_eq!(PID_BU0836A, 0x000B, "BU0836A PID");
    assert_eq!(PID_BBI32, 0x000C, "BBI-32 PID");
    assert_eq!(PID_WHEEL_INTERFACE, 0x000E, "Wheel Interface PID");
    assert_eq!(PID_FFB_JOYSTICK, 0x000F, "FFB Joystick PID");
    assert_eq!(PID_BU0836X, 0x0030, "BU0836X PID");
    assert_eq!(PID_BU0836_16BIT, 0x0031, "BU0836 16-bit PID");
    assert_eq!(PID_SLI_M, 0x1301, "SLI-M PID");
}
