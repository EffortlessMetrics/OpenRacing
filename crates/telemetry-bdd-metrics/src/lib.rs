//! BDD-oriented matrix parity metrics for telemetry registries.
//!
//! This crate keeps deterministic metric generation separate from registry
//! comparison logic so service/orchestrator layers can assert parity behavior
//! in acceptance tests and runtime diagnostics.

#![deny(static_mut_refs)]

use std::collections::BTreeSet;

/// Policy used to evaluate matrix-vs-registry parity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatrixParityPolicy {
    /// Allow matrix entries without registry coverage.
    pub allow_missing_registry: bool,
    /// Allow registry entries that are not represented in the matrix.
    pub allow_extra_registry: bool,
}

impl MatrixParityPolicy {
    /// Exact matrix/registry parity required.
    pub const STRICT: Self = Self {
        allow_missing_registry: false,
        allow_extra_registry: false,
    };

    /// Matrix coverage must be complete; extra registry IDs are allowed.
    pub const MATRIX_COMPLETE: Self = Self {
        allow_missing_registry: false,
        allow_extra_registry: true,
    };

    /// Missing and extra IDs are allowed.
    pub const LENIENT: Self = Self {
        allow_missing_registry: true,
        allow_extra_registry: true,
    };

    /// Return true when supplied counts satisfy this policy.
    pub fn is_satisfied(self, missing_count: usize, extra_count: usize) -> bool {
        (self.allow_missing_registry || missing_count == 0)
            && (self.allow_extra_registry || extra_count == 0)
    }
}

/// Deterministic BDD metrics for a matrix-vs-registry comparison.
///
/// These fields intentionally mirror the telemetry spec's required matrix
/// counters and ratios:
/// - `matrix_game_count`
/// - `registry_game_count`
/// - `missing_count`
/// - `extra_count`
/// - `matrix_coverage_ratio`
/// - `registry_coverage_ratio`
/// - `parity_ok`
#[derive(Debug, Clone, PartialEq)]
pub struct BddMatrixMetrics {
    pub matrix_game_count: usize,
    pub registry_game_count: usize,
    pub missing_count: usize,
    pub extra_count: usize,
    pub matrix_coverage_ratio: f64,
    pub registry_coverage_ratio: f64,
    pub parity_ok: bool,
    pub missing_game_ids: Vec<String>,
    pub extra_game_ids: Vec<String>,
}

impl BddMatrixMetrics {
    /// Build deterministic metrics from matrix and registry ID collections.
    pub fn from_sets<M, R, MItem, RItem>(
        matrix_game_ids: M,
        registry_game_ids: R,
        policy: MatrixParityPolicy,
    ) -> Self
    where
        M: IntoIterator<Item = MItem>,
        R: IntoIterator<Item = RItem>,
        MItem: AsRef<str>,
        RItem: AsRef<str>,
    {
        let matrix_set = normalize_ids(matrix_game_ids);
        let registry_set = normalize_ids(registry_game_ids);

        let missing_game_ids: Vec<String> = matrix_set.difference(&registry_set).cloned().collect();
        let extra_game_ids: Vec<String> = registry_set.difference(&matrix_set).cloned().collect();

        Self::from_parts(
            matrix_set.iter().cloned().collect(),
            registry_set.iter().cloned().collect(),
            missing_game_ids,
            extra_game_ids,
            policy,
        )
    }

    /// Build deterministic metrics from precomputed coverage vectors.
    pub fn from_parts(
        matrix_game_ids: Vec<String>,
        registry_game_ids: Vec<String>,
        missing_game_ids: Vec<String>,
        extra_game_ids: Vec<String>,
        policy: MatrixParityPolicy,
    ) -> Self {
        let matrix_game_ids = normalize_owned_ids(matrix_game_ids);
        let registry_game_ids = normalize_owned_ids(registry_game_ids);
        let missing_game_ids = normalize_owned_ids(missing_game_ids);
        let extra_game_ids = normalize_owned_ids(extra_game_ids);

        let matrix_game_count = matrix_game_ids.len();
        let registry_game_count = registry_game_ids.len();
        let missing_count = missing_game_ids.len();
        let extra_count = extra_game_ids.len();

        let matrix_coverage_ratio = if matrix_game_count == 0 {
            0.0
        } else {
            matrix_game_count.saturating_sub(missing_count) as f64 / matrix_game_count as f64
        };
        let registry_coverage_ratio = if registry_game_count == 0 {
            0.0
        } else {
            registry_game_count.saturating_sub(extra_count) as f64 / registry_game_count as f64
        };

        let parity_ok = policy.is_satisfied(missing_count, extra_count);

        Self {
            matrix_game_count,
            registry_game_count,
            missing_count,
            extra_count,
            matrix_coverage_ratio,
            registry_coverage_ratio,
            parity_ok,
            missing_game_ids,
            extra_game_ids,
        }
    }
}

