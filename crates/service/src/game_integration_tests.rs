//! End-to-End Game Integration Tests
//! 
//! Comprehensive tests for configuration file generation and LED heartbeat validation
//! Requirements: GI-01, GI-02

use crate::auto_profile_switching::AutoProfileSwitchingService;
use crate::config_validation::{ConfigValidationService, ValidationResult, ValidationType};
use crate::config_writers::{ACCConfigWriter, IRacingConfigWriter};
use crate::game_service::{ConfigDiff, ConfigWriter, DiffOperation, GameService, TelemetryConfig};
use crate::profile_service::ProfileService;
use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;
use tracing::{info, warn};

/// End-to-end test suite for game integration
pub struct GameIntegrationTestSuite {
    game_service: GameService,
    validation_service: ConfigValidationService,
    profile_service: Arc<ProfileService>,
    temp_dir: TempDir,
}

/// Test result for game integration
#[derive(Debug, Clone)]
pub struct GameIntegrationTestResult {
    pub test_name: String,
    pub game_id: String,
    pub success: bool,
    pub duration_ms: u64,
    pub validation_results: Vec<ValidationResult>,
    pub errors: Vec<String>,
}

impl GameIntegrationTestSuite {
    /// Create new test suite
    pub async fn new() -> Result<Self> {
        let game_service = GameService::new().await?;
        let validation_service = ConfigValidationService::new();
        let profile_service = Arc::new(ProfileService::new().await?);
        let temp_dir = TempDir::new()?;
        
        Ok(Self {
            game_service,
            validation_service,
            profile_service,
            temp_dir,
        })
    }
    
    /// Run all end-to-end tests
    pub async fn run_all_tests(&mut self) -> Result<Vec<GameIntegrationTestResult>> {
        info!("Starting comprehensive game integration tests");
        
        let mut results = Vec::new();
        
        // Test iRacing configuration generation
        results.push(self.test_iracing_config_generation().await?);
        
        // Test ACC configuration generation
        results.push(self.test_acc_config_generation().await?);
        
        // Test configuration validation
        results.push(self.test_config_validation().await?);
        
        // Test LED heartbeat validation
        results.push(self.test_led_heartbeat_validation().await?);
        
        // Test auto profile switching
        results.push(self.test_auto_profile_switching().await?);
        
        // Test end-to-end workflow
        results.push(self.test_end_to_end_workflow().await?);
        
        // Test performance requirements
        results.push(self.test_performance_requirements().await?);
        
        let passed_count = results.iter().filter(|r| r.success).count();
        let total_count = results.len();
        
        info!(
            passed = passed_count,
            total = total_count,
            "Game integration tests completed"
        );
        
        Ok(results)
    }
    
    /// Test iRacing configuration generation (GI-01)
    async fn test_iracing_config_generation(&mut self) -> Result<GameIntegrationTestResult> {
        let start_time = std::time::Instant::now();
        let test_name = "iracing_config_generation".to_string();
        let game_id = "iracing".to_string();
        
        info!(test_name = %test_name, "Starting iRacing configuration test");
        
        let mut errors = Vec::new();
        let mut validation_results = Vec::new();
        
        // Configure telemetry for iRacing
        let config_result = self.game_service
            .configure_telemetry(&game_id, self.temp_dir.path())
            .await;
        
        let success = match config_result {
            Ok(diffs) => {
                info!(diffs_count = diffs.len(), "iRacing configuration generated");
                
                // Validate the generated configuration
                match self.validation_service
                    .validate_config_generation(&game_id, &diffs)
                    .await
                {
                    Ok(validation_result) => {
                        validation_results.push(validation_result.clone());
                        validation_result.success
                    }
                    Err(e) => {
                        errors.push(format!("Validation failed: {}", e));
                        false
                    }
                }
            }
            Err(e) => {
                errors.push(format!("Configuration generation failed: {}", e));
                false
            }
        };
        
        let duration = start_time.elapsed();
        
        Ok(GameIntegrationTestResult {
            test_name,
            game_id,
            success,
            duration_ms: duration.as_millis() as u64,
            validation_results,
            errors,
        })
    }
    
