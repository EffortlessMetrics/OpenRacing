//! IPC backward compatibility and protocol versioning tests.
//!
//! Validates:
//! - Protocol version negotiation (client ↔ server version agreement)
//! - Feature negotiation (capability exchange and intersection)
//! - Backward compatibility (newer server handles older client messages)
//! - Forward compatibility (unknown fields are gracefully ignored)
//! - Wire format stability (snapshot tests for serialized protobuf messages)
//! - Error handling (version mismatch produces clear error messages)
//! - Graceful degradation (reduced functionality when features are unsupported)
//! - Connection lifecycle (connect, negotiate, use, disconnect, reconnect)

use std::collections::BTreeMap;

use openracing_ipc::codec::{
    MessageCodec, MessageDecoder, MessageEncoder, MessageHeader, message_flags, message_types,
};
use openracing_ipc::error::{IpcError, IpcResult};
use openracing_ipc::server::{
    ClientInfo, HealthEvent, HealthEventType, IpcConfig, IpcServer, PeerInfo, ServerState,
    is_version_compatible,
};
use openracing_ipc::{MIN_CLIENT_VERSION, PROTOCOL_VERSION};

use prost::Message;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Inline protobuf messages for wire-level testing ─────────────────────────

/// Mirrors the FeatureNegotiationRequest from wheel.proto
#[derive(Clone, PartialEq, prost::Message)]
struct ProtoFeatureRequest {
    #[prost(string, tag = "1")]
    client_version: String,
    #[prost(string, repeated, tag = "2")]
    supported_features: Vec<String>,
    #[prost(string, tag = "3")]
    namespace: String,
}

/// Mirrors the FeatureNegotiationResponse from wheel.proto
#[derive(Clone, PartialEq, prost::Message)]
struct ProtoFeatureResponse {
    #[prost(string, tag = "1")]
    server_version: String,
    #[prost(string, repeated, tag = "2")]
    supported_features: Vec<String>,
    #[prost(string, repeated, tag = "3")]
    enabled_features: Vec<String>,
    #[prost(bool, tag = "4")]
    compatible: bool,
    #[prost(string, tag = "5")]
    min_client_version: String,
}

/// Minimal DeviceId (tag 1 only)
#[derive(Clone, PartialEq, prost::Message)]
struct ProtoDeviceId {
    #[prost(string, tag = "1")]
    id: String,
}

/// Full DeviceInfo for backward compat tests
#[derive(Clone, PartialEq, prost::Message)]
struct ProtoDeviceInfo {
    #[prost(string, tag = "1")]
    id: String,
    #[prost(string, tag = "2")]
    name: String,
    #[prost(int32, tag = "3")]
    device_type: i32,
    #[prost(int32, tag = "5")]
    state: i32,
}

/// "Older client" DeviceInfo that lacks the `state` field (forward-compat scenario)
#[derive(Clone, PartialEq, prost::Message)]
struct LegacyDeviceInfo {
    #[prost(string, tag = "1")]
    id: String,
    #[prost(string, tag = "2")]
    name: String,
    #[prost(int32, tag = "3")]
    device_type: i32,
}

/// "Future client" DeviceInfo with an extra field the current server does not know
#[derive(Clone, PartialEq, prost::Message)]
struct FutureDeviceInfo {
    #[prost(string, tag = "1")]
    id: String,
    #[prost(string, tag = "2")]
    name: String,
    #[prost(int32, tag = "3")]
    device_type: i32,
    #[prost(int32, tag = "5")]
    state: i32,
    /// New field added in a hypothetical future version
    #[prost(string, tag = "100")]
    firmware_hash: String,
}

/// OpResult proto mirror (used for backward compat tests)
#[derive(Clone, PartialEq, prost::Message)]
#[allow(dead_code)]
struct ProtoOpResult {
    #[prost(bool, tag = "1")]
    success: bool,
    #[prost(string, tag = "2")]
    error_message: String,
}

// ════════════════════════════════════════════════════════════════════════════
// 1. Protocol version negotiation
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn protocol_version_constant_is_semver() -> TestResult {
    let parts: Vec<&str> = PROTOCOL_VERSION.split('.').collect();
    assert_eq!(parts.len(), 3, "PROTOCOL_VERSION must be semver (x.y.z)");
    for part in &parts {
        let _: u32 = part
            .parse()
            .map_err(|_| format!("PROTOCOL_VERSION component '{part}' is not a valid u32"))?;
    }
    Ok(())
}

