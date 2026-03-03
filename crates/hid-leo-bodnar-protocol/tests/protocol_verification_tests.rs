//! Protocol verification tests for the Leo Bodnar HID protocol implementation.
//!
//! These tests cross-reference our constants against independent public sources
//! to verify that VID/PID values, report format constants, and device metadata
//! are accurate.
//!
//! ## Sources cited
//!
//! | # | Source | What it confirms |
//! |---|--------|------------------|
//! | 1 | the-sz.com USB vendor database | VID `0x1DD2` = "LEO BODNAR" |
//! | 2 | devicehunt.com vendor database | VID `0x1DD2` = "Leo Bodnar Electronics Ltd" |
//! | 3 | JacKeTUs/simracing-hwdb `90-leo-bodnar.hwdb` | Pedals `v1DD2p100C`, LC Pedals `v1DD2p22D0` |
//! | 4 | USB HID PID specification (`pid1_01.pdf`) | Usage Page `0x000F` = Physical Interface Device |
//! | 5 | USB 2.0 spec §5.7.3 (Full-Speed Interrupt) | Max interrupt transfer payload = 64 bytes |

use racing_wheel_hid_leo_bodnar_protocol::{
    HID_PID_USAGE_PAGE, LeoBodnarDevice, MAX_REPORT_BYTES, PID_BBI32, PID_BU0836A,
    PID_BU0836X, PID_BU0836_16BIT, PID_FFB_JOYSTICK, PID_SLI_M, PID_USB_JOYSTICK,
    PID_WHEEL_INTERFACE, VENDOR_ID, WHEEL_DEFAULT_MAX_TORQUE_NM, WHEEL_ENCODER_CPR,
    is_leo_bodnar, is_leo_bodnar_device, is_leo_bodnar_ffb_pid,
};

// Pedal PIDs (from ids.rs)
use racing_wheel_hid_leo_bodnar_protocol::ids::{PID_PEDALS, PID_LC_PEDALS};

// ════════════════════════════════════════════════════════════════════════════
// § 1. VID verification against USB vendor databases
// ════════════════════════════════════════════════════════════════════════════

/// VID `0x1DD2` = Leo Bodnar Electronics Ltd.
/// Source [1]: the-sz.com → "LEO BODNAR"
/// Source [2]: devicehunt.com → "Leo Bodnar Electronics Ltd"
#[test]
fn vid_matches_usb_vendor_databases() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        VENDOR_ID, 0x1DD2,
        "Leo Bodnar VID must be 0x1DD2 (USB-IF registered)"
    );
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 2. Confirmed PID verification
// ════════════════════════════════════════════════════════════════════════════

/// USB Joystick PID `0x0001`.
#[test]
fn usb_joystick_pid() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(PID_USB_JOYSTICK, 0x0001);
    Ok(())
}

/// BBI-32 Button Box PID `0x000C`.
#[test]
fn bbi32_pid() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(PID_BBI32, 0x000C);
    Ok(())
}

/// USB Sim Racing Wheel Interface PID `0x000E` (HID PID FFB).
#[test]
fn wheel_interface_pid() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(PID_WHEEL_INTERFACE, 0x000E);
    Ok(())
}

/// FFB Joystick PID `0x000F`.
#[test]
fn ffb_joystick_pid() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(PID_FFB_JOYSTICK, 0x000F);
    Ok(())
}

/// SLI-Pro (SLI-M) PID `0x1301` (community estimate).
#[test]
fn sli_m_pid() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(PID_SLI_M, 0x1301);
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 3. Community-confirmed PIDs (simracing-hwdb)
// ════════════════════════════════════════════════════════════════════════════

/// Pedals PID `0x100C` — confirmed via simracing-hwdb.
/// Source [3]: `v1DD2p100C` labeled "Leo Bodnar pedals controller"
#[test]
fn pedals_pid_matches_hwdb() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(PID_PEDALS, 0x100C, "Pedals PID must be 0x100C");
    Ok(())
}

