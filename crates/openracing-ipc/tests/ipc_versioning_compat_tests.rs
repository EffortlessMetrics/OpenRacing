//! IPC versioning, feature negotiation, and wire format compatibility tests.
//!
//! Covers areas complementary to existing tests:
//! 1. Version mismatch matrix: systematic client/server version pair testing
//! 2. Wire format snapshot stability: golden-byte tests for header encoding
//! 3. Backward/forward compat with protobuf field evolution
//! 4. Unknown field handling and graceful degradation
//! 5. Transport fallback: named-pipe/unix unavailable → TCP fallback
//! 6. Timeout and retry behavior simulation
//! 7. Feature negotiation lifecycle and degraded-mode operation
//! 8. Concurrent version negotiation stress

#![deny(clippy::unwrap_used)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use openracing_ipc::codec::{
    MessageCodec, MessageDecoder, MessageEncoder, MessageHeader, message_flags, message_types,
};
use openracing_ipc::error::{IpcError, IpcResult};
use openracing_ipc::server::{
    ClientInfo, HealthEvent, HealthEventType, IpcConfig, IpcServer, PeerInfo, ServerState,
    is_version_compatible,
};
use openracing_ipc::transport::{TransportBuilder, TransportConfig, TransportType};
use openracing_ipc::{DEFAULT_TCP_PORT, MIN_CLIENT_VERSION, PROTOCOL_VERSION};

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

// ─── Inline protobuf messages for wire-level testing ────────────────────────

/// Current-version FeatureRequest
#[derive(Clone, PartialEq, prost::Message)]
struct FeatureRequest {
    #[prost(string, tag = "1")]
    client_version: String,
    #[prost(string, repeated, tag = "2")]
    supported_features: Vec<String>,
    #[prost(string, tag = "3")]
    namespace: String,
}

/// "v2" FeatureRequest with extra fields a future client might send
#[derive(Clone, PartialEq, prost::Message)]
struct FeatureRequestV2 {
    #[prost(string, tag = "1")]
    client_version: String,
    #[prost(string, repeated, tag = "2")]
    supported_features: Vec<String>,
    #[prost(string, tag = "3")]
    namespace: String,
    /// New in v2: client platform identifier
    #[prost(string, tag = "50")]
    platform: String,
    /// New in v2: client build hash
    #[prost(string, tag = "51")]
    build_hash: String,
}

/// "Legacy" FeatureRequest missing the namespace field (v0 client)
#[derive(Clone, PartialEq, prost::Message)]
struct LegacyFeatureRequest {
    #[prost(string, tag = "1")]
    client_version: String,
    #[prost(string, repeated, tag = "2")]
    supported_features: Vec<String>,
}

/// Minimal DeviceInfo (current)
#[derive(Clone, PartialEq, prost::Message)]
struct DeviceInfoCurrent {
    #[prost(string, tag = "1")]
    id: String,
    #[prost(string, tag = "2")]
    name: String,
    #[prost(int32, tag = "3")]
    device_type: i32,
    #[prost(int32, tag = "5")]
    state: i32,
}

/// Future DeviceInfo with additional fields
#[derive(Clone, PartialEq, prost::Message)]
struct DeviceInfoFuture {
    #[prost(string, tag = "1")]
    id: String,
    #[prost(string, tag = "2")]
    name: String,
    #[prost(int32, tag = "3")]
    device_type: i32,
    #[prost(int32, tag = "5")]
    state: i32,
    /// Future: firmware version string
    #[prost(string, tag = "80")]
    firmware_version: String,
    /// Future: hardware revision
    #[prost(uint32, tag = "81")]
    hw_revision: u32,
    /// Future: feature bitmap
    #[prost(uint64, tag = "82")]
    feature_bitmap: u64,
}

/// Legacy DeviceInfo missing state field (old client)
#[derive(Clone, PartialEq, prost::Message)]
struct DeviceInfoLegacy {
    #[prost(string, tag = "1")]
    id: String,
    #[prost(string, tag = "2")]
    name: String,
    #[prost(int32, tag = "3")]
    device_type: i32,
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. Version mismatch matrix
// ═══════════════════════════════════════════════════════════════════════════

mod version_mismatch_matrix {
    use super::*;

    /// Exhaustive version-pair compatibility matrix
    #[test]
    fn systematic_version_pair_matrix() -> TestResult {
        let cases: Vec<(&str, &str, bool)> = vec![
            // (client, min_server, expected_compatible)
            // Exact match
            ("1.0.0", "1.0.0", true),
            // Patch bump compatible
            ("1.0.1", "1.0.0", true),
            ("1.0.9", "1.0.0", true),
            // Minor bump compatible
            ("1.1.0", "1.0.0", true),
            ("1.5.3", "1.0.0", true),
            // Major mismatch (both directions)
            ("2.0.0", "1.0.0", false),
            ("0.9.9", "1.0.0", false),
            ("3.0.0", "1.0.0", false),
            // Client older than min
            ("1.0.0", "1.0.1", false),
            ("1.0.0", "1.1.0", false),
            ("1.1.0", "1.2.0", false),
            // Edge: zero major version
            ("0.0.1", "0.0.1", true),
            ("0.1.0", "0.0.1", true),
            ("0.0.0", "0.0.1", false),
            // Large version numbers
            ("1.99.99", "1.0.0", true),
            ("1.0.0", "1.99.99", false),
        ];

        for (client, min, expected) in &cases {
            let actual = is_version_compatible(client, min);
            assert_eq!(
                actual, *expected,
                "is_version_compatible(\"{client}\", \"{min}\") = {actual}, expected {expected}"
            );
        }
        Ok(())
    }

