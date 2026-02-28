//! Fuzzes the Asetek HID input report parser, output report builder, and device identification.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_asetek_input
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_asetek_protocol::{
    AsetekInputReport, AsetekModel, AsetekOutputReport, asetek_model_from_info, is_asetek_device,
};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes.
    let _ = AsetekInputReport::parse(data);

    // Output report builder with arbitrary torque values.
    if data.len() >= 6 {
        let seq = u16::from_le_bytes([data[0], data[1]]);
        let torque_bytes: [u8; 4] = [data[2], data[3], data[4], data[5]];
        let torque = f32::from_le_bytes(torque_bytes);
        let report = AsetekOutputReport::new(seq).with_torque(torque);
        let _ = report.build();
    }

    // Device identification with arbitrary VID/PID.
    if data.len() >= 4 {
        let vid = u16::from_le_bytes([data[0], data[1]]);
        let pid = u16::from_le_bytes([data[2], data[3]]);
        let _ = AsetekModel::from_product_id(pid);
        let _ = asetek_model_from_info(vid, pid);
        let _ = is_asetek_device(vid);
    }
});