#[test]
fn min_client_version_constant_is_semver() -> TestResult {
    let parts: Vec<&str> = MIN_CLIENT_VERSION.split('.').collect();
    assert_eq!(parts.len(), 3, "MIN_CLIENT_VERSION must be semver (x.y.z)");
    for part in &parts {
        let _: u32 = part
            .parse()
            .map_err(|_| format!("MIN_CLIENT_VERSION component '{part}' is not a valid u32"))?;
    }
    Ok(())
}

#[test]
fn server_version_is_at_least_min_client_version() {
    assert!(
        is_version_compatible(PROTOCOL_VERSION, MIN_CLIENT_VERSION),
        "Server PROTOCOL_VERSION must be compatible with its own MIN_CLIENT_VERSION"
    );
}

#[test]
fn version_negotiation_same_major_higher_minor() {
    // Client 1.3.0, min 1.0.0 → compatible (newer client, same major)
    assert!(is_version_compatible("1.3.0", "1.0.0"));
}

#[test]
fn version_negotiation_same_major_higher_patch() {
    assert!(is_version_compatible("1.0.7", "1.0.0"));
}

#[test]
fn version_negotiation_exact_match() {
    assert!(is_version_compatible("1.0.0", "1.0.0"));
}

#[test]
fn version_negotiation_major_mismatch_rejected() {
    assert!(!is_version_compatible("2.0.0", "1.0.0"));
    assert!(!is_version_compatible("0.9.9", "1.0.0"));
}

#[test]
fn version_negotiation_minor_too_low_rejected() {
    assert!(!is_version_compatible("1.0.0", "1.1.0"));
}

#[test]
fn version_negotiation_patch_too_low_rejected() {
    assert!(!is_version_compatible("1.1.0", "1.1.1"));
}

#[test]
fn version_negotiation_malformed_versions() {
    let bad_versions = ["", "1", "1.0", "abc", "v1.0.0", "-1.0.0"];
    for v in &bad_versions {
        assert!(
            !is_version_compatible(v, "1.0.0"),
            "Malformed client version '{v}' should be incompatible"
        );
    }
}

#[test]
fn version_negotiation_malformed_min_version() {
    assert!(
        !is_version_compatible("1.0.0", ""),
        "Empty min_version should yield incompatible"
    );
    assert!(
        !is_version_compatible("1.0.0", "abc"),
        "Non-numeric min_version should yield incompatible"
    );
}

