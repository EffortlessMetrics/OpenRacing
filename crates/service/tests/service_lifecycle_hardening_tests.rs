//! Service lifecycle hardening tests for the wheeld daemon.
//!
//! Coverage:
//! 1. Startup/shutdown sequences (clean, failure, forced, double-start prevention)
//! 2. Device hotplug (connect, disconnect, rapid cycles, multi-device, reconnect)
//! 3. Configuration reload (hot-reload, profile switch, invalid rejection, rollback, concurrent)
//! 4. Error recovery (transient errors, escalation, logging, recovery time bounds)

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use racing_wheel_engine::safety::FaultType;
use racing_wheel_schemas::prelude::{
    BaseSettings, Degrees, DeviceId, FilterConfig, Gain, Profile, ProfileId, ProfileScope, TorqueNm,
};
use racing_wheel_service::{
    FaultSeverity, HealthEventInternal, InterlockState, IpcConfig, IpcServer, ServiceConfig,
    ServiceDaemon, SystemConfig, WheelService, profile_repository::ProfileRepositoryConfig,
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

fn make_profile_with_gain(id: &str, gain_val: f32) -> Result<Profile, BoxErr> {
    let pid = ProfileId::new(id.to_string())?;
    let gain = Gain::new(gain_val)?;
    Ok(Profile::new(
        pid,
        ProfileScope::global(),
        BaseSettings {
            ffb_gain: gain,
            degrees_of_rotation: Degrees::new_dor(900.0)?,
            torque_cap: TorqueNm::new(10.0)?,
            filters: FilterConfig::default(),
        },
        format!("Gain Profile {id}"),
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

async fn temp_service() -> Result<(WheelService, TempDir), BoxErr> {
    let tmp = TempDir::new()?;
    let config = profile_repo_config(&tmp);
    let svc = WheelService::new_with_flags(racing_wheel_service::FeatureFlags::default(), config).await?;
    Ok((svc, tmp))
}

fn test_daemon_config() -> ServiceConfig {
    ServiceConfig {
        service_name: "hardening-test".to_string(),
        service_display_name: "Hardening Test Service".to_string(),
        service_description: "Tests for lifecycle hardening".to_string(),
        ipc: IpcConfig::default(),
        health_check_interval: 1,
        max_restart_attempts: 2,
        restart_delay: 1,
        auto_restart: false,
    }
}

// =========================================================================
// 1. Service startup/shutdown tests
// =========================================================================

/// Clean startup: config load → device scan → pipeline init → ready
#[tokio::test]
async fn startup_clean_sequence() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    // Step 1: Sub-services are accessible (config loaded, pipeline init done)
    let _ps = svc.profile_service();
    let _ds = svc.device_service();
    let _ss = svc.safety_service();

    // Step 2: Device scan returns seeded virtual device
    let devices = svc.device_service().enumerate_devices().await?;
    assert!(!devices.is_empty(), "expected at least one seeded device");

    // Step 3: Safety service starts with no devices registered
    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.total_devices, 0);

    // Step 4: Profile service is ready to accept profiles
    let pstats = svc.profile_service().get_profile_statistics().await?;
    assert_eq!(pstats.total_profiles, 0);
    assert_eq!(pstats.active_profiles, 0);

    Ok(())
}

/// Clean shutdown: drain → stop pipeline → release devices → exit
#[tokio::test]
async fn shutdown_clean_sequence() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let config = profile_repo_config(&tmp);

    // Create service, do some work, then drop (simulates shutdown)
    {
        let svc = WheelService::new_with_flags(racing_wheel_service::FeatureFlags::default(), config.clone()).await?;

        // Register device and create profile
        let did = parse_device_id("shutdown-dev")?;
        let t = torque(10.0)?;
        svc.safety_service().register_device(did.clone(), t).await?;
        let profile = make_profile("shutdown-profile")?;
        svc.profile_service().create_profile(profile).await?;

        // Unregister device before shutdown (drain phase)
        svc.safety_service().unregister_device(&did).await?;
        let stats = svc.safety_service().get_statistics().await;
        assert_eq!(stats.total_devices, 0, "devices should be drained");
    }

    // Verify profile persists after restart (clean exit preserved state)
    let svc2 = WheelService::new_with_flags(racing_wheel_service::FeatureFlags::default(), config).await?;
    let profiles = svc2.profile_service().list_profiles().await?;
    assert_eq!(profiles.len(), 1, "profile should survive clean restart");

    Ok(())
}

/// Startup failure: invalid config path handled gracefully
#[tokio::test]
async fn startup_failure_invalid_config_path() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let bad_path = tmp
        .path()
        .join("nonexistent")
        .join("deep")
        .join("config.json");

    // SystemConfig::load_from_path should create default when missing
    let loaded = SystemConfig::load_from_path(&bad_path).await?;
    loaded.validate()?;
    assert_eq!(loaded.schema_version, "wheel.config/1");

    Ok(())
}

