//! Additional coverage tests for `racing-wheel-telemetry-bdd-metrics`.
//!
//! Targets gaps left by the original suite:
//!
//! * `from_parts` accepting explicit `missing`/`extra` vectors that do not
//!   match what would be recomputed from the matrix/registry (the contract is
//!   that caller-supplied lists win).
//! * `from_parts` normalisation of those vectors (trim, lowercase, dedupe).
//! * The `saturating_sub` branch when `missing_count > matrix_game_count`
//!   (and the symmetric extra-vs-registry case).
//! * Non-ASCII ID semantics (`to_ascii_lowercase` is intentionally
//!   ASCII-only).
//! * `RuntimeBddMatrixMetrics` preserving `matrix_game_count` independent of
//!   the inner per-registry snapshots.
//! * `PartialEq` inequality paths and `MatrixParityPolicy` struct-literal
//!   equality with the named constants.
//! * `is_satisfied` at `usize::MAX`.

use racing_wheel_telemetry_bdd_metrics::{
    BddMatrixMetrics, MatrixParityPolicy, RuntimeBddMatrixMetrics,
};

// ---------------------------------------------------------------------------
// from_parts: caller-supplied missing/extra lists win
// ---------------------------------------------------------------------------

#[test]
fn from_parts_preserves_explicit_missing_vector_disjoint_from_matrix() {
    let metrics = BddMatrixMetrics::from_parts(
        vec!["acc".to_string(), "iracing".to_string()],
        vec!["acc".to_string(), "iracing".to_string()],
        vec!["zzz".to_string()],
        vec![],
        MatrixParityPolicy::LENIENT,
    );
    assert_eq!(metrics.missing_count, 1);
    assert_eq!(metrics.missing_game_ids, vec!["zzz".to_string()]);
    // Matrix coverage is computed from the caller-supplied missing count, so
    // even though matrix==registry the ratio is reduced.
    assert!((metrics.matrix_coverage_ratio - 0.5).abs() < f64::EPSILON);
}

#[test]
fn from_parts_normalises_explicit_missing_and_extra_vectors() {
    let metrics = BddMatrixMetrics::from_parts(
        vec!["acc".to_string()],
        vec!["acc".to_string()],
        vec![
            "DIRT5".to_string(),
            " dirt5 ".to_string(),
            "".to_string(),
            "Dirt5".to_string(),
        ],
        vec![
            "EXP".to_string(),
            "exp".to_string(),
            "\t".to_string(),
        ],
        MatrixParityPolicy::LENIENT,
    );
    // Trim+lowercase+dedupe collapses every "dirt5" variant into one entry.
    assert_eq!(metrics.missing_game_ids, vec!["dirt5".to_string()]);
    assert_eq!(metrics.missing_count, 1);
    assert_eq!(metrics.extra_game_ids, vec!["exp".to_string()]);
    assert_eq!(metrics.extra_count, 1);
}

// ---------------------------------------------------------------------------
// saturating_sub branches
// ---------------------------------------------------------------------------

#[test]
fn from_parts_saturating_sub_when_missing_exceeds_matrix() {
    let metrics = BddMatrixMetrics::from_parts(
        vec!["a".to_string()],
        vec!["a".to_string()],
        vec!["a".to_string(), "b".to_string(), "c".to_string()],
        vec![],
        MatrixParityPolicy::LENIENT,
    );
    // matrix_count = 1, missing_count = 3 → saturating_sub yields 0 / 1 = 0.0
    assert_eq!(metrics.matrix_game_count, 1);
    assert_eq!(metrics.missing_count, 3);
    assert_eq!(metrics.matrix_coverage_ratio, 0.0);
}

#[test]
fn from_parts_saturating_sub_when_extra_exceeds_registry() {
    let metrics = BddMatrixMetrics::from_parts(
        vec!["a".to_string()],
        vec!["a".to_string()],
        vec![],
        vec!["a".to_string(), "b".to_string(), "c".to_string()],
        MatrixParityPolicy::LENIENT,
    );
    // registry_count = 1, extra_count = 3 → saturating_sub yields 0 / 1 = 0.0
    assert_eq!(metrics.registry_game_count, 1);
    assert_eq!(metrics.extra_count, 3);
    assert_eq!(metrics.registry_coverage_ratio, 0.0);
}

// ---------------------------------------------------------------------------
// Non-ASCII semantics (documented: `to_ascii_lowercase` does not touch
// non-ASCII bytes)
// ---------------------------------------------------------------------------

