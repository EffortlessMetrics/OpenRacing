//! Main integration test runner
//!
//! This module contains the primary integration tests that can be run with `cargo test`

use anyhow::Result;
use racing_wheel_integration_tests::*;
use std::time::Duration;

#[tokio::test]
async fn test_user_journey_uj01_first_run() -> Result<()> {
    init_test_environment()?;

    let result = user_journeys::test_uj01_first_run().await?;

    if !result.passed {
        panic!("UJ-01 test failed: {:?}", result.errors);
    }

    assert!(result.metrics.meets_performance_gates());
    Ok(())
}

#[tokio::test]
async fn test_user_journey_uj02_profile_switching() -> Result<()> {
    init_test_environment()?;

    let result = user_journeys::test_uj02_profile_switching().await?;

    if !result.passed {
        panic!("UJ-02 test failed: {:?}", result.errors);
    }

    Ok(())
}

#[tokio::test]
async fn test_user_journey_uj03_fault_recovery() -> Result<()> {
    init_test_environment()?;

    let result = user_journeys::test_uj03_fault_recovery().await?;

    if !result.passed {
        panic!("UJ-03 test failed: {:?}", result.errors);
    }

    Ok(())
}

#[tokio::test]
async fn test_user_journey_uj04_debug_workflow() -> Result<()> {
    init_test_environment()?;

    let result = user_journeys::test_uj04_debug_workflow().await?;

    if !result.passed {
        panic!("UJ-04 test failed: {:?}", result.errors);
    }

    Ok(())
}

#[tokio::test]
async fn test_performance_gates_ffb_jitter() -> Result<()> {
    init_test_environment()?;

    let result = gates::test_ffb_jitter_gate().await?;

    if !result.passed {
        panic!("FFB jitter gate failed: {:?}", result.errors);
    }

    assert!(result.metrics.jitter_p99_ms <= MAX_JITTER_P99_MS);
    Ok(())
}

#[tokio::test]
async fn test_performance_gates_hid_latency() -> Result<()> {
    init_test_environment()?;

    let result = gates::test_hid_latency_gate().await?;

    if !result.passed {
        panic!("HID latency gate failed: {:?}", result.errors);
    }

    assert!(result.metrics.hid_latency_p99_us <= MAX_HID_LATENCY_P99_US);
    Ok(())
}

#[tokio::test]
async fn test_performance_gates_zero_missed_ticks() -> Result<()> {
    init_test_environment()?;

    let result = gates::test_zero_missed_ticks_gate().await?;

    if !result.passed {
        panic!("Zero missed ticks gate failed: {:?}", result.errors);
    }

    assert_eq!(result.metrics.missed_ticks, 0);
    Ok(())
}

#[tokio::test]
async fn test_hotplug_stress_basic() -> Result<()> {
    init_test_environment()?;

    let result = stress::test_hotplug_stress().await?;

    if !result.passed {
        panic!("Hot-plug stress test failed: {:?}", result.errors);
    }

    Ok(())
}

#[tokio::test]
async fn test_fault_injection_stress() -> Result<()> {
    init_test_environment()?;

    let result = stress::test_fault_injection_stress().await?;

    if !result.passed {
        panic!("Fault injection stress test failed: {:?}", result.errors);
    }

    Ok(())
}

#[tokio::test]
#[ignore] // Long-running test, run with --ignored
async fn test_ci_soak_test() -> Result<()> {
    init_test_environment()?;

    let result = soak::run_ci_soak_test().await?;

    if !result.passed {
        panic!("CI soak test failed: {:?}", result.errors);
    }

    assert_eq!(result.metrics.missed_ticks, 0);
    Ok(())
}

#[tokio::test]
async fn test_acceptance_tests_subset() -> Result<()> {
    init_test_environment()?;

    // Run a subset of acceptance tests for regular CI
    let results = acceptance::run_all_acceptance_tests().await?;

    let failed_tests: Vec<_> = results
        .iter()
        .filter(|(_, result)| !result.passed)
        .collect();

    if !failed_tests.is_empty() {
        panic!("Acceptance tests failed: {:?}", failed_tests);
    }

    Ok(())
}

#[tokio::test]
async fn test_performance_benchmark_suite() -> Result<()> {
    init_test_environment()?;

    let results = performance::run_performance_benchmark_suite().await?;

    let failed_benchmarks: Vec<_> = results.iter().filter(|result| !result.passed).collect();

    if !failed_benchmarks.is_empty() {
        panic!("Performance benchmarks failed: {:?}", failed_benchmarks);
    }

    Ok(())
}

// Test fixtures validation
#[tokio::test]
async fn test_device_fixtures() -> Result<()> {
    init_test_environment()?;

    let fixtures = fixtures::get_device_fixtures();

    assert!(!fixtures.is_empty());

    for fixture in fixtures {
        // Validate fixture data
        assert!(!fixture.name.is_empty());
        assert!(fixture.capabilities.max_torque_cnm > 0);
        assert!(fixture.capabilities.encoder_cpr > 0);
        assert!(!fixture.telemetry_data.samples.is_empty());
    }

    Ok(())
}

#[tokio::test]
async fn test_profile_fixtures() -> Result<()> {
    init_test_environment()?;

    let fixtures = fixtures::get_profile_fixtures();

    assert!(!fixtures.is_empty());

    for fixture in fixtures {
        // Validate fixture structure
        assert!(!fixture.name.is_empty());
        assert!(!fixture.json_content.is_empty());

        // Try to parse JSON
        let parse_result = serde_json::from_str::<serde_json::Value>(&fixture.json_content);

        if fixture.expected_valid {
            assert!(
                parse_result.is_ok(),
                "Valid fixture should parse: {}",
                fixture.name
            );
        }
        // Note: Invalid fixtures might still parse as JSON but fail schema validation
    }

    Ok(())
}

// Integration test configuration validation
#[test]
fn test_performance_thresholds() {
    // Validate that our performance thresholds are reasonable
    assert!(MAX_JITTER_P99_MS > 0.0);
    assert!(MAX_JITTER_P99_MS <= 1.0); // Should be sub-millisecond

    assert!(MAX_HID_LATENCY_P99_US > 0.0);
    assert!(MAX_HID_LATENCY_P99_US <= 1000.0); // Should be sub-millisecond

    assert_eq!(FFB_FREQUENCY_HZ, 1000); // 1kHz requirement
}

#[test]
fn test_soak_test_duration() {
    // Validate soak test duration is 48 hours
    assert_eq!(SOAK_TEST_DURATION, Duration::from_secs(48 * 60 * 60));
}

// Helper function to run a quick smoke test
async fn run_smoke_test() -> Result<TestResult> {
    let config = TestConfig {
        duration: Duration::from_secs(5),
        virtual_device: true,
        enable_tracing: false,
        enable_metrics: false,
        ..Default::default()
    };

    let mut harness = common::TestHarness::new(config).await?;
    let start_time = std::time::Instant::now();

    harness.start_service().await?;

    // Basic functionality check
    tokio::time::sleep(Duration::from_secs(3)).await;

    let metrics = harness.collect_metrics().await;
    harness.shutdown().await?;

    Ok(TestResult {
        passed: true,
        duration: start_time.elapsed(),
        metrics,
        errors: vec![],
        requirement_coverage: vec!["SMOKE".to_string()],
    })
}

#[tokio::test]
async fn test_smoke_test() -> Result<()> {
    init_test_environment()?;

    let result = run_smoke_test().await?;

    if !result.passed {
        panic!("Smoke test failed: {:?}", result.errors);
    }

    Ok(())
}
