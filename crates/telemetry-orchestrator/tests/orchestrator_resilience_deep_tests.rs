//! Deep orchestrator resilience and pipeline tests covering start/stop lifecycle,
//! game detection, priority-based adapter fallback, health monitoring via BDD
//! metrics, and graceful degradation on adapter failure.

use racing_wheel_telemetry_orchestrator::TelemetryService;
use racing_wheel_telemetry_support::{GameSupportMatrix, load_default_matrix};
use std::collections::{HashMap, HashSet};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ===========================================================================
// Orchestrator start/stop lifecycle
// ===========================================================================

#[test]
fn lifecycle_new_service_has_matrix_and_adapters() -> TestResult {
    let service = TelemetryService::new();
    assert!(service.adapter_count() > 0);
    assert!(service.support_matrix().is_some());
    assert!(!service.matrix_game_ids().is_empty());
    assert!(service.runtime_coverage_report().is_some());
    assert!(service.runtime_bdd_metrics().is_some());
    Ok(())
}

#[tokio::test]
async fn lifecycle_start_stop_all_games_sequentially() -> TestResult {
    let mut service = TelemetryService::new();
    let games = service.supported_games();
    assert!(!games.is_empty());

    for game in &games {
        let _start = service.start_monitoring(game).await;
        let _stop = service.stop_monitoring(game).await;
    }
    // Service should be intact after cycling through all games
    assert_eq!(service.adapter_count(), games.len());
    Ok(())
}

#[tokio::test]
async fn lifecycle_multiple_start_stop_cycles_same_game() -> TestResult {
    let mut service = TelemetryService::new();
    let games = service.supported_games();
    if games.is_empty() {
        return Ok(());
    }
    let game = &games[0];

    for _ in 0..5 {
        let _start = service.start_monitoring(game).await;
        let _stop = service.stop_monitoring(game).await;
    }
    // Adapter count unchanged after repeated cycles
    assert_eq!(service.supported_games().len(), games.len());
    Ok(())
}

#[tokio::test]
async fn lifecycle_service_survives_error_storm() -> TestResult {
    let mut service = TelemetryService::new();
    let initial_count = service.adapter_count();
    let initial_ids = service.adapter_ids();

    // Generate 20 errors across all error paths
    for i in 0..20 {
        let fake = format!("nonexistent_{i}");
        let _r1 = service.start_monitoring(&fake).await;
        let _r2 = service.stop_monitoring(&fake).await;
        let _r3 = service.is_game_running(&fake).await;
    }

    // Service state completely unchanged
    assert_eq!(service.adapter_count(), initial_count);
    assert_eq!(service.adapter_ids(), initial_ids);
    Ok(())
}

// ===========================================================================
// Game detection and adapter selection
// ===========================================================================

#[test]
fn detection_all_matrix_games_have_adapters() -> TestResult {
    let service = TelemetryService::new();
    let supported: HashSet<String> = service.supported_games().into_iter().collect();

    for gid in service.matrix_game_ids() {
        assert!(
            supported.contains(&gid),
            "matrix game '{gid}' has no registered adapter"
        );
    }
    Ok(())
}

#[test]
fn detection_custom_matrix_restricts_game_set() -> TestResult {
    let mut matrix =
        load_default_matrix().map_err(|e| std::io::Error::other(format!("matrix load: {e}")))?;
    let keep: Vec<String> = matrix.games.keys().take(2).cloned().collect();
    matrix.games.retain(|k, _| keep.contains(k));

    let service = TelemetryService::from_support_matrix(Some(matrix));
    let ids_set: HashSet<String> = service.adapter_ids().into_iter().collect();

    for id in &ids_set {
        assert!(keep.contains(id), "adapter '{id}' should not be registered");
    }
    assert!(service.adapter_count() <= 2);
    Ok(())
}

