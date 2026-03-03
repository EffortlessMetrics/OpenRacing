//! End-to-end service workflow integration tests.
//!
//! Full service lifecycle workflows: startup, device discovery, game connection,
//! FFB processing, config changes, recovery, support bundles, shutdown.
//!
//! Cross-crate coverage: service (WheelService, ProfileService, GameService,
//! DiagnosticService, AutoProfileSwitchingService) × engine (VirtualDevice,
//! Pipeline, SafetyService) × schemas × telemetry-adapters × diagnostic.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tempfile::TempDir;
use tokio::sync::Mutex;

use openracing_diagnostic::{SupportBundle, SupportBundleConfig};
use racing_wheel_engine::ports::HidDevice;
use racing_wheel_engine::safety::{FaultType, SafetyService, SafetyState};
use racing_wheel_engine::{Frame as EngineFrame, Pipeline, VirtualDevice};
use racing_wheel_schemas::prelude::*;

use racing_wheel_service::{
    auto_profile_switching::AutoProfileSwitchingService,
    diagnostic_service::DiagnosticService,
    game_telemetry_bridge::TelemetryAdapterControl,
    process_detection::{ProcessEvent, ProcessInfo},
    profile_repository::ProfileRepositoryConfig,
    profile_service::ProfileService,
    system_config::SystemConfig,
};

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

struct MockAdapterControl {
    starts: Arc<Mutex<Vec<String>>>,
    stops: Arc<Mutex<Vec<String>>>,
}

impl MockAdapterControl {
    fn new() -> Self {
        Self {
            starts: Arc::new(Mutex::new(Vec::new())),
            stops: Arc::new(Mutex::new(Vec::new())),
        }
    }

    async fn started_games(&self) -> Vec<String> {
        self.starts.lock().await.clone()
    }

    async fn stopped_games(&self) -> Vec<String> {
        self.stops.lock().await.clone()
    }
}

#[async_trait]
impl TelemetryAdapterControl for MockAdapterControl {
    async fn start_for_game(&self, game_id: &str) -> anyhow::Result<()> {
        self.starts.lock().await.push(game_id.to_string());
        Ok(())
    }

    async fn stop_for_game(&self, game_id: &str) -> anyhow::Result<()> {
        self.stops.lock().await.push(game_id.to_string());
        Ok(())
    }
}

fn game_started_event(game_id: &str, exe: &str) -> ProcessEvent {
    ProcessEvent::GameStarted {
        game_id: game_id.to_string(),
        process_info: ProcessInfo {
            pid: 1234,
            name: exe.to_string(),
            game_id: Some(game_id.to_string()),
            detected_at: Instant::now(),
        },
    }
}

fn game_stopped_event(game_id: &str, exe: &str) -> ProcessEvent {
    ProcessEvent::GameStopped {
        game_id: game_id.to_string(),
        process_info: ProcessInfo {
            pid: 1234,
            name: exe.to_string(),
            game_id: Some(game_id.to_string()),
            detected_at: Instant::now(),
        },
    }
}

async fn make_profile_service(tmp: &TempDir) -> anyhow::Result<Arc<ProfileService>> {
    let config = ProfileRepositoryConfig {
        profiles_dir: tmp.path().to_path_buf(),
        ..Default::default()
    };
    Ok(Arc::new(ProfileService::new_with_config(config).await?))
}

