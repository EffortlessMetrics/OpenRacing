//! Soak test binary for 48-hour continuous operation testing

use anyhow::Result;
use clap::{Parser, ValueEnum};
use racing_wheel_integration_tests::{init_test_environment, soak};
use std::process;
use tracing::{error, info};

#[derive(Parser)]
#[command(name = "soak-test")]
#[command(about = "Run soak tests for racing wheel software")]
struct Args {
    /// Test mode to run
    #[arg(short, long, default_value = "ci")]
    mode: SoakMode,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
}

#[derive(ValueEnum, Clone, Debug)]
enum SoakMode {
    /// Full 48-hour soak test
    Full,
    /// CI-friendly 1-hour soak test
    Ci,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    init_test_environment()?;

    let result = match args.mode {
        SoakMode::Full => {
            info!("Starting full 48-hour soak test");
            soak::run_soak_test().await?
        }
        SoakMode::Ci => {
            info!("Starting CI 1-hour soak test");
            soak::run_ci_soak_test().await?
        }
    };

    info!("Soak Test Results:");
    info!("  Duration: {:?}", result.duration);
    info!(
        "  Status: {}",
        if result.passed { "PASSED" } else { "FAILED" }
    );
    info!("  {}", result.metrics.report());

    if !result.errors.is_empty() {
        error!("Errors encountered:");
        for error in &result.errors {
            error!("  - {}", error);
        }
    }

    info!("Requirements covered: {:?}", result.requirement_coverage);

    if result.passed {
        info!("🎉 Soak test PASSED");
        process::exit(0);
    } else {
        error!("❌ Soak test FAILED");
        process::exit(1);
    }
}