#[tokio::test]
async fn server_negotiate_returns_protocol_version() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server
        .negotiate_features("1.0.0", &["device_management".to_string()])
        .await?;

    assert_eq!(result.server_version, PROTOCOL_VERSION);
    assert_eq!(result.min_client_version, MIN_CLIENT_VERSION);

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn server_negotiate_compatible_sets_flag() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server.negotiate_features("1.0.0", &[]).await?;
    assert!(result.compatible);

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn server_negotiate_incompatible_sets_flag() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server.negotiate_features("0.1.0", &[]).await?;
    assert!(!result.compatible);

    server.stop().await?;
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// 2. Feature negotiation
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn feature_negotiation_returns_all_server_features() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server.negotiate_features("1.0.0", &[]).await?;

    let expected = [
        "device_management",
        "profile_management",
        "safety_control",
        "health_monitoring",
        "game_integration",
        "streaming_health",
        "streaming_devices",
    ];
    for feat in &expected {
        assert!(
            result.supported_features.contains(&feat.to_string()),
            "Server should advertise feature '{feat}'"
        );
    }

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn feature_negotiation_enables_only_intersection() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let client_features = vec![
        "device_management".to_string(),
        "safety_control".to_string(),
        "nonexistent_feature".to_string(),
    ];
    let result = server.negotiate_features("1.0.0", &client_features).await?;

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
    assert!(
        !result
            .enabled_features
            .contains(&"nonexistent_feature".to_string())
    );
    assert_eq!(result.enabled_features.len(), 2);

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn feature_negotiation_empty_client_features() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server.negotiate_features("1.0.0", &[]).await?;
    assert!(result.compatible);
    assert!(result.enabled_features.is_empty());
    assert!(!result.supported_features.is_empty());

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn feature_negotiation_all_supported_features() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let all_features: Vec<String> = vec![
        "device_management".to_string(),
        "profile_management".to_string(),
        "safety_control".to_string(),
        "health_monitoring".to_string(),
        "game_integration".to_string(),
        "streaming_health".to_string(),
        "streaming_devices".to_string(),
    ];
    let result = server.negotiate_features("1.0.0", &all_features).await?;

    assert_eq!(result.enabled_features.len(), all_features.len());

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn feature_negotiation_unknown_features_silently_dropped() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server
        .negotiate_features(
            "1.0.0",
            &[
                "future_vr_support".to_string(),
                "quantum_telemetry".to_string(),
            ],
        )
        .await?;

    assert!(result.compatible);
    assert!(result.enabled_features.is_empty());

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn feature_negotiation_incompatible_client_not_registered() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let _result = server
        .negotiate_features("0.1.0", &["device_management".to_string()])
        .await?;

    // Incompatible clients should NOT be registered
    assert_eq!(server.client_count().await, 0);

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn feature_negotiation_compatible_client_registered() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let _result = server
        .negotiate_features("1.0.0", &["device_management".to_string()])
        .await?;

    assert_eq!(server.client_count().await, 1);

    server.stop().await?;
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// 3. Backward compatibility (newer server, older client messages)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn backward_compat_legacy_device_info_decoded_by_full_schema() -> TestResult {
    let codec = MessageCodec::new();

    // An older client that only populates id, name, device_type
    let legacy = LegacyDeviceInfo {
        id: "wheel-001".to_string(),
        name: "SimuCUBE 2".to_string(),
        device_type: 1,
    };
    let bytes: Vec<u8> = codec.encode(&legacy)?;

    // Newer server decodes with full DeviceInfo (includes `state`)
    let full: ProtoDeviceInfo = codec.decode(&bytes)?;
    assert_eq!(full.id, "wheel-001");
    assert_eq!(full.name, "SimuCUBE 2");
    assert_eq!(full.device_type, 1);
    // Missing field defaults to zero (protobuf default)
    assert_eq!(full.state, 0);

    Ok(())
}

#[test]
fn backward_compat_feature_request_without_namespace() -> TestResult {
    let codec = MessageCodec::new();

    // An older client that doesn't send `namespace` (tag 3)
    #[derive(Clone, PartialEq, prost::Message)]
    struct OldFeatureRequest {
        #[prost(string, tag = "1")]
        client_version: String,
        #[prost(string, repeated, tag = "2")]
        supported_features: Vec<String>,
    }

    let old_req = OldFeatureRequest {
        client_version: "1.0.0".to_string(),
        supported_features: vec!["device_management".to_string()],
    };
    let bytes: Vec<u8> = codec.encode(&old_req)?;

    // Server decodes with the full FeatureRequest schema
    let full_req: ProtoFeatureRequest = codec.decode(&bytes)?;
    assert_eq!(full_req.client_version, "1.0.0");
    assert_eq!(full_req.supported_features.len(), 1);
    // namespace defaults to empty string
    assert!(full_req.namespace.is_empty());

    Ok(())
}

