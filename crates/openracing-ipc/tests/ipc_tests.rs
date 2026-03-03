//! Comprehensive tests for openracing-ipc crate
//!
//! Covers serde round-trips, codec encode/decode, message validation,
//! error type handling, and connection state management types.

// ── Serde round-trips for IPC configuration types ────────────────────────

mod serde_roundtrip {
    use std::time::Duration;

    use openracing_ipc::server::IpcConfig;
    use openracing_ipc::transport::{TransportBuilder, TransportConfig, TransportType};

    #[test]
    fn transport_type_tcp_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let original = TransportType::tcp();
        let json = serde_json::to_string(&original)?;
        let deserialized: TransportType = serde_json::from_str(&json)?;

        match deserialized {
            TransportType::Tcp { address, port } => {
                assert_eq!(address, "127.0.0.1");
                assert_eq!(port, 50051);
            }
            #[allow(unreachable_patterns)]
            _ => return Err("Expected TCP transport after deserialization".into()),
        }
        Ok(())
    }

    #[test]
    fn transport_type_tcp_custom_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let original = TransportType::tcp_with_address("192.168.1.100", 9090);
        let json = serde_json::to_string(&original)?;
        let deserialized: TransportType = serde_json::from_str(&json)?;

        match deserialized {
            TransportType::Tcp { address, port } => {
                assert_eq!(address, "192.168.1.100");
                assert_eq!(port, 9090);
            }
            #[allow(unreachable_patterns)]
            _ => return Err("Expected TCP transport after deserialization".into()),
        }
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn transport_type_named_pipe_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let original = TransportType::named_pipe(r"\\.\pipe\test-ipc");
        let json = serde_json::to_string(&original)?;
        let deserialized: TransportType = serde_json::from_str(&json)?;

        match deserialized {
            TransportType::NamedPipe { pipe_name } => {
                assert_eq!(pipe_name, r"\\.\pipe\test-ipc");
            }
            _ => return Err("Expected NamedPipe transport after deserialization".into()),
        }
        Ok(())
    }

    #[test]
    fn transport_config_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let original = TransportBuilder::new()
            .transport(TransportType::tcp_with_address("10.0.0.1", 8080))
            .max_connections(200)
            .connection_timeout(Duration::from_secs(60))
            .enable_acl(true)
            .build();

        let json = serde_json::to_string(&original)?;
        let deserialized: TransportConfig = serde_json::from_str(&json)?;

        assert_eq!(deserialized.max_connections, 200);
        assert_eq!(deserialized.connection_timeout, Duration::from_secs(60));
        assert!(deserialized.enable_acl);
        assert_eq!(deserialized.recv_buffer_size, 64 * 1024);
        assert_eq!(deserialized.send_buffer_size, 64 * 1024);
        Ok(())
    }

    #[test]
    fn transport_config_default_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let original = TransportConfig::default();
        let json = serde_json::to_string(&original)?;
        let deserialized: TransportConfig = serde_json::from_str(&json)?;

        assert_eq!(deserialized.max_connections, original.max_connections);
        assert_eq!(
            deserialized.connection_timeout,
            original.connection_timeout
        );
        assert_eq!(deserialized.enable_acl, original.enable_acl);
        Ok(())
    }

    #[test]
    fn ipc_config_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let original = IpcConfig::with_transport(TransportType::tcp())
            .max_connections(50)
            .health_buffer_size(2000);

        let json = serde_json::to_string(&original)?;
        let deserialized: IpcConfig = serde_json::from_str(&json)?;

        assert_eq!(deserialized.server_name, "openracing-ipc");
        assert_eq!(deserialized.transport.max_connections, 50);
        assert_eq!(deserialized.health_buffer_size, 2000);
        assert!(deserialized.enable_connection_logging);
        Ok(())
    }

    #[test]
    fn ipc_config_default_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let original = IpcConfig::default();
        let json = serde_json::to_string(&original)?;
        let deserialized: IpcConfig = serde_json::from_str(&json)?;

        assert_eq!(deserialized.server_name, original.server_name);
        assert_eq!(deserialized.health_buffer_size, original.health_buffer_size);
        assert_eq!(
            deserialized.enable_connection_logging,
            original.enable_connection_logging
        );
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn platform_default_transport_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let original = TransportType::platform_default();
        let json = serde_json::to_string(&original)?;
        let deserialized: TransportType = serde_json::from_str(&json)?;

        // On Windows, platform default is NamedPipe
        match deserialized {
            TransportType::NamedPipe { pipe_name } => {
                assert!(pipe_name.contains("openracing"));
            }
            _ => return Err("Expected NamedPipe transport on Windows".into()),
        }
        Ok(())
    }
}

// ── Codec encode/decode and message validation ───────────────────────────

mod codec_validation {
    use openracing_ipc::codec::{
        MessageCodec, MessageDecoder, MessageEncoder, MessageHeader, message_flags, message_types,
    };

