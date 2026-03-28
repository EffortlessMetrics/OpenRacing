//! Service lifecycle tests covering start → configure → run → shutdown,
//! configuration hot-reload, device discovery callbacks, and health checks.

use std::time::Duration;

use racing_wheel_service::profile_repository::ProfileRepositoryConfig;
use racing_wheel_service::{IpcConfig, ServiceConfig, ServiceDaemon, SystemConfig, WheelService};
use tempfile::TempDir;

// ── Helpers ──────────────────────────────────────────────────────────────

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

/// Unwrap a Result without `unwrap()` / `expect()`.
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
    let svc =
        WheelService::new_with_flags(racing_wheel_service::FeatureFlags::default(), config).await?;
    Ok((svc, tmp))
}

fn test_service_config() -> ServiceConfig {
    ServiceConfig {
        service_name: "lifecycle-test".to_string(),
        service_display_name: "Lifecycle Test Service".to_string(),
        service_description: "Service for lifecycle tests".to_string(),
        ipc: IpcConfig::default(),
        health_check_interval: 1,
        max_restart_attempts: 1,
        restart_delay: 1,
        auto_restart: false,
    }
}

// =========================================================================
// 1. Full service lifecycle: start → configure → run → shutdown
// =========================================================================

#[tokio::test]
async fn lifecycle_create_and_check_sub_services() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    // All sub-services should be accessible immediately after creation
    let _profile = svc.profile_service();
    let _device = svc.device_service();
    let _safety = svc.safety_service();
    Ok(())
}

#[tokio::test]
async fn lifecycle_device_then_safety_registration() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    let device_id: racing_wheel_schemas::prelude::DeviceId = "lifecycle-dev-0"
        .parse()
        .map_err(|e| -> BoxErr { format!("bad device id: {e}").into() })?;

    let torque = racing_wheel_schemas::prelude::TorqueNm::new(12.0)
        .map_err(|e| -> BoxErr { format!("bad torque: {e}").into() })?;

    // Register device with safety service
    svc.safety_service()
        .register_device(device_id.clone(), torque)
        .await?;

    // Verify registration by querying safety state
    let state = svc.safety_service().get_safety_state(&device_id).await?;
    assert_eq!(
        state.interlock_state,
        racing_wheel_service::InterlockState::SafeTorque
    );

    // Unregister and verify it's gone
    svc.safety_service().unregister_device(&device_id).await?;
    let gone = svc.safety_service().get_safety_state(&device_id).await;
    assert!(gone.is_err(), "device should be unregistered");

    Ok(())
}

#[tokio::test]
async fn lifecycle_daemon_create_and_abort() -> Result<(), BoxErr> {
    let config = test_service_config();
    let daemon = ServiceDaemon::new(config).await?;

    // Launch the daemon and abort it to simulate shutdown
    let handle = tokio::spawn(async move { daemon.run().await });

    // Give it a brief moment to start, then cancel
    tokio::time::sleep(Duration::from_millis(50)).await;
    handle.abort();

    let result = handle.await;
    // Aborted task returns a JoinError with is_cancelled() == true
    assert!(result.is_err(), "aborted task should return Err");
    Ok(())
}

#[tokio::test]
async fn lifecycle_repeated_create_destroy() -> Result<(), BoxErr> {
    // Ensure service can be created and dropped cleanly multiple times
    for i in 0..3 {
        let (svc, _tmp) = temp_service().await?;
        let device_id: racing_wheel_schemas::prelude::DeviceId = format!("repeat-dev-{i}")
            .parse()
            .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
        let torque = racing_wheel_schemas::prelude::TorqueNm::new(8.0)
            .map_err(|e| -> BoxErr { format!("bad torque: {e}").into() })?;
        svc.safety_service()
            .register_device(device_id, torque)
            .await?;
        // svc drops here
    }
    Ok(())
}

// =========================================================================
// 2. Configuration hot-reload
// =========================================================================

#[tokio::test]
async fn config_json_roundtrip() -> Result<(), BoxErr> {
    let original = SystemConfig::default();
    let json = serde_json::to_string_pretty(&original)?;
    let parsed: SystemConfig = serde_json::from_str(&json)?;

    // Validate that round-tripped config also validates
    parsed.validate()?;
    assert_eq!(parsed.schema_version, original.schema_version);
    Ok(())
}

#[tokio::test]
async fn config_save_and_reload() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let path = tmp.path().join("test_config.json");

    let config = test_service_config();
    let json = serde_json::to_string_pretty(&config)?;
    tokio::fs::write(&path, &json).await?;

    let content = tokio::fs::read_to_string(&path).await?;
    let loaded: ServiceConfig = serde_json::from_str(&content)?;

    assert_eq!(loaded.service_name, "lifecycle-test");
    assert_eq!(loaded.max_restart_attempts, 1);
    Ok(())
}

