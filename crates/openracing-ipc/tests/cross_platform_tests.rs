//! Cross-platform correctness tests for the IPC crate.
//!
//! Validates that the transport abstraction layer, platform-default selection,
//! builder API, wire protocol types, and version negotiation behave consistently
//! on Windows, Linux, and macOS.
//!
//! Every test returns `Result` — no `unwrap()` / `expect()`.

use std::time::Duration;

use openracing_ipc::prelude::*;
use openracing_ipc::{DEFAULT_TCP_PORT, MIN_CLIENT_VERSION, PROTOCOL_VERSION};

/// Convenience alias.
type R = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════════
// 1. TransportType — platform default selection
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn platform_default_transport_is_not_tcp() -> R {
    let transport = TransportType::platform_default();
    // On every supported platform the default should be a native transport,
    // not plain TCP (TCP is the fallback).
    let desc = transport.description();
    #[cfg(windows)]
    assert!(
        desc.contains("pipe"),
        "Windows default should be Named Pipe, got: {desc}"
    );
    #[cfg(unix)]
    assert!(
        desc.contains("socket") || desc.contains("Unix"),
        "Unix default should be Unix socket, got: {desc}"
    );
    Ok(())
}

#[test]
fn tcp_transport_compiles_on_all_platforms() -> R {
    let transport = TransportType::tcp();
    let desc = transport.description();
    assert!(
        desc.contains("TCP"),
        "TCP transport description should contain 'TCP': {desc}"
    );
    Ok(())
}

#[test]
fn tcp_transport_uses_loopback_and_default_port() -> R {
    let transport = TransportType::tcp();
    match transport {
        TransportType::Tcp { address, port } => {
            assert_eq!(address, "127.0.0.1");
            assert_eq!(port, DEFAULT_TCP_PORT);
        }
        #[allow(unreachable_patterns)]
        _ => return Err("tcp() should return Tcp variant".into()),
    }
    Ok(())
}

#[test]
fn tcp_transport_custom_address_and_port() -> R {
    let transport = TransportType::tcp_with_address("0.0.0.0", 9999);
    match transport {
        TransportType::Tcp { address, port } => {
            assert_eq!(address, "0.0.0.0");
            assert_eq!(port, 9999);
        }
        #[allow(unreachable_patterns)]
        _ => return Err("tcp_with_address should return Tcp variant".into()),
    }
    Ok(())
}