    #[test]
    fn header_roundtrip_all_message_types() -> Result<(), Box<dyn std::error::Error>> {
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

        for (seq, &msg_type) in types.iter().enumerate() {
            let header = MessageHeader::new(msg_type, 256, seq as u32);
            let encoded = header.encode();
            let decoded = MessageHeader::decode(&encoded)?;

            assert_eq!(decoded.message_type, msg_type);
            assert_eq!(decoded.payload_len, 256);
            assert_eq!(decoded.sequence, seq as u32);
            assert_eq!(decoded.flags, 0);
        }
        Ok(())
    }

    #[test]
    fn header_roundtrip_all_flags() -> Result<(), Box<dyn std::error::Error>> {
        let flags = [
            message_flags::COMPRESSED,
            message_flags::REQUIRES_ACK,
            message_flags::IS_RESPONSE,
            message_flags::IS_ERROR,
            message_flags::STREAMING,
        ];

        for &flag in &flags {
            let mut header = MessageHeader::new(message_types::DEVICE, 100, 0);
            header.set_flag(flag);

            let encoded = header.encode();
            let decoded = MessageHeader::decode(&encoded)?;

            assert!(decoded.has_flag(flag));
        }
        Ok(())
    }

    #[test]
    fn header_combined_flags_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let mut header = MessageHeader::new(message_types::SAFETY, 512, 99);
        header.set_flag(message_flags::COMPRESSED);
        header.set_flag(message_flags::REQUIRES_ACK);
        header.set_flag(message_flags::IS_RESPONSE);

        let encoded = header.encode();
        let decoded = MessageHeader::decode(&encoded)?;

        assert!(decoded.has_flag(message_flags::COMPRESSED));
        assert!(decoded.has_flag(message_flags::REQUIRES_ACK));
        assert!(decoded.has_flag(message_flags::IS_RESPONSE));
        assert!(!decoded.has_flag(message_flags::IS_ERROR));
        assert!(!decoded.has_flag(message_flags::STREAMING));
        Ok(())
    }

    #[test]
    fn header_boundary_values() -> Result<(), Box<dyn std::error::Error>> {
        // Max values
        let header = MessageHeader::new(u16::MAX, u32::MAX, u32::MAX);
        let encoded = header.encode();
        let decoded = MessageHeader::decode(&encoded)?;

        assert_eq!(decoded.message_type, u16::MAX);
        assert_eq!(decoded.payload_len, u32::MAX);
        assert_eq!(decoded.sequence, u32::MAX);

        // Min values (zeros)
        let header = MessageHeader::new(0, 0, 0);
        let encoded = header.encode();
        let decoded = MessageHeader::decode(&encoded)?;

        assert_eq!(decoded.message_type, 0);
        assert_eq!(decoded.payload_len, 0);
        assert_eq!(decoded.sequence, 0);

        Ok(())
    }

    #[test]
    fn header_size_constant() {
        assert_eq!(MessageHeader::SIZE, 12);

        let header = MessageHeader::new(message_types::DEVICE, 100, 1);
        let encoded = header.encode();
        assert_eq!(encoded.len(), MessageHeader::SIZE);
    }

    #[test]
    fn header_decode_empty_bytes() {
        let result = MessageHeader::decode(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn header_decode_exactly_size_bytes() -> Result<(), Box<dyn std::error::Error>> {
        let bytes = [0u8; MessageHeader::SIZE];
        let decoded = MessageHeader::decode(&bytes)?;
        assert_eq!(decoded.message_type, 0);
        assert_eq!(decoded.payload_len, 0);
        assert_eq!(decoded.sequence, 0);
        assert_eq!(decoded.flags, 0);
        Ok(())
    }

    #[test]
    fn header_decode_extra_bytes_ignored() -> Result<(), Box<dyn std::error::Error>> {
        let mut bytes = [0u8; 20]; // More than SIZE
        // Set message_type = 1 in LE
        bytes[0] = 1;

        let decoded = MessageHeader::decode(&bytes)?;
        assert_eq!(decoded.message_type, 1);
        Ok(())
    }

    #[test]
    fn codec_rejects_zero_size_message() {
        let codec = MessageCodec::with_max_size(1024);
        assert!(!codec.is_valid_size(0));
    }

    #[test]
    fn codec_accepts_exact_max_size() {
        let codec = MessageCodec::with_max_size(100);
        assert!(codec.is_valid_size(100));
        assert!(!codec.is_valid_size(101));
    }

    #[test]
    fn codec_with_max_size_one() {
        let codec = MessageCodec::with_max_size(1);
        assert!(codec.is_valid_size(1));
        assert!(!codec.is_valid_size(2));
        assert!(!codec.is_valid_size(0));
    }

    #[test]
    fn prost_encode_decode_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        // Use prost_types::Timestamp as a real protobuf message
        let original = prost_types::Timestamp {
            seconds: 1_700_000_000,
            nanos: 123_456_789,
        };

        let codec = MessageCodec::new();
        let encoded = codec.encode(&original)?;
        let decoded: prost_types::Timestamp = codec.decode(&encoded)?;

        assert_eq!(decoded.seconds, original.seconds);
        assert_eq!(decoded.nanos, original.nanos);
        Ok(())
    }

    #[test]
    fn prost_encode_to_buffer_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let original = prost_types::Duration {
            seconds: 300,
            nanos: 500_000,
        };

        let codec = MessageCodec::new();
        let mut buffer = Vec::new();
        codec.encode_to_buffer(&original, &mut buffer)?;

        let decoded: prost_types::Duration = codec.decode(&buffer)?;
        assert_eq!(decoded.seconds, original.seconds);
        assert_eq!(decoded.nanos, original.nanos);
        Ok(())
    }

    #[test]
    fn prost_encode_to_buffer_clears_existing() -> Result<(), Box<dyn std::error::Error>> {
        let codec = MessageCodec::new();
        let mut buffer = vec![0xFF; 100]; // pre-fill with junk

        let msg = prost_types::Timestamp {
            seconds: 42,
            nanos: 0,
        };
        codec.encode_to_buffer(&msg, &mut buffer)?;

        let decoded: prost_types::Timestamp = codec.decode(&buffer)?;
        assert_eq!(decoded.seconds, 42);
        Ok(())
    }

    #[test]
    fn codec_rejects_oversized_encode() {
        // Use a tiny max size that a real message will exceed
        let codec = MessageCodec::with_max_size(1);
        let msg = prost_types::Timestamp {
            seconds: 1_700_000_000,
            nanos: 123_456_789,
        };

        let result = codec.encode(&msg);
        assert!(result.is_err());
        if let Err(e) = result {
            let msg = format!("{e}");
            assert!(msg.contains("exceeds maximum"));
        }
    }

    #[test]
    fn codec_rejects_oversized_decode() {
        let codec = MessageCodec::with_max_size(2);
        let bytes = [0u8; 100];

        let result: Result<prost_types::Timestamp, _> = codec.decode(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn codec_encoded_len() {
        let codec = MessageCodec::new();
        let msg = prost_types::Timestamp {
            seconds: 100,
            nanos: 200,
        };

        let len = codec.encoded_len(&msg);
        assert!(len > 0);

        // encoded_len should match actual encoded size
        let encoded = codec.encode(&msg);
        assert!(encoded.is_ok());
        if let Ok(bytes) = encoded {
            assert_eq!(bytes.len(), len);
        }
    }

    #[test]
    fn codec_decode_invalid_protobuf() {
        let codec = MessageCodec::new();
        // Random bytes that aren't valid protobuf for Timestamp
        // prost is lenient, so use a very specific invalid encoding
        // Actually prost may accept almost anything; test with corrupt varint
        let bytes = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x02];

        let result: Result<prost_types::Timestamp, _> = codec.decode(&bytes);
        assert!(result.is_err());
    }
}

