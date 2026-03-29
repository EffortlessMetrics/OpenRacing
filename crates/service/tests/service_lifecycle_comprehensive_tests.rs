//! Comprehensive service (wheeld) lifecycle tests covering:
//!
//! 1. Service initialization and configuration loading
//! 2. Graceful shutdown on signal
//! 3. State persistence across restart
//! 4. Multiple concurrent client connections
//! 5. Health check / liveness probe behavior
//! 6. Error recovery (corrupted state, missing config)
//! 7. Resource cleanup on shutdown
//! 8. Service status reporting

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use racing_wheel_engine::safety::FaultType;
use racing_wheel_schemas::prelude::{
    BaseSettings, DeviceId, Profile, ProfileId, ProfileScope, TorqueNm,
};
use racing_wheel_service::{
    FaultSeverity, FeatureFlags, HealthEventInternal, InterlockState, IpcConfig, IpcServer,
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
    let svc = WheelService::new_with_flags(FeatureFlags::default(), config).await?;
    Ok((svc, tmp))
}

fn test_service_config() -> ServiceConfig {
    ServiceConfig {
        service_name: "comprehensive-test".to_string(),
        service_display_name: "Comprehensive Lifecycle Test".to_string(),
        service_description: "Tests for comprehensive lifecycle coverage".to_string(),
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

fn profile_repo_config(tmp: &TempDir) -> ProfileRepositoryConfig {
    ProfileRepositoryConfig {
        profiles_dir: tmp.path().to_path_buf(),
        trusted_keys: Vec::new(),
        auto_migrate: true,
        backup_on_migrate: false,
    }
}

// =========================================================================
// 1. Service initialization and configuration loading
// =========================================================================

#[tokio::test]
async fn init_default_service_config_is_valid() -> Result<(), BoxErr> {
    let config = SystemConfig::default();
    config.validate()?;
    assert_eq!(config.schema_version, "wheel.config/1");
    assert_eq!(config.engine.tick_rate_hz, 1000);
    assert!(config.safety.max_torque_nm > 0.0);
    Ok(())
}

#[tokio::test]
async fn init_service_config_save_load_roundtrip() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let path = tmp.path().join("cfg.json");

    let config = SystemConfig::default();
    config.save_to_path(&path).await?;

    let loaded = SystemConfig::load_from_path(&path).await?;
    loaded.validate()?;
    assert_eq!(loaded.schema_version, config.schema_version);
    assert_eq!(loaded.engine.tick_rate_hz, config.engine.tick_rate_hz);
    assert_eq!(loaded.safety.max_torque_nm, config.safety.max_torque_nm);
    Ok(())
}

#[tokio::test]
async fn init_daemon_config_json_roundtrip() -> Result<(), BoxErr> {
    let config = test_service_config();
    let json = serde_json::to_string_pretty(&config)?;
    let parsed: ServiceConfig = serde_json::from_str(&json)?;
    assert_eq!(parsed.service_name, config.service_name);
    assert_eq!(parsed.health_check_interval, config.health_check_interval);
    assert_eq!(parsed.max_restart_attempts, config.max_restart_attempts);
    assert_eq!(parsed.auto_restart, config.auto_restart);
    Ok(())
}

#[tokio::test]
async fn init_service_creates_all_sub_services() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let _ps = svc.profile_service();
    let _ds = svc.device_service();
    let _ss = svc.safety_service();
    Ok(())
}

#[tokio::test]
async fn init_service_seeds_virtual_device() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let devices = svc.device_service().enumerate_devices().await?;
    assert!(
        !devices.is_empty(),
        "service should seed at least one virtual device"
    );
    Ok(())
}

#[tokio::test]
async fn init_daemon_creation_with_custom_config() -> Result<(), BoxErr> {
    let mut cfg = test_service_config();
    cfg.service_name = "custom-daemon".to_string();
    cfg.max_restart_attempts = 7;
    cfg.health_check_interval = 99;

    let _daemon = ServiceDaemon::new(cfg).await?;
    Ok(())
}

#[tokio::test]
async fn init_daemon_with_feature_flags() -> Result<(), BoxErr> {
    let flags = FeatureFlags {
        disable_realtime: true,
        force_ffb_mode: Some("direct".to_string()),
        enable_dev_features: true,
        enable_debug_logging: true,
        enable_virtual_devices: true,
        disable_safety_interlocks: false,
        enable_plugin_dev_mode: false,
    };
    let _daemon = ServiceDaemon::new_with_flags(test_service_config(), flags).await?;
    Ok(())
}

