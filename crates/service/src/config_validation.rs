//! Configuration Validation Module
//!
//! Implements validation system to verify configuration file changes were applied correctly
//! Supports golden file testing and LED heartbeat validation

use crate::game_service::{ConfigDiff, DiffOperation, TelemetryConfig};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, warn};

/// Configuration validation service
pub struct ConfigValidationService {
    /// Golden file fixtures for testing
    golden_files: HashMap<String, GoldenFileFixture>,
    /// LED heartbeat validation settings
    led_validation: LedValidationConfig,
}

/// Golden file fixture for testing configuration generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenFileFixture {
    pub game_id: String,
    pub config: TelemetryConfig,
    pub expected_diffs: Vec<ConfigDiff>,
    pub expected_files: Vec<ExpectedFile>,
}

/// Expected file content for validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedFile {
    pub path: String,
    pub content: String,
    pub checksum: Option<String>,
}

/// LED heartbeat validation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedValidationConfig {
    pub heartbeat_interval: Duration,
    pub validation_timeout: Duration,
    pub expected_pattern: LedPattern,
}

/// LED pattern for validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedPattern {
    pub sequence: Vec<LedState>,
    pub repeat_count: u32,
}

/// LED state for pattern validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedState {
    pub color: String,
    pub brightness: f32,
    pub duration_ms: u64,
}

/// Validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub success: bool,
    pub validation_type: ValidationType,
    pub details: ValidationDetails,
    pub duration_ms: u64,
    pub timestamp: Instant,
}

/// Type of validation performed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationType {
    ConfigFileGeneration,
    ConfigFileContent,
    LedHeartbeat,
    EndToEnd,
}

/// Detailed validation results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationDetails {
    pub expected_count: usize,
    pub actual_count: usize,
    pub matched_items: Vec<String>,
    pub missing_items: Vec<String>,
    pub unexpected_items: Vec<String>,
    pub errors: Vec<String>,
}

impl ConfigValidationService {
    /// Create new configuration validation service
    pub fn new() -> Self {
        Self {
            golden_files: Self::load_golden_files(),
            led_validation: LedValidationConfig {
                heartbeat_interval: Duration::from_millis(100),
                validation_timeout: Duration::from_secs(5),
                expected_pattern: LedPattern {
                    sequence: vec![
                        LedState {
                            color: "green".to_string(),
                            brightness: 1.0,
                            duration_ms: 100,
                        },
                        LedState {
                            color: "off".to_string(),
                            brightness: 0.0,
                            duration_ms: 100,
                        },
                    ],
                    repeat_count: 5,
                },
            },
        }
    }