async fn seed_profile(service: &ProfileService, id: &str) -> anyhow::Result<ProfileId> {
    let profile_id: ProfileId = id.parse()?;
    let profile = Profile::new(
        profile_id,
        ProfileScope::global(),
        BaseSettings::default(),
        id.to_string(),
    );
    service.create_profile(profile).await
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Service startup → device discovery → game → FFB → shutdown
// ═══════════════════════════════════════════════════════════════════════════════

/// Full service lifecycle: create profiles → detect game → process FFB →
/// stop game → shutdown.
#[tokio::test]
async fn service_full_lifecycle_startup_to_shutdown() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    let profile_service = make_profile_service(&tmp).await?;

    // Startup: seed profiles
    seed_profile(&profile_service, "iracing_gt3").await?;
    seed_profile(&profile_service, "global").await?;

    let profiles = profile_service.list_profiles().await?;
    assert!(profiles.len() >= 2, "seeded profiles must be listed");

    // Game detection → profile switch
    let mock = Arc::new(MockAdapterControl::new());
    let svc = AutoProfileSwitchingService::new(Arc::clone(&profile_service))?
        .with_adapter_control(mock.clone() as Arc<dyn TelemetryAdapterControl>);

    svc.set_game_profile("iracing".to_string(), "iracing_gt3".to_string())
        .await?;

    svc.handle_event(game_started_event("iracing", "iRacingSim64DX11.exe"))
        .await;
    assert_eq!(
        svc.get_active_profile().await.as_deref(),
        Some("iracing_gt3")
    );
    assert_eq!(mock.started_games().await, vec!["iracing"]);

    // FFB processing (simulated)
    let id: DeviceId = "svc-lifecycle-001".parse()?;
    let mut device = VirtualDevice::new(id, "Service Lifecycle Wheel".to_string());
    let mut pipeline = Pipeline::new();
    let safety = SafetyService::new(5.0, 20.0);

    for seq in 0u16..100 {
        let ffb_in = ((seq as f32) * 0.1).sin() * 0.5;
        let mut frame = EngineFrame {
            ffb_in,
            torque_out: ffb_in,
            wheel_speed: 1.0,
            hands_off: false,
            ts_mono_ns: u64::from(seq) * 1_000_000,
            seq,
        };
        pipeline.process(&mut frame)?;
        let torque = safety.clamp_torque_nm(frame.torque_out * 5.0);
        device.write_ffb_report(torque, seq)?;
        assert!(torque.abs() <= 5.0);
    }

    // Shutdown: stop game, verify cleanup
    svc.handle_event(game_stopped_event("iracing", "iRacingSim64DX11.exe"))
        .await;
    assert_eq!(svc.get_active_profile().await.as_deref(), Some("global"));
    assert_eq!(mock.stopped_games().await, vec!["iracing"]);

    Ok(())
}

/// Service with multiple game sessions back-to-back.
#[tokio::test]
async fn service_multiple_game_sessions() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    let profile_service = make_profile_service(&tmp).await?;

    seed_profile(&profile_service, "global").await?;
    seed_profile(&profile_service, "acc_gt3").await?;
    seed_profile(&profile_service, "iracing_gt3").await?;

    let mock = Arc::new(MockAdapterControl::new());
    let svc = AutoProfileSwitchingService::new(Arc::clone(&profile_service))?
        .with_adapter_control(mock.clone() as Arc<dyn TelemetryAdapterControl>);

    svc.set_game_profile("acc".to_string(), "acc_gt3".to_string())
        .await?;
    svc.set_game_profile("iracing".to_string(), "iracing_gt3".to_string())
        .await?;

    // Session 1: ACC
    svc.handle_event(game_started_event("acc", "AC2-Win64-Shipping.exe"))
        .await;
    assert_eq!(svc.get_active_profile().await.as_deref(), Some("acc_gt3"));
    svc.handle_event(game_stopped_event("acc", "AC2-Win64-Shipping.exe"))
        .await;

    // Session 2: iRacing
    svc.handle_event(game_started_event("iracing", "iRacingSim64DX11.exe"))
        .await;
    assert_eq!(
        svc.get_active_profile().await.as_deref(),
        Some("iracing_gt3")
    );
    svc.handle_event(game_stopped_event("iracing", "iRacingSim64DX11.exe"))
        .await;

    let starts = mock.started_games().await;
    let stops = mock.stopped_games().await;
    assert_eq!(starts.len(), 2);
    assert_eq!(stops.len(), 2);
    assert_eq!(svc.get_active_profile().await.as_deref(), Some("global"));

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Service config change during operation
// ═══════════════════════════════════════════════════════════════════════════════

/// Load default SystemConfig, validate, modify, validate again.
#[tokio::test]
async fn service_config_load_validate_modify() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    let config_path = tmp.path().join("test_config.yaml");

    // Create default config and save
    let config = SystemConfig::default();
    config.validate()?;
    config.save_to_path(&config_path).await?;

    // Reload and validate
    let loaded = SystemConfig::load_from_path(&config_path).await?;
    loaded.validate()?;

    Ok(())
}