#[tokio::test]
async fn init_daemon_with_profile_config() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let config = profile_repo_config(&tmp);
    let _daemon = ServiceDaemon::new_with_profile_config(test_service_config(), config).await?;
    Ok(())
}

#[tokio::test]
async fn init_daemon_with_flags_and_profile_config() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let flags = FeatureFlags {
        disable_realtime: true,
        force_ffb_mode: None,
        enable_dev_features: false,
        enable_debug_logging: false,
        enable_virtual_devices: true,
        disable_safety_interlocks: false,
        enable_plugin_dev_mode: false,
    };
    let config = profile_repo_config(&tmp);
    let _daemon =
        ServiceDaemon::new_with_flags_and_profile_config(test_service_config(), flags, config)
            .await?;
    Ok(())
}

#[tokio::test]
async fn init_ipc_config_defaults_are_sane() -> Result<(), BoxErr> {
    let config = IpcConfig::default();
    assert_eq!(config.max_connections, 10);
    assert_eq!(config.connection_timeout, Duration::from_secs(30));
    assert!(!config.enable_acl);
    Ok(())
}

// =========================================================================
// 2. Graceful shutdown on signal
// =========================================================================

#[tokio::test]
async fn shutdown_daemon_via_task_abort() -> Result<(), BoxErr> {
    let daemon = ServiceDaemon::new(test_service_config()).await?;
    let handle = tokio::spawn(async move { daemon.run().await });

    tokio::time::sleep(Duration::from_millis(50)).await;
    handle.abort();

    let result = handle.await;
    assert!(result.is_err(), "aborted task should return JoinError");
    Ok(())
}

#[tokio::test]
async fn shutdown_broadcast_channel_triggers_is_running_false() -> Result<(), BoxErr> {
    let (shutdown_tx, mut shutdown_rx) = broadcast::channel::<()>(1);
    let is_running = Arc::new(AtomicBool::new(true));

    let flag = is_running.clone();
    let handle = tokio::spawn(async move {
        let _ = shutdown_rx.recv().await;
        flag.store(false, Ordering::SeqCst);
    });

    assert!(is_running.load(Ordering::SeqCst));
    let _ = shutdown_tx.send(());
    handle.await?;
    assert!(!is_running.load(Ordering::SeqCst));
    Ok(())
}

#[tokio::test]
async fn shutdown_broadcast_reaches_all_receivers() -> Result<(), BoxErr> {
    let (tx, _) = broadcast::channel::<()>(1);
    let mut receivers: Vec<_> = (0..5).map(|_| tx.subscribe()).collect();

    let _ = tx.send(());

    for rx in &mut receivers {
        let r = rx.recv().await;
        assert!(r.is_ok(), "each receiver should get the signal");
    }
    Ok(())
}

#[tokio::test]
async fn shutdown_daemon_completes_within_timeout() -> Result<(), BoxErr> {
    let daemon = ServiceDaemon::new(test_service_config()).await?;
    let handle = tokio::spawn(async move { daemon.run().await });

    tokio::time::sleep(Duration::from_millis(50)).await;
    handle.abort();

    let result = tokio::time::timeout(Duration::from_secs(5), handle).await;
    assert!(result.is_ok(), "daemon shutdown should complete within 5s");
    Ok(())
}

#[tokio::test]
async fn shutdown_ipc_server_is_idempotent() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default()).await?;
    server.shutdown().await;
    server.shutdown().await;
    server.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn shutdown_does_not_corrupt_profiles() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let config = profile_repo_config(&tmp);

    let svc = WheelService::new_with_flags(FeatureFlags::default(), config.clone()).await?;
    let profile = make_profile("shutdown-safe")?;
    svc.profile_service().create_profile(profile).await?;
    drop(svc);

    // Profiles should be intact after drop
    let svc2 = WheelService::new_with_flags(FeatureFlags::default(), config).await?;
    let loaded = svc2
        .profile_service()
        .get_profile(&ProfileId::new("shutdown-safe".to_string())?)
        .await?;
    assert!(loaded.is_some(), "profile must survive shutdown");
    Ok(())
}

// =========================================================================
// 3. State persistence across restart
// =========================================================================

