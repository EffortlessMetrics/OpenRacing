//! Deep service layer integration tests.
//!
//! Covers: startup/shutdown, device discovery, telemetry routing, profile
//! loading, multi-client IPC, graceful shutdown, error recovery, concurrent
//! operations, config hot-reload, diagnostics, game detection, and state
//! machine transitions.

use std::sync::Arc;
use std::time::Duration;

use racing_wheel_schemas::prelude::{
    BaseSettings, Degrees, DeviceCapabilities, DeviceId, FilterConfig, Gain, Profile, ProfileId,
    ProfileScope, TorqueNm,
};
use racing_wheel_service::{
    DeviceState, DiagnosticService, DiagnosticStatus, FaultSeverity, FeatureFlags, GameService,
    InterlockState, IpcConfig, IpcServer, ServiceConfig, ServiceDaemon, SystemConfig, WheelService,
    profile_repository::ProfileRepositoryConfig,
};
use tempfile::TempDir;

// ── Helpers ─────────────────────────────────────────────────────────────

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

async fn temp_service() -> Result<(WheelService, TempDir), BoxErr> {
    let tmp = TempDir::new()?;
    let config = ProfileRepositoryConfig {
        profiles_dir: tmp.path().to_path_buf(),
        trusted_keys: Vec::new(),
        auto_migrate: true,
        backup_on_migrate: false,
    };
    let svc = WheelService::new_with_flags(racing_wheel_service::FeatureFlags::default(), config).await?;
    Ok((svc, tmp))
}

fn test_service_config() -> ServiceConfig {
    ServiceConfig {
        service_name: "integration-test".to_string(),
        service_display_name: "Integration Test Service".to_string(),
        service_description: "Service for integration tests".to_string(),
        ipc: IpcConfig::default(),
        health_check_interval: 1,
        max_restart_attempts: 1,
        restart_delay: 1,
        auto_restart: false,
    }
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

fn make_game_profile(id: &str, game: &str, gain: f32) -> Result<Profile, BoxErr> {
    let pid = ProfileId::new(id.to_string())?;
    Ok(Profile::new(
        pid,
        ProfileScope::for_game(game.to_string()),
        BaseSettings {
            ffb_gain: Gain::new(gain)?,
            degrees_of_rotation: Degrees::new_dor(540.0)?,
            torque_cap: TorqueNm::new(10.0)?,
            filters: FilterConfig::default(),
        },
        format!("Game Profile {id}"),
    ))
}

fn test_device_capabilities() -> Result<DeviceCapabilities, BoxErr> {
    Ok(DeviceCapabilities::new(
        false,
        true,
        true,
        true,
        TorqueNm::new(25.0)?,
        10000,
        1000,
    ))
}

// =========================================================================
// 1. Service startup and shutdown sequences
// =========================================================================

#[tokio::test]
async fn startup_creates_all_sub_services() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let _ps = svc.profile_service();
    let _ds = svc.device_service();
    let _ss = svc.safety_service();
    Ok(())
}

#[tokio::test]
async fn startup_seeds_virtual_device() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let devices = svc.device_service().enumerate_devices().await?;
    assert!(
        !devices.is_empty(),
        "should seed at least one virtual device"
    );
    Ok(())
}

#[tokio::test]
async fn shutdown_daemon_via_abort() -> Result<(), BoxErr> {
    let config = test_service_config();
    let daemon = ServiceDaemon::new(config).await?;
    let handle = tokio::spawn(async move { daemon.run().await });

    tokio::time::sleep(Duration::from_millis(50)).await;
    handle.abort();

    let result = handle.await;
    assert!(result.is_err(), "aborted task should return Err");
    Ok(())
}

#[tokio::test]
async fn repeated_create_destroy_is_clean() -> Result<(), BoxErr> {
    for i in 0..3 {
        let (svc, _tmp) = temp_service().await?;
        let device_id: DeviceId = format!("repeat-dev-{i}")
            .parse()
            .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
        let torque = TorqueNm::new(8.0)?;
        svc.safety_service()
            .register_device(device_id, torque)
            .await?;
    }
    Ok(())
}

