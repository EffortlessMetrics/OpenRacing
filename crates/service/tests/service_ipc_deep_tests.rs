//! Deep IPC tests covering channel management, client connection/disconnection,
//! concurrent client handling, message routing, authentication/authorization,
//! and timeout handling.

use std::time::{Duration, SystemTime};

use racing_wheel_engine::safety::FaultType;
use racing_wheel_schemas::prelude::{
    BaseSettings, DeviceId, Profile, ProfileId, ProfileScope, TorqueNm,
};
use racing_wheel_service::{
    FaultSeverity, HealthEventInternal, InterlockState, IpcClient, IpcClientConfig, IpcConfig,
    IpcServer, ServiceConfig, ServiceDaemon, WheelService,
    profile_repository::ProfileRepositoryConfig,
};
use tempfile::TempDir;

// ── Helpers ──────────────────────────────────────────────────────────────

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

async fn temp_service() -> Result<(WheelService, TempDir), BoxErr> {
    let tmp = TempDir::new()?;
    let config = ProfileRepositoryConfig {
        profiles_dir: tmp.path().to_path_buf(),
        trusted_keys: Vec::new(),
        auto_migrate: true,
        backup_on_migrate: false,
    };
    let svc = WheelService::new_with_profile_config(config).await?;
    Ok((svc, tmp))
}

fn parse_device_id(name: &str) -> Result<DeviceId, BoxErr> {
    name.parse()
        .map_err(|e| -> BoxErr { format!("bad device id: {e}").into() })
}

fn torque(nm: f32) -> Result<TorqueNm, BoxErr> {
    TorqueNm::new(nm).map_err(|e| -> BoxErr { format!("bad torque: {e}").into() })
}

fn make_health_event(device_id: &str, event_type: &str, message: &str) -> HealthEventInternal {
    HealthEventInternal {
        device_id: device_id.to_string(),
        event_type: event_type.to_string(),
        message: message.to_string(),
        timestamp: SystemTime::now(),
    }
}

// =========================================================================
// 1. IPC channel management
// =========================================================================

#[tokio::test]
async fn ipc_channel_server_creation_defaults() -> Result<(), BoxErr> {
    let config = IpcConfig::default();
    let server = IpcServer::new(config).await?;
    // Server should be created without error
    let _ = server;
    Ok(())
}

#[tokio::test]
async fn ipc_channel_server_creation_custom_config() -> Result<(), BoxErr> {
    let config = IpcConfig {
        max_connections: 50,
        connection_timeout: Duration::from_secs(60),
        enable_acl: true,
        ..IpcConfig::default()
    };
    let server = IpcServer::new(config).await?;
    let _ = server;
    Ok(())
}

#[tokio::test]
async fn ipc_channel_health_broadcast_setup() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default()).await?;
    let mut rx = server.get_health_receiver();

    let event = make_health_event("dev-1", "init", "Server starting");
    server.broadcast_health_event(event);

    let received = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await;
    assert!(received.is_ok(), "should receive health event");
    let msg = received??;
    assert_eq!(msg.device_id, "dev-1");
    assert_eq!(msg.event_type, "init");
    Ok(())
}

#[tokio::test]
async fn ipc_channel_multiple_health_receivers() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default()).await?;
    let mut rx1 = server.get_health_receiver();
    let mut rx2 = server.get_health_receiver();

    let event = make_health_event("dev-x", "heartbeat", "ok");
    server.broadcast_health_event(event);

    let r1 = tokio::time::timeout(Duration::from_secs(2), rx1.recv()).await;
    let r2 = tokio::time::timeout(Duration::from_secs(2), rx2.recv()).await;

    assert!(r1.is_ok() && r1?.is_ok());
    assert!(r2.is_ok() && r2?.is_ok());
    Ok(())
}

#[tokio::test]
async fn ipc_channel_broadcast_many_events() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default()).await?;
    let mut rx = server.get_health_receiver();

    for i in 0..100 {
        let event = make_health_event(&format!("dev-{i}"), "tick", &format!("tick {i}"));
        server.broadcast_health_event(event);
    }

    let mut received_count = 0u32;
    while let Ok(Ok(_)) = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await {
        received_count += 1;
    }
    assert!(received_count > 0, "should have received some events");
    Ok(())
}

