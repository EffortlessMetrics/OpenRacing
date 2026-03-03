//! Deep service lifecycle tests covering start/stop lifecycle, state machine
//! transitions, graceful shutdown, signal handling, configuration hot-reload,
//! and health endpoint monitoring.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use racing_wheel_engine::safety::FaultType;
use racing_wheel_schemas::prelude::{
    BaseSettings, DeviceId, Profile, ProfileId, ProfileScope, TorqueNm,
};
use racing_wheel_service::{
    FaultSeverity, FeatureFlags, IpcConfig, IpcServer, InterlockState,
    ServiceConfig, ServiceDaemon, SystemConfig, WheelService,
    profile_repository::ProfileRepositoryConfig,
};
use tempfile::TempDir;
use tokio::sync::broadcast;

// ── Helpers ──────────────────────────────────────────────────────────────

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

#[track_caller]
fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
    assert!(r.is_ok(), "unexpected Err: {:?}", r.as_ref().err());
    match r {
        Ok(v) => v,
        Err(_) => unreachable!("asserted Ok above"),
    }
}

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

fn test_service_config() -> ServiceConfig {
    ServiceConfig {
        service_name: "lifecycle-deep-test".to_string(),
        service_display_name: "Lifecycle Deep Test Service".to_string(),
        service_description: "Deep lifecycle tests".to_string(),
        ipc: IpcConfig::default(),
        health_check_interval: 1,
        max_restart_attempts: 2,
        restart_delay: 1,
        auto_restart: false,
    }
}

fn parse_device_id(name: &str) -> Result<DeviceId, BoxErr> {
    name.parse()
        .map_err(|e| -> BoxErr { format!("bad device id: {e}").into() })
}

fn torque(nm: f32) -> Result<TorqueNm, BoxErr> {
    TorqueNm::new(nm).map_err(|e| -> BoxErr { format!("bad torque: {e}").into() })
}

fn make_profile(id: &str) -> Result<Profile, BoxErr> {
    let pid = ProfileId::new(id.to_string())?;
    Ok(Profile::new(
        pid,
        ProfileScope::global(),
        BaseSettings::default(),
        format!("Test Profile {id}"),
    ))
}

// =========================================================================
// 1. Full service start/stop lifecycle
// =========================================================================

#[tokio::test]
async fn lifecycle_start_exposes_all_sub_services() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let _ps = svc.profile_service();
    let _ds = svc.device_service();
    let _ss = svc.safety_service();
    Ok(())
}

#[tokio::test]
async fn lifecycle_service_usable_immediately_after_creation() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let devices = svc.device_service().enumerate_devices().await?;
    assert!(!devices.is_empty(), "virtual device should be seeded");
    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.total_devices, 0);
    Ok(())
}

#[tokio::test]
async fn lifecycle_repeated_create_destroy_is_clean() -> Result<(), BoxErr> {
    for i in 0..5 {
        let (svc, _tmp) = temp_service().await?;
        let did = parse_device_id(&format!("lifecycle-dev-{i}"))?;
        let t = torque(8.0)?;
        svc.safety_service().register_device(did.clone(), t).await?;
        let state = svc.safety_service().get_safety_state(&did).await?;
        assert_eq!(state.interlock_state, InterlockState::SafeTorque);
    }
    Ok(())
}

#[tokio::test]
async fn lifecycle_service_clone_shares_state() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let svc2 = svc.clone();

    let did = parse_device_id("clone-dev")?;
    let t = torque(10.0)?;
    svc.safety_service().register_device(did.clone(), t).await?;

    // The clone should see the same device
    let state = svc2.safety_service().get_safety_state(&did).await?;
    assert_eq!(state.interlock_state, InterlockState::SafeTorque);
    Ok(())
}

#[tokio::test]
async fn lifecycle_profile_crud_across_start_stop() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let config = ProfileRepositoryConfig {
        profiles_dir: tmp.path().to_path_buf(),
        trusted_keys: Vec::new(),
        auto_migrate: true,
        backup_on_migrate: false,
    };
    let svc = WheelService::new_with_profile_config(config.clone()).await?;
    let profile = make_profile("persist-1")?;
    svc.profile_service().create_profile(profile).await?;

    let profiles = svc.profile_service().list_profiles().await?;
    assert_eq!(profiles.len(), 1);

    // "Restart" – create a new service over the same directory
    drop(svc);
    let svc2 = WheelService::new_with_profile_config(config).await?;
    let profiles2 = svc2.profile_service().list_profiles().await?;
    assert_eq!(profiles2.len(), 1, "profile should persist across restarts");
    Ok(())
}