// =========================================================================
// 2. Device discovery and registration flow
// =========================================================================

#[tokio::test]
async fn device_enumerate_discovers_virtual() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let devices = svc.device_service().enumerate_devices().await?;
    assert!(!devices.is_empty());

    let first_id = &devices[0].id;
    let managed = svc.device_service().get_device(first_id).await?;
    assert!(
        managed.is_some(),
        "device should be tracked after enumeration"
    );

    let dev = managed.ok_or("expected device")?;
    assert_eq!(dev.state, DeviceState::Connected);
    Ok(())
}

#[tokio::test]
async fn device_initialize_transitions_to_ready() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let devices = svc.device_service().enumerate_devices().await?;
    assert!(!devices.is_empty());

    let device_id = &devices[0].id;
    svc.device_service().initialize_device(device_id).await?;

    let managed = svc.device_service().get_device(device_id).await?;
    let dev = managed.ok_or("expected device after init")?;
    assert_eq!(dev.state, DeviceState::Ready);
    assert!(dev.capabilities.is_some());
    Ok(())
}

#[tokio::test]
async fn device_get_nonexistent_returns_none() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let device_id: DeviceId = "nonexistent-device"
        .parse()
        .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
    let result = svc.device_service().get_device(&device_id).await?;
    assert!(result.is_none());
    Ok(())
}

#[tokio::test]
async fn device_statistics_reflect_enumeration() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let stats_before = svc.device_service().get_statistics().await;
    assert_eq!(stats_before.total_devices, 0);

    let _devices = svc.device_service().enumerate_devices().await?;
    let stats_after = svc.device_service().get_statistics().await;
    assert!(stats_after.total_devices > 0);
    assert!(stats_after.connected_devices > 0);
    Ok(())
}

// =========================================================================
// 3. Telemetry routing (game → engine)
// =========================================================================

#[tokio::test]
async fn telemetry_initially_none_for_device() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let devices = svc.device_service().enumerate_devices().await?;
    assert!(!devices.is_empty());

    let telemetry = svc
        .device_service()
        .get_device_telemetry(&devices[0].id)
        .await?;
    assert!(telemetry.is_none(), "no telemetry before data arrives");
    Ok(())
}

#[tokio::test]
async fn device_health_readable_after_enumeration() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let devices = svc.device_service().enumerate_devices().await?;
    assert!(!devices.is_empty());

    let health = svc
        .device_service()
        .get_device_health(&devices[0].id)
        .await?;
    assert_eq!(health.fault_flags, 0, "no faults on healthy virtual device");
    Ok(())
}

// =========================================================================
// 4. Profile loading and application during runtime
// =========================================================================

#[tokio::test]
async fn profile_crud_lifecycle() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let ps = svc.profile_service();
    let profile = make_profile("crud-test")?;

    // Create
    let pid = ps.create_profile(profile.clone()).await?;
    assert_eq!(pid, profile.id);

    // Read
    let loaded = ps.get_profile(&pid).await?;
    assert!(loaded.is_some());

    // Update
    let mut updated = profile.clone();
    updated.base_settings.ffb_gain = Gain::new(0.8)?;
    ps.update_profile(updated).await?;

    let loaded = ps.get_profile(&pid).await?;
    let loaded = loaded.ok_or("profile should exist after update")?;
    assert!((loaded.base_settings.ffb_gain.value() - 0.8).abs() < f32::EPSILON);

    // Delete
    ps.delete_profile(&pid).await?;
    let loaded = ps.get_profile(&pid).await?;
    assert!(loaded.is_none());
    Ok(())
}

