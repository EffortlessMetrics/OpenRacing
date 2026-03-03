//! Comprehensive integration tests for the racing-wheel-telemetry-orchestrator crate.
//!
//! Exercises adapter registration, routing, lifecycle management, error propagation,
//! matrix-driven selection, and recording integration.

use racing_wheel_telemetry_orchestrator::TelemetryService;
use std::collections::HashSet;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════════
// Construction and defaults
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn new_service_has_adapters() -> TestResult {
    let service = TelemetryService::new();
    assert!(service.adapter_count() > 0, "expected at least one adapter");
    Ok(())
}

#[test]
fn default_is_equivalent_to_new() -> TestResult {
    let from_new = TelemetryService::new();
    let from_default = TelemetryService::default();
    assert_eq!(from_new.adapter_count(), from_default.adapter_count());
    assert_eq!(from_new.adapter_ids(), from_default.adapter_ids());
    Ok(())
}

#[test]
fn supported_games_matches_adapter_count() -> TestResult {
    let service = TelemetryService::new();
    assert_eq!(service.supported_games().len(), service.adapter_count());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Adapter registration and routing
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn adapter_ids_sorted_lexicographically() -> TestResult {
    let service = TelemetryService::new();
    let ids = service.adapter_ids();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted);
    Ok(())
}

#[test]
fn adapter_ids_are_unique() -> TestResult {
    let service = TelemetryService::new();
    let ids = service.adapter_ids();
    let unique: HashSet<&String> = ids.iter().collect();
    assert_eq!(ids.len(), unique.len(), "adapter IDs must be unique");
    Ok(())
}

