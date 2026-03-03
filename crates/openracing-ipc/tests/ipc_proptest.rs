#![allow(clippy::redundant_closure)]
//! Property-based tests for IPC message encoding roundtrips, header encoding,
//! codec size validation, error classification invariants, and flag operations.

use openracing_ipc::codec::{MessageHeader, message_flags, message_types};
use openracing_ipc::error::IpcError;
use proptest::prelude::*;

// ── Tests ───────────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    // === MessageHeader: encode/decode roundtrip ===

    #[test]
    fn prop_header_encode_decode_roundtrip(
        message_type in any::<u16>(),
        payload_len in any::<u32>(),
        sequence in any::<u32>(),
        flags in any::<u16>(),
    ) {
        let mut header = MessageHeader::new(message_type, payload_len, sequence);
        header.flags = flags;

        let encoded = header.encode();
        prop_assert_eq!(encoded.len(), MessageHeader::SIZE);

        let decoded = MessageHeader::decode(&encoded)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert_eq!(header.message_type, decoded.message_type);
        prop_assert_eq!(header.payload_len, decoded.payload_len);
        prop_assert_eq!(header.sequence, decoded.sequence);
        prop_assert_eq!(header.flags, decoded.flags);
    }

    // === MessageHeader: encode always produces SIZE bytes ===

    #[test]
    fn prop_header_encode_always_fixed_size(
        message_type in any::<u16>(),
        payload_len in any::<u32>(),
        sequence in any::<u32>(),
    ) {
        let header = MessageHeader::new(message_type, payload_len, sequence);
        let encoded = header.encode();
        prop_assert_eq!(
            encoded.len(), MessageHeader::SIZE,
            "header should always be {} bytes", MessageHeader::SIZE
        );
    }

    // === MessageHeader: decode with insufficient bytes fails ===

    #[test]
    fn prop_header_decode_truncated(len in 0usize..MessageHeader::SIZE) {
        let bytes = vec![0u8; len];
        let result = MessageHeader::decode(&bytes);
        prop_assert!(result.is_err(), "decode with {} bytes should fail", len);
    }

    // === MessageHeader: decode with excess bytes succeeds ===

    #[test]
    fn prop_header_decode_with_extra_bytes(
        message_type in any::<u16>(),
        payload_len in any::<u32>(),
        sequence in any::<u32>(),
        extra_len in 1usize..100,
    ) {
        let header = MessageHeader::new(message_type, payload_len, sequence);
        let encoded = header.encode();
        let mut extended = encoded.to_vec();
        extended.extend(vec![0u8; extra_len]);

        let decoded = MessageHeader::decode(&extended)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert_eq!(header.message_type, decoded.message_type);
        prop_assert_eq!(header.payload_len, decoded.payload_len);
        prop_assert_eq!(header.sequence, decoded.sequence);
    }

    // === MessageHeader: flag set/check roundtrip ===

    #[test]
    fn prop_header_flag_set_check(
        flag_bits in prop::collection::vec(
            prop_oneof![
                Just(message_flags::COMPRESSED),
                Just(message_flags::REQUIRES_ACK),
                Just(message_flags::IS_RESPONSE),
                Just(message_flags::IS_ERROR),
                Just(message_flags::STREAMING),
            ],
            0..=5
        ),
    ) {
        let mut header = MessageHeader::new(message_types::DEVICE, 0, 0);
        for &flag in &flag_bits {
            header.set_flag(flag);
        }
        // Every flag that was set should be present
        for &flag in &flag_bits {
            prop_assert!(
                header.has_flag(flag),
                "flag 0x{:04x} should be set after set_flag", flag
            );
        }
    }

    // === MessageHeader: flags are idempotent ===

    #[test]
    fn prop_header_flag_idempotent(
        flag in prop_oneof![
            Just(message_flags::COMPRESSED),
            Just(message_flags::REQUIRES_ACK),
            Just(message_flags::IS_RESPONSE),
            Just(message_flags::IS_ERROR),
            Just(message_flags::STREAMING),
        ],
    ) {
        let mut header = MessageHeader::new(message_types::DEVICE, 0, 0);
        header.set_flag(flag);
        let flags_after_first = header.flags;
        header.set_flag(flag);
        prop_assert_eq!(
            header.flags, flags_after_first,
            "setting same flag twice should be idempotent"
        );
    }

    // === MessageHeader: flags preserve other flags ===

    #[test]
    fn prop_header_flag_preserves_others(initial_flags in any::<u16>()) {
        let mut header = MessageHeader::new(message_types::DEVICE, 0, 0);
        header.flags = initial_flags;
        header.set_flag(message_flags::COMPRESSED);
        // All bits that were set before should still be set
        prop_assert_eq!(
            header.flags & initial_flags, initial_flags,
            "set_flag should not clear existing flags"
        );
    }

    // === Codec: is_valid_size boundary conditions ===

    #[test]
    fn prop_codec_valid_size_boundary(max_size in 1usize..=1_000_000) {
        let codec = openracing_ipc::MessageCodec::with_max_size(max_size);
        // size 0 is always invalid
        prop_assert!(!codec.is_valid_size(0), "size 0 should be invalid");
        // size == max_size is valid
        prop_assert!(codec.is_valid_size(max_size), "size == max should be valid");
        // size > max_size is invalid
        if max_size < usize::MAX {
            prop_assert!(!codec.is_valid_size(max_size + 1), "size > max should be invalid");
        }
    }

    // === Codec: max_message_size preserved ===

    #[test]
    fn prop_codec_max_size_preserved(max_size in 1usize..=100_000_000) {
        let codec = openracing_ipc::MessageCodec::with_max_size(max_size);
        prop_assert_eq!(
            codec.max_message_size(), max_size,
            "max_message_size should be preserved"
        );
    }

    // === IpcError: is_recoverable and is_fatal are mutually exclusive ===

    #[test]
    fn prop_error_recoverable_fatal_exclusive(idx in 0u32..13) {
        let variant = match idx {
            0 => IpcError::TransportInit("test".into()),
            1 => IpcError::ConnectionFailed("test".into()),
            2 => IpcError::EncodingFailed("test".into()),
            3 => IpcError::DecodingFailed("test".into()),
            4 => IpcError::VersionIncompatibility {
                client: "1.0.0".into(), server: "2.0.0".into(),
            },
            5 => IpcError::FeatureNegotiation("test".into()),
            6 => IpcError::ServerNotRunning,
            7 => IpcError::ConnectionLimitExceeded { max: 100 },
            8 => IpcError::Timeout { timeout_ms: 5000 },
            9 => IpcError::Grpc("test".into()),
            10 => IpcError::InvalidConfig("test".into()),
            11 => IpcError::PlatformNotSupported("test".into()),
            _ => IpcError::ShutdownRequested,
        };
        let recoverable = variant.is_recoverable();
        let fatal = variant.is_fatal();
        prop_assert!(
            !(recoverable && fatal),
            "error variant {} should not be both recoverable and fatal", idx
        );
    }

    // === IpcError: display is non-empty ===

    #[test]
    fn prop_error_display_non_empty(idx in 0u32..8) {
        let variant = match idx {
            0 => IpcError::TransportInit("x".into()),
            1 => IpcError::ConnectionFailed("x".into()),
            2 => IpcError::EncodingFailed("x".into()),
            3 => IpcError::DecodingFailed("x".into()),
            4 => IpcError::ServerNotRunning,
            5 => IpcError::ShutdownRequested,
            6 => IpcError::Timeout { timeout_ms: 1 },
            _ => IpcError::ConnectionLimitExceeded { max: 1 },
        };
        let display = format!("{variant}");
        prop_assert!(!display.is_empty(), "error display should not be empty");
    }

    // === IpcError: timeout helper preserves value ===

    #[test]
    fn prop_error_timeout_preserves_value(ms in any::<u64>()) {
        let err = IpcError::timeout(ms);
        match err {
            IpcError::Timeout { timeout_ms } => prop_assert_eq!(timeout_ms, ms),
            other => prop_assert!(false, "expected Timeout, got {:?}", other),
        }
    }

    // === IpcError: connection_limit helper preserves value ===

    #[test]
    fn prop_error_connection_limit_preserves_value(max in any::<usize>()) {
        let err = IpcError::connection_limit(max);
        match err {
            IpcError::ConnectionLimitExceeded { max: m } => prop_assert_eq!(m, max),
            other => prop_assert!(false, "expected ConnectionLimitExceeded, got {:?}", other),
        }
    }

    // === IpcError: version incompatibility preserves versions ===

    #[test]
    fn prop_error_version_incompat_preserves(
        client in "[0-9]+\\.[0-9]+\\.[0-9]+",
        server in "[0-9]+\\.[0-9]+\\.[0-9]+",
    ) {
        let err = IpcError::VersionIncompatibility {
            client: client.clone(),
            server: server.clone(),
        };
        let display = format!("{err}");
        prop_assert!(display.contains(&client), "display should contain client version");
        prop_assert!(display.contains(&server), "display should contain server version");
    }

    // === MessageHeader: sequence monotonicity encoding ===

    #[test]
    fn prop_header_sequence_order_preserved(seq1 in any::<u32>(), seq2 in any::<u32>()) {
        let h1 = MessageHeader::new(message_types::DEVICE, 0, seq1);
        let h2 = MessageHeader::new(message_types::DEVICE, 0, seq2);

        let d1 = MessageHeader::decode(&h1.encode())
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let d2 = MessageHeader::decode(&h2.encode())
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

        // Ordering must be preserved through encode/decode
        prop_assert_eq!(
            seq1.cmp(&seq2),
            d1.sequence.cmp(&d2.sequence),
            "sequence ordering should be preserved"
        );
    }
}