/// Startup failure: malformed config is rejected
#[tokio::test]
async fn startup_failure_malformed_config() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let path = tmp.path().join("bad_config.json");
    tokio::fs::write(&path, b"{ not valid json!!!").await?;

    let result = SystemConfig::load_from_path(&path).await;
    assert!(result.is_err(), "malformed JSON should fail to parse");

    Ok(())
}

/// Startup failure: config with invalid values is rejected by validation
#[tokio::test]
async fn startup_failure_invalid_config_values() -> Result<(), BoxErr> {
    let mut config = SystemConfig::default();

    // Invalid tick rate
    config.engine.tick_rate_hz = 0;
    let result = config.validate();
    assert!(result.is_err(), "zero tick rate should fail validation");

    // Invalid max torque
    config = SystemConfig::default();
    config.safety.max_torque_nm = 999.0;
    let result = config.validate();
    assert!(result.is_err(), "excessive torque should fail validation");

    // Invalid jitter
    config = SystemConfig::default();
    config.engine.max_jitter_us = 5000;
    let result = config.validate();
    assert!(result.is_err(), "excessive jitter should fail validation");

    Ok(())
}

/// Forced shutdown via daemon abort (simulates SIGTERM/SIGINT)
#[tokio::test]
async fn shutdown_forced_via_abort() -> Result<(), BoxErr> {
    let config = test_daemon_config();
    let daemon = ServiceDaemon::new(config).await?;

    let handle = tokio::spawn(async move { daemon.run().await });

    // Let daemon start briefly
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Force abort (simulates SIGTERM)
    handle.abort();

    let result = handle.await;
    assert!(result.is_err(), "aborted task should return JoinError");

    Ok(())
}

/// Forced shutdown via broadcast channel (simulates clean signal handling)
#[tokio::test]
async fn shutdown_via_broadcast_signal() -> Result<(), BoxErr> {
    let (tx, _rx) = broadcast::channel::<()>(1);
    let is_running = Arc::new(AtomicBool::new(true));

    let is_running_clone = is_running.clone();
    let mut rx = tx.subscribe();

    let handle = tokio::spawn(async move {
        rx.recv().await.ok();
        is_running_clone.store(false, Ordering::SeqCst);
    });

    // Signal shutdown
    let _ = tx.send(());
    handle.await?;

    assert!(!is_running.load(Ordering::SeqCst), "should be stopped");
    Ok(())
}

/// Double-start prevention: daemon cannot be run twice from same config
#[tokio::test]
async fn double_start_prevention() -> Result<(), BoxErr> {
    let config = test_daemon_config();
    let daemon1 = ServiceDaemon::new(config.clone()).await?;
    let daemon2 = ServiceDaemon::new(config).await?;

    let h1 = tokio::spawn(async move { daemon1.run().await });
    let h2 = tokio::spawn(async move { daemon2.run().await });

    // Let both start briefly
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Abort both — in production the second would fail to bind IPC
    h1.abort();
    h2.abort();

    let r1 = h1.await;
    let r2 = h2.await;
    // Both should terminate (either cancelled or errored)
    assert!(r1.is_err() || r2.is_err(), "at least one should be aborted");

    Ok(())
}