/// Runtime telemetry matrix metrics across adapter and writer registries.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeBddMatrixMetrics {
    pub matrix_game_count: usize,
    pub adapter: BddMatrixMetrics,
    pub writer: BddMatrixMetrics,
    pub parity_ok: bool,
}

impl RuntimeBddMatrixMetrics {
    /// Create runtime metrics from per-registry parity snapshots.
    pub fn new(
        matrix_game_count: usize,
        adapter: BddMatrixMetrics,
        writer: BddMatrixMetrics,
    ) -> Self {
        let parity_ok = adapter.parity_ok && writer.parity_ok;

        Self {
            matrix_game_count,
            adapter,
            writer,
            parity_ok,
        }
    }
}

fn normalize_ids<I, T>(ids: I) -> BTreeSet<String>
where
    I: IntoIterator<Item = T>,
    T: AsRef<str>,
{
    ids.into_iter()
        .map(|id| id.as_ref().trim().to_ascii_lowercase())
        .filter(|id| !id.is_empty())
        .collect::<BTreeSet<_>>()
}

fn normalize_owned_ids(ids: Vec<String>) -> Vec<String> {
    normalize_ids(ids.iter().map(String::as_str))
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{BddMatrixMetrics, MatrixParityPolicy, RuntimeBddMatrixMetrics};

    #[test]
    fn bdd_metrics_matrix_complete_fails_when_registry_is_missing_matrix_id() {
        let metrics = BddMatrixMetrics::from_sets(
            ["acc", "iracing", "dirt5"],
            ["acc", "iracing"],
            MatrixParityPolicy::MATRIX_COMPLETE,
        );

        assert_eq!(metrics.matrix_game_count, 3);
        assert_eq!(metrics.registry_game_count, 2);
        assert_eq!(metrics.missing_count, 1);
        assert_eq!(metrics.extra_count, 0);
        assert!(!metrics.parity_ok);
        assert_eq!(metrics.missing_game_ids, vec!["dirt5".to_string()]);
    }

    #[test]
    fn bdd_metrics_matrix_complete_allows_experimental_extras() {
        let metrics = BddMatrixMetrics::from_sets(
            ["acc", "iracing", "dirt5"],
            ["acc", "iracing", "dirt5", "experimental_game"],
            MatrixParityPolicy::MATRIX_COMPLETE,
        );

        assert_eq!(metrics.missing_count, 0);
        assert_eq!(metrics.extra_count, 1);
        assert!(metrics.parity_ok);
        assert_eq!(
            metrics.extra_game_ids,
            vec!["experimental_game".to_string()]
        );
    }

    #[test]
    fn bdd_metrics_strict_requires_exact_parity() {
        let metrics = BddMatrixMetrics::from_sets(
            ["acc", "iracing"],
            ["acc", "iracing", "dirt5"],
            MatrixParityPolicy::STRICT,
        );

        assert!(!metrics.parity_ok);
        assert_eq!(metrics.extra_count, 1);
    }

    #[test]
    fn bdd_metrics_ratio_values_are_deterministic() {
        let metrics = BddMatrixMetrics::from_sets(
            ["acc", "iracing", "dirt5"],
            ["acc", "iracing", "eawrc"],
            MatrixParityPolicy::LENIENT,
        );

        assert_eq!(metrics.matrix_coverage_ratio, 2.0 / 3.0);
        assert_eq!(metrics.registry_coverage_ratio, 2.0 / 3.0);
        assert!(metrics.parity_ok);
    }

    #[test]
    fn runtime_bdd_metrics_combine_adapter_and_writer_parity() {
        let adapter = BddMatrixMetrics::from_sets(
            ["acc", "iracing", "dirt5"],
            ["acc", "iracing", "dirt5", "experimental_game"],
            MatrixParityPolicy::MATRIX_COMPLETE,
        );
        let writer = BddMatrixMetrics::from_sets(
            ["acc", "iracing", "dirt5"],
            ["acc", "iracing"],
            MatrixParityPolicy::MATRIX_COMPLETE,
        );
        let runtime = RuntimeBddMatrixMetrics::new(3, adapter, writer);

        assert_eq!(runtime.matrix_game_count, 3);
        assert!(!runtime.parity_ok);
        assert!(runtime.adapter.parity_ok);
        assert!(!runtime.writer.parity_ok);
    }

    // -----------------------------------------------------------------------
    // Empty set handling
    // -----------------------------------------------------------------------

    #[test]
    fn empty_matrix_and_registry_is_parity_ok_strict() {
        let metrics = BddMatrixMetrics::from_sets(
            Vec::<&str>::new(),
            Vec::<&str>::new(),
            MatrixParityPolicy::STRICT,
        );
        assert_eq!(metrics.matrix_game_count, 0);
        assert_eq!(metrics.registry_game_count, 0);
        assert_eq!(metrics.missing_count, 0);
        assert_eq!(metrics.extra_count, 0);
        assert!(metrics.parity_ok);
    }

    #[test]
    fn empty_matrix_with_registry_entries_strict_fails() {
        let metrics = BddMatrixMetrics::from_sets(
            Vec::<&str>::new(),
            ["acc", "iracing"],
            MatrixParityPolicy::STRICT,
        );
        assert_eq!(metrics.matrix_game_count, 0);
        assert_eq!(metrics.registry_game_count, 2);
        assert_eq!(metrics.extra_count, 2);
        assert!(!metrics.parity_ok);
    }

    #[test]
    fn matrix_with_empty_registry_strict_fails() {
        let metrics = BddMatrixMetrics::from_sets(
            ["acc", "iracing"],
            Vec::<&str>::new(),
            MatrixParityPolicy::STRICT,
        );
        assert_eq!(metrics.missing_count, 2);
        assert!(!metrics.parity_ok);
    }

    #[test]
    fn empty_matrix_with_registry_entries_lenient_ok() {
        let metrics =
            BddMatrixMetrics::from_sets(Vec::<&str>::new(), ["acc"], MatrixParityPolicy::LENIENT);
        assert!(metrics.parity_ok);
    }

    // -----------------------------------------------------------------------
    // Coverage ratio edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn coverage_ratio_zero_when_all_missing() {
        let metrics = BddMatrixMetrics::from_sets(
            ["a", "b", "c"],
            Vec::<&str>::new(),
            MatrixParityPolicy::LENIENT,
        );
        assert_eq!(metrics.matrix_coverage_ratio, 0.0);
    }

    #[test]
    fn coverage_ratio_one_when_perfect_match() {
        let metrics = BddMatrixMetrics::from_sets(
            ["acc", "iracing"],
            ["acc", "iracing"],
            MatrixParityPolicy::STRICT,
        );
        assert_eq!(metrics.matrix_coverage_ratio, 1.0);
        assert_eq!(metrics.registry_coverage_ratio, 1.0);
        assert!(metrics.parity_ok);
    }

    #[test]
    fn coverage_ratios_empty_sets_are_zero() {
        let metrics = BddMatrixMetrics::from_sets(
            Vec::<&str>::new(),
            Vec::<&str>::new(),
            MatrixParityPolicy::LENIENT,
        );
        assert_eq!(metrics.matrix_coverage_ratio, 0.0);
        assert_eq!(metrics.registry_coverage_ratio, 0.0);
    }

    // -----------------------------------------------------------------------
    // Case normalisation
    // -----------------------------------------------------------------------

    #[test]
    fn ids_are_case_normalised() {
        let metrics = BddMatrixMetrics::from_sets(
            ["ACC", "iRacing"],
            ["acc", "iracing"],
            MatrixParityPolicy::STRICT,
        );
        assert_eq!(metrics.missing_count, 0);
        assert_eq!(metrics.extra_count, 0);
        assert!(metrics.parity_ok);
    }

    #[test]
    fn empty_ids_are_filtered_out() {
        let metrics = BddMatrixMetrics::from_sets(
            ["acc", "", "iracing"],
            ["acc", "iracing", ""],
            MatrixParityPolicy::STRICT,
        );
        assert_eq!(metrics.matrix_game_count, 2);
        assert_eq!(metrics.registry_game_count, 2);
        assert!(metrics.parity_ok);
    }

    #[test]
    fn whitespace_only_ids_are_filtered_out() {
        let metrics = BddMatrixMetrics::from_sets(
            ["acc", "   ", "iracing"],
            ["acc", "iracing", "\t"],
            MatrixParityPolicy::STRICT,
        );
        assert_eq!(metrics.matrix_game_count, 2);
        assert_eq!(metrics.registry_game_count, 2);
        assert!(metrics.parity_ok);
    }

    #[test]
    fn ids_are_trimmed_before_comparison() {
        let metrics = BddMatrixMetrics::from_sets(
            [" ACC ", "iracing"],
            ["acc", " iracing\t"],
            MatrixParityPolicy::STRICT,
        );
        assert_eq!(metrics.missing_count, 0);
        assert_eq!(metrics.extra_count, 0);
        assert!(metrics.parity_ok);
    }

    // -----------------------------------------------------------------------
    // Duplicate handling
    // -----------------------------------------------------------------------

    #[test]
    fn duplicate_ids_are_deduplicated() {
        let metrics = BddMatrixMetrics::from_sets(
            ["acc", "acc", "iracing"],
            ["acc", "iracing", "iracing"],
            MatrixParityPolicy::STRICT,
        );
        assert_eq!(metrics.matrix_game_count, 2);
        assert_eq!(metrics.registry_game_count, 2);
        assert!(metrics.parity_ok);
    }

    // -----------------------------------------------------------------------
    // Policy satisfaction
    // -----------------------------------------------------------------------

    #[test]
    fn policy_is_satisfied_all_combinations() {
        assert!(MatrixParityPolicy::STRICT.is_satisfied(0, 0));
        assert!(!MatrixParityPolicy::STRICT.is_satisfied(1, 0));
        assert!(!MatrixParityPolicy::STRICT.is_satisfied(0, 1));
        assert!(!MatrixParityPolicy::STRICT.is_satisfied(1, 1));

        assert!(MatrixParityPolicy::MATRIX_COMPLETE.is_satisfied(0, 0));
        assert!(!MatrixParityPolicy::MATRIX_COMPLETE.is_satisfied(1, 0));
        assert!(MatrixParityPolicy::MATRIX_COMPLETE.is_satisfied(0, 5));

        assert!(MatrixParityPolicy::LENIENT.is_satisfied(0, 0));
        assert!(MatrixParityPolicy::LENIENT.is_satisfied(5, 0));
        assert!(MatrixParityPolicy::LENIENT.is_satisfied(0, 5));
        assert!(MatrixParityPolicy::LENIENT.is_satisfied(5, 5));
    }

    // -----------------------------------------------------------------------
    // Missing and extra game ID lists
    // -----------------------------------------------------------------------

    #[test]
    fn missing_and_extra_ids_are_sorted() {
        let metrics = BddMatrixMetrics::from_sets(
            ["z_game", "a_game", "m_game"],
            ["x_game", "a_game"],
            MatrixParityPolicy::LENIENT,
        );
        // Missing: m_game, z_game (in matrix but not registry)
        assert_eq!(
            metrics.missing_game_ids,
            vec!["m_game".to_string(), "z_game".to_string()]
        );
        // Extra: x_game (in registry but not matrix)
        assert_eq!(metrics.extra_game_ids, vec!["x_game".to_string()]);
    }

    // -----------------------------------------------------------------------
    // RuntimeBddMatrixMetrics
    // -----------------------------------------------------------------------

    #[test]
    fn runtime_parity_ok_when_both_registries_ok() {
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
    }

    #[test]
    fn runtime_parity_fails_when_adapter_fails() {
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
    }

    #[test]
    fn runtime_bdd_metrics_clone_and_debug() {
        let adapter = BddMatrixMetrics::from_sets(["acc"], ["acc"], MatrixParityPolicy::STRICT);
        let writer = adapter.clone();
        let runtime = RuntimeBddMatrixMetrics::new(1, adapter, writer);
        let cloned = runtime.clone();
        assert_eq!(cloned.parity_ok, runtime.parity_ok);
        let debug = format!("{runtime:?}");
        assert!(!debug.is_empty());
    }

    // -----------------------------------------------------------------------
    // from_parts
    // -----------------------------------------------------------------------

    #[test]
    fn from_parts_normalises_ids() {
        let metrics = BddMatrixMetrics::from_parts(
            vec!["ACC".to_string(), "iRacing".to_string()],
            vec!["acc".to_string(), "iracing".to_string()],
            vec![],
            vec![],
            MatrixParityPolicy::STRICT,
        );
        assert_eq!(metrics.matrix_game_count, 2);
        assert_eq!(metrics.registry_game_count, 2);
        assert!(metrics.parity_ok);
    }

    #[test]
    fn from_parts_filters_empty_ids() {
        let metrics = BddMatrixMetrics::from_parts(
            vec!["acc".to_string(), "".to_string()],
            vec!["acc".to_string()],
            vec![],
            vec![],
            MatrixParityPolicy::STRICT,
        );
        assert_eq!(metrics.matrix_game_count, 1);
        assert!(metrics.parity_ok);
    }
}