#[test]
fn backward_compat_op_result_without_metadata() -> TestResult {
    let codec = MessageCodec::new();

    // Old-style OpResult that has no `metadata` map
    #[derive(Clone, PartialEq, prost::Message)]
    struct OldOpResult {
        #[prost(bool, tag = "1")]
        success: bool,
        #[prost(string, tag = "2")]
        error_message: String,
    }

    let old = OldOpResult {
        success: true,
        error_message: String::new(),
    };
    let bytes: Vec<u8> = codec.encode(&old)?;

    // Full OpResult includes metadata map
    #[derive(Clone, PartialEq, prost::Message)]
    struct FullOpResult {
        #[prost(bool, tag = "1")]
        success: bool,
        #[prost(string, tag = "2")]
        error_message: String,
        #[prost(btree_map = "string, string", tag = "3")]
        metadata: BTreeMap<String, String>,
    }

    let full: FullOpResult = codec.decode(&bytes)?;
    assert!(full.success);
    assert!(full.metadata.is_empty());

    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// 4. Forward compatibility (unknown fields gracefully ignored)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn forward_compat_unknown_fields_ignored_on_decode() -> TestResult {
    let codec = MessageCodec::new();

    // A future client sends a DeviceInfo with an extra field (tag 100)
    let future_msg = FutureDeviceInfo {
        id: "wheel-future".to_string(),
        name: "Future Wheel".to_string(),
        device_type: 1,
        state: 1,
        firmware_hash: "sha256:abcdef1234567890".to_string(),
    };
    let bytes: Vec<u8> = codec.encode(&future_msg)?;

    // Current server decodes with the current schema — unknown tag 100 is skipped
    let current: ProtoDeviceInfo = codec.decode(&bytes)?;
    assert_eq!(current.id, "wheel-future");
    assert_eq!(current.name, "Future Wheel");
    assert_eq!(current.device_type, 1);
    assert_eq!(current.state, 1);
    // firmware_hash is silently dropped

    Ok(())
}

#[test]
fn forward_compat_future_feature_request_with_extra_fields() -> TestResult {
    let codec = MessageCodec::new();

    // Future client sends negotiation request with extra tags
    #[derive(Clone, PartialEq, prost::Message)]
    struct FutureFeatureRequest {
        #[prost(string, tag = "1")]
        client_version: String,
        #[prost(string, repeated, tag = "2")]
        supported_features: Vec<String>,
        #[prost(string, tag = "3")]
        namespace: String,
        /// Hypothetical future field
        #[prost(string, tag = "50")]
        auth_token: String,
        /// Another hypothetical future field
        #[prost(uint32, tag = "51")]
        protocol_revision: u32,
    }

    let future_req = FutureFeatureRequest {
        client_version: "2.5.0".to_string(),
        supported_features: vec!["device_management".to_string()],
        namespace: "wheel.v2".to_string(),
        auth_token: "bearer_xyz_123".to_string(),
        protocol_revision: 42,
    };
    let bytes: Vec<u8> = codec.encode(&future_req)?;

    // Current server decodes with current schema
    let current: ProtoFeatureRequest = codec.decode(&bytes)?;
    assert_eq!(current.client_version, "2.5.0");
    assert_eq!(current.supported_features.len(), 1);
    assert_eq!(current.namespace, "wheel.v2");
    // auth_token and protocol_revision are silently ignored

    Ok(())
}

#[test]
fn forward_compat_extra_bytes_after_valid_header() -> TestResult {
    // MessageHeader::decode should succeed even with trailing bytes
    let header = MessageHeader::new(message_types::DEVICE, 256, 1);
    let mut buffer = header.encode().to_vec();
    // Append junk bytes (simulating header from a future protocol revision)
    buffer.extend_from_slice(&[0xFF, 0xFE, 0xFD, 0xFC]);

    let decoded = MessageHeader::decode(&buffer)?;
    assert_eq!(decoded.message_type, message_types::DEVICE);
    assert_eq!(decoded.payload_len, 256);
    assert_eq!(decoded.sequence, 1);

    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// 5. Wire format stability (snapshot tests for serialized protobuf)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn wire_format_feature_request_deterministic() -> TestResult {
    let req = ProtoFeatureRequest {
        client_version: "1.0.0".to_string(),
        supported_features: vec![
            "device_management".to_string(),
            "safety_control".to_string(),
        ],
        namespace: "wheel.v1".to_string(),
    };

    let mut buf = Vec::new();
    req.encode(&mut buf)
        .map_err(|e| format!("encode failed: {e}"))?;

    // Same message should always produce the same bytes
    let mut buf2 = Vec::new();
    req.encode(&mut buf2)
        .map_err(|e| format!("encode failed: {e}"))?;
    assert_eq!(buf, buf2, "Protobuf encoding must be deterministic");

    // Verify round-trip
    let decoded =
        ProtoFeatureRequest::decode(buf.as_slice()).map_err(|e| format!("decode failed: {e}"))?;
    assert_eq!(decoded, req);

    Ok(())
}

