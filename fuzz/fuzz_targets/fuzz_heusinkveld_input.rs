//! Fuzzes the Heusinkveld pedal HID input report parser and device identification.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_heusinkveld_input
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_heusinkveld_protocol::{
    HeusinkveldInputReport, HeusinkveldModel, heusinkveld_model_from_info, is_heusinkveld_device,
};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes.
    let _ = HeusinkveldInputReport::parse(data);

    // Device identification with arbitrary VID/PID.
    if data.len() >= 4 {
        let vid = u16::from_le_bytes([data[0], data[1]]);
        let pid = u16::from_le_bytes([data[2], data[3]]);
        let model = HeusinkveldModel::from_product_id(pid);
        let _ = model.display_name();
        let _ = model.max_load_kg();
        let _ = model.pedal_count();
        let _ = heusinkveld_model_from_info(vid, pid);
        let _ = is_heusinkveld_device(vid);
    }
});