// ── Error type handling ──────────────────────────────────────────────────

mod error_handling {
    use openracing_ipc::error::{IpcError, IpcResult};

    #[test]
    fn all_error_variants_display() {
        let errors: Vec<IpcError> = vec![
            IpcError::TransportInit("init failed".to_string()),
            IpcError::ConnectionFailed("conn refused".to_string()),
            IpcError::EncodingFailed("bad encode".to_string()),
            IpcError::DecodingFailed("bad decode".to_string()),
            IpcError::VersionIncompatibility {
                client: "0.5.0".to_string(),
                server: "1.0.0".to_string(),
            },
            IpcError::FeatureNegotiation("missing feature".to_string()),
            IpcError::ServerNotRunning,
            IpcError::ConnectionLimitExceeded { max: 10 },
            IpcError::Timeout { timeout_ms: 5000 },
            IpcError::Grpc("rpc failed".to_string()),
            IpcError::InvalidConfig("bad config".to_string()),
            IpcError::PlatformNotSupported("BeOS".to_string()),
            IpcError::ShutdownRequested,
        ];

        for err in &errors {
            let display = format!("{err}");
            assert!(!display.is_empty(), "Display should not be empty");
        }
    }

    #[test]
    fn error_display_contains_context() {
        let err = IpcError::TransportInit("socket bind failed".to_string());
        assert!(format!("{err}").contains("socket bind failed"));

        let err = IpcError::ConnectionFailed("ECONNREFUSED".to_string());
        assert!(format!("{err}").contains("ECONNREFUSED"));

        let err = IpcError::EncodingFailed("buffer overflow".to_string());
        assert!(format!("{err}").contains("buffer overflow"));

        let err = IpcError::DecodingFailed("truncated".to_string());
        assert!(format!("{err}").contains("truncated"));

        let err = IpcError::Timeout { timeout_ms: 3000 };
        assert!(format!("{err}").contains("3000"));

        let err = IpcError::ConnectionLimitExceeded { max: 42 };
        assert!(format!("{err}").contains("42"));

        let err = IpcError::PlatformNotSupported("plan9".to_string());
        assert!(format!("{err}").contains("plan9"));

        let err = IpcError::Grpc("unavailable".to_string());
        assert!(format!("{err}").contains("unavailable"));
    }