#[cfg(windows)]
#[test]
fn named_pipe_transport_follows_unc_convention() -> R {
    let transport = TransportType::named_pipe(r"\\.\pipe\test-pipe");
    match transport {
        TransportType::NamedPipe { pipe_name } => {
            assert!(
                pipe_name.starts_with(r"\\.\pipe\"),
                "Named pipe should follow UNC convention: {pipe_name}"
            );
        }
        _ => return Err("named_pipe should return NamedPipe variant".into()),
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_socket_transport_preserves_path() -> R {
    let transport = TransportType::unix_socket("/tmp/test.sock");
    match transport {
        TransportType::UnixSocket { socket_path } => {
            assert_eq!(socket_path, std::path::PathBuf::from("/tmp/test.sock"));
        }
        _ => return Err("unix_socket should return UnixSocket variant".into()),
    }
    Ok(())
}

#[test]
fn transport_default_impl_matches_platform_default() -> R {
    let from_default: TransportType = Default::default();
    let from_fn = TransportType::platform_default();
    // Both should produce the same description
    assert_eq!(
        from_default.description(),
        from_fn.description(),
        "Default::default() and platform_default() should agree"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. TransportType — serialization round-trip
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn tcp_transport_serde_round_trip() -> R {
    let original = TransportType::tcp();
    let json = serde_json::to_string(&original)?;
    let restored: TransportType = serde_json::from_str(&json)?;
    assert_eq!(original.description(), restored.description());
    Ok(())
}

#[test]
fn platform_default_transport_serde_round_trip() -> R {
    let original = TransportType::platform_default();
    let json = serde_json::to_string(&original)?;
    let restored: TransportType = serde_json::from_str(&json)?;
    assert_eq!(original.description(), restored.description());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. TransportConfig & TransportBuilder
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn transport_config_defaults_are_reasonable() -> R {
    let config = TransportConfig::default();
    assert_eq!(config.max_connections, 100);
    assert_eq!(config.connection_timeout, Duration::from_secs(30));
    assert!(!config.enable_acl);
    assert!(config.recv_buffer_size > 0);
    assert!(config.send_buffer_size > 0);
    Ok(())
}

#[test]
fn transport_builder_overrides_max_connections() -> R {
    let config = TransportBuilder::new().max_connections(42).build();
    assert_eq!(config.max_connections, 42);
    Ok(())
}

#[test]
fn transport_builder_overrides_timeout() -> R {
    let config = TransportBuilder::new()
        .connection_timeout(Duration::from_millis(500))
        .build();
    assert_eq!(config.connection_timeout, Duration::from_millis(500));
    Ok(())
}

#[test]
fn transport_builder_overrides_acl() -> R {
    let config = TransportBuilder::new().enable_acl(true).build();
    assert!(config.enable_acl);
    Ok(())
}

#[test]
fn transport_builder_overrides_transport_type() -> R {
    let config = TransportBuilder::new()
        .transport(TransportType::tcp_with_address("10.0.0.1", 8080))
        .build();
    assert!(config.transport.description().contains("10.0.0.1"));
    assert!(config.transport.description().contains("8080"));
    Ok(())
}

#[test]
fn transport_builder_default_impl() -> R {
    let builder: TransportBuilder = Default::default();
    let config = builder.build();
    // Should produce the same as TransportConfig::default()
    assert_eq!(config.max_connections, 100);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. IpcConfig — platform-agnostic properties
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn ipc_config_default_server_name() -> R {
    let config = IpcConfig::default();
    assert_eq!(config.server_name, "openracing-ipc");
    Ok(())
}

#[test]
fn ipc_config_default_health_buffer_size() -> R {
    let config = IpcConfig::default();
    assert_eq!(config.health_buffer_size, 1000);
    Ok(())
}

#[test]
fn ipc_config_with_transport_overrides_type() -> R {
    let config = IpcConfig::with_transport(TransportType::tcp());
    let desc = config.transport.transport.description();
    assert!(
        desc.contains("TCP"),
        "transport should be TCP after with_transport: {desc}"
    );
    Ok(())
}

#[test]
fn ipc_config_builder_chain() -> R {
    let config = IpcConfig::default()
        .max_connections(25)
        .health_buffer_size(200);
    assert_eq!(config.transport.max_connections, 25);
    assert_eq!(config.health_buffer_size, 200);
    Ok(())
}

#[test]
fn ipc_config_serde_round_trip() -> R {
    let original = IpcConfig::default().max_connections(77);
    let json = serde_json::to_string(&original)?;
    let restored: IpcConfig = serde_json::from_str(&json)?;
    assert_eq!(restored.transport.max_connections, 77);
    assert_eq!(restored.server_name, "openracing-ipc");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. IpcServer — lifecycle state machine
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn server_starts_in_stopped_state() -> R {
    let server = IpcServer::new(IpcConfig::default());
    assert_eq!(server.state().await, ServerState::Stopped);
    assert!(!server.is_running().await);
    Ok(())
}

#[tokio::test]
async fn server_start_transitions_to_running() -> R {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;
    assert_eq!(server.state().await, ServerState::Running);
    assert!(server.is_running().await);
    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn server_stop_transitions_to_stopped() -> R {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;
    server.stop().await?;
    assert_eq!(server.state().await, ServerState::Stopped);
    assert!(!server.is_running().await);
    Ok(())
}

#[tokio::test]
async fn server_double_start_returns_error() -> R {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;
    let result = server.start().await;
    assert!(result.is_err(), "double start should fail");
    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn server_double_stop_is_idempotent() -> R {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;
    server.stop().await?;
    // Second stop should not error
    server.stop().await?;
    assert_eq!(server.state().await, ServerState::Stopped);
    Ok(())
}

#[tokio::test]
async fn server_client_count_starts_at_zero() -> R {
    let server = IpcServer::new(IpcConfig::default());
    assert_eq!(server.client_count().await, 0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Feature negotiation — version compatibility
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn feature_negotiation_with_matching_version() -> R {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server
        .negotiate_features(
            PROTOCOL_VERSION,
            &[
                "device_management".to_string(),
                "safety_control".to_string(),
            ],
        )
        .await?;

    assert!(result.compatible);
    assert!(
        result
            .enabled_features
            .contains(&"device_management".to_string())
    );
    assert!(
        result
            .enabled_features
            .contains(&"safety_control".to_string())
    );
    assert_eq!(result.server_version, PROTOCOL_VERSION);
    assert_eq!(result.min_client_version, MIN_CLIENT_VERSION);

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn feature_negotiation_with_incompatible_version() -> R {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server
        .negotiate_features("0.1.0", &["device_management".to_string()])
        .await?;

    assert!(!result.compatible, "version 0.1.0 should be incompatible");

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn feature_negotiation_with_unknown_features() -> R {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server
        .negotiate_features(PROTOCOL_VERSION, &["nonexistent_feature".to_string()])
        .await?;

    assert!(result.compatible);
    assert!(
        result.enabled_features.is_empty(),
        "unknown features should not be enabled: {:?}",
        result.enabled_features
    );

    server.stop().await?;
    Ok(())
}

#[test]
fn version_compatibility_checks() -> R {
    // Same version
    assert!(is_version_compatible("1.0.0", "1.0.0"));
    // Higher minor
    assert!(is_version_compatible("1.1.0", "1.0.0"));
    // Higher patch
    assert!(is_version_compatible("1.0.1", "1.0.0"));
    // Different major
    assert!(!is_version_compatible("2.0.0", "1.0.0"));
    // Lower version
    assert!(!is_version_compatible("0.9.0", "1.0.0"));
    // Invalid format
    assert!(!is_version_compatible("abc", "1.0.0"));
    assert!(!is_version_compatible("1.0", "1.0.0"));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. ProtocolVersion — wire format
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn protocol_version_new_and_accessors() -> R {
    let v = ProtocolVersion::new(1, 2, 3);
    assert_eq!(v.major(), 1);
    assert_eq!(v.minor(), 2);
    assert_eq!(v.patch(), 3);
    Ok(())
}

#[test]
fn protocol_version_wire_round_trip() -> R {
    let original = ProtocolVersion::new(1, 0, 0);
    let bytes = original.to_bytes();
    assert_eq!(bytes.len(), ProtocolVersion::SIZE);
    let decoded = ProtocolVersion::from_bytes(&bytes)?;
    assert_eq!(original, decoded);
    Ok(())
}

#[test]
fn protocol_version_parse_valid() -> R {
    let v = ProtocolVersion::parse("2.5.10")?;
    assert_eq!(v.major(), 2);
    assert_eq!(v.minor(), 5);
    assert_eq!(v.patch(), 10);
    Ok(())
}

#[test]
fn protocol_version_parse_invalid_format() -> R {
    let result = ProtocolVersion::parse("not-a-version");
    assert!(result.is_err());
    let result = ProtocolVersion::parse("1.0");
    assert!(result.is_err());
    Ok(())
}

#[test]
fn protocol_version_display() -> R {
    let v = ProtocolVersion::new(1, 2, 3);
    let s = format!("{v}");
    assert_eq!(s, "1.2.3");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. FeatureFlags — bitmask operations
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn feature_flags_none_has_no_features() -> R {
    let flags = FeatureFlags::NONE;
    assert!(flags.is_empty());
    Ok(())
}

#[test]
fn feature_flags_device_management_exists() -> R {
    let flags = FeatureFlags::DEVICE_MANAGEMENT;
    assert!(!flags.is_empty());
    assert!(flags.contains(FeatureFlags::DEVICE_MANAGEMENT));
    Ok(())
}

#[test]
fn feature_flags_combine_with_bitor() -> R {
    let combined = FeatureFlags::from_bits(
        FeatureFlags::DEVICE_MANAGEMENT.bits() | FeatureFlags::SAFETY_CONTROL.bits(),
    );
    assert!(combined.contains(FeatureFlags::DEVICE_MANAGEMENT));
    assert!(combined.contains(FeatureFlags::SAFETY_CONTROL));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. VersionNegotiator — structured negotiation
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn version_negotiator_compatible_client() -> R {
    let server = ProtocolVersion::new(1, 0, 0);
    let min = ProtocolVersion::new(1, 0, 0);
    let negotiator = VersionNegotiator::new(server, min);

    let client = ProtocolVersion::new(1, 1, 0);
    let result = negotiator.negotiate(&client, FeatureFlags::DEVICE_MANAGEMENT)?;
    assert!(result.compatible);
    Ok(())
}

#[test]
fn version_negotiator_incompatible_client() -> R {
    let server = ProtocolVersion::new(1, 0, 0);
    let min = ProtocolVersion::new(1, 0, 0);
    let negotiator = VersionNegotiator::new(server, min);

    let client = ProtocolVersion::new(2, 0, 0);
    let result = negotiator.negotiate(&client, FeatureFlags::NONE)?;
    assert!(
        !result.compatible,
        "different major version should be incompatible"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. IpcError — error classification
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn ipc_error_recoverable_classification() -> R {
    let recoverable = IpcError::ConnectionFailed("timeout".to_string());
    assert!(recoverable.is_recoverable());
    assert!(!recoverable.is_fatal());

    let timeout = IpcError::timeout(5000);
    assert!(timeout.is_recoverable());
    Ok(())
}

#[test]
fn ipc_error_fatal_classification() -> R {
    let fatal = IpcError::TransportInit("port in use".to_string());
    assert!(fatal.is_fatal());
    assert!(!fatal.is_recoverable());
    Ok(())
}

#[test]
fn ipc_error_platform_not_supported() -> R {
    let err = IpcError::PlatformNotSupported("test transport".to_string());
    let msg = format!("{err}");
    assert!(msg.contains("test transport"));
    assert!(!err.is_recoverable());
    assert!(!err.is_fatal());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 11. MessageHeader — wire codec
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn message_header_round_trip() -> R {
    let header = MessageHeader::new(message_types::DEVICE, 256, 1);
    let bytes = header.encode();
    assert_eq!(bytes.len(), MessageHeader::SIZE);
    let decoded = MessageHeader::decode(&bytes)?;
    assert_eq!(decoded.message_type, message_types::DEVICE);
    assert_eq!(decoded.payload_len, 256);
    assert_eq!(decoded.sequence, 1);
    Ok(())
}

#[test]
fn message_header_zero_payload() -> R {
    let header = MessageHeader::new(message_types::HEALTH, 0, 0);
    let bytes = header.encode();
    let decoded = MessageHeader::decode(&bytes)?;
    assert_eq!(decoded.payload_len, 0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 12. MessageCodec — size validation
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn message_codec_default_validates_reasonable_sizes() -> R {
    let codec = openracing_ipc::MessageCodec::new();
    assert!(codec.is_valid_size(1024));
    assert!(codec.is_valid_size(1));
    assert!(!codec.is_valid_size(0));
    Ok(())
}

#[test]
fn message_codec_custom_limit() -> R {
    let codec = openracing_ipc::MessageCodec::with_max_size(512);
    assert!(codec.is_valid_size(256));
    assert!(!codec.is_valid_size(1024));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 13. Health events — cross-platform broadcast
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn health_event_broadcast_and_receive() -> R {
    let server = IpcServer::new(IpcConfig::default());
    let mut receiver = server.subscribe_health();

    let event = HealthEvent {
        timestamp: std::time::SystemTime::now(),
        device_id: "dev-001".to_string(),
        event_type: HealthEventType::Connected,
        message: "Device connected".to_string(),
        metadata: std::collections::HashMap::new(),
    };

    server.broadcast_health_event(event);

    let received = receiver.try_recv();
    assert!(received.is_ok(), "should receive broadcasted event");
    let evt = received?;
    assert_eq!(evt.device_id, "dev-001");
    assert_eq!(evt.event_type, HealthEventType::Connected);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 14. Constants — cross-platform consistency
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn protocol_constants_are_valid_semver() -> R {
    let proto = ProtocolVersion::parse(PROTOCOL_VERSION)?;
    assert!(proto.major() >= 1, "protocol version major should be >= 1");

    let min = ProtocolVersion::parse(MIN_CLIENT_VERSION)?;
    assert!(min.major() >= 1, "min client version major should be >= 1");
    Ok(())
}

#[test]
fn default_tcp_port_is_in_valid_range() -> R {
    const {
        assert!(DEFAULT_TCP_PORT > 1024);
        assert!(DEFAULT_TCP_PORT < 65535);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 15. PeerInfo — platform-specific fields compile
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn peer_info_default_compiles_on_all_platforms() -> R {
    let peer = PeerInfo::default();
    // Platform-specific field access
    #[cfg(windows)]
    {
        assert!(peer.process_id.is_none());
    }
    #[cfg(unix)]
    {
        assert!(peer.uid.is_none());
        assert!(peer.gid.is_none());
    }
    // The struct itself should exist on all platforms
    let _debug = format!("{peer:?}");
    Ok(())
}
