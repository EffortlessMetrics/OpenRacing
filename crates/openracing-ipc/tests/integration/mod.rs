//! Integration tests for client-server communication

use std::time::Duration;

use openracing_ipc::prelude::*;

mod server_lifecycle {
    use super::*;

    #[tokio::test]
    async fn server_can_start_and_stop() -> IpcResult<()> {
        let config = IpcConfig::default();
        let server = IpcServer::new(config);

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
    async fn server_can_restart_after_stop() -> IpcResult<()> {
        let config = IpcConfig::default();
        let server = IpcServer::new(config);

        server.start().await?;
        server.stop().await?;

        server.start().await?;
        assert!(server.is_running().await);

        server.stop().await?;

        Ok(())
    }
}

mod feature_negotiation {
    use super::*;

    #[tokio::test]
    async fn negotiate_with_compatible_client() -> IpcResult<()> {
        let config = IpcConfig::default();
        let server = IpcServer::new(config);
        server.start().await?;

        let result = server
            .negotiate_features("1.0.0", &["device_management".to_string()])
            .await?;

        assert!(result.compatible);
        assert!(
            result
                .enabled_features
                .contains(&"device_management".to_string())
        );
        assert_eq!(result.server_version, PROTOCOL_VERSION);

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn negotiate_with_incompatible_client() -> IpcResult<()> {
        let config = IpcConfig::default();
        let server = IpcServer::new(config);
        server.start().await?;

        let result = server
            .negotiate_features("0.1.0", &["device_management".to_string()])
            .await?;

        assert!(!result.compatible);

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn negotiate_with_multiple_features() -> IpcResult<()> {
        let config = IpcConfig::default();
        let server = IpcServer::new(config);
        server.start().await?;

        let features = vec![
            "device_management".to_string(),
            "profile_management".to_string(),
            "unknown_feature".to_string(),
        ];

        let result = server.negotiate_features("1.0.0", &features).await?;

        assert!(result.compatible);
        assert!(
            result
                .enabled_features
                .contains(&"device_management".to_string())
        );
        assert!(
            result
                .enabled_features
                .contains(&"profile_management".to_string())
        );
        assert!(
            !result
                .enabled_features
                .contains(&"unknown_feature".to_string())
        );

        server.stop().await?;
        Ok(())
    }
}

mod health_events {
    use super::*;

    #[tokio::test]
    async fn broadcast_health_event() {
        let config = IpcConfig::default();
        let server = IpcServer::new(config);

        let mut receiver = server.subscribe_health();

        let event = HealthEvent {
            timestamp: std::time::SystemTime::now(),
            device_id: "test-device".to_string(),
            event_type: HealthEventType::Connected,
            message: "Device connected".to_string(),
            metadata: std::collections::HashMap::new(),
        };

        server.broadcast_health_event(event);

        let received = receiver.try_recv();
        assert!(received.is_ok());
        let received_event = received.expect("should have event");
        assert_eq!(received_event.device_id, "test-device");
        assert_eq!(received_event.event_type, HealthEventType::Connected);
    }

    #[tokio::test]
    async fn broadcast_multiple_health_events() {
        let config = IpcConfig::default();
        let server = IpcServer::new(config);

        let mut receiver = server.subscribe_health();

        for i in 0..5 {
            let event = HealthEvent {
                timestamp: std::time::SystemTime::now(),
                device_id: format!("device-{}", i),
                event_type: HealthEventType::Connected,
                message: format!("Device {} connected", i),
                metadata: std::collections::HashMap::new(),
            };
            server.broadcast_health_event(event);
        }

        for i in 0..5 {
            let received = receiver.try_recv();
            assert!(received.is_ok());
            assert_eq!(received.expect("event").device_id, format!("device-{}", i));
        }
    }
}

mod client_management {
    use super::*;

    #[tokio::test]
    async fn client_count_increases_on_negotiation() -> IpcResult<()> {
        let config = IpcConfig::default();
        let server = IpcServer::new(config);
        server.start().await?;

        assert_eq!(server.client_count().await, 0);

        server
            .negotiate_features("1.0.0", &["device_management".to_string()])
            .await?;

        assert_eq!(server.client_count().await, 1);

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn client_registration_manual() {
        let config = IpcConfig::default();
        let server = IpcServer::new(config);

        let client = ClientInfo {
            id: "manual-client".to_string(),
            connected_at: std::time::Instant::now(),
            version: "1.0.0".to_string(),
            features: vec!["device_management".to_string()],
            peer_info: PeerInfo::default(),
        };

        server.register_client(client).await;
        assert_eq!(server.client_count().await, 1);

        let clients = server.connected_clients().await;
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0].id, "manual-client");
    }

    #[tokio::test]
    async fn client_unregistration() {
        let config = IpcConfig::default();
        let server = IpcServer::new(config);

        let client = ClientInfo {
            id: "test-client".to_string(),
            connected_at: std::time::Instant::now(),
            version: "1.0.0".to_string(),
            features: vec![],
            peer_info: PeerInfo::default(),
        };

        server.register_client(client).await;
        assert_eq!(server.client_count().await, 1);

        server.unregister_client("test-client").await;
        assert_eq!(server.client_count().await, 0);
    }
}

mod message_codec {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        let codec = MessageCodec::new();

        let header = MessageHeader::new(message_types::DEVICE, 100, 1);
        let encoded = header.encode();

        assert!(codec.is_valid_size(encoded.len()));

        let decoded = MessageHeader::decode(&encoded).expect("decode should succeed");
        assert_eq!(decoded.message_type, message_types::DEVICE);
        assert_eq!(decoded.payload_len, 100);
        assert_eq!(decoded.sequence, 1);
    }

    #[test]
    fn message_size_validation() {
        let codec = MessageCodec::with_max_size(100);

        let header = MessageHeader::new(message_types::DEVICE, 100, 1);
        let encoded = header.encode();

        assert!(codec.is_valid_size(encoded.len()));
        assert!(!codec.is_valid_size(101));
        assert!(!codec.is_valid_size(0));
    }
}