    #[test]
    fn error_version_incompatibility_display() {
        let err = IpcError::VersionIncompatibility {
            client: "0.9.0".to_string(),
            server: "1.0.0".to_string(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("0.9.0"));
        assert!(msg.contains("1.0.0"));
    }

    #[test]
    fn error_io_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe broken");
        let ipc_err: IpcError = io_err.into();

        assert!(matches!(ipc_err, IpcError::Io(_)));
        assert!(format!("{ipc_err}").contains("pipe broken"));
    }

    #[test]
    fn error_is_recoverable_comprehensive() {
        // Recoverable errors
        assert!(IpcError::ConnectionFailed("x".into()).is_recoverable());
        assert!(IpcError::Timeout { timeout_ms: 100 }.is_recoverable());
        assert!(IpcError::VersionIncompatibility {
            client: "0.1.0".into(),
            server: "1.0.0".into(),
        }
        .is_recoverable());
        assert!(IpcError::FeatureNegotiation("x".into()).is_recoverable());

        // Non-recoverable errors
        assert!(!IpcError::TransportInit("x".into()).is_recoverable());
        assert!(!IpcError::EncodingFailed("x".into()).is_recoverable());
        assert!(!IpcError::DecodingFailed("x".into()).is_recoverable());
        assert!(!IpcError::ServerNotRunning.is_recoverable());
        assert!(!IpcError::ConnectionLimitExceeded { max: 1 }.is_recoverable());
        assert!(!IpcError::Grpc("x".into()).is_recoverable());
        assert!(!IpcError::InvalidConfig("x".into()).is_recoverable());
        assert!(!IpcError::PlatformNotSupported("x".into()).is_recoverable());
        assert!(!IpcError::ShutdownRequested.is_recoverable());
    }

    #[test]
    fn error_is_fatal_comprehensive() {
        // Fatal errors
        assert!(IpcError::TransportInit("x".into()).is_fatal());
        assert!(IpcError::ServerNotRunning.is_fatal());
        assert!(IpcError::ShutdownRequested.is_fatal());

        // Non-fatal errors
        assert!(!IpcError::ConnectionFailed("x".into()).is_fatal());
        assert!(!IpcError::Timeout { timeout_ms: 100 }.is_fatal());
        assert!(!IpcError::EncodingFailed("x".into()).is_fatal());
        assert!(!IpcError::DecodingFailed("x".into()).is_fatal());
        assert!(!IpcError::VersionIncompatibility {
            client: "0.1.0".into(),
            server: "1.0.0".into(),
        }
        .is_fatal());
        assert!(!IpcError::FeatureNegotiation("x".into()).is_fatal());
        assert!(!IpcError::ConnectionLimitExceeded { max: 1 }.is_fatal());
        assert!(!IpcError::Grpc("x".into()).is_fatal());
        assert!(!IpcError::InvalidConfig("x".into()).is_fatal());
        assert!(!IpcError::PlatformNotSupported("x".into()).is_fatal());
    }

    #[test]
    fn error_recoverable_and_fatal_are_disjoint() {
        let errors: Vec<IpcError> = vec![
            IpcError::TransportInit("x".into()),
            IpcError::ConnectionFailed("x".into()),
            IpcError::EncodingFailed("x".into()),
            IpcError::DecodingFailed("x".into()),
            IpcError::VersionIncompatibility {
                client: "0.1.0".into(),
                server: "1.0.0".into(),
            },
            IpcError::FeatureNegotiation("x".into()),
            IpcError::ServerNotRunning,
            IpcError::ConnectionLimitExceeded { max: 1 },
            IpcError::Timeout { timeout_ms: 100 },
            IpcError::Grpc("x".into()),
            IpcError::InvalidConfig("x".into()),
            IpcError::PlatformNotSupported("x".into()),
            IpcError::ShutdownRequested,
        ];

        for err in &errors {
            assert!(
                !(err.is_recoverable() && err.is_fatal()),
                "Error {:?} should not be both recoverable and fatal",
                err
            );
        }
    }

    #[test]
    fn ipc_result_type_alias_works() -> IpcResult<()> {
        let ok_result: IpcResult<u32> = Ok(42);
        assert_eq!(ok_result?, 42);

        let err_result: IpcResult<u32> = Err(IpcError::ServerNotRunning);
        assert!(err_result.is_err());

        Ok(())
    }

    #[test]
    fn error_debug_impl() {
        let err = IpcError::Timeout { timeout_ms: 999 };
        let debug = format!("{err:?}");
        assert!(debug.contains("Timeout"));
        assert!(debug.contains("999"));
    }
}

// ── Connection state management types ────────────────────────────────────

mod connection_state {
    use std::collections::HashMap;
    use std::time::Instant;

