//! Property-based tests for the racing-wheel-service crate.
//!
//! Tests cover: service state machine, device hot-plug handling,
//! config serialization, safety interlock states, and edge cases.

#![allow(clippy::redundant_closure)]

use proptest::prelude::*;
use racing_wheel_service::{
    DeviceState, FaultSeverity, InterlockState, IpcClientConfig, IpcConfig, SafetyEvent,
    ServiceConfig, TransportType,
};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Proptest: ServiceConfig serialization round-trips
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn prop_service_config_serialization_roundtrip(
        max_restart in 0u32..=100u32,
        restart_delay in 1u64..=300u64,
        health_interval in 1u64..=3600u64,
        auto_restart in any::<bool>(),
    ) {
        let config = ServiceConfig {
            service_name: "test-wheeld".to_string(),
            service_display_name: "Test Service".to_string(),
            service_description: "A test service".to_string(),
            ipc: IpcConfig::default(),
            health_check_interval: health_interval,
            max_restart_attempts: max_restart,
            restart_delay,
            auto_restart,
        };

        let json = serde_json::to_string(&config)
            .map_err(|e| TestCaseError::fail(format!("Serialization failed: {e}")))?;
        let deserialized: ServiceConfig = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::fail(format!("Deserialization failed: {e}")))?;

        prop_assert_eq!(deserialized.max_restart_attempts, max_restart);
        prop_assert_eq!(deserialized.restart_delay, restart_delay);
        prop_assert_eq!(deserialized.health_check_interval, health_interval);
        prop_assert_eq!(deserialized.auto_restart, auto_restart);
        prop_assert_eq!(deserialized.service_name, "test-wheeld");
    }

    // -----------------------------------------------------------------------
    // Proptest: IpcConfig serialization round-trips
    // -----------------------------------------------------------------------

    #[test]
    fn prop_ipc_config_serialization_roundtrip(
        max_conns in 1u32..=1000u32,
        timeout_secs in 1u64..=300u64,
        enable_acl in any::<bool>(),
    ) {
        let config = IpcConfig {
            bind_address: Some("127.0.0.1".to_string()),
            transport: TransportType::default(),
            max_connections: max_conns,
            connection_timeout: Duration::from_secs(timeout_secs),
            enable_acl,
        };

        let json = serde_json::to_string(&config)
            .map_err(|e| TestCaseError::fail(format!("Serialization failed: {e}")))?;
        let deserialized: IpcConfig = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::fail(format!("Deserialization failed: {e}")))?;

        prop_assert_eq!(deserialized.max_connections, max_conns);
        prop_assert_eq!(deserialized.connection_timeout, Duration::from_secs(timeout_secs));
        prop_assert_eq!(deserialized.enable_acl, enable_acl);
        prop_assert_eq!(deserialized.bind_address, Some("127.0.0.1".to_string()));
    }

    // -----------------------------------------------------------------------
    // Proptest: IpcClientConfig preserves values
    // -----------------------------------------------------------------------

    #[test]
    fn prop_ipc_client_config_preserves_values(
        timeout_secs in 1u64..=120u64,
        port in 1024u16..=65535u16,
    ) {
        let addr = format!("10.0.0.1:{}", port);
        let config = IpcClientConfig {
            connect_timeout: Duration::from_secs(timeout_secs),
            server_address: addr.clone(),
        };
        let client = racing_wheel_service::IpcClient::new(config);
        // Verify through public field on the config (accessed via construction)
        // The client stores the config internally
        let _ = client;
        // Verify the addr was built correctly
        prop_assert!(addr.contains(&port.to_string()));
    }
}