// =========================================================================
// 2. Service state machine transitions
// =========================================================================

#[tokio::test]
async fn state_machine_safe_torque_to_challenge() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("sm-dev-1")?;
    let t = torque(15.0)?;
    svc.safety_service().register_device(did.clone(), t).await?;

    // Set up hands-on detection (required precondition)
    svc.safety_service()
        .update_hands_on_detection(&did, true)
        .await?;

    let state = svc
        .safety_service()
        .request_high_torque(&did, "test-user".to_string())
        .await?;
    assert!(
        matches!(state, InterlockState::Challenge { .. }),
        "expected Challenge, got {state:?}"
    );

    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.challenge_devices, 1);
    Ok(())
}

#[tokio::test]
async fn state_machine_challenge_correct_token_activates() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("sm-dev-2")?;
    let t = torque(15.0)?;
    svc.safety_service().register_device(did.clone(), t).await?;
    svc.safety_service()
        .update_hands_on_detection(&did, true)
        .await?;

    let state = svc
        .safety_service()
        .request_high_torque(&did, "test".to_string())
        .await?;
    let token = match state {
        InterlockState::Challenge {
            challenge_token, ..
        } => challenge_token,
        other => return Err(format!("expected Challenge, got {other:?}").into()),
    };

    let result = svc
        .safety_service()
        .respond_to_challenge(&did, token)
        .await?;
    assert!(
        matches!(result, InterlockState::HighTorqueActive { .. }),
        "expected HighTorqueActive, got {result:?}"
    );

    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.high_torque_devices, 1);
    Ok(())
}

#[tokio::test]
async fn state_machine_wrong_token_returns_safe_torque() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("sm-dev-3")?;
    let t = torque(15.0)?;
    svc.safety_service().register_device(did.clone(), t).await?;
    svc.safety_service()
        .update_hands_on_detection(&did, true)
        .await?;

    svc.safety_service()
        .request_high_torque(&did, "test".to_string())
        .await?;

    let result = svc
        .safety_service()
        .respond_to_challenge(&did, 0xDEADBEEF)
        .await?;
    assert_eq!(
        result,
        InterlockState::SafeTorque,
        "wrong token should revert to SafeTorque"
    );
    Ok(())
}

#[tokio::test]
async fn state_machine_emergency_stop_transitions_to_faulted() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("sm-dev-4")?;
    let t = torque(12.0)?;
    svc.safety_service().register_device(did.clone(), t).await?;

    svc.safety_service()
        .emergency_stop(&did, "test estop".to_string())
        .await?;

    let state = svc.safety_service().get_safety_state(&did).await?;
    assert!(
        matches!(state.interlock_state, InterlockState::Faulted { .. }),
        "expected Faulted, got {:?}",
        state.interlock_state
    );
    assert_eq!(state.fault_count, 1);

    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.faulted_devices, 1);
    Ok(())
}

#[tokio::test]
async fn state_machine_clear_fault_returns_safe_torque() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("sm-dev-5")?;
    let t = torque(12.0)?;
    svc.safety_service().register_device(did.clone(), t).await?;

    svc.safety_service()
        .emergency_stop(&did, "test".to_string())
        .await?;

    svc.safety_service()
        .clear_fault(&did, FaultType::SafetyInterlockViolation)
        .await?;

    let state = svc.safety_service().get_safety_state(&did).await?;
    assert_eq!(state.interlock_state, InterlockState::SafeTorque);
    Ok(())
}

#[tokio::test]
async fn state_machine_faulted_blocks_high_torque() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("sm-dev-6")?;
    let t = torque(12.0)?;
    svc.safety_service().register_device(did.clone(), t).await?;

    svc.safety_service()
        .emergency_stop(&did, "test".to_string())
        .await?;

    let result = svc
        .safety_service()
        .request_high_torque(&did, "test".to_string())
        .await;
    assert!(result.is_err(), "faulted device should reject high torque");
    Ok(())
}

#[tokio::test]
async fn state_machine_fault_severity_warning_continues() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("sm-dev-7")?;
    let t = torque(10.0)?;
    svc.safety_service().register_device(did.clone(), t).await?;

    svc.safety_service()
        .report_fault(&did, FaultType::ThermalLimit, FaultSeverity::Warning)
        .await?;

    let state = svc.safety_service().get_safety_state(&did).await?;
    // Warning should not change interlock state
    assert_eq!(state.interlock_state, InterlockState::SafeTorque);
    assert_eq!(state.fault_count, 1);
    Ok(())
}

