//! Fuzzes the Fanatec HID output report builders and `fix_report_values`.
//!
//! Covers: FFB output encoding, rotation range sequences, gain, LED/display/rumble
//! reports, and the sign-correction helper. Must never panic on arbitrary input.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_fanatec_output
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_fanatec_protocol::{
    build_display_report, build_kernel_range_sequence, build_led_report,
    build_rotation_range_report, build_rumble_report, build_set_gain_report, fix_report_values,
};

fuzz_target!(|data: &[u8]| {
    // fix_report_values: needs exactly 14 bytes → [i16; 7].
    if data.len() >= 14 {
        let mut values: [i16; 7] = [
            i16::from_le_bytes([data[0], data[1]]),
            i16::from_le_bytes([data[2], data[3]]),
            i16::from_le_bytes([data[4], data[5]]),
            i16::from_le_bytes([data[6], data[7]]),
            i16::from_le_bytes([data[8], data[9]]),
            i16::from_le_bytes([data[10], data[11]]),
            i16::from_le_bytes([data[12], data[13]]),
        ];
        fix_report_values(&mut values);
    }

    // Rotation range and kernel range sequence with arbitrary degree values.
    if data.len() >= 2 {
        let degrees = u16::from_le_bytes([data[0], data[1]]);
        let _ = build_rotation_range_report(degrees);
        let _ = build_kernel_range_sequence(degrees);
    }

    // Gain report with arbitrary percentage.
    if let Some(&gain) = data.first() {
        let _ = build_set_gain_report(gain);
    }

    // LED report with arbitrary bitmask and brightness.
    if data.len() >= 3 {
        let bitmask = u16::from_le_bytes([data[0], data[1]]);
        let _ = build_led_report(bitmask, data[2]);
    }

    // Display report with arbitrary mode, digits, and brightness.
    if data.len() >= 5 {
        let digits = [data[1], data[2], data[3]];
        let _ = build_display_report(data[0], digits, data[4]);
    }

    // Rumble report with arbitrary left, right, duration.
    if data.len() >= 3 {
        let _ = build_rumble_report(data[0], data[1], data[2]);
    }
});