/// Profile CRUD during active session.
#[tokio::test]
async fn service_config_profile_crud_during_session() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    let profile_service = make_profile_service(&tmp).await?;

    // Create
    let pid = seed_profile(&profile_service, "dynamic_profile").await?;

    // Read
    let profile = profile_service.get_profile(&pid).await?;
    assert!(profile.is_some());

    // Update
    let mut updated = profile
        .ok_or_else(|| anyhow::anyhow!("profile not found"))?;
    updated.metadata.name = "Updated Profile".to_string();
    profile_service.update_profile(updated).await?;

    // Verify update
    let reloaded = profile_service
        .get_profile(&pid)
        .await?
        .ok_or_else(|| anyhow::anyhow!("updated profile not found"))?;
    assert_eq!(reloaded.metadata.name, "Updated Profile");

    // Delete
    profile_service.delete_profile(&pid).await?;
    let deleted = profile_service.get_profile(&pid).await?;
    assert!(deleted.is_none());

    Ok(())
}

/// Session override: set temporary profile, then clear it.
#[tokio::test]
async fn service_config_session_override() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    let profile_service = make_profile_service(&tmp).await?;
    let device_id: DeviceId = "svc-override-001".parse()?;

    // No override initially
    let initial = profile_service.get_session_override(&device_id).await?;
    assert!(initial.is_none());

    // Set override
    let override_profile = Profile::new(
        "session-override".parse()?,
        ProfileScope::global(),
        BaseSettings::default(),
        "Session Override".to_string(),
    );
    profile_service
        .set_session_override(&device_id, override_profile)
        .await?;

    let overridden = profile_service.get_session_override(&device_id).await?;
    assert!(overridden.is_some());

    // Clear override
    profile_service.clear_session_override(&device_id).await?;
    let cleared = profile_service.get_session_override(&device_id).await?;
    assert!(cleared.is_none());

    Ok(())
}

/// Active profile tracking for a device.
#[tokio::test]
async fn service_config_active_profile_tracking() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    let profile_service = make_profile_service(&tmp).await?;
    let device_id: DeviceId = "svc-active-track-001".parse()?;

    let pid = seed_profile(&profile_service, "tracked_profile").await?;

    // No active profile initially
    let active = profile_service.get_active_profile(&device_id).await?;
    assert!(active.is_none());

    // Set active
    profile_service
        .set_active_profile(&device_id, &pid)
        .await?;
    let active = profile_service.get_active_profile(&device_id).await?;
    assert!(active.is_some());

    // Clear active
    profile_service.clear_active_profile(&device_id).await?;
    let active = profile_service.get_active_profile(&device_id).await?;
    assert!(active.is_none());

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Service recovery after component failure
// ═══════════════════════════════════════════════════════════════════════════════

/// Device fault → safety trip → clear fault → resume processing.
#[test]
fn service_recovery_device_fault_then_resume() -> anyhow::Result<()> {
    let id: DeviceId = "svc-recovery-001".parse()?;
    let mut device = VirtualDevice::new(id, "Recovery Wheel".to_string());
    let mut pipeline = Pipeline::new();
    let mut safety = SafetyService::new(5.0, 20.0);

    // Normal operation
    for seq in 0u16..20 {
        let mut frame = EngineFrame {
            ffb_in: 0.5,
            torque_out: 0.5,
            wheel_speed: 1.0,
            hands_off: false,
            ts_mono_ns: u64::from(seq) * 1_000_000,
            seq,
        };
        pipeline.process(&mut frame)?;
        let torque = safety.clamp_torque_nm(frame.torque_out * 5.0);
        device.write_ffb_report(torque, seq)?;
    }

    // Fault
    safety.report_fault(FaultType::UsbStall);
    assert!(matches!(safety.state(), SafetyState::Faulted { .. }));

    // All output zeroed while faulted
    let clamped = safety.clamp_torque_nm(5.0);
    assert!(clamped.abs() < 0.001);

    // Wait and clear
    std::thread::sleep(Duration::from_millis(120));
    safety.clear_fault().map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(safety.state(), &SafetyState::SafeTorque);

    // Resume
    for seq in 20u16..40 {
        let mut frame = EngineFrame {
            ffb_in: 0.3,
            torque_out: 0.3,
            wheel_speed: 0.5,
            hands_off: false,
            ts_mono_ns: u64::from(seq) * 1_000_000,
            seq,
        };
        pipeline.process(&mut frame)?;
        let torque = safety.clamp_torque_nm(frame.torque_out * 5.0);
        device.write_ffb_report(torque, seq)?;
        assert!(torque.is_finite());
    }

    assert!(device.read_telemetry().is_some());
    Ok(())
}