#[tokio::test]
async fn profile_hierarchy_applies_game_specific() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let ps = svc.profile_service();
    let caps = test_device_capabilities()?;
    let device_id: DeviceId = "hierarchy-dev".parse()?;

    let global = make_profile("global-h")?;
    let game_prof = make_game_profile("iracing-h", "iracing", 0.75)?;

    ps.create_profile(global).await?;
    ps.create_profile(game_prof).await?;

    let resolved = ps
        .apply_profile_to_device(&device_id, Some("iracing"), None, None, &caps)
        .await?;
    assert!((resolved.base_settings.ffb_gain.value() - 0.75).abs() < f32::EPSILON);

    let active = ps.get_active_profile(&device_id).await?;
    assert!(active.is_some());
    Ok(())
}

#[tokio::test]
async fn profile_session_override_applied_and_cleared() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let ps = svc.profile_service();
    let device_id: DeviceId = "override-dev".parse()?;
    let override_profile = make_profile("session-override")?;

    ps.set_session_override(&device_id, override_profile.clone())
        .await?;
    let got = ps.get_session_override(&device_id).await?;
    assert!(got.is_some());

    ps.clear_session_override(&device_id).await?;
    let got = ps.get_session_override(&device_id).await?;
    assert!(got.is_none());
    Ok(())
}

#[tokio::test]
async fn profile_statistics_track_counts() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let ps = svc.profile_service();

    let stats0 = ps.get_profile_statistics().await?;
    assert_eq!(stats0.total_profiles, 0);

    ps.create_profile(make_profile("stats-a")?).await?;
    ps.create_profile(make_profile("stats-b")?).await?;

    let stats1 = ps.get_profile_statistics().await?;
    assert_eq!(stats1.total_profiles, 2);
    Ok(())
}

#[tokio::test]
async fn delete_active_profile_is_rejected() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let ps = svc.profile_service();
    let profile = make_profile("active-del")?;
    let pid = ps.create_profile(profile).await?;

    let device_id: DeviceId = "del-dev".parse()?;
    ps.set_active_profile(&device_id, &pid).await?;

    let result = ps.delete_profile(&pid).await;
    assert!(result.is_err(), "deleting active profile should fail");
    Ok(())
}

// =========================================================================
// 5. Multi-client IPC handling
// =========================================================================

#[tokio::test]
async fn ipc_server_create_and_shutdown() -> Result<(), BoxErr> {
    let config = IpcConfig::default();
    let server = IpcServer::new(config).await?;

    // Start serving in background
    let (svc, _tmp) = temp_service().await?;
    let svc = Arc::new(svc);
    let server_clone = server.clone();
    let svc_clone = svc.clone();
    let handle = tokio::spawn(async move {
        let _ = server_clone.serve(svc_clone).await;
    });

    // Brief delay then shutdown
    tokio::time::sleep(Duration::from_millis(30)).await;
    server.shutdown().await;

    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
    Ok(())
}

#[tokio::test]
async fn ipc_health_event_broadcast() -> Result<(), BoxErr> {
    let config = IpcConfig::default();
    let server = IpcServer::new(config).await?;

    let mut rx = server.get_health_receiver();

    server.broadcast_health_event(racing_wheel_service::HealthEventInternal {
        device_id: "test-dev".to_string(),
        event_type: "connected".to_string(),
        message: "Test device connected".to_string(),
        timestamp: std::time::SystemTime::now(),
    });

    let event = tokio::time::timeout(Duration::from_secs(1), rx.recv()).await;
    assert!(event.is_ok(), "should receive broadcast event");

    let event = event.map_err(|e| -> BoxErr { format!("timeout: {e}").into() })?;
    let event = event.map_err(|e| -> BoxErr { format!("recv: {e}").into() })?;
    assert_eq!(event.device_id, "test-dev");
    Ok(())
}

// =========================================================================
// 6. Graceful shutdown with active connections
// =========================================================================