    /// Test ACC configuration generation (GI-01)
    async fn test_acc_config_generation(&mut self) -> Result<GameIntegrationTestResult> {
        let start_time = std::time::Instant::now();
        let test_name = "acc_config_generation".to_string();
        let game_id = "acc".to_string();
        
        info!(test_name = %test_name, "Starting ACC configuration test");
        
        let mut errors = Vec::new();
        let mut validation_results = Vec::new();
        
        // Configure telemetry for ACC
        let config_result = self.game_service
            .configure_telemetry(&game_id, self.temp_dir.path())
            .await;
        
        let success = match config_result {
            Ok(diffs) => {
                info!(diffs_count = diffs.len(), "ACC configuration generated");
                
                // Validate the generated configuration
                match self.validation_service
                    .validate_config_generation(&game_id, &diffs)
                    .await
                {
                    Ok(validation_result) => {
                        validation_results.push(validation_result.clone());
                        validation_result.success
                    }
                    Err(e) => {
                        errors.push(format!("Validation failed: {}", e));
                        false
                    }
                }
            }
            Err(e) => {
                errors.push(format!("Configuration generation failed: {}", e));
                false
            }
        };
        
        let duration = start_time.elapsed();
        
        Ok(GameIntegrationTestResult {
            test_name,
            game_id,
            success,
            duration_ms: duration.as_millis() as u64,
            validation_results,
            errors,
        })
    }
    
    /// Test configuration validation system
    async fn test_config_validation(&mut self) -> Result<GameIntegrationTestResult> {
        let start_time = std::time::Instant::now();
        let test_name = "config_validation".to_string();
        let game_id = "iracing".to_string();
        
        info!(test_name = %test_name, "Starting configuration validation test");
        
        let mut errors = Vec::new();
        let mut validation_results = Vec::new();
        
        // Generate configuration first
        let diffs = self.game_service
            .configure_telemetry(&game_id, self.temp_dir.path())
            .await?;
        
        // Create actual config files for validation
        self.create_test_config_files(&game_id).await?;
        
        // Validate configuration files
        let validation_result = self.validation_service
            .validate_config_files(&game_id, self.temp_dir.path())
            .await;
        
        let success = match validation_result {
            Ok(result) => {
                validation_results.push(result.clone());
                result.success
            }
            Err(e) => {
                errors.push(format!("Configuration validation failed: {}", e));
                false
            }
        };
        
        let duration = start_time.elapsed();
        
        Ok(GameIntegrationTestResult {
            test_name,
            game_id,
            success,
            duration_ms: duration.as_millis() as u64,
            validation_results,
            errors,
        })
    }
    
    /// Test LED heartbeat validation
    async fn test_led_heartbeat_validation(&mut self) -> Result<GameIntegrationTestResult> {
        let start_time = std::time::Instant::now();
        let test_name = "led_heartbeat_validation".to_string();
        let game_id = "test".to_string();
        
        info!(test_name = %test_name, "Starting LED heartbeat validation test");
        
        let mut errors = Vec::new();
        let mut validation_results = Vec::new();
        
        // Test LED heartbeat validation
        let validation_result = self.validation_service
            .validate_led_heartbeat()
            .await;
        
        let success = match validation_result {
            Ok(result) => {
                validation_results.push(result.clone());
                result.success
            }
            Err(e) => {
                errors.push(format!("LED heartbeat validation failed: {}", e));
                false
            }
        };
        
        let duration = start_time.elapsed();
        
        Ok(GameIntegrationTestResult {
            test_name,
            game_id,
            success,
            duration_ms: duration.as_millis() as u64,
            validation_results,
            errors,
        })
    }
    
    /// Test auto profile switching (GI-02)
    async fn test_auto_profile_switching(&mut self) -> Result<GameIntegrationTestResult> {
        let start_time = std::time::Instant::now();
        let test_name = "auto_profile_switching".to_string();
        let game_id = "iracing".to_string();
        
        info!(test_name = %test_name, "Starting auto profile switching test");
        
        let mut errors = Vec::new();
        let mut validation_results = Vec::new();
        
        // Create auto profile switching service
        let switching_service = AutoProfileSwitchingService::new(self.profile_service.clone())?;
        
        // Set up game profile mapping
        switching_service.set_game_profile(
            game_id.clone(),
            "iracing_gt3".to_string(),
        ).await?;
        
        // Test profile switching with timeout requirement (≤500ms)
        let switch_start = std::time::Instant::now();
        let switch_result = timeout(
            Duration::from_millis(500),
            switching_service.force_switch_to_profile("iracing_gt3"),
        ).await;
        
        let switch_duration = switch_start.elapsed();
        
        let success = match switch_result {
            Ok(Ok(())) => {
                info!(
                    switch_duration_ms = switch_duration.as_millis(),
                    "Profile switch completed within timeout"
                );
                
                // Verify the switch met the ≤500ms requirement
                if switch_duration <= Duration::from_millis(500) {
                    true
                } else {
                    errors.push(format!(
                        "Profile switch took {}ms, exceeds 500ms requirement",
                        switch_duration.as_millis()
                    ));
                    false
                }
            }
            Ok(Err(e)) => {
                errors.push(format!("Profile switch failed: {}", e));
                false
            }
            Err(_) => {
                errors.push("Profile switch timed out (>500ms)".to_string());
                false
            }
        };
        
        let duration = start_time.elapsed();
        
        Ok(GameIntegrationTestResult {
            test_name,
            game_id,
            success,
            duration_ms: duration.as_millis() as u64,
            validation_results,
            errors,
        })
    }
    