    use openracing_ipc::error::IpcResult;
    use openracing_ipc::server::{
        ClientInfo, HealthEvent, HealthEventType, IpcConfig, IpcServer, PeerInfo, ServerState,
    };

    #[test]
    fn server_state_equality() {
        assert_eq!(ServerState::Stopped, ServerState::Stopped);
        assert_eq!(ServerState::Starting, ServerState::Starting);
        assert_eq!(ServerState::Running, ServerState::Running);
        assert_eq!(ServerState::ShuttingDown, ServerState::ShuttingDown);
        assert_ne!(ServerState::Stopped, ServerState::Running);
        assert_ne!(ServerState::Starting, ServerState::ShuttingDown);
    }

    #[test]
    fn server_state_debug() {
        let states = [
            ServerState::Stopped,
            ServerState::Starting,
            ServerState::Running,
            ServerState::ShuttingDown,
        ];
        for state in &states {
            let debug = format!("{state:?}");
            assert!(!debug.is_empty());
        }
    }

    #[test]
    fn health_event_type_values() {
        assert_eq!(HealthEventType::Connected as i32, 0);
        assert_eq!(HealthEventType::Disconnected as i32, 1);
        assert_eq!(HealthEventType::Fault as i32, 2);
        assert_eq!(HealthEventType::FaultCleared as i32, 3);
        assert_eq!(HealthEventType::TemperatureWarning as i32, 4);
        assert_eq!(HealthEventType::TemperatureCritical as i32, 5);
        assert_eq!(HealthEventType::ProfileChanged as i32, 6);
        assert_eq!(HealthEventType::HighTorqueEnabled as i32, 7);
        assert_eq!(HealthEventType::EmergencyStop as i32, 8);
    }

    #[test]
    fn health_event_type_equality() {
        assert_eq!(HealthEventType::Connected, HealthEventType::Connected);
        assert_ne!(HealthEventType::Connected, HealthEventType::Disconnected);
        assert_ne!(HealthEventType::Fault, HealthEventType::FaultCleared);
    }

    #[test]
    fn health_event_type_copy() {
        let a = HealthEventType::EmergencyStop;
        let b = a; // Copy
        assert_eq!(a, b);
    }

    #[test]
    fn health_event_construction() {
        let event = HealthEvent {
            timestamp: std::time::SystemTime::now(),
            device_id: "wheel-001".to_string(),
            event_type: HealthEventType::Fault,
            message: "Motor overcurrent".to_string(),
            metadata: HashMap::from([
                ("fault_code".to_string(), "E001".to_string()),
                ("current_ma".to_string(), "15000".to_string()),
            ]),
        };

        assert_eq!(event.device_id, "wheel-001");
        assert_eq!(event.event_type, HealthEventType::Fault);
        assert_eq!(event.metadata.len(), 2);
        assert_eq!(event.metadata.get("fault_code").map(|s| s.as_str()), Some("E001"));
    }

    #[test]
    fn health_event_clone() {
        let event = HealthEvent {
            timestamp: std::time::SystemTime::now(),
            device_id: "device-1".to_string(),
            event_type: HealthEventType::Connected,
            message: "connected".to_string(),
            metadata: HashMap::new(),
        };

        let cloned = event.clone();
        assert_eq!(cloned.device_id, event.device_id);
        assert_eq!(cloned.event_type, event.event_type);
    }

    #[test]
    fn client_info_construction() {
        let info = ClientInfo {
            id: "client-abc".to_string(),
            connected_at: Instant::now(),
            version: "1.2.3".to_string(),
            features: vec!["device_management".to_string(), "safety_control".to_string()],
            peer_info: PeerInfo::default(),
        };

        assert_eq!(info.id, "client-abc");
        assert_eq!(info.version, "1.2.3");
        assert_eq!(info.features.len(), 2);
    }

    #[test]
    fn peer_info_default() {
        let peer = PeerInfo::default();
        #[cfg(windows)]
        assert!(peer.process_id.is_none());
        #[cfg(unix)]
        {
            assert!(peer.uid.is_none());
            assert!(peer.gid.is_none());
        }
        // Ensure it compiles and has Debug
        let _debug = format!("{peer:?}");
    }

    #[tokio::test]
    async fn server_state_transitions() -> IpcResult<()> {
        let server = IpcServer::new(IpcConfig::default());

        // Initial state
        assert_eq!(server.state().await, ServerState::Stopped);
        assert!(!server.is_running().await);

        // Start
        server.start().await?;
        assert_eq!(server.state().await, ServerState::Running);
        assert!(server.is_running().await);

        // Stop
        server.stop().await?;
        assert_eq!(server.state().await, ServerState::Stopped);
        assert!(!server.is_running().await);

        Ok(())
    }

