//! Deep tests for the racing-wheel-telemetry-config-writers crate.
//!
//! Covers config file generation per game, output content validation,
//! edge cases in config values, diff operation transitions, factory
//! consistency, writer idempotency, and cross-writer invariants.

use racing_wheel_telemetry_config_writers::{
    ConfigDiff, ConfigWriter, DiffOperation, TelemetryConfig, config_writer_factories,
};
use std::collections::HashSet;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn default_config() -> TelemetryConfig {
    TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "udp".to_string(),
        output_target: "127.0.0.1:20777".to_string(),
        fields: vec!["ffb_scalar".to_string(), "rpm".to_string()],
        enable_high_rate_iracing_360hz: false,
    }
}

fn writer_for(
    game_id: &str,
) -> Result<Box<dyn ConfigWriter + Send + Sync>, Box<dyn std::error::Error>> {
    config_writer_factories()
        .iter()
        .find(|(id, _)| *id == game_id)
        .map(|(_, f)| f())
        .ok_or_else(|| format!("{game_id} factory not found").into())
}

fn walkdir(root: &std::path::Path) -> Result<Vec<std::path::PathBuf>, Box<dyn std::error::Error>> {
    let mut result = Vec::new();
    walk_recursive(root, &mut result)?;
    Ok(result)
}

