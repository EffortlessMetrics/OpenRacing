//! Comprehensive integration tests for racing-wheel-telemetry-bdd-metrics.
//!
//! Covers: metric collection, reporting/aggregation, policy satisfaction,
//! coverage ratios, edge cases, and RuntimeBddMatrixMetrics.

use racing_wheel_telemetry_bdd_metrics::{
    BddMatrixMetrics, MatrixParityPolicy, RuntimeBddMatrixMetrics,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ── Policy constants ────────────────────────────────────────────────────

#[test]
fn strict_policy_fields() -> TestResult {
    let p = MatrixParityPolicy::STRICT;
    assert!(!p.allow_missing_registry);
    assert!(!p.allow_extra_registry);
    Ok(())
}

#[test]
fn matrix_complete_policy_fields() -> TestResult {
    let p = MatrixParityPolicy::MATRIX_COMPLETE;
    assert!(!p.allow_missing_registry);
    assert!(p.allow_extra_registry);
    Ok(())
}

#[test]
fn lenient_policy_fields() -> TestResult {
    let p = MatrixParityPolicy::LENIENT;
    assert!(p.allow_missing_registry);
    assert!(p.allow_extra_registry);
    Ok(())
}

// ── Policy is_satisfied ─────────────────────────────────────────────────

#[test]
fn strict_satisfied_only_when_both_zero() -> TestResult {
    assert!(MatrixParityPolicy::STRICT.is_satisfied(0, 0));
    assert!(!MatrixParityPolicy::STRICT.is_satisfied(1, 0));
    assert!(!MatrixParityPolicy::STRICT.is_satisfied(0, 1));
    assert!(!MatrixParityPolicy::STRICT.is_satisfied(1, 1));
    Ok(())
}

#[test]
fn matrix_complete_allows_extra_only() -> TestResult {
    assert!(MatrixParityPolicy::MATRIX_COMPLETE.is_satisfied(0, 0));
    assert!(!MatrixParityPolicy::MATRIX_COMPLETE.is_satisfied(1, 0));
    assert!(MatrixParityPolicy::MATRIX_COMPLETE.is_satisfied(0, 5));
    assert!(!MatrixParityPolicy::MATRIX_COMPLETE.is_satisfied(1, 5));
    Ok(())
}

#[test]
fn lenient_always_satisfied() -> TestResult {
    assert!(MatrixParityPolicy::LENIENT.is_satisfied(0, 0));
    assert!(MatrixParityPolicy::LENIENT.is_satisfied(5, 0));
    assert!(MatrixParityPolicy::LENIENT.is_satisfied(0, 5));
    assert!(MatrixParityPolicy::LENIENT.is_satisfied(5, 5));
    Ok(())
}

// ── BddMatrixMetrics::from_sets ─────────────────────────────────────────

#[test]
fn from_sets_perfect_match() -> TestResult {
    let m = BddMatrixMetrics::from_sets(
        ["acc", "iracing", "dirt5"],
        ["acc", "iracing", "dirt5"],
        MatrixParityPolicy::STRICT,
    );
    assert_eq!(m.matrix_game_count, 3);
    assert_eq!(m.registry_game_count, 3);
    assert_eq!(m.missing_count, 0);
    assert_eq!(m.extra_count, 0);
    assert_eq!(m.matrix_coverage_ratio, 1.0);
    assert_eq!(m.registry_coverage_ratio, 1.0);
    assert!(m.parity_ok);
    assert!(m.missing_game_ids.is_empty());
    assert!(m.extra_game_ids.is_empty());
    Ok(())
}

#[test]
fn from_sets_missing_registry_entries() -> TestResult {
    let m = BddMatrixMetrics::from_sets(
        ["acc", "iracing", "dirt5"],
        ["acc", "iracing"],
        MatrixParityPolicy::MATRIX_COMPLETE,
    );
    assert_eq!(m.missing_count, 1);
    assert_eq!(m.extra_count, 0);
    assert!(!m.parity_ok);
    assert_eq!(m.missing_game_ids, vec!["dirt5".to_string()]);
    Ok(())
}

#[test]
fn from_sets_extra_registry_entries() -> TestResult {
    let m = BddMatrixMetrics::from_sets(
        ["acc", "iracing"],
        ["acc", "iracing", "experimental"],
        MatrixParityPolicy::MATRIX_COMPLETE,
    );
    assert_eq!(m.extra_count, 1);
    assert!(m.parity_ok);
    assert_eq!(m.extra_game_ids, vec!["experimental".to_string()]);
    Ok(())
}

#[test]
fn from_sets_extra_fails_strict() -> TestResult {
    let m = BddMatrixMetrics::from_sets(
        ["acc"],
        ["acc", "iracing"],
        MatrixParityPolicy::STRICT,
    );
    assert!(!m.parity_ok);
    assert_eq!(m.extra_count, 1);
    Ok(())
}

// ── Empty set handling ──────────────────────────────────────────────────

#[test]
fn empty_sets_strict_parity_ok() -> TestResult {
    let m = BddMatrixMetrics::from_sets(
        Vec::<&str>::new(),
        Vec::<&str>::new(),
        MatrixParityPolicy::STRICT,
    );
    assert_eq!(m.matrix_game_count, 0);
    assert_eq!(m.registry_game_count, 0);
    assert!(m.parity_ok);
    Ok(())
}

#[test]
fn empty_matrix_with_registry_strict_fails() -> TestResult {
    let m = BddMatrixMetrics::from_sets(
        Vec::<&str>::new(),
        ["acc"],
        MatrixParityPolicy::STRICT,
    );
    assert!(!m.parity_ok);
    assert_eq!(m.extra_count, 1);
    Ok(())
}

#[test]
fn matrix_with_empty_registry_strict_fails() -> TestResult {
    let m = BddMatrixMetrics::from_sets(
        ["acc", "iracing"],
        Vec::<&str>::new(),
        MatrixParityPolicy::STRICT,
    );
    assert!(!m.parity_ok);
    assert_eq!(m.missing_count, 2);
    Ok(())
}

#[test]
fn empty_matrix_lenient_ok() -> TestResult {
    let m = BddMatrixMetrics::from_sets(
        Vec::<&str>::new(),
        ["acc"],
        MatrixParityPolicy::LENIENT,
    );
    assert!(m.parity_ok);
    Ok(())
}

// ── Coverage ratios ─────────────────────────────────────────────────────

#[test]
fn coverage_ratio_zero_when_all_missing() -> TestResult {
    let m = BddMatrixMetrics::from_sets(
        ["a", "b", "c"],
        Vec::<&str>::new(),
        MatrixParityPolicy::LENIENT,
    );
    assert_eq!(m.matrix_coverage_ratio, 0.0);
    Ok(())
}

#[test]
fn coverage_ratio_partial() -> TestResult {
    let m = BddMatrixMetrics::from_sets(
        ["a", "b", "c"],
        ["a", "b", "d"],
        MatrixParityPolicy::LENIENT,
    );
    assert_eq!(m.matrix_coverage_ratio, 2.0 / 3.0);
    assert_eq!(m.registry_coverage_ratio, 2.0 / 3.0);
    Ok(())
}

#[test]
fn coverage_ratios_empty_sets_are_zero() -> TestResult {
    let m = BddMatrixMetrics::from_sets(
        Vec::<&str>::new(),
        Vec::<&str>::new(),
        MatrixParityPolicy::LENIENT,
    );
    assert_eq!(m.matrix_coverage_ratio, 0.0);
    assert_eq!(m.registry_coverage_ratio, 0.0);
    Ok(())
}

// ── Case normalization ──────────────────────────────────────────────────

#[test]
fn ids_are_case_normalized() -> TestResult {
    let m = BddMatrixMetrics::from_sets(
        ["ACC", "iRacing"],
        ["acc", "iracing"],
        MatrixParityPolicy::STRICT,
    );
    assert_eq!(m.missing_count, 0);
    assert_eq!(m.extra_count, 0);
    assert!(m.parity_ok);
    Ok(())
}

#[test]
fn empty_ids_are_filtered_out() -> TestResult {
    let m = BddMatrixMetrics::from_sets(
        ["acc", "", "iracing"],
        ["acc", "iracing", ""],
        MatrixParityPolicy::STRICT,
    );
    assert_eq!(m.matrix_game_count, 2);
    assert_eq!(m.registry_game_count, 2);
    assert!(m.parity_ok);
    Ok(())
}

#[test]
fn duplicates_are_deduplicated() -> TestResult {
    let m = BddMatrixMetrics::from_sets(
        ["acc", "acc", "iracing"],
        ["acc", "iracing", "iracing"],
        MatrixParityPolicy::STRICT,
    );
    assert_eq!(m.matrix_game_count, 2);
    assert_eq!(m.registry_game_count, 2);
    assert!(m.parity_ok);
    Ok(())
}

// ── Missing/extra ID lists are sorted ───────────────────────────────────

#[test]
fn missing_and_extra_ids_sorted() -> TestResult {
    let m = BddMatrixMetrics::from_sets(
        ["z_game", "a_game", "m_game"],
        ["x_game", "a_game"],
        MatrixParityPolicy::LENIENT,
    );
    assert_eq!(
        m.missing_game_ids,
        vec!["m_game".to_string(), "z_game".to_string()]
    );
    assert_eq!(m.extra_game_ids, vec!["x_game".to_string()]);
    Ok(())
}

// ── from_parts ──────────────────────────────────────────────────────────

#[test]
fn from_parts_normalizes_ids() -> TestResult {
    let m = BddMatrixMetrics::from_parts(
        vec!["ACC".to_string(), "iRacing".to_string()],
        vec!["acc".to_string(), "iracing".to_string()],
        vec![],
        vec![],
        MatrixParityPolicy::STRICT,
    );
    assert_eq!(m.matrix_game_count, 2);
    assert_eq!(m.registry_game_count, 2);
    assert!(m.parity_ok);
    Ok(())
}

#[test]
fn from_parts_filters_empty_ids() -> TestResult {
    let m = BddMatrixMetrics::from_parts(
        vec!["acc".to_string(), "".to_string()],
        vec!["acc".to_string()],
        vec![],
        vec![],
        MatrixParityPolicy::STRICT,
    );
    assert_eq!(m.matrix_game_count, 1);
    assert!(m.parity_ok);
    Ok(())
}

// ── RuntimeBddMatrixMetrics ─────────────────────────────────────────────

#[test]
fn runtime_parity_ok_when_both_ok() -> TestResult {
    let adapter = BddMatrixMetrics::from_sets(
        ["acc", "iracing"],
        ["acc", "iracing"],
        MatrixParityPolicy::STRICT,
    );
    let writer = BddMatrixMetrics::from_sets(
        ["acc", "iracing"],
        ["acc", "iracing"],
        MatrixParityPolicy::STRICT,
    );
    let runtime = RuntimeBddMatrixMetrics::new(2, adapter, writer);
    assert!(runtime.parity_ok);
    assert_eq!(runtime.matrix_game_count, 2);
    Ok(())
}

#[test]
fn runtime_parity_fails_when_adapter_fails() -> TestResult {
    let adapter = BddMatrixMetrics::from_sets(
        ["acc", "iracing", "dirt5"],
        ["acc"],
        MatrixParityPolicy::STRICT,
    );
    let writer = BddMatrixMetrics::from_sets(
        ["acc", "iracing", "dirt5"],
        ["acc", "iracing", "dirt5"],
        MatrixParityPolicy::STRICT,
    );
    let runtime = RuntimeBddMatrixMetrics::new(3, adapter, writer);
    assert!(!runtime.parity_ok);
    assert!(!runtime.adapter.parity_ok);
    assert!(runtime.writer.parity_ok);
    Ok(())
}

#[test]
fn runtime_parity_fails_when_writer_fails() -> TestResult {
    let adapter = BddMatrixMetrics::from_sets(
        ["acc", "iracing"],
        ["acc", "iracing"],
        MatrixParityPolicy::STRICT,
    );
    let writer = BddMatrixMetrics::from_sets(
        ["acc", "iracing"],
        ["acc"],
        MatrixParityPolicy::STRICT,
    );
    let runtime = RuntimeBddMatrixMetrics::new(2, adapter, writer);
    assert!(!runtime.parity_ok);
    assert!(runtime.adapter.parity_ok);
    assert!(!runtime.writer.parity_ok);
    Ok(())
}

// ── Clone / Debug / PartialEq ───────────────────────────────────────────

#[test]
fn bdd_metrics_clone_and_debug() -> TestResult {
    let m = BddMatrixMetrics::from_sets(
        ["acc"],
        ["acc"],
        MatrixParityPolicy::STRICT,
    );
    let cloned = m.clone();
    assert_eq!(m, cloned);
    let debug = format!("{m:?}");
    assert!(!debug.is_empty());
    Ok(())
}

#[test]
fn runtime_metrics_clone_and_debug() -> TestResult {
    let adapter = BddMatrixMetrics::from_sets(
        ["acc"],
        ["acc"],
        MatrixParityPolicy::STRICT,
    );
    let writer = adapter.clone();
    let runtime = RuntimeBddMatrixMetrics::new(1, adapter, writer);
    let cloned = runtime.clone();
    assert_eq!(runtime, cloned);
    let debug = format!("{runtime:?}");
    assert!(!debug.is_empty());
    Ok(())
}

#[test]
fn policy_clone_copy_debug() -> TestResult {
    let p = MatrixParityPolicy::STRICT;
    let p2 = p;
    assert_eq!(p, p2);
    let debug = format!("{p:?}");
    assert!(!debug.is_empty());
    Ok(())
}

// ── Custom policy construction ──────────────────────────────────────────

#[test]
fn custom_policy_allows_missing_only() -> TestResult {
    let policy = MatrixParityPolicy {
        allow_missing_registry: true,
        allow_extra_registry: false,
    };
    assert!(policy.is_satisfied(5, 0));
    assert!(!policy.is_satisfied(0, 1));
    assert!(!policy.is_satisfied(5, 1));
    Ok(())
}

// ── Large set handling ──────────────────────────────────────────────────

#[test]
fn large_sets_compute_correctly() -> TestResult {
    let matrix: Vec<String> = (0..100).map(|i| format!("game_{i}")).collect();
    let registry: Vec<String> = (50..150).map(|i| format!("game_{i}")).collect();
    let m = BddMatrixMetrics::from_sets(
        matrix.iter().map(String::as_str),
        registry.iter().map(String::as_str),
        MatrixParityPolicy::LENIENT,
    );
    assert_eq!(m.matrix_game_count, 100);
    assert_eq!(m.registry_game_count, 100);
    assert_eq!(m.missing_count, 50);
    assert_eq!(m.extra_count, 50);
    assert!(m.parity_ok);
    Ok(())
}
