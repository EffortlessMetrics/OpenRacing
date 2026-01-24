//! Performance gate binary for CI
//!
//! This binary runs the performance gates and exits with appropriate codes for CI

use anyhow::Result;
use racing_wheel_integration_tests::{gates, init_test_environment};
use std::process;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    init_test_environment()?;

    info!("Starting CI performance gates");

    let results = gates::run_ci_performance_gates().await?;

    let mut all_passed = true;
    let mut total_tests = 0;
    let mut passed_tests = 0;

    for (i, result) in results.iter().enumerate() {
        total_tests += 1;

        if result.passed {
            passed_tests += 1;
            info!("✓ Performance Gate {}: PASSED", i + 1);
            info!("  {}", result.metrics.report());
        } else {
            all_passed = false;
            error!("✗ Performance Gate {}: FAILED", i + 1);
            error!("  Errors: {:?}", result.errors);
            error!("  {}", result.metrics.report());
        }
    }

    info!("Performance Gates Summary:");
    info!("  Total: {}", total_tests);
    info!("  Passed: {}", passed_tests);
    info!("  Failed: {}", total_tests - passed_tests);

    if all_passed {
        info!("🎉 All performance gates PASSED");
        process::exit(0);
    } else {
        error!("❌ Performance gates FAILED");
        process::exit(1);
    }
}