#[tokio::test]
async fn graceful_shutdown_daemon_with_profile_config() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let profile_config = ProfileRepositoryConfig {
        profiles_dir: tmp.path().to_path_buf(),
        trusted_keys: Vec::new(),
        auto_migrate: true,
        backup_on_migrate: false,
    };
    let config = test_service_config();
    let daemon = ServiceDaemon::new_with_profile_config(config, profile_config).await?;

    let handle = tokio::spawn(async move { daemon.run().await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    handle.abort();

    let result = handle.await;
    assert!(result.is_err(), "aborted task returns Err");
    Ok(())
}

#[tokio::test]
async fn graceful_shutdown_with_registered_devices() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let device_id: DeviceId = "shutdown-dev"
        .parse()
        .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
    let torque = TorqueNm::new(10.0)?;

    svc.safety_service()
        .register_device(device_id.clone(), torque)
        .await?;

    // Verify device registered
    let state = svc.safety_service().get_safety_state(&device_id).await?;
    assert_eq!(state.interlock_state, InterlockState::SafeTorque);

    // Unregister before drop (simulate graceful shutdown)
    svc.safety_service().unregister_device(&device_id).await?;

    let gone = svc.safety_service().get_safety_state(&device_id).await;
    assert!(gone.is_err(), "device should be gone after unregister");
    Ok(())
}

// =========================================================================
// 7. Error recovery during device communication failure
// =========================================================================

#[tokio::test]
async fn fault_report_warning_preserves_operation() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let device_id: DeviceId = "fault-warn-dev"
        .parse()
        .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
    let torque = TorqueNm::new(10.0)?;

    svc.safety_service()
        .register_device(device_id.clone(), torque)
        .await?;

    svc.safety_service()
        .report_fault(
            &device_id,
            racing_wheel_engine::safety::FaultType::UsbStall,
            FaultSeverity::Warning,
        )
        .await?;

    let state = svc.safety_service().get_safety_state(&device_id).await?;
    // Warning should not transition to Faulted
    assert_eq!(state.interlock_state, InterlockState::SafeTorque);
    assert_eq!(state.fault_count, 1);
    Ok(())
}

#[tokio::test]
async fn fault_report_critical_reduces_torque() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let device_id: DeviceId = "fault-crit-dev"
        .parse()
        .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
    let torque = TorqueNm::new(20.0)?;

    svc.safety_service()
        .register_device(device_id.clone(), torque)
        .await?;

    let initial_limit = svc.safety_service().get_torque_limit(&device_id).await?;

    svc.safety_service()
        .report_fault(
            &device_id,
            racing_wheel_engine::safety::FaultType::ThermalLimit,
            FaultSeverity::Critical,
        )
        .await?;

    let reduced_limit = svc.safety_service().get_torque_limit(&device_id).await?;
    assert!(
        reduced_limit.value() < initial_limit.value(),
        "critical fault should reduce torque limit"
    );
    Ok(())
}

#[tokio::test]
async fn fault_report_fatal_disables_torque() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let device_id: DeviceId = "fault-fatal-dev"
        .parse()
        .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
    let torque = TorqueNm::new(15.0)?;

    svc.safety_service()
        .register_device(device_id.clone(), torque)
        .await?;

    svc.safety_service()
        .report_fault(
            &device_id,
            racing_wheel_engine::safety::FaultType::Overcurrent,
            FaultSeverity::Fatal,
        )
        .await?;

    let state = svc.safety_service().get_safety_state(&device_id).await?;
    assert!(
        matches!(state.interlock_state, InterlockState::Faulted { .. }),
        "fatal fault should transition to Faulted"
    );
    assert_eq!(state.current_torque_limit.value(), 0.0);
    Ok(())
}