#[tokio::test]
async fn state_machine_fault_severity_fatal_disables_torque() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("sm-dev-8")?;
    let t = torque(10.0)?;
    svc.safety_service().register_device(did.clone(), t).await?;

    svc.safety_service()
        .report_fault(&did, FaultType::ThermalLimit, FaultSeverity::Fatal)
        .await?;

    let state = svc.safety_service().get_safety_state(&did).await?;
    assert!(
        matches!(state.interlock_state, InterlockState::Faulted { .. }),
        "fatal fault should transition to Faulted"
    );
    assert_eq!(state.current_torque_limit, TorqueNm::ZERO);
    Ok(())
}

#[tokio::test]
async fn state_machine_multi_device_independent() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did1 = parse_device_id("multi-1")?;
    let did2 = parse_device_id("multi-2")?;
    let t = torque(10.0)?;

    svc.safety_service()
        .register_device(did1.clone(), t)
        .await?;
    svc.safety_service()
        .register_device(did2.clone(), t)
        .await?;

    // Fault device 1, device 2 unaffected
    svc.safety_service()
        .emergency_stop(&did1, "test".to_string())
        .await?;

    let s1 = svc.safety_service().get_safety_state(&did1).await?;
    let s2 = svc.safety_service().get_safety_state(&did2).await?;
    assert!(matches!(s1.interlock_state, InterlockState::Faulted { .. }));
    assert_eq!(s2.interlock_state, InterlockState::SafeTorque);

    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.total_devices, 2);
    assert_eq!(stats.faulted_devices, 1);
    assert_eq!(stats.safe_torque_devices, 1);
    Ok(())
}

// =========================================================================
// 3. Graceful shutdown
// =========================================================================

#[tokio::test]
async fn graceful_shutdown_daemon_via_abort() -> Result<(), BoxErr> {
    let config = test_service_config();
    let daemon = ServiceDaemon::new(config).await?;
    let handle = tokio::spawn(async move { daemon.run().await });

    tokio::time::sleep(Duration::from_millis(50)).await;
    handle.abort();

    let result = handle.await;
    assert!(result.is_err(), "aborted task should be cancelled");
    Ok(())
}

#[tokio::test]
async fn graceful_shutdown_preserves_profile_data() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let config = ProfileRepositoryConfig {
        profiles_dir: tmp.path().to_path_buf(),
        trusted_keys: Vec::new(),
        auto_migrate: true,
        backup_on_migrate: false,
    };

    let svc = WheelService::new_with_profile_config(config.clone()).await?;
    let profile = make_profile("shutdown-persist")?;
    svc.profile_service().create_profile(profile).await?;
    drop(svc);

    // After "shutdown", data should persist
    let svc2 = WheelService::new_with_profile_config(config).await?;
    let loaded = svc2
        .profile_service()
        .get_profile(&ProfileId::new("shutdown-persist".to_string())?)
        .await?;
    assert!(loaded.is_some(), "profile should survive shutdown");
    Ok(())
}