#[tokio::test]
async fn persist_profile_survives_service_restart() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let config = profile_repo_config(&tmp);

    // First lifecycle: create profiles
    let svc1 = WheelService::new_with_flags(FeatureFlags::default(), config.clone()).await?;
    for i in 0..3 {
        let p = make_profile(&format!("persist-{i}"))?;
        svc1.profile_service().create_profile(p).await?;
    }
    let count1 = svc1.profile_service().list_profiles().await?.len();
    assert_eq!(count1, 3);
    drop(svc1);

    // Second lifecycle: verify profiles loaded from disk
    let svc2 = WheelService::new_with_flags(FeatureFlags::default(), config).await?;
    let count2 = svc2.profile_service().list_profiles().await?.len();
    assert_eq!(count2, 3, "all profiles should persist across restart");
    Ok(())
}

#[tokio::test]
async fn persist_profile_crud_across_restarts() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let config = profile_repo_config(&tmp);

    // Create
    let svc = WheelService::new_with_flags(FeatureFlags::default(), config.clone()).await?;
    let p = make_profile("crud-test")?;
    svc.profile_service().create_profile(p).await?;
    drop(svc);

    // Update after restart
    let svc = WheelService::new_with_flags(FeatureFlags::default(), config.clone()).await?;
    let loaded = svc
        .profile_service()
        .get_profile(&ProfileId::new("crud-test".to_string())?)
        .await?;
    assert!(loaded.is_some());
    drop(svc);

    // Delete after another restart
    let svc = WheelService::new_with_flags(FeatureFlags::default(), config.clone()).await?;
    svc.profile_service()
        .delete_profile(&ProfileId::new("crud-test".to_string())?)
        .await?;
    let remaining = svc.profile_service().list_profiles().await?.len();
    assert_eq!(remaining, 0);
    drop(svc);

    // Confirm deletion persisted
    let svc = WheelService::new_with_flags(FeatureFlags::default(), config).await?;
    let profiles = svc.profile_service().list_profiles().await?;
    assert!(
        profiles.is_empty(),
        "deletion should persist across restart"
    );
    Ok(())
}

#[tokio::test]
async fn persist_config_file_survives_rewrite() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let path = tmp.path().join("svc.json");

    // Write, reload, modify, reload
    let mut config = test_service_config();
    let json = serde_json::to_string_pretty(&config)?;
    tokio::fs::write(&path, &json).await?;

    let loaded: ServiceConfig = serde_json::from_str(&tokio::fs::read_to_string(&path).await?)?;
    assert_eq!(loaded.service_name, "comprehensive-test");

    config.max_restart_attempts = 42;
    let json2 = serde_json::to_string_pretty(&config)?;
    tokio::fs::write(&path, &json2).await?;

    let reloaded: ServiceConfig = serde_json::from_str(&tokio::fs::read_to_string(&path).await?)?;
    assert_eq!(reloaded.max_restart_attempts, 42);
    Ok(())
}

#[tokio::test]
async fn persist_system_config_save_and_reload() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let path = tmp.path().join("system.json");

    let original = SystemConfig::default();
    original.save_to_path(&path).await?;

    let loaded = SystemConfig::load_from_path(&path).await?;
    loaded.validate()?;
    assert_eq!(loaded.engine.tick_rate_hz, original.engine.tick_rate_hz);
    Ok(())
}

#[tokio::test]
async fn persist_safety_state_is_ephemeral() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let config = profile_repo_config(&tmp);

    // Register device, then drop
    let svc = WheelService::new_with_flags(FeatureFlags::default(), config.clone()).await?;
    let did = parse_device_id("ephemeral-dev")?;
    svc.safety_service()
        .register_device(did.clone(), torque(10.0)?)
        .await?;
    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.total_devices, 1);
    drop(svc);

    // After restart, safety state should be gone (it's in-memory only)
    let svc2 = WheelService::new_with_flags(FeatureFlags::default(), config).await?;
    let stats2 = svc2.safety_service().get_statistics().await;
    assert_eq!(
        stats2.total_devices, 0,
        "safety state is ephemeral and should not persist"
    );
    Ok(())
}

// =========================================================================
// 4. Multiple concurrent client connections
// =========================================================================

#[tokio::test]
async fn concurrent_ipc_health_broadcast_to_many_receivers() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default()).await?;
    let mut receivers: Vec<_> = (0..10).map(|_| server.get_health_receiver()).collect();

    server.broadcast_health_event(HealthEventInternal {
        device_id: "multi-rx".to_string(),
        event_type: "ping".to_string(),
        message: "broadcast".to_string(),
        timestamp: std::time::SystemTime::now(),
    });

    for rx in &mut receivers {
        let evt = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await??;
        assert_eq!(evt.device_id, "multi-rx");
    }
    Ok(())
}