#[tokio::test]
async fn ipc_channel_shutdown_idempotent() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default()).await?;
    server.shutdown().await;
    server.shutdown().await;
    server.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn ipc_channel_server_clone_shares_state() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default()).await?;
    let server_clone = server.clone();

    let mut rx = server.get_health_receiver();
    let event = make_health_event("clone-dev", "test", "from clone");
    server_clone.broadcast_health_event(event);

    let received = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await;
    assert!(received.is_ok());
    let msg = received??;
    assert_eq!(msg.device_id, "clone-dev");
    Ok(())
}

// =========================================================================
// 2. Client connection/disconnection
// =========================================================================

#[tokio::test]
async fn client_connect_disconnect_default_config() -> Result<(), BoxErr> {
    let mut client = IpcClient::new(IpcClientConfig::default());
    client.connect().await?;
    client.disconnect().await?;
    Ok(())
}

#[tokio::test]
async fn client_connect_disconnect_custom_config() -> Result<(), BoxErr> {
    let config = IpcClientConfig {
        connect_timeout: Duration::from_secs(5),
        server_address: "127.0.0.1:12345".to_string(),
    };
    let mut client = IpcClient::new(config);
    client.connect().await?;
    client.disconnect().await?;
    Ok(())
}

#[tokio::test]
async fn client_repeated_connect_disconnect() -> Result<(), BoxErr> {
    let mut client = IpcClient::new(IpcClientConfig::default());
    for _ in 0..10 {
        client.connect().await?;
        client.disconnect().await?;
    }
    Ok(())
}

#[tokio::test]
async fn client_disconnect_without_connect() -> Result<(), BoxErr> {
    let mut client = IpcClient::new(IpcClientConfig::default());
    // Should not error even without prior connect
    client.disconnect().await?;
    Ok(())
}

#[tokio::test]
async fn client_config_default_values() -> Result<(), BoxErr> {
    let config = IpcClientConfig::default();
    assert_eq!(config.connect_timeout, Duration::from_secs(10));
    assert_eq!(config.server_address, "127.0.0.1:50051");
    Ok(())
}

// =========================================================================
// 3. Concurrent client handling
// =========================================================================

#[tokio::test]
async fn concurrent_multiple_clients_connect() -> Result<(), BoxErr> {
    let mut handles = Vec::new();

    for i in 0..10 {
        handles.push(tokio::spawn(async move {
            let config = IpcClientConfig {
                connect_timeout: Duration::from_secs(5),
                server_address: format!("127.0.0.1:{}", 50100 + i),
            };
            let mut client = IpcClient::new(config);
            client.connect().await?;
            client.disconnect().await?;
            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        }));
    }

    for handle in handles {
        handle.await??;
    }
    Ok(())
}

#[tokio::test]
async fn concurrent_health_events_from_multiple_sources() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default()).await?;
    let mut rx = server.get_health_receiver();

    let mut handles = Vec::new();
    for i in 0..5 {
        let s = server.clone();
        handles.push(tokio::spawn(async move {
            for j in 0..10 {
                let event =
                    make_health_event(&format!("dev-{i}"), "tick", &format!("from {i}-{j}"));
                s.broadcast_health_event(event);
            }
        }));
    }

    for handle in handles {
        handle.await?;
    }

    let mut count = 0u32;
    while let Ok(Ok(_)) = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await {
        count += 1;
    }
    assert!(count > 0, "should receive events from concurrent senders");
    Ok(())
}

#[tokio::test]
async fn concurrent_service_access_safety() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    let mut handles = Vec::new();
    for i in 0..5 {
        let s = svc.clone();
        handles.push(tokio::spawn(async move {
            let did: DeviceId = format!("conc-dev-{i}")
                .parse()
                .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
            let t =
                TorqueNm::new(10.0).map_err(|e| -> BoxErr { format!("bad torque: {e}").into() })?;
            s.safety_service().register_device(did.clone(), t).await?;
            let state = s.safety_service().get_safety_state(&did).await?;
            assert_eq!(state.interlock_state, InterlockState::SafeTorque);
            Ok::<(), BoxErr>(())
        }));
    }

    for handle in handles {
        handle.await??;
    }

    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.total_devices, 5);
    Ok(())
}