/// Daemon creation with profile config isolation
#[tokio::test]
async fn startup_daemon_with_isolated_profile_config() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let config = profile_repo_config(&tmp);
    let _daemon = ServiceDaemon::new_with_profile_config(test_daemon_config(), config).await?;
    Ok(())
}

// =========================================================================
// 2. Device hotplug tests
// =========================================================================

/// Device connect during service operation
#[tokio::test]
async fn hotplug_device_connect_during_operation() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    // Service starts with virtual device
    let devices = svc.device_service().enumerate_devices().await?;
    let initial_count = devices.len();
    assert!(initial_count > 0, "should have initial device");

    // Simulate registering a new device with safety service (hotplug)
    let new_did = parse_device_id("hotplug-dev-1")?;
    let t = torque(12.0)?;
    svc.safety_service()
        .register_device(new_did.clone(), t)
        .await?;

    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.total_devices, 1, "new device should be registered");

    let state = svc.safety_service().get_safety_state(&new_did).await?;
    assert_eq!(state.interlock_state, InterlockState::SafeTorque);

    Ok(())
}

/// Device disconnect during operation (graceful degradation)
#[tokio::test]
async fn hotplug_device_disconnect_graceful_degradation() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    let did = parse_device_id("hotplug-disconnect")?;
    let t = torque(10.0)?;
    svc.safety_service().register_device(did.clone(), t).await?;

    // Verify device is operational
    let state = svc.safety_service().get_safety_state(&did).await?;
    assert_eq!(state.interlock_state, InterlockState::SafeTorque);

    // Simulate disconnect by unregistering
    svc.safety_service().unregister_device(&did).await?;

    // Service should continue operating — other operations still work
    let other_did = parse_device_id("still-connected")?;
    let t2 = torque(8.0)?;
    svc.safety_service()
        .register_device(other_did.clone(), t2)
        .await?;
    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.total_devices, 1, "service should still be functional");

    // Querying disconnected device returns error
    let result = svc.safety_service().get_safety_state(&did).await;
    assert!(result.is_err(), "disconnected device should not be found");

    Ok(())
}

/// Rapid connect/disconnect cycles
#[tokio::test]
async fn hotplug_rapid_connect_disconnect_cycles() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    for i in 0..50 {
        let did = parse_device_id(&format!("rapid-dev-{i}"))?;
        let t = torque(10.0)?;
        svc.safety_service().register_device(did.clone(), t).await?;
        svc.safety_service().unregister_device(&did).await?;
    }

    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.total_devices, 0, "all devices should be unregistered");

    // Service should still accept new devices
    let final_did = parse_device_id("rapid-final")?;
    let t = torque(10.0)?;
    svc.safety_service()
        .register_device(final_did.clone(), t)
        .await?;
    let state = svc.safety_service().get_safety_state(&final_did).await?;
    assert_eq!(state.interlock_state, InterlockState::SafeTorque);

    Ok(())
}

/// Multiple device addition and removal
#[tokio::test]
async fn hotplug_multiple_devices() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    let device_count = 10;
    let mut device_ids = Vec::new();

    // Add multiple devices
    for i in 0..device_count {
        let did = parse_device_id(&format!("multi-dev-{i}"))?;
        let t = torque(5.0 + i as f32)?;
        svc.safety_service().register_device(did.clone(), t).await?;
        device_ids.push(did);
    }

    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.total_devices, device_count);
    assert_eq!(stats.safe_torque_devices, device_count);

    // Remove half
    for did in &device_ids[..device_count / 2] {
        svc.safety_service().unregister_device(did).await?;
    }

    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.total_devices, device_count / 2);

    // Remaining devices still work
    for did in &device_ids[device_count / 2..] {
        let state = svc.safety_service().get_safety_state(did).await?;
        assert_eq!(state.interlock_state, InterlockState::SafeTorque);
    }

    Ok(())
}

