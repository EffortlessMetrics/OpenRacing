//! Golden file tests for game configuration writers
//!
//! Tests that compare generated configs against known fixtures
//! Requirements: GI-01 (one-click telemetry configuration)

// Test helper functions to replace unwrap
#[track_caller]
fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
    match r {
        Ok(v) => v,
        Err(e) => panic!("unexpected Err: {e:?}"),
    }
}

use racing_wheel_service::config_writers::{ACCConfigWriter, IRacingConfigWriter};
use racing_wheel_service::game_service::*;
use tempfile::TempDir;

/// Test data for golden file tests
struct TestGameConfig {
    #[allow(dead_code)]
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
            expected_diffs: vec![ConfigDiff {
                file_path: "Documents/iRacing/app.ini".to_string(),
                section: Some("Telemetry".to_string()),
                key: "telemetryDiskFile".to_string(),
                old_value: None,
                new_value: "1".to_string(),
                operation: DiffOperation::Add,
            }],
        }
    }

    fn acc_test_config() -> Self {
        Self {
            game_id: "acc".to_string(),
            config: TelemetryConfig {
                enabled: true,
                update_rate_hz: 100,
                output_method: "udp_broadcast".to_string(),
                output_target: "127.0.0.1:9000".to_string(),
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
            expected_diffs: vec![ConfigDiff {
                file_path: "Documents/Assetto Corsa Competizione/Config/broadcasting.json"
                    .to_string(),
                section: None,
                key: "entire_file".to_string(),
                old_value: None,
                new_value: must(serde_json::to_string_pretty(&serde_json::json!({
                    "updListenerPort": 9000,
                    "udpListenerPort": 9000,
                    "connectionPassword": "",
                    "commandPassword": ""
                }))),
                operation: DiffOperation::Add,
            }],
        }
    }
}

#[tokio::test]
async fn test_iracing_config_writer_golden() {
    let writer = IRacingConfigWriter;
    let test_config = TestGameConfig::iracing_test_config();
    let temp_dir = must(TempDir::new());

    // Test expected diffs match actual diffs
    let expected_diffs = must(writer.get_expected_diffs(&test_config.config));
    assert_eq!(expected_diffs, test_config.expected_diffs);

    // Test actual config writing
    let actual_diffs = must(writer.write_config(temp_dir.path(), &test_config.config));

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
    let writer = ACCConfigWriter;
    let test_config = TestGameConfig::acc_test_config();
    let temp_dir = must(TempDir::new());

    // Test expected diffs match actual diffs
    let expected_diffs = must(writer.get_expected_diffs(&test_config.config));
    assert_eq!(expected_diffs.len(), 1);

    // Test actual config writing
    let actual_diffs = must(writer.write_config(temp_dir.path(), &test_config.config));

    // Compare actual diffs with expected (ignoring file paths which will be different in temp dir)
    assert_eq!(actual_diffs.len(), expected_diffs.len());
    for (actual, expected) in actual_diffs.iter().zip(expected_diffs.iter()) {
        assert_eq!(actual.section, expected.section);
        assert_eq!(actual.key, expected.key);
        assert_eq!(actual.operation, expected.operation);

        // For JSON content, parse and compare structure
        if actual.key == "entire_file" {
            let actual_json: serde_json::Value = must(serde_json::from_str(&actual.new_value));
            let expected_json: serde_json::Value = must(serde_json::from_str(&expected.new_value));
            assert_eq!(actual_json, expected_json);
        } else {
            assert_eq!(actual.new_value, expected.new_value);
        }
    }
}

#[tokio::test]
async fn test_game_service_yaml_loading() {
    let service = must(GameService::new().await);

    // Test supported games loaded from YAML
    let supported_games = service.get_supported_games().await;
    assert!(supported_games.contains(&"iracing".to_string()));
    assert!(supported_games.contains(&"acc".to_string()));
    assert!(supported_games.contains(&"ac_rally".to_string()));
    assert!(supported_games.contains(&"ams2".to_string()));
    assert!(supported_games.contains(&"rfactor2".to_string()));
    assert!(supported_games.contains(&"eawrc".to_string()));
    assert_eq!(supported_games.len(), 6);
}

