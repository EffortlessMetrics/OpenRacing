//! Additional coverage tests for racing-wheel-telemetry-bdd-metrics.
//!
//! Targets edge cases in metrics computation, policy handling,
//! and set operations not covered by unit tests or comprehensive.rs.

use racing_wheel_telemetry_bdd_metrics::{
    BddMatrixMetrics, MatrixParityPolicy, RuntimeBddMatrixMetrics,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════════
// Single-element sets
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn single_element_perfect_match() -> TestResult {
    let m = BddMatrixMetrics::from_sets(["acc"], ["acc"], MatrixParityPolicy::STRICT);
    assert_eq!(m.matrix_game_count, 1);
    assert_eq!(m.registry_game_count, 1);
    assert_eq!(m.missing_count, 0);
    assert_eq!(m.extra_count, 0);
    assert_eq!(m.matrix_coverage_ratio, 1.0);
    assert_eq!(m.registry_coverage_ratio, 1.0);
    assert!(m.parity_ok);
    Ok(())
}

#[test]
fn single_element_mismatch() -> TestResult {
    let m = BddMatrixMetrics::from_sets(["acc"], ["iracing"], MatrixParityPolicy::STRICT);
    assert_eq!(m.matrix_game_count, 1);
    assert_eq!(m.registry_game_count, 1);
    assert_eq!(m.missing_count, 1);
    assert_eq!(m.extra_count, 1);
    assert_eq!(m.matrix_coverage_ratio, 0.0);
    assert_eq!(m.registry_coverage_ratio, 0.0);
    assert!(!m.parity_ok);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Symmetric set operations
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn swapping_matrix_and_registry_swaps_missing_and_extra() -> TestResult {
    let m1 = BddMatrixMetrics::from_sets(
        ["acc", "iracing", "dirt5"],
        ["acc", "iracing"],
        MatrixParityPolicy::LENIENT,
    );
    let m2 = BddMatrixMetrics::from_sets(
        ["acc", "iracing"],
        ["acc", "iracing", "dirt5"],
        MatrixParityPolicy::LENIENT,
    );
    assert_eq!(m1.missing_count, m2.extra_count);
    assert_eq!(m1.extra_count, m2.missing_count);
    Ok(())
}

#[test]
fn disjoint_sets_all_missing_all_extra() -> TestResult {
    let m = BddMatrixMetrics::from_sets(["a", "b"], ["c", "d"], MatrixParityPolicy::STRICT);
    assert_eq!(m.missing_count, 2);
    assert_eq!(m.extra_count, 2);
    assert_eq!(m.matrix_coverage_ratio, 0.0);
    assert_eq!(m.registry_coverage_ratio, 0.0);
    assert!(!m.parity_ok);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// from_parts with inconsistent counts
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn from_parts_with_all_missing() -> TestResult {
    let m = BddMatrixMetrics::from_parts(
        vec!["a".to_string(), "b".to_string(), "c".to_string()],
        vec![],
        vec!["a".to_string(), "b".to_string(), "c".to_string()],
        vec![],
        MatrixParityPolicy::STRICT,
    );
    assert_eq!(m.matrix_game_count, 3);
    assert_eq!(m.registry_game_count, 0);
    assert_eq!(m.missing_count, 3);
    assert_eq!(m.matrix_coverage_ratio, 0.0);
    assert!(!m.parity_ok);
    Ok(())
}

#[test]
fn from_parts_with_all_extra() -> TestResult {
    let m = BddMatrixMetrics::from_parts(
        vec![],
        vec!["x".to_string(), "y".to_string()],
        vec![],
        vec!["x".to_string(), "y".to_string()],
        MatrixParityPolicy::STRICT,
    );
    assert_eq!(m.matrix_game_count, 0);
    assert_eq!(m.registry_game_count, 2);
    assert_eq!(m.extra_count, 2);
    assert!(!m.parity_ok);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Coverage ratio precision
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn coverage_ratio_one_of_three() -> TestResult {
    let m = BddMatrixMetrics::from_sets(["a", "b", "c"], ["a"], MatrixParityPolicy::LENIENT);
    assert!((m.matrix_coverage_ratio - 1.0 / 3.0).abs() < 1e-10);
    Ok(())
}

#[test]
fn coverage_ratio_two_of_four() -> TestResult {
    let m = BddMatrixMetrics::from_sets(
        ["a", "b", "c", "d"],
        ["a", "b"],
        MatrixParityPolicy::LENIENT,
    );
    assert!((m.matrix_coverage_ratio - 0.5).abs() < 1e-10);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// RuntimeBddMatrixMetrics edge cases
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn runtime_metrics_with_zero_game_count() -> TestResult {
    let adapter = BddMatrixMetrics::from_sets(
        Vec::<&str>::new(),
        Vec::<&str>::new(),
        MatrixParityPolicy::STRICT,
    );
    let writer = BddMatrixMetrics::from_sets(
        Vec::<&str>::new(),
        Vec::<&str>::new(),
        MatrixParityPolicy::STRICT,
    );
    let runtime = RuntimeBddMatrixMetrics::new(0, adapter, writer);
    assert_eq!(runtime.matrix_game_count, 0);
    assert!(runtime.parity_ok);
    Ok(())
}

#[test]
fn runtime_metrics_both_registries_fail() -> TestResult {
    let adapter =
        BddMatrixMetrics::from_sets(["acc", "iracing"], ["dirt5"], MatrixParityPolicy::STRICT);
    let writer =
        BddMatrixMetrics::from_sets(["acc", "iracing"], ["eawrc"], MatrixParityPolicy::STRICT);
    let runtime = RuntimeBddMatrixMetrics::new(2, adapter, writer);
    assert!(!runtime.parity_ok);
    assert!(!runtime.adapter.parity_ok);
    assert!(!runtime.writer.parity_ok);
    Ok(())
}

#[test]
fn runtime_metrics_partial_eq() -> TestResult {
    let adapter = BddMatrixMetrics::from_sets(["acc"], ["acc"], MatrixParityPolicy::STRICT);
    let writer = adapter.clone();
    let a = RuntimeBddMatrixMetrics::new(1, adapter.clone(), writer.clone());
    let b = RuntimeBddMatrixMetrics::new(1, adapter, writer);
    assert_eq!(a, b);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Policy edge cases
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn policy_equality_same_constants() -> TestResult {
    assert_eq!(MatrixParityPolicy::STRICT, MatrixParityPolicy::STRICT);
    assert_eq!(
        MatrixParityPolicy::MATRIX_COMPLETE,
        MatrixParityPolicy::MATRIX_COMPLETE
    );
    assert_eq!(MatrixParityPolicy::LENIENT, MatrixParityPolicy::LENIENT);
    Ok(())
}

#[test]
fn policy_inequality_different_constants() -> TestResult {
    assert_ne!(MatrixParityPolicy::STRICT, MatrixParityPolicy::LENIENT);
    assert_ne!(
        MatrixParityPolicy::STRICT,
        MatrixParityPolicy::MATRIX_COMPLETE
    );
    assert_ne!(
        MatrixParityPolicy::MATRIX_COMPLETE,
        MatrixParityPolicy::LENIENT
    );
    Ok(())
}

#[test]
fn custom_policy_missing_only_allowed() -> TestResult {
    let policy = MatrixParityPolicy {
        allow_missing_registry: true,
        allow_extra_registry: false,
    };
    let m = BddMatrixMetrics::from_sets(["a", "b", "c"], ["a"], policy);
    assert!(
        m.parity_ok,
        "missing-allowed policy should accept missing IDs"
    );
    Ok(())
}

#[test]
fn custom_policy_missing_only_rejects_extra() -> TestResult {
    let policy = MatrixParityPolicy {
        allow_missing_registry: true,
        allow_extra_registry: false,
    };
    let m = BddMatrixMetrics::from_sets(["a"], ["a", "b"], policy);
    assert!(
        !m.parity_ok,
        "missing-allowed policy should reject extra IDs"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Mixed case + duplicates combined
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn mixed_case_duplicates_across_sets() -> TestResult {
    let m = BddMatrixMetrics::from_sets(
        ["ACC", "Acc", "acc"],
        ["acc", "ACC"],
        MatrixParityPolicy::STRICT,
    );
    assert_eq!(m.matrix_game_count, 1);
    assert_eq!(m.registry_game_count, 1);
    assert!(m.parity_ok);
    Ok(())
}