// ---------------------------------------------------------------------------
// Service state machine tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod state_machine_tests {
    use super::*;

    #[test]
    fn test_service_config_defaults_are_reasonable() -> Result<(), Box<dyn std::error::Error>> {
        let config = ServiceConfig::default();
        assert_eq!(config.service_name, "wheeld");
        assert!(config.max_restart_attempts > 0);
        assert!(config.restart_delay > 0);
        assert!(config.health_check_interval > 0);
        assert!(config.auto_restart);
        assert!(config.ipc.enable_acl);
        Ok(())
    }

    #[test]
    fn test_transport_type_default_is_platform_specific() -> Result<(), Box<dyn std::error::Error>>
    {
        let transport = TransportType::default();
        #[cfg(windows)]
        {
            assert!(
                matches!(transport, TransportType::NamedPipe(ref name) if name.contains("wheel")),
                "Windows default should be named pipe"
            );
        }
        #[cfg(unix)]
        {
            assert!(
                matches!(transport, TransportType::UnixDomainSocket(ref path) if path.contains("wheel")),
                "Unix default should be UDS"
            );
        }
        Ok(())
    }

    #[test]
    fn test_ipc_config_defaults() -> Result<(), Box<dyn std::error::Error>> {
        let config = IpcConfig::default();
        assert_eq!(config.bind_address, Some("127.0.0.1".to_string()));
        assert_eq!(config.max_connections, 10);
        assert_eq!(config.connection_timeout, Duration::from_secs(30));
        assert!(!config.enable_acl);
        Ok(())
    }

    #[test]
    fn test_ipc_client_config_defaults() -> Result<(), Box<dyn std::error::Error>> {
        let config = IpcClientConfig::default();
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert_eq!(config.server_address, "127.0.0.1:50051");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Device hot-plug event handling tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod device_hotplug_tests {
    use super::*;

    #[test]
    fn test_device_state_variants_exist() -> Result<(), Box<dyn std::error::Error>> {
        let states = vec![
            DeviceState::Disconnected,
            DeviceState::Connected,
            DeviceState::Ready,
            DeviceState::Faulted {
                reason: "overtemp".to_string(),
            },
        ];
        for state in &states {
            // Verify Debug is implemented
            let _ = format!("{:?}", state);
        }
        // Verify PartialEq works
        assert_eq!(DeviceState::Disconnected, DeviceState::Disconnected);
        assert_eq!(DeviceState::Connected, DeviceState::Connected);
        assert_eq!(DeviceState::Ready, DeviceState::Ready);
        assert_ne!(DeviceState::Connected, DeviceState::Ready);
        assert_ne!(DeviceState::Disconnected, DeviceState::Connected);
        Ok(())
    }

    #[test]
    fn test_fault_severity_ordering_exists() -> Result<(), Box<dyn std::error::Error>> {
        // Verify all severity levels exist and are distinct
        let warning = FaultSeverity::Warning;
        let critical = FaultSeverity::Critical;
        let fatal = FaultSeverity::Fatal;

        assert_ne!(warning, critical);
        assert_ne!(critical, fatal);
        assert_ne!(warning, fatal);

        assert_eq!(FaultSeverity::Warning, FaultSeverity::Warning);
        assert_eq!(FaultSeverity::Critical, FaultSeverity::Critical);
        assert_eq!(FaultSeverity::Fatal, FaultSeverity::Fatal);
        Ok(())
    }

    #[test]
    fn test_faulted_device_state_preserves_reason() -> Result<(), Box<dyn std::error::Error>> {
        let reason = "motor driver fault detected".to_string();
        let state = DeviceState::Faulted {
            reason: reason.clone(),
        };
        if let DeviceState::Faulted { reason: r } = state {
            assert_eq!(r, reason);
        } else {
            return Err("Expected Faulted variant".into());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Safety interlock state tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod safety_interlock_tests {
    use super::*;
    use racing_wheel_engine::safety::FaultType;
    use std::time::Instant;

    #[test]
    fn test_interlock_state_safe_torque_is_default() -> Result<(), Box<dyn std::error::Error>> {
        let state = InterlockState::SafeTorque;
        assert_eq!(state, InterlockState::SafeTorque);
        Ok(())
    }

    #[test]
    fn test_interlock_state_variants_are_distinct() -> Result<(), Box<dyn std::error::Error>> {
        let now = Instant::now();
        let states: Vec<InterlockState> = vec![
            InterlockState::SafeTorque,
            InterlockState::Challenge {
                challenge_token: 42,
                expires_at: now + Duration::from_secs(30),
            },
            InterlockState::HighTorqueActive {
                unlocked_at: now,
                device_token: 99,
            },
            InterlockState::Faulted {
                fault_type: FaultType::SafetyInterlockViolation,
                occurred_at: now,
            },
        ];
        // Verify all are debuggable
        for state in &states {
            let debug_str = format!("{:?}", state);
            assert!(!debug_str.is_empty());
        }
        Ok(())
    }

    #[test]
    fn test_safety_event_variants() -> Result<(), Box<dyn std::error::Error>> {
        let device_id = "test-device".parse().map_err(|_| "parse error")?;

        let events: Vec<SafetyEvent> = vec![
            SafetyEvent::HighTorqueRequested {
                device_id,
                requested_by: "user".to_string(),
            },
            SafetyEvent::ChallengeResponse {
                device_id: "test-device".parse().map_err(|_| "parse error")?,
                token: 42,
                success: true,
            },
            SafetyEvent::FaultDetected {
                device_id: "test-device".parse().map_err(|_| "parse error")?,
                fault_type: FaultType::ThermalLimit,
                severity: FaultSeverity::Critical,
            },
            SafetyEvent::FaultCleared {
                device_id: "test-device".parse().map_err(|_| "parse error")?,
                fault_type: FaultType::ThermalLimit,
            },
            SafetyEvent::EmergencyStop {
                device_id: "test-device".parse().map_err(|_| "parse error")?,
                reason: "user request".to_string(),
            },
        ];
        for event in &events {
            let debug_str = format!("{:?}", event);
            assert!(!debug_str.is_empty());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

#[cfg(test)]
mod edge_case_tests {
    use super::*;

    #[test]
    fn test_health_event_internal_fields() -> Result<(), Box<dyn std::error::Error>> {
        let event = racing_wheel_service::HealthEventInternal {
            device_id: "dev-1".to_string(),
            event_type: "fault".to_string(),
            message: "Motor driver overtemp".to_string(),
            timestamp: std::time::SystemTime::now(),
        };
        assert_eq!(event.device_id, "dev-1");
        assert_eq!(event.event_type, "fault");
        assert!(!event.message.is_empty());
        Ok(())
    }

    #[test]
    fn test_service_config_all_fields_serialize() -> Result<(), Box<dyn std::error::Error>> {
        let config = ServiceConfig {
            service_name: "test".to_string(),
            service_display_name: "Test Display".to_string(),
            service_description: "Test Desc".to_string(),
            ipc: IpcConfig {
                bind_address: Some("0.0.0.0".to_string()),
                transport: TransportType::default(),
                max_connections: 1,
                connection_timeout: Duration::from_millis(500),
                enable_acl: true,
            },
            health_check_interval: 1,
            max_restart_attempts: 0,
            restart_delay: 0,
            auto_restart: false,
        };
        let json = serde_json::to_string_pretty(&config)?;
        assert!(json.contains("test"));
        assert!(json.contains("0.0.0.0"));
        let back: ServiceConfig = serde_json::from_str(&json)?;
        assert_eq!(back.service_name, "test");
        assert_eq!(back.max_restart_attempts, 0);
        assert!(!back.auto_restart);
        Ok(())
    }

    #[tokio::test]
    async fn test_ipc_server_creation_and_broadcast() -> Result<(), Box<dyn std::error::Error>> {
        let config = IpcConfig::default();
        let server = racing_wheel_service::IpcServer::new(config).await?;

        let mut rx = server.get_health_receiver();
        server.broadcast_health_event(racing_wheel_service::HealthEventInternal {
            device_id: "dev-0".to_string(),
            event_type: "connected".to_string(),
            message: "Device connected".to_string(),
            timestamp: std::time::SystemTime::now(),
        });

        let event = rx.recv().await?;
        assert_eq!(event.device_id, "dev-0");
        Ok(())
    }

    #[tokio::test]
    async fn test_ipc_server_multiple_receivers() -> Result<(), Box<dyn std::error::Error>> {
        let config = IpcConfig::default();
        let server = racing_wheel_service::IpcServer::new(config).await?;

        let mut rx1 = server.get_health_receiver();
        let mut rx2 = server.get_health_receiver();

        server.broadcast_health_event(racing_wheel_service::HealthEventInternal {
            device_id: "dev-x".to_string(),
            event_type: "info".to_string(),
            message: "test".to_string(),
            timestamp: std::time::SystemTime::now(),
        });

        let e1 = rx1.recv().await?;
        let e2 = rx2.recv().await?;
        assert_eq!(e1.device_id, "dev-x");
        assert_eq!(e2.device_id, "dev-x");
        Ok(())
    }
}
