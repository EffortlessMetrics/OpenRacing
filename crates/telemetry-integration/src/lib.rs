//! Telemetry integration governance utilities.
//!
//! The matrix is the source of truth for supported game IDs; these helpers keep
//! runtime registries (adapters, config writers, etc.) synchronized with that
//! source.

use racing_wheel_telemetry_bdd_metrics::{
    BddMatrixMetrics, MatrixParityPolicy, RuntimeBddMatrixMetrics,
};
use std::collections::BTreeSet;

/// Coverage report for comparing a runtime registry against the support matrix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryCoverage {
    /// Matrix-backed game IDs, sorted for deterministic output.
    pub matrix_game_ids: Vec<String>,
    /// Registry/game IDs that have constructors or implementations, sorted.
    pub registry_game_ids: Vec<String>,
    /// Matrix IDs that are missing from the registry.
    pub missing_in_registry: Vec<String>,
    /// Registry IDs that are not listed in the matrix.
    pub extra_in_registry: Vec<String>,
}

impl RegistryCoverage {
    /// Return true when matrix and registry are in exact set-based alignment.
    pub fn is_exact(&self) -> bool {
        self.missing_in_registry.is_empty() && self.extra_in_registry.is_empty()
    }

    /// Return matrix coverage ratio as a normalized [0.0, 1.0] value.
    pub fn matrix_coverage_ratio(&self) -> f64 {
        if self.matrix_game_ids.is_empty() {
            return 0.0;
        }

        let covered = self
            .matrix_game_ids
            .len()
            .saturating_sub(self.missing_in_registry.len());
        covered as f64 / self.matrix_game_ids.len() as f64
    }

    /// Return registry-to-matrix ratio as a normalized [0.0, 1.0] value.
    pub fn registry_coverage_ratio(&self) -> f64 {
        if self.registry_game_ids.is_empty() {
            return 0.0;
        }

        let aligned = self
            .registry_game_ids
            .len()
            .saturating_sub(self.extra_in_registry.len());
        aligned as f64 / self.registry_game_ids.len() as f64
    }

    /// Return true when every matrix game is present in the registry.
    pub fn has_complete_matrix_coverage(&self) -> bool {
        self.missing_in_registry.is_empty()
    }

    /// Return true when registry does not define IDs outside the matrix.
    pub fn has_no_extra_coverage(&self) -> bool {
        self.extra_in_registry.is_empty()
    }

    /// Return deterministic coverage metrics for dashboards and BDD gates.
    pub fn metrics(&self) -> RegistryCoverageMetrics {
        RegistryCoverageMetrics {
            matrix_game_count: self.matrix_game_ids.len(),
            registry_game_count: self.registry_game_ids.len(),
            missing_count: self.missing_in_registry.len(),
            extra_count: self.extra_in_registry.len(),
            matrix_coverage_ratio: self.matrix_coverage_ratio(),
            registry_coverage_ratio: self.registry_coverage_ratio(),
        }
    }

    /// Return deterministic policy-aware BDD metrics for this registry coverage.
    pub fn bdd_metrics(&self, policy: CoveragePolicy) -> BddMatrixMetrics {
        BddMatrixMetrics::from_parts(
            self.matrix_game_ids.clone(),
            self.registry_game_ids.clone(),
            self.missing_in_registry.clone(),
            self.extra_in_registry.clone(),
            policy.as_bdd_policy(),
        )
    }
}

/// Deterministic metrics for a single matrix-vs-registry comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct RegistryCoverageMetrics {
    pub matrix_game_count: usize,
    pub registry_game_count: usize,
    pub missing_count: usize,
    pub extra_count: usize,
    pub matrix_coverage_ratio: f64,
    pub registry_coverage_ratio: f64,
}

/// Compare matrix game IDs with registry IDs and produce a deterministic coverage report.
pub fn compare_matrix_and_registry<M, R, MItem, RItem>(
    matrix_game_ids: M,
    registry_game_ids: R,
) -> RegistryCoverage
where
    M: IntoIterator<Item = MItem>,
    R: IntoIterator<Item = RItem>,
    MItem: AsRef<str>,
    RItem: AsRef<str>,
{
    let matrix_set = normalize_ids(matrix_game_ids);
    let registry_set = normalize_ids(registry_game_ids);

    let missing_in_registry = matrix_set
        .difference(&registry_set)
        .cloned()
        .collect::<Vec<_>>();
    let extra_in_registry = registry_set
        .difference(&matrix_set)
        .cloned()
        .collect::<Vec<_>>();

    RegistryCoverage {
        matrix_game_ids: matrix_set.iter().cloned().collect(),
        registry_game_ids: registry_set.iter().cloned().collect(),
        missing_in_registry,
        extra_in_registry,
    }
}

