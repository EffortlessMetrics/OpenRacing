//! Unit tests for IPC crate

mod codec_tests {
    use openracing_ipc::codec::message_types;
    use openracing_ipc::codec::{MessageCodec, MessageHeader};

    #[test]
    fn test_codec_creation() {
        let codec = MessageCodec::new();
        assert_eq!(codec.max_message_size(), 16 * 1024 * 1024);
    }

    #[test]
    fn test_codec_custom_max_size() {
        let codec = MessageCodec::with_max_size(1024);
        assert_eq!(codec.max_message_size(), 1024);
    }

    #[test]
    fn test_valid_size_check() {
        let codec = MessageCodec::with_max_size(1000);
        assert!(codec.is_valid_size(500));
        assert!(codec.is_valid_size(1000));
        assert!(!codec.is_valid_size(0));
        assert!(!codec.is_valid_size(1001));
    }

    #[test]
    fn test_message_header_encode_decode() {
        let header = MessageHeader::new(message_types::DEVICE, 1024, 42);
        let encoded = header.encode();
        let decoded = MessageHeader::decode(&encoded).expect("decode should succeed");

        assert_eq!(decoded.message_type, message_types::DEVICE);
        assert_eq!(decoded.payload_len, 1024);
        assert_eq!(decoded.sequence, 42);
        assert_eq!(decoded.flags, 0);
    }

    #[test]
    fn test_message_header_decode_insufficient_bytes() {
        let bytes = [0u8; 4];
        let result = MessageHeader::decode(&bytes);
        assert!(result.is_err());
    }
}

mod transport_tests {
    use std::time::Duration;

    use openracing_ipc::transport::{TransportBuilder, TransportConfig, TransportType};

    #[test]
    fn test_transport_type_tcp() {
        let transport = TransportType::tcp();
        match transport {
            TransportType::Tcp { address, port } => {
                assert_eq!(address, "127.0.0.1");
                assert_eq!(port, 50051);
            }
            _ => panic!("Expected TCP transport"),
        }
    }

    #[test]
    fn test_transport_type_tcp_custom() {
        let transport = TransportType::tcp_with_address("0.0.0.0", 8080);
        match transport {
            TransportType::Tcp { address, port } => {
                assert_eq!(address, "0.0.0.0");
                assert_eq!(port, 8080);
            }
            _ => panic!("Expected TCP transport"),
        }
    }

    #[test]
    fn test_transport_type_description() {
        let transport = TransportType::tcp();
        assert!(transport.description().contains("TCP"));
    }

    #[test]
    fn test_transport_config_default() {
        let config = TransportConfig::default();
        assert_eq!(config.max_connections, 100);
        assert!(!config.enable_acl);
        assert_eq!(config.recv_buffer_size, 64 * 1024);
    }

    #[test]
    fn test_transport_builder() {
        let config = TransportBuilder::new()
            .max_connections(50)
            .connection_timeout(Duration::from_secs(10))
            .enable_acl(true)
            .build();

        assert_eq!(config.max_connections, 50);
        assert!(config.enable_acl);
        assert_eq!(config.connection_timeout, Duration::from_secs(10));
    }
}

mod server_tests {
    use openracing_ipc::server::{IpcConfig, IpcServer, ServerState, is_version_compatible};
    use openracing_ipc::transport::TransportType;

    #[test]
    fn test_version_compatibility_same_version() {
        assert!(is_version_compatible("1.0.0", "1.0.0"));
    }

    #[test]
    fn test_version_compatibility_higher_minor() {
        assert!(is_version_compatible("1.1.0", "1.0.0"));
        assert!(is_version_compatible("1.2.5", "1.0.0"));
    }

    #[test]
    fn test_version_compatibility_higher_patch() {
        assert!(is_version_compatible("1.0.1", "1.0.0"));
        assert!(is_version_compatible("1.0.5", "1.0.0"));
    }

    #[test]
    fn test_version_incompatibility_lower_minor() {
        assert!(!is_version_compatible("1.0.0", "1.1.0"));
        assert!(!is_version_compatible("0.9.0", "1.0.0"));
    }

    #[test]
    fn test_version_incompatibility_major_mismatch() {
        assert!(!is_version_compatible("2.0.0", "1.0.0"));
        assert!(!is_version_compatible("0.1.0", "1.0.0"));
    }

    #[test]
    fn test_version_incompatibility_invalid_format() {
        assert!(!is_version_compatible("1.0", "1.0.0"));
        assert!(!is_version_compatible("invalid", "1.0.0"));
    }

    #[test]
    fn test_ipc_config_default() {
        let config = IpcConfig::default();
        assert_eq!(config.server_name, "openracing-ipc");
        assert_eq!(config.health_buffer_size, 1000);
    }

    #[test]
    fn test_ipc_config_with_transport() {
        let config = IpcConfig::with_transport(TransportType::tcp());
        match config.transport.transport {
            TransportType::Tcp { .. } => {}
            _ => panic!("Expected TCP transport"),
        }
    }

    #[tokio::test]
    async fn test_server_creation() {
        let config = IpcConfig::default();
        let server = IpcServer::new(config);
        assert_eq!(server.state().await, ServerState::Stopped);
    }

    #[tokio::test]
    async fn test_server_start_stop() {
        let config = IpcConfig::default();
        let server = IpcServer::new(config);

        server.start().await.expect("start should succeed");
        assert_eq!(server.state().await, ServerState::Running);

        server.stop().await.expect("stop should succeed");
        assert_eq!(server.state().await, ServerState::Stopped);
    }

    #[tokio::test]
    async fn test_server_double_start() {
        let config = IpcConfig::default();
        let server = IpcServer::new(config);

        server.start().await.expect("first start should succeed");
        let result = server.start().await;
        assert!(result.is_err());

        server.stop().await.expect("stop should succeed");
    }
}

mod error_tests {
    use openracing_ipc::error::IpcError;

    #[test]
    fn test_error_is_recoverable() {
        let err = IpcError::ConnectionFailed("test".to_string());
        assert!(err.is_recoverable());

        let err = IpcError::Timeout { timeout_ms: 1000 };
        assert!(err.is_recoverable());

        let err = IpcError::TransportInit("test".to_string());
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_error_is_fatal() {
        let err = IpcError::TransportInit("test".to_string());
        assert!(err.is_fatal());

        let err = IpcError::ServerNotRunning;
        assert!(err.is_fatal());

        let err = IpcError::ConnectionFailed("test".to_string());
        assert!(!err.is_fatal());
    }

    #[test]
    fn test_error_helpers() {
        let err = IpcError::timeout(5000);
        match err {
            IpcError::Timeout { timeout_ms } => assert_eq!(timeout_ms, 5000),
            _ => panic!("Expected Timeout error"),
        }

        let err = IpcError::connection_limit(100);
        match err {
            IpcError::ConnectionLimitExceeded { max } => assert_eq!(max, 100),
            _ => panic!("Expected ConnectionLimitExceeded error"),
        }
    }

    #[test]
    fn test_error_display() {
        let err = IpcError::VersionIncompatibility {
            client: "0.9.0".to_string(),
            server: "1.0.0".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("0.9.0"));
        assert!(msg.contains("1.0.0"));
    }
}