    /// Load golden file fixtures
    fn load_golden_files() -> HashMap<String, GoldenFileFixture> {
        let mut fixtures = HashMap::new();

        // iRacing golden file fixture
        fixtures.insert(
            "iracing".to_string(),
            GoldenFileFixture {
                game_id: "iracing".to_string(),
                config: TelemetryConfig {
                    enabled: true,
                    update_rate_hz: 60,
                    output_method: "shared_memory".to_string(),
                    output_target: "127.0.0.1:12345".to_string(),
                    fields: vec![
                        "ffb_scalar".to_string(),
                        "rpm".to_string(),
                        "speed_ms".to_string(),
                    ],
                },
                expected_diffs: vec![ConfigDiff {
                    file_path: "Documents/iRacing/app.ini".to_string(),
                    section: Some("Telemetry".to_string()),
                    key: "telemetryDiskFile".to_string(),
                    old_value: None,
                    new_value: "1".to_string(),
                    operation: DiffOperation::Add,
                }],
                expected_files: vec![ExpectedFile {
                    path: "Documents/iRacing/app.ini".to_string(),
                    content: "[Telemetry]\ntelemetryDiskFile=1\n".to_string(),
                    checksum: None,
                }],
            },
        );

        // ACC golden file fixture
        fixtures.insert(
            "acc".to_string(),
            GoldenFileFixture {
                game_id: "acc".to_string(),
                config: TelemetryConfig {
                    enabled: true,
                    update_rate_hz: 100,
                    output_method: "udp_broadcast".to_string(),
                    output_target: "127.0.0.1:9000".to_string(),
                    fields: vec![
                        "ffb_scalar".to_string(),
                        "rpm".to_string(),
                        "gear".to_string(),
                    ],
                },
                expected_diffs: vec![ConfigDiff {
                    file_path: "Documents/Assetto Corsa Competizione/Config/broadcasting.json"
                        .to_string(),
                    section: None,
                    key: "entire_file".to_string(),
                    old_value: None,
                    new_value: r#"{
  "updListenerPort": 9000,
  "connectionId": "",
  "connectionPassword": "",
  "broadcastingPort": 9000,
  "commandPassword": "",
  "updateRateHz": 100
}"#
                    .to_string(),
                    operation: DiffOperation::Add,
                }],
                expected_files: vec![ExpectedFile {
                    path: "Documents/Assetto Corsa Competizione/Config/broadcasting.json"
                        .to_string(),
                    content: r#"{
  "updListenerPort": 9000,
  "connectionId": "",
  "connectionPassword": "",
  "broadcastingPort": 9000,
  "commandPassword": "",
  "updateRateHz": 100
}"#
                    .to_string(),
                    checksum: None,
                }],
            },
        );

        fixtures
    }

    /// Validate configuration file generation against golden files
    pub async fn validate_config_generation(
        &self,
        game_id: &str,
        actual_diffs: &[ConfigDiff],
    ) -> Result<ValidationResult> {
        let start_time = Instant::now();

        info!(
            game_id = %game_id,
            actual_diffs_count = actual_diffs.len(),
            "Validating configuration file generation"
        );

        let fixture = self
            .golden_files
            .get(game_id)
            .ok_or_else(|| anyhow::anyhow!("No golden file fixture for game: {}", game_id))?;

        let expected_diffs = &fixture.expected_diffs;
        let mut details = ValidationDetails {
            expected_count: expected_diffs.len(),
            actual_count: actual_diffs.len(),
            matched_items: Vec::new(),
            missing_items: Vec::new(),
            unexpected_items: Vec::new(),
            errors: Vec::new(),
        };

        // Compare expected vs actual diffs
        for expected_diff in expected_diffs {
            let diff_key = format!("{}:{}", expected_diff.file_path, expected_diff.key);

            if let Some(actual_diff) = actual_diffs
                .iter()
                .find(|d| d.file_path == expected_diff.file_path && d.key == expected_diff.key)
            {
                if self.compare_config_diffs(expected_diff, actual_diff) {
                    details.matched_items.push(diff_key);
                } else {
                    details.errors.push(format!(
                        "Diff mismatch for {}: expected {:?}, got {:?}",
                        diff_key, expected_diff, actual_diff
                    ));
                }
            } else {
                details.missing_items.push(diff_key);
            }
        }

        // Check for unexpected diffs
        for actual_diff in actual_diffs {
            let diff_key = format!("{}:{}", actual_diff.file_path, actual_diff.key);

            if !expected_diffs
                .iter()
                .any(|d| d.file_path == actual_diff.file_path && d.key == actual_diff.key)
            {
                details.unexpected_items.push(diff_key);
            }
        }

        let success = details.missing_items.is_empty()
            && details.unexpected_items.is_empty()
            && details.errors.is_empty();

        let duration = start_time.elapsed();

        if success {
            info!(
                game_id = %game_id,
                matched_count = details.matched_items.len(),
                duration_ms = duration.as_millis(),
                "Configuration generation validation passed"
            );
        } else {
            warn!(
                game_id = %game_id,
                missing_count = details.missing_items.len(),
                unexpected_count = details.unexpected_items.len(),
                error_count = details.errors.len(),
                duration_ms = duration.as_millis(),
                "Configuration generation validation failed"
            );
        }

        Ok(ValidationResult {
            success,
            validation_type: ValidationType::ConfigFileGeneration,
            details,
            duration_ms: duration.as_millis() as u64,
            timestamp: start_time,
        })
    }

    /// Validate configuration file content on disk
    pub async fn validate_config_files(
        &self,
        game_id: &str,
        base_path: &Path,
    ) -> Result<ValidationResult> {
        let start_time = Instant::now();

        info!(
            game_id = %game_id,
            base_path = ?base_path,
            "Validating configuration file content"
        );

        let fixture = self
            .golden_files
            .get(game_id)
            .ok_or_else(|| anyhow::anyhow!("No golden file fixture for game: {}", game_id))?;

        let expected_files = &fixture.expected_files;
        let mut details = ValidationDetails {
            expected_count: expected_files.len(),
            actual_count: 0,
            matched_items: Vec::new(),
            missing_items: Vec::new(),
            unexpected_items: Vec::new(),
            errors: Vec::new(),
        };

        // Validate each expected file
        for expected_file in expected_files {
            let file_path = base_path.join(&expected_file.path);

            if file_path.exists() {
                details.actual_count += 1;

                match fs::read_to_string(&file_path) {
                    Ok(actual_content) => {
                        if self.compare_file_content(&expected_file.content, &actual_content) {
                            details.matched_items.push(expected_file.path.clone());
                        } else {
                            details
                                .errors
                                .push(format!("Content mismatch in file: {}", expected_file.path));
                        }
                    }
                    Err(e) => {
                        details
                            .errors
                            .push(format!("Failed to read file {}: {}", expected_file.path, e));
                    }
                }
            } else {
                details.missing_items.push(expected_file.path.clone());
            }
        }

        let success = details.missing_items.is_empty() && details.errors.is_empty();
        let duration = start_time.elapsed();

        if success {
            info!(
                game_id = %game_id,
                matched_count = details.matched_items.len(),
                duration_ms = duration.as_millis(),
                "Configuration file validation passed"
            );
        } else {
            warn!(
                game_id = %game_id,
                missing_count = details.missing_items.len(),
                error_count = details.errors.len(),
                duration_ms = duration.as_millis(),
                "Configuration file validation failed"
            );
        }

        Ok(ValidationResult {
            success,
            validation_type: ValidationType::ConfigFileContent,
            details,
            duration_ms: duration.as_millis() as u64,
            timestamp: start_time,
        })
    }

    /// Validate LED heartbeat pattern
    pub async fn validate_led_heartbeat(&self) -> Result<ValidationResult> {
        let start_time = Instant::now();

        info!("Starting LED heartbeat validation");

        let mut details = ValidationDetails {
            expected_count: self.led_validation.expected_pattern.repeat_count as usize,
            actual_count: 0,
            matched_items: Vec::new(),
            missing_items: Vec::new(),
            unexpected_items: Vec::new(),
            errors: Vec::new(),
        };

        // Simulate LED heartbeat validation
        // In a real implementation, this would interface with the LED system
        let validation_result = timeout(
            self.led_validation.validation_timeout,
            self.simulate_led_heartbeat_check(),
        )
        .await;

        let success = match validation_result {
            Ok(Ok(heartbeat_count)) => {
                details.actual_count = heartbeat_count;
                details
                    .matched_items
                    .push(format!("heartbeat_cycles:{}", heartbeat_count));

                heartbeat_count >= (self.led_validation.expected_pattern.repeat_count as usize)
            }
            Ok(Err(e)) => {
                details
                    .errors
                    .push(format!("LED heartbeat check failed: {}", e));
                false
            }
            Err(_) => {
                details
                    .errors
                    .push("LED heartbeat validation timed out".to_string());
                false
            }
        };

        let duration = start_time.elapsed();

        if success {
            info!(
                heartbeat_count = details.actual_count,
                duration_ms = duration.as_millis(),
                "LED heartbeat validation passed"
            );
        } else {
            warn!(
                error_count = details.errors.len(),
                duration_ms = duration.as_millis(),
                "LED heartbeat validation failed"
            );
        }

        Ok(ValidationResult {
            success,
            validation_type: ValidationType::LedHeartbeat,
            details,
            duration_ms: duration.as_millis() as u64,
            timestamp: start_time,
        })
    }

    /// Perform end-to-end validation
    pub async fn validate_end_to_end(
        &self,
        game_id: &str,
        base_path: &Path,
        actual_diffs: &[ConfigDiff],
    ) -> Result<ValidationResult> {
        let start_time = Instant::now();

        info!(
            game_id = %game_id,
            "Starting end-to-end validation"
        );

        // Validate configuration generation
        let config_gen_result = self
            .validate_config_generation(game_id, actual_diffs)
            .await?;

        // Validate configuration files
        let config_file_result = self.validate_config_files(game_id, base_path).await?;

        // Validate LED heartbeat
        let led_result = self.validate_led_heartbeat().await?;

        // Combine results
        let mut details = ValidationDetails {
            expected_count: config_gen_result.details.expected_count
                + config_file_result.details.expected_count
                + led_result.details.expected_count,
            actual_count: config_gen_result.details.actual_count
                + config_file_result.details.actual_count
                + led_result.details.actual_count,
            matched_items: Vec::new(),
            missing_items: Vec::new(),
            unexpected_items: Vec::new(),
            errors: Vec::new(),
        };

        // Merge all validation details
        details
            .matched_items
            .extend(config_gen_result.details.matched_items);
        details
            .matched_items
            .extend(config_file_result.details.matched_items);
        details
            .matched_items
            .extend(led_result.details.matched_items);

        details
            .missing_items
            .extend(config_gen_result.details.missing_items);
        details
            .missing_items
            .extend(config_file_result.details.missing_items);
        details
            .missing_items
            .extend(led_result.details.missing_items);

        details.errors.extend(config_gen_result.details.errors);
        details.errors.extend(config_file_result.details.errors);
        details.errors.extend(led_result.details.errors);

        let success = config_gen_result.success && config_file_result.success && led_result.success;

        let duration = start_time.elapsed();

        if success {
            info!(
                game_id = %game_id,
                total_matched = details.matched_items.len(),
                duration_ms = duration.as_millis(),
                "End-to-end validation passed"
            );
        } else {
            error!(
                game_id = %game_id,
                total_errors = details.errors.len(),
                duration_ms = duration.as_millis(),
                "End-to-end validation failed"
            );
        }

        Ok(ValidationResult {
            success,
            validation_type: ValidationType::EndToEnd,
            details,
            duration_ms: duration.as_millis() as u64,
            timestamp: start_time,
        })
    }

    /// Compare two configuration diffs
    fn compare_config_diffs(&self, expected: &ConfigDiff, actual: &ConfigDiff) -> bool {
        expected.file_path == actual.file_path
            && expected.section == actual.section
            && expected.key == actual.key
            && expected.operation == actual.operation
            && self.compare_diff_values(&expected.new_value, &actual.new_value)
    }

    /// Compare diff values (allowing for minor formatting differences)
    fn compare_diff_values(&self, expected: &str, actual: &str) -> bool {
        // Normalize whitespace and compare
        let expected_normalized = expected.trim().replace('\r', "");
        let actual_normalized = actual.trim().replace('\r', "");

        expected_normalized == actual_normalized
    }

    /// Compare file content (allowing for minor formatting differences)
    fn compare_file_content(&self, expected: &str, actual: &str) -> bool {
        // Normalize line endings and whitespace
        let expected_lines: Vec<&str> = expected.lines().map(|l| l.trim()).collect();
        let actual_lines: Vec<&str> = actual.lines().map(|l| l.trim()).collect();

        expected_lines == actual_lines
    }

    /// Simulate LED heartbeat check (placeholder for real LED interface)
    async fn simulate_led_heartbeat_check(&self) -> Result<usize> {
        let mut heartbeat_count = 0;
        let pattern = &self.led_validation.expected_pattern;

        for cycle in 0..pattern.repeat_count {
            for (step, led_state) in pattern.sequence.iter().enumerate() {
                debug!(
                    cycle = cycle,
                    step = step,
                    color = %led_state.color,
                    brightness = led_state.brightness,
                    "LED heartbeat step"
                );

                sleep(Duration::from_millis(led_state.duration_ms)).await;
            }

            heartbeat_count += 1;
        }

        info!(
            heartbeat_count = heartbeat_count,
            "LED heartbeat simulation completed"
        );
        Ok(heartbeat_count)
    }

    /// Get golden file fixture for a game
    pub fn get_golden_file(&self, game_id: &str) -> Option<&GoldenFileFixture> {
        self.golden_files.get(game_id)
    }

    /// Add or update golden file fixture
    pub fn set_golden_file(&mut self, game_id: String, fixture: GoldenFileFixture) {
        self.golden_files.insert(game_id, fixture);
    }
}