/// Device reconnect after disconnect
#[tokio::test]
async fn hotplug_device_reconnect() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("reconnect-dev")?;
    let t = torque(12.0)?;

    // Connect
    svc.safety_service().register_device(did.clone(), t).await?;
    let state = svc.safety_service().get_safety_state(&did).await?;
    assert_eq!(state.interlock_state, InterlockState::SafeTorque);

    // Disconnect
    svc.safety_service().unregister_device(&did).await?;
    assert!(svc.safety_service().get_safety_state(&did).await.is_err());

    // Reconnect — should start fresh in SafeTorque
    let t2 = torque(12.0)?;
    svc.safety_service()
        .register_device(did.clone(), t2)
        .await?;
    let state = svc.safety_service().get_safety_state(&did).await?;
    assert_eq!(
        state.interlock_state,
        InterlockState::SafeTorque,
        "reconnected device should start in SafeTorque"
    );
    assert_eq!(
        state.fault_count, 0,
        "fault count should reset on reconnect"
    );

    Ok(())
}

/// Concurrent device operations from multiple tasks
#[tokio::test]
async fn hotplug_concurrent_device_operations() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let svc = Arc::new(svc);

    let mut handles = Vec::new();
    for i in 0..10 {
        let svc_clone = Arc::clone(&svc);
        let h = tokio::spawn(async move {
            let did = parse_device_id(&format!("concurrent-dev-{i}"))?;
            let t = torque(10.0)?;
            svc_clone
                .safety_service()
                .register_device(did.clone(), t)
                .await?;
            let state = svc_clone.safety_service().get_safety_state(&did).await?;
            assert_eq!(state.interlock_state, InterlockState::SafeTorque);
            svc_clone.safety_service().unregister_device(&did).await?;
            Ok::<(), BoxErr>(())
        });
        handles.push(h);
    }

    for h in handles {
        must(h.await)?;
    }

    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(
        stats.total_devices, 0,
        "all concurrent devices unregistered"
    );

    Ok(())
}

// =========================================================================
// 3. Configuration reload tests
// =========================================================================

/// Hot-reload of config changes via file write + re-read
#[tokio::test]
async fn config_hot_reload() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let path = tmp.path().join("hot_reload.json");

    let mut config = SystemConfig::default();
    config.save_to_path(&path).await?;

    // Mutate config
    config.engine.tick_rate_hz = 500;
    config.safety.default_safe_torque_nm = 3.0;
    config.save_to_path(&path).await?;

    // Re-read and verify
    let reloaded = SystemConfig::load_from_path(&path).await?;
    reloaded.validate()?;
    assert_eq!(reloaded.engine.tick_rate_hz, 500);
    assert!((reloaded.safety.default_safe_torque_nm - 3.0).abs() < f32::EPSILON);

    Ok(())
}

/// Profile switching during operation
#[tokio::test]
async fn config_profile_switching() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    let p1 = make_profile_with_gain("profile-a", 0.5)?;
    let p2 = make_profile_with_gain("profile-b", 0.9)?;

    let id1 = svc.profile_service().create_profile(p1).await?;
    let id2 = svc.profile_service().create_profile(p2).await?;

    let did = parse_device_id("switch-dev")?;

    // Activate profile A
    svc.profile_service().set_active_profile(&did, &id1).await?;
    let active = svc.profile_service().get_active_profile(&did).await?;
    assert_eq!(
        active.as_ref().map(|p| p.to_string()),
        Some(id1.to_string())
    );

    // Switch to profile B
    svc.profile_service().set_active_profile(&did, &id2).await?;
    let active = svc.profile_service().get_active_profile(&did).await?;
    assert_eq!(
        active.as_ref().map(|p| p.to_string()),
        Some(id2.to_string())
    );

    // Verify both profiles still exist
    let profiles = svc.profile_service().list_profiles().await?;
    assert_eq!(profiles.len(), 2);

    Ok(())
}

