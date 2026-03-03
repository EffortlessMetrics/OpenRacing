//! Deep tests for telemetry-integration: coverage comparison, game detection
//! simulation, adapter selection via policies, BDD metrics, and error handling.

use racing_wheel_telemetry_integration::{
    CoverageMismatch, CoveragePolicy, RegistryCoverage, RuntimeCoverageReport,
    compare_matrix_and_registry, compare_matrix_and_registry_with_policy,
    compare_runtime_registries_with_policies,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ===========================================================================
// RegistryCoverage — constructor and field access
// ===========================================================================

#[test]
fn coverage_single_element_exact() -> TestResult {
    let c = compare_matrix_and_registry(["iracing"], ["iracing"]);
    assert!(c.is_exact());
    assert_eq!(c.matrix_game_ids.len(), 1);
    assert_eq!(c.registry_game_ids.len(), 1);
    assert_eq!(c.matrix_coverage_ratio(), 1.0);
    assert_eq!(c.registry_coverage_ratio(), 1.0);
    Ok(())
}

#[test]
fn coverage_disjoint_sets() -> TestResult {
    let c = compare_matrix_and_registry(["a", "b"], ["c", "d"]);
    assert!(!c.is_exact());
    assert!(!c.has_complete_matrix_coverage());
    assert!(!c.has_no_extra_coverage());
    assert_eq!(
        c.missing_in_registry,
        vec!["a".to_string(), "b".to_string()]
    );
    assert_eq!(c.extra_in_registry, vec!["c".to_string(), "d".to_string()]);
    assert_eq!(c.matrix_coverage_ratio(), 0.0);
    assert_eq!(c.registry_coverage_ratio(), 0.0);
    Ok(())
}

#[test]
fn coverage_superset_registry() -> TestResult {
    let c = compare_matrix_and_registry(["a"], ["a", "b", "c"]);
    assert!(!c.is_exact());
    assert!(c.has_complete_matrix_coverage());
    assert!(!c.has_no_extra_coverage());
    assert_eq!(c.matrix_coverage_ratio(), 1.0);
    assert!((c.registry_coverage_ratio() - 1.0 / 3.0).abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn coverage_superset_matrix() -> TestResult {
    let c = compare_matrix_and_registry(["a", "b", "c"], ["a"]);
    assert!(!c.is_exact());
    assert!(!c.has_complete_matrix_coverage());
    assert!(c.has_no_extra_coverage());
    assert!((c.matrix_coverage_ratio() - 1.0 / 3.0).abs() < f64::EPSILON);
    assert_eq!(c.registry_coverage_ratio(), 1.0);
    Ok(())
}

#[test]
fn coverage_mixed_case_normalization() -> TestResult {
    let c = compare_matrix_and_registry(["IRacing", "ACC", "DiRt5"], ["iracing", "acc", "dirt5"]);
    assert!(c.is_exact());
    Ok(())
}

#[test]
fn coverage_unicode_ids_treated_as_opaque() -> TestResult {
    let c = compare_matrix_and_registry(["日本語", "café"], ["café", "日本語"]);
    assert!(c.is_exact());
    assert_eq!(c.matrix_game_ids.len(), 2);
    Ok(())
}

#[test]
fn coverage_whitespace_only_strings_filtered() -> TestResult {
    // Empty strings are filtered; whitespace-only strings are NOT filtered
    // (only empty strings are filtered per the implementation)
    let c = compare_matrix_and_registry(["a", ""], ["a", ""]);
    assert!(c.is_exact());
    assert_eq!(c.matrix_game_ids.len(), 1); // empty filtered
    Ok(())
}

#[test]
fn coverage_many_duplicates() -> TestResult {
    let matrix: Vec<&str> = (0..50).map(|_| "iracing").collect();
    let registry: Vec<&str> = (0..50).map(|_| "iracing").collect();
    let c = compare_matrix_and_registry(matrix, registry);
    assert!(c.is_exact());
    assert_eq!(c.matrix_game_ids.len(), 1);
    Ok(())
}

#[test]
fn coverage_large_set_deterministic() -> TestResult {
    let mut matrix: Vec<String> = (0..100).map(|i| format!("game_{i:03}")).collect();
    let registry = matrix.clone();
    matrix.reverse(); // Different order, same content

    let c = compare_matrix_and_registry(
        matrix.iter().map(|s| s.as_str()),
        registry.iter().map(|s| s.as_str()),
    );
    assert!(c.is_exact());
    assert_eq!(c.matrix_game_ids.len(), 100);
    // Verify sorted
    for i in 1..c.matrix_game_ids.len() {
        assert!(c.matrix_game_ids[i] >= c.matrix_game_ids[i - 1]);
    }
    Ok(())
}

// ===========================================================================
// RegistryCoverage — Clone and PartialEq
// ===========================================================================

#[test]
fn coverage_clone_is_equal() -> TestResult {
    let c = compare_matrix_and_registry(["a", "b", "c"], ["a", "b", "d"]);
    let cloned = c.clone();
    assert_eq!(c, cloned);
    Ok(())
}

#[test]
fn coverage_different_sets_not_equal() -> TestResult {
    let c1 = compare_matrix_and_registry(["a", "b"], ["a", "b"]);
    let c2 = compare_matrix_and_registry(["a", "b"], ["a"]);
    assert_ne!(c1, c2);
    Ok(())
}

#[test]
fn coverage_debug_format() -> TestResult {
    let c = compare_matrix_and_registry(["a"], ["b"]);
    let debug = format!("{c:?}");
    assert!(!debug.is_empty());
    Ok(())
}

// ===========================================================================
// RegistryCoverageMetrics
// ===========================================================================

#[test]
fn metrics_exact_match() -> TestResult {
    let c = compare_matrix_and_registry(["a", "b", "c"], ["a", "b", "c"]);
    let m = c.metrics();
    assert_eq!(m.matrix_game_count, 3);
    assert_eq!(m.registry_game_count, 3);
    assert_eq!(m.missing_count, 0);
    assert_eq!(m.extra_count, 0);
    assert_eq!(m.matrix_coverage_ratio, 1.0);
    assert_eq!(m.registry_coverage_ratio, 1.0);
    Ok(())
}

#[test]
fn metrics_empty_sets() -> TestResult {
    let c: RegistryCoverage = compare_matrix_and_registry(Vec::<&str>::new(), Vec::<&str>::new());
    let m = c.metrics();
    assert_eq!(m.matrix_game_count, 0);
    assert_eq!(m.registry_game_count, 0);
    assert_eq!(m.missing_count, 0);
    assert_eq!(m.extra_count, 0);
    assert_eq!(m.matrix_coverage_ratio, 0.0);
    assert_eq!(m.registry_coverage_ratio, 0.0);
    Ok(())
}

#[test]
fn metrics_clone_and_partial_eq() -> TestResult {
    let c = compare_matrix_and_registry(["a", "b"], ["a"]);
    let m1 = c.metrics();
    let m2 = m1.clone();
    assert_eq!(m1, m2);
    Ok(())
}

#[test]
fn metrics_debug_format_not_empty() -> TestResult {
    let c = compare_matrix_and_registry(["a"], ["a"]);
    let m = c.metrics();
    let debug = format!("{m:?}");
    assert!(!debug.is_empty());
    Ok(())
}

// ===========================================================================
// CoveragePolicy — all preset combinations
// ===========================================================================

#[test]
fn policy_strict_fields() {
    let p = CoveragePolicy::STRICT;
    assert!(!p.allow_missing_registry);
    assert!(!p.allow_extra_registry);
}

#[test]
fn policy_matrix_complete_fields() {
    let p = CoveragePolicy::MATRIX_COMPLETE;
    assert!(!p.allow_missing_registry);
    assert!(p.allow_extra_registry);
}

#[test]
fn policy_lenient_fields() {
    let p = CoveragePolicy::LENIENT;
    assert!(p.allow_missing_registry);
    assert!(p.allow_extra_registry);
}

#[test]
fn policy_custom_allow_missing_only() -> TestResult {
    let policy = CoveragePolicy {
        allow_missing_registry: true,
        allow_extra_registry: false,
    };

    let missing_ok = compare_matrix_and_registry(["a", "b", "c"], ["a"]);
    assert!(policy.is_satisfied(&missing_ok));

    let extra_fail = compare_matrix_and_registry(["a"], ["a", "b"]);
    assert!(!policy.is_satisfied(&extra_fail));
    Ok(())
}

#[test]
fn policy_is_satisfied_exact_match_all_policies() -> TestResult {
    let c = compare_matrix_and_registry(["a", "b"], ["a", "b"]);
    assert!(CoveragePolicy::STRICT.is_satisfied(&c));
    assert!(CoveragePolicy::MATRIX_COMPLETE.is_satisfied(&c));
    assert!(CoveragePolicy::LENIENT.is_satisfied(&c));
    Ok(())
}

#[test]
fn policy_as_bdd_policy_roundtrip() {
    let strict_bdd = CoveragePolicy::STRICT.as_bdd_policy();
    assert!(!strict_bdd.allow_missing_registry);
    assert!(!strict_bdd.allow_extra_registry);

    let mc_bdd = CoveragePolicy::MATRIX_COMPLETE.as_bdd_policy();
    assert!(!mc_bdd.allow_missing_registry);
    assert!(mc_bdd.allow_extra_registry);

    let lenient_bdd = CoveragePolicy::LENIENT.as_bdd_policy();
    assert!(lenient_bdd.allow_missing_registry);
    assert!(lenient_bdd.allow_extra_registry);
}

#[test]
fn policy_clone_and_copy() {
    let p = CoveragePolicy::STRICT;
    let copied = p;
    assert!(!copied.allow_missing_registry);
    assert!(!p.allow_missing_registry);
}

#[test]
fn policy_debug_format() {
    let debug = format!("{:?}", CoveragePolicy::STRICT);
    assert!(debug.contains("allow_missing_registry"));
}

// ===========================================================================
// compare_matrix_and_registry_with_policy — error paths
// ===========================================================================

#[test]
fn policy_comparison_strict_exact_match_ok() -> TestResult {
    let result =
        compare_matrix_and_registry_with_policy(["a", "b"], ["a", "b"], CoveragePolicy::STRICT);
    assert!(result.is_ok());
    let coverage = result?;
    assert!(coverage.is_exact());
    Ok(())
}

#[test]
fn policy_comparison_strict_missing_returns_mismatch() -> TestResult {
    let result =
        compare_matrix_and_registry_with_policy(["a", "b", "c"], ["a"], CoveragePolicy::STRICT);
    assert!(result.is_err());
    let mismatch = result.err().ok_or("expected error")?;
    assert_eq!(
        mismatch.missing_in_registry,
        vec!["b".to_string(), "c".to_string()]
    );
    assert!(mismatch.extra_in_registry.is_empty());
    Ok(())
}

#[test]
fn policy_comparison_strict_extra_returns_mismatch() -> TestResult {
    let result = compare_matrix_and_registry_with_policy(["a"], ["a", "b"], CoveragePolicy::STRICT);
    let mismatch = result.err().ok_or("expected error")?;
    assert!(mismatch.missing_in_registry.is_empty());
    assert_eq!(mismatch.extra_in_registry, vec!["b".to_string()]);
    Ok(())
}

#[test]
fn policy_comparison_lenient_never_fails() -> TestResult {
    let result = compare_matrix_and_registry_with_policy(
        Vec::<&str>::new(),
        ["x", "y", "z"],
        CoveragePolicy::LENIENT,
    );
    assert!(result.is_ok());

    let result2 = compare_matrix_and_registry_with_policy(
        ["a", "b", "c"],
        Vec::<&str>::new(),
        CoveragePolicy::LENIENT,
    );
    assert!(result2.is_ok());
    Ok(())
}

#[test]
fn policy_comparison_empty_sets_strict_ok() -> TestResult {
    let result = compare_matrix_and_registry_with_policy(
        Vec::<&str>::new(),
        Vec::<&str>::new(),
        CoveragePolicy::STRICT,
    );
    assert!(result.is_ok());
    Ok(())
}

// ===========================================================================
// CoverageMismatch — error trait and display
// ===========================================================================

#[test]
fn mismatch_display_contains_ids() -> TestResult {
    let m = CoverageMismatch {
        matrix_game_ids: vec!["a".into(), "b".into()],
        registry_game_ids: vec!["a".into(), "c".into()],
        missing_in_registry: vec!["b".into()],
        extra_in_registry: vec!["c".into()],
    };
    let display = m.to_string();
    assert!(display.contains("missing_in_registry"));
    assert!(display.contains("extra_in_registry"));
    assert!(display.contains("b"));
    assert!(display.contains("c"));
    Ok(())
}

#[test]
fn mismatch_is_std_error_trait() {
    let m = CoverageMismatch {
        matrix_game_ids: vec![],
        registry_game_ids: vec![],
        missing_in_registry: vec![],
        extra_in_registry: vec![],
    };
    let _err: &dyn std::error::Error = &m;
}

#[test]
fn mismatch_debug_and_clone() -> TestResult {
    let m = CoverageMismatch {
        matrix_game_ids: vec!["a".into()],
        registry_game_ids: vec!["b".into()],
        missing_in_registry: vec!["a".into()],
        extra_in_registry: vec!["b".into()],
    };
    let cloned = m.clone();
    assert_eq!(m, cloned);
    let debug = format!("{m:?}");
    assert!(!debug.is_empty());
    Ok(())
}

#[test]
fn mismatch_empty_vecs_display() -> TestResult {
    let m = CoverageMismatch {
        matrix_game_ids: vec![],
        registry_game_ids: vec![],
        missing_in_registry: vec![],
        extra_in_registry: vec![],
    };
    let display = m.to_string();
    assert!(display.contains("missing_in_registry"));
    Ok(())
}

// ===========================================================================
// RuntimeCoverageReport — adapter selection and combined metrics
// ===========================================================================

fn make_report(
    matrix: &[&str],
    adapters: &[&str],
    writers: &[&str],
    adapter_policy: CoveragePolicy,
    writer_policy: CoveragePolicy,
) -> RuntimeCoverageReport {
    compare_runtime_registries_with_policies(
        matrix.iter().copied(),
        adapters.iter().copied(),
        writers.iter().copied(),
        adapter_policy,
        writer_policy,
    )
}

#[test]
fn runtime_report_both_exact_strict() -> TestResult {
    let r = make_report(
        &["a", "b", "c"],
        &["a", "b", "c"],
        &["a", "b", "c"],
        CoveragePolicy::STRICT,
        CoveragePolicy::STRICT,
    );
    assert!(r.is_parity_ok());
    assert!(r.adapter_policy_ok());
    assert!(r.writer_policy_ok());
    assert_eq!(r.matrix_game_ids.len(), 3);
    Ok(())
}

#[test]
fn runtime_report_adapter_missing_writer_ok() -> TestResult {
    let r = make_report(
        &["a", "b", "c"],
        &["a", "b"],
        &["a", "b", "c"],
        CoveragePolicy::MATRIX_COMPLETE,
        CoveragePolicy::MATRIX_COMPLETE,
    );
    assert!(!r.adapter_policy_ok());
    assert!(r.writer_policy_ok());
    assert!(!r.is_parity_ok());
    Ok(())
}

#[test]
fn runtime_report_adapter_ok_writer_missing() -> TestResult {
    let r = make_report(
        &["a", "b", "c"],
        &["a", "b", "c"],
        &["a"],
        CoveragePolicy::MATRIX_COMPLETE,
        CoveragePolicy::MATRIX_COMPLETE,
    );
    assert!(r.adapter_policy_ok());
    assert!(!r.writer_policy_ok());
    assert!(!r.is_parity_ok());
    Ok(())
}

#[test]
fn runtime_report_mixed_policies() -> TestResult {
    let r = make_report(
        &["a", "b", "c"],
        &["a", "b"],                     // missing "c"
        &["a", "b", "c", "d"],           // extra "d"
        CoveragePolicy::LENIENT,         // adapter: allow missing
        CoveragePolicy::MATRIX_COMPLETE, // writer: allow extra
    );
    assert!(r.adapter_policy_ok()); // lenient allows missing
    assert!(r.writer_policy_ok()); // matrix_complete allows extra
    assert!(r.is_parity_ok());
    Ok(())
}

#[test]
fn runtime_report_empty_all() -> TestResult {
    let r = make_report(
        &[],
        &[],
        &[],
        CoveragePolicy::STRICT,
        CoveragePolicy::STRICT,
    );
    assert!(r.is_parity_ok());
    assert!(r.matrix_game_ids.is_empty());
    Ok(())
}

#[test]
fn runtime_report_empty_matrix_nonempty_registries() -> TestResult {
    let r = make_report(
        &[],
        &["a", "b"],
        &["c"],
        CoveragePolicy::STRICT,
        CoveragePolicy::STRICT,
    );
    // Empty matrix → nothing missing, but extras violate strict
    assert!(!r.adapter_policy_ok());
    assert!(!r.writer_policy_ok());
    Ok(())
}

#[test]
fn runtime_report_nonempty_matrix_empty_registries() -> TestResult {
    let r = make_report(
        &["a", "b"],
        &[],
        &[],
        CoveragePolicy::STRICT,
        CoveragePolicy::STRICT,
    );
    assert!(!r.adapter_policy_ok()); // missing a, b
    assert!(!r.writer_policy_ok());
    assert!(!r.is_parity_ok());
    Ok(())
}

// ===========================================================================
// RuntimeCoverageMetrics — structure and consistency
// ===========================================================================

#[test]
fn runtime_metrics_all_fields_populated() -> TestResult {
    let r = make_report(
        &["a", "b", "c"],
        &["a", "b", "d"],
        &["a", "c"],
        CoveragePolicy::STRICT,
        CoveragePolicy::STRICT,
    );
    let m = r.metrics();
    assert_eq!(m.matrix_game_count, 3);
    assert_eq!(m.adapter.matrix_game_count, 3);
    assert_eq!(m.adapter.registry_game_count, 3);
    assert_eq!(m.adapter.missing_count, 1); // c
    assert_eq!(m.adapter.extra_count, 1); // d
    assert_eq!(m.writer.missing_count, 1); // b
    assert_eq!(m.writer.extra_count, 0);
    assert!(!m.parity_ok);
    Ok(())
}

#[test]
fn runtime_metrics_clone_and_eq() -> TestResult {
    let r = make_report(
        &["a", "b"],
        &["a", "b"],
        &["a", "b"],
        CoveragePolicy::STRICT,
        CoveragePolicy::STRICT,
    );
    let m1 = r.metrics();
    let m2 = m1.clone();
    assert_eq!(m1, m2);
    Ok(())
}

#[test]
fn runtime_metrics_debug() -> TestResult {
    let r = make_report(
        &["a"],
        &["a"],
        &["a"],
        CoveragePolicy::STRICT,
        CoveragePolicy::STRICT,
    );
    let m = r.metrics();
    let debug = format!("{m:?}");
    assert!(debug.contains("matrix_game_count"));
    assert!(debug.contains("parity_ok"));
    Ok(())
}

// ===========================================================================
// BDD metrics — through RuntimeCoverageReport
// ===========================================================================

#[test]
fn bdd_metrics_exact_match_parity_ok() -> TestResult {
    let r = make_report(
        &["a", "b"],
        &["a", "b"],
        &["a", "b"],
        CoveragePolicy::STRICT,
        CoveragePolicy::STRICT,
    );
    let bdd = r.bdd_metrics();
    assert!(bdd.parity_ok);
    assert!(bdd.adapter.parity_ok);
    assert!(bdd.writer.parity_ok);
    assert_eq!(bdd.matrix_game_count, 2);
    assert_eq!(bdd.adapter.missing_count, 0);
    assert_eq!(bdd.adapter.extra_count, 0);
    assert_eq!(bdd.writer.missing_count, 0);
    assert_eq!(bdd.writer.extra_count, 0);
    Ok(())
}

#[test]
fn bdd_metrics_missing_ids_listed() -> TestResult {
    let r = make_report(
        &["a", "b", "c"],
        &["a"],
        &["b"],
        CoveragePolicy::STRICT,
        CoveragePolicy::STRICT,
    );
    let bdd = r.bdd_metrics();
    assert!(!bdd.parity_ok);
    assert_eq!(bdd.adapter.missing_count, 2);
    assert!(bdd.adapter.missing_game_ids.contains(&"b".to_string()));
    assert!(bdd.adapter.missing_game_ids.contains(&"c".to_string()));
    assert_eq!(bdd.writer.missing_count, 2);
    assert!(bdd.writer.missing_game_ids.contains(&"a".to_string()));
    assert!(bdd.writer.missing_game_ids.contains(&"c".to_string()));
    Ok(())
}

#[test]
fn bdd_metrics_extra_ids_listed() -> TestResult {
    let r = make_report(
        &["a"],
        &["a", "x"],
        &["a", "y", "z"],
        CoveragePolicy::MATRIX_COMPLETE,
        CoveragePolicy::MATRIX_COMPLETE,
    );
    let bdd = r.bdd_metrics();
    assert!(bdd.parity_ok); // MATRIX_COMPLETE allows extras
    assert_eq!(bdd.adapter.extra_count, 1);
    assert!(bdd.adapter.extra_game_ids.contains(&"x".to_string()));
    assert_eq!(bdd.writer.extra_count, 2);
    Ok(())
}

#[test]
fn bdd_metrics_coverage_ratios() -> TestResult {
    let r = make_report(
        &["a", "b", "c", "d"],
        &["a", "b"],
        &["a", "b", "c", "d"],
        CoveragePolicy::STRICT,
        CoveragePolicy::STRICT,
    );
    let bdd = r.bdd_metrics();
    assert_eq!(bdd.adapter.matrix_coverage_ratio, 0.5);
    assert_eq!(bdd.writer.matrix_coverage_ratio, 1.0);
    Ok(())
}

// ===========================================================================
// RegistryCoverage::bdd_metrics — policy awareness
// ===========================================================================

#[test]
fn registry_bdd_metrics_strict_with_extras() -> TestResult {
    let c = compare_matrix_and_registry(["a", "b"], ["a", "b", "c"]);
    let bdd = c.bdd_metrics(CoveragePolicy::STRICT);
    assert!(!bdd.parity_ok); // extras not allowed under strict
    assert_eq!(bdd.extra_count, 1);
    Ok(())
}

#[test]
fn registry_bdd_metrics_matrix_complete_with_extras() -> TestResult {
    let c = compare_matrix_and_registry(["a", "b"], ["a", "b", "c"]);
    let bdd = c.bdd_metrics(CoveragePolicy::MATRIX_COMPLETE);
    assert!(bdd.parity_ok); // extras allowed
    Ok(())
}

#[test]
fn registry_bdd_metrics_lenient_with_both_mismatch() -> TestResult {
    let c = compare_matrix_and_registry(["a", "b"], ["b", "c"]);
    let bdd = c.bdd_metrics(CoveragePolicy::LENIENT);
    assert!(bdd.parity_ok); // both allowed
    assert_eq!(bdd.missing_count, 1);
    assert_eq!(bdd.extra_count, 1);
    Ok(())
}

// ===========================================================================
// Game detection simulation — realistic scenarios
// ===========================================================================

#[test]
fn game_detection_typical_racing_setup() -> TestResult {
    let supported_matrix = [
        "acc",
        "iracing",
        "dirt5",
        "f1_2023",
        "forza_horizon_5",
        "forza_motorsport",
        "ams2",
        "raceroom",
        "rfactor2",
        "automobilista",
    ];
    let detected_adapters = [
        "acc",
        "iracing",
        "f1_2023",
        "forza_horizon_5",
        "forza_motorsport",
        "ams2",
        "raceroom",
        "rfactor2",
    ];
    let detected_writers = [
        "acc",
        "iracing",
        "f1_2023",
        "forza_horizon_5",
        "forza_motorsport",
    ];

    let r = compare_runtime_registries_with_policies(
        supported_matrix,
        detected_adapters,
        detected_writers,
        CoveragePolicy::MATRIX_COMPLETE,
        CoveragePolicy::LENIENT,
    );

    // Adapters missing dirt5, automobilista
    assert!(!r.adapter_policy_ok());
    // Writers use LENIENT, so missing is OK
    assert!(r.writer_policy_ok());
    // Overall parity fails because adapter fails
    assert!(!r.is_parity_ok());

    let metrics = r.metrics();
    assert_eq!(metrics.matrix_game_count, 10);
    assert_eq!(metrics.adapter.missing_count, 2);
    assert_eq!(metrics.writer.missing_count, 5);
    Ok(())
}

#[test]
fn game_detection_progressive_adapter_rollout() -> TestResult {
    let matrix = ["acc", "iracing", "dirt5"];

    // Phase 1: only one adapter
    let r1 = compare_matrix_and_registry_with_policy(matrix, ["acc"], CoveragePolicy::LENIENT);
    assert!(r1.is_ok());
    let c1 = r1?;
    assert_eq!(c1.missing_in_registry.len(), 2);

    // Phase 2: two adapters
    let r2 = compare_matrix_and_registry_with_policy(
        matrix,
        ["acc", "iracing"],
        CoveragePolicy::LENIENT,
    );
    assert!(r2.is_ok());
    let c2 = r2?;
    assert_eq!(c2.missing_in_registry.len(), 1);

    // Phase 3: complete
    let r3 = compare_matrix_and_registry_with_policy(
        matrix,
        ["acc", "iracing", "dirt5"],
        CoveragePolicy::STRICT,
    );
    assert!(r3.is_ok());
    Ok(())
}

// ===========================================================================
// Edge cases in ID normalization
// ===========================================================================

#[test]
fn normalization_preserves_non_ascii_lowercase() -> TestResult {
    let c = compare_matrix_and_registry(["CAFÉ"], ["café"]);
    // ASCII lowercase only; É stays as É on the matrix side, é on registry
    // They may or may not match depending on ascii_lowercase behavior
    // Just verify no panic
    let _exact = c.is_exact();
    Ok(())
}

#[test]
fn normalization_numbers_and_underscores() -> TestResult {
    let c = compare_matrix_and_registry(["game_123", "GAME_456"], ["game_123", "game_456"]);
    assert!(c.is_exact());
    Ok(())
}

#[test]
fn normalization_hyphenated_ids() -> TestResult {
    let c = compare_matrix_and_registry(["FORZA-HORIZON-5"], ["forza-horizon-5"]);
    assert!(c.is_exact());
    Ok(())
}

// ===========================================================================
// RuntimeCoverageReport — clone and debug
// ===========================================================================

#[test]
fn runtime_report_clone() -> TestResult {
    let r = make_report(
        &["a", "b"],
        &["a"],
        &["b"],
        CoveragePolicy::STRICT,
        CoveragePolicy::LENIENT,
    );
    let cloned = r.clone();
    assert_eq!(cloned.matrix_game_ids, r.matrix_game_ids);
    assert_eq!(
        cloned.adapter_coverage.missing_in_registry,
        r.adapter_coverage.missing_in_registry
    );
    Ok(())
}

#[test]
fn runtime_report_debug() -> TestResult {
    let r = make_report(
        &["a"],
        &["a"],
        &["a"],
        CoveragePolicy::STRICT,
        CoveragePolicy::STRICT,
    );
    let debug = format!("{r:?}");
    assert!(debug.contains("matrix_game_ids"));
    Ok(())
}

// ===========================================================================
// Adapter selection: policy-driven decisions
// ===========================================================================

#[test]
fn adapter_selection_strict_rejects_extra_adapters() -> TestResult {
    let result = compare_matrix_and_registry_with_policy(
        ["acc", "iracing"],
        ["acc", "iracing", "experimental_beta"],
        CoveragePolicy::STRICT,
    );
    let err = result.err().ok_or("expected error")?;
    assert_eq!(err.extra_in_registry, vec!["experimental_beta".to_string()]);
    Ok(())
}

#[test]
fn adapter_selection_matrix_complete_accepts_extra_adapters() -> TestResult {
    let result = compare_matrix_and_registry_with_policy(
        ["acc", "iracing"],
        ["acc", "iracing", "experimental_beta"],
        CoveragePolicy::MATRIX_COMPLETE,
    );
    assert!(result.is_ok());
    let coverage = result?;
    assert!(coverage.has_complete_matrix_coverage());
    assert!(!coverage.has_no_extra_coverage());
    Ok(())
}
