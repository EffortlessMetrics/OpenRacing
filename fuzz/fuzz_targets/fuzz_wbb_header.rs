//! Fuzzes the .wbb v1 diagnostic file format header and footer validation.
//!
//! Constructs `WbbHeader` and `WbbFooter` structs with fuzz-derived field
//! values and exercises `validate()`, stream-flag helpers, and footer
//! validation to ensure they never panic on any combination of inputs.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_wbb_header

#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_diagnostic::{WbbFooter, WbbHeader};

fuzz_target!(|data: &[u8]| {
    if data.len() < 32 {
        return;
    }

    // Build a WbbHeader with fuzz-derived fields.
    let mut magic = [0u8; 4];
    magic.copy_from_slice(&data[0..4]);
    let version = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let ffb_mode = data[8];
    let stream_flags = data[9];
    let compression_level = data[10];

    let header = WbbHeader {
        magic,
        version,
        device_id: String::from_utf8_lossy(&data[11..19]).into_owned(),
        engine_version: String::from_utf8_lossy(&data[19..24]).into_owned(),
        start_time_unix: u64::from_le_bytes([
            data[24], data[25], data[26], data[27], data[28], data[29], data[30], data[31],
        ]),
        timebase_ns: 1_000_000,
        ffb_mode,
        stream_flags,
        compression_level,
        reserved: [0; 15],
        header_size: 0,
    };

    // validate() must never panic — errors are expected.
    let _ = header.validate();

    // Stream-flag helpers must never panic.
    let _ = header.has_stream_a();
    let _ = header.has_stream_b();
    let _ = header.has_stream_c();

    // Footer validation with fuzz-derived magic.
    if data.len() >= 36 {
        let mut footer_magic = [0u8; 4];
        footer_magic.copy_from_slice(&data[32..36]);

        let footer = WbbFooter {
            duration_ms: u32::from_le_bytes([data[4], data[5], data[6], data[7]]),
            total_frames: 0,
            index_offset: 0,
            index_count: 0,
            file_crc32c: 0,
            footer_magic,
        };

        let _ = footer.validate();
    }
});