#[test]
fn wire_format_feature_response_deterministic() -> TestResult {
    let resp = ProtoFeatureResponse {
        server_version: "1.0.0".to_string(),
        supported_features: vec![
            "device_management".to_string(),
            "profile_management".to_string(),
        ],
        enabled_features: vec!["device_management".to_string()],
        compatible: true,
        min_client_version: "1.0.0".to_string(),
    };

    let mut buf = Vec::new();
    resp.encode(&mut buf)
        .map_err(|e| format!("encode failed: {e}"))?;

    let mut buf2 = Vec::new();
    resp.encode(&mut buf2)
        .map_err(|e| format!("encode failed: {e}"))?;
    assert_eq!(buf, buf2);

    let decoded =
        ProtoFeatureResponse::decode(buf.as_slice()).map_err(|e| format!("decode failed: {e}"))?;
    assert_eq!(decoded, resp);

    Ok(())
}

#[test]
fn wire_format_message_header_is_12_bytes() {
    assert_eq!(MessageHeader::SIZE, 12);
    let header = MessageHeader::new(message_types::FEATURE_NEGOTIATION, 100, 42);
    let encoded = header.encode();
    assert_eq!(encoded.len(), 12);
}

#[test]
fn wire_format_header_little_endian() -> TestResult {
    let header = MessageHeader::new(0x1234, 0xAABBCCDD, 0x11223344);
    let bytes = header.encode();

    // message_type: u16 little-endian
    assert_eq!(bytes[0], 0x34);
    assert_eq!(bytes[1], 0x12);

    // payload_len: u32 little-endian
    assert_eq!(bytes[2], 0xDD);
    assert_eq!(bytes[3], 0xCC);
    assert_eq!(bytes[4], 0xBB);
    assert_eq!(bytes[5], 0xAA);

    // sequence: u32 little-endian
    assert_eq!(bytes[6], 0x44);
    assert_eq!(bytes[7], 0x33);
    assert_eq!(bytes[8], 0x22);
    assert_eq!(bytes[9], 0x11);

    Ok(())
}

#[test]
fn wire_format_snapshot_feature_negotiation_header() -> TestResult {
    let mut header = MessageHeader::new(message_types::FEATURE_NEGOTIATION, 64, 1);
    header.set_flag(message_flags::REQUIRES_ACK);

    let bytes = header.encode();
    insta::assert_snapshot!(
        "compat_feature_negotiation_header",
        format!("{:02x?}", bytes.as_slice())
    );

    Ok(())
}

#[test]
fn wire_format_snapshot_device_id_proto() -> TestResult {
    let msg = ProtoDeviceId {
        id: "test-device-001".to_string(),
    };

    let mut buf = Vec::new();
    msg.encode(&mut buf)
        .map_err(|e| format!("encode failed: {e}"))?;

    insta::assert_snapshot!("compat_device_id_wire", format!("{:02x?}", buf.as_slice()));

    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// 6. Error handling (version mismatch, clear error messages)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn error_version_incompatibility_contains_both_versions() {
    let err = IpcError::VersionIncompatibility {
        client: "2.0.0".to_string(),
        server: "1.0.0".to_string(),
    };
    let msg = format!("{err}");
    assert!(msg.contains("2.0.0"), "Error should contain client version");
    assert!(msg.contains("1.0.0"), "Error should contain server version");
}

#[test]
fn error_version_incompatibility_is_recoverable() {
    let err = IpcError::VersionIncompatibility {
        client: "2.0.0".to_string(),
        server: "1.0.0".to_string(),
    };
    assert!(err.is_recoverable());
    assert!(!err.is_fatal());
}

#[test]
fn error_feature_negotiation_is_recoverable() {
    let err = IpcError::FeatureNegotiation("unsupported feature set".to_string());
    assert!(err.is_recoverable());
    assert!(!err.is_fatal());
}

#[test]
fn error_feature_negotiation_contains_message() {
    let err = IpcError::FeatureNegotiation("client requires vr_support".to_string());
    let msg = format!("{err}");
    assert!(msg.contains("vr_support"));
}

#[test]
fn error_timeout_during_negotiation() {
    let err = IpcError::timeout(5000);
    assert!(err.is_recoverable());
    let msg = format!("{err}");
    assert!(msg.contains("5000"));
}

#[test]
fn error_server_not_running_is_fatal() {
    let err = IpcError::ServerNotRunning;
    assert!(err.is_fatal());
    assert!(!err.is_recoverable());
}

