//! Property-based tests for button box device identification constants.
//!
//! Uses proptest with 500 cases to verify invariants on:
//! - VID/PID constants are non-zero
//! - VID and PID are distinct values
//! - Report size and button/axis limits are sane
//! - ButtonBoxType variants can be constructed

use hid_button_box_protocol::{
    ButtonBoxType, MAX_AXES, MAX_BUTTONS, PRODUCT_ID_BUTTON_BOX, REPORT_SIZE_GAMEPAD,
    VENDOR_ID_GENERIC,
};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// VENDOR_ID_GENERIC must always be non-zero.
    #[test]
    fn prop_vendor_id_nonzero(_unused: u8) {
        prop_assert!(VENDOR_ID_GENERIC != 0,
            "VENDOR_ID_GENERIC must not be zero");
    }

    /// PRODUCT_ID_BUTTON_BOX must always be non-zero.
    #[test]
    fn prop_product_id_nonzero(_unused: u8) {
        prop_assert!(PRODUCT_ID_BUTTON_BOX != 0,
            "PRODUCT_ID_BUTTON_BOX must not be zero");
    }

    /// VID and PID must be distinct values.
    #[test]
    fn prop_vid_pid_distinct(_unused: u8) {
        prop_assert!(VENDOR_ID_GENERIC != PRODUCT_ID_BUTTON_BOX,
            "VENDOR_ID_GENERIC and PRODUCT_ID_BUTTON_BOX must differ");
    }

    /// VENDOR_ID_GENERIC must match expected value (0x1209 — pid.codes open-source VID).
    #[test]
    fn prop_vendor_id_value(_unused: u8) {
        prop_assert_eq!(VENDOR_ID_GENERIC, 0x1209,
            "VENDOR_ID_GENERIC must be 0x1209");
    }

    /// REPORT_SIZE_GAMEPAD must be positive and reasonable (≤ 64 bytes for USB HID).
    #[test]
    fn prop_report_size_sane(_unused: u8) {
        prop_assert!(REPORT_SIZE_GAMEPAD > 0,
            "REPORT_SIZE_GAMEPAD must be positive");
        prop_assert!(REPORT_SIZE_GAMEPAD <= 64,
            "REPORT_SIZE_GAMEPAD must be ≤ 64 bytes for USB HID");
    }

    /// MAX_BUTTONS must be positive and ≤ 128 (sane upper bound).
    #[test]
    fn prop_max_buttons_sane(_unused: u8) {
        prop_assert!(MAX_BUTTONS > 0,
            "MAX_BUTTONS must be positive");
        prop_assert!(MAX_BUTTONS <= 128,
            "MAX_BUTTONS must be ≤ 128");
    }

    /// MAX_AXES must be positive and ≤ 16 (sane upper bound for gamepad axes).
    #[test]
    fn prop_max_axes_sane(_unused: u8) {
        prop_assert!(MAX_AXES > 0,
            "MAX_AXES must be positive");
        prop_assert!(MAX_AXES <= 16,
            "MAX_AXES must be ≤ 16");
    }

    /// ButtonBoxType variants can be constructed and compared.
    #[test]
    fn prop_button_box_type_simple(_unused: u8) {
        let t = ButtonBoxType::Simple;
        prop_assert_eq!(t, ButtonBoxType::Simple);
    }

    #[test]
    fn prop_button_box_type_standard(_unused: u8) {
        let t = ButtonBoxType::Standard;
        prop_assert_eq!(t, ButtonBoxType::Standard);
    }

    #[test]
    fn prop_button_box_type_extended(_unused: u8) {
        let t = ButtonBoxType::Extended;
        prop_assert_eq!(t, ButtonBoxType::Extended);
    }

    /// All three ButtonBoxType variants must be distinct.
    #[test]
    fn prop_button_box_type_distinct(_unused: u8) {
        prop_assert!(ButtonBoxType::Simple != ButtonBoxType::Standard,
            "Simple and Standard must differ");
        prop_assert!(ButtonBoxType::Simple != ButtonBoxType::Extended,
            "Simple and Extended must differ");
        prop_assert!(ButtonBoxType::Standard != ButtonBoxType::Extended,
            "Standard and Extended must differ");
    }

    /// Default ButtonBoxType must be Standard.
    #[test]
    fn prop_button_box_type_default(_unused: u8) {
        let default_type = ButtonBoxType::default();
        prop_assert_eq!(default_type, ButtonBoxType::Standard,
            "Default ButtonBoxType must be Standard");
    }
}