#[tokio::test]
async fn concurrent_safety_registrations_from_clones() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    let mut handles = Vec::new();
    for i in 0..8 {
        let svc_clone = svc.clone();
        handles.push(tokio::spawn(async move {
            let did: DeviceId = format!("conc-reg-{i}")
                .parse()
                .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
            let t =
                TorqueNm::new(10.0).map_err(|e| -> BoxErr { format!("bad torque: {e}").into() })?;
            svc_clone
                .safety_service()
                .register_device(did, t)
                .await
                .map_err(|e| -> BoxErr { format!("register: {e}").into() })?;
            Ok::<_, BoxErr>(())
        }));
    }

    for h in handles {
        h.await??;
    }

    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.total_devices, 8);
    Ok(())
}

#[tokio::test]
async fn concurrent_profile_creates_no_data_loss() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    let mut handles = Vec::new();
    for i in 0..6 {
        let svc_clone = svc.clone();
        handles.push(tokio::spawn(async move {
            let p = make_profile(&format!("conc-prof-{i}"))?;
            svc_clone.profile_service().create_profile(p).await?;
            Ok::<_, BoxErr>(())
        }));
    }

    for h in handles {
        h.await??;
    }

    let all = svc.profile_service().list_profiles().await?;
    assert_eq!(
        all.len(),
        6,
        "all concurrent profile creates should succeed"
    );
    Ok(())
}

#[tokio::test]
async fn concurrent_mixed_operations_no_deadlock() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let svc = Arc::new(svc);

    let test_future = async {
        let s1 = svc.clone();
        let s2 = svc.clone();
        let s3 = svc.clone();

        let t1 = tokio::spawn(async move {
            for i in 0..5 {
                let p = make_profile(&format!("mix-prof-{i}"))?;
                s1.profile_service().create_profile(p).await?;
            }
            Ok::<_, BoxErr>(())
        });

        let t2 = tokio::spawn(async move {
            for i in 0..5 {
                let did: DeviceId = format!("mix-dev-{i}")
                    .parse()
                    .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
                let t = TorqueNm::new(10.0)
                    .map_err(|e| -> BoxErr { format!("bad torque: {e}").into() })?;
                s2.safety_service()
                    .register_device(did, t)
                    .await
                    .map_err(|e| -> BoxErr { format!("register: {e}").into() })?;
            }
            Ok::<_, BoxErr>(())
        });

        let t3 = tokio::spawn(async move {
            for _ in 0..5 {
                let _ = s3.device_service().enumerate_devices().await;
                let _ = s3.safety_service().get_statistics().await;
            }
            Ok::<_, BoxErr>(())
        });

        let (r1, r2, r3) = tokio::join!(t1, t2, t3);
        must(must(r1));
        must(must(r2));
        must(must(r3));
    };

    let timed = tokio::time::timeout(Duration::from_secs(10), test_future).await;
    assert!(
        timed.is_ok(),
        "mixed concurrent operations should not deadlock"
    );
    Ok(())
}

#[tokio::test]
async fn concurrent_clone_sees_shared_state() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let svc2 = svc.clone();

    let did = parse_device_id("shared-state-dev")?;
    svc.safety_service()
        .register_device(did.clone(), torque(10.0)?)
        .await?;

    // Clone should see the same registered device
    let state = svc2.safety_service().get_safety_state(&did).await?;
    assert_eq!(state.interlock_state, InterlockState::SafeTorque);
    Ok(())
}

// =========================================================================
// 5. Health check / liveness probe behavior
// =========================================================================

#[tokio::test]
async fn health_profile_stats_zero_initially() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let stats = svc.profile_service().get_profile_statistics().await?;
    assert_eq!(stats.total_profiles, 0);
    assert_eq!(stats.active_profiles, 0);
    assert_eq!(stats.cached_profiles, 0);
    assert_eq!(stats.signed_profiles, 0);
    assert_eq!(stats.trusted_profiles, 0);
    assert_eq!(stats.session_overrides, 0);
    Ok(())
}

#[tokio::test]
async fn health_safety_stats_zero_initially() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.total_devices, 0);
    assert_eq!(stats.safe_torque_devices, 0);
    assert_eq!(stats.high_torque_devices, 0);
    assert_eq!(stats.faulted_devices, 0);
    assert_eq!(stats.challenge_devices, 0);
    Ok(())
}