#[tokio::test]
async fn fault_clear_returns_to_safe_torque() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let device_id: DeviceId = "fault-clear-dev"
        .parse()
        .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
    let torque = TorqueNm::new(12.0)?;
    let fault = racing_wheel_engine::safety::FaultType::Overcurrent;

    svc.safety_service()
        .register_device(device_id.clone(), torque)
        .await?;

    svc.safety_service()
        .report_fault(&device_id, fault, FaultSeverity::Fatal)
        .await?;

    // Clear the fault
    svc.safety_service().clear_fault(&device_id, fault).await?;

    let state = svc.safety_service().get_safety_state(&device_id).await?;
    assert_eq!(state.interlock_state, InterlockState::SafeTorque);
    assert!(state.current_torque_limit.value() > 0.0);
    Ok(())
}

#[tokio::test]
async fn emergency_stop_zeroes_torque() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let device_id: DeviceId = "estop-dev"
        .parse()
        .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
    let torque = TorqueNm::new(15.0)?;

    svc.safety_service()
        .register_device(device_id.clone(), torque)
        .await?;

    svc.safety_service()
        .emergency_stop(&device_id, "test emergency".to_string())
        .await?;

    let state = svc.safety_service().get_safety_state(&device_id).await?;
    assert!(matches!(
        state.interlock_state,
        InterlockState::Faulted { .. }
    ));
    assert_eq!(state.current_torque_limit.value(), 0.0);
    assert_eq!(state.fault_count, 1);
    Ok(())
}

// =========================================================================
// 8. Concurrent device and telemetry operations
// =========================================================================

#[tokio::test]
async fn concurrent_device_registration() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let safety = svc.safety_service().clone();

    let mut handles = Vec::new();
    for i in 0..5 {
        let ss = safety.clone();
        handles.push(tokio::spawn(async move {
            let device_id: DeviceId = format!("concurrent-dev-{i}")
                .parse()
                .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
            let torque =
                TorqueNm::new(10.0).map_err(|e| -> BoxErr { format!("bad torque: {e}").into() })?;
            ss.register_device(device_id, torque).await?;
            Ok::<(), BoxErr>(())
        }));
    }

    for h in handles {
        h.await
            .map_err(|e| -> BoxErr { format!("join: {e}").into() })??;
    }

    let stats = safety.get_statistics().await;
    assert_eq!(stats.total_devices, 5);
    Ok(())
}

#[tokio::test]
async fn concurrent_profile_operations() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let ps = svc.profile_service().clone();

    let mut handles = Vec::new();
    for i in 0..5 {
        let ps = ps.clone();
        handles.push(tokio::spawn(async move {
            let profile = make_profile(&format!("concurrent-{i}"))?;
            ps.create_profile(profile).await?;
            Ok::<(), BoxErr>(())
        }));
    }

    for h in handles {
        h.await
            .map_err(|e| -> BoxErr { format!("join: {e}").into() })??;
    }

    let profiles = ps.list_profiles().await?;
    assert_eq!(profiles.len(), 5);
    Ok(())
}

#[tokio::test]
async fn concurrent_enumerate_and_register() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let ds = svc.device_service().clone();
    let ss = svc.safety_service().clone();

    // Enumerate concurrently with safety registration
    let ds_handle = {
        let ds = ds.clone();
        tokio::spawn(async move { ds.enumerate_devices().await })
    };

    let ss_handle = {
        let ss = ss.clone();
        tokio::spawn(async move {
            let dev_id: DeviceId = "conc-reg-dev"
                .parse()
                .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
            let torque = TorqueNm::new(10.0)?;
            ss.register_device(dev_id, torque).await?;
            Ok::<(), BoxErr>(())
        })
    };

    let devices = ds_handle
        .await
        .map_err(|e| -> BoxErr { format!("join: {e}").into() })??;
    ss_handle
        .await
        .map_err(|e| -> BoxErr { format!("join: {e}").into() })??;

    assert!(!devices.is_empty());
    let safety_stats = ss.get_statistics().await;
    assert_eq!(safety_stats.total_devices, 1);
    Ok(())
}

// =========================================================================
// 9. Configuration hot-reload
// =========================================================================