#[test]
fn error_encoding_large_message_has_context() {
    let codec = MessageCodec::with_max_size(64);

    let big = ProtoDeviceInfo {
        id: "x".repeat(100),
        name: "y".repeat(100),
        device_type: 1,
        state: 1,
    };

    let result: IpcResult<Vec<u8>> = codec.encode(&big);
    assert!(result.is_err());
    if let Err(IpcError::EncodingFailed(msg)) = result {
        assert!(
            msg.contains("exceeds"),
            "Error should mention size exceeded"
        );
    }
}

#[test]
fn error_decoding_corrupted_protobuf() {
    let codec = MessageCodec::new();
    // Not valid protobuf (random bytes but not empty)
    let garbage = vec![0xFF, 0xFE, 0xFD, 0xFC, 0xFB, 0xFA];
    let result: IpcResult<ProtoDeviceId> = codec.decode(&garbage);
    assert!(result.is_err());
}

#[test]
fn error_decoding_empty_buffer() {
    let codec = MessageCodec::new();
    let result: IpcResult<ProtoDeviceId> = codec.decode(&[]);
    assert!(result.is_err());
}

// ════════════════════════════════════════════════════════════════════════════
// 7. Graceful degradation (reduced features when not supported)
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn degradation_partial_feature_set() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    // Client only supports a subset of features
    let result = server
        .negotiate_features("1.0.0", &["device_management".to_string()])
        .await?;

    assert!(result.compatible);
    // Only 1 feature enabled out of all available
    assert_eq!(result.enabled_features.len(), 1);
    assert!(result.supported_features.len() > 1);

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn degradation_no_matching_features() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    // Client requests only future features that don't exist yet
    let result = server
        .negotiate_features(
            "1.0.0",
            &[
                "ai_driving_assist".to_string(),
                "cloud_telemetry".to_string(),
            ],
        )
        .await?;

    // Still compatible (version matches) but no features enabled
    assert!(result.compatible);
    assert!(result.enabled_features.is_empty());

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn degradation_streaming_only_client() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server
        .negotiate_features(
            "1.0.0",
            &[
                "streaming_health".to_string(),
                "streaming_devices".to_string(),
            ],
        )
        .await?;

    assert!(result.compatible);
    assert_eq!(result.enabled_features.len(), 2);
    assert!(
        result
            .enabled_features
            .contains(&"streaming_health".to_string())
    );
    assert!(
        result
            .enabled_features
            .contains(&"streaming_devices".to_string())
    );

    server.stop().await?;
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// 8. Connection lifecycle
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn lifecycle_connect_negotiate_disconnect() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());

    // Start
    assert_eq!(server.state().await, ServerState::Stopped);
    server.start().await?;
    assert_eq!(server.state().await, ServerState::Running);
    assert!(server.is_running().await);

    // Negotiate
    let result = server
        .negotiate_features("1.0.0", &["device_management".to_string()])
        .await?;
    assert!(result.compatible);
    assert_eq!(server.client_count().await, 1);

    // Stop (all clients cleared)
    server.stop().await?;
    assert_eq!(server.state().await, ServerState::Stopped);
    assert!(!server.is_running().await);
    assert_eq!(server.client_count().await, 0);

    Ok(())
}

#[tokio::test]
async fn lifecycle_start_stop_restart() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());

    // First cycle
    server.start().await?;
    assert!(server.is_running().await);
    server.stop().await?;
    assert!(!server.is_running().await);

    // Second cycle (restart)
    server.start().await?;
    assert!(server.is_running().await);

    // Should accept new negotiation after restart
    let result = server
        .negotiate_features("1.0.0", &["safety_control".to_string()])
        .await?;
    assert!(result.compatible);

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn lifecycle_double_start_returns_error() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server.start().await;
    assert!(result.is_err());

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn lifecycle_double_stop_is_ok() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;
    server.stop().await?;

    // Second stop should be a no-op, not an error
    let result = server.stop().await;
    assert!(result.is_ok());

    Ok(())
}