#[tokio::test]
async fn config_mutate_and_reload() -> Result<(), BoxErr> {
    let tmp = TempDir::new()?;
    let path = tmp.path().join("hot_reload.json");

    // Write initial config
    let mut config = test_service_config();
    let json = serde_json::to_string_pretty(&config)?;
    tokio::fs::write(&path, &json).await?;

    // Mutate and re-write (simulating hot-reload)
    config.health_check_interval = 99;
    config.auto_restart = true;
    let updated_json = serde_json::to_string_pretty(&config)?;
    tokio::fs::write(&path, &updated_json).await?;

    // Reload and verify
    let content = tokio::fs::read_to_string(&path).await?;
    let reloaded: ServiceConfig = serde_json::from_str(&content)?;
    assert_eq!(reloaded.health_check_interval, 99);
    assert!(reloaded.auto_restart);
    Ok(())
}

// =========================================================================
// 3. Device discovery callbacks
// =========================================================================

#[tokio::test]
async fn device_enumeration_returns_virtual() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let devices = svc.device_service().enumerate_devices().await?;
    // The default service seeds one virtual device
    assert!(
        !devices.is_empty(),
        "should have at least one virtual device"
    );
    Ok(())
}

#[tokio::test]
async fn device_get_nonexistent_returns_none() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let device_id: racing_wheel_schemas::prelude::DeviceId = "does-not-exist"
        .parse()
        .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
    let result = svc.device_service().get_device(&device_id).await?;
    assert!(result.is_none(), "nonexistent device should be None");
    Ok(())
}

#[tokio::test]
async fn device_statistics_start_at_zero() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let stats = svc.device_service().get_statistics().await;
    // Faulted devices should start at zero
    assert_eq!(stats.faulted_devices, 0);
    Ok(())
}

// =========================================================================
// 4. Service health checks
// =========================================================================

#[tokio::test]
async fn health_profile_stats_initially_empty() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;
    let stats = svc.profile_service().get_profile_statistics().await?;
    assert_eq!(stats.active_profiles, 0);
    assert_eq!(stats.total_profiles, 0);
    Ok(())
}

#[tokio::test]
async fn health_safety_stats_track_registration() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    let stats_before = svc.safety_service().get_statistics().await;
    assert_eq!(stats_before.total_devices, 0);

    let device_id: racing_wheel_schemas::prelude::DeviceId = "health-dev"
        .parse()
        .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;
    let torque = racing_wheel_schemas::prelude::TorqueNm::new(10.0)
        .map_err(|e| -> BoxErr { format!("bad torque: {e}").into() })?;

    svc.safety_service()
        .register_device(device_id.clone(), torque)
        .await?;

    let stats_after = svc.safety_service().get_statistics().await;
    assert_eq!(stats_after.total_devices, 1);
    assert_eq!(stats_after.safe_torque_devices, 1);

    svc.safety_service().unregister_device(&device_id).await?;

    let stats_final = svc.safety_service().get_statistics().await;
    assert_eq!(stats_final.total_devices, 0);
    Ok(())
}

#[tokio::test]
async fn health_operations_complete_within_timeout() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    let result = tokio::time::timeout(
        Duration::from_secs(5),
        svc.device_service().enumerate_devices(),
    )
    .await;

    assert!(result.is_ok(), "operation should not time out");
    let devices = must(result)?;
    // Just ensure it returned without error
    let _ = devices;
    Ok(())
}

#[tokio::test]
async fn health_service_resilient_after_error() -> Result<(), BoxErr> {
    let (svc, _tmp) = temp_service().await?;

    let missing: racing_wheel_schemas::prelude::DeviceId = "ghost-device"
        .parse()
        .map_err(|e| -> BoxErr { format!("bad id: {e}").into() })?;

    // Cause an error — getting state for unregistered device
    let err = svc.safety_service().get_safety_state(&missing).await;
    assert!(err.is_err());

    // Service should still work after the error
    let torque = racing_wheel_schemas::prelude::TorqueNm::new(10.0)
        .map_err(|e| -> BoxErr { format!("bad torque: {e}").into() })?;
    svc.safety_service()
        .register_device(missing.clone(), torque)
        .await?;

    let state = svc.safety_service().get_safety_state(&missing).await?;
    assert_eq!(
        state.interlock_state,
        racing_wheel_service::InterlockState::SafeTorque
    );
    Ok(())
}