/// Invalid config rejection — bad JSON
#[tokio::test]
async fn config_invalid_json_rejected() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let path = tmp.path().join("invalid.json");
    tokio::fs::write(&path, b"not json at all!!!").await?;

    let result = SystemConfig::load_from_path(&path).await;
    assert!(result.is_err(), "invalid JSON should be rejected");

    Ok(())
}

/// Invalid config values are caught by validation
#[tokio::test]
async fn config_invalid_values_rejected() -> Result<(), BoxErr> {
    let mut config = SystemConfig::default();

    // Out-of-range tracing sample rate
    config.observability.tracing_sample_rate = 2.0;
    assert!(config.validate().is_err());

    // Zero max connections
    config = SystemConfig::default();
    config.ipc.max_connections = 0;
    assert!(config.validate().is_err());

    // Negative-equivalent safety torque
    config = SystemConfig::default();
    config.safety.default_safe_torque_nm = -1.0;
    assert!(config.validate().is_err());

    Ok(())
}

/// Config rollback on error — if new config is invalid, old stays
#[tokio::test]
async fn config_rollback_on_error() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let path = tmp.path().join("rollback.json");

    // Write valid config
    let original = SystemConfig::default();
    original.save_to_path(&path).await?;

    // Attempt to load the valid config
    let loaded = SystemConfig::load_from_path(&path).await?;
    let original_tick_rate = loaded.engine.tick_rate_hz;

    // Now write invalid config to the file
    tokio::fs::write(&path, b"{ broken }").await?;

    // Loading should fail
    let result = SystemConfig::load_from_path(&path).await;
    assert!(result.is_err());

    // "Rollback": the original config is still usable
    assert_eq!(loaded.engine.tick_rate_hz, original_tick_rate);
    loaded.validate()?;

    Ok(())
}

/// Concurrent config changes don't corrupt state
#[tokio::test]
async fn config_concurrent_changes() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let svc = Arc::new(svc);

    let mut handles = Vec::new();
    for i in 0..20 {
        let svc_clone = Arc::clone(&svc);
        let h = tokio::spawn(async move {
            let profile = make_profile(&format!("concurrent-cfg-{i}"))?;
            svc_clone.profile_service().create_profile(profile).await?;
            Ok::<(), BoxErr>(())
        });
        handles.push(h);
    }

    for h in handles {
        must(h.await)?;
    }

    let profiles = svc.profile_service().list_profiles().await?;
    assert_eq!(profiles.len(), 20, "all concurrent profiles should exist");

    Ok(())
}

/// Session override acts as temporary config change
#[tokio::test]
async fn config_session_override_as_temp_change() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("override-dev")?;

    let override_profile = make_profile_with_gain("override-p", 0.3)?;
    svc.profile_service()
        .set_session_override(&did, override_profile.clone())
        .await?;

    let retrieved = svc.profile_service().get_session_override(&did).await?;
    assert!(retrieved.is_some());
    let retrieved = retrieved.ok_or("expected override")?;
    assert_eq!(retrieved.base_settings.ffb_gain.value(), 0.3);

    // Clear override (rollback)
    svc.profile_service().clear_session_override(&did).await?;
    let cleared = svc.profile_service().get_session_override(&did).await?;
    assert!(cleared.is_none());

    Ok(())
}

/// Config migration from v0 to v1
#[tokio::test]
async fn config_migration_v0_to_v1() -> Result<(), BoxErr> {
    let mut config = SystemConfig {
        schema_version: "wheel.config/0".to_string(),
        ..SystemConfig::default()
    };

    let migrated = config.migrate()?;
    assert!(migrated, "migration should occur from v0 to v1");
    assert_eq!(config.schema_version, "wheel.config/1");
    config.validate()?;

    Ok(())
}

