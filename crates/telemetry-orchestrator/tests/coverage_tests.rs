//! Additional coverage tests for racing-wheel-telemetry-orchestrator.
//!
//! Targets edge cases and scenarios not covered by unit tests or comprehensive.rs.

use racing_wheel_telemetry_orchestrator::TelemetryService;
use racing_wheel_telemetry_support::load_default_matrix;
use std::collections::HashMap;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════════
// Matrix edge-case filtering
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn single_game_matrix_registers_at_most_one_adapter() -> TestResult {
    let mut matrix = load_default_matrix()
        .map_err(|e| std::io::Error::other(format!("matrix load: {e}")))?;
    let first_key = matrix
        .games
        .keys()
        .next()
        .cloned()
        .ok_or_else(|| std::io::Error::other("matrix must have at least one game"))?;
    matrix.games.retain(|k, _| k == &first_key);
    assert_eq!(matrix.games.len(), 1);

    let service = TelemetryService::from_support_matrix(Some(matrix));
    assert!(service.adapter_count() <= 1);
    Ok(())
}

#[test]
fn adapter_ids_consistent_across_multiple_calls() -> TestResult {
    let service = TelemetryService::new();
    let first = service.adapter_ids();
    let second = service.adapter_ids();
    assert_eq!(first, second, "adapter_ids must be deterministic");
    Ok(())
}

#[test]
fn supported_games_consistent_across_calls() -> TestResult {
    let service = TelemetryService::new();
    let mut first = service.supported_games();
    let mut second = service.supported_games();
    first.sort();
    second.sort();
    assert_eq!(first, second);
    Ok(())
}

#[test]
fn matrix_game_ids_never_contain_empty_strings() -> TestResult {
    let service = TelemetryService::new();
    for gid in service.matrix_game_ids() {
        assert!(!gid.is_empty(), "matrix game IDs must not be empty");
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// BDD metrics field consistency
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn bdd_metrics_adapter_extra_count_matches_extra_game_ids_len() -> TestResult {
    let service = TelemetryService::new();
    let metrics = service
        .runtime_bdd_metrics()
        .ok_or_else(|| std::io::Error::other("BDD metrics should be present"))?;
    assert_eq!(
        metrics.adapter.extra_count,
        metrics.adapter.extra_game_ids.len()
    );
    assert_eq!(
        metrics.adapter.missing_count,
        metrics.adapter.missing_game_ids.len()
    );
    Ok(())
}

#[test]
fn bdd_metrics_writer_extra_count_matches_extra_game_ids_len() -> TestResult {
    let service = TelemetryService::new();
    let metrics = service
        .runtime_bdd_metrics()
        .ok_or_else(|| std::io::Error::other("BDD metrics should be present"))?;
    assert_eq!(
        metrics.writer.extra_count,
        metrics.writer.extra_game_ids.len()
    );
    assert_eq!(
        metrics.writer.missing_count,
        metrics.writer.missing_game_ids.len()
    );
    Ok(())
}

#[test]
fn bdd_metrics_matrix_game_count_matches_matrix_game_ids() -> TestResult {
    let service = TelemetryService::new();
    let metrics = service
        .runtime_bdd_metrics()
        .ok_or_else(|| std::io::Error::other("BDD metrics should be present"))?;
    assert_eq!(metrics.matrix_game_count, service.matrix_game_ids().len());
    Ok(())
}

#[test]
fn runtime_coverage_report_and_bdd_metrics_agree_on_presence() -> TestResult {
    let service = TelemetryService::new();
    assert_eq!(
        service.runtime_coverage_report().is_some(),
        service.runtime_bdd_metrics().is_some(),
    );
    Ok(())
}

#[test]
fn coverage_report_absent_matches_bdd_metrics_absent_for_no_matrix() -> TestResult {
    let service = TelemetryService::from_support_matrix(None);
    assert!(service.runtime_coverage_report().is_none());
    assert!(service.runtime_bdd_metrics().is_none());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Recording edge cases
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn enable_recording_with_deeply_nested_path() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir
        .path()
        .join("a")
        .join("b")
        .join("c")
        .join("d")
        .join("recording.json");
    let mut service = TelemetryService::new();
    service.enable_recording(path.clone())?;
    assert!(path.parent().is_some_and(|p| p.exists()));
    service.disable_recording();
    Ok(())
}

#[test]
fn multiple_enable_disable_recording_cycles() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = TelemetryService::new();

    for i in 0..5 {
        let path = dir.path().join(format!("recording_{i}.json"));
        service.enable_recording(path)?;
        service.disable_recording();
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Async error paths
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn start_monitoring_empty_string_returns_error() -> TestResult {
    let mut service = TelemetryService::new();
    let result = service.start_monitoring("").await;
    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn stop_monitoring_empty_string_returns_error() -> TestResult {
    let service = TelemetryService::new();
    let result = service.stop_monitoring("").await;
    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn is_game_running_empty_string_returns_error() -> TestResult {
    let service = TelemetryService::new();
    let result = service.is_game_running("").await;
    assert!(result.is_err());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Empty matrix edge cases
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn empty_matrix_bdd_metrics_have_zero_game_count() -> TestResult {
    let matrix = racing_wheel_telemetry_support::GameSupportMatrix {
        games: HashMap::new(),
    };
    let service = TelemetryService::from_support_matrix(Some(matrix));
    let metrics = service
        .runtime_bdd_metrics()
        .ok_or_else(|| std::io::Error::other("BDD metrics should be present for empty matrix"))?;
    assert_eq!(metrics.matrix_game_count, 0);
    Ok(())
}

#[test]
fn empty_matrix_has_no_supported_games() -> TestResult {
    let matrix = racing_wheel_telemetry_support::GameSupportMatrix {
        games: HashMap::new(),
    };
    let service = TelemetryService::from_support_matrix(Some(matrix));
    assert!(service.supported_games().is_empty());
    assert_eq!(service.adapter_count(), 0);
    assert!(service.matrix_game_ids().is_empty());
    Ok(())
}
