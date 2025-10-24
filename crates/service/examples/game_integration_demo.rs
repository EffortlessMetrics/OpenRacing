//! Game Integration Demo
//! 
//! Demonstrates the complete game integration functionality for task 9
//! 
//! Usage: cargo run --example game_integration_demo

use racing_wheel_service::{
    GameIntegrationService, OneClickConfigRequest, ProfileService,
};
use std::sync::Arc;
use tempfile::TempDir;
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();
    
    info!("Starting Game Integration Demo");
    
    // Demo 1: Basic service functionality
    demo_basic_functionality().await?;
    
    // Demo 2: One-click configuration
    demo_one_click_configuration().await?;
    
    // Demo 3: Performance testing
    demo_performance_testing().await?;
    
    // Demo 4: Comprehensive E2E tests
    demo_comprehensive_tests().await?;
    
    info!("Game Integration Demo completed successfully");
    Ok(())
}

/// Demonstrate basic service functionality
async fn demo_basic_functionality() -> Result<(), Box<dyn std::error::Error>> {
    info!("=== Demo 1: Basic Service Functionality ===");
    
    // Create profile service
    let profile_service = Arc::new(ProfileService::new().await?);
    
    // Create game integration service
    let mut integration_service = GameIntegrationService::new(profile_service).await?;
    integration_service.start().await?;
    
    // Get supported games
    let supported_games = integration_service.get_supported_games().await;
    info!("Supported games: {:?}", supported_games);
    
    // Get metrics
    let metrics = integration_service.get_metrics().await;
    info!("Initial metrics: {:?}", metrics);
    
    info!("Basic functionality demo completed");
    Ok(())
}

/// Demonstrate one-click configuration
async fn demo_one_click_configuration() -> Result<(), Box<dyn std::error::Error>> {
    info!("=== Demo 2: One-Click Configuration ===");
    
    let profile_service = Arc::new(ProfileService::new().await?);
    let mut integration_service = GameIntegrationService::new(profile_service).await?;
    integration_service.start().await?;
    
    let temp_dir = TempDir::new()?;
    
    // Configure iRacing
    info!("Configuring iRacing...");
    let iracing_request = OneClickConfigRequest {
        game_id: "iracing".to_string(),
        game_path: temp_dir.path().to_string_lossy().to_string(),
        enable_auto_switching: true,
        profile_id: Some("iracing_default".to_string()),
    };
    
    let iracing_result = integration_service.configure_one_click(iracing_request).await?;
    info!(
        "iRacing configuration result: success={}, diffs={}, duration={}ms",
        iracing_result.success,
        iracing_result.config_diffs.len(),
        iracing_result.duration_ms
    );
    
    // Configure ACC
    info!("Configuring ACC...");
    let acc_request = OneClickConfigRequest {
        game_id: "acc".to_string(),
        game_path: temp_dir.path().to_string_lossy().to_string(),
        enable_auto_switching: false,
        profile_id: None,
    };
    
    let acc_result = integration_service.configure_one_click(acc_request).await?;
    info!(
        "ACC configuration result: success={}, diffs={}, duration={}ms",
        acc_result.success,
        acc_result.config_diffs.len(),
        acc_result.duration_ms
    );
    
    // Show final metrics
    let final_metrics = integration_service.get_metrics().await;
    info!("Final metrics: {:?}", final_metrics);
    
    info!("One-click configuration demo completed");
    Ok(())
}