impl Default for ConfigValidationService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_validation_service_creation() {
        let service = ConfigValidationService::new();
        assert!(service.golden_files.contains_key("iracing"));
        assert!(service.golden_files.contains_key("acc"));
    }

    #[tokio::test]
    async fn test_config_generation_validation() {
        let service = ConfigValidationService::new();

        let actual_diffs = vec![ConfigDiff {
            file_path: "Documents/iRacing/app.ini".to_string(),
            section: Some("Telemetry".to_string()),
            key: "telemetryDiskFile".to_string(),
            old_value: None,
            new_value: "1".to_string(),
            operation: DiffOperation::Add,
        }];

        let result = service
            .validate_config_generation("iracing", &actual_diffs)
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.details.matched_items.len(), 1);
    }

    #[tokio::test]
    async fn test_config_file_validation() {
        let service = ConfigValidationService::new();
        let temp_dir = TempDir::new().unwrap();

        // Create test config file
        let config_dir = temp_dir.path().join("Documents/iRacing");
        fs::create_dir_all(&config_dir).unwrap();

        let config_file = config_dir.join("app.ini");
        fs::write(&config_file, "[Telemetry]\ntelemetryDiskFile=1\n").unwrap();

        let result = service
            .validate_config_files("iracing", temp_dir.path())
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.details.matched_items.len(), 1);
    }

    #[tokio::test]
    async fn test_led_heartbeat_validation() {
        let service = ConfigValidationService::new();

        let result = service.validate_led_heartbeat().await.unwrap();
        assert!(result.success);
        assert!(result.details.actual_count > 0);
    }

    #[test]
    fn test_diff_comparison() {
        let service = ConfigValidationService::new();

        let diff1 = ConfigDiff {
            file_path: "test.ini".to_string(),
            section: Some("Section".to_string()),
            key: "key".to_string(),
            old_value: None,
            new_value: "value".to_string(),
            operation: DiffOperation::Add,
        };

        let diff2 = diff1.clone();

        assert!(service.compare_config_diffs(&diff1, &diff2));
    }

    #[test]
    fn test_content_comparison() {
        let service = ConfigValidationService::new();

        let content1 = "[Section]\nkey=value\n";
        let content2 = "[Section]\r\nkey=value\r\n"; // Different line endings

        assert!(service.compare_file_content(content1, content2));
    }
}
