//! Hot-plug stress test binary

use std::process;
use anyhow::Result;
use clap::Parser;
use tracing::{info, error};
use racing_wheel_integration_tests::{init_test_environment, stress};

#[derive(Parser)]
#[command(name = "hotplug-stress")]
#[command(about = "Run hot-plug stress tests for racing wheel software")]
struct Args {
    /// Test duration in seconds
    #[arg(short, long, default_value = "300")]
    duration: u64,
    
    /// Number of virtual devices to test with
    #[arg(short, long, default_value = "3")]
    devices: usize,
    
    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    init_test_environment()?;
    
    info!("Starting hot-plug stress test");
    info!("  Duration: {}s", args.duration);
    info!("  Devices: {}", args.devices);
    
    // Run all stress tests
    let tests = vec![
        ("Hot-plug Stress", stress::test_hotplug_stress()),
        ("Fault Injection Stress", stress::test_fault_injection_stress()),
        ("Memory Pressure Stress", stress::test_memory_pressure_stress()),
        ("CPU Load Stress", stress::test_cpu_load_stress()),
    ];
    
    let mut all_passed = true;
    let mut total_tests = 0;
    let mut passed_tests = 0;
    
    for (test_name, test_future) in tests {
        total_tests += 1;
        
        info!("Running: {}", test_name);
        let result = test_future.await?;
        
        if result.passed {
            passed_tests += 1;
            info!("✓ {}: PASSED", test_name);
            info!("  Duration: {:?}", result.duration);
            info!("  {}", result.metrics.report());
        } else {
            all_passed = false;
            error!("✗ {}: FAILED", test_name);
            error!("  Errors: {:?}", result.errors);
            error!("  {}", result.metrics.report());
        }
        
        info!("Requirements covered: {:?}", result.requirement_coverage);
        info!("");
    }
    
    info!("Stress Test Summary:");
    info!("  Total: {}", total_tests);
    info!("  Passed: {}", passed_tests);
    info!("  Failed: {}", total_tests - passed_tests);
    
    if all_passed {
        info!("🎉 All stress tests PASSED");
        process::exit(0);
    } else {
        error!("❌ Stress tests FAILED");
        process::exit(1);
    }
}