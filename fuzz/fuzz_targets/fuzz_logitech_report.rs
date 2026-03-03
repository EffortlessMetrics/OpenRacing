//! Fuzzes the Logitech HID report parsing and command round-trips.
//!
//! Exercises input report parsing, vendor report building, and treats the
//! output of every command builder as a candidate input report to catch
//! cross-layer panics.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_logitech_report

#![no_main]

use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_logitech_protocol::{
    CONSTANT_FORCE_REPORT_LEN, LogitechConstantForceEncoder, LogitechModel, is_wheel_product,
    build_gain_report, build_mode_switch_report, build_native_mode_report,
    build_set_autocenter_report, build_set_leds_report, build_set_range_dfp_report,
    build_set_range_report, parse_input_report,
};

fuzz_target!(|data: &[u8]| {
    // Direct input report parsing with arbitrary bytes.
    let _ = parse_input_report(data);

    // Round-trip: build vendor reports and re-parse them as input reports.
    if data.len() >= 2 {
        let degrees = u16::from_le_bytes([data[0], data[1]]);
        let range_rpt = build_set_range_report(degrees);
        let _ = parse_input_report(&range_rpt);

        let dfp_rpt = build_set_range_dfp_report(degrees);
        let _ = parse_input_report(&dfp_rpt);
    }

    if data.len() >= 2 {
        let autocenter = build_set_autocenter_report(data[0], data[1]);
        let _ = parse_input_report(&autocenter);

        let mode_sw = build_mode_switch_report(data[0], data[1] != 0);
        let _ = parse_input_report(&mode_sw);
    }

    if let Some(&b) = data.first() {
        let gain = build_gain_report(b);
        let _ = parse_input_report(&gain);

        let leds = build_set_leds_report(b);
        let _ = parse_input_report(&leds);
    }

    // Native mode report (no parameters) round-trip.
    let native = build_native_mode_report();
    let _ = parse_input_report(&native);

    // Constant-force encode round-trip.
    if data.len() >= 4 {
        let torque = f32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let max_nm = torque.abs().max(0.1);
        let enc = LogitechConstantForceEncoder::new(max_nm);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, &mut out);
        let _ = parse_input_report(&out);
        enc.encode_zero(&mut out);
        let _ = parse_input_report(&out);
    }

    // Model identification with arbitrary product IDs.
    if data.len() >= 2 {
        let pid = u16::from_le_bytes([data[0], data[1]]);
        let _ = LogitechModel::from_product_id(pid);
        let _ = is_wheel_product(pid);
    }
});