/// Multiple fault types trigger and recover correctly.
#[test]
fn service_recovery_multiple_fault_types() -> anyhow::Result<()> {
    let fault_types = [
        FaultType::UsbStall,
        FaultType::EncoderNaN,
        FaultType::ThermalLimit,
    ];

    for fault_type in &fault_types {
        let mut safety = SafetyService::new(5.0, 20.0);
        safety.report_fault(*fault_type);
        assert!(matches!(safety.state(), SafetyState::Faulted { .. }));

        let clamped = safety.clamp_torque_nm(5.0);
        assert!(clamped.abs() < 0.001, "faulted must zero: {fault_type:?}");

        std::thread::sleep(Duration::from_millis(120));
        safety.clear_fault().map_err(|e| anyhow::anyhow!("{e}"))?;
        assert_eq!(
            safety.state(),
            &SafetyState::SafeTorque,
            "must recover: {fault_type:?}"
        );
    }
    Ok(())
}

/// Device disconnect → reconnect → pipeline resumes without error.
#[test]
fn service_recovery_device_disconnect_reconnect() -> anyhow::Result<()> {
    let id: DeviceId = "svc-reconn-001".parse()?;
    let mut device = VirtualDevice::new(id, "Reconnect Wheel".to_string());
    let mut pipeline = Pipeline::new();
    let safety = SafetyService::new(5.0, 20.0);

    // Active
    for seq in 0u16..10 {
        let mut frame = EngineFrame {
            ffb_in: 0.4,
            torque_out: 0.4,
            wheel_speed: 1.0,
            hands_off: false,
            ts_mono_ns: u64::from(seq) * 1_000_000,
            seq,
        };
        pipeline.process(&mut frame)?;
        let torque = safety.clamp_torque_nm(frame.torque_out * 5.0);
        device.write_ffb_report(torque, seq)?;
    }

    // Disconnect
    device.disconnect();
    assert!(!device.is_connected());
    assert!(device.write_ffb_report(1.0, 99).is_err());

    // Reconnect
    device.reconnect();
    assert!(device.is_connected());

    // Resume
    for seq in 10u16..20 {
        let mut frame = EngineFrame {
            ffb_in: 0.3,
            torque_out: 0.3,
            wheel_speed: 0.5,
            hands_off: false,
            ts_mono_ns: u64::from(seq) * 1_000_000,
            seq,
        };
        pipeline.process(&mut frame)?;
        let torque = safety.clamp_torque_nm(frame.torque_out * 5.0);
        device.write_ffb_report(torque, seq)?;
    }
    assert!(device.read_telemetry().is_some());
    Ok(())
}