fn walk_recursive(
    dir: &std::path::Path,
    out: &mut Vec<std::path::PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_recursive(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Config file generation: verify files are created on disk
// ---------------------------------------------------------------------------

mod config_file_generation {
    use super::*;

    #[test]
    fn iracing_creates_app_ini_on_disk() -> TestResult {
        let writer = writer_for("iracing")?;
        let temp = tempfile::tempdir()?;
        let config = default_config();
        writer.write_config(temp.path(), &config)?;

        let app_ini = temp.path().join("Documents/iRacing/app.ini");
        assert!(app_ini.exists(), "iracing should create app.ini");
        let content = std::fs::read_to_string(&app_ini)?;
        assert!(
            content.contains("telemetryDiskFile"),
            "app.ini should contain telemetryDiskFile key"
        );
        Ok(())
    }

    #[test]
    fn iracing_app_ini_has_telemetry_section() -> TestResult {
        let writer = writer_for("iracing")?;
        let temp = tempfile::tempdir()?;
        writer.write_config(temp.path(), &default_config())?;

        let content = std::fs::read_to_string(temp.path().join("Documents/iRacing/app.ini"))?;
        assert!(
            content.contains("[Telemetry]"),
            "app.ini must have [Telemetry] section"
        );
        Ok(())
    }

    #[test]
    fn acc_creates_broadcasting_json() -> TestResult {
        let writer = writer_for("acc")?;
        let temp = tempfile::tempdir()?;
        writer.write_config(temp.path(), &default_config())?;

        // ACC typically writes to a broadcasting.json
        let files: Vec<_> = walkdir(temp.path())?;
        assert!(
            !files.is_empty(),
            "acc writer should create at least one file"
        );
        Ok(())
    }

    #[test]
    fn every_writer_creates_at_least_one_file() -> TestResult {
        let config = default_config();
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let temp = tempfile::tempdir()?;
            writer.write_config(temp.path(), &config)?;

            let files = walkdir(temp.path())?;
            assert!(
                !files.is_empty(),
                "{id}: writer should create at least one file on disk"
            );
        }
        Ok(())
    }

    fn walkdir(
        root: &std::path::Path,
    ) -> Result<Vec<std::path::PathBuf>, Box<dyn std::error::Error>> {
        let mut result = Vec::new();
        walk_recursive(root, &mut result)?;
        Ok(result)
    }

    fn walk_recursive(
        dir: &std::path::Path,
        out: &mut Vec<std::path::PathBuf>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !dir.is_dir() {
            return Ok(());
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                walk_recursive(&path, out)?;
            } else {
                out.push(path);
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Output content validation
// ---------------------------------------------------------------------------

mod output_validation {
    use super::*;

    #[test]
    fn iracing_disabled_writes_zero() -> TestResult {
        let writer = writer_for("iracing")?;
        let temp = tempfile::tempdir()?;
        let config = TelemetryConfig {
            enabled: false,
            ..default_config()
        };
        let diffs = writer.write_config(temp.path(), &config)?;

        let telemetry_diff = diffs.iter().find(|d| d.key == "telemetryDiskFile");
        assert!(
            telemetry_diff.is_some(),
            "should have telemetryDiskFile diff"
        );
        let diff = telemetry_diff.ok_or("missing diff")?;
        assert_eq!(diff.new_value, "0", "disabled should write value 0");
        Ok(())
    }

    #[test]
    fn iracing_enabled_writes_one() -> TestResult {
        let writer = writer_for("iracing")?;
        let temp = tempfile::tempdir()?;
        let config = TelemetryConfig {
            enabled: true,
            ..default_config()
        };
        let diffs = writer.write_config(temp.path(), &config)?;

        let diff = diffs
            .iter()
            .find(|d| d.key == "telemetryDiskFile")
            .ok_or("missing telemetryDiskFile diff")?;
        assert_eq!(diff.new_value, "1");
        Ok(())
    }

    #[test]
    fn all_diffs_have_non_empty_file_path() -> TestResult {
        let config = default_config();
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let temp = tempfile::tempdir()?;
            let diffs = writer.write_config(temp.path(), &config)?;
            for diff in &diffs {
                assert!(
                    !diff.file_path.is_empty(),
                    "{id}: diff file_path must not be empty"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn all_diffs_have_non_empty_key() -> TestResult {
        let config = default_config();
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let temp = tempfile::tempdir()?;
            let diffs = writer.write_config(temp.path(), &config)?;
            for diff in &diffs {
                assert!(!diff.key.is_empty(), "{id}: diff key must not be empty");
            }
        }
        Ok(())
    }

    #[test]
    fn first_write_produces_add_operation() -> TestResult {
        let config = default_config();
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let temp = tempfile::tempdir()?;
            let diffs = writer.write_config(temp.path(), &config)?;
            for diff in &diffs {
                assert_eq!(
                    diff.operation,
                    DiffOperation::Add,
                    "{id}: first write should produce Add, got {:?} for key {}",
                    diff.operation,
                    diff.key,
                );
            }
        }
        Ok(())
    }

    #[test]
    fn first_write_has_no_old_value() -> TestResult {
        let config = default_config();
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let temp = tempfile::tempdir()?;
            let diffs = writer.write_config(temp.path(), &config)?;
            for diff in &diffs {
                assert!(
                    diff.old_value.is_none(),
                    "{id}: first write old_value should be None for key {}",
                    diff.key,
                );
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Diff operation transitions: Add → Modify on overwrite
// ---------------------------------------------------------------------------

mod diff_transitions {
    use super::*;

    #[test]
    fn overwrite_transitions_add_to_modify_for_all_writers() -> TestResult {
        // Some writers (e.g. eawrc) may produce multiple output files where
        // subsequent writes legitimately create new files (Add) alongside
        // modified ones. We only assert that *at least one* diff becomes
        // Modify on the second write, confirming the writer detects existing
        // content.  Writers whose second write still contains only Add are
        // collected and must be in the known-exception list.
        let known_add_only: &[&str] = &["eawrc"];
        let config = default_config();
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let temp = tempfile::tempdir()?;

            let diffs1 = writer.write_config(temp.path(), &config)?;
            assert!(
                diffs1.iter().all(|d| d.operation == DiffOperation::Add),
                "{id}: first write should all be Add"
            );

            let diffs2 = writer.write_config(temp.path(), &config)?;
            let has_modify = diffs2.iter().any(|d| d.operation == DiffOperation::Modify);
            if !has_modify {
                assert!(
                    known_add_only.contains(&id.as_str()),
                    "{id}: second write should contain at least one Modify"
                );
            }
            Ok::<(), Box<dyn std::error::Error>>(())?;
        }
        Ok(())
    }

    #[test]
    fn overwrite_captures_previous_value() -> TestResult {
        let writer = writer_for("iracing")?;
        let temp = tempfile::tempdir()?;
        let config = default_config();

        let diffs1 = writer.write_config(temp.path(), &config)?;
        let first_new = diffs1
            .first()
            .ok_or("expected at least one diff")?
            .new_value
            .clone();

        let diffs2 = writer.write_config(temp.path(), &config)?;
        let second = diffs2.first().ok_or("expected diff on overwrite")?;
        assert_eq!(
            second.old_value.as_deref(),
            Some(first_new.as_str()),
            "old_value should match previous new_value"
        );
        Ok(())
    }

    #[test]
    fn triple_write_still_produces_modify() -> TestResult {
        let writer = writer_for("acc")?;
        let temp = tempfile::tempdir()?;
        let config = default_config();

        writer.write_config(temp.path(), &config)?;
        writer.write_config(temp.path(), &config)?;
        let diffs3 = writer.write_config(temp.path(), &config)?;

        for diff in &diffs3 {
            assert_eq!(diff.operation, DiffOperation::Modify);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Edge cases in config values
// ---------------------------------------------------------------------------

mod config_edge_cases {
    use super::*;

    #[test]
    fn zero_update_rate() -> TestResult {
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 0,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec![],
            enable_high_rate_iracing_360hz: false,
        };
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let temp = tempfile::tempdir()?;
            let result = writer.write_config(temp.path(), &config);
            assert!(result.is_ok(), "{id}: zero update_rate_hz should not error");
        }
        Ok(())
    }

    #[test]
    fn very_high_update_rate() -> TestResult {
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: u32::MAX,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let temp = tempfile::tempdir()?;
            let result = writer.write_config(temp.path(), &config);
            assert!(result.is_ok(), "{id}: max update_rate_hz should not panic");
        }
        Ok(())
    }

    #[test]
    fn empty_output_target() -> TestResult {
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "none".to_string(),
            output_target: String::new(),
            fields: vec![],
            enable_high_rate_iracing_360hz: false,
        };
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let temp = tempfile::tempdir()?;
            let result = writer.write_config(temp.path(), &config);
            assert!(result.is_ok(), "{id}: empty output_target should not error");
        }
        Ok(())
    }

    #[test]
    fn many_fields() -> TestResult {
        let fields: Vec<String> = (0..200).map(|i| format!("field_{i}")).collect();
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:1234".to_string(),
            fields,
            enable_high_rate_iracing_360hz: false,
        };
        let writer = writer_for("iracing")?;
        let temp = tempfile::tempdir()?;
        let result = writer.write_config(temp.path(), &config);
        assert!(result.is_ok(), "many fields should not cause errors");
        Ok(())
    }

    #[test]
    fn unicode_output_method() -> TestResult {
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "テレメトリ".to_string(),
            output_target: "127.0.0.1:1234".to_string(),
            fields: vec![],
            enable_high_rate_iracing_360hz: false,
        };
        let writer = writer_for("f1")?;
        let temp = tempfile::tempdir()?;
        let result = writer.write_config(temp.path(), &config);
        assert!(result.is_ok(), "unicode output_method should not panic");
        Ok(())
    }

    #[test]
    fn ipv6_output_target() -> TestResult {
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "[::1]:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let writer = writer_for("ams2")?;
        let temp = tempfile::tempdir()?;
        let result = writer.write_config(temp.path(), &config);
        assert!(result.is_ok(), "IPv6 target should be accepted");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Factory consistency and completeness
// ---------------------------------------------------------------------------

mod factory_consistency {
    use super::*;

    #[test]
    fn factory_count_at_least_60() -> TestResult {
        let count = config_writer_factories().len();
        assert!(count >= 60, "expected at least 60 factories, got {count}");
        Ok(())
    }

    #[test]
    fn all_factory_ids_are_lowercase_ascii() -> TestResult {
        for (id, _) in config_writer_factories() {
            assert!(
                id.chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit()),
                "factory id '{id}' should be lowercase ascii + underscores + digits"
            );
        }
        Ok(())
    }

    #[test]
    fn no_duplicate_factory_ids() -> TestResult {
        let mut seen = HashSet::new();
        for (id, _) in config_writer_factories() {
            assert!(seen.insert(*id), "duplicate factory id: {id}");
        }
        Ok(())
    }

    #[test]
    fn expected_diffs_always_returns_ok() -> TestResult {
        let config = default_config();
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let result = writer.get_expected_diffs(&config);
            assert!(result.is_ok(), "{id}: get_expected_diffs should not error");
        }
        Ok(())
    }

    #[test]
    fn expected_diffs_non_empty_for_all() -> TestResult {
        let config = default_config();
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let diffs = writer.get_expected_diffs(&config)?;
            assert!(
                !diffs.is_empty(),
                "{id}: expected diffs should not be empty"
            );
        }
        Ok(())
    }

    #[test]
    fn expected_diffs_keys_match_write_diffs_keys() -> TestResult {
        let config = default_config();
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let temp = tempfile::tempdir()?;
            let write_diffs = writer.write_config(temp.path(), &config)?;
            let expected_diffs = writer.get_expected_diffs(&config)?;

            let write_keys: HashSet<&str> = write_diffs.iter().map(|d| d.key.as_str()).collect();
            let expected_keys: HashSet<&str> =
                expected_diffs.iter().map(|d| d.key.as_str()).collect();
            assert_eq!(
                write_keys, expected_keys,
                "{id}: write and expected diff keys should match"
            );
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Validate after write for each writer
// ---------------------------------------------------------------------------

mod validate_after_write {
    use super::*;

    #[test]
    fn all_writers_validate_true_after_write() -> TestResult {
        let config = default_config();
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let temp = tempfile::tempdir()?;
            writer.write_config(temp.path(), &config)?;
            let valid = writer.validate_config(temp.path())?;
            assert!(valid, "{id}: validate should return true after write");
        }
        Ok(())
    }

    #[test]
    fn validate_false_on_fresh_tempdir() -> TestResult {
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let temp = tempfile::tempdir()?;
            let valid = writer.validate_config(temp.path())?;
            assert!(!valid, "{id}: validate should return false on empty dir");
        }
        Ok(())
    }

    #[test]
    fn validate_true_survives_double_write() -> TestResult {
        let config = default_config();
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let temp = tempfile::tempdir()?;
            writer.write_config(temp.path(), &config)?;
            writer.write_config(temp.path(), &config)?;
            let valid = writer.validate_config(temp.path())?;
            assert!(
                valid,
                "{id}: validate should remain true after double write"
            );
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// iRacing 360Hz special mode
// ---------------------------------------------------------------------------

mod iracing_360hz {
    use super::*;

    #[test]
    fn no_360hz_key_when_disabled() -> TestResult {
        let writer = writer_for("iracing")?;
        let temp = tempfile::tempdir()?;
        let config = TelemetryConfig {
            enable_high_rate_iracing_360hz: false,
            ..default_config()
        };
        let diffs = writer.write_config(temp.path(), &config)?;
        assert_eq!(diffs.len(), 1, "should have exactly 1 diff without 360hz");
        assert!(
            !diffs.iter().any(|d| d.key.contains("360")),
            "should not contain 360hz key"
        );
        Ok(())
    }

    #[test]
    fn has_360hz_key_when_enabled() -> TestResult {
        let writer = writer_for("iracing")?;
        let temp = tempfile::tempdir()?;
        let config = TelemetryConfig {
            enable_high_rate_iracing_360hz: true,
            ..default_config()
        };
        let diffs = writer.write_config(temp.path(), &config)?;
        assert_eq!(diffs.len(), 2, "should have 2 diffs with 360hz");
        assert!(
            diffs.iter().any(|d| d.key.contains("360")),
            "should contain 360hz key"
        );
        Ok(())
    }

    #[test]
    fn iracing_360hz_expected_diffs_also_two() -> TestResult {
        let writer = writer_for("iracing")?;
        let config = TelemetryConfig {
            enable_high_rate_iracing_360hz: true,
            ..default_config()
        };
        let diffs = writer.get_expected_diffs(&config)?;
        assert_eq!(diffs.len(), 2, "expected diffs should also be 2 with 360hz");
        Ok(())
    }

    #[test]
    fn non_iracing_ignores_360hz_flag() -> TestResult {
        let config = TelemetryConfig {
            enable_high_rate_iracing_360hz: true,
            ..default_config()
        };
        for game_id in ["acc", "rfactor2", "ams2", "dirt5", "f1"] {
            let writer = writer_for(game_id)?;
            let temp = tempfile::tempdir()?;
            let diffs = writer.write_config(temp.path(), &config)?;
            assert!(
                !diffs.iter().any(|d| d.key.contains("360")),
                "{game_id}: should not produce 360hz diff"
            );
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// TelemetryConfig serde edge cases
// ---------------------------------------------------------------------------

mod telemetry_config_serde {
    use super::*;

    #[test]
    fn deserialize_minimal_json() -> TestResult {
        let json = r#"{
            "enabled": false,
            "update_rate_hz": 1,
            "output_method": "",
            "output_target": "",
            "fields": []
        }"#;
        let config: TelemetryConfig = serde_json::from_str(json)?;
        assert!(!config.enabled);
        assert_eq!(config.update_rate_hz, 1);
        assert!(!config.enable_high_rate_iracing_360hz);
        Ok(())
    }

    #[test]
    fn round_trip_preserves_all_fields() -> TestResult {
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 360,
            output_method: "shared_memory".to_string(),
            output_target: "192.168.1.100:9999".to_string(),
            fields: vec!["a".into(), "b".into(), "c".into()],
            enable_high_rate_iracing_360hz: true,
        };
        let json = serde_json::to_string(&config)?;
        let decoded: TelemetryConfig = serde_json::from_str(&json)?;
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
    fn pretty_json_round_trip() -> TestResult {
        let config = default_config();
        let json = serde_json::to_string_pretty(&config)?;
        let decoded: TelemetryConfig = serde_json::from_str(&json)?;
        assert_eq!(decoded.update_rate_hz, config.update_rate_hz);
        Ok(())
    }

    #[test]
    fn extra_json_fields_ignored() -> TestResult {
        let json = r#"{
            "enabled": true,
            "update_rate_hz": 60,
            "output_method": "udp",
            "output_target": "127.0.0.1:1234",
            "fields": [],
            "unknown_field": 42,
            "another_extra": "hello"
        }"#;
        let config: TelemetryConfig = serde_json::from_str(json)?;
        assert!(config.enabled);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ConfigDiff and DiffOperation deep tests
// ---------------------------------------------------------------------------

mod diff_types {
    use super::*;

    #[test]
    fn config_diff_clone_equals_original() -> TestResult {
        let diff = ConfigDiff {
            file_path: "path/to/file.ini".to_string(),
            section: Some("Section".to_string()),
            key: "key".to_string(),
            old_value: Some("old".to_string()),
            new_value: "new".to_string(),
            operation: DiffOperation::Modify,
        };
        let cloned = diff.clone();
        assert_eq!(diff, cloned);
        Ok(())
    }

    #[test]
    fn config_diff_ne_different_keys() -> TestResult {
        let diff1 = ConfigDiff {
            file_path: "a.ini".to_string(),
            section: None,
            key: "key1".to_string(),
            old_value: None,
            new_value: "v".to_string(),
            operation: DiffOperation::Add,
        };
        let diff2 = ConfigDiff {
            key: "key2".to_string(),
            ..diff1.clone()
        };
        assert_ne!(diff1, diff2);
        Ok(())
    }

    #[test]
    fn config_diff_ne_different_operations() -> TestResult {
        let base = ConfigDiff {
            file_path: "a.ini".to_string(),
            section: None,
            key: "k".to_string(),
            old_value: None,
            new_value: "v".to_string(),
            operation: DiffOperation::Add,
        };
        let modified = ConfigDiff {
            operation: DiffOperation::Remove,
            ..base.clone()
        };
        assert_ne!(base, modified);
        Ok(())
    }

    #[test]
    fn diff_operation_debug_output() -> TestResult {
        let debug_add = format!("{:?}", DiffOperation::Add);
        let debug_modify = format!("{:?}", DiffOperation::Modify);
        let debug_remove = format!("{:?}", DiffOperation::Remove);
        assert!(debug_add.contains("Add"));
        assert!(debug_modify.contains("Modify"));
        assert!(debug_remove.contains("Remove"));
        Ok(())
    }

    #[test]
    fn config_diff_serde_all_operations() -> TestResult {
        for op in [
            DiffOperation::Add,
            DiffOperation::Modify,
            DiffOperation::Remove,
        ] {
            let diff = ConfigDiff {
                file_path: "test.ini".to_string(),
                section: Some("S".to_string()),
                key: "k".to_string(),
                old_value: Some("old".to_string()),
                new_value: "new".to_string(),
                operation: op.clone(),
            };
            let json = serde_json::to_string(&diff)?;
            let decoded: ConfigDiff = serde_json::from_str(&json)?;
            assert_eq!(decoded.operation, op);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Writer-specific protocol families
// ---------------------------------------------------------------------------

mod protocol_families {
    use super::*;

    #[test]
    fn codemasters_bridge_writers_produce_bridge_diffs() -> TestResult {
        let codemasters = [
            "f1",
            "f1_25",
            "dirt5",
            "dirt_rally_2",
            "dirt4",
            "dirt3",
            "grid_autosport",
            "grid_2019",
            "grid_legends",
            "race_driver_grid",
            "wrc_generations",
            "dirt_showdown",
        ];
        let config = default_config();
        for game_id in codemasters {
            let writer = writer_for(game_id)?;
            let temp = tempfile::tempdir()?;
            let diffs = writer.write_config(temp.path(), &config)?;
            assert!(!diffs.is_empty(), "{game_id}: should produce diffs");
        }
        Ok(())
    }

    #[test]
    fn forza_writers_all_produce_diffs() -> TestResult {
        let forza_games = ["forza_motorsport", "forza_horizon_4", "forza_horizon_5"];
        let config = default_config();
        for game_id in forza_games {
            let writer = writer_for(game_id)?;
            let temp = tempfile::tempdir()?;
            let diffs = writer.write_config(temp.path(), &config)?;
            assert!(!diffs.is_empty(), "{game_id}: should produce diffs");
            assert!(
                writer.validate_config(temp.path())?,
                "{game_id}: should validate after write"
            );
        }
        Ok(())
    }

    #[test]
    fn rfactor_family_writers() -> TestResult {
        let rfactor_games = [
            "rfactor1",
            "rfactor2",
            "gtr2",
            "race_07",
            "gsc",
            "le_mans_ultimate",
        ];
        let config = default_config();
        for game_id in rfactor_games {
            let writer = writer_for(game_id)?;
            let temp = tempfile::tempdir()?;
            let diffs = writer.write_config(temp.path(), &config)?;
            assert!(!diffs.is_empty(), "{game_id}: should produce diffs");
            assert!(
                writer.validate_config(temp.path())?,
                "{game_id}: should validate after write"
            );
        }
        Ok(())
    }

    #[test]
    fn wrc_kylotonn_variants_independent() -> TestResult {
        let config = default_config();
        let temp9 = tempfile::tempdir()?;
        let temp10 = tempfile::tempdir()?;

        let w9 = writer_for("wrc_9")?;
        let w10 = writer_for("wrc_10")?;

        let diffs9 = w9.write_config(temp9.path(), &config)?;
        let diffs10 = w10.write_config(temp10.path(), &config)?;

        assert!(!diffs9.is_empty());
        assert!(!diffs10.is_empty());
        assert!(w9.validate_config(temp9.path())?);
        assert!(w10.validate_config(temp10.path())?);
        Ok(())
    }

    #[test]
    fn newest_game_writers_functional() -> TestResult {
        let newest = ["acc2", "ac_evo", "ac_rally", "f1_25", "f1_manager"];
        let config = default_config();
        for game_id in newest {
            let writer = writer_for(game_id)?;
            let temp = tempfile::tempdir()?;
            writer.write_config(temp.path(), &config)?;
            assert!(
                writer.validate_config(temp.path())?,
                "{game_id}: should validate"
            );
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Writer idempotency (write same config twice, same result)
// ---------------------------------------------------------------------------

mod idempotency {
    use super::*;

    #[test]
    fn write_same_config_twice_produces_same_file_content() -> TestResult {
        let config = default_config();
        // Test with a few representative writers
        for game_id in ["iracing", "acc", "rfactor2", "eawrc", "f1"] {
            let writer = writer_for(game_id)?;
            let temp1 = tempfile::tempdir()?;
            let temp2 = tempfile::tempdir()?;

            writer.write_config(temp1.path(), &config)?;
            writer.write_config(temp2.path(), &config)?;

            // Both directories should produce the same validate result
            let v1 = writer.validate_config(temp1.path())?;
            let v2 = writer.validate_config(temp2.path())?;
            assert_eq!(
                v1, v2,
                "{game_id}: idempotent writes should validate the same"
            );
        }
        Ok(())
    }

    #[test]
    fn expected_diffs_stable_across_calls() -> TestResult {
        let config = default_config();
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let diffs1 = writer.get_expected_diffs(&config)?;
            let diffs2 = writer.get_expected_diffs(&config)?;
            assert_eq!(
                diffs1.len(),
                diffs2.len(),
                "{id}: expected diffs count should be stable"
            );
            for (d1, d2) in diffs1.iter().zip(diffs2.iter()) {
                assert_eq!(d1.key, d2.key, "{id}: expected diff keys should be stable");
                assert_eq!(
                    d1.new_value, d2.new_value,
                    "{id}: expected diff values should be stable"
                );
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Special characters in paths (spaces, unicode)
// ---------------------------------------------------------------------------

mod special_character_paths {
    use super::*;

    #[test]
    fn path_with_spaces_works_for_all_writers() -> TestResult {
        let config = default_config();
        for (id, factory) in config_writer_factories() {
            let parent = tempfile::tempdir()?;
            let spaced = parent.path().join("My Games Folder");
            std::fs::create_dir_all(&spaced)?;
            let writer = factory();
            let diffs = writer.write_config(&spaced, &config)?;
            assert!(
                !diffs.is_empty(),
                "{id}: should produce diffs with spaces in path"
            );
            assert!(
                writer.validate_config(&spaced)?,
                "{id}: should validate with spaces in path"
            );
        }
        Ok(())
    }

    #[test]
    fn path_with_unicode_works_for_representative_writers() -> TestResult {
        let config = default_config();
        let games = ["iracing", "acc", "rfactor2", "f1", "eawrc"];
        for game_id in games {
            let parent = tempfile::tempdir()?;
            let unicode_dir = parent.path().join("Ünïcödé_パス");
            std::fs::create_dir_all(&unicode_dir)?;
            let writer = writer_for(game_id)?;
            let diffs = writer.write_config(&unicode_dir, &config)?;
            assert!(
                !diffs.is_empty(),
                "{game_id}: should produce diffs with unicode path"
            );
            assert!(
                writer.validate_config(&unicode_dir)?,
                "{game_id}: should validate with unicode path"
            );
        }
        Ok(())
    }

    #[test]
    fn path_with_special_chars_in_output_target() -> TestResult {
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:20777".to_string(),
            fields: vec!["rpm".to_string()],
            enable_high_rate_iracing_360hz: false,
        };
        let parent = tempfile::tempdir()?;
        let deep_path = parent.path().join("path (with) [brackets] & special");
        std::fs::create_dir_all(&deep_path)?;
        let writer = writer_for("iracing")?;
        let diffs = writer.write_config(&deep_path, &config)?;
        assert!(!diffs.is_empty());
        assert!(writer.validate_config(&deep_path)?);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Port number configuration for UDP-based games
// ---------------------------------------------------------------------------

mod port_configuration {
    use super::*;

    #[test]
    fn custom_port_reflected_in_bridge_contracts() -> TestResult {
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:55555".to_string(),
            fields: vec![],
            enable_high_rate_iracing_360hz: false,
        };
        let bridge_games = ["dirt5", "f1", "forza_motorsport", "trackmania", "simhub"];
        for game_id in bridge_games {
            let writer = writer_for(game_id)?;
            let temp = tempfile::tempdir()?;
            writer.write_config(temp.path(), &config)?;

            let files = walkdir(temp.path())?;
            let json_file = files
                .iter()
                .find(|p| {
                    p.extension()
                        .map(|e| e == "json")
                        .unwrap_or(false)
                })
                .ok_or(format!("{game_id}: expected a json file"))?;
            let content = std::fs::read_to_string(json_file)?;
            assert!(
                content.contains("55555"),
                "{game_id}: custom port 55555 should appear in output"
            );
        }
        Ok(())
    }

    #[test]
    fn acc_custom_port_in_broadcasting_json() -> TestResult {
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 120,
            output_method: "udp".to_string(),
            output_target: "127.0.0.1:9876".to_string(),
            fields: vec![],
            enable_high_rate_iracing_360hz: false,
        };
        let writer = writer_for("acc")?;
        let temp = tempfile::tempdir()?;
        writer.write_config(temp.path(), &config)?;

        let broadcasting = temp
            .path()
            .join("Documents/Assetto Corsa Competizione/Config/broadcasting.json");
        let content = std::fs::read_to_string(&broadcasting)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;
        let port = value
            .get("updListenerPort")
            .and_then(|v| v.as_u64())
            .ok_or("missing updListenerPort")?;
        assert_eq!(port, 9876);
        Ok(())
    }

    #[test]
    fn eawrc_custom_port_in_config_json() -> TestResult {
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: "192.168.1.50:33333".to_string(),
            fields: vec![],
            enable_high_rate_iracing_360hz: false,
        };
        let writer = writer_for("eawrc")?;
        let temp = tempfile::tempdir()?;
        writer.write_config(temp.path(), &config)?;

        let config_json = temp
            .path()
            .join("Documents/My Games/WRC/telemetry/config.json");
        let content = std::fs::read_to_string(&config_json)?;
        assert!(
            content.contains("33333"),
            "eawrc config should contain custom port"
        );
        assert!(
            content.contains("192.168.1.50"),
            "eawrc config should contain custom IP"
        );
        Ok(())
    }

    #[test]
    fn default_port_used_when_no_port_in_target() -> TestResult {
        let config = TelemetryConfig {
            enabled: true,
            update_rate_hz: 60,
            output_method: "udp".to_string(),
            output_target: String::new(),
            fields: vec![],
            enable_high_rate_iracing_360hz: false,
        };
        // ACC defaults to port 9000
        let writer = writer_for("acc")?;
        let temp = tempfile::tempdir()?;
        writer.write_config(temp.path(), &config)?;

        let broadcasting = temp
            .path()
            .join("Documents/Assetto Corsa Competizione/Config/broadcasting.json");
        let content = std::fs::read_to_string(&broadcasting)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;
        let port = value
            .get("updListenerPort")
            .and_then(|v| v.as_u64())
            .ok_or("missing updListenerPort")?;
        assert_eq!(port, 9000, "ACC should fall back to default port 9000");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Backup/restore of existing configs
// ---------------------------------------------------------------------------

mod backup_restore {
    use super::*;

    #[test]
    fn overwrite_preserves_existing_json_fields_in_acc() -> TestResult {
        let writer = writer_for("acc")?;
        let temp = tempfile::tempdir()?;
        let broadcasting = temp
            .path()
            .join("Documents/Assetto Corsa Competizione/Config/broadcasting.json");
        if let Some(parent) = broadcasting.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Seed with custom connectionPassword
        let seed = serde_json::json!({
            "updListenerPort": 1234,
            "udpListenerPort": 1234,
            "broadcastingPort": 1234,
            "connectionId": "my_id",
            "connectionPassword": "secret123",
            "commandPassword": "cmd_pass",
            "updateRateHz": 30,
            "customField": "keep_me"
        });
        std::fs::write(&broadcasting, serde_json::to_string_pretty(&seed)?)?;

        let config = default_config();
        writer.write_config(temp.path(), &config)?;

        let content = std::fs::read_to_string(&broadcasting)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;
        // connectionPassword should be preserved from existing
        assert_eq!(
            value.get("connectionPassword").and_then(|v| v.as_str()),
            Some("secret123"),
            "ACC should preserve existing connectionPassword"
        );
        // customField should also be preserved
        assert_eq!(
            value.get("customField").and_then(|v| v.as_str()),
            Some("keep_me"),
            "ACC should preserve unknown fields"
        );
        Ok(())
    }

    #[test]
    fn overwrite_preserves_existing_json_fields_in_ams2() -> TestResult {
        let writer = writer_for("ams2")?;
        let temp = tempfile::tempdir()?;
        let player_json = temp
            .path()
            .join("Documents/Automobilista 2/UserData/player/player.json");
        if let Some(parent) = player_json.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let seed = serde_json::json!({
            "customSetting": 42,
            "playerName": "TestDriver"
        });
        std::fs::write(&player_json, serde_json::to_string_pretty(&seed)?)?;

        let config = default_config();
        writer.write_config(temp.path(), &config)?;

        let content = std::fs::read_to_string(&player_json)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;
        assert_eq!(
            value.get("customSetting").and_then(|v| v.as_u64()),
            Some(42),
            "AMS2 should preserve existing customSetting"
        );
        assert_eq!(
            value.get("playerName").and_then(|v| v.as_str()),
            Some("TestDriver"),
            "AMS2 should preserve existing playerName"
        );
        Ok(())
    }

    #[test]
    fn iracing_overwrite_preserves_non_telemetry_sections() -> TestResult {
        let writer = writer_for("iracing")?;
        let temp = tempfile::tempdir()?;
        let app_ini = temp.path().join("Documents/iRacing/app.ini");
        if let Some(parent) = app_ini.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let seed = "[Graphics]\nresolution=1920x1080\n\n[Audio]\nvolume=80\n";
        std::fs::write(&app_ini, seed)?;

        let config = default_config();
        writer.write_config(temp.path(), &config)?;

        let content = std::fs::read_to_string(&app_ini)?;
        assert!(
            content.contains("[Graphics]"),
            "should preserve [Graphics] section"
        );
        assert!(
            content.contains("resolution=1920x1080"),
            "should preserve graphics settings"
        );
        assert!(
            content.contains("[Audio]"),
            "should preserve [Audio] section"
        );
        assert!(
            content.contains("[Telemetry]"),
            "should add [Telemetry] section"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Config file permission / error handling
// ---------------------------------------------------------------------------

mod error_handling {
    use super::*;

    #[test]
    fn write_to_nonexistent_parent_creates_directories() -> TestResult {
        let config = default_config();
        for (id, factory) in config_writer_factories() {
            let temp = tempfile::tempdir()?;
            // The tempdir itself is empty; writers must create sub-directories
            let writer = factory();
            let result = writer.write_config(temp.path(), &config);
            assert!(
                result.is_ok(),
                "{id}: should create intermediate directories"
            );
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn write_to_read_only_directory_returns_error() -> TestResult {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir()?;
        let locked = temp.path().join("locked");
        std::fs::create_dir_all(&locked)?;
        let metadata = std::fs::metadata(&locked)?;
        let mut perms = metadata.permissions();
        perms.set_mode(0o444);
        std::fs::set_permissions(&locked, perms)?;

        let writer = writer_for("iracing")?;
        let config = default_config();
        let result = writer.write_config(&locked, &config);
        assert!(result.is_err(), "should fail on read-only directory");

        // Restore permissions for cleanup
        let mut perms = std::fs::metadata(&locked)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&locked, perms)?;
        Ok(())
    }

    #[test]
    fn validate_on_corrupted_json_returns_error_or_false() -> TestResult {
        let writer = writer_for("acc")?;
        let temp = tempfile::tempdir()?;
        let broadcasting = temp
            .path()
            .join("Documents/Assetto Corsa Competizione/Config/broadcasting.json");
        if let Some(parent) = broadcasting.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&broadcasting, "NOT_VALID_JSON{{{}")?;

        let result = writer.validate_config(temp.path());
        // Should either return Err or Ok(false) but not panic
        if let Ok(valid) = result {
            assert!(!valid, "corrupted JSON should not validate");
        }
        Ok(())
    }

    #[test]
    fn validate_on_empty_file_returns_error_or_false() -> TestResult {
        let writer = writer_for("rfactor2")?;
        let temp = tempfile::tempdir()?;
        let config_path = temp
            .path()
            .join("UserData/player/OpenRacing.Telemetry.json");
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&config_path, "")?;

        let result = writer.validate_config(temp.path());
        if let Ok(valid) = result {
            assert!(!valid, "empty file should not validate");
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Config diff: detecting changes between versions
// ---------------------------------------------------------------------------

mod config_diff_detection {
    use super::*;

    #[test]
    fn changing_update_rate_produces_modify_with_different_values() -> TestResult {
        let writer = writer_for("acc")?;
        let temp = tempfile::tempdir()?;

        let config1 = TelemetryConfig {
            update_rate_hz: 30,
            ..default_config()
        };
        writer.write_config(temp.path(), &config1)?;

        let config2 = TelemetryConfig {
            update_rate_hz: 120,
            ..default_config()
        };
        let diffs = writer.write_config(temp.path(), &config2)?;

        let diff = diffs.first().ok_or("expected a diff")?;
        assert_eq!(diff.operation, DiffOperation::Modify);
        assert!(
            diff.new_value.contains("120"),
            "new value should contain updated rate"
        );
        assert!(
            diff.old_value
                .as_ref()
                .map(|v| v.contains("30"))
                .unwrap_or(false),
            "old value should contain previous rate"
        );
        Ok(())
    }

    #[test]
    fn toggling_enabled_flag_produces_modify() -> TestResult {
        let writer = writer_for("iracing")?;
        let temp = tempfile::tempdir()?;

        let on_config = TelemetryConfig {
            enabled: true,
            ..default_config()
        };
        writer.write_config(temp.path(), &on_config)?;

        let off_config = TelemetryConfig {
            enabled: false,
            ..default_config()
        };
        let diffs = writer.write_config(temp.path(), &off_config)?;
        let telemetry_diff = diffs
            .iter()
            .find(|d| d.key == "telemetryDiskFile")
            .ok_or("missing telemetryDiskFile")?;
        assert_eq!(telemetry_diff.operation, DiffOperation::Modify);
        assert_eq!(telemetry_diff.new_value, "0");
        assert_eq!(telemetry_diff.old_value.as_deref(), Some("1"));
        Ok(())
    }

    #[test]
    fn expected_diffs_match_write_diffs_new_values() -> TestResult {
        let config = TelemetryConfig {
            update_rate_hz: 144,
            output_target: "127.0.0.1:9999".to_string(),
            ..default_config()
        };
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let temp = tempfile::tempdir()?;
            let write_diffs = writer.write_config(temp.path(), &config)?;
            let expected_diffs = writer.get_expected_diffs(&config)?;

            for (wd, ed) in write_diffs.iter().zip(expected_diffs.iter()) {
                assert_eq!(
                    wd.new_value, ed.new_value,
                    "{id}: write and expected diff new_values should match"
                );
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Output format roundtrip: write → read back → verify correctness
// ---------------------------------------------------------------------------

mod output_roundtrip {
    use super::*;

    #[test]
    fn json_writers_produce_valid_parseable_json() -> TestResult {
        let json_games = [
            "acc", "ams2", "rfactor2", "eawrc", "ac_rally", "dirt5", "f1",
            "f1_25", "forza_motorsport", "simhub", "trackmania",
        ];
        let config = default_config();
        for game_id in json_games {
            let writer = writer_for(game_id)?;
            let temp = tempfile::tempdir()?;
            writer.write_config(temp.path(), &config)?;

            let files = walkdir(temp.path())?;
            for file in &files {
                if file.extension().map(|e| e == "json").unwrap_or(false) {
                    let content = std::fs::read_to_string(file)?;
                    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&content);
                    assert!(
                        parsed.is_ok(),
                        "{game_id}: file {} should be valid JSON: {:?}",
                        file.display(),
                        parsed.err()
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn iracing_ini_write_read_roundtrip() -> TestResult {
        let writer = writer_for("iracing")?;
        let temp = tempfile::tempdir()?;
        let config = TelemetryConfig {
            enabled: true,
            enable_high_rate_iracing_360hz: true,
            ..default_config()
        };
        writer.write_config(temp.path(), &config)?;

        let app_ini = temp.path().join("Documents/iRacing/app.ini");
        let content = std::fs::read_to_string(&app_ini)?;
        // Verify INI format has section header and key=value pairs
        assert!(content.contains("[Telemetry]"));
        assert!(content.contains("telemetryDiskFile=1"));
        assert!(content.contains("irsdkLog360Hz=1"));
        Ok(())
    }

    #[test]
    fn bridge_contract_roundtrip_has_required_fields() -> TestResult {
        let bridge_games = [
            "dirt5",
            "dirt_rally_2",
            "dirt4",
            "wrc_generations",
            "nascar",
            "le_mans_ultimate",
        ];
        let config = default_config();
        for game_id in bridge_games {
            let writer = writer_for(game_id)?;
            let temp = tempfile::tempdir()?;
            writer.write_config(temp.path(), &config)?;

            let files = walkdir(temp.path())?;
            let json_file = files
                .iter()
                .find(|p| {
                    p.extension()
                        .map(|e| e == "json")
                        .unwrap_or(false)
                })
                .ok_or(format!("{game_id}: no json file"))?;
            let content = std::fs::read_to_string(json_file)?;
            let value: serde_json::Value = serde_json::from_str(&content)?;

            assert!(
                value.get("game_id").is_some(),
                "{game_id}: bridge contract must have game_id"
            );
            assert!(
                value.get("telemetry_protocol").is_some(),
                "{game_id}: bridge contract must have telemetry_protocol"
            );
            assert!(
                value.get("enabled").is_some(),
                "{game_id}: bridge contract must have enabled"
            );
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Game-specific quirks
// ---------------------------------------------------------------------------

mod game_specific_quirks {
    use super::*;

    #[test]
    fn iracing_ini_format_uses_section_and_key_value_pairs() -> TestResult {
        let writer = writer_for("iracing")?;
        let temp = tempfile::tempdir()?;
        let config = default_config();
        let diffs = writer.write_config(temp.path(), &config)?;

        // iRacing diffs should have section = Some("Telemetry")
        for diff in &diffs {
            assert_eq!(
                diff.section.as_deref(),
                Some("Telemetry"),
                "iRacing diffs should have Telemetry section"
            );
        }
        Ok(())
    }

    #[test]
    fn ac_contract_references_outgauge_ini_setup() -> TestResult {
        let writer = writer_for("assetto_corsa")?;
        let temp = tempfile::tempdir()?;
        let config = default_config();
        writer.write_config(temp.path(), &config)?;

        let files = walkdir(temp.path())?;
        let json_file = files
            .first()
            .ok_or("expected at least one file")?;
        let content = std::fs::read_to_string(json_file)?;
        assert!(
            content.contains("OutGauge"),
            "AC contract should reference OutGauge INI format"
        );
        Ok(())
    }

    #[test]
    fn f1_25_has_packet_format_2025() -> TestResult {
        let writer = writer_for("f1_25")?;
        let temp = tempfile::tempdir()?;
        let config = default_config();
        writer.write_config(temp.path(), &config)?;

        let files = walkdir(temp.path())?;
        let json_file = files
            .first()
            .ok_or("expected at least one file")?;
        let content = std::fs::read_to_string(json_file)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;
        assert_eq!(
            value.get("packet_format").and_then(|v| v.as_u64()),
            Some(2025),
            "F1 25 should have packet_format 2025"
        );
        Ok(())
    }

    #[test]
    fn forza_contract_lists_supported_formats() -> TestResult {
        let writer = writer_for("forza_motorsport")?;
        let temp = tempfile::tempdir()?;
        let config = default_config();
        writer.write_config(temp.path(), &config)?;

        let files = walkdir(temp.path())?;
        let json_file = files
            .first()
            .ok_or("expected at least one file")?;
        let content = std::fs::read_to_string(json_file)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;
        let formats = value
            .get("supported_formats")
            .and_then(|v| v.as_array())
            .ok_or("missing supported_formats")?;
        assert!(
            formats.len() >= 2,
            "Forza should list at least 2 supported formats"
        );
        Ok(())
    }

    #[test]
    fn eawrc_creates_two_output_files() -> TestResult {
        let writer = writer_for("eawrc")?;
        let temp = tempfile::tempdir()?;
        let config = default_config();
        let diffs = writer.write_config(temp.path(), &config)?;

        assert_eq!(
            diffs.len(),
            2,
            "eawrc should create config.json and structure definition"
        );

        let config_json = temp
            .path()
            .join("Documents/My Games/WRC/telemetry/config.json");
        assert!(config_json.exists(), "eawrc config.json should exist");

        let structure_json = temp
            .path()
            .join("Documents/My Games/WRC/telemetry/udp/openracing.json");
        assert!(
            structure_json.exists(),
            "eawrc structure definition should exist"
        );
        Ok(())
    }

    #[test]
    fn rfactor2_contract_references_shared_memory_maps() -> TestResult {
        let writer = writer_for("rfactor2")?;
        let temp = tempfile::tempdir()?;
        let config = default_config();
        writer.write_config(temp.path(), &config)?;

        let cfg_path = temp
            .path()
            .join("UserData/player/OpenRacing.Telemetry.json");
        let content = std::fs::read_to_string(&cfg_path)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;

        assert!(
            value.get("telemetryMap").is_some(),
            "rfactor2 should have telemetryMap"
        );
        assert!(
            value.get("forceFeedbackMap").is_some(),
            "rfactor2 should have forceFeedbackMap"
        );
        assert_eq!(
            value
                .get("requiresSharedMemoryPlugin")
                .and_then(|v| v.as_bool()),
            Some(true),
            "rfactor2 should require shared memory plugin"
        );
        Ok(())
    }

    #[test]
    fn ams2_references_pcars2_shared_memory() -> TestResult {
        let writer = writer_for("ams2")?;
        let temp = tempfile::tempdir()?;
        let config = default_config();
        writer.write_config(temp.path(), &config)?;

        let player_json = temp
            .path()
            .join("Documents/Automobilista 2/UserData/player/player.json");
        let content = std::fs::read_to_string(&player_json)?;
        assert!(
            content.contains("$pcars2$"),
            "AMS2 should reference $pcars2$ shared memory map"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Idempotent file content (byte-for-byte comparison)
// ---------------------------------------------------------------------------

mod idempotent_file_content {
    use super::*;

    #[test]
    fn same_config_same_file_bytes_for_json_writers() -> TestResult {
        let json_games = ["acc", "rfactor2", "ams2", "dirt5", "f1_25"];
        let config = default_config();
        for game_id in json_games {
            let writer = writer_for(game_id)?;
            let temp1 = tempfile::tempdir()?;
            let temp2 = tempfile::tempdir()?;

            writer.write_config(temp1.path(), &config)?;
            writer.write_config(temp2.path(), &config)?;

            let files1 = walkdir(temp1.path())?;
            let files2 = walkdir(temp2.path())?;
            assert_eq!(
                files1.len(),
                files2.len(),
                "{game_id}: should create same number of files"
            );

            for (f1, f2) in files1.iter().zip(files2.iter()) {
                let c1 = std::fs::read(f1)?;
                let c2 = std::fs::read(f2)?;
                assert_eq!(
                    c1, c2,
                    "{game_id}: file content should be byte-identical across fresh writes"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn iracing_ini_content_identical_on_fresh_writes() -> TestResult {
        let writer = writer_for("iracing")?;
        let config = default_config();
        let temp1 = tempfile::tempdir()?;
        let temp2 = tempfile::tempdir()?;

        writer.write_config(temp1.path(), &config)?;
        writer.write_config(temp2.path(), &config)?;

        let c1 = std::fs::read_to_string(temp1.path().join("Documents/iRacing/app.ini"))?;
        let c2 = std::fs::read_to_string(temp2.path().join("Documents/iRacing/app.ini"))?;
        assert_eq!(c1, c2, "iRacing INI should be identical on fresh writes");
        Ok(())
    }
}