    #[tokio::test]
    async fn server_double_start_returns_error() -> Result<(), Box<dyn std::error::Error>> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let result = server.start().await;
        assert!(result.is_err());

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn server_stop_when_stopped_is_ok() -> IpcResult<()> {
        let server = IpcServer::new(IpcConfig::default());
        // Stopping a stopped server should be fine
        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn server_client_lifecycle() -> IpcResult<()> {
        let server = IpcServer::new(IpcConfig::default());
        assert_eq!(server.client_count().await, 0);

        let client = ClientInfo {
            id: "test-1".to_string(),
            connected_at: Instant::now(),
            version: "1.0.0".to_string(),
            features: vec![],
            peer_info: PeerInfo::default(),
        };
        server.register_client(client).await;
        assert_eq!(server.client_count().await, 1);

        let clients = server.connected_clients().await;
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0].id, "test-1");

        server.unregister_client("test-1").await;
        assert_eq!(server.client_count().await, 0);

        Ok(())
    }

    #[tokio::test]
    async fn unregister_nonexistent_client_is_noop() {
        let server = IpcServer::new(IpcConfig::default());
        server.unregister_client("does-not-exist").await;
        assert_eq!(server.client_count().await, 0);
    }

    #[tokio::test]
    async fn server_stop_clears_clients() -> IpcResult<()> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        // Add some clients via negotiation
        server
            .negotiate_features("1.0.0", &["device_management".to_string()])
            .await?;
        server
            .negotiate_features("1.0.0", &["safety_control".to_string()])
            .await?;
        assert_eq!(server.client_count().await, 2);

        server.stop().await?;
        assert_eq!(server.client_count().await, 0);

        Ok(())
    }

    #[tokio::test]
    async fn server_config_accessor() {
        let config = IpcConfig::default().max_connections(75).health_buffer_size(500);
        let server = IpcServer::new(config);

        assert_eq!(server.config().transport.max_connections, 75);
        assert_eq!(server.config().health_buffer_size, 500);
        assert_eq!(server.config().server_name, "openracing-ipc");
    }

    #[tokio::test]
    async fn health_event_subscribe_before_broadcast() -> Result<(), Box<dyn std::error::Error>> {
        let server = IpcServer::new(IpcConfig::default());
        let mut rx = server.subscribe_health();

        let event = HealthEvent {
            timestamp: std::time::SystemTime::now(),
            device_id: "dev-1".to_string(),
            event_type: HealthEventType::TemperatureWarning,
            message: "Motor temp 85°C".to_string(),
            metadata: HashMap::new(),
        };
        server.broadcast_health_event(event);

        let received = rx.try_recv()?;
        assert_eq!(received.device_id, "dev-1");
        assert_eq!(received.event_type, HealthEventType::TemperatureWarning);
        Ok(())
    }

    #[tokio::test]
    async fn health_event_multiple_subscribers() -> Result<(), Box<dyn std::error::Error>> {
        let server = IpcServer::new(IpcConfig::default());
        let mut rx1 = server.subscribe_health();
        let mut rx2 = server.subscribe_health();

        let event = HealthEvent {
            timestamp: std::time::SystemTime::now(),
            device_id: "dev-x".to_string(),
            event_type: HealthEventType::EmergencyStop,
            message: "E-stop triggered".to_string(),
            metadata: HashMap::new(),
        };
        server.broadcast_health_event(event);

        let r1 = rx1.try_recv()?;
        let r2 = rx2.try_recv()?;
        assert_eq!(r1.device_id, "dev-x");
        assert_eq!(r2.device_id, "dev-x");
        assert_eq!(r1.event_type, HealthEventType::EmergencyStop);
        assert_eq!(r2.event_type, HealthEventType::EmergencyStop);
        Ok(())
    }
}

// ── Feature negotiation and version compatibility ────────────────────────

mod feature_negotiation {
    use openracing_ipc::error::IpcResult;
    use openracing_ipc::server::{IpcConfig, IpcServer, is_version_compatible};
    use openracing_ipc::{MIN_CLIENT_VERSION, PROTOCOL_VERSION};

    #[test]
    fn version_compat_edge_cases() {
        // Empty string
        assert!(!is_version_compatible("", "1.0.0"));
        assert!(!is_version_compatible("1.0.0", ""));

        // Partial versions
        assert!(!is_version_compatible("1.0", "1.0.0"));
        assert!(!is_version_compatible("1", "1.0.0"));

        // Non-numeric
        assert!(!is_version_compatible("a.b.c", "1.0.0"));

        // Extra components are ignored (only first 3 parsed)
        assert!(is_version_compatible("1.0.0.1", "1.0.0"));
    }

    #[test]
    fn version_compat_major_zero() {
        // 0.x.y vs 0.x.y
        assert!(is_version_compatible("0.1.0", "0.1.0"));
        assert!(is_version_compatible("0.2.0", "0.1.0"));
        assert!(!is_version_compatible("0.0.9", "0.1.0"));
    }

    #[test]
    fn protocol_version_constants_valid() {
        assert!(is_version_compatible(PROTOCOL_VERSION, MIN_CLIENT_VERSION));
    }