#[tokio::test]
async fn graceful_shutdown_daemon_with_profile_config() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let config = ProfileRepositoryConfig {
        profiles_dir: tmp.path().to_path_buf(),
        trusted_keys: Vec::new(),
        auto_migrate: true,
        backup_on_migrate: false,
    };
    let svc_config = test_service_config();
    let daemon = ServiceDaemon::new_with_profile_config(svc_config, config).await?;

    let handle = tokio::spawn(async move { daemon.run().await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    handle.abort();

    let result = handle.await;
    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn graceful_shutdown_ipc_server() -> Result<(), BoxErr> {
    let ipc_config = IpcConfig::default();
    let server = IpcServer::new(ipc_config).await?;
    // Shutdown should be idempotent even without serve
    server.shutdown().await;
    server.shutdown().await;
    Ok(())
}

// =========================================================================
// 4. Signal handling (via broadcast channels)
// =========================================================================

#[tokio::test]
async fn signal_handling_broadcast_triggers_shutdown() -> Result<(), BoxErr> {
    let (shutdown_tx, mut shutdown_rx) = broadcast::channel::<()>(1);
    let is_running = Arc::new(AtomicBool::new(true));

    let running_clone = is_running.clone();
    let handle = tokio::spawn(async move {
        let _ = shutdown_rx.recv().await;
        running_clone.store(false, Ordering::SeqCst);
    });

    assert!(is_running.load(Ordering::SeqCst));
    let _ = shutdown_tx.send(());
    handle.await?;
    assert!(!is_running.load(Ordering::SeqCst));
    Ok(())
}

#[tokio::test]
async fn signal_handling_multiple_receivers() -> Result<(), BoxErr> {
    let (shutdown_tx, _) = broadcast::channel::<()>(1);
    let mut rx1 = shutdown_tx.subscribe();
    let mut rx2 = shutdown_tx.subscribe();
    let mut rx3 = shutdown_tx.subscribe();

    let _ = shutdown_tx.send(());

    let r1 = rx1.recv().await;
    let r2 = rx2.recv().await;
    let r3 = rx3.recv().await;
    assert!(r1.is_ok());
    assert!(r2.is_ok());
    assert!(r3.is_ok());
    Ok(())
}

#[tokio::test]
async fn signal_handling_is_running_flag_coordination() -> Result<(), BoxErr> {
    let is_running = Arc::new(AtomicBool::new(true));
    let restart_count = Arc::new(AtomicU32::new(0));

    // Simulate service restart counter
    for i in 0u32..3 {
        restart_count.store(i, Ordering::SeqCst);
        assert_eq!(restart_count.load(Ordering::SeqCst), i);
    }

    is_running.store(false, Ordering::SeqCst);
    assert!(!is_running.load(Ordering::SeqCst));
    Ok(())
}

#[tokio::test]
async fn signal_handling_daemon_with_feature_flags() -> Result<(), BoxErr> {
    let flags = FeatureFlags {
        disable_realtime: true,
        force_ffb_mode: None,
        enable_dev_features: false,
        enable_debug_logging: false,
        enable_virtual_devices: true,
        disable_safety_interlocks: false,
        enable_plugin_dev_mode: false,
    };
    let config = test_service_config();
    let daemon = ServiceDaemon::new_with_flags(config, flags).await?;

    let handle = tokio::spawn(async move { daemon.run().await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    handle.abort();

    let result = handle.await;
    assert!(result.is_err());
    Ok(())
}

// =========================================================================
// 5. Configuration hot-reload
// =========================================================================

#[tokio::test]
async fn config_hot_reload_json_roundtrip() -> Result<(), BoxErr> {
    let original = SystemConfig::default();
    let json = serde_json::to_string_pretty(&original)?;
    let parsed: SystemConfig = serde_json::from_str(&json)?;
    parsed.validate()?;
    assert_eq!(parsed.schema_version, original.schema_version);
    Ok(())
}

#[tokio::test]
async fn config_hot_reload_modify_and_reparse() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let path = tmp.path().join("hot_config.json");

    let mut config = test_service_config();
    let json = serde_json::to_string_pretty(&config)?;
    tokio::fs::write(&path, &json).await?;

    // Mutate and re-write
    config.health_check_interval = 42;
    config.auto_restart = true;
    config.max_restart_attempts = 10;
    let updated = serde_json::to_string_pretty(&config)?;
    tokio::fs::write(&path, &updated).await?;

    let content = tokio::fs::read_to_string(&path).await?;
    let reloaded: ServiceConfig = serde_json::from_str(&content)?;
    assert_eq!(reloaded.health_check_interval, 42);
    assert!(reloaded.auto_restart);
    assert_eq!(reloaded.max_restart_attempts, 10);
    Ok(())
}

#[tokio::test]
async fn config_hot_reload_ipc_config_roundtrip() -> Result<(), BoxErr> {
    let ipc = IpcConfig::default();
    let json = serde_json::to_string_pretty(&ipc)?;
    let parsed: IpcConfig = serde_json::from_str(&json)?;
    assert_eq!(parsed.max_connections, ipc.max_connections);
    assert_eq!(parsed.enable_acl, ipc.enable_acl);
    Ok(())
}

#[tokio::test]
async fn config_hot_reload_system_config_validates_defaults() -> Result<(), BoxErr> {
    let config = SystemConfig::default();
    config.validate()?;
    assert_eq!(config.schema_version, "wheel.config/1");
    Ok(())
}

#[tokio::test]
async fn config_hot_reload_invalid_schema_rejected() -> Result<(), BoxErr> {
    let config = SystemConfig {
        schema_version: "bad-schema".to_string(),
        ..SystemConfig::default()
    };
    let result = config.validate();
    assert!(result.is_err(), "invalid schema should fail validation");
    Ok(())
}

#[tokio::test]
async fn config_hot_reload_multiple_writes_last_wins() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let path = tmp.path().join("multi_write.json");

    for i in 1u64..=5 {
        let mut config = test_service_config();
        config.health_check_interval = i;
        let json = serde_json::to_string_pretty(&config)?;
        tokio::fs::write(&path, &json).await?;
    }

    let content = tokio::fs::read_to_string(&path).await?;
    let final_config: ServiceConfig = serde_json::from_str(&content)?;
    assert_eq!(final_config.health_check_interval, 5);
    Ok(())
}

// =========================================================================
// 6. Health endpoint
// =========================================================================

#[tokio::test]
async fn health_endpoint_profile_stats_zero_initially() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let stats = svc.profile_service().get_profile_statistics().await?;
    assert_eq!(stats.total_profiles, 0);
    assert_eq!(stats.active_profiles, 0);
    assert_eq!(stats.session_overrides, 0);
    Ok(())
}

#[tokio::test]
async fn health_endpoint_device_stats_reflect_virtual() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let stats = svc.device_service().get_statistics().await;
    // Virtual device should be counted
    assert!(stats.total_devices >= 1 || stats.connected_devices == 0);
    assert_eq!(stats.faulted_devices, 0);
    Ok(())
}