/// Demonstrate performance testing
async fn demo_performance_testing() -> Result<(), Box<dyn std::error::Error>> {
    info!("=== Demo 3: Performance Testing ===");
    
    let profile_service = Arc::new(ProfileService::new().await?);
    let mut integration_service = GameIntegrationService::new(profile_service).await?;
    integration_service.start().await?;
    
    // Test profile switching performance (GI-02 requirement: ≤500ms)
    info!("Testing profile switching performance...");
    let switch_result = integration_service
        .test_profile_switching_performance("iracing")
        .await;
    
    match switch_result {
        Ok(duration) => {
            let duration_ms = duration.as_millis();
            info!("Profile switch completed in {}ms", duration_ms);
            
            if duration_ms <= 500 {
                info!("✓ Profile switching meets ≤500ms requirement (GI-02)");
            } else {
                info!("✗ Profile switching exceeds 500ms requirement: {}ms", duration_ms);
            }
        }
        Err(e) => {
            info!("Profile switching test failed: {}", e);
        }
    }
    
    // Test configuration performance
    info!("Testing configuration performance...");
    let temp_dir = TempDir::new()?;
    let config_start = std::time::Instant::now();
    
    let request = OneClickConfigRequest {
        game_id: "iracing".to_string(),
        game_path: temp_dir.path().to_string_lossy().to_string(),
        enable_auto_switching: false,
        profile_id: None,
    };
    
    let _result = integration_service.configure_one_click(request).await?;
    let config_duration = config_start.elapsed();
    
    info!("Configuration completed in {}ms", config_duration.as_millis());
    
    if config_duration.as_millis() < 1000 {
        info!("✓ Configuration performance is acceptable (<1000ms)");
    } else {
        info!("✗ Configuration performance is slow: {}ms", config_duration.as_millis());
    }
    
    info!("Performance testing demo completed");
    Ok(())
}

/// Demonstrate comprehensive E2E tests
async fn demo_comprehensive_tests() -> Result<(), Box<dyn std::error::Error>> {
    info!("=== Demo 4: Comprehensive E2E Tests ===");
    
    // Create and run comprehensive test suite
    let mut test_suite = GameIntegrationE2ETestSuite::new().await?;
    
    info!("Running comprehensive end-to-end test suite...");
    let test_results = test_suite.run_all_tests().await?;
    
    // Print detailed results
    print_test_summary(&test_results);
    
    // Show metrics
    let metrics = test_suite.get_metrics().await;
    info!("Test suite metrics: {:?}", metrics);
    
    // Validate requirements coverage
    info!("=== Requirements Validation ===");
    
    let iracing_config_test = test_results.iter()
        .find(|r| r.test_name == "iracing_one_click_config");
    let acc_config_test = test_results.iter()
        .find(|r| r.test_name == "acc_one_click_config");
    let performance_test = test_results.iter()
        .find(|r| r.test_name == "auto_profile_switching_performance");
    let validation_test = test_results.iter()
        .find(|r| r.test_name == "configuration_validation");
    let led_test = test_results.iter()
        .find(|r| r.test_name == "led_heartbeat_validation");
    
    // Check GI-01 requirement
    if iracing_config_test.map(|t| t.success).unwrap_or(false) &&
       acc_config_test.map(|t| t.success).unwrap_or(false) {
        info!("✓ GI-01: One-click telemetry configuration - PASSED");
    } else {
        info!("✗ GI-01: One-click telemetry configuration - FAILED");
    }
    
    // Check GI-02 requirement
    if performance_test.map(|t| t.success).unwrap_or(false) {
        info!("✓ GI-02: Auto profile switching ≤500ms - PASSED");
    } else {
        info!("✗ GI-02: Auto profile switching ≤500ms - FAILED");
    }
    
    // Check validation system
    if validation_test.map(|t| t.success).unwrap_or(false) {
        info!("✓ Configuration validation system - PASSED");
    } else {
        info!("✗ Configuration validation system - FAILED");
    }
    
    // Check LED heartbeat validation
    if led_test.map(|t| t.success).unwrap_or(false) {
        info!("✓ LED heartbeat validation - PASSED");
    } else {
        info!("✗ LED heartbeat validation - FAILED");
    }
    
    let passed_count = test_results.iter().filter(|r| r.success).count();
    let total_count = test_results.len();
    let success_rate = (passed_count as f64 / total_count as f64) * 100.0;
    
    info!(
        "Overall test results: {}/{} passed ({:.1}% success rate)",
        passed_count,
        total_count,
        success_rate
    );
    
    if success_rate >= 75.0 {
        info!("✓ Task 9 implementation meets requirements");
    } else {
        info!("✗ Task 9 implementation needs improvement");
    }
    
    info!("Comprehensive E2E tests demo completed");
    Ok(())
}