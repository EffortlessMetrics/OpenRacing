//! Property tests for IPC message serialization, connection lifecycle,
//! error propagation, and edge cases.

#![allow(clippy::redundant_closure)]

use proptest::prelude::*;

use openracing_ipc::codec::{MessageCodec, MessageHeader, message_flags, message_types};
use openracing_ipc::error::IpcError;
use openracing_ipc::server::{
    HealthEvent, HealthEventType, IpcConfig, IpcServer, ServerState, is_version_compatible,
};
use openracing_ipc::transport::{TransportBuilder, TransportConfig, TransportType};

proptest! {
    #[test]
    fn prop_message_header_roundtrip(
        message_type in 0u16..=u16::MAX,
        payload_len in 0u32..=1_000_000u32,
        sequence in 0u32..=u32::MAX,
        flags in 0u16..=u16::MAX
    ) {
        let mut header = MessageHeader::new(message_type, payload_len, sequence);
        header.flags = flags;

        let encoded = header.encode();
        let decoded = MessageHeader::decode(&encoded).map_err(|e| {
            TestCaseError::fail(format!("decode should succeed: {e:?}"))
        })?;

        prop_assert_eq!(decoded.message_type, message_type);
        prop_assert_eq!(decoded.payload_len, payload_len);
        prop_assert_eq!(decoded.sequence, sequence);
        prop_assert_eq!(decoded.flags, flags);
    }

    #[test]
    fn prop_message_size_validation(
        max_size in 100usize..=10_000usize,
        test_size in 0usize..=20_000usize
    ) {
        let codec = MessageCodec::with_max_size(max_size);

        let expected_valid = test_size > 0 && test_size <= max_size;
        let actual_valid = codec.is_valid_size(test_size);

        prop_assert_eq!(actual_valid, expected_valid);
    }

    #[test]
    fn prop_version_compatibility_major_match(
        minor_a in 0u32..=100u32,
        patch_a in 0u32..=100u32,
        minor_b in 0u32..=100u32,
        patch_b in 0u32..=100u32
    ) {
        let client = format!("1.{}.{}", minor_a, patch_a);
        let min = format!("1.{}.{}", minor_b, patch_b);

        let result = is_version_compatible(&client, &min);

        let expected = if minor_a > minor_b {
            true
        } else if minor_a == minor_b {
            patch_a >= patch_b
        } else {
            false
        };

        prop_assert_eq!(result, expected);
    }

    #[test]
    fn prop_message_type_flags_combination(
        msg_type in any::<u16>(),
        flag_bits in any::<u16>()
    ) {
        let mut header = MessageHeader::new(msg_type, 100, 0);

        header.flags = 0;
        for bit in 0..16 {
            if (flag_bits & (1 << bit)) != 0 {
                header.set_flag(1 << bit);
            }
        }

        prop_assert_eq!(header.flags, flag_bits);

        for bit in 0..16 {
            let has_flag = header.has_flag(1 << bit);
            let expected = (flag_bits & (1 << bit)) != 0;
            prop_assert_eq!(has_flag, expected);
        }
    }

    // --- Error propagation through IPC ---

    #[test]
    fn prop_error_recoverability_is_consistent(
        timeout_ms in 1u64..=60_000u64,
        max_conns in 1usize..=1000usize
    ) {
        let timeout_err = IpcError::timeout(timeout_ms);
        prop_assert!(timeout_err.is_recoverable(), "Timeout errors should be recoverable");
        prop_assert!(!timeout_err.is_fatal(), "Timeout errors should not be fatal");

        let limit_err = IpcError::connection_limit(max_conns);
        prop_assert!(!limit_err.is_recoverable(), "Connection limit errors should not be recoverable");
        prop_assert!(!limit_err.is_fatal(), "Connection limit errors should not be fatal");
    }

    #[test]
    fn prop_error_display_contains_context(
        timeout_ms in 1u64..=60_000u64,
        max_conns in 1usize..=1000usize
    ) {
        let timeout_err = IpcError::timeout(timeout_ms);
        let msg = format!("{}", timeout_err);
        prop_assert!(
            msg.contains(&timeout_ms.to_string()),
            "Timeout error display should contain the timeout value"
        );

        let limit_err = IpcError::connection_limit(max_conns);
        let msg = format!("{}", limit_err);
        prop_assert!(
            msg.contains(&max_conns.to_string()),
            "Connection limit error display should contain the max connections"
        );
    }

    #[test]
    fn prop_version_incompatibility_error_preserves_versions(
        client_minor in 0u32..=50u32,
        server_minor in 0u32..=50u32,
    ) {
        let client_ver = format!("1.{}.0", client_minor);
        let server_ver = format!("1.{}.0", server_minor);
        let err = IpcError::VersionIncompatibility {
            client: client_ver.clone(),
            server: server_ver.clone(),
        };
        let msg = format!("{}", err);
        prop_assert!(msg.contains(&client_ver));
        prop_assert!(msg.contains(&server_ver));
        prop_assert!(err.is_recoverable());
    }

    // --- Edge cases: oversized messages ---

    #[test]
    fn prop_oversized_message_rejected(
        max_size in 100usize..=10_000usize,
    ) {
        let codec = MessageCodec::with_max_size(max_size);
        let too_large = max_size + 1;
        prop_assert!(!codec.is_valid_size(too_large), "Size above max should be invalid");
        prop_assert!(!codec.is_valid_size(0), "Zero size should be invalid");
        prop_assert!(codec.is_valid_size(max_size), "Exact max size should be valid");
        prop_assert!(codec.is_valid_size(1), "Size 1 should be valid");
    }

    // --- Edge cases: version compatibility ---

    #[test]
    fn prop_version_different_major_always_incompatible(
        major_a in 0u32..=10u32,
        major_b in 0u32..=10u32,
        minor in 0u32..=50u32,
        patch in 0u32..=50u32,
    ) {
        prop_assume!(major_a != major_b);
        let client = format!("{}.{}.{}", major_a, minor, patch);
        let min = format!("{}.{}.{}", major_b, minor, patch);
        prop_assert!(
            !is_version_compatible(&client, &min),
            "Different major versions should always be incompatible"
        );
    }

    // --- Transport configuration ---

    #[test]
    fn prop_transport_builder_preserves_config(
        max_conns in 1usize..=1000usize,
        timeout_secs in 1u64..=300u64,
        enable_acl in any::<bool>(),
    ) {
        let config = TransportBuilder::new()
            .max_connections(max_conns)
            .connection_timeout(std::time::Duration::from_secs(timeout_secs))
            .enable_acl(enable_acl)
            .build();

        prop_assert_eq!(config.max_connections, max_conns);
        prop_assert_eq!(
            config.connection_timeout,
            std::time::Duration::from_secs(timeout_secs)
        );
        prop_assert_eq!(config.enable_acl, enable_acl);
    }

    // --- IpcConfig builder ---

    #[test]
    fn prop_ipc_config_builder_preserves_values(
        max_conns in 1usize..=500usize,
        health_buf in 1usize..=10_000usize,
    ) {
        let config = IpcConfig::default()
            .max_connections(max_conns)
            .health_buffer_size(health_buf);

        prop_assert_eq!(config.transport.max_connections, max_conns);
        prop_assert_eq!(config.health_buffer_size, health_buf);
    }

    // --- Header encoding is little-endian and fixed-size ---

    #[test]
    fn prop_header_encode_always_correct_size(
        msg_type in any::<u16>(),
        payload_len in any::<u32>(),
        seq in any::<u32>(),
        flags in any::<u16>(),
    ) {
        let mut header = MessageHeader::new(msg_type, payload_len, seq);
        header.flags = flags;
        let encoded = header.encode();
        prop_assert_eq!(encoded.len(), MessageHeader::SIZE);
    }
}