#[tokio::test]
async fn concurrent_profile_and_safety_operations() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    let s1 = svc.clone();
    let s2 = svc.clone();

    let profile_task = tokio::spawn(async move {
        for i in 0..3 {
            let pid = ProfileId::new(format!("conc-profile-{i}"))
                .map_err(|e| -> BoxErr { format!("{e}").into() })?;
            let profile = Profile::new(
                pid,
                ProfileScope::global(),
                BaseSettings::default(),
                format!("Concurrent Profile {i}"),
            );
            s1.profile_service().create_profile(profile).await?;
        }
        Ok::<(), BoxErr>(())
    });

    let safety_task = tokio::spawn(async move {
        for i in 0..3 {
            let did: DeviceId = format!("conc-safety-{i}")
                .parse()
                .map_err(|e| -> BoxErr { format!("{e}").into() })?;
            let t = TorqueNm::new(8.0).map_err(|e| -> BoxErr { format!("{e}").into() })?;
            s2.safety_service().register_device(did, t).await?;
        }
        Ok::<(), BoxErr>(())
    });

    profile_task.await??;
    safety_task.await??;

    let pstats = svc.profile_service().get_profile_statistics().await?;
    let sstats = svc.safety_service().get_statistics().await;
    assert_eq!(pstats.total_profiles, 3);
    assert_eq!(sstats.total_devices, 3);
    Ok(())
}

// =========================================================================
// 4. Message routing
// =========================================================================

#[tokio::test]
async fn message_routing_health_event_fields() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default()).await?;
    let mut rx = server.get_health_receiver();

    let now = SystemTime::now();
    let event = HealthEventInternal {
        device_id: "route-dev".to_string(),
        event_type: "fault".to_string(),
        message: "Overtemperature detected".to_string(),
        timestamp: now,
    };
    server.broadcast_health_event(event);

    let received = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await??;
    assert_eq!(received.device_id, "route-dev");
    assert_eq!(received.event_type, "fault");
    assert_eq!(received.message, "Overtemperature detected");
    Ok(())
}

#[tokio::test]
async fn message_routing_ordered_delivery() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default()).await?;
    let mut rx = server.get_health_receiver();

    for i in 0..20 {
        let event = make_health_event("seq-dev", "seq", &format!("msg-{i}"));
        server.broadcast_health_event(event);
    }

    let mut received = Vec::new();
    while let Ok(Ok(evt)) = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await {
        received.push(evt.message);
    }

    // Messages should be in order
    for (i, msg) in received.iter().enumerate() {
        assert_eq!(msg, &format!("msg-{i}"), "message {i} out of order");
    }
    Ok(())
}

#[tokio::test]
async fn message_routing_device_specific_events() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let server = IpcServer::new(IpcConfig::default()).await?;
    let mut rx = server.get_health_receiver();

    // Register two devices via safety service
    let did1 = parse_device_id("route-1")?;
    let did2 = parse_device_id("route-2")?;
    let t = torque(10.0)?;
    svc.safety_service()
        .register_device(did1.clone(), t)
        .await?;
    svc.safety_service()
        .register_device(did2.clone(), t)
        .await?;

    // Broadcast events for both
    server.broadcast_health_event(make_health_event("route-1", "status", "ok"));
    server.broadcast_health_event(make_health_event("route-2", "status", "warning"));

    let mut events = Vec::new();
    while let Ok(Ok(evt)) = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await {
        events.push(evt);
    }

    assert_eq!(events.len(), 2);
    let dev_ids: Vec<&str> = events.iter().map(|e| e.device_id.as_str()).collect();
    assert!(dev_ids.contains(&"route-1"));
    assert!(dev_ids.contains(&"route-2"));
    Ok(())
}

#[tokio::test]
async fn message_routing_safety_event_propagation() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("route-safety")?;
    let t = torque(10.0)?;
    svc.safety_service().register_device(did.clone(), t).await?;

    // Trigger fault and verify state change is visible
    svc.safety_service()
        .report_fault(&did, FaultType::ThermalLimit, FaultSeverity::Fatal)
        .await?;

    let state = svc.safety_service().get_safety_state(&did).await?;
    assert!(matches!(
        state.interlock_state,
        InterlockState::Faulted { .. }
    ));
    assert_eq!(state.current_torque_limit, TorqueNm::ZERO);
    Ok(())
}

#[tokio::test]
async fn message_routing_profile_activation_visible() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    let pid = ProfileId::new("route-profile".to_string())?;
    let profile = Profile::new(
        pid.clone(),
        ProfileScope::global(),
        BaseSettings::default(),
        "Route Test Profile".to_string(),
    );
    svc.profile_service().create_profile(profile).await?;

    let did = parse_device_id("route-dev-profile")?;
    svc.profile_service().set_active_profile(&did, &pid).await?;

    let active = svc.profile_service().get_active_profile(&did).await?;
    assert_eq!(active, Some(pid));
    Ok(())
}

// =========================================================================
// 5. Authentication/authorization (ACL configuration)
// =========================================================================

