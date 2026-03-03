//! Fuzzes the IPC MessageHeader binary decoder.
//!
//! The 12-byte wire header is parsed with manual little-endian field extraction.
//! This target verifies the decode path is safe for any input length and content,
//! and round-trips successfully when decode succeeds.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_ipc_header

#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_ipc::codec::{MessageHeader, message_flags, message_types};

fuzz_target!(|data: &[u8]| {
    // Decode must never panic on arbitrary bytes.
    if let Ok(header) = MessageHeader::decode(data) {
        // Encode/decode round-trip must be lossless.
        let encoded = header.encode();
        let rt = MessageHeader::decode(&encoded);
        assert!(rt.is_ok());
        let rt = rt.unwrap_or_else(|_| unreachable!());
        assert_eq!(header.message_type, rt.message_type);
        assert_eq!(header.payload_len, rt.payload_len);
        assert_eq!(header.sequence, rt.sequence);
        assert_eq!(header.flags, rt.flags);

        // Flag queries must not panic.
        let _ = header.has_flag(message_flags::COMPRESSED);
        let _ = header.has_flag(message_flags::REQUIRES_ACK);
        let _ = header.has_flag(message_flags::IS_RESPONSE);
        let _ = header.has_flag(message_flags::IS_ERROR);
        let _ = header.has_flag(message_flags::STREAMING);

        // Message-type range check (informational, no assertion).
        let _valid = matches!(
            header.message_type,
            message_types::DEVICE
                | message_types::PROFILE
                | message_types::SAFETY
                | message_types::HEALTH
                | message_types::FEATURE_NEGOTIATION
                | message_types::GAME
                | message_types::TELEMETRY
                | message_types::DIAGNOSTIC
        );
    }
});
