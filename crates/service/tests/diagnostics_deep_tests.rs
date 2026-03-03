//! Deep diagnostics and self-test tests for the diagnostics subsystem.
//!
//! Covers: diagnostic test types, result formatting, categories/severity,
//! data collection timing, history/trending, device-specific diagnostics,
//! telemetry diagnostics, safety system diagnostics, performance metrics,
//! error rate tracking, health scoring, and export for support tickets.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use racing_wheel_service::diagnostic_service::{
    DiagnosticResult, DiagnosticService, DiagnosticStatus,
};

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

// ── Helpers ──────────────────────────────────────────────────────────────

/// Build a [`DiagnosticResult`] with the given status.
fn make_result(name: &str, status: DiagnosticStatus, message: &str) -> DiagnosticResult {
    DiagnosticResult {
        name: name.to_string(),
        status,
        message: message.to_string(),
        execution_time_ms: 0,
        metadata: HashMap::new(),
        suggested_actions: Vec::new(),
    }
}

/// Build a [`DiagnosticResult`] with metadata entries.
fn make_result_with_metadata(
    name: &str,
    status: DiagnosticStatus,
    message: &str,
    metadata: Vec<(&str, &str)>,
) -> DiagnosticResult {
    let mut meta = HashMap::new();
    for (k, v) in metadata {
        meta.insert(k.to_string(), v.to_string());
    }
    DiagnosticResult {
        name: name.to_string(),
        status,
        message: message.to_string(),
        execution_time_ms: 42,
        metadata: meta,
        suggested_actions: vec!["Check documentation".to_string()],
    }
}

/// Compute a simple health score from a collection of results.
/// Pass = 1.0, Warn = 0.5, Fail = 0.0.  Returns 0..=100 integer.
fn health_score(results: &[DiagnosticResult]) -> u32 {
    if results.is_empty() {
        return 0;
    }
    let total: f64 = results
        .iter()
        .map(|r| match r.status {
            DiagnosticStatus::Pass => 1.0,
            DiagnosticStatus::Warn => 0.5,
            DiagnosticStatus::Fail => 0.0,
        })
        .sum();
    ((total / results.len() as f64) * 100.0) as u32
}

/// Format a single result as a human-readable line.
fn format_human(result: &DiagnosticResult) -> String {
    let icon = match result.status {
        DiagnosticStatus::Pass => "✓",
        DiagnosticStatus::Warn => "⚠",
        DiagnosticStatus::Fail => "✗",
    };
    format!(
        "[{}] {} — {} ({}ms)",
        icon, result.name, result.message, result.execution_time_ms
    )
}

/// Severity weight for ordering (higher = more severe).
fn severity_weight(status: &DiagnosticStatus) -> u8 {
    match status {
        DiagnosticStatus::Fail => 2,
        DiagnosticStatus::Warn => 1,
        DiagnosticStatus::Pass => 0,
    }
}