#[tokio::test]
async fn test_game_support_matrix_structure() {
    let service = must(GameService::new().await);

    // Test iRacing support structure
    let iracing_support = must(service.get_game_support("iracing").await);
    assert_eq!(iracing_support.name, "iRacing");
    assert_eq!(iracing_support.telemetry.method, "shared_memory");
    assert_eq!(iracing_support.telemetry.update_rate_hz, 60);
    assert_eq!(iracing_support.config_writer, "iracing");

    // Verify version information
    assert_eq!(iracing_support.versions.len(), 1);
    assert_eq!(iracing_support.versions[0].version, "2024.x");
    assert!(
        iracing_support.versions[0]
            .config_paths
            .contains(&"Documents/iRacing/app.ini".to_string())
    );
    assert!(
        iracing_support.versions[0]
            .executable_patterns
            .contains(&"iRacingSim64DX11.exe".to_string())
    );

    // Verify auto-detection config
    assert!(
        iracing_support
            .auto_detect
            .process_names
            .contains(&"iRacingSim64DX11.exe".to_string())
    );
    assert!(
        iracing_support
            .auto_detect
            .install_registry_keys
            .contains(&"HKEY_CURRENT_USER\\Software\\iRacing.com\\iRacing".to_string())
    );

    // Test ACC support structure
    let acc_support = must(service.get_game_support("acc").await);
    assert_eq!(acc_support.name, "Assetto Corsa Competizione");
    assert_eq!(acc_support.telemetry.method, "udp_broadcast");
    assert_eq!(acc_support.telemetry.update_rate_hz, 100);
    assert_eq!(acc_support.config_writer, "acc");

    // Verify version information
    assert_eq!(acc_support.versions.len(), 1);
    assert_eq!(acc_support.versions[0].version, "1.9.x");
    assert!(
        acc_support.versions[0]
            .config_paths
            .contains(&"Documents/Assetto Corsa Competizione/Config/broadcasting.json".to_string())
    );
}

#[tokio::test]
async fn test_telemetry_field_mapping_coverage() {
    let service = must(GameService::new().await);

    // Test iRacing field mapping coverage
    let iracing_mapping = must(service.get_telemetry_mapping("iracing").await);
    assert_eq!(
        iracing_mapping.ffb_scalar,
        Some("SteeringWheelTorque".to_string())
    );
    assert_eq!(iracing_mapping.rpm, Some("RPM".to_string()));
    assert_eq!(iracing_mapping.speed_ms, Some("Speed".to_string()));
    assert_eq!(iracing_mapping.slip_ratio, Some("LFslipRatio".to_string()));
    assert_eq!(iracing_mapping.gear, Some("Gear".to_string()));
    assert_eq!(iracing_mapping.flags, Some("SessionFlags".to_string()));
    assert_eq!(iracing_mapping.car_id, Some("CarIdx".to_string()));
    assert_eq!(iracing_mapping.track_id, Some("TrackId".to_string()));

    // Test ACC field mapping coverage
    let acc_mapping = must(service.get_telemetry_mapping("acc").await);
    assert_eq!(acc_mapping.ffb_scalar, Some("steerAngle".to_string()));
    assert_eq!(acc_mapping.rpm, Some("rpms".to_string()));
    assert_eq!(acc_mapping.speed_ms, Some("speedKmh".to_string()));
    assert_eq!(acc_mapping.slip_ratio, Some("wheelSlip".to_string()));
    assert_eq!(acc_mapping.gear, Some("gear".to_string()));
    assert_eq!(acc_mapping.flags, Some("flag".to_string()));
    assert_eq!(acc_mapping.car_id, Some("carModel".to_string()));
    assert_eq!(acc_mapping.track_id, Some("track".to_string()));
}