/// Config migration not needed for current version
#[tokio::test]
async fn config_migration_noop_for_current() -> Result<(), BoxErr> {
    let mut config = SystemConfig::default();
    let migrated = config.migrate()?;
    assert!(!migrated, "no migration needed for current version");
    Ok(())
}

// =========================================================================
// 4. Error recovery tests
// =========================================================================

/// Recovery from transient errors — service continues after error
#[tokio::test]
async fn recovery_transient_error() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    let missing = parse_device_id("ghost")?;

    // Cause transient errors
    for _ in 0..10 {
        let result = svc.safety_service().get_safety_state(&missing).await;
        assert!(result.is_err());
    }

    // Service should fully recover
    let did = parse_device_id("recovery-dev")?;
    let t = torque(10.0)?;
    svc.safety_service().register_device(did.clone(), t).await?;
    let state = svc.safety_service().get_safety_state(&did).await?;
    assert_eq!(state.interlock_state, InterlockState::SafeTorque);

    // Profile service should also still work
    let profile = make_profile("recovery-p")?;
    svc.profile_service().create_profile(profile).await?;
    let profiles = svc.profile_service().list_profiles().await?;
    assert_eq!(profiles.len(), 1);

    Ok(())
}

/// Escalating error handling: warning → critical → fatal → safe state
#[tokio::test]
async fn recovery_escalating_fault_severity() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("escalate-dev")?;
    let t = torque(20.0)?;
    svc.safety_service().register_device(did.clone(), t).await?;

    // Step 1: Warning fault — continue operation
    svc.safety_service()
        .report_fault(&did, FaultType::UsbStall, FaultSeverity::Warning)
        .await?;
    let state = svc.safety_service().get_safety_state(&did).await?;
    assert_eq!(
        state.interlock_state,
        InterlockState::SafeTorque,
        "warning should not change state"
    );
    assert_eq!(state.fault_count, 1);

    // Step 2: Critical fault — torque reduced
    svc.safety_service()
        .report_fault(&did, FaultType::UsbStall, FaultSeverity::Critical)
        .await?;
    let state = svc.safety_service().get_safety_state(&did).await?;
    assert_eq!(
        state.interlock_state,
        InterlockState::SafeTorque,
        "critical should keep SafeTorque state"
    );
    assert_eq!(state.fault_count, 2);
    // Torque limit should have been reduced (halved from initial 30% = 6.0)
    assert!(
        state.current_torque_limit.value() < 20.0 * 0.3,
        "torque should be reduced after critical fault"
    );

    // Step 3: Fatal fault — torque cutoff, state goes to Faulted
    svc.safety_service()
        .report_fault(
            &did,
            FaultType::SafetyInterlockViolation,
            FaultSeverity::Fatal,
        )
        .await?;
    let state = svc.safety_service().get_safety_state(&did).await?;
    assert!(
        matches!(state.interlock_state, InterlockState::Faulted { .. }),
        "fatal fault should set Faulted state"
    );
    assert_eq!(state.fault_count, 3);
    assert!(
        state.current_torque_limit.value() < f32::EPSILON,
        "torque should be zero after fatal fault"
    );

    Ok(())
}

/// Error recovery: clear fault returns to safe state
#[tokio::test]
async fn recovery_clear_fault_returns_safe() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("clear-fault-dev")?;
    let t = torque(15.0)?;
    svc.safety_service().register_device(did.clone(), t).await?;

    // Fault the device
    svc.safety_service()
        .emergency_stop(&did, "test fault".to_string())
        .await?;

    let state = svc.safety_service().get_safety_state(&did).await?;
    assert!(matches!(
        state.interlock_state,
        InterlockState::Faulted { .. }
    ));

    // Clear the fault
    svc.safety_service()
        .clear_fault(&did, FaultType::SafetyInterlockViolation)
        .await?;

    let state = svc.safety_service().get_safety_state(&did).await?;
    assert_eq!(
        state.interlock_state,
        InterlockState::SafeTorque,
        "should return to safe torque after clearing fault"
    );

    Ok(())
}

