//! Fuzzes Moza wheelbase handshake frame construction with arbitrary product IDs.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_moza_handshake_frames

#![no_main]

use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_moza_protocol::{MozaProtocol, RawWheelbaseReport};

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }
    let product_id = u16::from_le_bytes([data[0], data[1]]);
    let protocol = MozaProtocol::new(product_id);

    // Constructing and parsing the report with arbitrary rest of data must not panic.
    let rest = &data[2..];
    let _ = protocol.parse_input_state(rest);
    let _ = RawWheelbaseReport::new(rest);
});
