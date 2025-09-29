//! Golden file tests for game configuration writers
//! 
//! Tests that compare generated configs against known fixtures
//! Requirements: GI-01 (one-click telemetry configuration)

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_service::*;
    use crate::config_writers::{IRacingConfigWriter, ACCConfigWriter};
    use std::path::PathBuf;
    use tempfile::TempDir;
    use serde_json;

    /// Test data for golden file tests
    struct TestGameConfig {
        game_id: String,
        config: TelemetryConfig,
        expected_diffs: Vec<ConfigDiff>,
    }

    impl TestGameConfig {
        fn iracing_test_config() -> Self {
            Self {
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
                        "slip_ratio".to_string(),
                        "gear".to_string(),
                        "car_id".to_string(),
                        "track_id".to_string(),
                    ],
                },
                expected_diffs: vec![
                    ConfigDiff {
                        file_path: "Documents/iRacing/app.ini".to_string(),
                        section: Some("Telemetry".to_string()),
                        key: "telemetryDiskFile".to_string(),
                        old_value: None,
                        new_value: "1".to_string(),
                        operation: DiffOperation::Add,
                    },
                ],
            }
        }

        fn acc_test_config() -> Self {
            Self {
                game_id: "acc".to_string(),
                config: TelemetryConfig {
                    enabled: true,
                    update_rate_hz: 100,
                    output_method: "udp_broadcast".to_string(),
                    output_target: "127.0.0.1:9996".to_string(),
                    fields: vec![
                        "ffb_scalar".to_string(),
                        "rpm".to_string(),
                        "speed_ms".to_string(),
                        "slip_ratio".to_string(),
                        "gear".to_string(),
                        "car_id".to_string(),
                        "track_id".to_string(),
                    ],
                },
                expected_diffs: vec![
                    ConfigDiff {
                        file_path: "Documents/Assetto Corsa Competizione/Config/broadcasting.json".to_string(),
                        section: None,
                        key: "entire_file".to_string(),
                        old_value: None,
                        new_value: serde_json::to_string_pretty(&serde_json::json!({
                            "updListenerPort": 9996,
                            "connectionId": "",
                            "broadcastingPort": 9000,
                            "commandPassword": "",
                            "updateRateHz": 100
                        })).unwrap(),
                        operation: DiffOperation::Add,
                    },
                ],
            }
        }
    }

    #[tokio::test]
    async fn test_iracing_config_writer_golden() {
        let writer = IRacingConfigWriter::new();
        let test_config = TestGameConfig::iracing_test_config();
        let temp_dir = TempDir::new().unwrap();
        
        // Test expected diffs match actual diffs
        let expected_diffs = writer.get_expected_diffs(&test_config.config).unwrap();
        assert_eq!(expected_diffs, test_config.expected_diffs);
        
        // Test actual config writing
        let actual_diffs = writer.write_config(temp_dir.path(), &test_config.config).unwrap();
        
        // Compare actual diffs with expected (ignoring file paths which will be different in temp dir)
        assert_eq!(actual_diffs.len(), expected_diffs.len());
        for (actual, expected) in actual_diffs.iter().zip(expected_diffs.iter()) {
            assert_eq!(actual.section, expected.section);
            assert_eq!(actual.key, expected.key);
            assert_eq!(actual.new_value, expected.new_value);
            assert_eq!(actual.operation, expected.operation);
        }
    }

    #[tokio::test]
    async fn test_acc_config_writer_golden() {
        let writer = ACCConfigWriter::new();
        let test_config = TestGameConfig::acc_test_config();
        let temp_dir = TempDir::new().unwrap();
        
        // Test expected diffs match actual diffs
        let expected_diffs = writer.get_expected_diffs(&test_config.config).unwrap();
        assert_eq!(expected_diffs.len(), 1);
        
        // Test actual config writing
        let actual_diffs = writer.write_config(temp_dir.path(), &test_config.config).unwrap();
        
        // Compare actual diffs with expected (ignoring file paths which will be different in temp dir)
        assert_eq!(actual_diffs.len(), expected_diffs.len());
        for (actual, expected) in actual_diffs.iter().zip(expected_diffs.iter()) {
            assert_eq!(actual.section, expected.section);
            assert_eq!(actual.key, expected.key);
            assert_eq!(actual.operation, expected.operation);
            
            // For JSON content, parse and compare structure
            if actual.key == "entire_file" {
                let actual_json: serde_json::Value = serde_json::from_str(&actual.new_value).unwrap();
                let expected_json: serde_json::Value = serde_json::from_str(&expected.new_value).unwrap();
                assert_eq!(actual_json, expected_json);
            } else {
                assert_eq!(actual.new_value, expected.new_value);
            }
        }
    }

    #[tokio::test]
    async fn test_game_service_integration() {
        let service = GameService::new().await.unwrap();
        
        // Test supported games
        let supported_games = service.get_supported_games().await;
        assert!(supported_games.contains(&"iracing".to_string()));
        assert!(supported_games.contains(&"acc".to_string()));
        
        // Test game support information
        let iracing_support = service.get_game_support("iracing").await.unwrap();
        assert_eq!(iracing_support.name, "iRacing");
        assert_eq!(iracing_support.telemetry.method, "shared_memory");
        assert_eq!(iracing_support.telemetry.update_rate_hz, 60);
        
        let acc_support = service.get_game_support("acc").await.unwrap();
        assert_eq!(acc_support.name, "Assetto Corsa Competizione");
        assert_eq!(acc_support.telemetry.method, "udp_broadcast");
        assert_eq!(acc_support.telemetry.update_rate_hz, 100);
    }

    #[tokio::test]
    async fn test_telemetry_field_mapping() {
        let service = GameService::new().await.unwrap();
        
        // Test iRacing field mapping
        let iracing_mapping = service.get_telemetry_mapping("iracing").await.unwrap();
        assert_eq!(iracing_mapping.ffb_scalar, Some("SteeringWheelTorque".to_string()));
        assert_eq!(iracing_mapping.rpm, Some("RPM".to_string()));
        assert_eq!(iracing_mapping.speed_ms, Some("Speed".to_string()));
        assert_eq!(iracing_mapping.slip_ratio, Some("LFslipRatio".to_string()));
        assert_eq!(iracing_mapping.gear, Some("Gear".to_string()));
        assert_eq!(iracing_mapping.car_id, Some("CarIdx".to_string()));
        assert_eq!(iracing_mapping.track_id, Some("TrackId".to_string()));
        
        // Test ACC field mapping
        let acc_mapping = service.get_telemetry_mapping("acc").await.unwrap();
        assert_eq!(acc_mapping.ffb_scalar, Some("steerAngle".to_string()));
        assert_eq!(acc_mapping.rpm, Some("rpms".to_string()));
        assert_eq!(acc_mapping.speed_ms, Some("speedKmh".to_string()));
        assert_eq!(acc_mapping.slip_ratio, Some("wheelSlip".to_string()));
        assert_eq!(acc_mapping.gear, Some("gear".to_string()));
        assert_eq!(acc_mapping.car_id, Some("carModel".to_string()));
        assert_eq!(acc_mapping.track_id, Some("track".to_string()));
    }

    #[tokio::test]
    async fn test_active_game_switching() {
        let service = GameService::new().await.unwrap();
        
        // Initially no active game
        assert_eq!(service.get_active_game().await, None);
        
        // Set active game
        service.set_active_game(Some("iracing".to_string())).await.unwrap();
        assert_eq!(service.get_active_game().await, Some("iracing".to_string()));
        
        // Switch to different game
        service.set_active_game(Some("acc".to_string())).await.unwrap();
        assert_eq!(service.get_active_game().await, Some("acc".to_string()));
        
        // Clear active game
        service.set_active_game(None).await.unwrap();
        assert_eq!(service.get_active_game().await, None);
    }

    #[tokio::test]
    async fn test_unsupported_game_error() {
        let service = GameService::new().await.unwrap();
        
        // Test unsupported game returns error
        let result = service.get_game_support("unsupported_game").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unsupported game"));
        
        let mapping_result = service.get_telemetry_mapping("unsupported_game").await;
        assert!(mapping_result.is_err());
        assert!(mapping_result.unwrap_err().to_string().contains("Unsupported game"));
    }
}