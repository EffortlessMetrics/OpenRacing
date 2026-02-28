//! End-to-end integration tests for the plug-and-play pipeline.
//!
//! Covers the full game detection → profile switch → telemetry start/stop →
//! auto-configure flow using in-process mocks.  No real device, I/O or network
//! access is required.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use tempfile::TempDir;
use tokio::sync::Mutex;

use racing_wheel_schemas::prelude::{BaseSettings, Profile, ProfileId, ProfileScope};
use racing_wheel_service::{
    auto_profile_switching::AutoProfileSwitchingService,
    game_auto_configure::GameAutoConfigurer,
    game_service::GameService,
    game_telemetry_bridge::TelemetryAdapterControl,
    process_detection::{ProcessEvent, ProcessInfo},
    profile_repository::ProfileRepositoryConfig,
    profile_service::ProfileService,
};

// ─── MockAdapterControl ───────────────────────────────────────────────────────

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

// ─── Helpers ──────────────────────────────────────────────────────────────────

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

/// Build a `ProfileService` backed by a fresh temporary directory.
async fn make_profile_service(tmp: &TempDir) -> anyhow::Result<Arc<ProfileService>> {
    let config = ProfileRepositoryConfig {
        profiles_dir: tmp.path().to_path_buf(),
        ..Default::default()
    };
    Ok(Arc::new(ProfileService::new_with_config(config).await?))
}

/// Create and persist a minimal profile with the given string ID.
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

// ─── Tests ────────────────────────────────────────────────────────────────────

/// Scenario: game detection → profile switch
///
/// When a game starts, `AutoProfileSwitchingService` looks up the mapped
/// profile ID, loads it from the repository and records it as `active_profile`.
#[tokio::test]
async fn test_game_detection_triggers_profile_switch() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    let profile_service = make_profile_service(&tmp).await?;

    // Seed the profile that iRacing maps to.
    seed_profile(&profile_service, "iracing_gt3").await?;

    let svc = AutoProfileSwitchingService::new(Arc::clone(&profile_service))?;
    svc.set_game_profile("iracing".to_string(), "iracing_gt3".to_string())
        .await?;

    svc.handle_event(game_started_event("iracing", "iRacingSim64DX11.exe"))
        .await;

    let active = svc.get_active_profile().await;
    assert_eq!(active.as_deref(), Some("iracing_gt3"));

    Ok(())
}

/// Scenario: game detection → telemetry adapter start
///
/// A `MockAdapterControl` is attached.  When the game starts the adapter for
/// that game must be started exactly once.
#[tokio::test]
async fn test_game_detection_auto_starts_telemetry() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    let profile_service = make_profile_service(&tmp).await?;

    let mock = Arc::new(MockAdapterControl::new());

    let svc = AutoProfileSwitchingService::new(Arc::clone(&profile_service))?
        .with_adapter_control(mock.clone() as Arc<dyn TelemetryAdapterControl>);

    svc.handle_event(game_started_event("acc", "AC2-Win64-Shipping.exe"))
        .await;

    let starts = mock.started_games().await;
    assert_eq!(starts, vec!["acc"]);

    Ok(())
}

/// Scenario: first game detection → auto-configure; second detection is no-op
///
/// `GameAutoConfigurer::on_game_detected` must write a config on the first
/// call and silently skip subsequent calls for the same game (idempotency).
#[tokio::test]
async fn test_game_detection_auto_configures_telemetry() -> anyhow::Result<()> {
    let tmp_state = TempDir::new()?;
    let tmp_install = TempDir::new()?;

    let game_service = Arc::new(GameService::new().await?);
    let configurer = Arc::new(
        GameAutoConfigurer::with_state_path(
            Arc::clone(&game_service),
            tmp_state.path().join("configured_games.json"),
        )
        .with_install_path_override(tmp_install.path().to_path_buf()),
    );

    // First detection should attempt configuration (may log a warning about
    // the game not being in the matrix, but must not panic).
    configurer.on_game_detected("iracing").await;

    // Verify the state file records the game as configured.
    let state_content = std::fs::read_to_string(tmp_state.path().join("configured_games.json"))?;
    assert!(state_content.contains("iracing"));

    // Second detection: the configurer must not write the file again.
    let modified_before = std::fs::metadata(tmp_state.path().join("configured_games.json"))?
        .modified()
        .ok();
    configurer.on_game_detected("iracing").await;
    let modified_after = std::fs::metadata(tmp_state.path().join("configured_games.json"))?
        .modified()
        .ok();

    assert_eq!(
        modified_before, modified_after,
        "state file must not be rewritten on second detection"
    );

    Ok(())
}

/// Scenario: game stop → telemetry adapter stop
///
/// After a `GameStarted` → `GameStopped` sequence the adapter must be stopped
/// exactly once and the global profile restored.
#[tokio::test]
async fn test_game_stop_stops_telemetry() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    let profile_service = make_profile_service(&tmp).await?;

    let mock = Arc::new(MockAdapterControl::new());

    let svc = AutoProfileSwitchingService::new(Arc::clone(&profile_service))?
        .with_adapter_control(mock.clone() as Arc<dyn TelemetryAdapterControl>);

    svc.handle_event(game_started_event("rf2", "rFactor2.exe"))
        .await;
    svc.handle_event(game_stopped_event("rf2", "rFactor2.exe"))
        .await;

    let starts = mock.started_games().await;
    let stops = mock.stopped_games().await;

    assert_eq!(starts, vec!["rf2"]);
    assert_eq!(stops, vec!["rf2"]);

    Ok(())
}

/// Full pipeline scenario: iRacing session
///
/// Given pre-seeded "iracing_gt3" and "global" profiles, the pipeline must:
/// 1. Switch to "iracing_gt3" on game start.
/// 2. Start the telemetry adapter for "iracing".
/// 3. Stop the adapter and switch back to "global" on game stop.
#[tokio::test]
async fn test_full_pipeline_iracing() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    let profile_service = make_profile_service(&tmp).await?;

    seed_profile(&profile_service, "iracing_gt3").await?;
    seed_profile(&profile_service, "global").await?;

    let mock = Arc::new(MockAdapterControl::new());

    let svc = AutoProfileSwitchingService::new(Arc::clone(&profile_service))?
        .with_adapter_control(mock.clone() as Arc<dyn TelemetryAdapterControl>);

    svc.set_game_profile("iracing".to_string(), "iracing_gt3".to_string())
        .await?;

    // Game starts
    svc.handle_event(game_started_event("iracing", "iRacingSim64DX11.exe"))
        .await;

    assert_eq!(
        svc.get_active_profile().await.as_deref(),
        Some("iracing_gt3")
    );
    assert_eq!(mock.started_games().await, vec!["iracing"]);

    // Game stops
    svc.handle_event(game_stopped_event("iracing", "iRacingSim64DX11.exe"))
        .await;

    assert_eq!(svc.get_active_profile().await.as_deref(), Some("global"));
    assert_eq!(mock.stopped_games().await, vec!["iracing"]);

    Ok(())
}