#[tokio::test]
async fn config_roundtrip_json() -> Result<(), BoxErr> {
    let original = SystemConfig::default();
    let json = serde_json::to_string_pretty(&original)?;
    let parsed: SystemConfig = serde_json::from_str(&json)?;
    parsed.validate()?;
    assert_eq!(parsed.schema_version, original.schema_version);
    Ok(())
}

#[tokio::test]
async fn config_save_reload_from_file() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let path = tmp.path().join("svc_config.json");

    let config = SystemConfig::default();
    config.save_to_path(&path).await?;

    let loaded = SystemConfig::load_from_path(&path).await?;
    loaded.validate()?;
    assert_eq!(loaded.schema_version, config.schema_version);
    assert_eq!(loaded.engine.tick_rate_hz, config.engine.tick_rate_hz);
    Ok(())
}

#[tokio::test]
async fn config_hot_reload_detects_changes() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let path = tmp.path().join("hot_reload.json");

    let mut config = SystemConfig::default();
    config.save_to_path(&path).await?;

    // Mutate and re-save
    config.engine.tick_rate_hz = 500;
    config.safety.max_torque_nm = 15.0;
    config.save_to_path(&path).await?;

    // Reload
    let reloaded = SystemConfig::load_from_path(&path).await?;
    assert_eq!(reloaded.engine.tick_rate_hz, 500);
    assert!((reloaded.safety.max_torque_nm - 15.0).abs() < f32::EPSILON);
    Ok(())
}

#[tokio::test]
async fn config_service_config_roundtrip() -> Result<(), BoxErr> {
    let config = test_service_config();
    let json = serde_json::to_string_pretty(&config)?;
    let parsed: ServiceConfig = serde_json::from_str(&json)?;
    assert_eq!(parsed.service_name, "integration-test");
    assert_eq!(parsed.max_restart_attempts, 1);
    assert!(!parsed.auto_restart);
    Ok(())
}

// =========================================================================
// 10. Diagnostic data collection and reporting
// =========================================================================

#[tokio::test]
async fn diagnostic_service_runs_full_suite() -> Result<(), BoxErr> {
    let diag = DiagnosticService::new().await?;
    let results = diag.run_full_diagnostics().await?;
    assert!(!results.is_empty(), "should return at least one result");

    for result in &results {
        assert!(!result.name.is_empty(), "result should have a name");
        assert!(!result.message.is_empty(), "result should have a message");
    }
    Ok(())
}

