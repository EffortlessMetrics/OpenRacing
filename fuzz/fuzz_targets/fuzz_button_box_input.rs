//! Fuzzes the button box HID input report parsers.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_button_box_input
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_button_box_protocol::ButtonBoxInputReport;

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes.
    let _ = ButtonBoxInputReport::parse_gamepad(data);
    let _ = ButtonBoxInputReport::parse_extended(data);
});
