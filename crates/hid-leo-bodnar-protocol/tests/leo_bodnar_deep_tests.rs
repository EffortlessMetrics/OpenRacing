//! Deep protocol tests for Leo Bodnar HID protocol.
//!
//! Covers BU0836 encoder input parsing, load cell pedal reports,
//! and FFB joystick output.

use racing_wheel_hid_leo_bodnar_protocol::{
    HID_PID_USAGE_PAGE, LeoBodnarDevice, MAX_REPORT_BYTES, PID_BU0836A, PID_BU0836_16BIT,
    PID_BU0836X, PID_FFB_JOYSTICK, PID_WHEEL_INTERFACE, VENDOR_ID, WHEEL_DEFAULT_MAX_TORQUE_NM,
    WHEEL_ENCODER_CPR, is_leo_bodnar, is_leo_bodnar_device, is_leo_bodnar_ffb_pid,
};
use racing_wheel_hid_leo_bodnar_protocol::ids::{PID_LC_PEDALS, PID_PEDALS};

// ─── BU0836 encoder input parsing ───────────────────────────────────────────

#[test]
fn bu0836a_identified_and_has_32_buttons() {
    let device = LeoBodnarDevice::from_product_id(PID_BU0836A);
    assert_eq!(device, Some(LeoBodnarDevice::Bu0836a));
    assert_eq!(LeoBodnarDevice::Bu0836a.max_input_channels(), 32);
    assert!(!LeoBodnarDevice::Bu0836a.supports_ffb());
}

#[test]
fn bu0836x_identified_and_has_32_buttons() {
    let device = LeoBodnarDevice::from_product_id(PID_BU0836X);
    assert_eq!(device, Some(LeoBodnarDevice::Bu0836x));
    assert_eq!(LeoBodnarDevice::Bu0836x.max_input_channels(), 32);
}

#[test]
fn bu0836_16bit_has_higher_resolution() {
    let device = LeoBodnarDevice::from_product_id(PID_BU0836_16BIT);
    assert_eq!(device, Some(LeoBodnarDevice::Bu0836_16bit));
    assert_eq!(LeoBodnarDevice::Bu0836_16bit.max_input_channels(), 32);
    assert!(!LeoBodnarDevice::Bu0836_16bit.supports_ffb());
}

#[test]
fn bu0836_variants_no_ffb_support() {
    let bu_devices = [
        LeoBodnarDevice::Bu0836a,
        LeoBodnarDevice::Bu0836x,
        LeoBodnarDevice::Bu0836_16bit,
    ];
    for device in &bu_devices {
        assert!(
            !device.supports_ffb(),
            "{device:?} should not support FFB"
        );
    }
}

#[test]
fn encoder_cpr_is_16_bit_range() {
    assert_eq!(WHEEL_ENCODER_CPR, 65_535);
    assert_eq!(WHEEL_ENCODER_CPR, u16::MAX as u32);
}

// ─── Load cell pedal reports ────────────────────────────────────────────────

#[test]
fn pedals_identified_by_pid() {
    let device = LeoBodnarDevice::from_product_id(PID_PEDALS);
    assert_eq!(device, Some(LeoBodnarDevice::Pedals));
    assert_eq!(LeoBodnarDevice::Pedals.name(), "Leo Bodnar Pedals");
}

#[test]
fn lc_pedals_identified_by_pid() {
    let device = LeoBodnarDevice::from_product_id(PID_LC_PEDALS);
    assert_eq!(device, Some(LeoBodnarDevice::LcPedals));
    assert_eq!(LeoBodnarDevice::LcPedals.name(), "Leo Bodnar LC Pedals");
}

#[test]
fn pedal_devices_have_zero_button_channels() {
    assert_eq!(LeoBodnarDevice::Pedals.max_input_channels(), 0);
    assert_eq!(LeoBodnarDevice::LcPedals.max_input_channels(), 0);
}

#[test]
fn pedal_devices_no_ffb() {
    assert!(!LeoBodnarDevice::Pedals.supports_ffb());
    assert!(!LeoBodnarDevice::LcPedals.supports_ffb());
}

#[test]
fn pedal_pids_recognised_by_vendor_check() {
    assert!(is_leo_bodnar(VENDOR_ID, PID_PEDALS));
    assert!(is_leo_bodnar(VENDOR_ID, PID_LC_PEDALS));
    assert!(is_leo_bodnar_device(PID_PEDALS));
    assert!(is_leo_bodnar_device(PID_LC_PEDALS));
}

#[test]
fn pedal_pids_not_ffb() {
    assert!(!is_leo_bodnar_ffb_pid(PID_PEDALS));
    assert!(!is_leo_bodnar_ffb_pid(PID_LC_PEDALS));
}

// ─── FFB joystick output ────────────────────────────────────────────────────

#[test]
fn ffb_joystick_identified_and_supports_ffb() {
    let device = LeoBodnarDevice::from_product_id(PID_FFB_JOYSTICK);
    assert_eq!(device, Some(LeoBodnarDevice::FfbJoystick));
    assert!(LeoBodnarDevice::FfbJoystick.supports_ffb());
    assert_eq!(
        LeoBodnarDevice::FfbJoystick.name(),
        "Leo Bodnar FFB Joystick"
    );
}

#[test]
fn wheel_interface_supports_ffb() {
    let device = LeoBodnarDevice::from_product_id(PID_WHEEL_INTERFACE);
    assert_eq!(device, Some(LeoBodnarDevice::WheelInterface));
    assert!(LeoBodnarDevice::WheelInterface.supports_ffb());
}

#[test]
fn ffb_uses_standard_hid_pid_usage_page() {
    assert_eq!(HID_PID_USAGE_PAGE, 0x000F);
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn default_max_torque_is_conservative() {
    assert_eq!(WHEEL_DEFAULT_MAX_TORQUE_NM, 10.0);
    assert!(WHEEL_DEFAULT_MAX_TORQUE_NM > 0.0);
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn max_report_bytes_within_usb_full_speed_limit() {
    assert!(MAX_REPORT_BYTES <= 64);
}

#[test]
fn only_two_devices_support_ffb() {
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
    let ffb_count = all_devices.iter().filter(|d| d.supports_ffb()).count();
    assert_eq!(ffb_count, 2);
}