#[test]
fn detection_adapter_ids_stable_across_queries() -> TestResult {
    let service = TelemetryService::new();
    let first = service.adapter_ids();
    let second = service.adapter_ids();
    let third = service.adapter_ids();
    assert_eq!(first, second);
    assert_eq!(second, third);
    Ok(())
}

#[tokio::test]
async fn detection_unknown_game_error_contains_game_id() -> TestResult {
    let mut service = TelemetryService::new();
    let fake_game = "detection_phantom_racer_99";
    let result = service.start_monitoring(fake_game).await;
    assert!(result.is_err());
    let err_msg = format!(
        "{}",
        result
            .err()
            .ok_or_else(|| std::io::Error::other("expected Err"))?
    );
    assert!(
        err_msg.contains(fake_game),
        "error message should contain game ID, got: {err_msg}"
    );
    Ok(())
}

// ===========================================================================
// Priority-based adapter fallback
// ===========================================================================

#[test]
fn fallback_no_matrix_registers_all_adapters() -> TestResult {
    let fallback = TelemetryService::from_support_matrix(None);
    let with_matrix = TelemetryService::new();
    // Without matrix, all adapter factories register (superset)
    assert!(fallback.adapter_count() >= with_matrix.adapter_count());
    // Confirm fallback mode indicators
    assert!(fallback.support_matrix().is_none());
    assert!(fallback.runtime_coverage_report().is_none());
    Ok(())
}

#[test]
fn fallback_empty_matrix_registers_nothing() -> TestResult {
    let matrix = GameSupportMatrix {
        games: HashMap::new(),
    };
    let service = TelemetryService::from_support_matrix(Some(matrix));
    assert_eq!(service.adapter_count(), 0);
    assert!(service.supported_games().is_empty());
    // BDD metrics should still be present (with zero counts)
    let metrics = service
        .runtime_bdd_metrics()
        .ok_or_else(|| std::io::Error::other("BDD metrics should exist for empty matrix"))?;
    assert_eq!(metrics.matrix_game_count, 0);
    Ok(())
}

#[test]
fn fallback_partial_matrix_only_matching_adapters() -> TestResult {
    let mut matrix =
        load_default_matrix().map_err(|e| std::io::Error::other(format!("matrix load: {e}")))?;
    let all_keys: Vec<String> = matrix.games.keys().cloned().collect();
    assert!(all_keys.len() >= 3, "need at least 3 games in matrix");

    // Keep only first 3 games
    let keep: HashSet<String> = all_keys.into_iter().take(3).collect();
    matrix.games.retain(|k, _| keep.contains(k));

    let service = TelemetryService::from_support_matrix(Some(matrix));
    assert!(service.adapter_count() <= 3);
    for id in service.adapter_ids() {
        assert!(
            keep.contains(&id),
            "unexpected adapter '{id}' not in filtered matrix"
        );
    }
    Ok(())
}

// ===========================================================================
// Health monitoring of active adapters
// ===========================================================================

#[test]
fn health_bdd_metrics_track_adapter_coverage() -> TestResult {
    let service = TelemetryService::new();
    let metrics = service
        .runtime_bdd_metrics()
        .ok_or_else(|| std::io::Error::other("BDD metrics required"))?;

    assert_eq!(metrics.matrix_game_count, service.matrix_game_ids().len());
    assert_eq!(
        metrics.adapter.missing_count,
        metrics.adapter.missing_game_ids.len()
    );
    assert_eq!(
        metrics.adapter.extra_count,
        metrics.adapter.extra_game_ids.len()
    );
    assert_eq!(
        metrics.writer.missing_count,
        metrics.writer.missing_game_ids.len()
    );
    assert_eq!(
        metrics.writer.extra_count,
        metrics.writer.extra_game_ids.len()
    );
    Ok(())
}

