pub use racing_wheel_telemetry_config_writers::*;

#[cfg(test)]
mod tests {
    use super::*;
    type TestResult = Result<(), Box<dyn std::error::Error>>;
    #[test]
    fn config_writer_factories_is_non_empty() {
        assert!(!config_writer_factories().is_empty());
    }
    #[test]
    fn config_writer_factories_contains_known_game_ids() {
        let ids: Vec<&str> = config_writer_factories()
            .iter()
            .map(|(id, _)| *id)
            .collect();
        for expected in ["iracing", "acc", "ams2", "rfactor2", "eawrc"] {
            assert!(ids.contains(&expected), "missing: {}", expected);
        }
    }
    #[test]
    fn config_writer_factories_does_not_contain_unknown() {
        let ids: Vec<&str> = config_writer_factories()
            .iter()
            .map(|(id, _)| *id)
            .collect();
        assert!(!ids.contains(&"__no_such_game__"));
    }
    #[test]
    fn telemetry_config_serde_round_trip() -> TestResult {
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:9999".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let json = serde_json::to_string(&config)?;
        let decoded: TelemetryConfig = serde_json::from_str(&json)?;
        assert_eq!(decoded.enabled, config.enabled);
        assert_eq!(decoded.update_rate_hz, config.update_rate_hz);
        Ok(())
    }

    // --- New tests below ---

    #[test]
    fn telemetry_config_serde_yaml_round_trip() -> TestResult {
        let config = TelemetryConfig {
            enabled: false,
            update_rate_hz: 120,
            output_method: "shared_memory".to_string(),
            output_target: "127.0.0.1:20778".to_string(),
            fields: vec![
                "ffb_scalar".to_string(),
                "rpm".to_string(),
                "speed_ms".to_string(),
            ],
            enable_high_rate_iracing_360hz: true,
        };
        let yaml_str = serde_yaml::to_string(&config)?;
        let decoded: TelemetryConfig = serde_yaml::from_str(&yaml_str)?;
        assert_eq!(decoded.enabled, config.enabled);
        assert_eq!(decoded.update_rate_hz, config.update_rate_hz);
        assert_eq!(decoded.output_method, config.output_method);
        assert_eq!(decoded.output_target, config.output_target);
        assert_eq!(decoded.fields, config.fields);
        assert_eq!(
            decoded.enable_high_rate_iracing_360hz,
            config.enable_high_rate_iracing_360hz
        );
        Ok(())
    }