#[tokio::test]
async fn lifecycle_multiple_clients_connect() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    // Three distinct negotiations
    for i in 0..3 {
        let _result = server
            .negotiate_features("1.0.0", &["device_management".to_string()])
            .await?;
        assert_eq!(server.client_count().await, i + 1);
    }

    // Verify all 3 clients registered
    let clients = server.connected_clients().await;
    assert_eq!(clients.len(), 3);

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn lifecycle_register_unregister_client() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let client = ClientInfo {
        id: "manual-client-1".to_string(),
        connected_at: std::time::Instant::now(),
        version: "1.0.0".to_string(),
        features: vec!["device_management".to_string()],
        peer_info: PeerInfo::default(),
    };

    server.register_client(client).await;
    assert_eq!(server.client_count().await, 1);

    server.unregister_client("manual-client-1").await;
    assert_eq!(server.client_count().await, 0);

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn lifecycle_health_event_broadcast_after_negotiation() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let mut rx = server.subscribe_health();

    let _result = server
        .negotiate_features("1.0.0", &["health_monitoring".to_string()])
        .await?;

    // Broadcast a health event
    let event = HealthEvent {
        timestamp: std::time::SystemTime::now(),
        device_id: "wheel-001".to_string(),
        event_type: HealthEventType::Connected,
        message: "Device connected".to_string(),
        metadata: std::collections::HashMap::new(),
    };
    server.broadcast_health_event(event);

    let received = rx.try_recv();
    assert!(received.is_ok());

    server.stop().await?;
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// 9. Protobuf wire format roundtrip (codec layer)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn codec_roundtrip_feature_request() -> TestResult {
    let codec = MessageCodec::new();

    let req = ProtoFeatureRequest {
        client_version: "1.2.3".to_string(),
        supported_features: vec![
            "device_management".to_string(),
            "safety_control".to_string(),
            "health_monitoring".to_string(),
        ],
        namespace: "wheel.v1".to_string(),
    };

    let bytes: Vec<u8> = codec.encode(&req)?;
    let decoded: ProtoFeatureRequest = codec.decode(&bytes)?;
    assert_eq!(decoded, req);

    Ok(())
}

#[test]
fn codec_roundtrip_feature_response() -> TestResult {
    let codec = MessageCodec::new();

    let resp = ProtoFeatureResponse {
        server_version: "1.0.0".to_string(),
        supported_features: vec!["device_management".to_string()],
        enabled_features: vec!["device_management".to_string()],
        compatible: true,
        min_client_version: "1.0.0".to_string(),
    };

    let bytes: Vec<u8> = codec.encode(&resp)?;
    let decoded: ProtoFeatureResponse = codec.decode(&bytes)?;
    assert_eq!(decoded, resp);

    Ok(())
}

#[test]
fn codec_roundtrip_device_info() -> TestResult {
    let codec = MessageCodec::new();

    let info = ProtoDeviceInfo {
        id: "wheel-abc".to_string(),
        name: "Fanatec DD1".to_string(),
        device_type: 1,
        state: 1,
    };

    let bytes: Vec<u8> = codec.encode(&info)?;
    let decoded: ProtoDeviceInfo = codec.decode(&bytes)?;
    assert_eq!(decoded, info);

    Ok(())
}

#[test]
fn codec_rejects_default_proto3_message_with_zero_encoded_len() {
    let codec = MessageCodec::new();

    // A default proto3 message with all-empty/zero fields encodes to 0 bytes.
    // The codec correctly rejects zero-length messages.
    let req = ProtoFeatureRequest {
        client_version: String::new(),
        supported_features: vec![],
        namespace: String::new(),
    };

    let result: IpcResult<Vec<u8>> = codec.encode(&req);
    assert!(result.is_err(), "Codec should reject zero-length encoding");
}

// ════════════════════════════════════════════════════════════════════════════
// 10. Proptest: fuzz protobuf encoding roundtrips
// ════════════════════════════════════════════════════════════════════════════

mod proptest_compat {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        #[test]
        fn prop_feature_request_roundtrip(
            version in "[0-9]{1,3}\\.[0-9]{1,3}\\.[0-9]{1,3}",
            features in prop::collection::vec("[a-z_]{1,30}", 0..10),
            namespace in "[a-z.]{0,20}",
        ) {
            let codec = MessageCodec::new();
            let req = ProtoFeatureRequest {
                client_version: version,
                supported_features: features,
                namespace,
            };

            let bytes = codec.encode(&req)
                .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
            let decoded: ProtoFeatureRequest = codec.decode(&bytes)
                .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
            prop_assert_eq!(decoded, req);
        }

