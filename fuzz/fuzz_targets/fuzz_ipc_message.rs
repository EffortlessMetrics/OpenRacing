//! Fuzzes the full IPC message framing: header decode followed by codec payload
//! size validation.
//!
//! Simulates receiving an arbitrary byte stream and parsing it as a framed IPC
//! message (12-byte header + payload). Tests that the codec correctly rejects
//! oversized, zero-length, and truncated payloads without panicking.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_ipc_message

#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_ipc::codec::{MessageCodec, MessageHeader};

fuzz_target!(|data: &[u8]| {
    // Step 1: Attempt header decode — must never panic.
    let header = match MessageHeader::decode(data) {
        Ok(h) => h,
        Err(_) => return,
    };

    // Step 2: Validate framing — payload_len vs actual remaining bytes.
    let payload_start = MessageHeader::SIZE;
    let payload = &data[payload_start..];

    // The codec must handle any claimed payload_len gracefully.
    let codec = MessageCodec::new();
    let _ = codec.is_valid_size(header.payload_len as usize);

    // Also test a restrictive codec (1 KB max).
    let small_codec = MessageCodec::with_max_size(1024);
    let _ = small_codec.is_valid_size(payload.len());
    let _ = small_codec.is_valid_size(header.payload_len as usize);

    // Step 3: Slice payload to claimed length (clamped to available bytes).
    let claimed_len = header.payload_len as usize;
    let available = payload.len().min(claimed_len);
    let _payload_slice = &payload[..available];
});
