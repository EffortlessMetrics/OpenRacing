//! Fuzzes the Simucube HID input report parser and output report builder.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_simucube_input
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_simucube_protocol::{
    SimucubeInputReport, SimucubeModel, SimucubeOutputReport, simucube_model_from_info,
};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes.
    let _ = SimucubeInputReport::parse(data);

    // If we have at least 4 bytes, reinterpret first 4 as an f32 torque value
    // and verify the output report builder never panics.
    if data.len() >= 4 {
        let torque_bytes: [u8; 4] = [data[0], data[1], data[2], data[3]];
        let torque = f32::from_le_bytes(torque_bytes);
        let seq = if data.len() >= 6 {
            u16::from_le_bytes([data[4], data[5]])
        } else {
            0
        };
        let report = SimucubeOutputReport::new(seq).with_torque(torque);
        let _ = report.build();
    }

    // Reinterpret first 4 bytes as VID + PID and verify model detection never panics.
    if data.len() >= 4 {
        let vid = u16::from_le_bytes([data[0], data[1]]);
        let pid = u16::from_le_bytes([data[2], data[3]]);
        let _ = SimucubeModel::from_product_id(pid);
        let _ = simucube_model_from_info(vid, pid);
    }
});