        #[test]
        fn prop_feature_response_roundtrip(
            version in "[0-9]{1,3}\\.[0-9]{1,3}\\.[0-9]{1,3}",
            supported in prop::collection::vec("[a-z_]{1,20}", 0..8),
            enabled in prop::collection::vec("[a-z_]{1,20}", 0..4),
            compatible in any::<bool>(),
            min_ver in "[0-9]{1,3}\\.[0-9]{1,3}\\.[0-9]{1,3}",
        ) {
            let codec = MessageCodec::new();
            let resp = ProtoFeatureResponse {
                server_version: version,
                supported_features: supported,
                enabled_features: enabled,
                compatible,
                min_client_version: min_ver,
            };

            let bytes = codec.encode(&resp)
                .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
            let decoded: ProtoFeatureResponse = codec.decode(&bytes)
                .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
            prop_assert_eq!(decoded, resp);
        }

        #[test]
        fn prop_device_info_roundtrip(
            id in "[a-z0-9-]{1,50}",
            name in ".{0,100}",
            device_type in 0i32..10,
            state in 0i32..5,
        ) {
            let codec = MessageCodec::new();
            let info = ProtoDeviceInfo {
                id, name, device_type, state,
            };

            let bytes = codec.encode(&info)
                .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
            let decoded: ProtoDeviceInfo = codec.decode(&bytes)
                .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
            prop_assert_eq!(decoded, info);
        }

        #[test]
        fn prop_version_compat_reflexive(
            major in 0u32..10,
            minor in 0u32..100,
            patch in 0u32..100,
        ) {
            let v = format!("{major}.{minor}.{patch}");
            prop_assert!(
                is_version_compatible(&v, &v),
                "Version '{v}' should be compatible with itself"
            );
        }

        #[test]
        fn prop_version_compat_higher_minor_always_ok(
            major in 1u32..5,
            min_minor in 0u32..50,
            delta in 1u32..50,
            patch in 0u32..50,
        ) {
            let client = format!("{major}.{}.{patch}", min_minor + delta);
            let min_ver = format!("{major}.{min_minor}.0");
            prop_assert!(
                is_version_compatible(&client, &min_ver),
                "Client '{client}' should be compat with min '{min_ver}'"
            );
        }

        #[test]
        fn prop_version_compat_different_major_never_ok(
            c_major in 0u32..10,
            s_major in 0u32..10,
            minor in 0u32..50,
            patch in 0u32..50,
        ) {
            prop_assume!(c_major != s_major);
            let client = format!("{c_major}.{minor}.{patch}");
            let min_ver = format!("{s_major}.0.0");
            prop_assert!(
                !is_version_compatible(&client, &min_ver),
                "Different majors '{client}' vs '{min_ver}' must be incompatible"
            );
        }

        #[test]
        fn prop_message_header_roundtrip_compat(
            msg_type in any::<u16>(),
            payload_len in any::<u32>(),
            seq in any::<u32>(),
            flags in any::<u16>(),
        ) {
            let mut header = MessageHeader::new(msg_type, payload_len, seq);
            header.flags = flags;

            let encoded = header.encode();
            let decoded = MessageHeader::decode(&encoded)
                .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

            prop_assert_eq!(header.message_type, decoded.message_type);
            prop_assert_eq!(header.payload_len, decoded.payload_len);
            prop_assert_eq!(header.sequence, decoded.sequence);
            prop_assert_eq!(header.flags, decoded.flags);
        }

        #[test]
        fn prop_random_bytes_dont_panic_on_decode(
            data in prop::collection::vec(any::<u8>(), 1..256),
        ) {
            let codec = MessageCodec::new();
            // Should not panic — either Ok or Err
            let _result: IpcResult<ProtoDeviceId> = codec.decode(&data);
        }

        #[test]
        fn prop_random_bytes_dont_panic_on_header_decode(
            data in prop::collection::vec(any::<u8>(), 0..64),
        ) {
            // Should not panic — either Ok or Err
            let _result = MessageHeader::decode(&data);
        }
    }
}
