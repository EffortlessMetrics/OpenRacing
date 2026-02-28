//! Fuzzes the Fanatec HID input report parsers, FFB encoder, and device identification.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_fanatec_input
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_fanatec_protocol::{
    CONSTANT_FORCE_REPORT_LEN, FanatecConstantForceEncoder, FanatecModel, FanatecPedalModel,
    is_pedal_product, is_wheelbase_product, parse_extended_report, parse_pedal_report,
    parse_standard_report,
};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes.
    let _ = parse_standard_report(data);
    let _ = parse_extended_report(data);
    let _ = parse_pedal_report(data);

    // Constant-force encoder with arbitrary torque values.
    if data.len() >= 4 {
        let torque_bytes: [u8; 4] = [data[0], data[1], data[2], data[3]];
        let torque = f32::from_le_bytes(torque_bytes);
        let enc = FanatecConstantForceEncoder::new(8.0);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, 0, &mut out);
        enc.encode_zero(&mut out);
    }

    // Device identification with arbitrary PID.
    if data.len() >= 2 {
        let pid = u16::from_le_bytes([data[0], data[1]]);
        let _ = FanatecModel::from_product_id(pid);
        let _ = FanatecPedalModel::from_product_id(pid);
        let _ = is_wheelbase_product(pid);
        let _ = is_pedal_product(pid);
    }
});
