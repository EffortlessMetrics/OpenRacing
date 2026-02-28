//! Fuzzes the Cube Controls device classification functions.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_cube_controls_input
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_cube_controls_protocol::{CubeControlsModel, is_cube_controls_product};

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }
    let pid = u16::from_le_bytes([data[0], data[1]]);
    let _ = is_cube_controls_product(pid);
    let model = CubeControlsModel::from_product_id(pid);
    let _ = model.display_name();
    let _ = model.max_torque_nm();
    let _ = model.is_provisional();
});