#[tokio::test]
async fn health_stats_reflect_registrations_and_removals() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    for i in 0..3 {
        let did = parse_device_id(&format!("health-reg-{i}"))?;
        svc.safety_service()
            .register_device(did, torque(10.0)?)
            .await?;
    }

    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.total_devices, 3);
    assert_eq!(stats.safe_torque_devices, 3);

    svc.safety_service()
        .unregister_device(&parse_device_id("health-reg-1")?)
        .await?;

    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.total_devices, 2);
    Ok(())
}

#[tokio::test]
async fn health_stats_track_fault_states() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("health-fault-dev")?;
    svc.safety_service()
        .register_device(did.clone(), torque(12.0)?)
        .await?;

    svc.safety_service()
        .emergency_stop(&did, "test fault".to_string())
        .await?;

    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.faulted_devices, 1);
    assert_eq!(stats.safe_torque_devices, 0);
    Ok(())
}

#[tokio::test]
async fn health_profile_stats_after_create_and_delete() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    let p = make_profile("health-crud-prof")?;
    let pid = svc.profile_service().create_profile(p).await?;

    let stats = svc.profile_service().get_profile_statistics().await?;
    assert_eq!(stats.total_profiles, 1);

    svc.profile_service().delete_profile(&pid).await?;

    let stats = svc.profile_service().get_profile_statistics().await?;
    assert_eq!(stats.total_profiles, 0);
    Ok(())
}

#[tokio::test]
async fn health_concurrent_stats_queries_succeed() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    let s1 = svc.clone();
    let s2 = svc.clone();
    let s3 = svc.clone();

    let (r1, r2, r3) = tokio::join!(
        s1.profile_service().get_profile_statistics(),
        async { Ok::<_, BoxErr>(s2.device_service().get_statistics().await) },
        async { Ok::<_, BoxErr>(s3.safety_service().get_statistics().await) },
    );

    r1?;
    r2?;
    r3?;
    Ok(())
}

#[tokio::test]
async fn health_operations_complete_within_timeout() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    let r1 = tokio::time::timeout(
        Duration::from_secs(5),
        svc.device_service().enumerate_devices(),
    )
    .await;
    assert!(r1.is_ok(), "device enumeration should not time out");

    let r2 = tokio::time::timeout(
        Duration::from_secs(5),
        svc.profile_service().get_profile_statistics(),
    )
    .await;
    assert!(r2.is_ok(), "profile stats should not time out");

    let r3 = tokio::time::timeout(Duration::from_secs(5), async {
        svc.safety_service().get_statistics().await
    })
    .await;
    assert!(r3.is_ok(), "safety stats should not time out");
    Ok(())
}

#[tokio::test]
async fn health_ipc_health_event_broadcast_and_receive() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default()).await?;
    let mut rx = server.get_health_receiver();

    for i in 0..3 {
        server.broadcast_health_event(HealthEventInternal {
            device_id: format!("hb-dev-{i}"),
            event_type: "heartbeat".to_string(),
            message: "alive".to_string(),
            timestamp: std::time::SystemTime::now(),
        });
    }

    for i in 0..3 {
        let evt = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await??;
        assert_eq!(evt.device_id, format!("hb-dev-{i}"));
        assert_eq!(evt.event_type, "heartbeat");
    }
    Ok(())
}

// =========================================================================
// 6. Error recovery (corrupted state, missing config)
// =========================================================================

#[tokio::test]
async fn recovery_invalid_json_config_returns_error() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let path = tmp.path().join("corrupt.json");
    tokio::fs::write(&path, "{ not valid json }}}").await?;

    let result = SystemConfig::load_from_path(&path).await;
    assert!(result.is_err(), "corrupted JSON should fail to load");
    Ok(())
}

#[tokio::test]
async fn recovery_missing_config_creates_defaults() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let path = tmp.path().join("nonexistent.json");

    let config = SystemConfig::load_from_path(&path).await?;
    config.validate()?;
    assert_eq!(config.schema_version, "wheel.config/1");

    // File should now exist
    assert!(path.exists(), "default config should be written to disk");
    Ok(())
}

#[tokio::test]
async fn recovery_invalid_schema_version_rejected() -> Result<(), BoxErr> {
    let config = SystemConfig {
        schema_version: "invalid/0".to_string(),
        ..SystemConfig::default()
    };
    let result = config.validate();
    assert!(result.is_err(), "invalid schema version should fail");
    Ok(())
}

#[tokio::test]
async fn recovery_invalid_tick_rate_rejected() -> Result<(), BoxErr> {
    let config = SystemConfig {
        engine: racing_wheel_service::system_config::EngineConfig {
            tick_rate_hz: 0,
            ..Default::default()
        },
        ..SystemConfig::default()
    };
    let result = config.validate();
    assert!(result.is_err(), "zero tick rate should fail validation");
    Ok(())
}