/// Coverage policy for matrix/registry comparison.
#[derive(Debug, Clone, Copy)]
pub struct CoveragePolicy {
    /// Allow matrix entries without registry coverage.
    pub allow_missing_registry: bool,
    /// Allow registry entries that are not represented in the matrix.
    pub allow_extra_registry: bool,
}

impl CoveragePolicy {
    /// Strict policy: require matrix and registry to be exact.
    pub const STRICT: Self = Self {
        allow_missing_registry: false,
        allow_extra_registry: false,
    };

    /// Conservative policy for runtime startup: matrix coverage must be complete; extras are allowed.
    pub const MATRIX_COMPLETE: Self = Self {
        allow_missing_registry: false,
        allow_extra_registry: true,
    };

    /// Lenient policy for discovery/runtime behavior where both missing and extra are tolerated.
    pub const LENIENT: Self = Self {
        allow_missing_registry: true,
        allow_extra_registry: true,
    };

    /// Return true when the supplied coverage satisfies this policy.
    pub fn is_satisfied(self, coverage: &RegistryCoverage) -> bool {
        (self.allow_missing_registry || coverage.has_complete_matrix_coverage())
            && (self.allow_extra_registry || coverage.has_no_extra_coverage())
    }

    /// Convert to the shared BDD policy model.
    pub fn as_bdd_policy(self) -> MatrixParityPolicy {
        MatrixParityPolicy {
            allow_missing_registry: self.allow_missing_registry,
            allow_extra_registry: self.allow_extra_registry,
        }
    }
}

/// Combined matrix/registry parity report for adapter and config-writer registries.
#[derive(Debug, Clone)]
pub struct RuntimeCoverageReport {
    /// Sorted matrix game IDs used as the source of truth.
    pub matrix_game_ids: Vec<String>,
    /// Adapter registry coverage snapshot.
    pub adapter_coverage: RegistryCoverage,
    /// Config-writer registry coverage snapshot.
    pub writer_coverage: RegistryCoverage,
    /// Policy enforced for adapter registry checks.
    pub adapter_policy: CoveragePolicy,
    /// Policy enforced for writer registry checks.
    pub writer_policy: CoveragePolicy,
}

impl RuntimeCoverageReport {
    /// Return true when adapter registry coverage satisfies its policy.
    pub fn adapter_policy_ok(&self) -> bool {
        self.adapter_policy.is_satisfied(&self.adapter_coverage)
    }

    /// Return true when writer registry coverage satisfies its policy.
    pub fn writer_policy_ok(&self) -> bool {
        self.writer_policy.is_satisfied(&self.writer_coverage)
    }

    /// Return true when both registries satisfy their configured policies.
    pub fn is_parity_ok(&self) -> bool {
        self.adapter_policy_ok() && self.writer_policy_ok()
    }

    /// Return deterministic runtime matrix metrics for BDD/observability.
    pub fn metrics(&self) -> RuntimeCoverageMetrics {
        RuntimeCoverageMetrics {
            matrix_game_count: self.matrix_game_ids.len(),
            adapter: self.adapter_coverage.metrics(),
            writer: self.writer_coverage.metrics(),
            parity_ok: self.is_parity_ok(),
        }
    }

    /// Return policy-aware BDD metrics for adapters and config writers.
    pub fn bdd_metrics(&self) -> RuntimeBddMatrixMetrics {
        RuntimeBddMatrixMetrics::new(
            self.matrix_game_ids.len(),
            self.adapter_coverage.bdd_metrics(self.adapter_policy),
            self.writer_coverage.bdd_metrics(self.writer_policy),
        )
    }
}

/// Deterministic runtime matrix metrics across adapter and writer registries.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeCoverageMetrics {
    pub matrix_game_count: usize,
    pub adapter: RegistryCoverageMetrics,
    pub writer: RegistryCoverageMetrics,
    pub parity_ok: bool,
}

/// Compare matrix IDs with adapter and writer registries in one deterministic report.
pub fn compare_runtime_registries_with_policies<M, A, W, MItem, AItem, WItem>(
    matrix_game_ids: M,
    adapter_game_ids: A,
    writer_game_ids: W,
    adapter_policy: CoveragePolicy,
    writer_policy: CoveragePolicy,
) -> RuntimeCoverageReport
where
    M: IntoIterator<Item = MItem>,
    A: IntoIterator<Item = AItem>,
    W: IntoIterator<Item = WItem>,
    MItem: AsRef<str>,
    AItem: AsRef<str>,
    WItem: AsRef<str>,
{
    let matrix_game_ids_set = normalize_ids(matrix_game_ids);
    let matrix_game_ids: Vec<String> = matrix_game_ids_set.iter().cloned().collect();

    let adapter_coverage = compare_matrix_and_registry(&matrix_game_ids, adapter_game_ids);
    let writer_coverage = compare_matrix_and_registry(&matrix_game_ids, writer_game_ids);

    RuntimeCoverageReport {
        matrix_game_ids,
        adapter_coverage,
        writer_coverage,
        adapter_policy,
        writer_policy,
    }
}