#[test]
fn non_ascii_ids_are_not_lowercased() {
    // "Ácc" and "ácc" differ in the leading byte (U+00C1 vs U+00E1). The
    // ASCII-only lowercase leaves them distinct, so STRICT must fail.
    let metrics = BddMatrixMetrics::from_sets(
        ["Ácc"],
        ["ácc"],
        MatrixParityPolicy::STRICT,
    );
    assert_eq!(metrics.matrix_game_count, 1);
    assert_eq!(metrics.registry_game_count, 1);
    assert!(!metrics.parity_ok);
    assert_eq!(metrics.missing_count, 1);
    assert_eq!(metrics.extra_count, 1);
}

// ---------------------------------------------------------------------------
// RuntimeBddMatrixMetrics
// ---------------------------------------------------------------------------

#[test]
fn runtime_matrix_game_count_is_preserved_verbatim() {
    let adapter = BddMatrixMetrics::from_sets(
        ["acc", "iracing"],
        ["acc", "iracing"],
        MatrixParityPolicy::STRICT,
    );
    let writer = adapter.clone();
    // Pass a deliberately mismatched matrix_game_count; the constructor
    // must store it verbatim and not derive it from `adapter`/`writer`.
    let runtime = RuntimeBddMatrixMetrics::new(99, adapter, writer);
    assert_eq!(runtime.matrix_game_count, 99);
}

#[test]
fn runtime_partial_eq_inequality_on_matrix_game_count() {
    let adapter = BddMatrixMetrics::from_sets(["a"], ["a"], MatrixParityPolicy::STRICT);
    let writer = adapter.clone();
    let r1 = RuntimeBddMatrixMetrics::new(1, adapter.clone(), writer.clone());
    let r2 = RuntimeBddMatrixMetrics::new(2, adapter, writer);
    assert_ne!(r1, r2);
}

// ---------------------------------------------------------------------------
// PartialEq inequality / struct-literal vs constant equality
// ---------------------------------------------------------------------------

#[test]
fn bdd_metrics_partial_eq_inequality() {
    let m1 = BddMatrixMetrics::from_sets(
        ["a", "b"],
        ["a"],
        MatrixParityPolicy::LENIENT,
    );
    let m2 = BddMatrixMetrics::from_sets(
        ["a", "b"],
        ["a", "b"],
        MatrixParityPolicy::LENIENT,
    );
    assert_ne!(m1, m2);
}

#[test]
fn matrix_parity_policy_struct_literals_equal_named_constants() {
    let strict = MatrixParityPolicy {
        allow_missing_registry: false,
        allow_extra_registry: false,
    };
    let matrix_complete = MatrixParityPolicy {
        allow_missing_registry: false,
        allow_extra_registry: true,
    };
    let lenient = MatrixParityPolicy {
        allow_missing_registry: true,
        allow_extra_registry: true,
    };
    assert_eq!(strict, MatrixParityPolicy::STRICT);
    assert_eq!(matrix_complete, MatrixParityPolicy::MATRIX_COMPLETE);
    assert_eq!(lenient, MatrixParityPolicy::LENIENT);

    // And inequality between the named policies.
    assert_ne!(MatrixParityPolicy::STRICT, MatrixParityPolicy::MATRIX_COMPLETE);
    assert_ne!(MatrixParityPolicy::MATRIX_COMPLETE, MatrixParityPolicy::LENIENT);
}

// ---------------------------------------------------------------------------
// is_satisfied saturates at usize::MAX
// ---------------------------------------------------------------------------

#[test]
fn policy_is_satisfied_with_usize_max_arguments() {
    assert!(MatrixParityPolicy::LENIENT.is_satisfied(usize::MAX, usize::MAX));
    assert!(!MatrixParityPolicy::STRICT.is_satisfied(usize::MAX, 0));
    assert!(!MatrixParityPolicy::STRICT.is_satisfied(0, usize::MAX));
    assert!(!MatrixParityPolicy::MATRIX_COMPLETE.is_satisfied(usize::MAX, 0));
    assert!(MatrixParityPolicy::MATRIX_COMPLETE.is_satisfied(0, usize::MAX));
}

// ---------------------------------------------------------------------------
// MatrixParityPolicy derives are visibly exercised
// ---------------------------------------------------------------------------

#[test]
fn matrix_parity_policy_clone_copy_and_debug() {
    let p = MatrixParityPolicy::STRICT;
    let copied = p;
    let cloned = p;
    assert_eq!(p, copied);
    assert_eq!(p, cloned);
    let dbg = format!("{p:?}");
    assert!(dbg.contains("MatrixParityPolicy"));
    assert!(dbg.contains("allow_missing_registry"));
}