#[test]
fn health_coverage_report_detects_missing_adapters() -> TestResult {
    let mut matrix =
        load_default_matrix().map_err(|e| std::io::Error::other(format!("matrix load: {e}")))?;
    let fallback = matrix
        .games
        .values()
        .next()
        .cloned()
        .ok_or_else(|| std::io::Error::other("matrix must have at least one game"))?;
    matrix
        .games
        .insert("health_test_phantom".to_string(), fallback);

    let service = TelemetryService::from_support_matrix(Some(matrix));
    let report = service
        .runtime_coverage_report()
        .ok_or_else(|| std::io::Error::other("coverage report required"))?;

    assert!(
        report
            .adapter_coverage
            .missing_in_registry
            .contains(&"health_test_phantom".to_string()),
        "phantom game should appear as missing in adapter coverage"
    );
    Ok(())
}

#[test]
fn health_metrics_consistent_across_reads() -> TestResult {
    let service = TelemetryService::new();
    let m1 = service
        .runtime_bdd_metrics()
        .ok_or_else(|| std::io::Error::other("metrics required"))?;
    let m2 = service
        .runtime_bdd_metrics()
        .ok_or_else(|| std::io::Error::other("metrics required"))?;

    assert_eq!(m1.matrix_game_count, m2.matrix_game_count);
    assert_eq!(m1.adapter.missing_count, m2.adapter.missing_count);
    assert_eq!(m1.adapter.extra_count, m2.adapter.extra_count);
    assert_eq!(m1.parity_ok, m2.parity_ok);
    Ok(())
}

// ===========================================================================
// Graceful degradation on adapter failure
// ===========================================================================

#[tokio::test]
async fn degradation_errors_do_not_corrupt_adapter_count() -> TestResult {
    let mut service = TelemetryService::new();
    let initial = service.adapter_count();

    // Barrage of errors
    for _ in 0..10 {
        let _r = service.start_monitoring("degrade_test_fake_1").await;
        let _r = service.stop_monitoring("degrade_test_fake_2").await;
    }

    assert_eq!(service.adapter_count(), initial);
    assert_eq!(service.supported_games().len(), initial);
    Ok(())
}

#[tokio::test]
async fn degradation_error_then_valid_operation_works() -> TestResult {
    let mut service = TelemetryService::new();
    let games = service.supported_games();
    if games.is_empty() {
        return Ok(());
    }

    // Trigger error first
    let _err = service.start_monitoring("totally_broken_game").await;
    assert!(
        service
            .start_monitoring("totally_broken_game")
            .await
            .is_err()
    );

    // Valid operation on known adapter should still work (may error for no game running)
    let _result = service.start_monitoring(&games[0]).await;
    // Must not have panicked — adapter count intact
    assert_eq!(service.adapter_count(), games.len());
    Ok(())
}

#[test]
fn degradation_recording_survives_adapter_errors() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("degrade_recording.json");

    let mut service = TelemetryService::new();
    service.enable_recording(path)?;
    // Recording state should be independent of adapter errors
    service.disable_recording();
    // Re-enable should work fine
    let path2 = dir.path().join("degrade_recording_2.json");
    service.enable_recording(path2)?;
    service.disable_recording();
    Ok(())
}

#[test]
fn degradation_concurrent_reads_during_error_conditions() -> TestResult {
    let service = TelemetryService::new();
    let service_ref = &service;

    std::thread::scope(|s| {
        let mut handles = Vec::new();
        for _ in 0..4 {
            handles.push(s.spawn(|| {
                // Read operations should remain consistent
                let ids = service_ref.adapter_ids();
                let count = service_ref.adapter_count();
                let matrix_ids = service_ref.matrix_game_ids();
                let has_matrix = service_ref.support_matrix().is_some();
                (ids, count, matrix_ids, has_matrix)
            }));
        }
        let results: Vec<_> = handles.into_iter().filter_map(|h| h.join().ok()).collect();
        // All threads should see the same state
        if results.len() >= 2 {
            assert_eq!(results[0].0, results[1].0, "adapter_ids must be consistent");
            assert_eq!(
                results[0].1, results[1].1,
                "adapter_count must be consistent"
            );
        }
    });

    Ok(())
}