/// Detailed mismatch report for matrix/registry alignment checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageMismatch {
    pub matrix_game_ids: Vec<String>,
    pub registry_game_ids: Vec<String>,
    pub missing_in_registry: Vec<String>,
    pub extra_in_registry: Vec<String>,
}

impl std::fmt::Display for CoverageMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "registry coverage mismatch (missing_in_registry={:?}, extra_in_registry={:?})",
            self.missing_in_registry, self.extra_in_registry
        )
    }
}

impl std::error::Error for CoverageMismatch {}

/// Compare coverage using a policy and return mismatches when policy is violated.
pub fn compare_matrix_and_registry_with_policy<M, R, MItem, RItem>(
    matrix_game_ids: M,
    registry_game_ids: R,
    policy: CoveragePolicy,
) -> Result<RegistryCoverage, CoverageMismatch>
where
    M: IntoIterator<Item = MItem>,
    R: IntoIterator<Item = RItem>,
    MItem: AsRef<str>,
    RItem: AsRef<str>,
{
    let coverage = compare_matrix_and_registry(matrix_game_ids, registry_game_ids);

    if !policy.is_satisfied(&coverage) {
        return Err(CoverageMismatch {
            matrix_game_ids: coverage.matrix_game_ids.clone(),
            registry_game_ids: coverage.registry_game_ids.clone(),
            missing_in_registry: coverage.missing_in_registry.clone(),
            extra_in_registry: coverage.extra_in_registry.clone(),
        });
    }

    Ok(coverage)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coverage_exact_match() {
        let coverage =
            compare_matrix_and_registry(["acc", "iracing", "dirt5"], ["iracing", "acc", "dirt5"]);

        assert!(coverage.is_exact());
        assert!(coverage.has_complete_matrix_coverage());
        assert!(coverage.has_no_extra_coverage());
        assert!(coverage.missing_in_registry.is_empty());
        assert!(coverage.extra_in_registry.is_empty());
    }

    #[test]
    fn test_coverage_catches_missing_and_extra_ids() {
        let coverage = compare_matrix_and_registry(
            ["acc", "iracing", "dirt5"],
            ["acc", "iracing", "ams2", "eawrc"],
        );

        assert!(!coverage.is_exact());
        assert_eq!(coverage.missing_in_registry, vec!["dirt5".to_string()]);
        assert_eq!(
            coverage.extra_in_registry,
            vec!["ams2".to_string(), "eawrc".to_string()]
        );
    }

    #[test]
    fn test_coverage_normalizes_and_deduplicates() {
        let coverage = compare_matrix_and_registry(
            ["IRACING", "iracing", "acc", ""],
            ["acc", "iracing", "ACC"],
        );

        assert_eq!(
            coverage.matrix_game_ids,
            vec!["acc".to_string(), "iracing".to_string()]
        );
        assert_eq!(
            coverage.registry_game_ids,
            vec!["acc".to_string(), "iracing".to_string()]
        );
        assert!(coverage.missing_in_registry.is_empty());
        assert!(coverage.extra_in_registry.is_empty());
    }

    #[test]
    fn test_coverage_ratios_are_deterministic() {
        let coverage =
            compare_matrix_and_registry(["acc", "iracing", "dirt5"], ["acc", "iracing", "ams2"]);

        assert_eq!(coverage.matrix_coverage_ratio(), 2.0 / 3.0);
        assert_eq!(coverage.registry_coverage_ratio(), 2.0 / 3.0);
    }

    #[test]
    fn test_policy_strict_enforces_exact_coverage() {
        let result = compare_matrix_and_registry_with_policy(
            ["acc", "iracing", "dirt5"],
            ["acc", "iracing", "ams2", "eawrc"],
            CoveragePolicy::STRICT,
        );

        let mismatch = result.expect_err("strict policy should fail on mismatch");
        assert_eq!(mismatch.missing_in_registry, vec!["dirt5".to_string()]);
        assert_eq!(
            mismatch.extra_in_registry,
            vec!["ams2".to_string(), "eawrc".to_string()]
        );
    }

    #[test]
    fn test_policy_matrix_complete_allows_extras() {
        let pass = compare_matrix_and_registry_with_policy(
            ["acc", "iracing", "dirt5"],
            ["acc", "iracing", "dirt5", "ams2", "eawrc"],
            CoveragePolicy::MATRIX_COMPLETE,
        );
        assert!(pass.is_ok());

        let fail = compare_matrix_and_registry_with_policy(
            ["acc", "iracing", "dirt5"],
            ["acc", "iracing", "ams2", "eawrc"],
            CoveragePolicy::MATRIX_COMPLETE,
        );
        assert!(fail.is_err());
    }

    #[test]
    fn test_policy_lenient_always_ok() {
        let coverage = compare_matrix_and_registry_with_policy(
            ["acc", "iracing", "dirt5"],
            ["acc", "iracing", "ams2", "eawrc"],
            CoveragePolicy::LENIENT,
        )
        .expect("lenient policy should allow all coverage differences");

        assert_eq!(coverage.missing_in_registry, vec!["dirt5".to_string()]);
        assert_eq!(
            coverage.extra_in_registry,
            vec!["ams2".to_string(), "eawrc".to_string()]
        );
    }

    #[test]
    fn test_compare_runtime_registries_with_policies_combines_results() {
        let report = compare_runtime_registries_with_policies(
            ["acc", "iracing", "dirt5"],
            ["acc", "iracing"],
            ["acc", "dirt5"],
            CoveragePolicy::MATRIX_COMPLETE,
            CoveragePolicy::MATRIX_COMPLETE,
        );

        assert_eq!(
            report.adapter_coverage.missing_in_registry,
            vec!["dirt5".to_string()]
        );
        assert_eq!(
            report.writer_coverage.missing_in_registry,
            vec!["iracing".to_string()]
        );
        assert!(!report.adapter_policy_ok());
        assert!(!report.writer_policy_ok());
        assert!(!report.is_parity_ok());
        assert!(!report.adapter_policy.is_satisfied(&report.adapter_coverage));
        assert!(!report.writer_policy.is_satisfied(&report.writer_coverage));
    }

    #[test]
    fn test_registry_metrics_counts_and_ratios() {
        let coverage =
            compare_matrix_and_registry(["acc", "iracing", "dirt5"], ["acc", "iracing", "ams2"]);

        let metrics = coverage.metrics();
        assert_eq!(metrics.matrix_game_count, 3);
        assert_eq!(metrics.registry_game_count, 3);
        assert_eq!(metrics.missing_count, 1);
        assert_eq!(metrics.extra_count, 1);
        assert_eq!(metrics.matrix_coverage_ratio, 2.0 / 3.0);
        assert_eq!(metrics.registry_coverage_ratio, 2.0 / 3.0);
    }

    #[test]
    fn test_runtime_metrics_include_adapter_and_writer_snapshots() {
        let report = compare_runtime_registries_with_policies(
            ["acc", "iracing", "dirt5"],
            ["acc", "iracing", "dirt5", "eawrc"],
            ["acc", "iracing", "dirt5"],
            CoveragePolicy::MATRIX_COMPLETE,
            CoveragePolicy::MATRIX_COMPLETE,
        );

        let metrics = report.metrics();
        assert_eq!(metrics.matrix_game_count, 3);
        assert_eq!(metrics.adapter.extra_count, 1);
        assert_eq!(metrics.writer.extra_count, 0);
        assert!(metrics.parity_ok);
    }

    #[test]
    fn test_registry_bdd_metrics_include_parity_ok() {
        let coverage = compare_matrix_and_registry(["acc", "iracing", "dirt5"], ["acc", "iracing"]);

        let strict = coverage.bdd_metrics(CoveragePolicy::STRICT);
        let matrix_complete = coverage.bdd_metrics(CoveragePolicy::MATRIX_COMPLETE);

        assert_eq!(strict.missing_count, 1);
        assert_eq!(strict.extra_count, 0);
        assert!(!strict.parity_ok);
        assert!(!matrix_complete.parity_ok);
    }

    #[test]
    fn test_runtime_bdd_metrics_respect_registry_policies() {
        let report = compare_runtime_registries_with_policies(
            ["acc", "iracing", "dirt5"],
            ["acc", "iracing", "dirt5", "experimental_adapter"],
            ["acc", "iracing", "dirt5"],
            CoveragePolicy::MATRIX_COMPLETE,
            CoveragePolicy::STRICT,
        );
        let bdd_metrics = report.bdd_metrics();

        assert_eq!(bdd_metrics.matrix_game_count, 3);
        assert_eq!(bdd_metrics.adapter.extra_count, 1);
        assert_eq!(bdd_metrics.adapter.missing_count, 0);
        assert!(bdd_metrics.adapter.parity_ok);
        assert!(bdd_metrics.writer.parity_ok);
        assert!(bdd_metrics.parity_ok);
    }
}