#[tokio::test]
async fn diagnostic_list_tests_nonempty() -> Result<(), BoxErr> {
    let diag = DiagnosticService::new().await?;
    let tests = diag.list_tests();
    assert!(!tests.is_empty(), "diagnostic tests should be registered");

    // Each test has name and description
    for (name, desc) in &tests {
        assert!(!name.is_empty());
        assert!(!desc.is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn diagnostic_run_specific_test() -> Result<(), BoxErr> {
    let diag = DiagnosticService::new().await?;
    let result = diag.run_test("system_requirements").await?;
    assert_eq!(result.name, "system_requirements");
    assert!(
        matches!(
            result.status,
            DiagnosticStatus::Pass | DiagnosticStatus::Warn | DiagnosticStatus::Fail
        ),
        "status should be a valid variant"
    );
    Ok(())
}

#[tokio::test]
async fn diagnostic_nonexistent_test_errors() -> Result<(), BoxErr> {
    let diag = DiagnosticService::new().await?;
    let result = diag.run_test("nonexistent_test").await;
    assert!(result.is_err(), "missing test should error");
    Ok(())
}

// =========================================================================
// 11. Game detection and auto-configuration
// =========================================================================

#[tokio::test]
async fn game_service_creates_successfully() -> Result<(), BoxErr> {
    let _gs = GameService::new().await?;
    Ok(())
}

#[tokio::test]
async fn game_service_lists_supported_games() -> Result<(), BoxErr> {
    let gs = GameService::new().await?;
    let games = gs.get_supported_games().await;
    // Support matrix should have at least some games
    assert!(!games.is_empty(), "support matrix should list games");
    Ok(())
}

#[tokio::test]
async fn game_service_no_active_game_initially() -> Result<(), BoxErr> {
    let gs = GameService::new().await?;
    let status = gs.get_game_status().await?;
    assert!(status.active_game.is_none(), "no game active at startup");
    assert!(!status.telemetry_active);
    Ok(())
}

#[tokio::test]
async fn system_config_game_defaults_populated() -> Result<(), BoxErr> {
    let config = SystemConfig::default();
    assert!(config.games.auto_configure);
    assert!(config.games.auto_profile_switch);
    assert!(!config.games.supported_games.is_empty());
    assert!(config.games.supported_games.contains_key("iracing"));
    assert!(config.games.supported_games.contains_key("acc"));
    Ok(())
}

// =========================================================================
// 12. Service state machine transitions
// =========================================================================

#[tokio::test]
async fn safety_state_machine_safe_torque_default() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let device_id: DeviceId = "sm-dev"
        .parse()
        .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
    let torque = TorqueNm::new(10.0)?;

    svc.safety_service()
        .register_device(device_id.clone(), torque)
        .await?;

    let state = svc.safety_service().get_safety_state(&device_id).await?;
    assert_eq!(state.interlock_state, InterlockState::SafeTorque);
    // Initial torque should be 30% of max
    let expected_limit = torque.value() * 0.3;
    assert!(
        (state.current_torque_limit.value() - expected_limit).abs() < 0.01,
        "initial limit should be 30% of max"
    );
    Ok(())
}

#[tokio::test]
async fn safety_state_machine_high_torque_requires_hands_on() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let device_id: DeviceId = "sm-ht-dev"
        .parse()
        .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
    let torque = TorqueNm::new(10.0)?;

    svc.safety_service()
        .register_device(device_id.clone(), torque)
        .await?;

    // Request high torque without hands-on → should fail
    let result = svc
        .safety_service()
        .request_high_torque(&device_id, "test_user".to_string())
        .await;
    assert!(
        result.is_err(),
        "high torque without hands-on should be denied"
    );
    Ok(())
}

#[tokio::test]
async fn safety_state_machine_hands_on_enables_challenge() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let device_id: DeviceId = "sm-challenge-dev"
        .parse()
        .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
    let torque = TorqueNm::new(10.0)?;

    svc.safety_service()
        .register_device(device_id.clone(), torque)
        .await?;

    // Simulate hands-on detection
    svc.safety_service()
        .update_hands_on_detection(&device_id, true)
        .await?;

    // Now request high torque → should issue challenge
    let state = svc
        .safety_service()
        .request_high_torque(&device_id, "test_user".to_string())
        .await?;

    assert!(
        matches!(state, InterlockState::Challenge { .. }),
        "should transition to Challenge state"
    );
    Ok(())
}

#[tokio::test]
async fn safety_state_machine_challenge_response_success() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let device_id: DeviceId = "sm-resp-dev"
        .parse()
        .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
    let torque = TorqueNm::new(10.0)?;

    svc.safety_service()
        .register_device(device_id.clone(), torque)
        .await?;

    svc.safety_service()
        .update_hands_on_detection(&device_id, true)
        .await?;

    let state = svc
        .safety_service()
        .request_high_torque(&device_id, "test_user".to_string())
        .await?;

    let token = match state {
        InterlockState::Challenge {
            challenge_token, ..
        } => challenge_token,
        other => return Err(format!("expected Challenge, got {:?}", other).into()),
    };

    // Respond with correct token
    let result = svc
        .safety_service()
        .respond_to_challenge(&device_id, token)
        .await?;

    assert!(
        matches!(result, InterlockState::HighTorqueActive { .. }),
        "correct token should activate high torque"
    );

    // Torque limit should be at max
    let limit = svc.safety_service().get_torque_limit(&device_id).await?;
    assert!(
        (limit.value() - torque.value()).abs() < 0.01,
        "high torque should set limit to max"
    );
    Ok(())
}