/// LC Pedals PID `0x22D0` — confirmed via simracing-hwdb.
/// Source [3]: `v1DD2p22D0` labeled "Leo Bodnar LC pedals controller"
#[test]
fn lc_pedals_pid_matches_hwdb() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(PID_LC_PEDALS, 0x22D0, "LC Pedals PID must be 0x22D0");
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 4. Estimated PIDs (community USB captures)
// ════════════════════════════════════════════════════════════════════════════

/// BU0836A PID `0x000B` (estimated from community reports).
#[test]
fn bu0836a_pid() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(PID_BU0836A, 0x000B);
    Ok(())
}

/// BU0836X PID `0x0030` (estimated from community reports).
#[test]
fn bu0836x_pid() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(PID_BU0836X, 0x0030);
    Ok(())
}

/// BU0836 16-bit PID `0x0031` (estimated from community reports).
#[test]
fn bu0836_16bit_pid() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(PID_BU0836_16BIT, 0x0031);
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 5. Device identification functions
// ════════════════════════════════════════════════════════════════════════════

/// All known PIDs must be recognised by `is_leo_bodnar()` with correct VID.
#[test]
fn all_pids_recognised_with_correct_vid() -> Result<(), Box<dyn std::error::Error>> {
    let all_pids = [
        PID_USB_JOYSTICK, PID_BBI32, PID_WHEEL_INTERFACE, PID_FFB_JOYSTICK,
        PID_SLI_M, PID_BU0836A, PID_BU0836X, PID_BU0836_16BIT,
        PID_PEDALS, PID_LC_PEDALS,
    ];
    for &pid in &all_pids {
        assert!(
            is_leo_bodnar(VENDOR_ID, pid),
            "PID 0x{pid:04X} must be recognised with VID 0x1DD2"
        );
        assert!(
            is_leo_bodnar_device(pid),
            "PID 0x{pid:04X} must be recognised by is_leo_bodnar_device"
        );
    }
    Ok(())
}

/// Wrong VID must be rejected even with correct PID.
#[test]
fn wrong_vid_rejected() -> Result<(), Box<dyn std::error::Error>> {
    assert!(
        !is_leo_bodnar(0x0000, PID_WHEEL_INTERFACE),
        "VID 0x0000 must be rejected"
    );
    assert!(
        !is_leo_bodnar(0x16D0, PID_WHEEL_INTERFACE),
        "Simucube VID must be rejected"
    );
    assert!(
        !is_leo_bodnar(0x1FC9, PID_WHEEL_INTERFACE),
        "AccuForce VID must be rejected"
    );
    Ok(())
}

/// Unknown PIDs must not be recognised.
#[test]
fn unknown_pids_rejected() -> Result<(), Box<dyn std::error::Error>> {
    assert!(!is_leo_bodnar(VENDOR_ID, 0x0000));
    assert!(!is_leo_bodnar(VENDOR_ID, 0xFFFF));
    assert!(!is_leo_bodnar_device(0x0000));
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 6. FFB capability identification
// ════════════════════════════════════════════════════════════════════════════

/// Only Wheel Interface (0x000E) and FFB Joystick (0x000F) support FFB.
#[test]
fn ffb_pids_correct() -> Result<(), Box<dyn std::error::Error>> {
    assert!(is_leo_bodnar_ffb_pid(PID_WHEEL_INTERFACE));
    assert!(is_leo_bodnar_ffb_pid(PID_FFB_JOYSTICK));

    // Non-FFB devices
    assert!(!is_leo_bodnar_ffb_pid(PID_USB_JOYSTICK));
    assert!(!is_leo_bodnar_ffb_pid(PID_BBI32));
    assert!(!is_leo_bodnar_ffb_pid(PID_SLI_M));
    assert!(!is_leo_bodnar_ffb_pid(PID_BU0836A));
    assert!(!is_leo_bodnar_ffb_pid(PID_PEDALS));
    assert!(!is_leo_bodnar_ffb_pid(PID_LC_PEDALS));
    Ok(())
}

/// `LeoBodnarDevice::supports_ffb()` must agree with `is_leo_bodnar_ffb_pid()`.
#[test]
fn device_ffb_support_consistent() -> Result<(), Box<dyn std::error::Error>> {
    let all_pids = [
        PID_USB_JOYSTICK, PID_BBI32, PID_WHEEL_INTERFACE, PID_FFB_JOYSTICK,
        PID_SLI_M, PID_BU0836A, PID_BU0836X, PID_BU0836_16BIT,
        PID_PEDALS, PID_LC_PEDALS,
    ];
    for &pid in &all_pids {
        if let Some(device) = LeoBodnarDevice::from_product_id(pid) {
            assert_eq!(
                device.supports_ffb(),
                is_leo_bodnar_ffb_pid(pid),
                "supports_ffb and is_leo_bodnar_ffb_pid must agree for PID 0x{pid:04X}"
            );
        }
    }
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 7. Report format constants
// ════════════════════════════════════════════════════════════════════════════

/// Full-speed USB HID max payload is 64 bytes.
/// Source [5]: USB 2.0 spec §5.7.3
#[test]
fn max_report_bytes_is_64() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(MAX_REPORT_BYTES, 64);
    Ok(())
}

/// HID PID usage page is 0x000F.
/// Source [4]: USB HID PID spec
#[test]
fn hid_pid_usage_page() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(HID_PID_USAGE_PAGE, 0x000F);
    Ok(())
}

