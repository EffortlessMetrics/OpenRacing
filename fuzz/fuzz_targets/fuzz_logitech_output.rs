//! Fuzzes the Logitech HID output report builders and force encoding helpers.
//!
//! Covers: DFP/G-series range encoding, autocenter, gain, LED, mode switch,
//! and the constant-force zero-torque encoder. Must never panic on arbitrary input.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_logitech_output
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_logitech_protocol::{
    CONSTANT_FORCE_REPORT_LEN, LogitechConstantForceEncoder, build_gain_report,
    build_mode_switch_report, build_native_mode_report, build_set_autocenter_report,
    build_set_leds_report, build_set_range_dfp_report, build_set_range_report,
};

fuzz_target!(|data: &[u8]| {
    // Range reports with arbitrary degree values.
    if data.len() >= 2 {
        let degrees = u16::from_le_bytes([data[0], data[1]]);
        let _ = build_set_range_report(degrees);
        let _ = build_set_range_dfp_report(degrees);
    }

    // Autocenter with arbitrary strength and rate.
    if data.len() >= 2 {
        let _ = build_set_autocenter_report(data[0], data[1]);
    }

    // Gain, LED, and mode-switch reports with arbitrary byte values.
    if let Some(&b) = data.first() {
        let _ = build_gain_report(b);
        let _ = build_set_leds_report(b);
    }

    if data.len() >= 2 {
        let _ = build_mode_switch_report(data[0], data[1] != 0);
    }

    // Native mode report (no parameters).
    let _ = build_native_mode_report();

    // Constant-force encoder: encode_zero path.
    if data.len() >= 4 {
        let torque = f32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let enc = LogitechConstantForceEncoder::new(torque.abs().max(0.1));
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode_zero(&mut out);
    }
});