#[tokio::test]
async fn test_configuration_diff_generation() {
    let service = must(GameService::new().await);

    // Test iRacing expected diffs
    let iracing_config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "shared_memory".to_string(),
        output_target: "127.0.0.1:12345".to_string(),
        fields: vec!["ffb_scalar".to_string(), "rpm".to_string()],
    };

    let iracing_diffs = must(service.get_expected_diffs("iracing", &iracing_config).await);
    assert_eq!(iracing_diffs.len(), 1);
    assert_eq!(iracing_diffs[0].key, "telemetryDiskFile");
    assert_eq!(iracing_diffs[0].new_value, "1");
    assert_eq!(iracing_diffs[0].operation, DiffOperation::Add);

    // Test ACC expected diffs
    let acc_config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 100,
        output_method: "udp_broadcast".to_string(),
        output_target: "127.0.0.1:9000".to_string(),
        fields: vec!["ffb_scalar".to_string(), "rpm".to_string()],
    };

    let acc_diffs = must(service.get_expected_diffs("acc", &acc_config).await);
    assert_eq!(acc_diffs.len(), 1);
    assert_eq!(acc_diffs[0].key, "entire_file");
    assert_eq!(acc_diffs[0].operation, DiffOperation::Add);

    // Verify ACC JSON structure
    let acc_json: serde_json::Value = must(serde_json::from_str(&acc_diffs[0].new_value));
    assert_eq!(acc_json["updListenerPort"], 9000);
    assert_eq!(acc_json["udpListenerPort"], 9000);
    assert_eq!(acc_json["connectionPassword"], "");
    assert_eq!(acc_json["commandPassword"], "");
}

#[tokio::test]
async fn test_active_game_management() {
    let service = must(GameService::new().await);

    // Initially no active game
    assert_eq!(service.get_active_game().await, None);

    // Set active game
    must(service.set_active_game(Some("iracing".to_string())).await);
    assert_eq!(service.get_active_game().await, Some("iracing".to_string()));

    // Switch to different game
    must(service.set_active_game(Some("acc".to_string())).await);
    assert_eq!(service.get_active_game().await, Some("acc".to_string()));

    // Clear active game
    must(service.set_active_game(None).await);
    assert_eq!(service.get_active_game().await, None);
}

#[tokio::test]
async fn test_unsupported_game_handling() {
    let service = must(GameService::new().await);

    // Test unsupported game returns error
    let result = service.get_game_support("unsupported_game").await;
    assert!(result.is_err());
    assert!(
        result
            .err()
            .unwrap()
            .to_string()
            .contains("Unsupported game")
    );

    let mapping_result = service.get_telemetry_mapping("unsupported_game").await;
    assert!(mapping_result.is_err());
    assert!(
        mapping_result
            .err()
            .unwrap()
            .to_string()
            .contains("Unsupported game")
    );

    let config_result = service
        .get_expected_diffs(
            "unsupported_game",
            &TelemetryConfig {
                enabled: true,
                update_rate_hz: 60,
                output_method: "test".to_string(),
                output_target: "test".to_string(),
                fields: vec![],
            },
        )
        .await;
    assert!(config_result.is_err());
    assert!(
        config_result
            .err()
            .unwrap()
            .to_string()
            .contains("No config writer for game")
    );
}

#[tokio::test]
async fn test_end_to_end_telemetry_configuration() {
    let service = must(GameService::new().await);
    let temp_dir = must(TempDir::new());

    // Test iRacing end-to-end configuration
    let iracing_diffs = must(
        service
            .configure_telemetry("iracing", temp_dir.path())
            .await,
    );
    assert_eq!(iracing_diffs.len(), 1);
    assert_eq!(iracing_diffs[0].key, "telemetryDiskFile");
    assert_eq!(iracing_diffs[0].new_value, "1");

    // Test ACC end-to-end configuration
    let acc_diffs = must(service.configure_telemetry("acc", temp_dir.path()).await);
    assert_eq!(acc_diffs.len(), 1);
    assert_eq!(acc_diffs[0].key, "entire_file");

    // Verify ACC JSON is valid
    let acc_json: serde_json::Value = must(serde_json::from_str(&acc_diffs[0].new_value));
    assert!(acc_json.is_object());
    assert!(acc_json.get("updListenerPort").is_some());
    assert!(acc_json.get("udpListenerPort").is_some());
}