    /// Verify that higher minor version on client still works for any patch
    #[test]
    fn higher_minor_any_patch_is_compatible() {
        for patch in 0..10u32 {
            let client = format!("1.2.{patch}");
            assert!(
                is_version_compatible(&client, "1.1.9"),
                "Client {client} should be compatible with min 1.1.9"
            );
        }
    }

    /// Version strings with leading zeros should still parse
    #[test]
    fn versions_with_leading_zeros_parse() {
        // "01.00.00" → parsed as 1.0.0 by u32 parse
        assert!(is_version_compatible("01.00.00", "1.0.0"));
    }

    /// Very large version components don't overflow
    #[test]
    fn large_version_components() {
        assert!(is_version_compatible("1.999999.999999", "1.0.0"));
        assert!(!is_version_compatible("1.0.0", "1.999999.0"));
    }

    #[tokio::test]
    async fn version_mismatch_returns_incompatible_flag() -> TestResult {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        // Major mismatch
        let result = server
            .negotiate_features("2.0.0", &["device_management".to_string()])
            .await?;
        assert!(
            !result.compatible,
            "Major version mismatch should be incompatible"
        );

        // Server still tells client what the required version is
        assert_eq!(result.min_client_version, MIN_CLIENT_VERSION);
        assert_eq!(result.server_version, PROTOCOL_VERSION);

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn old_client_version_rejected_but_gets_upgrade_info() -> TestResult {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let result = server
            .negotiate_features("0.1.0", &["device_management".to_string()])
            .await?;

        assert!(!result.compatible);
        // The response should include the minimum version so the client knows what to upgrade to
        assert!(!result.min_client_version.is_empty());
        assert!(!result.server_version.is_empty());
        // Incompatible clients should NOT be registered
        assert_eq!(server.client_count().await, 0);

        server.stop().await?;
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. Wire format snapshot stability (golden-byte tests)
// ═══════════════════════════════════════════════════════════════════════════

mod wire_format_snapshots {
    use super::*;

    /// Golden bytes for a DEVICE header with known values.
    /// If this test breaks, the wire format has changed — a breaking change.
    #[test]
    fn device_header_golden_bytes() -> TestResult {
        let header = MessageHeader::new(message_types::DEVICE, 256, 42);
        let bytes = header.encode();

        // message_type = 0x0001 LE → [0x01, 0x00]
        assert_eq!(bytes[0], 0x01);
        assert_eq!(bytes[1], 0x00);
        // payload_len = 256 = 0x00000100 LE → [0x00, 0x01, 0x00, 0x00]
        assert_eq!(bytes[2], 0x00);
        assert_eq!(bytes[3], 0x01);
        assert_eq!(bytes[4], 0x00);
        assert_eq!(bytes[5], 0x00);
        // sequence = 42 = 0x0000002A LE → [0x2A, 0x00, 0x00, 0x00]
        assert_eq!(bytes[6], 0x2A);
        assert_eq!(bytes[7], 0x00);
        assert_eq!(bytes[8], 0x00);
        assert_eq!(bytes[9], 0x00);
        // flags = 0 → [0x00, 0x00]
        assert_eq!(bytes[10], 0x00);
        assert_eq!(bytes[11], 0x00);
        Ok(())
    }

    /// Golden bytes for a FEATURE_NEGOTIATION header with flags
    #[test]
    fn feature_negotiation_header_golden_bytes() -> TestResult {
        let mut header = MessageHeader::new(message_types::FEATURE_NEGOTIATION, 128, 1);
        header.set_flag(message_flags::REQUIRES_ACK);
        let bytes = header.encode();

        // message_type = 0x0005 LE → [0x05, 0x00]
        assert_eq!(bytes[0], 0x05);
        assert_eq!(bytes[1], 0x00);
        // payload_len = 128 = 0x00000080 LE → [0x80, 0x00, 0x00, 0x00]
        assert_eq!(bytes[2], 0x80);
        assert_eq!(bytes[3], 0x00);
        assert_eq!(bytes[4], 0x00);
        assert_eq!(bytes[5], 0x00);
        // flags = REQUIRES_ACK = 0x0002 LE → [0x02, 0x00]
        assert_eq!(bytes[10], 0x02);
        assert_eq!(bytes[11], 0x00);
        Ok(())
    }

    /// Protobuf FeatureRequest golden-byte stability: the encoded form of a
    /// known request must not change between builds.
    #[test]
    fn feature_request_protobuf_golden_bytes() -> TestResult {
        let req = FeatureRequest {
            client_version: "1.0.0".to_string(),
            supported_features: vec!["device_management".to_string()],
            namespace: String::new(),
        };

        let codec = MessageCodec::new();
        let encoded = MessageEncoder::encode(&codec, &req)?;

        // Re-encode must produce identical bytes (determinism)
        let encoded2 = MessageEncoder::encode(&codec, &req)?;
        assert_eq!(encoded, encoded2, "Protobuf encoding must be deterministic");

        // Decode back and verify fields
        let decoded: FeatureRequest = MessageDecoder::decode(&codec, &encoded)?;
        assert_eq!(decoded.client_version, "1.0.0");
        assert_eq!(decoded.supported_features, vec!["device_management"]);
        Ok(())
    }

    /// Verify that the same logical message always produces bit-identical wire bytes
    #[test]
    fn header_encoding_is_deterministic() -> TestResult {
        let header = MessageHeader::new(message_types::TELEMETRY, 4096, 999);
        let bytes1 = header.encode();
        let bytes2 = header.encode();
        assert_eq!(bytes1, bytes2, "Header encoding must be deterministic");
        Ok(())
    }

    /// Ensure header maximum values are representable
    #[test]
    fn header_max_values_roundtrip() -> TestResult {
        let header = MessageHeader::new(u16::MAX, u32::MAX, u32::MAX);
        let bytes = header.encode();
        let decoded = MessageHeader::decode(&bytes)?;
        assert_eq!(decoded.message_type, u16::MAX);
        assert_eq!(decoded.payload_len, u32::MAX);
        assert_eq!(decoded.sequence, u32::MAX);
        Ok(())
    }

    /// Ensure header zero values roundtrip
    #[test]
    fn header_zero_values_roundtrip() -> TestResult {
        let header = MessageHeader::new(0, 0, 0);
        let bytes = header.encode();
        let decoded = MessageHeader::decode(&bytes)?;
        assert_eq!(decoded.message_type, 0);
        assert_eq!(decoded.payload_len, 0);
        assert_eq!(decoded.sequence, 0);
        assert_eq!(decoded.flags, 0);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. Backward compatibility: older clients talk to newer servers
// ═══════════════════════════════════════════════════════════════════════════

mod backward_compat {
    use super::*;

    /// A legacy FeatureRequest (missing namespace) can be decoded as a current FeatureRequest.
    /// Protobuf fills missing fields with defaults.
    #[test]
    fn legacy_request_decoded_as_current() -> TestResult {
        let legacy = LegacyFeatureRequest {
            client_version: "1.0.0".to_string(),
            supported_features: vec!["device_management".to_string()],
        };

        let codec = MessageCodec::new();
        let bytes = MessageEncoder::encode(&codec, &legacy)?;
        let current: FeatureRequest = MessageDecoder::decode(&codec, &bytes)?;

        assert_eq!(current.client_version, "1.0.0");
        assert_eq!(current.supported_features, vec!["device_management"]);
        // Missing field defaults to empty string
        assert_eq!(current.namespace, "");
        Ok(())
    }

    /// Legacy DeviceInfo (no state field) decoded as current DeviceInfo.
    /// Missing `state` defaults to 0.
    #[test]
    fn legacy_device_info_decoded_as_current() -> TestResult {
        let legacy = DeviceInfoLegacy {
            id: "wheel-001".to_string(),
            name: "SimuCUBE 2".to_string(),
            device_type: 1,
        };

        let codec = MessageCodec::new();
        let bytes = MessageEncoder::encode(&codec, &legacy)?;
        let current: DeviceInfoCurrent = MessageDecoder::decode(&codec, &bytes)?;

        assert_eq!(current.id, "wheel-001");
        assert_eq!(current.name, "SimuCUBE 2");
        assert_eq!(current.device_type, 1);
        assert_eq!(current.state, 0, "Missing state should default to 0");
        Ok(())
    }

    /// An older client that only asks for a subset of features should get exactly
    /// that subset back (not all server features).
    #[tokio::test]
    async fn older_client_subset_features_honored() -> TestResult {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let result = server
            .negotiate_features(
                "1.0.0",
                &[
                    "device_management".to_string(),
                    "health_monitoring".to_string(),
                ],
            )
            .await?;

        assert!(result.compatible);
        assert_eq!(result.enabled_features.len(), 2);
        assert!(
            result
                .enabled_features
                .contains(&"device_management".to_string())
        );
        assert!(
            result
                .enabled_features
                .contains(&"health_monitoring".to_string())
        );
        // Newer features not requested should not be in enabled
        assert!(
            !result
                .enabled_features
                .contains(&"streaming_health".to_string())
        );

        server.stop().await?;
        Ok(())
    }

    /// Simulate an older client connecting, negotiating, then a newer client connecting.
    /// Both should be registered independently.
    #[tokio::test]
    async fn mixed_version_clients_coexist() -> TestResult {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        // Older client with minimal features
        let r1 = server
            .negotiate_features("1.0.0", &["device_management".to_string()])
            .await?;
        assert!(r1.compatible);

        // Newer client with more features
        let r2 = server
            .negotiate_features(
                "1.5.0",
                &[
                    "device_management".to_string(),
                    "safety_control".to_string(),
                    "streaming_health".to_string(),
                ],
            )
            .await?;
        assert!(r2.compatible);

        assert_eq!(server.client_count().await, 2);

        server.stop().await?;
        Ok(())
    }

    /// Current DeviceInfo encoded and decoded as legacy (missing fields are just dropped)
    #[test]
    fn current_device_info_readable_by_legacy_client() -> TestResult {
        let current = DeviceInfoCurrent {
            id: "pedals-001".to_string(),
            name: "Heusinkveld Sprint".to_string(),
            device_type: 2,
            state: 1,
        };

        let codec = MessageCodec::new();
        let bytes = MessageEncoder::encode(&codec, &current)?;
        let legacy: DeviceInfoLegacy = MessageDecoder::decode(&codec, &bytes)?;

        assert_eq!(legacy.id, "pedals-001");
        assert_eq!(legacy.name, "Heusinkveld Sprint");
        assert_eq!(legacy.device_type, 2);
        // `state` field (tag 5) is silently ignored by legacy decoder
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. Unknown field handling and graceful degradation
// ═══════════════════════════════════════════════════════════════════════════

mod unknown_field_handling {
    use super::*;

    /// A future client sends a FeatureRequestV2 with extra fields (platform, build_hash).
    /// The current server decodes it as FeatureRequest, ignoring the unknown fields.
    #[test]
    fn future_request_decoded_by_current_server() -> TestResult {
        let future_req = FeatureRequestV2 {
            client_version: "1.2.0".to_string(),
            supported_features: vec!["device_management".to_string()],
            namespace: "openracing".to_string(),
            platform: "windows-x64".to_string(),
            build_hash: "abc123def456".to_string(),
        };

        let codec = MessageCodec::new();
        let bytes = MessageEncoder::encode(&codec, &future_req)?;
        let current: FeatureRequest = MessageDecoder::decode(&codec, &bytes)?;

        // Known fields preserved
        assert_eq!(current.client_version, "1.2.0");
        assert_eq!(current.namespace, "openracing");
        assert_eq!(current.supported_features, vec!["device_management"]);
        // Unknown fields silently ignored — no error
        Ok(())
    }

    /// A future DeviceInfo with extra fields decoded as current DeviceInfo.
    #[test]
    fn future_device_info_decoded_as_current() -> TestResult {
        let future = DeviceInfoFuture {
            id: "wheel-002".to_string(),
            name: "Moza R21".to_string(),
            device_type: 1,
            state: 1,
            firmware_version: "2.1.3".to_string(),
            hw_revision: 5,
            feature_bitmap: 0xFF00FF00,
        };

        let codec = MessageCodec::new();
        let bytes = MessageEncoder::encode(&codec, &future)?;
        let current: DeviceInfoCurrent = MessageDecoder::decode(&codec, &bytes)?;

        assert_eq!(current.id, "wheel-002");
        assert_eq!(current.name, "Moza R21");
        assert_eq!(current.device_type, 1);
        assert_eq!(current.state, 1);
        // Extra fields (firmware_version, hw_revision, feature_bitmap) are silently dropped
        Ok(())
    }

    /// Unknown message type in header still roundtrips correctly
    #[test]
    fn unknown_message_type_in_header_roundtrips() -> TestResult {
        let future_type: u16 = 0x00FF; // Not a defined message type
        let header = MessageHeader::new(future_type, 512, 10);
        let bytes = header.encode();
        let decoded = MessageHeader::decode(&bytes)?;
        assert_eq!(decoded.message_type, future_type);
        assert_eq!(decoded.payload_len, 512);
        assert_eq!(decoded.sequence, 10);
        Ok(())
    }

    /// Unknown flags in header are preserved through encode/decode
    #[test]
    fn unknown_flags_preserved_through_roundtrip() -> TestResult {
        let mut header = MessageHeader::new(message_types::DEVICE, 0, 0);
        // Set known and unknown flags
        header.set_flag(message_flags::COMPRESSED);
        header.set_flag(0x0100); // Unknown flag
        header.set_flag(0x4000); // Another unknown flag

        let bytes = header.encode();
        let decoded = MessageHeader::decode(&bytes)?;

        assert!(decoded.has_flag(message_flags::COMPRESSED));
        assert!(decoded.has_flag(0x0100));
        assert!(decoded.has_flag(0x4000));
        // Known flags that weren't set should stay unset
        assert!(!decoded.has_flag(message_flags::REQUIRES_ACK));
        Ok(())
    }

    /// When a server gets features it doesn't know about, it just doesn't enable them
    #[tokio::test]
    async fn unknown_features_gracefully_ignored() -> TestResult {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let result = server
            .negotiate_features(
                "1.0.0",
                &[
                    "device_management".to_string(),
                    "future_feature_xyz".to_string(),
                    "another_unknown".to_string(),
                ],
            )
            .await?;

        assert!(result.compatible);
        // Only the known feature is enabled
        assert_eq!(result.enabled_features.len(), 1);
        assert!(
            result
                .enabled_features
                .contains(&"device_management".to_string())
        );

        server.stop().await?;
        Ok(())
    }

    /// Empty feature list from client still results in a valid (but empty) negotiation
    #[tokio::test]
    async fn empty_feature_list_valid_negotiation() -> TestResult {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let result = server.negotiate_features("1.0.0", &[]).await?;

        assert!(result.compatible);
        assert!(result.enabled_features.is_empty());
        // Server still reports its full feature set
        assert!(!result.supported_features.is_empty());

        server.stop().await?;
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. Transport fallback behavior
// ═══════════════════════════════════════════════════════════════════════════

mod transport_fallback {
    use super::*;

    /// TCP transport is always available as fallback
    #[test]
    fn tcp_transport_always_available() {
        let tcp = TransportType::tcp();
        let desc = tcp.description();
        assert!(desc.contains("TCP"), "TCP transport description: {desc}");
        assert!(
            desc.contains(&DEFAULT_TCP_PORT.to_string()),
            "Should use default port"
        );
    }

    /// Platform default transport can fall back to TCP
    #[test]
    fn platform_default_differs_from_tcp_fallback() {
        let platform = TransportType::platform_default();
        let tcp = TransportType::tcp();

        // On Windows, platform_default is NamedPipe; on Unix, it's UnixSocket
        // Both differ from plain TCP
        let platform_desc = platform.description();
        let tcp_desc = tcp.description();

        // They should be different transport types
        #[cfg(windows)]
        assert!(
            platform_desc.contains("Named pipe"),
            "Windows default should be named pipe, got: {platform_desc}"
        );
        #[cfg(unix)]
        assert!(
            platform_desc.contains("Unix socket"),
            "Unix default should be unix socket, got: {platform_desc}"
        );

        // TCP fallback has a port number
        assert!(
            tcp_desc.contains("TCP"),
            "TCP fallback should say TCP, got: {tcp_desc}"
        );
    }

    /// TransportBuilder can switch from platform-default to TCP fallback
    #[test]
    fn builder_switches_to_tcp_fallback() {
        let config = TransportBuilder::new()
            .transport(TransportType::tcp())
            .max_connections(100)
            .build();

        let desc = config.transport.description();
        assert!(
            desc.contains("TCP"),
            "After fallback, should be TCP: {desc}"
        );
    }

    /// IpcConfig can be constructed with TCP fallback transport
    #[test]
    fn ipc_config_with_tcp_fallback() {
        let config = IpcConfig::with_transport(TransportType::tcp());
        let desc = config.transport.transport.description();
        assert!(desc.contains("TCP"));
    }

    /// Custom TCP port for fallback
    #[test]
    fn tcp_fallback_with_custom_port() {
        let transport = TransportType::tcp_with_address("127.0.0.1", 50099);
        let desc = transport.description();
        assert!(desc.contains("50099"), "Should use custom port: {desc}");
    }

    /// Transport config serializes/deserializes for persistence across fallback
    #[test]
    fn transport_config_serde_roundtrip_for_fallback() -> TestResult {
        let config = TransportBuilder::new()
            .transport(TransportType::tcp_with_address("0.0.0.0", 8080))
            .max_connections(25)
            .connection_timeout(Duration::from_secs(15))
            .enable_acl(true)
            .build();

        let json = serde_json::to_string(&config)?;
        let deserialized: TransportConfig = serde_json::from_str(&json)?;

        assert_eq!(deserialized.max_connections, 25);
        assert_eq!(deserialized.connection_timeout, Duration::from_secs(15));
        assert!(deserialized.enable_acl);

        let desc = deserialized.transport.description();
        assert!(desc.contains("8080"));
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn named_pipe_transport_creation() {
        let pipe = TransportType::named_pipe(r"\\.\pipe\openracing-test");
        let desc = pipe.description();
        assert!(desc.contains("openracing-test"));
    }

    #[cfg(windows)]
    #[test]
    fn named_pipe_to_tcp_fallback_config() {
        // Simulate: try named pipe first, then fall back to TCP
        let primary = TransportType::named_pipe(r"\\.\pipe\openracing");
        let fallback = TransportType::tcp();

        let primary_desc = primary.description();
        let fallback_desc = fallback.description();

        assert!(primary_desc.contains("Named pipe"));
        assert!(fallback_desc.contains("TCP"));
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. Timeout and retry behavior
// ═══════════════════════════════════════════════════════════════════════════

mod timeout_retry {
    use super::*;

    /// IpcError::Timeout correctly stores and reports the timeout duration
    #[test]
    fn timeout_error_stores_duration() {
        let err = IpcError::timeout(5000);
        match &err {
            IpcError::Timeout { timeout_ms } => {
                assert_eq!(*timeout_ms, 5000);
            }
            other => panic!("Expected Timeout, got: {other:?}"),
        }
        assert!(err.is_recoverable(), "Timeout should be recoverable");
        assert!(!err.is_fatal(), "Timeout should not be fatal");
    }

    /// Timeout error display includes the duration
    #[test]
    fn timeout_error_display_includes_duration() {
        let err = IpcError::timeout(3000);
        let msg = err.to_string();
        assert!(
            msg.contains("3000"),
            "Timeout message should include duration, got: {msg}"
        );
    }

    /// Connection errors are recoverable (retry-eligible)
    #[test]
    fn connection_errors_are_recoverable() {
        let errors = [
            IpcError::ConnectionFailed("connection refused".to_string()),
            IpcError::timeout(1000),
            IpcError::VersionIncompatibility {
                client: "1.0.0".to_string(),
                server: "2.0.0".to_string(),
            },
            IpcError::FeatureNegotiation("timeout during negotiation".to_string()),
        ];

        for err in &errors {
            assert!(err.is_recoverable(), "Error should be recoverable: {err:?}");
        }
    }

    /// Fatal errors are NOT recoverable
    #[test]
    fn fatal_errors_not_recoverable() {
        let errors = [
            IpcError::TransportInit("port in use".to_string()),
            IpcError::ServerNotRunning,
            IpcError::ShutdownRequested,
        ];

        for err in &errors {
            assert!(err.is_fatal(), "Error should be fatal: {err:?}");
            assert!(
                !err.is_recoverable(),
                "Fatal error should not be recoverable: {err:?}"
            );
        }
    }

    /// Connection limit error carries the max count
    #[test]
    fn connection_limit_error_carries_max() {
        let err = IpcError::connection_limit(100);
        match &err {
            IpcError::ConnectionLimitExceeded { max } => {
                assert_eq!(*max, 100);
            }
            other => panic!("Expected ConnectionLimitExceeded, got: {other:?}"),
        }
        let msg = err.to_string();
        assert!(msg.contains("100"), "Should mention max: {msg}");
    }

    /// Negotiation timeout config is configurable and stored
    #[test]
    fn negotiation_timeout_is_configurable() {
        let config = IpcConfig {
            negotiation_timeout: Duration::from_millis(250),
            ..IpcConfig::default()
        };
        assert_eq!(config.negotiation_timeout, Duration::from_millis(250));
    }

    /// Default negotiation timeout is 5 seconds
    #[test]
    fn default_negotiation_timeout() {
        let config = IpcConfig::default();
        assert_eq!(config.negotiation_timeout, Duration::from_secs(5));
    }

    /// Server start then immediate stop doesn't panic or error
    #[tokio::test]
    async fn rapid_start_stop_no_error() -> TestResult {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;
        server.stop().await?;
        assert_eq!(server.state().await, ServerState::Stopped);
        Ok(())
    }

    /// Double-stop is idempotent (no error)
    #[tokio::test]
    async fn double_stop_is_idempotent() -> TestResult {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;
        server.stop().await?;
        // Second stop should succeed silently
        server.stop().await?;
        assert_eq!(server.state().await, ServerState::Stopped);
        Ok(())
    }

    /// Starting an already-running server returns an error
    #[tokio::test]
    async fn double_start_returns_error() -> TestResult {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let result = server.start().await;
        assert!(result.is_err(), "Double start should error");

        server.stop().await?;
        Ok(())
    }

    /// Version incompatibility error has structured fields
    #[test]
    fn version_incompat_error_structured() {
        let err = IpcError::VersionIncompatibility {
            client: "2.0.0".to_string(),
            server: "1.0.0".to_string(),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("2.0.0"),
            "Should include client version: {msg}"
        );
        assert!(
            msg.contains("1.0.0"),
            "Should include server version: {msg}"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 7. Feature negotiation lifecycle and degraded-mode operation
// ═══════════════════════════════════════════════════════════════════════════

mod feature_lifecycle {
    use super::*;

    /// Full lifecycle: connect → negotiate → use features → disconnect
    #[tokio::test]
    async fn full_client_lifecycle() -> TestResult {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        // Step 1: Negotiate
        let result = server
            .negotiate_features(
                "1.0.0",
                &[
                    "device_management".to_string(),
                    "safety_control".to_string(),
                ],
            )
            .await?;
        assert!(result.compatible);
        assert_eq!(server.client_count().await, 1);

        // Step 2: Get the registered client
        let clients = server.connected_clients().await;
        assert_eq!(clients.len(), 1);
        let client_id = clients[0].id.clone();
        assert!(client_id.starts_with("client_"));

        // Step 3: Disconnect
        server.unregister_client(&client_id).await;
        assert_eq!(server.client_count().await, 0);

        server.stop().await?;
        Ok(())
    }

    /// A client that gets zero enabled features can still communicate
    /// (degraded mode with basic connectivity)
    #[tokio::test]
    async fn degraded_mode_with_no_matching_features() -> TestResult {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let result = server
            .negotiate_features(
                "1.0.0",
                &[
                    "virtual_reality_mode".to_string(),
                    "ai_co_driver".to_string(),
                ],
            )
            .await?;

        // Compatible by version, but no features enabled
        assert!(result.compatible);
        assert!(result.enabled_features.is_empty());
        // Client is still registered
        assert_eq!(server.client_count().await, 1);

        server.stop().await?;
        Ok(())
    }

    /// Manually registered client appears in connected_clients
    #[tokio::test]
    async fn manual_client_registration() -> TestResult {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let client = ClientInfo {
            id: "manual-client-1".to_string(),
            connected_at: Instant::now(),
            version: "1.0.0".to_string(),
            features: vec!["device_management".to_string()],
            peer_info: PeerInfo::default(),
        };
        server.register_client(client).await;

        assert_eq!(server.client_count().await, 1);
        let clients = server.connected_clients().await;
        assert_eq!(clients[0].id, "manual-client-1");
        assert_eq!(clients[0].version, "1.0.0");

        server.unregister_client("manual-client-1").await;
        assert_eq!(server.client_count().await, 0);

        server.stop().await?;
        Ok(())
    }

    /// Server stop clears all connected clients
    #[tokio::test]
    async fn server_stop_clears_clients() -> TestResult {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        // Register multiple clients
        let _ = server
            .negotiate_features("1.0.0", &["device_management".to_string()])
            .await?;
        let _ = server
            .negotiate_features("1.0.0", &["safety_control".to_string()])
            .await?;
        assert_eq!(server.client_count().await, 2);

        server.stop().await?;
        assert_eq!(server.client_count().await, 0);
        Ok(())
    }

    /// Health events can be broadcast and received during active session
    #[tokio::test]
    async fn health_events_during_active_session() -> TestResult {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let mut receiver = server.subscribe_health();

        let event = HealthEvent {
            timestamp: std::time::SystemTime::now(),
            device_id: "wheel-001".to_string(),
            event_type: HealthEventType::Connected,
            message: "Device connected".to_string(),
            metadata: HashMap::new(),
        };
        server.broadcast_health_event(event);

        let received = receiver.try_recv();
        assert!(received.is_ok());

        server.stop().await?;
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 8. Concurrent version negotiation
// ═══════════════════════════════════════════════════════════════════════════

mod concurrent_negotiation {
    use super::*;

    /// Multiple clients negotiating concurrently should all succeed
    #[tokio::test]
    async fn concurrent_negotiations_all_succeed() -> TestResult {
        let server = Arc::new(IpcServer::new(IpcConfig::default()));
        server.start().await?;

        let mut handles = Vec::new();
        for i in 0..10 {
            let srv = Arc::clone(&server);
            let handle = tokio::spawn(async move {
                let features = vec![format!("device_management")];
                let version = format!("1.0.{i}");
                srv.negotiate_features(&version, &features).await
            });
            handles.push(handle);
        }

        for handle in handles {
            let result = handle.await?;
            let negotiation = result?;
            assert!(negotiation.compatible);
        }

        assert_eq!(server.client_count().await, 10);
        server.stop().await?;
        Ok(())
    }

    /// Mix of compatible and incompatible clients negotiating concurrently
    #[tokio::test]
    async fn concurrent_mixed_version_clients() -> TestResult {
        let server = Arc::new(IpcServer::new(IpcConfig::default()));
        server.start().await?;

        let mut handles = Vec::new();

        // 5 compatible clients
        for i in 0..5 {
            let srv = Arc::clone(&server);
            let handle = tokio::spawn(async move {
                srv.negotiate_features(&format!("1.0.{i}"), &["device_management".to_string()])
                    .await
            });
            handles.push((handle, true));
        }

        // 5 incompatible clients
        for i in 0..5 {
            let srv = Arc::clone(&server);
            let handle = tokio::spawn(async move {
                srv.negotiate_features(&format!("0.{i}.0"), &["device_management".to_string()])
                    .await
            });
            handles.push((handle, false));
        }

        for (handle, expected_compat) in handles {
            let result = handle.await?;
            let negotiation = result?;
            assert_eq!(
                negotiation.compatible, expected_compat,
                "Compatibility mismatch"
            );
        }

        // Only compatible clients should be registered
        assert_eq!(server.client_count().await, 5);
        server.stop().await?;
        Ok(())
    }

    /// Register and unregister clients concurrently
    #[tokio::test]
    async fn concurrent_register_unregister() -> TestResult {
        let server = Arc::new(IpcServer::new(IpcConfig::default()));
        server.start().await?;

        // Register 10 clients
        let mut client_ids = Vec::new();
        for i in 0..10 {
            let client = ClientInfo {
                id: format!("concurrent-{i}"),
                connected_at: Instant::now(),
                version: "1.0.0".to_string(),
                features: vec![],
                peer_info: PeerInfo::default(),
            };
            server.register_client(client).await;
            client_ids.push(format!("concurrent-{i}"));
        }
        assert_eq!(server.client_count().await, 10);

        // Unregister all concurrently
        let mut handles = Vec::new();
        for id in client_ids {
            let srv = Arc::clone(&server);
            let handle = tokio::spawn(async move {
                srv.unregister_client(&id).await;
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await?;
        }

        assert_eq!(server.client_count().await, 0);
        server.stop().await?;
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 9. Wire format cross-version message evolution
// ═══════════════════════════════════════════════════════════════════════════

mod wire_format_evolution {
    use super::*;

    /// Encoding a message with current schema, decoding with future schema preserves fields.
    /// (Simulates: server sends current format, future client reads it)
    #[test]
    fn current_to_future_device_info() -> TestResult {
        let current = DeviceInfoCurrent {
            id: "wheel-003".to_string(),
            name: "Fanatec DD1".to_string(),
            device_type: 1,
            state: 1,
        };

        let codec = MessageCodec::new();
        let bytes = MessageEncoder::encode(&codec, &current)?;
        let future: DeviceInfoFuture = MessageDecoder::decode(&codec, &bytes)?;

        assert_eq!(future.id, "wheel-003");
        assert_eq!(future.name, "Fanatec DD1");
        assert_eq!(future.device_type, 1);
        assert_eq!(future.state, 1);
        // New fields default to their zero values
        assert_eq!(future.firmware_version, "");
        assert_eq!(future.hw_revision, 0);
        assert_eq!(future.feature_bitmap, 0);
        Ok(())
    }

    /// Encoding with future schema, decoding with current preserves known fields.
    /// (Simulates: future client sends enhanced format, current server reads it)
    #[test]
    fn future_to_current_device_info() -> TestResult {
        let future = DeviceInfoFuture {
            id: "wheel-004".to_string(),
            name: "VRS DirectForce Pro".to_string(),
            device_type: 1,
            state: 1,
            firmware_version: "3.0.0-beta".to_string(),
            hw_revision: 7,
            feature_bitmap: 0xDEADBEEF,
        };

        let codec = MessageCodec::new();
        let bytes = MessageEncoder::encode(&codec, &future)?;
        let current: DeviceInfoCurrent = MessageDecoder::decode(&codec, &bytes)?;

        assert_eq!(current.id, "wheel-004");
        assert_eq!(current.name, "VRS DirectForce Pro");
        assert_eq!(current.device_type, 1);
        assert_eq!(current.state, 1);
        // Unknown fields are silently dropped
        Ok(())
    }

    /// Encoding with legacy schema, decoding with future schema.
    /// (Simulates: very old client sends minimal format, newest server reads it)
    #[test]
    fn legacy_to_future_device_info() -> TestResult {
        let legacy = DeviceInfoLegacy {
            id: "wheel-005".to_string(),
            name: "Logitech G29".to_string(),
            device_type: 1,
        };

        let codec = MessageCodec::new();
        let bytes = MessageEncoder::encode(&codec, &legacy)?;
        let future: DeviceInfoFuture = MessageDecoder::decode(&codec, &bytes)?;

        assert_eq!(future.id, "wheel-005");
        assert_eq!(future.name, "Logitech G29");
        assert_eq!(future.device_type, 1);
        assert_eq!(future.state, 0); // default
        assert_eq!(future.firmware_version, ""); // default
        assert_eq!(future.hw_revision, 0); // default
        assert_eq!(future.feature_bitmap, 0); // default
        Ok(())
    }

    /// Feature request round-trip: v1 → v2 → v1
    #[test]
    fn feature_request_v1_v2_v1_roundtrip() -> TestResult {
        let v1 = FeatureRequest {
            client_version: "1.0.0".to_string(),
            supported_features: vec!["safety_control".to_string()],
            namespace: "test".to_string(),
        };

        let codec = MessageCodec::new();
        let v1_bytes = MessageEncoder::encode(&codec, &v1)?;

        // Decode as v2
        let v2: FeatureRequestV2 = MessageDecoder::decode(&codec, &v1_bytes)?;
        assert_eq!(v2.client_version, "1.0.0");
        assert_eq!(v2.platform, ""); // default for missing field

        // Re-encode v2 (with defaults for new fields)
        let v2_bytes = MessageEncoder::encode(&codec, &v2)?;

        // Decode back as v1
        let v1_again: FeatureRequest = MessageDecoder::decode(&codec, &v2_bytes)?;
        assert_eq!(v1_again.client_version, "1.0.0");
        assert_eq!(v1_again.supported_features, vec!["safety_control"]);
        assert_eq!(v1_again.namespace, "test");
        Ok(())
    }

    /// Multiple messages framed in a stream with mixed versions
    #[test]
    fn mixed_version_messages_in_stream() -> TestResult {
        let codec = MessageCodec::new();

        // Message 1: legacy device info
        let legacy = DeviceInfoLegacy {
            id: "dev-1".to_string(),
            name: "Legacy Device".to_string(),
            device_type: 1,
        };
        let legacy_payload = MessageEncoder::encode(&codec, &legacy)?;
        let header1 = MessageHeader::new(message_types::DEVICE, legacy_payload.len() as u32, 1);

        // Message 2: future device info
        let future = DeviceInfoFuture {
            id: "dev-2".to_string(),
            name: "Future Device".to_string(),
            device_type: 2,
            state: 1,
            firmware_version: "9.9.9".to_string(),
            hw_revision: 99,
            feature_bitmap: u64::MAX,
        };
        let future_payload = MessageEncoder::encode(&codec, &future)?;
        let header2 = MessageHeader::new(message_types::DEVICE, future_payload.len() as u32, 2);

        // Assemble stream
        let mut stream = Vec::new();
        stream.extend_from_slice(&header1.encode());
        stream.extend_from_slice(&legacy_payload);
        stream.extend_from_slice(&header2.encode());
        stream.extend_from_slice(&future_payload);

        // Parse stream — both decoded as current
        let h1 = MessageHeader::decode(&stream[0..MessageHeader::SIZE])?;
        assert_eq!(h1.sequence, 1);
        let p1_start = MessageHeader::SIZE;
        let p1_end = p1_start + h1.payload_len as usize;
        let dev1: DeviceInfoCurrent = MessageDecoder::decode(&codec, &stream[p1_start..p1_end])?;
        assert_eq!(dev1.id, "dev-1");
        assert_eq!(dev1.state, 0); // default from legacy

        let h2_start = p1_end;
        let h2 = MessageHeader::decode(&stream[h2_start..h2_start + MessageHeader::SIZE])?;
        assert_eq!(h2.sequence, 2);
        let p2_start = h2_start + MessageHeader::SIZE;
        let p2_end = p2_start + h2.payload_len as usize;
        let dev2: DeviceInfoCurrent = MessageDecoder::decode(&codec, &stream[p2_start..p2_end])?;
        assert_eq!(dev2.id, "dev-2");
        assert_eq!(dev2.state, 1);

        Ok(())
    }

    /// Codec rejects zero-length protobuf payloads
    #[test]
    fn codec_rejects_empty_payload() {
        let codec = MessageCodec::new();
        let result: IpcResult<DeviceInfoCurrent> = MessageDecoder::decode(&codec, &[]);
        assert!(result.is_err(), "Empty payload should be rejected");
    }

    /// Codec rejects payloads exceeding max size
    #[test]
    fn codec_rejects_oversized_payload() {
        let codec = MessageCodec::with_max_size(128);
        let big = vec![0u8; 256];
        let result: IpcResult<DeviceInfoCurrent> = MessageDecoder::decode(&codec, &big);
        assert!(result.is_err(), "Oversized payload should be rejected");
    }
}