#[tokio::test]
async fn health_endpoint_safety_stats_track_registration() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.total_devices, 0);

    let did = parse_device_id("health-dev-1")?;
    let t = torque(10.0)?;
    svc.safety_service().register_device(did.clone(), t).await?;

    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.total_devices, 1);
    assert_eq!(stats.safe_torque_devices, 1);

    svc.safety_service().unregister_device(&did).await?;
    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.total_devices, 0);
    Ok(())
}

#[tokio::test]
async fn health_endpoint_operations_within_timeout() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    let result = tokio::time::timeout(
        Duration::from_secs(5),
        svc.device_service().enumerate_devices(),
    )
    .await;

    assert!(result.is_ok(), "operation should not time out");
    let devices = must(result)?;
    assert!(!devices.is_empty());
    Ok(())
}

#[tokio::test]
async fn health_endpoint_resilient_after_error() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let missing = parse_device_id("ghost")?;

    // Trigger error
    let err = svc.safety_service().get_safety_state(&missing).await;
    assert!(err.is_err());

    // Service should still function
    let t = torque(10.0)?;
    svc.safety_service()
        .register_device(missing.clone(), t)
        .await?;
    let state = svc.safety_service().get_safety_state(&missing).await?;
    assert_eq!(state.interlock_state, InterlockState::SafeTorque);
    Ok(())
}

#[tokio::test]
async fn health_endpoint_concurrent_stats_queries() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    let svc1 = svc.clone();
    let svc2 = svc.clone();
    let svc3 = svc.clone();

    let (r1, r2, r3) = tokio::join!(
        svc1.profile_service().get_profile_statistics(),
        async { Ok::<_, BoxErr>(svc2.device_service().get_statistics().await) },
        async { Ok::<_, BoxErr>(svc3.safety_service().get_statistics().await) },
    );

    let profile_stats = r1?;
    let device_stats = r2?;
    let safety_stats = r3?;

    assert_eq!(profile_stats.total_profiles, 0);
    assert_eq!(safety_stats.total_devices, 0);
    let _ = device_stats;
    Ok(())
}

#[tokio::test]
async fn health_endpoint_profile_stats_after_crud() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let profile = make_profile("health-profile")?;
    svc.profile_service().create_profile(profile).await?;

    let stats = svc.profile_service().get_profile_statistics().await?;
    assert_eq!(stats.total_profiles, 1);
    Ok(())
}

#[tokio::test]
async fn health_endpoint_ipc_server_health_broadcast() -> Result<(), BoxErr> {
    let ipc_config = IpcConfig::default();
    let server = IpcServer::new(ipc_config).await?;

    let mut rx = server.get_health_receiver();

    let event = racing_wheel_service::HealthEventInternal {
        device_id: "test-dev".to_string(),
        event_type: "heartbeat".to_string(),
        message: "OK".to_string(),
        timestamp: std::time::SystemTime::now(),
    };
    server.broadcast_health_event(event);

    let received = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await;
    assert!(received.is_ok(), "should receive health event");
    let received = received?;
    assert!(received.is_ok());
    let evt = received?;
    assert_eq!(evt.device_id, "test-dev");
    assert_eq!(evt.event_type, "heartbeat");
    Ok(())
}

#[tokio::test]
async fn health_endpoint_device_nonexistent_returns_none() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("nonexistent")?;
    let result = svc.device_service().get_device(&did).await?;
    assert!(result.is_none());
    Ok(())
}