#[test]
fn known_games_are_registered() -> TestResult {
    let service = TelemetryService::new();
    let supported: HashSet<String> = service.supported_games().into_iter().collect();
    let expected = ["acc", "forza_motorsport", "iracing", "rfactor2"];
    for game in &expected {
        assert!(
            supported.contains(*game),
            "expected '{game}' in supported games"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Matrix-driven adapter filtering
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn from_support_matrix_none_registers_all_adapters() -> TestResult {
    let service_no_matrix = TelemetryService::from_support_matrix(None);
    let service_with_matrix = TelemetryService::new();
    assert!(service_no_matrix.adapter_count() >= service_with_matrix.adapter_count());
    Ok(())
}

#[test]
fn empty_matrix_registers_no_adapters() -> TestResult {
    let matrix = racing_wheel_telemetry_support::GameSupportMatrix {
        games: std::collections::HashMap::new(),
    };
    let service = TelemetryService::from_support_matrix(Some(matrix));
    assert_eq!(service.adapter_count(), 0);
    assert!(service.supported_games().is_empty());
    assert!(service.adapter_ids().is_empty());
    Ok(())
}

#[test]
fn filtered_matrix_restricts_adapters() -> TestResult {
    let mut matrix = racing_wheel_telemetry_support::load_default_matrix()
        .map_err(|e| std::io::Error::other(format!("matrix load: {e}")))?;
    let keep: Vec<String> = matrix.games.keys().take(2).cloned().collect();
    matrix.games.retain(|k, _| keep.contains(k));
    assert_eq!(matrix.games.len(), 2);

    let service = TelemetryService::from_support_matrix(Some(matrix));
    assert!(service.adapter_count() <= 2);
    for id in service.adapter_ids() {
        assert!(keep.contains(&id));
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Matrix query helpers
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn support_matrix_is_present_when_loaded() -> TestResult {
    let service = TelemetryService::new();
    assert!(service.support_matrix().is_some());
    Ok(())
}

#[test]
fn support_matrix_none_when_no_matrix() -> TestResult {
    let service = TelemetryService::from_support_matrix(None);
    assert!(service.support_matrix().is_none());
    assert!(service.matrix_game_ids().is_empty());
    Ok(())
}

#[test]
fn matrix_game_ids_nonempty_when_loaded() -> TestResult {
    let service = TelemetryService::new();
    assert!(!service.matrix_game_ids().is_empty());
    Ok(())
}

#[test]
fn matrix_game_ids_are_subset_of_supported_games() -> TestResult {
    let service = TelemetryService::new();
    let supported: HashSet<String> = service.supported_games().into_iter().collect();
    for gid in service.matrix_game_ids() {
        assert!(
            supported.contains(&gid),
            "matrix game '{gid}' not in supported games"
        );
    }
    Ok(())
}

#[test]
fn is_game_matrix_supported_true_for_known() -> TestResult {
    let matrix = racing_wheel_telemetry_support::load_default_matrix()
        .map_err(|e| std::io::Error::other(format!("matrix load: {e}")))?;
    let first = matrix
        .games
        .keys()
        .next()
        .cloned()
        .ok_or_else(|| std::io::Error::other("matrix must have at least one game"))?;
    let service = TelemetryService::from_support_matrix(Some(matrix));
    assert!(service.is_game_matrix_supported(&first));
    Ok(())
}

#[test]
fn is_game_matrix_supported_false_for_unknown() -> TestResult {
    let service = TelemetryService::new();
    assert!(!service.is_game_matrix_supported("not_a_real_game_xyz_999"));
    Ok(())
}

#[test]
fn is_game_matrix_supported_false_when_no_matrix() -> TestResult {
    let service = TelemetryService::from_support_matrix(None);
    assert!(!service.is_game_matrix_supported("acc"));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Coverage and BDD metrics
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn runtime_coverage_report_present_with_matrix() -> TestResult {
    let service = TelemetryService::new();
    assert!(service.runtime_coverage_report().is_some());
    Ok(())
}

#[test]
fn runtime_bdd_metrics_present_with_matrix() -> TestResult {
    let service = TelemetryService::new();
    let metrics = service.runtime_bdd_metrics();
    assert!(metrics.is_some());
    if let Some(m) = metrics {
        assert_eq!(m.matrix_game_count, service.matrix_game_ids().len());
    }
    Ok(())
}

#[test]
fn coverage_and_bdd_metrics_none_without_matrix() -> TestResult {
    let service = TelemetryService::from_support_matrix(None);
    assert!(service.runtime_coverage_report().is_none());
    assert!(service.runtime_bdd_metrics().is_none());
    Ok(())
}

#[test]
fn bdd_metrics_parity_ok_with_matching_matrix() -> TestResult {
    let matrix = racing_wheel_telemetry_support::load_default_matrix()
        .map_err(|e| std::io::Error::other(format!("matrix load: {e}")))?;
    let subset: Vec<String> = matrix.games.keys().take(2).cloned().collect();
    let mut small_matrix = matrix;
    small_matrix.games.retain(|k, _| subset.contains(k));

    let service = TelemetryService::from_support_matrix(Some(small_matrix));
    let metrics = service
        .runtime_bdd_metrics()
        .ok_or_else(|| std::io::Error::other("BDD metrics should be present"))?;
    assert_eq!(metrics.matrix_game_count, 2);
    assert_eq!(metrics.adapter.missing_count, 0);
    assert!(metrics.adapter.parity_ok);
    Ok(())
}

#[test]
fn bdd_metrics_fail_for_unimplemented_game() -> TestResult {
    let mut matrix = racing_wheel_telemetry_support::load_default_matrix()
        .map_err(|e| std::io::Error::other(format!("matrix load: {e}")))?;
    let fallback = matrix
        .games
        .values()
        .next()
        .cloned()
        .ok_or_else(|| std::io::Error::other("matrix must have at least one game"))?;
    matrix
        .games
        .insert("bdd_missing_game_test".to_string(), fallback);

    let service = TelemetryService::from_support_matrix(Some(matrix));
    let metrics = service
        .runtime_bdd_metrics()
        .ok_or_else(|| std::io::Error::other("BDD metrics should be present"))?;
    assert!(metrics.adapter.missing_count >= 1);
    assert!(
        metrics
            .adapter
            .missing_game_ids
            .contains(&"bdd_missing_game_test".to_string())
    );
    assert!(!metrics.adapter.parity_ok);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Lifecycle management (async)
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn start_monitoring_unknown_game_returns_error() -> TestResult {
    let mut service = TelemetryService::new();
    let result = service.start_monitoring("nonexistent_xyz_999").await;
    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn stop_monitoring_unknown_game_returns_error() -> TestResult {
    let service = TelemetryService::new();
    let result = service.stop_monitoring("nonexistent_xyz_999").await;
    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn is_game_running_unknown_game_returns_error() -> TestResult {
    let service = TelemetryService::new();
    let result = service.is_game_running("nonexistent_xyz_999").await;
    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn start_monitoring_known_adapter_does_not_panic() -> TestResult {
    let mut service = TelemetryService::new();
    let games = service.supported_games();
    assert!(!games.is_empty());
    let _result = service.start_monitoring(&games[0]).await;
    Ok(())
}

#[tokio::test]
async fn stop_monitoring_known_adapter_does_not_panic() -> TestResult {
    let service = TelemetryService::new();
    let games = service.supported_games();
    assert!(!games.is_empty());
    let _result = service.stop_monitoring(&games[0]).await;
    Ok(())
}

#[tokio::test]
async fn is_game_running_known_adapter_returns_result() -> TestResult {
    let service = TelemetryService::new();
    let games = service.supported_games();
    assert!(!games.is_empty());
    let _result = service.is_game_running(&games[0]).await;
    Ok(())
}

#[tokio::test]
async fn switching_between_games_does_not_panic() -> TestResult {
    let mut service = TelemetryService::new();
    let games = service.supported_games();
    if games.len() >= 2 {
        let _r1 = service.stop_monitoring(&games[0]).await;
        let _r2 = service.start_monitoring(&games[1]).await;
        let _r3 = service.stop_monitoring(&games[1]).await;
        let _r4 = service.start_monitoring(&games[0]).await;
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Error propagation
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn error_message_contains_game_id() -> TestResult {
    let mut service = TelemetryService::new();
    let result = service.start_monitoring("totally_fake_game").await;
    assert!(result.is_err());
    let msg = format!(
        "{}",
        result
            .err()
            .ok_or_else(|| std::io::Error::other("expected Err"))?
    );
    assert!(
        msg.contains("totally_fake_game"),
        "error should mention the game id, got: {msg}"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Recording lifecycle integration
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn enable_then_disable_recording() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("telemetry_test.json");
    let mut service = TelemetryService::new();
    service.enable_recording(path)?;
    service.disable_recording();
    Ok(())
}

#[test]
fn enable_recording_creates_parent_dirs() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("nested").join("dir").join("out.json");
    let mut service = TelemetryService::new();
    service.enable_recording(path.clone())?;
    assert!(path.parent().is_some_and(|p| p.exists()));
    service.disable_recording();
    Ok(())
}

#[test]
fn disable_recording_is_idempotent() -> TestResult {
    let mut service = TelemetryService::new();
    service.disable_recording();
    service.disable_recording();
    Ok(())
}

#[test]
fn enable_recording_twice_replaces_recorder() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path1 = dir.path().join("recording1.json");
    let path2 = dir.path().join("recording2.json");
    let mut service = TelemetryService::new();
    service.enable_recording(path1)?;
    service.enable_recording(path2)?;
    service.disable_recording();
    Ok(())
}