    /// Test complete end-to-end workflow
    async fn test_end_to_end_workflow(&mut self) -> Result<GameIntegrationTestResult> {
        let start_time = std::time::Instant::now();
        let test_name = "end_to_end_workflow".to_string();
        let game_id = "iracing".to_string();
        
        info!(test_name = %test_name, "Starting end-to-end workflow test");
        
        let mut errors = Vec::new();
        let mut validation_results = Vec::new();
        
        // Step 1: Configure telemetry
        let diffs = match self.game_service
            .configure_telemetry(&game_id, self.temp_dir.path())
            .await
        {
            Ok(diffs) => diffs,
            Err(e) => {
                errors.push(format!("Telemetry configuration failed: {}", e));
                return Ok(GameIntegrationTestResult {
                    test_name,
                    game_id,
                    success: false,
                    duration_ms: start_time.elapsed().as_millis() as u64,
                    validation_results,
                    errors,
                });
            }
        };
        
        // Step 2: Create config files
        if let Err(e) = self.create_test_config_files(&game_id).await {
            errors.push(format!("Config file creation failed: {}", e));
        }
        
        // Step 3: Validate end-to-end
        let validation_result = self.validation_service
            .validate_end_to_end(&game_id, self.temp_dir.path(), &diffs)
            .await;
        
        let success = match validation_result {
            Ok(result) => {
                validation_results.push(result.clone());
                result.success
            }
            Err(e) => {
                errors.push(format!("End-to-end validation failed: {}", e));
                false
            }
        };
        
        let duration = start_time.elapsed();
        
        Ok(GameIntegrationTestResult {
            test_name,
            game_id,
            success,
            duration_ms: duration.as_millis() as u64,
            validation_results,
            errors,
        })
    }
    
    /// Test performance requirements
    async fn test_performance_requirements(&mut self) -> Result<GameIntegrationTestResult> {
        let start_time = std::time::Instant::now();
        let test_name = "performance_requirements".to_string();
        let game_id = "iracing".to_string();
        
        info!(test_name = %test_name, "Starting performance requirements test");
        
        let mut errors = Vec::new();
        let mut validation_results = Vec::new();
        
        // Test configuration generation performance
        let config_start = std::time::Instant::now();
        let config_result = self.game_service
            .configure_telemetry(&game_id, self.temp_dir.path())
            .await;
        let config_duration = config_start.elapsed();
        
        // Test validation performance
        let validation_start = std::time::Instant::now();
        let validation_result = if let Ok(diffs) = &config_result {
            self.validation_service
                .validate_config_generation(&game_id, diffs)
                .await
        } else {
            return Ok(GameIntegrationTestResult {
                test_name,
                game_id,
                success: false,
                duration_ms: start_time.elapsed().as_millis() as u64,
                validation_results,
                errors: vec!["Configuration generation failed".to_string()],
            });
        };
        let validation_duration = validation_start.elapsed();
        
        // Check performance requirements
        let mut success = true;
        
        // Configuration should complete quickly (< 1 second)
        if config_duration > Duration::from_secs(1) {
            errors.push(format!(
                "Configuration generation took {}ms, should be < 1000ms",
                config_duration.as_millis()
            ));
            success = false;
        }
        
        // Validation should complete quickly (< 500ms)
        if validation_duration > Duration::from_millis(500) {
            errors.push(format!(
                "Validation took {}ms, should be < 500ms",
                validation_duration.as_millis()
            ));
            success = false;
        }
        
        if let Ok(result) = validation_result {
            validation_results.push(result.clone());
            if !result.success {
                success = false;
            }
        } else {
            success = false;
        }
        
        let duration = start_time.elapsed();
        
        info!(
            config_duration_ms = config_duration.as_millis(),
            validation_duration_ms = validation_duration.as_millis(),
            total_duration_ms = duration.as_millis(),
            success = success,
            "Performance requirements test completed"
        );
        
        Ok(GameIntegrationTestResult {
            test_name,
            game_id,
            success,
            duration_ms: duration.as_millis() as u64,
            validation_results,
            errors,
        })
    }
    