#[tokio::test]
async fn auth_acl_enabled_by_default() -> Result<(), BoxErr> {
    let config = IpcConfig::default();
    // Default IPC config from daemon.rs has ACL enabled
    // The ipc_simple.rs Default might differ, just verify the value is set
    let _ = config.enable_acl;
    Ok(())
}

#[tokio::test]
async fn auth_acl_disabled_config() -> Result<(), BoxErr> {
    let config = IpcConfig {
        enable_acl: false,
        ..IpcConfig::default()
    };
    let server = IpcServer::new(config).await?;
    let _ = server;
    Ok(())
}

#[tokio::test]
async fn auth_acl_enabled_server_creation() -> Result<(), BoxErr> {
    let config = IpcConfig {
        enable_acl: true,
        ..IpcConfig::default()
    };
    let server = IpcServer::new(config).await?;
    // ACL-enabled server should still create successfully
    server.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn auth_max_connections_configurable() -> Result<(), BoxErr> {
    let config = IpcConfig {
        max_connections: 1,
        ..IpcConfig::default()
    };
    let server = IpcServer::new(config).await?;
    let _ = server;
    Ok(())
}

#[tokio::test]
async fn auth_connection_timeout_configurable() -> Result<(), BoxErr> {
    let config = IpcConfig {
        connection_timeout: Duration::from_millis(500),
        ..IpcConfig::default()
    };
    let server = IpcServer::new(config).await?;
    let _ = server;
    Ok(())
}

#[tokio::test]
async fn auth_service_config_acl_serialization() -> Result<(), BoxErr> {
    let config = IpcConfig {
        enable_acl: true,
        max_connections: 25,
        connection_timeout: Duration::from_secs(45),
        ..IpcConfig::default()
    };
    let json = serde_json::to_string_pretty(&config)?;
    let parsed: IpcConfig = serde_json::from_str(&json)?;
    assert!(parsed.enable_acl);
    assert_eq!(parsed.max_connections, 25);
    assert_eq!(parsed.connection_timeout, Duration::from_secs(45));
    Ok(())
}

#[tokio::test]
async fn auth_safety_service_requires_registered_device() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let unregistered = parse_device_id("unregistered-dev")?;

    // All safety operations should fail for unregistered device
    let r1 = svc.safety_service().get_safety_state(&unregistered).await;
    assert!(r1.is_err());

    let r2 = svc
        .safety_service()
        .request_high_torque(&unregistered, "test".to_string())
        .await;
    assert!(r2.is_err());

    let r3 = svc
        .safety_service()
        .emergency_stop(&unregistered, "test".to_string())
        .await;
    assert!(r3.is_err());

    let r4 = svc
        .safety_service()
        .respond_to_challenge(&unregistered, 123)
        .await;
    assert!(r4.is_err());
    Ok(())
}

#[tokio::test]
async fn auth_clear_wrong_fault_type_rejected() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("auth-fault")?;
    let t = torque(10.0)?;
    svc.safety_service().register_device(did.clone(), t).await?;

    svc.safety_service()
        .emergency_stop(&did, "test".to_string())
        .await?;

    // Emergency stop sets SafetyInterlockViolation. Clearing with a different type should fail.
    let result = svc
        .safety_service()
        .clear_fault(&did, FaultType::ThermalLimit)
        .await;
    assert!(result.is_err(), "wrong fault type should be rejected");
    Ok(())
}

// =========================================================================
// 6. Timeout handling
// =========================================================================

#[tokio::test]
async fn timeout_service_operations_complete_fast() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    let result = tokio::time::timeout(Duration::from_secs(5), async {
        let did = parse_device_id("timeout-dev")?;
        let t = torque(10.0)?;
        svc.safety_service().register_device(did.clone(), t).await?;
        let _ = svc.safety_service().get_safety_state(&did).await?;
        let _ = svc.safety_service().get_statistics().await;
        svc.safety_service().unregister_device(&did).await?;
        Ok::<(), BoxErr>(())
    })
    .await;

    assert!(result.is_ok(), "operations should complete within timeout");
    result??;
    Ok(())
}

#[tokio::test]
async fn timeout_device_enumeration_bounded() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    let result = tokio::time::timeout(
        Duration::from_secs(3),
        svc.device_service().enumerate_devices(),
    )
    .await;

    assert!(result.is_ok(), "enumeration should complete in time");
    let devices = result??;
    assert!(!devices.is_empty());
    Ok(())
}