/// Wheel encoder CPR is 65535 (16-bit range).
#[test]
fn wheel_encoder_cpr_is_16bit() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(WHEEL_ENCODER_CPR, 65_535);
    Ok(())
}

/// Default max torque is 10 Nm (conservative user-overridable value).
#[test]
fn default_max_torque_is_10nm() -> Result<(), Box<dyn std::error::Error>> {
    assert!(
        (WHEEL_DEFAULT_MAX_TORQUE_NM - 10.0).abs() < f32::EPSILON,
        "default max torque must be 10.0 Nm"
    );
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 8. Device model resolution and metadata
// ════════════════════════════════════════════════════════════════════════════

/// All known PIDs must resolve to a `LeoBodnarDevice` variant.
#[test]
fn all_pids_resolve_to_device() -> Result<(), Box<dyn std::error::Error>> {
    let all_pids = [
        PID_USB_JOYSTICK, PID_BBI32, PID_WHEEL_INTERFACE, PID_FFB_JOYSTICK,
        PID_SLI_M, PID_BU0836A, PID_BU0836X, PID_BU0836_16BIT,
        PID_PEDALS, PID_LC_PEDALS,
    ];
    for &pid in &all_pids {
        assert!(
            LeoBodnarDevice::from_product_id(pid).is_some(),
            "PID 0x{pid:04X} must resolve to a LeoBodnarDevice variant"
        );
    }
    Ok(())
}

/// Button box devices must report 32 input channels.
#[test]
fn button_boxes_have_32_channels() -> Result<(), Box<dyn std::error::Error>> {
    let button_box_pids = [PID_BBI32, PID_BU0836A, PID_BU0836X, PID_BU0836_16BIT];
    for &pid in &button_box_pids {
        if let Some(device) = LeoBodnarDevice::from_product_id(pid) {
            assert_eq!(
                device.max_input_channels(),
                32,
                "{device:?} must report 32 input channels"
            );
        }
    }
    Ok(())
}

/// SLI-M (output/display device) must report 0 input channels.
#[test]
fn sli_m_has_zero_input_channels() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        LeoBodnarDevice::SlimShiftLight.max_input_channels(),
        0,
        "SLI-M must report 0 input channels"
    );
    Ok(())
}

/// All device names must be non-empty and mention "Leo Bodnar".
#[test]
fn device_names_are_descriptive() -> Result<(), Box<dyn std::error::Error>> {
    let all_pids = [
        PID_USB_JOYSTICK, PID_BBI32, PID_WHEEL_INTERFACE, PID_FFB_JOYSTICK,
        PID_SLI_M, PID_BU0836A, PID_BU0836X, PID_BU0836_16BIT,
        PID_PEDALS, PID_LC_PEDALS,
    ];
    for &pid in &all_pids {
        if let Some(device) = LeoBodnarDevice::from_product_id(pid) {
            let name = device.name();
            assert!(
                name.contains("Leo Bodnar"),
                "device name for PID 0x{pid:04X} must mention 'Leo Bodnar', got: {name}"
            );
        }
    }
    Ok(())
}