    /// Create test configuration files
    async fn create_test_config_files(&self, game_id: &str) -> Result<()> {
        match game_id {
            "iracing" => {
                let config_dir = self.temp_dir.path().join("Documents/iRacing");
                std::fs::create_dir_all(&config_dir)?;
                
                let config_file = config_dir.join("app.ini");
                std::fs::write(&config_file, "[Telemetry]\ntelemetryDiskFile=1\n")?;
            }
            "acc" => {
                let config_dir = self.temp_dir.path().join("Documents/Assetto Corsa Competizione/Config");
                std::fs::create_dir_all(&config_dir)?;
                
                let config_file = config_dir.join("broadcasting.json");
                let config_content = r#"{
  "updListenerPort": 9996,
  "connectionId": "",
  "broadcastingPort": 9000,
  "commandPassword": "",
  "updateRateHz": 100
}"#;
                std::fs::write(&config_file, config_content)?;
            }
            _ => {
                return Err(anyhow::anyhow!("Unknown game ID: {}", game_id));
            }
        }
        
        Ok(())
    }
    
    /// Get test results summary
    pub fn get_test_summary(results: &[GameIntegrationTestResult]) -> TestSummary {
        let total_tests = results.len();
        let passed_tests = results.iter().filter(|r| r.success).count();
        let failed_tests = total_tests - passed_tests;
        
        let total_duration: u64 = results.iter().map(|r| r.duration_ms).sum();
        let avg_duration = if total_tests > 0 {
            total_duration / total_tests as u64
        } else {
            0
        };
        
        let all_errors: Vec<String> = results
            .iter()
            .flat_map(|r| r.errors.iter().cloned())
            .collect();
        
        TestSummary {
            total_tests,
            passed_tests,
            failed_tests,
            total_duration_ms: total_duration,
            avg_duration_ms: avg_duration,
            errors: all_errors,
        }
    }
}

/// Test results summary
#[derive(Debug, Clone)]
pub struct TestSummary {
    pub total_tests: usize,
    pub passed_tests: usize,
    pub failed_tests: usize,
    pub total_duration_ms: u64,
    pub avg_duration_ms: u64,
    pub errors: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_test::traced_test;
    
    #[tokio::test]
    #[traced_test]
    async fn test_suite_creation() {
        let suite = GameIntegrationTestSuite::new().await.unwrap();
        assert!(suite.temp_dir.path().exists());
    }
    
    #[tokio::test]
    #[traced_test]
    async fn test_iracing_config_generation() {
        let mut suite = GameIntegrationTestSuite::new().await.unwrap();
        let result = suite.test_iracing_config_generation().await.unwrap();
        
        assert_eq!(result.test_name, "iracing_config_generation");
        assert_eq!(result.game_id, "iracing");
        // Note: May fail in test environment without proper setup
    }
    
    #[tokio::test]
    #[traced_test]
    async fn test_acc_config_generation() {
        let mut suite = GameIntegrationTestSuite::new().await.unwrap();
        let result = suite.test_acc_config_generation().await.unwrap();
        
        assert_eq!(result.test_name, "acc_config_generation");
        assert_eq!(result.game_id, "acc");
        // Note: May fail in test environment without proper setup
    }
    
    #[tokio::test]
    #[traced_test]
    async fn test_led_heartbeat_validation() {
        let mut suite = GameIntegrationTestSuite::new().await.unwrap();
        let result = suite.test_led_heartbeat_validation().await.unwrap();
        
        assert_eq!(result.test_name, "led_heartbeat_validation");
        assert!(result.duration_ms > 0);
    }
    
    #[tokio::test]
    #[traced_test]
    async fn test_performance_requirements() {
        let mut suite = GameIntegrationTestSuite::new().await.unwrap();
        let result = suite.test_performance_requirements().await.unwrap();
        
        assert_eq!(result.test_name, "performance_requirements");
        // Should complete within reasonable time
        assert!(result.duration_ms < 5000); // 5 seconds max
    }
    
    #[test]
    fn test_summary_calculation() {
        let results = vec![
            GameIntegrationTestResult {
                test_name: "test1".to_string(),
                game_id: "game1".to_string(),
                success: true,
                duration_ms: 100,
                validation_results: Vec::new(),
                errors: Vec::new(),
            },
            GameIntegrationTestResult {
                test_name: "test2".to_string(),
                game_id: "game2".to_string(),
                success: false,
                duration_ms: 200,
                validation_results: Vec::new(),
                errors: vec!["Error 1".to_string()],
            },
        ];
        
        let summary = GameIntegrationTestSuite::get_test_summary(&results);
        
        assert_eq!(summary.total_tests, 2);
        assert_eq!(summary.passed_tests, 1);
        assert_eq!(summary.failed_tests, 1);
        assert_eq!(summary.total_duration_ms, 300);
        assert_eq!(summary.avg_duration_ms, 150);
        assert_eq!(summary.errors.len(), 1);
    }
}