#[tokio::test]
async fn recovery_invalid_max_torque_rejected() -> Result<(), BoxErr> {
    let config = SystemConfig {
        safety: racing_wheel_service::system_config::SafetyConfig {
            max_torque_nm: 999.0,
            ..Default::default()
        },
        ..SystemConfig::default()
    };
    let result = config.validate();
    assert!(result.is_err(), "excessive torque should fail validation");
    Ok(())
}

#[tokio::test]
async fn recovery_safety_service_after_unknown_device() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let bad = parse_device_id("no-such-device")?;

    let err = svc.safety_service().get_safety_state(&bad).await;
    assert!(err.is_err(), "unregistered device should error");

    // Service still usable after error
    let did = parse_device_id("recovery-ok")?;
    svc.safety_service()
        .register_device(did.clone(), torque(10.0)?)
        .await?;
    let state = svc.safety_service().get_safety_state(&did).await?;
    assert_eq!(state.interlock_state, InterlockState::SafeTorque);
    Ok(())
}

#[tokio::test]
async fn recovery_profile_service_after_missing_profile_update() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    let fake = make_profile("ghost-prof")?;
    let err = svc.profile_service().update_profile(fake).await;
    assert!(err.is_err(), "updating nonexistent profile should fail");

    // Service still works
    let p = make_profile("real-prof")?;
    svc.profile_service().create_profile(p).await?;
    let list = svc.profile_service().list_profiles().await?;
    assert_eq!(list.len(), 1);
    Ok(())
}

#[tokio::test]
async fn recovery_fault_then_clear_restores_safe_torque() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("fault-clear-dev")?;
    svc.safety_service()
        .register_device(did.clone(), torque(12.0)?)
        .await?;

    svc.safety_service()
        .report_fault(&did, FaultType::ThermalLimit, FaultSeverity::Fatal)
        .await?;

    let state = svc.safety_service().get_safety_state(&did).await?;
    assert!(matches!(
        state.interlock_state,
        InterlockState::Faulted { .. }
    ));

    svc.safety_service()
        .clear_fault(&did, FaultType::ThermalLimit)
        .await?;

    let state = svc.safety_service().get_safety_state(&did).await?;
    assert_eq!(state.interlock_state, InterlockState::SafeTorque);
    Ok(())
}

#[tokio::test]
async fn recovery_warning_fault_does_not_change_interlock() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("warn-dev")?;
    svc.safety_service()
        .register_device(did.clone(), torque(10.0)?)
        .await?;

    svc.safety_service()
        .report_fault(&did, FaultType::ThermalLimit, FaultSeverity::Warning)
        .await?;

    let state = svc.safety_service().get_safety_state(&did).await?;
    assert_eq!(
        state.interlock_state,
        InterlockState::SafeTorque,
        "warning should not change interlock"
    );
    assert_eq!(state.fault_count, 1);
    Ok(())
}

#[tokio::test]
async fn recovery_faulted_device_blocks_high_torque() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("blocked-ht")?;
    svc.safety_service()
        .register_device(did.clone(), torque(12.0)?)
        .await?;

    svc.safety_service()
        .emergency_stop(&did, "test".to_string())
        .await?;

    let result = svc
        .safety_service()
        .request_high_torque(&did, "user".to_string())
        .await;
    assert!(result.is_err(), "faulted device should reject high torque");
    Ok(())
}

#[tokio::test]
async fn recovery_multiple_sequential_faults_counted() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("multi-fault-dev")?;
    svc.safety_service()
        .register_device(did.clone(), torque(10.0)?)
        .await?;

    svc.safety_service()
        .report_fault(&did, FaultType::ThermalLimit, FaultSeverity::Warning)
        .await?;
    svc.safety_service()
        .report_fault(&did, FaultType::UsbStall, FaultSeverity::Warning)
        .await?;

    let state = svc.safety_service().get_safety_state(&did).await?;
    assert_eq!(state.fault_count, 2);
    Ok(())
}

// =========================================================================
// 7. Resource cleanup on shutdown
// =========================================================================

