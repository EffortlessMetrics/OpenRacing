//! Property-based tests for button box device identification constants.
//!
//! Uses proptest with 500 cases to verify invariants on:
//! - VID/PID constants are non-zero
//! - VID and PID are distinct values
//! - Report size and button/axis limits are sane
//! - ButtonBoxType variants can be constructed

use hid_button_box_protocol::{
    ButtonBoxCapabilities, ButtonBoxType, MAX_AXES, MAX_BUTTONS, PRODUCT_ID_BUTTON_BOX,
    REPORT_SIZE_GAMEPAD, VENDOR_ID_GENERIC,
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

    /// PRODUCT_ID_BUTTON_BOX must differ from VENDOR_ID_GENERIC (VID != PID).
    #[test]
    fn prop_pid_not_equal_vid(_unused: u8) {
        prop_assert_ne!(PRODUCT_ID_BUTTON_BOX, VENDOR_ID_GENERIC,
            "PID must not equal VID");
    }

    /// The single PID must be in a non-reserved USB range (> 0x00FF).
    #[test]
    fn prop_pid_in_valid_range(_unused: u8) {
        prop_assert!(PRODUCT_ID_BUTTON_BOX > 0x00FF,
            "PID must be outside the reserved low range");
    }

    /// ButtonBoxCapabilities::basic() must always return the same values.
    #[test]
    fn prop_capabilities_basic_consistent(_unused: u8) {
        let a = ButtonBoxCapabilities::basic();
        let b = ButtonBoxCapabilities::basic();
        prop_assert_eq!(a.button_count, b.button_count);
        prop_assert_eq!(a.analog_axis_count, b.analog_axis_count);
        prop_assert_eq!(a.has_pov_hat, b.has_pov_hat);
        prop_assert_eq!(a.has_rotary_encoders, b.has_rotary_encoders);
        prop_assert_eq!(a.rotary_encoder_count, b.rotary_encoder_count);
    }

    /// ButtonBoxCapabilities::extended() must always return the same values.
    #[test]
    fn prop_capabilities_extended_consistent(_unused: u8) {
        let a = ButtonBoxCapabilities::extended();
        let b = ButtonBoxCapabilities::extended();
        prop_assert_eq!(a.button_count, b.button_count);
        prop_assert_eq!(a.analog_axis_count, b.analog_axis_count);
        prop_assert_eq!(a.has_pov_hat, b.has_pov_hat);
        prop_assert_eq!(a.has_rotary_encoders, b.has_rotary_encoders);
        prop_assert_eq!(a.rotary_encoder_count, b.rotary_encoder_count);
    }

    /// ButtonBoxCapabilities::default() must match extended().
    #[test]
    fn prop_capabilities_default_matches_extended(_unused: u8) {
        let def = ButtonBoxCapabilities::default();
        let ext = ButtonBoxCapabilities::extended();
        prop_assert_eq!(def.button_count, ext.button_count);
        prop_assert_eq!(def.analog_axis_count, ext.analog_axis_count);
        prop_assert_eq!(def.has_pov_hat, ext.has_pov_hat);
        prop_assert_eq!(def.has_rotary_encoders, ext.has_rotary_encoders);
        prop_assert_eq!(def.rotary_encoder_count, ext.rotary_encoder_count);
    }

    /// basic() capabilities must have fewer buttons than extended().
    #[test]
    fn prop_basic_fewer_buttons_than_extended(_unused: u8) {
        let basic = ButtonBoxCapabilities::basic();
        let ext = ButtonBoxCapabilities::extended();
        prop_assert!(basic.button_count < ext.button_count,
            "basic must have fewer buttons than extended");
    }

    /// extended() must support rotary encoders; basic() must not.
    #[test]
    fn prop_rotary_support_matches_type(_unused: u8) {
        let basic = ButtonBoxCapabilities::basic();
        let ext = ButtonBoxCapabilities::extended();
        prop_assert!(!basic.has_rotary_encoders,
            "basic must not have rotary encoders");
        prop_assert!(ext.has_rotary_encoders,
            "extended must have rotary encoders");
    }

    /// All capabilities button counts must be within MAX_BUTTONS.
    #[test]
    fn prop_capabilities_within_max_buttons(_unused: u8) {
        prop_assert!(ButtonBoxCapabilities::basic().button_count <= MAX_BUTTONS);
        prop_assert!(ButtonBoxCapabilities::extended().button_count <= MAX_BUTTONS);
        prop_assert!(ButtonBoxCapabilities::default().button_count <= MAX_BUTTONS);
    }

    /// All capabilities axis counts must be within MAX_AXES.
    #[test]
    fn prop_capabilities_within_max_axes(_unused: u8) {
        prop_assert!(ButtonBoxCapabilities::basic().analog_axis_count <= MAX_AXES);
        prop_assert!(ButtonBoxCapabilities::extended().analog_axis_count <= MAX_AXES);
        prop_assert!(ButtonBoxCapabilities::default().analog_axis_count <= MAX_AXES);
    }
}