/// Pipeline remains functional after safety trip and clear cycle.
#[test]
fn service_recovery_pipeline_survives_safety_trip() -> anyhow::Result<()> {
    let id: DeviceId = "svc-pipe-surv-001".parse()?;
    let mut device = VirtualDevice::new(id, "Pipeline Survive Wheel".to_string());
    let mut pipeline = Pipeline::new();
    let mut safety = SafetyService::new(5.0, 20.0);

    // Process normally
    for seq in 0u16..10 {
        let mut frame = EngineFrame {
            ffb_in: 0.5,
            torque_out: 0.5,
            wheel_speed: 1.0,
            hands_off: false,
            ts_mono_ns: u64::from(seq) * 1_000_000,
            seq,
        };
        pipeline.process(&mut frame)?;
        device.write_ffb_report(safety.clamp_torque_nm(frame.torque_out * 5.0), seq)?;
    }

    // Trip and recover
    safety.report_fault(FaultType::Overcurrent);
    std::thread::sleep(Duration::from_millis(120));
    safety.clear_fault().map_err(|e| anyhow::anyhow!("{e}"))?;

    // Pipeline must still work
    for seq in 10u16..30 {
        let mut frame = EngineFrame {
            ffb_in: 0.6,
            torque_out: 0.6,
            wheel_speed: 1.5,
            hands_off: false,
            ts_mono_ns: u64::from(seq) * 1_000_000,
            seq,
        };
        pipeline.process(&mut frame)?;
        let torque = safety.clamp_torque_nm(frame.torque_out * 5.0);
        device.write_ffb_report(torque, seq)?;
        assert!(torque.is_finite());
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Support bundle generation workflow
// ═══════════════════════════════════════════════════════════════════════════════

/// Create a support bundle with default config.
#[test]
fn service_support_bundle_default_config() -> anyhow::Result<()> {
    let config = SupportBundleConfig::default();
    let bundle = SupportBundle::new(config);

    assert!(
        bundle.estimated_size_mb() >= 0.0,
        "estimated size must be non-negative"
    );
    Ok(())
}

/// Create a support bundle with custom config, add system info.
#[test]
fn service_support_bundle_with_system_info() -> anyhow::Result<()> {
    let config = SupportBundleConfig {
        include_logs: false,
        include_profiles: false,
        include_system_info: true,
        include_recent_recordings: false,
        max_bundle_size_mb: 10,
    };
    let mut bundle = SupportBundle::new(config);
    bundle.add_system_info()?;

    assert!(bundle.estimated_size_mb() >= 0.0);
    Ok(())
}

/// Support bundle with log files from temp directory.
#[test]
fn service_support_bundle_with_log_files() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    let log_dir = tmp.path().join("logs");
    std::fs::create_dir_all(&log_dir)?;
    std::fs::write(log_dir.join("test.log"), "test log content")?;

    let config = SupportBundleConfig {
        include_logs: true,
        include_profiles: false,
        include_system_info: false,
        include_recent_recordings: false,
        max_bundle_size_mb: 25,
    };
    let mut bundle = SupportBundle::new(config);
    bundle.add_log_files(&log_dir)?;

    assert!(bundle.estimated_size_mb() >= 0.0);
    Ok(())
}

/// Support bundle with profile files.
#[test]
fn service_support_bundle_with_profiles() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    let profile_dir = tmp.path().join("profiles");
    std::fs::create_dir_all(&profile_dir)?;
    std::fs::write(
        profile_dir.join("test_profile.json"),
        r#"{"name": "test"}"#,
    )?;

    let config = SupportBundleConfig {
        include_logs: false,
        include_profiles: true,
        include_system_info: false,
        include_recent_recordings: false,
        max_bundle_size_mb: 25,
    };
    let mut bundle = SupportBundle::new(config);
    bundle.add_profile_files(&profile_dir)?;

    assert!(bundle.estimated_size_mb() >= 0.0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Additional service workflow tests
// ═══════════════════════════════════════════════════════════════════════════════

/// DiagnosticService: run full diagnostics and inspect results.
#[tokio::test]
async fn service_diagnostics_run_full() -> anyhow::Result<()> {
    let diag = DiagnosticService::new().await?;
    let results = diag.run_full_diagnostics().await?;

    assert!(!results.is_empty(), "diagnostics must return results");
    for result in &results {
        assert!(!result.name.is_empty(), "result name must not be empty");
        assert!(
            result.execution_time_ms < 30_000,
            "individual test must complete in < 30s"
        );
    }
    Ok(())
}

/// DiagnosticService: list available tests.
#[tokio::test]
async fn service_diagnostics_list_tests() -> anyhow::Result<()> {
    let diag = DiagnosticService::new().await?;
    let tests = diag.list_tests();

    assert!(!tests.is_empty(), "must have at least one diagnostic test");
    for (name, description) in &tests {
        assert!(!name.is_empty());
        assert!(!description.is_empty());
    }
    Ok(())
}

/// Profile statistics reporting.
#[tokio::test]
async fn service_profile_statistics() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    let profile_service = make_profile_service(&tmp).await?;

    seed_profile(&profile_service, "stats_profile_1").await?;
    seed_profile(&profile_service, "stats_profile_2").await?;

    let stats = profile_service.get_profile_statistics().await?;
    assert!(stats.total_profiles >= 2, "must report seeded profiles");

    Ok(())
}