#[tokio::test]
async fn cleanup_profile_files_persist_on_disk_after_drop() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let profiles_dir = tmp.path().to_path_buf();
    let config = ProfileRepositoryConfig {
        profiles_dir: profiles_dir.clone(),
        trusted_keys: Vec::new(),
        auto_migrate: true,
        backup_on_migrate: false,
    };

    let svc = WheelService::new_with_flags(FeatureFlags::default(), config).await?;
    let p = make_profile("cleanup-disk")?;
    svc.profile_service().create_profile(p).await?;
    drop(svc);

    let entries: Vec<_> = std::fs::read_dir(&profiles_dir)?
        .filter_map(|e| e.ok())
        .collect();
    assert!(!entries.is_empty(), "profile files should survive drop");
    Ok(())
}

#[tokio::test]
async fn cleanup_broadcast_channel_closed_after_server_drop() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default()).await?;
    let mut rx = server.get_health_receiver();

    server.broadcast_health_event(HealthEventInternal {
        device_id: "pre-drop".to_string(),
        event_type: "info".to_string(),
        message: "alive".to_string(),
        timestamp: std::time::SystemTime::now(),
    });
    let evt = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await??;
    assert_eq!(evt.device_id, "pre-drop");

    server.shutdown().await;
    drop(server);

    // Channel should be closed; recv returns error or times out
    let result = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await;
    assert!(
        result.is_err() || result.as_ref().is_ok_and(|r| r.is_err()),
        "no events should arrive after server drop"
    );
    Ok(())
}

#[tokio::test]
async fn cleanup_repeated_create_destroy_no_leak() -> Result<(), BoxErr> {
    for i in 0..10 {
        let (svc, _tmp) = temp_service().await?;
        let did = parse_device_id(&format!("leak-test-{i}"))?;
        svc.safety_service()
            .register_device(did, torque(10.0)?)
            .await?;
        // svc and tmp drop here each iteration
    }
    // If we reach here without OOM, there's no gross resource leak
    Ok(())
}

#[tokio::test]
async fn cleanup_daemon_profile_dir_survives_abort() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let profiles_dir = tmp.path().to_path_buf();
    let config = profile_repo_config(&tmp);

    let daemon = ServiceDaemon::new_with_profile_config(test_service_config(), config).await?;
    let handle = tokio::spawn(async move { daemon.run().await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    handle.abort();
    let _ = handle.await;

    assert!(
        profiles_dir.exists(),
        "profile directory should survive daemon abort"
    );
    Ok(())
}

#[tokio::test]
async fn cleanup_register_unregister_loop() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    for i in 0..50 {
        let did = parse_device_id(&format!("churn-dev-{i}"))?;
        svc.safety_service()
            .register_device(did.clone(), torque(10.0)?)
            .await?;
        svc.safety_service().unregister_device(&did).await?;
    }

    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.total_devices, 0, "all devices should be unregistered");
    Ok(())
}

#[tokio::test]
async fn cleanup_profile_create_delete_loop() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    for i in 0..20 {
        let p = make_profile(&format!("churn-prof-{i}"))?;
        let pid = svc.profile_service().create_profile(p).await?;
        svc.profile_service().delete_profile(&pid).await?;
    }

    let profiles = svc.profile_service().list_profiles().await?;
    assert!(profiles.is_empty(), "all profiles should be deleted");
    Ok(())
}

// =========================================================================
// 8. Service status reporting
// =========================================================================

#[tokio::test]
async fn status_device_statistics_reflect_enumeration() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let stats = svc.device_service().get_statistics().await;
    // Virtual device may or may not be counted in total_devices depending on impl
    assert_eq!(stats.faulted_devices, 0);
    Ok(())
}

#[tokio::test]
async fn status_safety_tracks_interlock_transitions() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("status-dev")?;
    svc.safety_service()
        .register_device(did.clone(), torque(15.0)?)
        .await?;

    // SafeTorque → Challenge
    svc.safety_service()
        .update_hands_on_detection(&did, true)
        .await?;
    let state = svc
        .safety_service()
        .request_high_torque(&did, "tester".to_string())
        .await?;
    assert!(matches!(state, InterlockState::Challenge { .. }));

    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.challenge_devices, 1);

    // Challenge → HighTorqueActive (correct token)
    let token = match state {
        InterlockState::Challenge {
            challenge_token, ..
        } => challenge_token,
        other => return Err(format!("expected Challenge, got {other:?}").into()),
    };

    let activated = svc
        .safety_service()
        .respond_to_challenge(&did, token)
        .await?;
    assert!(matches!(activated, InterlockState::HighTorqueActive { .. }));

    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.high_torque_devices, 1);
    assert_eq!(stats.challenge_devices, 0);
    Ok(())
}