    #[tokio::test]
    async fn negotiate_empty_features() -> IpcResult<()> {
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
    async fn negotiate_unsupported_features_only() -> IpcResult<()> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let features = vec![
            "nonexistent_feature".to_string(),
            "another_fake".to_string(),
        ];
        let result = server.negotiate_features("1.0.0", &features).await?;

        assert!(result.compatible);
        assert!(result.enabled_features.is_empty());

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn negotiate_all_server_features() -> IpcResult<()> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let features = vec![
            "device_management".to_string(),
            "profile_management".to_string(),
            "safety_control".to_string(),
            "health_monitoring".to_string(),
            "game_integration".to_string(),
            "streaming_health".to_string(),
            "streaming_devices".to_string(),
        ];
        let result = server.negotiate_features("1.0.0", &features).await?;

        assert!(result.compatible);
        assert_eq!(result.enabled_features.len(), 7);

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn negotiate_incompatible_does_not_register_client() -> IpcResult<()> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let result = server
            .negotiate_features("0.1.0", &["device_management".to_string()])
            .await?;

        assert!(!result.compatible);
        assert_eq!(server.client_count().await, 0);

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn negotiate_result_contains_server_version() -> IpcResult<()> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let result = server.negotiate_features("1.0.0", &[]).await?;
        assert_eq!(result.server_version, PROTOCOL_VERSION);
        assert_eq!(result.min_client_version, MIN_CLIENT_VERSION);

        server.stop().await?;
        Ok(())
    }
}

// ── Handler types construction and validation ────────────────────────────

mod handler_types {
    use std::collections::BTreeMap;

    use openracing_ipc::handlers::{
        DeviceCapabilities, DeviceInfo, DeviceStatus, DiagnosticInfo, FaultRecord,
        FeatureNegotiationResult, PerformanceMetrics, ProfileInfo, ProfileScope, TelemetryData,
    };

    #[test]
    fn device_info_with_capabilities() {
        let caps = DeviceCapabilities {
            supports_pid: true,
            supports_raw_torque_1khz: true,
            supports_health_stream: true,
            supports_led_bus: false,
            max_torque_cnm: 2500,
            encoder_cpr: 65536,
            min_report_period_us: 1000,
        };

        let info = DeviceInfo {
            id: "sim-wheel-01".to_string(),
            name: "SimCube 2 Pro".to_string(),
            device_type: 1,
            state: 1,
            capabilities: Some(caps),
        };

        assert_eq!(info.id, "sim-wheel-01");
        assert!(info.capabilities.is_some());
        let caps = info.capabilities.as_ref();
        assert!(caps.is_some());
        if let Some(c) = caps {
            assert!(c.supports_pid);
            assert!(c.supports_raw_torque_1khz);
            assert_eq!(c.max_torque_cnm, 2500);
        }
    }

    #[test]
    fn device_info_without_capabilities() {
        let info = DeviceInfo {
            id: "unknown-dev".to_string(),
            name: "Unknown Device".to_string(),
            device_type: 0,
            state: 0,
            capabilities: None,
        };

        assert_eq!(info.state, 0);
        assert!(info.capabilities.is_none());
    }

    #[test]
    fn device_status_with_faults() {
        let status = DeviceStatus {
            device: DeviceInfo {
                id: "dev-1".to_string(),
                name: "Wheel".to_string(),
                device_type: 1,
                state: 1,
                capabilities: None,
            },
            last_seen: 1_700_000_000,
            active_faults: vec!["OVERCURRENT".to_string(), "OVERTEMP".to_string()],
            telemetry: None,
        };

        assert_eq!(status.active_faults.len(), 2);
        assert_eq!(status.last_seen, 1_700_000_000);
    }

    #[test]
    fn telemetry_data_construction() {
        let telemetry = TelemetryData {
            wheel_angle_deg: 450.0,
            wheel_speed_rad_s: 3.14,
            temperature_c: 42.5,
            fault_flags: 0,
            hands_on: true,
        };

        assert!((telemetry.wheel_angle_deg - 450.0).abs() < f32::EPSILON);
        assert!(telemetry.hands_on);
        assert_eq!(telemetry.fault_flags, 0);
    }

    #[test]
    fn telemetry_data_with_faults() {
        let telemetry = TelemetryData {
            wheel_angle_deg: 0.0,
            wheel_speed_rad_s: 0.0,
            temperature_c: 95.0,
            fault_flags: 0x03, // two faults
            hands_on: false,
        };

        assert_eq!(telemetry.fault_flags, 3);
        assert!(!telemetry.hands_on);
    }

    #[test]
    fn profile_info_with_scope() {
        let profile = ProfileInfo {
            id: "drift-profile-1".to_string(),
            schema_version: "2.0.0".to_string(),
            name: "Drift Setup".to_string(),
            scope: ProfileScope {
                game: Some("Assetto Corsa".to_string()),
                car: Some("AE86".to_string()),
                track: Some("Ebisu".to_string()),
            },
        };

        assert_eq!(profile.name, "Drift Setup");
        assert_eq!(profile.scope.game.as_deref(), Some("Assetto Corsa"));
        assert_eq!(profile.scope.car.as_deref(), Some("AE86"));
        assert_eq!(profile.scope.track.as_deref(), Some("Ebisu"));
    }