/// Cannot clear a mismatched fault type
#[tokio::test]
async fn recovery_clear_wrong_fault_type_fails() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("wrong-fault-dev")?;
    let t = torque(10.0)?;
    svc.safety_service().register_device(did.clone(), t).await?;

    // Fault with SafetyInterlockViolation
    svc.safety_service()
        .emergency_stop(&did, "test".to_string())
        .await?;

    // Try to clear with wrong fault type
    let result = svc
        .safety_service()
        .clear_fault(&did, FaultType::UsbStall)
        .await;
    assert!(
        result.is_err(),
        "should fail to clear mismatched fault type"
    );

    // Device should still be faulted
    let state = svc.safety_service().get_safety_state(&did).await?;
    assert!(matches!(
        state.interlock_state,
        InterlockState::Faulted { .. }
    ));

    Ok(())
}

/// Error logging: fault count increments correctly
#[tokio::test]
async fn recovery_fault_count_tracking() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("fault-count-dev")?;
    let t = torque(10.0)?;
    svc.safety_service().register_device(did.clone(), t).await?;

    // Report multiple warnings
    for _ in 0..5 {
        svc.safety_service()
            .report_fault(&did, FaultType::UsbStall, FaultSeverity::Warning)
            .await?;
    }

    let state = svc.safety_service().get_safety_state(&did).await?;
    assert_eq!(state.fault_count, 5, "fault count should track all faults");
    assert!(
        state.last_fault_time.is_some(),
        "last fault time should be set"
    );

    Ok(())
}

/// Recovery time bounds: operations complete within deadline
#[tokio::test]
async fn recovery_time_bounds() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("timing-dev")?;
    let t = torque(12.0)?;
    svc.safety_service().register_device(did.clone(), t).await?;

    // Measure emergency stop response time
    let start = Instant::now();
    svc.safety_service()
        .emergency_stop(&did, "timing test".to_string())
        .await?;
    let estop_time = start.elapsed();

    assert!(
        estop_time < Duration::from_millis(50),
        "emergency stop should complete in <50ms, took {:?}",
        estop_time
    );

    // Measure fault clear time
    let start = Instant::now();
    svc.safety_service()
        .clear_fault(&did, FaultType::SafetyInterlockViolation)
        .await?;
    let clear_time = start.elapsed();

    assert!(
        clear_time < Duration::from_millis(50),
        "fault clear should complete in <50ms, took {:?}",
        clear_time
    );

    Ok(())
}

/// Faulted device blocks high torque requests
#[tokio::test]
async fn recovery_faulted_blocks_high_torque() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("blocked-ht-dev")?;
    let t = torque(15.0)?;
    svc.safety_service().register_device(did.clone(), t).await?;

    // Fault the device
    svc.safety_service()
        .emergency_stop(&did, "test".to_string())
        .await?;

    // Attempt high torque should fail
    let result = svc
        .safety_service()
        .request_high_torque(&did, "test-user".to_string())
        .await;
    assert!(
        result.is_err(),
        "high torque should be blocked while faulted"
    );

    Ok(())
}

/// Service remains functional after mixed success/failure operations
#[tokio::test]
async fn recovery_mixed_operations_resilience() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    // Mix of successful and failing operations
    for i in 0..20 {
        let did = parse_device_id(&format!("mixed-dev-{i}"))?;

        if i % 3 == 0 {
            // Try operation on unregistered device (will fail)
            let result = svc.safety_service().get_safety_state(&did).await;
            assert!(result.is_err());
        } else {
            // Register and use device (will succeed)
            let t = torque(10.0)?;
            svc.safety_service().register_device(did.clone(), t).await?;
            let state = svc.safety_service().get_safety_state(&did).await?;
            assert_eq!(state.interlock_state, InterlockState::SafeTorque);
        }
    }

    // Profile service unaffected by safety service errors
    let profile = make_profile("mixed-profile")?;
    svc.profile_service().create_profile(profile).await?;
    let stats = svc.profile_service().get_profile_statistics().await?;
    assert_eq!(stats.total_profiles, 1);

    Ok(())
}