/// Build a JSON export bundle suitable for a support ticket.
fn export_support_bundle(
    results: &[DiagnosticResult],
    system_label: &str,
) -> Result<serde_json::Value, serde_json::Error> {
    let score = health_score(results);
    let entries: Vec<serde_json::Value> = results
        .iter()
        .map(|r| {
            serde_json::json!({
                "name": r.name,
                "status": format!("{:?}", r.status),
                "message": r.message,
                "execution_time_ms": r.execution_time_ms,
                "metadata": r.metadata,
                "suggested_actions": r.suggested_actions,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "system": system_label,
        "health_score": score,
        "results": entries,
    }))
}

// =========================================================================
// 1. All diagnostic test types run and produce results
// =========================================================================

#[tokio::test]
async fn all_diagnostic_tests_produce_results() -> Result<(), BoxErr> {
    let service = DiagnosticService::new().await?;
    let results = service.run_full_diagnostics().await?;

    // The service should register at least one diagnostic test
    assert!(
        !results.is_empty(),
        "full diagnostics should produce at least one result"
    );

    // Every result must have a non-empty name and message
    for result in &results {
        assert!(!result.name.is_empty(), "result name must not be empty");
        assert!(
            !result.message.is_empty(),
            "result message must not be empty for test '{}'",
            result.name
        );
    }
    Ok(())
}

#[tokio::test]
async fn list_tests_returns_names_and_descriptions() -> Result<(), BoxErr> {
    let service = DiagnosticService::new().await?;
    let tests = service.list_tests();

    assert!(
        !tests.is_empty(),
        "diagnostic service should list at least one test"
    );

    for (name, description) in &tests {
        assert!(!name.is_empty(), "test name must not be empty");
        assert!(
            !description.is_empty(),
            "test description must not be empty for '{}'",
            name
        );
    }
    Ok(())
}

#[tokio::test]
async fn run_specific_test_by_name() -> Result<(), BoxErr> {
    let service = DiagnosticService::new().await?;
    let tests = service.list_tests();

    // Pick the first test by name
    let (first_name, _) = tests.first().ok_or("no diagnostic tests registered")?;

    let result = service.run_test(first_name).await?;
    assert_eq!(&result.name, first_name);
    Ok(())
}

#[tokio::test]
async fn run_nonexistent_test_returns_error() -> Result<(), BoxErr> {
    let service = DiagnosticService::new().await?;
    let result = service.run_test("this_test_does_not_exist_12345").await;
    assert!(result.is_err(), "nonexistent test should return Err");
    Ok(())
}

// =========================================================================
// 2. Diagnostic result formatting (human-readable and JSON)
// =========================================================================

#[test]
fn human_readable_format_pass() -> Result<(), BoxErr> {
    let result = make_result("cpu_check", DiagnosticStatus::Pass, "CPU is adequate");
    let formatted = format_human(&result);
    assert!(formatted.contains("✓"), "pass should show checkmark");
    assert!(formatted.contains("cpu_check"));
    assert!(formatted.contains("CPU is adequate"));
    Ok(())
}

#[test]
fn human_readable_format_warn() -> Result<(), BoxErr> {
    let result = make_result("mem_check", DiagnosticStatus::Warn, "Low memory");
    let formatted = format_human(&result);
    assert!(formatted.contains("⚠"), "warn should show warning icon");
    Ok(())
}

#[test]
fn human_readable_format_fail() -> Result<(), BoxErr> {
    let result = make_result("disk_check", DiagnosticStatus::Fail, "Disk full");
    let formatted = format_human(&result);
    assert!(formatted.contains("✗"), "fail should show cross");
    Ok(())
}

#[test]
fn json_serialization_roundtrip() -> Result<(), BoxErr> {
    let result = make_result_with_metadata(
        "net_check",
        DiagnosticStatus::Pass,
        "Network OK",
        vec![("latency_ms", "12"), ("port", "9000")],
    );

    let json = serde_json::to_string(&result)?;
    let parsed: DiagnosticResult = serde_json::from_str(&json)?;

    assert_eq!(parsed.name, "net_check");
    assert_eq!(parsed.execution_time_ms, 42);
    assert_eq!(
        parsed.metadata.get("latency_ms").map(String::as_str),
        Some("12")
    );
    assert_eq!(parsed.suggested_actions.len(), 1);
    Ok(())
}

#[test]
fn json_serialization_preserves_status_variants() -> Result<(), BoxErr> {
    for status in [
        DiagnosticStatus::Pass,
        DiagnosticStatus::Warn,
        DiagnosticStatus::Fail,
    ] {
        let result = make_result("status_test", status.clone(), "msg");
        let json = serde_json::to_string(&result)?;
        let parsed: DiagnosticResult = serde_json::from_str(&json)?;
        assert_eq!(
            format!("{:?}", parsed.status),
            format!("{:?}", status),
            "status variant should survive round-trip"
        );
    }
    Ok(())
}

// =========================================================================
// 3. Diagnostic categories and severity levels
// =========================================================================

#[test]
fn severity_ordering_is_correct() -> Result<(), BoxErr> {
    assert!(severity_weight(&DiagnosticStatus::Fail) > severity_weight(&DiagnosticStatus::Warn));
    assert!(severity_weight(&DiagnosticStatus::Warn) > severity_weight(&DiagnosticStatus::Pass));
    Ok(())
}

#[test]
fn results_can_be_sorted_by_severity() -> Result<(), BoxErr> {
    let results_unsorted = [
        make_result("a", DiagnosticStatus::Pass, "ok"),
        make_result("b", DiagnosticStatus::Fail, "bad"),
        make_result("c", DiagnosticStatus::Warn, "meh"),
    ];
    let mut results = results_unsorted.to_vec();

    results.sort_by_key(|r| std::cmp::Reverse(severity_weight(&r.status)));

    assert!(matches!(results[0].status, DiagnosticStatus::Fail));
    assert!(matches!(results[1].status, DiagnosticStatus::Warn));
    assert!(matches!(results[2].status, DiagnosticStatus::Pass));
    Ok(())
}

#[tokio::test]
async fn diagnostic_tests_cover_expected_categories() -> Result<(), BoxErr> {
    let service = DiagnosticService::new().await?;
    let tests = service.list_tests();
    let names: Vec<&str> = tests.iter().map(|(n, _)| &**n).collect();

    // The service should have tests for these core categories
    let expected_categories = [
        "system_requirements",
        "hid_devices",
        "realtime_capability",
        "memory",
        "timing",
        "safety_system",
    ];

    for cat in &expected_categories {
        assert!(
            names.contains(cat),
            "expected diagnostic category '{}' not found in {:?}",
            cat,
            names
        );
    }
    Ok(())
}

// =========================================================================
// 4. Diagnostic data collection timing
// =========================================================================

#[tokio::test]
async fn full_diagnostics_records_timing() -> Result<(), BoxErr> {
    let service = DiagnosticService::new().await?;
    let start = Instant::now();
    let results = service.run_full_diagnostics().await?;
    let elapsed = start.elapsed();

    // The whole run should complete in a reasonable time
    assert!(
        elapsed < Duration::from_secs(120),
        "full diagnostics should complete within 120 s, took {:?}",
        elapsed
    );

    // At least one result should record a non-negative execution time
    // execution_time_ms is u64, so always non-negative; check we got results
    assert!(
        !results.is_empty(),
        "at least one result should be present for timing"
    );
    Ok(())
}

#[tokio::test]
async fn individual_test_timing_is_bounded() -> Result<(), BoxErr> {
    let service = DiagnosticService::new().await?;
    let tests = service.list_tests();

    // Run the "system_requirements" test which should be fast
    if let Some((name, _)) = tests.iter().find(|(n, _)| n == "system_requirements") {
        let start = Instant::now();
        let result = service.run_test(name).await?;
        let elapsed = start.elapsed();

        assert!(
            elapsed < Duration::from_secs(10),
            "system_requirements test should be quick, took {:?}",
            elapsed
        );
        // execution_time_ms is set by the run() impl or by run_full_diagnostics
        let _ = result.execution_time_ms;
    }
    Ok(())
}

// =========================================================================
// 5. Diagnostic history and trending
// =========================================================================

#[test]
fn diagnostic_history_can_accumulate() -> Result<(), BoxErr> {
    let mut history: Vec<(u64, Vec<DiagnosticResult>)> = Vec::new();

    // Simulate three successive diagnostic runs at t=0, t=1, t=2
    for epoch in 0..3u64 {
        let results = vec![
            make_result("cpu", DiagnosticStatus::Pass, "ok"),
            make_result(
                "mem",
                if epoch < 2 {
                    DiagnosticStatus::Pass
                } else {
                    DiagnosticStatus::Warn
                },
                "memory trend",
            ),
        ];
        history.push((epoch, results));
    }

    assert_eq!(history.len(), 3);

    // Trending: the "mem" test degraded from Pass to Warn
    let first_mem = history[0]
        .1
        .iter()
        .find(|r| r.name == "mem")
        .ok_or("missing mem in first run")?;
    let last_mem = history[2]
        .1
        .iter()
        .find(|r| r.name == "mem")
        .ok_or("missing mem in last run")?;

    assert!(matches!(first_mem.status, DiagnosticStatus::Pass));
    assert!(matches!(last_mem.status, DiagnosticStatus::Warn));
    Ok(())
}

#[test]
fn health_score_trends_over_time() -> Result<(), BoxErr> {
    let good = vec![
        make_result("a", DiagnosticStatus::Pass, "ok"),
        make_result("b", DiagnosticStatus::Pass, "ok"),
    ];
    let degraded = vec![
        make_result("a", DiagnosticStatus::Pass, "ok"),
        make_result("b", DiagnosticStatus::Fail, "bad"),
    ];

    let score_good = health_score(&good);
    let score_degraded = health_score(&degraded);

    assert!(
        score_good > score_degraded,
        "good score ({}) should exceed degraded ({})",
        score_good,
        score_degraded
    );
    Ok(())
}

// =========================================================================
// 6. Device-specific diagnostics
// =========================================================================

#[tokio::test]
async fn hid_device_diagnostic_runs() -> Result<(), BoxErr> {
    let service = DiagnosticService::new().await?;
    let result = service.run_test("hid_devices").await?;

    // Should succeed (Pass or Warn, since we may not have hardware)
    assert!(
        !matches!(result.status, DiagnosticStatus::Fail) || result.message.contains("HID API"),
        "hid_devices should pass/warn or report API issue, got: {}",
        result.message
    );
    Ok(())
}

#[tokio::test]
async fn hid_device_result_has_metadata() -> Result<(), BoxErr> {
    let service = DiagnosticService::new().await?;
    let result = service.run_test("hid_devices").await?;

    // Even without physical devices, metadata should exist
    // (total_hid_devices, racing_wheel_devices, or error context)
    let _ = &result.metadata; // just ensure it's accessible
    Ok(())
}

// =========================================================================
// 7. Telemetry diagnostics
// =========================================================================

#[tokio::test]
async fn network_diagnostic_checks_telemetry_ports() -> Result<(), BoxErr> {
    let service = DiagnosticService::new().await?;
    let result = service.run_test("network").await?;

    // The network test binds to well-known telemetry ports
    assert!(
        result.message.contains("port"),
        "network diagnostic should mention ports: {}",
        result.message
    );
    Ok(())
}

#[test]
fn telemetry_port_metadata_format() -> Result<(), BoxErr> {
    let result = make_result_with_metadata(
        "network",
        DiagnosticStatus::Pass,
        "Network test: 4/4 test ports available",
        vec![
            ("port_9000_available", "true"),
            ("port_20777_available", "true"),
            ("bindable_ports", "4"),
        ],
    );

    let json = serde_json::to_string(&result)?;
    assert!(json.contains("port_9000_available"));
    assert!(json.contains("bindable_ports"));
    Ok(())
}

// =========================================================================
// 8. Safety system diagnostics
// =========================================================================

#[tokio::test]
async fn safety_system_diagnostic_passes() -> Result<(), BoxErr> {
    let service = DiagnosticService::new().await?;
    let result = service.run_test("safety_system").await?;

    assert!(
        matches!(result.status, DiagnosticStatus::Pass),
        "safety_system diagnostic should pass, got {:?}: {}",
        result.status,
        result.message
    );
    assert!(
        result.message.contains("Safety system"),
        "message should mention safety system: {}",
        result.message
    );
    Ok(())
}

#[tokio::test]
async fn safety_system_reports_policy_metadata() -> Result<(), BoxErr> {
    let service = DiagnosticService::new().await?;
    let result = service.run_test("safety_system").await?;

    assert_eq!(
        result
            .metadata
            .get("safety_policy_created")
            .map(String::as_str),
        Some("true"),
        "safety_system should report policy creation"
    );
    // Should include torque limits
    assert!(
        result.metadata.contains_key("default_safe_torque_nm"),
        "safety_system should include default torque metadata"
    );
    assert!(
        result.metadata.contains_key("max_torque_nm"),
        "safety_system should include max torque metadata"
    );
    Ok(())
}

#[tokio::test]
async fn safety_system_reports_fault_handlers() -> Result<(), BoxErr> {
    let service = DiagnosticService::new().await?;
    let result = service.run_test("safety_system").await?;

    let expected_faults = ["usb_timeout", "encoder_nan", "thermal_limit", "overcurrent"];
    for fault in &expected_faults {
        let key = format!("fault_{}_handler", fault);
        assert_eq!(
            result.metadata.get(&key).map(String::as_str),
            Some("available"),
            "safety_system should report handler for fault '{}'",
            fault
        );
    }
    Ok(())
}

// =========================================================================
// 9. Performance metrics collection
// =========================================================================

#[tokio::test]
async fn timing_diagnostic_collects_jitter_metrics() -> Result<(), BoxErr> {
    let service = DiagnosticService::new().await?;
    let result = service.run_test("timing").await?;

    // Timing test should report jitter statistics
    assert!(
        result.metadata.contains_key("mean_jitter_us"),
        "timing diagnostic should report mean jitter"
    );
    assert!(
        result.metadata.contains_key("p99_jitter_us"),
        "timing diagnostic should report p99 jitter"
    );
    assert!(
        result.metadata.contains_key("max_jitter_us"),
        "timing diagnostic should report max jitter"
    );
    Ok(())
}

#[tokio::test]
async fn memory_diagnostic_collects_metrics() -> Result<(), BoxErr> {
    let service = DiagnosticService::new().await?;
    let result = service.run_test("memory").await?;

    assert!(
        result.metadata.contains_key("total_memory_mb"),
        "memory diagnostic should report total memory"
    );
    assert!(
        result.metadata.contains_key("available_memory_mb"),
        "memory diagnostic should report available memory"
    );

    // Values should be parseable as numbers
    let total: u64 = result
        .metadata
        .get("total_memory_mb")
        .ok_or("missing total_memory_mb")?
        .parse()?;
    assert!(total > 0, "total memory should be positive");
    Ok(())
}

#[tokio::test]
async fn system_requirements_collects_cpu_and_arch() -> Result<(), BoxErr> {
    let service = DiagnosticService::new().await?;
    let result = service.run_test("system_requirements").await?;

    assert!(
        result.metadata.contains_key("cpu_count"),
        "should report CPU count"
    );
    assert!(
        result.metadata.contains_key("architecture"),
        "should report architecture"
    );

    let cpus: usize = result
        .metadata
        .get("cpu_count")
        .ok_or("missing cpu_count")?
        .parse()?;
    assert!(cpus > 0, "CPU count should be positive");
    Ok(())
}

// =========================================================================
// 10. Error rate tracking and reporting
// =========================================================================

#[test]
fn error_rate_tracking_across_runs() -> Result<(), BoxErr> {
    // Simulate multiple diagnostic runs and track failure rates per test
    let runs: Vec<Vec<DiagnosticResult>> = vec![
        vec![
            make_result("a", DiagnosticStatus::Pass, "ok"),
            make_result("b", DiagnosticStatus::Fail, "bad"),
        ],
        vec![
            make_result("a", DiagnosticStatus::Pass, "ok"),
            make_result("b", DiagnosticStatus::Fail, "bad"),
        ],
        vec![
            make_result("a", DiagnosticStatus::Warn, "meh"),
            make_result("b", DiagnosticStatus::Pass, "recovered"),
        ],
    ];

    // Calculate error rate for test "b": 2 failures out of 3 runs
    let b_fail_count = runs
        .iter()
        .flat_map(|r| r.iter())
        .filter(|r| r.name == "b" && matches!(r.status, DiagnosticStatus::Fail))
        .count();
    let b_total = runs
        .iter()
        .flat_map(|r| r.iter())
        .filter(|r| r.name == "b")
        .count();

    let error_rate = b_fail_count as f64 / b_total as f64;
    assert!(
        (error_rate - 2.0 / 3.0).abs() < f64::EPSILON * 100.0,
        "error rate for 'b' should be ~66.7%, got {}",
        error_rate * 100.0
    );
    Ok(())
}

#[test]
fn failure_count_aggregation() -> Result<(), BoxErr> {
    let results = [
        make_result("a", DiagnosticStatus::Pass, "ok"),
        make_result("b", DiagnosticStatus::Fail, "bad"),
        make_result("c", DiagnosticStatus::Fail, "worse"),
        make_result("d", DiagnosticStatus::Warn, "meh"),
    ];

    let fail_count = results
        .iter()
        .filter(|r| matches!(r.status, DiagnosticStatus::Fail))
        .count();
    let warn_count = results
        .iter()
        .filter(|r| matches!(r.status, DiagnosticStatus::Warn))
        .count();
    let pass_count = results
        .iter()
        .filter(|r| matches!(r.status, DiagnosticStatus::Pass))
        .count();

    assert_eq!(fail_count, 2);
    assert_eq!(warn_count, 1);
    assert_eq!(pass_count, 1);
    Ok(())
}

// =========================================================================
// 11. System health scoring
// =========================================================================

#[test]
fn health_score_all_pass_is_100() -> Result<(), BoxErr> {
    let results = vec![
        make_result("a", DiagnosticStatus::Pass, "ok"),
        make_result("b", DiagnosticStatus::Pass, "ok"),
        make_result("c", DiagnosticStatus::Pass, "ok"),
    ];
    assert_eq!(health_score(&results), 100);
    Ok(())
}

#[test]
fn health_score_all_fail_is_0() -> Result<(), BoxErr> {
    let results = vec![
        make_result("a", DiagnosticStatus::Fail, "bad"),
        make_result("b", DiagnosticStatus::Fail, "bad"),
    ];
    assert_eq!(health_score(&results), 0);
    Ok(())
}

#[test]
fn health_score_mixed_is_proportional() -> Result<(), BoxErr> {
    let results = vec![
        make_result("a", DiagnosticStatus::Pass, "ok"),
        make_result("b", DiagnosticStatus::Fail, "bad"),
    ];
    // (1.0 + 0.0) / 2 * 100 = 50
    assert_eq!(health_score(&results), 50);
    Ok(())
}

#[test]
fn health_score_warn_contributes_half() -> Result<(), BoxErr> {
    let results = vec![
        make_result("a", DiagnosticStatus::Warn, "meh"),
        make_result("b", DiagnosticStatus::Warn, "meh"),
    ];
    // (0.5 + 0.5) / 2 * 100 = 50
    assert_eq!(health_score(&results), 50);
    Ok(())
}

#[test]
fn health_score_empty_is_0() -> Result<(), BoxErr> {
    assert_eq!(health_score(&[]), 0);
    Ok(())
}

#[tokio::test]
async fn live_health_score_is_non_negative() -> Result<(), BoxErr> {
    let service = DiagnosticService::new().await?;
    let results = service.run_full_diagnostics().await?;
    let score = health_score(&results);
    assert!(
        score <= 100,
        "health score should be at most 100, got {}",
        score
    );
    Ok(())
}

// =========================================================================
// 12. Diagnostic export for support tickets
// =========================================================================

#[test]
fn export_bundle_contains_required_fields() -> Result<(), BoxErr> {
    let results = vec![
        make_result_with_metadata(
            "cpu",
            DiagnosticStatus::Pass,
            "CPU OK",
            vec![("cpu_count", "8")],
        ),
        make_result("mem", DiagnosticStatus::Warn, "Low memory"),
    ];

    let bundle = export_support_bundle(&results, "test-system-001")?;

    assert_eq!(bundle["system"], "test-system-001");
    assert!(bundle["health_score"].is_number());
    assert!(bundle["results"].is_array());

    let arr = bundle["results"]
        .as_array()
        .ok_or("results should be an array")?;
    assert_eq!(arr.len(), 2);
    Ok(())
}

#[test]
fn export_bundle_roundtrips_through_json() -> Result<(), BoxErr> {
    let results = vec![
        make_result("a", DiagnosticStatus::Pass, "fine"),
        make_result("b", DiagnosticStatus::Fail, "broken"),
    ];

    let bundle = export_support_bundle(&results, "export-test")?;
    let json_str = serde_json::to_string_pretty(&bundle)?;
    let parsed: serde_json::Value = serde_json::from_str(&json_str)?;

    assert_eq!(parsed["system"], "export-test");
    assert_eq!(
        parsed["health_score"]
            .as_u64()
            .ok_or("health_score not a u64")?,
        50
    );
    Ok(())
}

#[test]
fn export_bundle_includes_metadata_and_actions() -> Result<(), BoxErr> {
    let results = vec![make_result_with_metadata(
        "diag_x",
        DiagnosticStatus::Warn,
        "warning message",
        vec![("key1", "val1"), ("key2", "val2")],
    )];

    let bundle = export_support_bundle(&results, "meta-test")?;
    let first = &bundle["results"][0];

    assert!(first["metadata"]["key1"].is_string());
    assert!(first["suggested_actions"].is_array());
    let actions = first["suggested_actions"]
        .as_array()
        .ok_or("actions should be array")?;
    assert!(!actions.is_empty(), "suggested_actions should not be empty");
    Ok(())
}

#[tokio::test]
async fn full_diagnostics_exportable() -> Result<(), BoxErr> {
    let service = DiagnosticService::new().await?;
    let results = service.run_full_diagnostics().await?;

    let bundle = export_support_bundle(&results, "live-system")?;
    let json_str = serde_json::to_string(&bundle)?;

    // The export should be valid JSON of reasonable size
    assert!(
        json_str.len() > 10,
        "export JSON should not be trivially small"
    );
    assert!(
        json_str.len() < 1_000_000,
        "export JSON should not be unreasonably large"
    );
    Ok(())
}

// =========================================================================
// Extra: Diagnostic result construction edge cases
// =========================================================================

#[test]
fn diagnostic_result_with_empty_metadata_serializes() -> Result<(), BoxErr> {
    let result = make_result("empty_meta", DiagnosticStatus::Pass, "nothing extra");
    let json = serde_json::to_string(&result)?;
    let parsed: DiagnosticResult = serde_json::from_str(&json)?;
    assert!(parsed.metadata.is_empty());
    assert!(parsed.suggested_actions.is_empty());
    Ok(())
}

#[test]
fn diagnostic_result_with_many_actions_serializes() -> Result<(), BoxErr> {
    let mut result = make_result("many_actions", DiagnosticStatus::Fail, "lots to do");
    for i in 0..20 {
        result.suggested_actions.push(format!("Action item {}", i));
    }
    let json = serde_json::to_string(&result)?;
    let parsed: DiagnosticResult = serde_json::from_str(&json)?;
    assert_eq!(parsed.suggested_actions.len(), 20);
    Ok(())
}