// --- Connection lifecycle tests (async) ---

#[cfg(test)]
mod connection_lifecycle {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_server_start_stop_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
        let server = IpcServer::new(IpcConfig::default());
        assert_eq!(server.state().await, ServerState::Stopped);

        server.start().await?;
        assert_eq!(server.state().await, ServerState::Running);
        assert!(server.is_running().await);

        server.stop().await?;
        assert_eq!(server.state().await, ServerState::Stopped);
        assert!(!server.is_running().await);
        Ok(())
    }

    #[tokio::test]
    async fn test_server_double_start_returns_error() -> Result<(), Box<dyn std::error::Error>> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let result = server.start().await;
        assert!(result.is_err(), "Double start should return an error");

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_server_stop_when_already_stopped_is_ok() -> Result<(), Box<dyn std::error::Error>>
    {
        let server = IpcServer::new(IpcConfig::default());
        // Stop without starting should be fine
        let result = server.stop().await;
        assert!(result.is_ok(), "Stopping a stopped server should be ok");
        Ok(())
    }

    #[tokio::test]
    async fn test_server_restart_cycle() -> Result<(), Box<dyn std::error::Error>> {
        let server = IpcServer::new(IpcConfig::default());
        for _ in 0..3 {
            server.start().await?;
            assert!(server.is_running().await);
            server.stop().await?;
            assert!(!server.is_running().await);
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_client_registration_and_count() -> Result<(), Box<dyn std::error::Error>> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        assert_eq!(server.client_count().await, 0);

        // Register via feature negotiation
        let result = server
            .negotiate_features("1.0.0", &["device_management".to_string()])
            .await?;
        assert!(result.compatible);
        assert_eq!(server.client_count().await, 1);

        server.stop().await?;
        // After stop, clients should be cleared
        assert_eq!(server.client_count().await, 0);
        Ok(())
    }

    #[tokio::test]
    async fn test_feature_negotiation_incompatible_version(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let result = server
            .negotiate_features("0.1.0", &["device_management".to_string()])
            .await?;
        assert!(
            !result.compatible,
            "Version 0.1.0 should be incompatible with 1.0.0"
        );
        // Incompatible clients should not be registered
        assert_eq!(server.client_count().await, 0);

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_feature_negotiation_unknown_features(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let result = server
            .negotiate_features("1.0.0", &["nonexistent_feature".to_string()])
            .await?;
        assert!(result.compatible);
        assert!(
            result.enabled_features.is_empty(),
            "Unknown features should not be enabled"
        );

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_health_event_broadcast_and_receive() -> Result<(), Box<dyn std::error::Error>> {
        let server = IpcServer::new(IpcConfig::default());
        let mut rx = server.subscribe_health();

        let event = HealthEvent {
            timestamp: std::time::SystemTime::now(),
            device_id: "dev-abc".to_string(),
            event_type: HealthEventType::Fault,
            message: "overtemp".to_string(),
            metadata: HashMap::new(),
        };
        server.broadcast_health_event(event);

        let received = rx.try_recv();
        assert!(received.is_ok());
        let evt = received?;
        assert_eq!(evt.device_id, "dev-abc");
        assert_eq!(evt.event_type, HealthEventType::Fault);
        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_feature_negotiation() -> Result<(), Box<dyn std::error::Error>> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let features = vec!["device_management".to_string()];
        let versions: Vec<String> = (0..5).map(|i| format!("1.{}.0", i)).collect();

        for ver in &versions {
            let result = server.negotiate_features(ver, &features).await?;
            assert!(result.compatible);
        }
        assert_eq!(server.client_count().await, 5);

        server.stop().await?;
        Ok(())
    }
}

// --- Error classification tests ---

#[cfg(test)]
mod error_classification {
    use super::*;

    #[test]
    fn test_fatal_errors() -> Result<(), Box<dyn std::error::Error>> {
        let fatal_errors = vec![
            IpcError::TransportInit("init fail".to_string()),
            IpcError::ServerNotRunning,
            IpcError::ShutdownRequested,
        ];
        for err in fatal_errors {
            assert!(err.is_fatal(), "Expected fatal: {}", err);
            assert!(!err.is_recoverable(), "Fatal errors should not be recoverable: {}", err);
        }
        Ok(())
    }

    #[test]
    fn test_recoverable_errors() -> Result<(), Box<dyn std::error::Error>> {
        let recoverable_errors: Vec<IpcError> = vec![
            IpcError::ConnectionFailed("conn fail".to_string()),
            IpcError::Timeout { timeout_ms: 1000 },
            IpcError::VersionIncompatibility {
                client: "1.0.0".to_string(),
                server: "2.0.0".to_string(),
            },
            IpcError::FeatureNegotiation("neg fail".to_string()),
        ];
        for err in recoverable_errors {
            assert!(err.is_recoverable(), "Expected recoverable: {}", err);
            assert!(!err.is_fatal(), "Recoverable errors should not be fatal: {}", err);
        }
        Ok(())
    }

    #[test]
    fn test_non_fatal_non_recoverable_errors() -> Result<(), Box<dyn std::error::Error>> {
        let errors = vec![
            IpcError::EncodingFailed("encode fail".to_string()),
            IpcError::DecodingFailed("decode fail".to_string()),
            IpcError::ConnectionLimitExceeded { max: 100 },
            IpcError::InvalidConfig("bad config".to_string()),
            IpcError::PlatformNotSupported("unsupported".to_string()),
            IpcError::Grpc("grpc fail".to_string()),
        ];
        for err in errors {
            assert!(!err.is_fatal(), "Should not be fatal: {}", err);
            assert!(!err.is_recoverable(), "Should not be recoverable: {}", err);
        }
        Ok(())
    }
}

// --- Edge case: header decode with truncated bytes ---

#[cfg(test)]
mod edge_cases {
    use super::*;

    #[test]
    fn test_header_decode_short_buffer() -> Result<(), Box<dyn std::error::Error>> {
        for len in 0..MessageHeader::SIZE {
            let buf = vec![0u8; len];
            let result = MessageHeader::decode(&buf);
            assert!(result.is_err(), "Decode should fail for {}-byte buffer", len);
        }
        Ok(())
    }

    #[test]
    fn test_codec_zero_max_size() -> Result<(), Box<dyn std::error::Error>> {
        let codec = MessageCodec::with_max_size(0);
        assert!(!codec.is_valid_size(0));
        assert!(!codec.is_valid_size(1));
        assert_eq!(codec.max_message_size(), 0);
        Ok(())
    }

    #[test]
    fn test_message_type_constants_are_distinct() -> Result<(), Box<dyn std::error::Error>> {
        let types = [
            message_types::DEVICE,
            message_types::PROFILE,
            message_types::SAFETY,
            message_types::HEALTH,
            message_types::FEATURE_NEGOTIATION,
            message_types::GAME,
            message_types::TELEMETRY,
            message_types::DIAGNOSTIC,
        ];
        for i in 0..types.len() {
            for j in (i + 1)..types.len() {
                assert_ne!(types[i], types[j], "Message types at index {} and {} collide", i, j);
            }
        }
        Ok(())
    }

    #[test]
    fn test_message_flag_constants_are_powers_of_two() -> Result<(), Box<dyn std::error::Error>> {
        let flags = [
            message_flags::COMPRESSED,
            message_flags::REQUIRES_ACK,
            message_flags::IS_RESPONSE,
            message_flags::IS_ERROR,
            message_flags::STREAMING,
        ];
        for &flag in &flags {
            assert!(flag.is_power_of_two(), "Flag 0x{:04x} is not a power of two", flag);
        }
        Ok(())
    }

    #[test]
    fn test_transport_type_tcp_constructors() -> Result<(), Box<dyn std::error::Error>> {
        let tcp = TransportType::tcp();
        assert!(tcp.description().contains("TCP"));

        let custom = TransportType::tcp_with_address("10.0.0.1", 9090);
        let desc = custom.description();
        assert!(desc.contains("10.0.0.1"));
        assert!(desc.contains("9090"));
        Ok(())
    }

    #[test]
    fn test_transport_config_default_values() -> Result<(), Box<dyn std::error::Error>> {
        let config = TransportConfig::default();
        assert_eq!(config.max_connections, 100);
        assert_eq!(config.recv_buffer_size, 64 * 1024);
        assert_eq!(config.send_buffer_size, 64 * 1024);
        assert!(!config.enable_acl);
        Ok(())
    }
}
