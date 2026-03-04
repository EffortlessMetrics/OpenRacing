//! Fuzzes the OpenRacing Firmware Bundle (.owfb) binary format parser.
//!
//! Exercises `FirmwareBundle::parse()` with arbitrary byte input to verify the
//! parser gracefully rejects malformed magic, truncated headers, invalid JSON
//! blocks, and oversized payload claims without panicking.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_firmware_bundle

#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_firmware_update::FirmwareBundle;

fuzz_target!(|data: &[u8]| {
    // Cap input size to prevent OOM from large payload-length claims.
    if data.len() > 64 * 1024 {
        return;
    }
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let _ = FirmwareBundle::parse(data);
});