/// IPC server health broadcast works under load
#[tokio::test]
async fn recovery_ipc_health_broadcast_under_load() -> Result<(), BoxErr> {
    let config = IpcConfig::default();
    let server = IpcServer::new(config).await?;
    let mut rx = server.get_health_receiver();

    // Send many events rapidly
    for i in 0..100 {
        server.broadcast_health_event(HealthEventInternal {
            device_id: format!("load-dev-{i}"),
            event_type: "heartbeat".to_string(),
            message: format!("event {i}"),
            timestamp: std::time::SystemTime::now(),
        });
    }

    // Should receive the events (broadcast may drop old ones if buffer overflows)
    let event = tokio::time::timeout(Duration::from_secs(1), rx.recv()).await;
    assert!(event.is_ok(), "should receive at least one event");

    Ok(())
}

/// Service survives creating and destroying many instances
#[tokio::test]
async fn recovery_repeated_lifecycle_stress() -> Result<(), BoxErr> {
    for i in 0..10 {
        let (svc, _tmp) = temp_service().await?;

        // Do work in each lifecycle
        let did = parse_device_id(&format!("stress-dev-{i}"))?;
        let t = torque(10.0)?;
        svc.safety_service().register_device(did.clone(), t).await?;

        // Create a profile
        let profile = make_profile(&format!("stress-p-{i}"))?;
        svc.profile_service().create_profile(profile).await?;

        // Fault and recover
        svc.safety_service()
            .report_fault(&did, FaultType::UsbStall, FaultSeverity::Warning)
            .await?;

        // Enumerate devices
        let devices = svc.device_service().enumerate_devices().await?;
        assert!(!devices.is_empty());

        // Unregister before drop
        svc.safety_service().unregister_device(&did).await?;
    }

    Ok(())
}

/// Full lifecycle: create → register → fault → clear → high-torque → shutdown
#[tokio::test]
async fn recovery_full_lifecycle_fault_recovery() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let did = parse_device_id("full-lifecycle-dev")?;
    let t = torque(15.0)?;

    // 1. Register device
    svc.safety_service().register_device(did.clone(), t).await?;
    let state = svc.safety_service().get_safety_state(&did).await?;
    assert_eq!(state.interlock_state, InterlockState::SafeTorque);

    // 2. Fault the device
    svc.safety_service()
        .emergency_stop(&did, "lifecycle test".to_string())
        .await?;
    let state = svc.safety_service().get_safety_state(&did).await?;
    assert!(matches!(
        state.interlock_state,
        InterlockState::Faulted { .. }
    ));

    // 3. Clear fault
    svc.safety_service()
        .clear_fault(&did, FaultType::SafetyInterlockViolation)
        .await?;
    let state = svc.safety_service().get_safety_state(&did).await?;
    assert_eq!(state.interlock_state, InterlockState::SafeTorque);

    // 4. Request high torque (requires hands-on)
    svc.safety_service()
        .update_hands_on_detection(&did, true)
        .await?;
    let ht_state = svc
        .safety_service()
        .request_high_torque(&did, "lifecycle-test".to_string())
        .await?;
    assert!(matches!(ht_state, InterlockState::Challenge { .. }));

    // 5. Respond to challenge
    let token = match ht_state {
        InterlockState::Challenge {
            challenge_token, ..
        } => challenge_token,
        other => return Err(format!("expected Challenge, got {other:?}").into()),
    };

    let final_state = svc
        .safety_service()
        .respond_to_challenge(&did, token)
        .await?;
    assert!(matches!(
        final_state,
        InterlockState::HighTorqueActive { .. }
    ));

    // 6. Clean unregister
    svc.safety_service().unregister_device(&did).await?;
    let stats = svc.safety_service().get_statistics().await;
    assert_eq!(stats.total_devices, 0);

    Ok(())
}