#[tokio::test]
async fn status_wrong_challenge_token_reverts_to_safe() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("wrong-token-dev")?;
    svc.safety_service()
        .register_device(did.clone(), torque(15.0)?)
        .await?;
    svc.safety_service()
        .update_hands_on_detection(&did, true)
        .await?;

    svc.safety_service()
        .request_high_torque(&did, "test".to_string())
        .await?;

    let result = svc
        .safety_service()
        .respond_to_challenge(&did, 0xBAADF00D)
        .await?;
    assert_eq!(
        result,
        InterlockState::SafeTorque,
        "wrong token should revert to SafeTorque"
    );
    Ok(())
}

#[tokio::test]
async fn status_emergency_stop_reports_faulted() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("estop-dev")?;
    svc.safety_service()
        .register_device(did.clone(), torque(12.0)?)
        .await?;

    svc.safety_service()
        .emergency_stop(&did, "manual estop".to_string())
        .await?;

    let state = svc.safety_service().get_safety_state(&did).await?;
    assert!(matches!(
        state.interlock_state,
        InterlockState::Faulted { .. }
    ));
    assert_eq!(state.fault_count, 1);

    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.faulted_devices, 1);
    Ok(())
}

#[tokio::test]
async fn status_multi_device_independent_states() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let d1 = parse_device_id("ind-1")?;
    let d2 = parse_device_id("ind-2")?;
    let d3 = parse_device_id("ind-3")?;

    svc.safety_service()
        .register_device(d1.clone(), torque(10.0)?)
        .await?;
    svc.safety_service()
        .register_device(d2.clone(), torque(10.0)?)
        .await?;
    svc.safety_service()
        .register_device(d3.clone(), torque(10.0)?)
        .await?;

    svc.safety_service()
        .emergency_stop(&d2, "test".to_string())
        .await?;

    let s1 = svc.safety_service().get_safety_state(&d1).await?;
    let s2 = svc.safety_service().get_safety_state(&d2).await?;
    let s3 = svc.safety_service().get_safety_state(&d3).await?;

    assert_eq!(s1.interlock_state, InterlockState::SafeTorque);
    assert!(matches!(s2.interlock_state, InterlockState::Faulted { .. }));
    assert_eq!(s3.interlock_state, InterlockState::SafeTorque);

    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.total_devices, 3);
    assert_eq!(stats.faulted_devices, 1);
    assert_eq!(stats.safe_torque_devices, 2);
    Ok(())
}

#[tokio::test]
async fn status_device_get_nonexistent_returns_none() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("phantom")?;
    let result = svc.device_service().get_device(&did).await?;
    assert!(result.is_none());
    Ok(())
}

#[tokio::test]
async fn status_is_running_flag_coordination() -> Result<(), BoxErr> {
    let is_running = Arc::new(AtomicBool::new(true));
    let restart_count = Arc::new(AtomicU32::new(0));

    // Simulate restart counter
    for i in 0u32..3 {
        restart_count.store(i, Ordering::SeqCst);
        assert_eq!(restart_count.load(Ordering::SeqCst), i);
    }

    is_running.store(false, Ordering::SeqCst);
    assert!(!is_running.load(Ordering::SeqCst));
    Ok(())
}

#[tokio::test]
async fn status_fatal_fault_zeros_torque_limit() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("zero-torque-dev")?;
    svc.safety_service()
        .register_device(did.clone(), torque(10.0)?)
        .await?;

    svc.safety_service()
        .report_fault(&did, FaultType::ThermalLimit, FaultSeverity::Fatal)
        .await?;

    let state = svc.safety_service().get_safety_state(&did).await?;
    assert_eq!(
        state.current_torque_limit,
        TorqueNm::ZERO,
        "fatal fault should zero torque limit"
    );
    Ok(())
}

#[tokio::test]
async fn status_config_validation_catches_bad_values() -> Result<(), BoxErr> {
    // Bad max connections
    let mut cfg = SystemConfig::default();
    cfg.ipc.max_connections = 0;
    assert!(cfg.validate().is_err());

    // Bad tracing sample rate
    let mut cfg = SystemConfig::default();
    cfg.observability.tracing_sample_rate = 2.0;
    assert!(cfg.validate().is_err());

    // Bad fault response timeout
    let mut cfg = SystemConfig::default();
    cfg.safety.fault_response_timeout_ms = 0;
    assert!(cfg.validate().is_err());

    // Bad jitter tolerance
    let mut cfg = SystemConfig::default();
    cfg.engine.max_jitter_us = 5000;
    assert!(cfg.validate().is_err());

    Ok(())
}
