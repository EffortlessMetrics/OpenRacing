//! Fuzzes the SimpleMotion V2 command decoder.
//!
//! Exercises `decode_command` against arbitrary byte input including truncated
//! frames, corrupted CRC, and invalid command types.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_simplemotion_command

#![no_main]

use libfuzzer_sys::fuzz_target;
use racing_wheel_simplemotion_v2::commands::decode_command;

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes.
    let _ = decode_command(data);
});
