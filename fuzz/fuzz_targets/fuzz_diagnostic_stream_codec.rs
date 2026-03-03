//! Fuzzes the diagnostic blackbox stream record reader.
//!
//! `StreamReader` parses length-prefixed, bincode-encoded records from raw
//! bytes.  This target feeds arbitrary bytes and verifies that all three stream
//! readers (A, B, C) handle malformed length prefixes, truncated payloads, and
//! invalid bincode data without panicking.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_diagnostic_stream_codec

#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_diagnostic::StreamReader;

fuzz_target!(|data: &[u8]| {
    // Cap input size to avoid large allocations from length prefixes.
    if data.len() > 16 * 1024 {
        return;
    }

    // Use first byte to select which stream type to parse.
    if data.is_empty() {
        return;
    }
    let mode = data[0] % 3;
    let payload = data[1..].to_vec();

    let mut reader = StreamReader::new(payload);

    match mode {
        0 => {
            // Stream A: 1kHz frames
            while let Ok(Some(_record)) = reader.read_stream_a_record() {}
        }
        1 => {
            // Stream B: 60Hz telemetry
            while let Ok(Some(_record)) = reader.read_stream_b_record() {}
        }
        _ => {
            // Stream C: health/fault events
            while let Ok(Some(_record)) = reader.read_stream_c_record() {}
        }
    }
});
