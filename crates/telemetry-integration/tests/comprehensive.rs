//! Comprehensive tests for the telemetry integration crate.
//!
//! Tests cover the integration layer between telemetry adapters and engine:
//! adapter discovery, registration, data flow, coverage comparison,
//! and policy enforcement.

use racing_wheel_telemetry_integration::{
    CoverageMismatch, CoveragePolicy, RegistryCoverage, RuntimeCoverageReport,
    compare_matrix_and_registry, compare_matrix_and_registry_with_policy,
    compare_runtime_registries_with_policies,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Adapter discovery and registration (compare_matrix_and_registry)
// ---------------------------------------------------------------------------

mod adapter_discovery {
    use super::*;

    #[test]
    fn exact_match_matrix_and_registry() -> TestResult {
        let coverage =
            compare_matrix_and_registry(["iracing", "acc", "dirt5"], ["acc", "dirt5", "iracing"]);
        assert!(coverage.is_exact());
        assert!(coverage.has_complete_matrix_coverage());
        assert!(coverage.has_no_extra_coverage());
        assert_eq!(coverage.matrix_coverage_ratio(), 1.0);
        assert_eq!(coverage.registry_coverage_ratio(), 1.0);
        Ok(())
    }

    #[test]
    fn registry_missing_some_matrix_ids() -> TestResult {
        let coverage = compare_matrix_and_registry(["iracing", "acc", "dirt5"], ["iracing", "acc"]);
        assert!(!coverage.is_exact());
        assert!(!coverage.has_complete_matrix_coverage());
        assert!(coverage.has_no_extra_coverage());
        assert_eq!(coverage.missing_in_registry, vec!["dirt5".to_string()]);
        assert!(coverage.extra_in_registry.is_empty());
        Ok(())
    }

    #[test]
    fn registry_has_extra_ids() -> TestResult {
        let coverage = compare_matrix_and_registry(["iracing", "acc"], ["iracing", "acc", "ams2"]);
        assert!(!coverage.is_exact());
        assert!(coverage.has_complete_matrix_coverage());
        assert!(!coverage.has_no_extra_coverage());
        assert!(coverage.missing_in_registry.is_empty());
        assert_eq!(coverage.extra_in_registry, vec!["ams2".to_string()]);
        Ok(())
    }

    #[test]
    fn both_missing_and_extra() -> TestResult {
        let coverage =
            compare_matrix_and_registry(["iracing", "acc", "dirt5"], ["iracing", "ams2", "eawrc"]);
        assert_eq!(
            coverage.missing_in_registry,
            vec!["acc".to_string(), "dirt5".to_string()]
        );
        assert_eq!(
            coverage.extra_in_registry,
            vec!["ams2".to_string(), "eawrc".to_string()]
        );
        Ok(())
    }

    #[test]
    fn empty_matrix_and_registry() -> TestResult {
        let coverage: RegistryCoverage =
            compare_matrix_and_registry(Vec::<&str>::new(), Vec::<&str>::new());
        assert!(coverage.is_exact());
        assert!(coverage.matrix_game_ids.is_empty());
        assert!(coverage.registry_game_ids.is_empty());
        Ok(())
    }

    #[test]
    fn empty_matrix_with_registry_entries() -> TestResult {
        let coverage = compare_matrix_and_registry(Vec::<&str>::new(), ["iracing", "acc"]);
        assert!(!coverage.is_exact());
        assert!(coverage.has_complete_matrix_coverage()); // nothing to miss
        assert!(!coverage.has_no_extra_coverage());
        assert_eq!(coverage.extra_in_registry.len(), 2);
        Ok(())
    }

    #[test]
    fn empty_registry_with_matrix_entries() -> TestResult {
        let coverage = compare_matrix_and_registry(["iracing", "acc"], Vec::<&str>::new());
        assert!(!coverage.is_exact());
        assert!(!coverage.has_complete_matrix_coverage());
        assert!(coverage.has_no_extra_coverage());
        assert_eq!(coverage.missing_in_registry.len(), 2);
        Ok(())
    }

    #[test]
    fn case_normalization() -> TestResult {
        let coverage = compare_matrix_and_registry(["IRACING", "ACC"], ["iracing", "acc"]);
        assert!(coverage.is_exact());
        Ok(())
    }

    #[test]
    fn deduplication() -> TestResult {
        let coverage =
            compare_matrix_and_registry(["iracing", "iracing", "acc"], ["acc", "acc", "iracing"]);
        assert_eq!(coverage.matrix_game_ids.len(), 2);
        assert_eq!(coverage.registry_game_ids.len(), 2);
        assert!(coverage.is_exact());
        Ok(())
    }

    #[test]
    fn empty_strings_filtered_out() -> TestResult {
        let coverage = compare_matrix_and_registry(["iracing", "", "acc"], ["acc", "", "iracing"]);
        assert_eq!(coverage.matrix_game_ids.len(), 2);
        assert!(coverage.is_exact());
        Ok(())
    }

    #[test]
    fn ids_are_sorted_in_output() -> TestResult {
        let coverage =
            compare_matrix_and_registry(["dirt5", "acc", "iracing"], ["iracing", "dirt5", "acc"]);
        assert_eq!(
            coverage.matrix_game_ids,
            vec![
                "acc".to_string(),
                "dirt5".to_string(),
                "iracing".to_string()
            ]
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Coverage metrics
// ---------------------------------------------------------------------------

mod coverage_metrics {
    use super::*;

    #[test]
    fn metrics_counts_are_correct() -> TestResult {
        let coverage = compare_matrix_and_registry(["a", "b", "c", "d"], ["a", "b", "e"]);
        let metrics = coverage.metrics();
        assert_eq!(metrics.matrix_game_count, 4);
        assert_eq!(metrics.registry_game_count, 3);
        assert_eq!(metrics.missing_count, 2); // c, d
        assert_eq!(metrics.extra_count, 1); // e
        Ok(())
    }

    #[test]
    fn matrix_coverage_ratio_calculation() -> TestResult {
        let coverage = compare_matrix_and_registry(["a", "b", "c", "d"], ["a", "b"]);
        // 2 out of 4 matrix entries covered
        assert!((coverage.matrix_coverage_ratio() - 0.5).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    fn registry_coverage_ratio_calculation() -> TestResult {
        let coverage = compare_matrix_and_registry(["a", "b"], ["a", "b", "c", "d"]);
        // 2 out of 4 registry entries are in matrix
        assert!((coverage.registry_coverage_ratio() - 0.5).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    fn empty_matrix_coverage_ratio_is_zero() -> TestResult {
        let coverage = compare_matrix_and_registry(Vec::<&str>::new(), ["iracing"]);
        assert_eq!(coverage.matrix_coverage_ratio(), 0.0);
        Ok(())
    }

    #[test]
    fn empty_registry_coverage_ratio_is_zero() -> TestResult {
        let coverage = compare_matrix_and_registry(["iracing"], Vec::<&str>::new());
        assert_eq!(coverage.registry_coverage_ratio(), 0.0);
        Ok(())
    }

    #[test]
    fn full_coverage_ratios_are_one() -> TestResult {
        let coverage = compare_matrix_and_registry(["a", "b", "c"], ["a", "b", "c"]);
        assert_eq!(coverage.matrix_coverage_ratio(), 1.0);
        assert_eq!(coverage.registry_coverage_ratio(), 1.0);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Policy enforcement
// ---------------------------------------------------------------------------

mod policy_enforcement {
    use super::*;

    #[test]
    fn strict_policy_requires_exact_match() -> TestResult {
        let ok =
            compare_matrix_and_registry_with_policy(["a", "b"], ["a", "b"], CoveragePolicy::STRICT);
        assert!(ok.is_ok());

        let fail =
            compare_matrix_and_registry_with_policy(["a", "b"], ["a"], CoveragePolicy::STRICT);
        assert!(fail.is_err());

        let fail2 =
            compare_matrix_and_registry_with_policy(["a"], ["a", "b"], CoveragePolicy::STRICT);
        assert!(fail2.is_err());
        Ok(())
    }

    #[test]
    fn matrix_complete_allows_extras_but_not_missing() -> TestResult {
        let ok = compare_matrix_and_registry_with_policy(
            ["a", "b"],
            ["a", "b", "c"],
            CoveragePolicy::MATRIX_COMPLETE,
        );
        assert!(ok.is_ok());

        let fail = compare_matrix_and_registry_with_policy(
            ["a", "b", "c"],
            ["a", "b"],
            CoveragePolicy::MATRIX_COMPLETE,
        );
        assert!(fail.is_err());
        Ok(())
    }

    #[test]
    fn lenient_policy_always_passes() -> TestResult {
        let result = compare_matrix_and_registry_with_policy(
            ["a", "b", "c"],
            ["d", "e"],
            CoveragePolicy::LENIENT,
        );
        assert!(result.is_ok());
        Ok(())
    }

    #[test]
    fn custom_policy_allow_missing_only() -> TestResult {
        let policy = CoveragePolicy {
            allow_missing_registry: true,
            allow_extra_registry: false,
        };

        let ok = compare_matrix_and_registry_with_policy(["a", "b", "c"], ["a"], policy);
        assert!(ok.is_ok());

        let fail = compare_matrix_and_registry_with_policy(["a"], ["a", "b"], policy);
        assert!(fail.is_err());
        Ok(())
    }

    #[test]
    fn policy_is_satisfied_method() -> TestResult {
        let coverage = compare_matrix_and_registry(["a", "b", "c"], ["a", "b"]);
        assert!(!CoveragePolicy::STRICT.is_satisfied(&coverage));
        assert!(!CoveragePolicy::MATRIX_COMPLETE.is_satisfied(&coverage));
        assert!(CoveragePolicy::LENIENT.is_satisfied(&coverage));
        Ok(())
    }

    #[test]
    fn mismatch_error_display() -> TestResult {
        let err = CoverageMismatch {
            matrix_game_ids: vec!["a".into(), "b".into()],
            registry_game_ids: vec!["a".into()],
            missing_in_registry: vec!["b".into()],
            extra_in_registry: vec![],
        };
        let display = err.to_string();
        assert!(display.contains("missing_in_registry"));
        assert!(display.contains("b"));
        Ok(())
    }

    #[test]
    fn mismatch_is_std_error() {
        let err = CoverageMismatch {
            matrix_game_ids: vec![],
            registry_game_ids: vec![],
            missing_in_registry: vec![],
            extra_in_registry: vec![],
        };
        let _: &dyn std::error::Error = &err;
    }
}

// ---------------------------------------------------------------------------
// Data flow: runtime coverage report
// ---------------------------------------------------------------------------

mod runtime_report {
    use super::*;

    fn make_report(matrix: &[&str], adapters: &[&str], writers: &[&str]) -> RuntimeCoverageReport {
        compare_runtime_registries_with_policies(
            matrix.iter().copied(),
            adapters.iter().copied(),
            writers.iter().copied(),
            CoveragePolicy::MATRIX_COMPLETE,
            CoveragePolicy::MATRIX_COMPLETE,
        )
    }

    #[test]
    fn parity_ok_when_both_registries_complete() -> TestResult {
        let report = make_report(
            &["iracing", "acc", "dirt5"],
            &["iracing", "acc", "dirt5"],
            &["iracing", "acc", "dirt5"],
        );
        assert!(report.is_parity_ok());
        assert!(report.adapter_policy_ok());
        assert!(report.writer_policy_ok());
        Ok(())
    }

    #[test]
    fn parity_fails_when_adapter_missing() -> TestResult {
        let report = make_report(
            &["iracing", "acc", "dirt5"],
            &["iracing", "acc"],
            &["iracing", "acc", "dirt5"],
        );
        assert!(!report.adapter_policy_ok());
        assert!(report.writer_policy_ok());
        assert!(!report.is_parity_ok());
        Ok(())
    }

    #[test]
    fn parity_fails_when_writer_missing() -> TestResult {
        let report = make_report(
            &["iracing", "acc", "dirt5"],
            &["iracing", "acc", "dirt5"],
            &["iracing"],
        );
        assert!(report.adapter_policy_ok());
        assert!(!report.writer_policy_ok());
        assert!(!report.is_parity_ok());
        Ok(())
    }

    #[test]
    fn extras_are_ok_with_matrix_complete_policy() -> TestResult {
        let report = make_report(
            &["iracing", "acc"],
            &["iracing", "acc", "experimental"],
            &["iracing", "acc", "beta_writer"],
        );
        assert!(report.is_parity_ok());
        assert_eq!(report.adapter_coverage.extra_in_registry.len(), 1);
        assert_eq!(report.writer_coverage.extra_in_registry.len(), 1);
        Ok(())
    }

    #[test]
    fn runtime_metrics_structure() -> TestResult {
        let report = make_report(
            &["iracing", "acc", "dirt5"],
            &["iracing", "acc"],
            &["iracing", "acc", "dirt5", "ams2"],
        );
        let metrics = report.metrics();
        assert_eq!(metrics.matrix_game_count, 3);
        assert_eq!(metrics.adapter.missing_count, 1);
        assert_eq!(metrics.adapter.extra_count, 0);
        assert_eq!(metrics.writer.missing_count, 0);
        assert_eq!(metrics.writer.extra_count, 1);
        assert!(!metrics.parity_ok); // adapter missing dirt5
        Ok(())
    }

    #[test]
    fn empty_matrix_runtime_report() -> TestResult {
        let report = compare_runtime_registries_with_policies(
            Vec::<&str>::new(),
            Vec::<&str>::new(),
            Vec::<&str>::new(),
            CoveragePolicy::STRICT,
            CoveragePolicy::STRICT,
        );
        assert!(report.is_parity_ok());
        assert!(report.matrix_game_ids.is_empty());
        Ok(())
    }

    #[test]
    fn mixed_policies_adapter_strict_writer_lenient() -> TestResult {
        let report = compare_runtime_registries_with_policies(
            ["iracing", "acc"],
            ["iracing", "acc"],
            ["iracing"], // missing acc
            CoveragePolicy::STRICT,
            CoveragePolicy::LENIENT,
        );
        assert!(report.adapter_policy_ok());
        assert!(report.writer_policy_ok()); // lenient allows missing
        assert!(report.is_parity_ok());
        Ok(())
    }

    #[test]
    fn bdd_metrics_structure() -> TestResult {
        let report = make_report(
            &["iracing", "acc", "dirt5"],
            &["iracing", "acc", "dirt5"],
            &["iracing", "acc", "dirt5"],
        );
        let bdd = report.bdd_metrics();
        assert_eq!(bdd.matrix_game_count, 3);
        assert!(bdd.parity_ok);
        assert!(bdd.adapter.parity_ok);
        assert!(bdd.writer.parity_ok);
        Ok(())
    }

    #[test]
    fn bdd_metrics_with_failures() -> TestResult {
        let report = compare_runtime_registries_with_policies(
            ["iracing", "acc", "dirt5"],
            ["iracing"],
            ["iracing"],
            CoveragePolicy::STRICT,
            CoveragePolicy::STRICT,
        );
        let bdd = report.bdd_metrics();
        assert!(!bdd.parity_ok);
        assert!(!bdd.adapter.parity_ok);
        assert!(!bdd.writer.parity_ok);
        assert_eq!(bdd.adapter.missing_count, 2);
        assert_eq!(bdd.writer.missing_count, 2);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Coverage policy as_bdd_policy round-trip
// ---------------------------------------------------------------------------

mod policy_conversion {
    use super::*;

    #[test]
    fn strict_converts_correctly() {
        let bdd = CoveragePolicy::STRICT.as_bdd_policy();
        assert!(!bdd.allow_missing_registry);
        assert!(!bdd.allow_extra_registry);
    }

    #[test]
    fn matrix_complete_converts_correctly() {
        let bdd = CoveragePolicy::MATRIX_COMPLETE.as_bdd_policy();
        assert!(!bdd.allow_missing_registry);
        assert!(bdd.allow_extra_registry);
    }

    #[test]
    fn lenient_converts_correctly() {
        let bdd = CoveragePolicy::LENIENT.as_bdd_policy();
        assert!(bdd.allow_missing_registry);
        assert!(bdd.allow_extra_registry);
    }
}

// ---------------------------------------------------------------------------
// RegistryCoverage BDD metrics
// ---------------------------------------------------------------------------

mod bdd_metrics {
    use super::*;

    #[test]
    fn coverage_bdd_metrics_reflect_policy() -> TestResult {
        let coverage = compare_matrix_and_registry(["a", "b", "c"], ["a", "b"]);

        let strict = coverage.bdd_metrics(CoveragePolicy::STRICT);
        assert!(!strict.parity_ok);
        assert_eq!(strict.missing_count, 1);
        assert_eq!(strict.extra_count, 0);

        let lenient = coverage.bdd_metrics(CoveragePolicy::LENIENT);
        assert!(lenient.parity_ok);
        Ok(())
    }

    #[test]
    fn exact_coverage_bdd_metrics_all_ok() -> TestResult {
        let coverage = compare_matrix_and_registry(["a", "b"], ["a", "b"]);
        let bdd = coverage.bdd_metrics(CoveragePolicy::STRICT);
        assert!(bdd.parity_ok);
        assert_eq!(bdd.missing_count, 0);
        assert_eq!(bdd.extra_count, 0);
        Ok(())
    }
}