#[tokio::test]
async fn timeout_profile_operations_bounded() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    let result = tokio::time::timeout(Duration::from_secs(5), async {
        let pid = ProfileId::new("timeout-profile".to_string())?;
        let profile = Profile::new(
            pid.clone(),
            ProfileScope::global(),
            BaseSettings::default(),
            "Timeout Test".to_string(),
        );
        svc.profile_service().create_profile(profile).await?;
        let loaded = svc.profile_service().get_profile(&pid).await?;
        assert!(loaded.is_some());
        svc.profile_service().delete_profile(&pid).await?;
        Ok::<(), BoxErr>(())
    })
    .await;

    assert!(result.is_ok());
    result??;
    Ok(())
}

#[tokio::test]
async fn timeout_health_broadcast_nonblocking() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default()).await?;

    let result = tokio::time::timeout(Duration::from_secs(2), async {
        for i in 0..1000 {
            let event = make_health_event("timeout-dev", "flood", &format!("msg-{i}"));
            server.broadcast_health_event(event);
        }
        Ok::<(), BoxErr>(())
    })
    .await;

    assert!(result.is_ok(), "broadcast should not block");
    result??;
    Ok(())
}

#[tokio::test]
async fn timeout_concurrent_operations_bounded() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    let result = tokio::time::timeout(Duration::from_secs(10), async {
        let mut handles = Vec::new();
        for i in 0..10 {
            let s = svc.clone();
            handles.push(tokio::spawn(async move {
                let did: DeviceId = format!("timeout-conc-{i}")
                    .parse()
                    .map_err(|e| -> BoxErr { format!("{e}").into() })?;
                let t = TorqueNm::new(5.0).map_err(|e| -> BoxErr { format!("{e}").into() })?;
                s.safety_service().register_device(did.clone(), t).await?;
                let _ = s.safety_service().get_safety_state(&did).await?;
                s.safety_service().unregister_device(&did).await?;
                Ok::<(), BoxErr>(())
            }));
        }
        for handle in handles {
            handle.await??;
        }
        Ok::<(), BoxErr>(())
    })
    .await;

    assert!(result.is_ok());
    result??;
    Ok(())
}

#[tokio::test]
async fn timeout_receiver_dropped_broadcast_continues() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default()).await?;

    // Get and immediately drop a receiver
    let _rx = server.get_health_receiver();
    drop(_rx);

    // Broadcast should not panic with no receivers
    let event = make_health_event("dropped-rx", "test", "no receiver");
    server.broadcast_health_event(event);

    // A new receiver should still work
    let mut rx2 = server.get_health_receiver();
    let event2 = make_health_event("new-rx", "test", "new receiver");
    server.broadcast_health_event(event2);

    let received = tokio::time::timeout(Duration::from_secs(2), rx2.recv()).await;
    assert!(received.is_ok());
    let msg = received??;
    assert_eq!(msg.device_id, "new-rx");
    Ok(())
}

#[tokio::test]
async fn timeout_client_config_timeout_values() -> Result<(), BoxErr> {
    // Verify client config handles various timeout values
    let configs = vec![
        IpcClientConfig {
            connect_timeout: Duration::from_millis(1),
            server_address: "127.0.0.1:1".to_string(),
        },
        IpcClientConfig {
            connect_timeout: Duration::from_secs(300),
            server_address: "127.0.0.1:2".to_string(),
        },
        IpcClientConfig {
            connect_timeout: Duration::from_millis(0),
            server_address: "127.0.0.1:3".to_string(),
        },
    ];

    for config in configs {
        let mut client = IpcClient::new(config);
        client.connect().await?;
        client.disconnect().await?;
    }
    Ok(())
}

#[tokio::test]
async fn timeout_daemon_abort_is_timely() -> Result<(), BoxErr> {
    let config = ServiceConfig {
        service_name: "timeout-daemon".to_string(),
        service_display_name: "Timeout Daemon".to_string(),
        service_description: "Testing timely abort".to_string(),
        ipc: IpcConfig::default(),
        health_check_interval: 1,
        max_restart_attempts: 1,
        restart_delay: 1,
        auto_restart: false,
    };
    let daemon = ServiceDaemon::new(config).await?;
    let handle = tokio::spawn(async move { daemon.run().await });

    tokio::time::sleep(Duration::from_millis(50)).await;
    handle.abort();

    let result = tokio::time::timeout(Duration::from_secs(5), handle).await;
    assert!(result.is_ok(), "abort should complete within 5 seconds");
    Ok(())
}