    #[test]
    fn telemetry_config_serde_all_fields_preserved() -> TestResult {
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 360,
            output_method: "udp_broadcast".to_string(),
            output_target: "192.168.1.100:5300".to_string(),
            fields: vec![
                "ffb_scalar".to_string(),
                "rpm".to_string(),
                "speed_ms".to_string(),
                "slip_ratio".to_string(),
                "gear".to_string(),
                "flags".to_string(),
                "car_id".to_string(),
                "track_id".to_string(),
            ],
            enable_high_rate_iracing_360hz: true,
        };
        let json = serde_json::to_string(&config)?;
        let decoded: TelemetryConfig = serde_json::from_str(&json)?;
        assert!(decoded.enabled);
        assert_eq!(decoded.update_rate_hz, 360);
        assert_eq!(decoded.output_method, "udp_broadcast");
        assert_eq!(decoded.output_target, "192.168.1.100:5300");
        assert_eq!(decoded.fields.len(), 8);
        assert!(decoded.enable_high_rate_iracing_360hz);
        Ok(())
    }

    #[test]
    fn telemetry_config_empty_fields_round_trip() -> TestResult {
        let config = TelemetryConfig {
            enabled: false,
            update_rate_hz: 0,
            output_method: String::new(),
            output_target: String::new(),
            fields: vec![],
            enable_high_rate_iracing_360hz: false,
        };
        let json = serde_json::to_string(&config)?;
        let decoded: TelemetryConfig = serde_json::from_str(&json)?;
        assert!(!decoded.enabled);
        assert_eq!(decoded.update_rate_hz, 0);
        assert!(decoded.fields.is_empty());
        Ok(())
    }

    #[test]
    fn telemetry_config_high_rate_defaults_to_false() -> TestResult {
        let json = r#"{
            "enabled": true,
            "update_rate_hz": 60,
            "output_method": "udp",
            "output_target": "127.0.0.1:9999",
            "fields": []
        }"#;
        let decoded: TelemetryConfig = serde_json::from_str(json)?;
        assert!(!decoded.enable_high_rate_iracing_360hz);
        Ok(())
    }

    #[test]
    fn config_diff_serde_round_trip() -> TestResult {
        let diff = ConfigDiff {
            file_path: "Documents/iRacing/app.ini".to_string(),
            section: Some("Telemetry".to_string()),
            key: "telemetryDiskFile".to_string(),
            old_value: Some("0".to_string()),
            new_value: "1".to_string(),
            operation: DiffOperation::Modify,
        };
        let json = serde_json::to_string(&diff)?;
        let decoded: ConfigDiff = serde_json::from_str(&json)?;
        assert_eq!(decoded, diff);
        Ok(())
    }

    #[test]
    fn config_diff_add_operation_round_trip() -> TestResult {
        let diff = ConfigDiff {
            file_path: "config.json".to_string(),
            section: None,
            key: "udpEnabled".to_string(),
            old_value: None,
            new_value: "true".to_string(),
            operation: DiffOperation::Add,
        };
        let json = serde_json::to_string(&diff)?;
        let decoded: ConfigDiff = serde_json::from_str(&json)?;
        assert_eq!(decoded.operation, DiffOperation::Add);
        assert!(decoded.old_value.is_none());
        assert!(decoded.section.is_none());
        Ok(())
    }

    #[test]
    fn config_diff_remove_operation_round_trip() -> TestResult {
        let diff = ConfigDiff {
            file_path: "settings.ini".to_string(),
            section: Some("Network".to_string()),
            key: "legacyPort".to_string(),
            old_value: Some("8080".to_string()),
            new_value: String::new(),
            operation: DiffOperation::Remove,
        };
        let json = serde_json::to_string(&diff)?;
        let decoded: ConfigDiff = serde_json::from_str(&json)?;
        assert_eq!(decoded.operation, DiffOperation::Remove);
        assert_eq!(decoded.old_value, Some("8080".to_string()));
        Ok(())
    }

    #[test]
    fn diff_operation_serde_round_trip_all_variants() -> TestResult {
        for op in [
            DiffOperation::Add,
            DiffOperation::Modify,
            DiffOperation::Remove,
        ] {
            let json = serde_json::to_string(&op)?;
            let decoded: DiffOperation = serde_json::from_str(&json)?;
            assert_eq!(decoded, op);
        }
        Ok(())
    }

    #[test]
    fn config_writer_factory_ids_are_unique() {
        let factories = config_writer_factories();
        let mut seen = std::collections::HashSet::new();
        for (id, _) in factories {
            assert!(
                seen.insert(*id),
                "duplicate config writer factory id: {}",
                id
            );
        }
    }

    #[test]
    fn each_config_writer_factory_produces_a_writer() {
        for (id, factory) in config_writer_factories() {
            let _writer = factory();
            // If this doesn't panic, the factory works
            assert!(!id.is_empty(), "factory has empty id");
        }
    }

    #[test]
    fn config_writer_factories_match_matrix_game_writers() -> TestResult {
        let matrix = crate::load_default_matrix()?;
        let factory_ids: std::collections::HashSet<&str> = config_writer_factories()
            .iter()
            .map(|(id, _)| *id)
            .collect();
        for (game_id, game) in &matrix.games {
            assert!(
                factory_ids.contains(game.config_writer.as_str()),
                "game {} references config_writer '{}' which has no factory",
                game_id,
                game.config_writer
            );
        }
        Ok(())
    }

    #[test]
    fn telemetry_config_with_ipv6_target_round_trip() -> TestResult {
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "[::1]:9999".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let json = serde_json::to_string(&config)?;
        let decoded: TelemetryConfig = serde_json::from_str(&json)?;
        assert_eq!(decoded.output_target, "[::1]:9999");
        Ok(())
    }

    #[test]
    fn telemetry_config_clone_is_equal() {
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 100,
            output_method: "shared_memory".to_string(),
            output_target: "127.0.0.1:12345".to_string(),
            fields: vec!["ffb_scalar".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let cloned = config.clone();
        assert_eq!(cloned.enabled, config.enabled);
        assert_eq!(cloned.update_rate_hz, config.update_rate_hz);
        assert_eq!(cloned.output_method, config.output_method);
        assert_eq!(cloned.output_target, config.output_target);
        assert_eq!(cloned.fields, config.fields);
    }

    #[test]
    fn config_diff_equality() {
        let diff1 = ConfigDiff {
            file_path: "a.ini".to_string(),
            section: Some("S".to_string()),
            key: "k".to_string(),
            old_value: None,
            new_value: "v".to_string(),
            operation: DiffOperation::Add,
        };
        let diff2 = diff1.clone();
        assert_eq!(diff1, diff2);
    }

    #[test]
    fn config_diff_inequality_on_operation() {
        let diff1 = ConfigDiff {
            file_path: "a.ini".to_string(),
            section: None,
            key: "k".to_string(),
            old_value: None,
            new_value: "v".to_string(),
            operation: DiffOperation::Add,
        };
        let diff2 = ConfigDiff {
            operation: DiffOperation::Modify,
            ..diff1.clone()
        };
        assert_ne!(diff1, diff2);
    }
}
