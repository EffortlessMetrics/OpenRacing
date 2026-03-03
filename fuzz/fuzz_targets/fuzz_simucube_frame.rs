//! Fuzzes Simucube SimpleMotion V2 frame-level parsing: command decode,
//! feedback report parsing, and round-trip command building.
//!
//! Complements `fuzz_simucube_input` (Simucube HID reports) and
//! `fuzz_simplemotion_command` (command decode only) by exercising the full
//! SimpleMotion V2 frame protocol including CRC validation, truncated frames,
//! and torque command round-trips.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_simucube_frame

#![no_main]

use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_simucube_protocol::{
    SimucubeInputReport, SimucubeModel, SimucubeOutputReport, simucube_model_from_info,
};
use racing_wheel_simplemotion_v2::{
    commands::{
        build_get_parameter, build_get_status, build_set_parameter, build_set_torque_command,
        decode_command,
    },
    parse_feedback_report,
};

fuzz_target!(|data: &[u8]| {
    // SimpleMotion V2 frame parsing — must handle truncated/corrupt frames.
    let _ = decode_command(data);
    let _ = parse_feedback_report(data);

    // Simucube HID-level input report parsing.
    let _ = SimucubeInputReport::parse(data);

    // Round-trip: build SimpleMotion commands from fuzz data, then decode them.
    if data.len() >= 5 {
        let torque = i16::from_le_bytes([data[0], data[1]]);
        let seq = data[2];

        let torque_frame = build_set_torque_command(torque, seq);
        let _ = decode_command(&torque_frame);

        let param_addr = u16::from_le_bytes([data[3], data[4]]);
        let get_param_frame = build_get_parameter(param_addr, seq);
        let _ = decode_command(&get_param_frame);

        let status_frame = build_get_status(seq);
        let _ = decode_command(&status_frame);
    }

    // Set-parameter round-trip with 4-byte value.
    if data.len() >= 7 {
        let param_addr = u16::from_le_bytes([data[0], data[1]]);
        let value = i32::from_le_bytes([data[2], data[3], data[4], data[5]]);
        let seq = data[6];
        let set_frame = build_set_parameter(param_addr, value, seq);
        let _ = decode_command(&set_frame);
    }

    // Simucube output report building with arbitrary torque values.
    if data.len() >= 4 {
        let torque = f32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let seq = if data.len() >= 6 {
            u16::from_le_bytes([data[4], data[5]])
        } else {
            0
        };
        let report = SimucubeOutputReport::new(seq).with_torque(torque);
        let _ = report.build();
    }

    // Model identification from arbitrary VID/PID.
    if data.len() >= 4 {
        let vid = u16::from_le_bytes([data[0], data[1]]);
        let pid = u16::from_le_bytes([data[2], data[3]]);
        let _ = SimucubeModel::from_product_id(pid);
        let _ = simucube_model_from_info(vid, pid);
    }
});
