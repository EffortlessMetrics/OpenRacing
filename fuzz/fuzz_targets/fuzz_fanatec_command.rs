//! Fuzzes the Fanatec command protocol layer: round-trip command encoding,
//! kernel range sequences, and display/rumble/LED command parsing with
//! arbitrary payloads.
//!
//! Complements `fuzz_fanatec_input` (input reports) and `fuzz_fanatec_output`
//! (output builders) by treating raw bytes as full command frames and
//! exercising every builder→parse path that accepts untrusted data.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_fanatec_command

#![no_main]

use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_fanatec_protocol::{
    CONSTANT_FORCE_REPORT_LEN, FanatecConstantForceEncoder, FanatecModel, FanatecPedalModel,
    build_display_report, build_kernel_range_sequence, build_led_report,
    build_rotation_range_report, build_rumble_report, build_set_gain_report,
    fix_report_values, is_pedal_product, is_wheelbase_product, parse_extended_report,
    parse_pedal_report, parse_standard_report,
};

fuzz_target!(|data: &[u8]| {
    // Round-trip: build commands from fuzz data, then re-parse as input reports.
    // This catches encoder/decoder mismatches.
    if data.len() >= 2 {
        let degrees = u16::from_le_bytes([data[0], data[1]]);
        let range_report = build_rotation_range_report(degrees);
        let _ = parse_standard_report(&range_report);

        for frame in &build_kernel_range_sequence(degrees) {
            let _ = parse_standard_report(frame);
        }
    }

    // Feed display/LED/rumble command output back into the input parsers.
    if data.len() >= 5 {
        let display_cmd = build_display_report(data[0], [data[1], data[2], data[3]], data[4]);
        let _ = parse_standard_report(&display_cmd);
        let _ = parse_extended_report(&display_cmd);
    }

    if data.len() >= 3 {
        let bitmask = u16::from_le_bytes([data[0], data[1]]);
        let led_cmd = build_led_report(bitmask, data[2]);
        let _ = parse_standard_report(&led_cmd);

        let rumble_cmd = build_rumble_report(data[0], data[1], data[2]);
        let _ = parse_standard_report(&rumble_cmd);
    }

    // Gain command round-trip.
    if let Some(&gain) = data.first() {
        let gain_cmd = build_set_gain_report(gain);
        let _ = parse_standard_report(&gain_cmd);
    }

    // fix_report_values with arbitrary sign-correction input.
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

    // Encode a torque command and feed it back as a standard report.
    if data.len() >= 4 {
        let torque = f32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let enc = FanatecConstantForceEncoder::new(8.0);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, 0, &mut out);
        let _ = parse_standard_report(&out);
        let _ = parse_pedal_report(&out);
    }

    // Brute-force model identification from arbitrary u16 values.
    if data.len() >= 4 {
        let pid1 = u16::from_le_bytes([data[0], data[1]]);
        let pid2 = u16::from_le_bytes([data[2], data[3]]);
        let _ = FanatecModel::from_product_id(pid1);
        let _ = FanatecPedalModel::from_product_id(pid2);
        let _ = is_wheelbase_product(pid1);
        let _ = is_pedal_product(pid2);
    }
});