    #[test]
    fn profile_scope_all_none() {
        let scope = ProfileScope {
            game: None,
            car: None,
            track: None,
        };

        assert!(scope.game.is_none());
        assert!(scope.car.is_none());
        assert!(scope.track.is_none());
    }

    #[test]
    fn diagnostic_info_with_metrics() {
        let diag = DiagnosticInfo {
            device_id: "dev-diag".to_string(),
            system_info: BTreeMap::from([
                ("firmware".to_string(), "1.5.2".to_string()),
                ("driver".to_string(), "2.0.0".to_string()),
            ]),
            recent_faults: vec![FaultRecord {
                timestamp: 1_700_000_000,
                code: "E001".to_string(),
                message: "Overcurrent detected".to_string(),
                active: true,
            }],
            performance: Some(PerformanceMetrics {
                p99_jitter_us: 150.0,
                missed_tick_rate: 0.0001,
                total_ticks: 1_000_000,
                missed_ticks: 100,
            }),
        };

        assert_eq!(diag.system_info.len(), 2);
        assert_eq!(diag.recent_faults.len(), 1);
        assert!(diag.recent_faults[0].active);
        assert!(diag.performance.is_some());
        if let Some(perf) = &diag.performance {
            assert!(perf.p99_jitter_us < 250.0);
            assert_eq!(perf.total_ticks, 1_000_000);
        }
    }

    #[test]
    fn diagnostic_info_empty() {
        let diag = DiagnosticInfo {
            device_id: "empty-dev".to_string(),
            system_info: BTreeMap::new(),
            recent_faults: vec![],
            performance: None,
        };

        assert!(diag.system_info.is_empty());
        assert!(diag.recent_faults.is_empty());
        assert!(diag.performance.is_none());
    }

    #[test]
    fn fault_record_active_and_cleared() {
        let active = FaultRecord {
            timestamp: 100,
            code: "F001".to_string(),
            message: "Active fault".to_string(),
            active: true,
        };

        let cleared = FaultRecord {
            timestamp: 200,
            code: "F001".to_string(),
            message: "Cleared fault".to_string(),
            active: false,
        };

        assert!(active.active);
        assert!(!cleared.active);
        assert!(cleared.timestamp > active.timestamp);
    }

    #[test]
    fn performance_metrics_within_budget() {
        let metrics = PerformanceMetrics {
            p99_jitter_us: 200.0,
            missed_tick_rate: 0.00001,
            total_ticks: 10_000_000,
            missed_ticks: 100,
        };

        // Performance gates: p99 jitter <= 250us, missed rate <= 0.001%
        assert!(metrics.p99_jitter_us <= 250.0);
        assert!(metrics.missed_tick_rate <= 0.00001);
    }

    #[test]
    fn feature_negotiation_result_fields() {
        let result = FeatureNegotiationResult {
            server_version: "1.0.0".to_string(),
            supported_features: vec![
                "device_management".to_string(),
                "safety_control".to_string(),
            ],
            enabled_features: vec!["device_management".to_string()],
            compatible: true,
            min_client_version: "1.0.0".to_string(),
        };

        assert!(result.compatible);
        assert_eq!(result.supported_features.len(), 2);
        assert_eq!(result.enabled_features.len(), 1);
        assert!(
            result
                .enabled_features
                .contains(&"device_management".to_string())
        );
    }

    #[test]
    fn device_capabilities_clone() {
        let caps = DeviceCapabilities {
            supports_pid: true,
            supports_raw_torque_1khz: false,
            supports_health_stream: true,
            supports_led_bus: false,
            max_torque_cnm: 1000,
            encoder_cpr: 32768,
            min_report_period_us: 500,
        };

        let cloned = caps.clone();
        assert_eq!(cloned.max_torque_cnm, caps.max_torque_cnm);
        assert_eq!(cloned.encoder_cpr, caps.encoder_cpr);
    }
}

// ── Constants and module-level checks ────────────────────────────────────

mod constants {
    use openracing_ipc::{DEFAULT_TCP_PORT, MIN_CLIENT_VERSION, PROTOCOL_VERSION};

    #[test]
    fn default_tcp_port_is_grpc_convention() {
        assert_eq!(DEFAULT_TCP_PORT, 50051);
    }

    #[test]
    fn protocol_version_is_semver() {
        let parts: Vec<&str> = PROTOCOL_VERSION.split('.').collect();
        assert_eq!(parts.len(), 3);
        for part in &parts {
            assert!(part.parse::<u32>().is_ok());
        }
    }

    #[test]
    fn min_client_version_is_semver() {
        let parts: Vec<&str> = MIN_CLIENT_VERSION.split('.').collect();
        assert_eq!(parts.len(), 3);
        for part in &parts {
            assert!(part.parse::<u32>().is_ok());
        }
    }
}