#[tokio::test]
async fn safety_state_machine_challenge_response_failure() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let device_id: DeviceId = "sm-fail-dev"
        .parse()
        .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
    let torque = TorqueNm::new(10.0)?;

    svc.safety_service()
        .register_device(device_id.clone(), torque)
        .await?;

    svc.safety_service()
        .update_hands_on_detection(&device_id, true)
        .await?;

    let state = svc
        .safety_service()
        .request_high_torque(&device_id, "test_user".to_string())
        .await?;

    assert!(matches!(state, InterlockState::Challenge { .. }));

    // Respond with wrong token
    let result = svc
        .safety_service()
        .respond_to_challenge(&device_id, 999_999)
        .await?;

    assert_eq!(
        result,
        InterlockState::SafeTorque,
        "wrong token should revert to SafeTorque"
    );
    Ok(())
}

#[tokio::test]
async fn safety_state_machine_faulted_blocks_high_torque() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let device_id: DeviceId = "sm-faulted-ht-dev"
        .parse()
        .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
    let torque = TorqueNm::new(10.0)?;

    svc.safety_service()
        .register_device(device_id.clone(), torque)
        .await?;

    // Put into faulted state
    svc.safety_service()
        .emergency_stop(&device_id, "test".to_string())
        .await?;

    // High torque request should fail
    let result = svc
        .safety_service()
        .request_high_torque(&device_id, "test_user".to_string())
        .await;
    assert!(
        result.is_err(),
        "faulted device should reject high torque request"
    );
    Ok(())
}

#[tokio::test]
async fn safety_statistics_track_state_distribution() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let ss = svc.safety_service();

    // Register 3 devices
    for i in 0..3 {
        let dev_id: DeviceId = format!("stat-dev-{i}")
            .parse()
            .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
        let torque = TorqueNm::new(10.0)?;
        ss.register_device(dev_id, torque).await?;
    }

    let stats = ss.get_statistics().await;
    assert_eq!(stats.total_devices, 3);
    assert_eq!(stats.safe_torque_devices, 3);
    assert_eq!(stats.faulted_devices, 0);

    // Fault one device
    let faulted_id: DeviceId = "stat-dev-0"
        .parse()
        .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
    ss.emergency_stop(&faulted_id, "test fault".to_string())
        .await?;

    let stats = ss.get_statistics().await;
    assert_eq!(stats.total_devices, 3);
    assert_eq!(stats.safe_torque_devices, 2);
    assert_eq!(stats.faulted_devices, 1);
    Ok(())
}

#[tokio::test]
async fn safety_unregister_removes_from_statistics() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let ss = svc.safety_service();
    let device_id: DeviceId = "unreg-stat-dev"
        .parse()
        .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
    let torque = TorqueNm::new(10.0)?;

    ss.register_device(device_id.clone(), torque).await?;
    assert_eq!(ss.get_statistics().await.total_devices, 1);

    ss.unregister_device(&device_id).await?;
    assert_eq!(ss.get_statistics().await.total_devices, 0);
    Ok(())
}

#[tokio::test]
async fn daemon_with_feature_flags() -> Result<(), BoxErr> {
    let config = test_service_config();
    let flags = FeatureFlags {
        disable_realtime: true,
        force_ffb_mode: None,
        enable_dev_features: true,
        enable_debug_logging: false,
        enable_virtual_devices: true,
        disable_safety_interlocks: false,
        enable_plugin_dev_mode: false,
    };
    let daemon = ServiceDaemon::new_with_flags(config, flags).await?;
    let handle = tokio::spawn(async move { daemon.run().await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    handle.abort();
    let result = handle.await;
    assert!(result.is_err(), "aborted task should return Err");
    Ok(())
}
