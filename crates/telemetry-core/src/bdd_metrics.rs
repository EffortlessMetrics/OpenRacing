//! BDD-oriented matrix parity metrics for telemetry registries.
//!
//! This module keeps deterministic metric generation separate from registry
//! comparison logic so service/orchestrator layers can assert parity behavior
//! in acceptance tests and runtime diagnostics.

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
        .map(|id| id.as_ref().to_ascii_lowercase())
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

    #[test]
    fn test_policy_is_satisfied() {
        assert!(MatrixParityPolicy::STRICT.is_satisfied(0, 0));
        assert!(!MatrixParityPolicy::STRICT.is_satisfied(1, 0));
        assert!(!MatrixParityPolicy::STRICT.is_satisfied(0, 1));

        assert!(MatrixParityPolicy::MATRIX_COMPLETE.is_satisfied(0, 0));
        assert!(!MatrixParityPolicy::MATRIX_COMPLETE.is_satisfied(1, 0));
        assert!(MatrixParityPolicy::MATRIX_COMPLETE.is_satisfied(0, 1));

        assert!(MatrixParityPolicy::LENIENT.is_satisfied(0, 0));
        assert!(MatrixParityPolicy::LENIENT.is_satisfied(1, 0));
        assert!(MatrixParityPolicy::LENIENT.is_satisfied(0, 1));
        assert!(MatrixParityPolicy::LENIENT.is_satisfied(1, 1));
    }

    #[test]
    fn test_empty_inputs() {
        let metrics = BddMatrixMetrics::from_sets(
            Vec::<String>::new(),
            Vec::<String>::new(),
            MatrixParityPolicy::STRICT,
        );

        assert_eq!(metrics.matrix_game_count, 0);
        assert_eq!(metrics.registry_game_count, 0);
        assert!(metrics.parity_ok);
    }

    #[test]
    fn test_case_normalization() {
        let metrics = BddMatrixMetrics::from_sets(
            ["ACC", "IRACING"],
            ["acc", "iracing"],
            MatrixParityPolicy::STRICT,
        );

        assert_eq!(metrics.missing_count, 0);
        assert_eq!(metrics.extra_count, 0);
        assert!(metrics.parity_ok);
    }

    #[test]
    fn test_empty_string_filtered() {
        let metrics = BddMatrixMetrics::from_sets(
            ["acc", "", "iracing"],
            ["acc", "iracing"],
            MatrixParityPolicy::STRICT,
        );

        assert_eq!(metrics.matrix_game_count, 2);
        assert!(metrics.parity_ok);
    }
}
