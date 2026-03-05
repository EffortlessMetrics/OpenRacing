//! Deep validation tests for game telemetry config writer system.
//!
//! Covers: config detection via validate_config, config writing format correctness,
//! backup/preservation of existing data, syntactic validation of written configs,
//! error handling on corrupted/missing/read-only paths, and round-trip fidelity
//! (write → read → verify values match) for all supported games.

use racing_wheel_telemetry_config_writers::{
    ConfigWriter, DiffOperation, TelemetryConfig, config_writer_factories,
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
// Config detection: validate_config returns false on empty, true after write
// ---------------------------------------------------------------------------

mod config_detection {
    use super::*;

    #[test]
    fn validate_returns_false_on_uninitialized_path_for_all() -> TestResult {
        for (id, factory) in config_writer_factories() {
            let temp = tempfile::tempdir()?;
            let writer = factory();
            let valid = writer.validate_config(temp.path())?;
            assert!(
                !valid,
                "{id}: validate_config must return false on empty directory"
            );
        }
        Ok(())
    }

    #[test]
    fn validate_returns_true_after_write_for_all() -> TestResult {
        let config = default_config();
        for (id, factory) in config_writer_factories() {
            let temp = tempfile::tempdir()?;
            let writer = factory();
            writer.write_config(temp.path(), &config)?;
            let valid = writer.validate_config(temp.path())?;
            assert!(
                valid,
                "{id}: validate_config must return true after successful write"
            );
        }
        Ok(())
    }

    #[test]
    fn validate_returns_true_after_disabled_config_write() -> TestResult {
        let config = TelemetryConfig {
            enabled: false,
            ..default_config()
        };
        // Games that embed enabled=false still produce a valid config file
        let bridge_games = [
            "dirt5",
            "dirt_rally_2",
            "f1",
            "rbr",
            "forza_motorsport",
            "simhub",
        ];
        for game_id in bridge_games {
            let writer = writer_for(game_id)?;
            let temp = tempfile::tempdir()?;
            writer.write_config(temp.path(), &config)?;
            let valid = writer.validate_config(temp.path())?;
            assert!(
                valid,
                "{game_id}: validate should return true even when enabled=false"
            );
        }
        Ok(())
    }

    #[test]
    fn validate_detects_missing_subdir_as_false() -> TestResult {
        // Use a real tempdir but don't write anything — just create the parent dir
        let writer = writer_for("acc")?;
        let temp = tempfile::tempdir()?;
        let sub = temp.path().join("some_unrelated_subdir");
        std::fs::create_dir_all(&sub)?;
        let valid = writer.validate_config(temp.path())?;
        assert!(
            !valid,
            "ACC validate should be false without broadcasting.json"
        );
        Ok(())
    }

    #[test]
    fn validate_detects_truncated_json_as_invalid() -> TestResult {
        let writer = writer_for("rfactor2")?;
        let temp = tempfile::tempdir()?;
        let config_path = temp
            .path()
            .join("UserData/player/OpenRacing.Telemetry.json");
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Write truncated JSON
        std::fs::write(&config_path, r#"{"enabled": true, "requiresShared"#)?;
        let result = writer.validate_config(temp.path());
        // Should either return Err (parse failure) or Ok(false)
        if let Ok(valid) = result {
            assert!(!valid, "truncated JSON should not validate");
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Config writing: verify correct format per game family
// ---------------------------------------------------------------------------

mod config_format_correctness {
    use super::*;

    #[test]
    fn iracing_ini_has_correct_section_and_keys() -> TestResult {
        let writer = writer_for("iracing")?;
        let temp = tempfile::tempdir()?;
        let config = TelemetryConfig {
            enabled: true,
            enable_high_rate_iracing_360hz: true,
            ..default_config()
        };
        writer.write_config(temp.path(), &config)?;

        let content = std::fs::read_to_string(temp.path().join("Documents/iRacing/app.ini"))?;
        assert!(content.contains("[Telemetry]"));
        assert!(content.contains("telemetryDiskFile=1"));
        assert!(content.contains("irsdkLog360Hz=1"));
        // Verify INI key=value format (no spaces around =)
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') || trimmed.is_empty() {
                continue;
            }
            assert!(
                trimmed.contains('='),
                "INI line must be key=value format: {trimmed}"
            );
        }
        Ok(())
    }

    #[test]
    fn acc_broadcasting_json_has_all_required_keys() -> TestResult {
        let writer = writer_for("acc")?;
        let temp = tempfile::tempdir()?;
        writer.write_config(temp.path(), &default_config())?;

        let path = temp
            .path()
            .join("Documents/Assetto Corsa Competizione/Config/broadcasting.json");
        let content = std::fs::read_to_string(&path)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;
        let obj = value.as_object().ok_or("expected JSON object")?;

        let required_keys = [
            "updListenerPort",
            "udpListenerPort",
            "broadcastingPort",
            "connectionId",
            "connectionPassword",
            "commandPassword",
            "updateRateHz",
        ];
        for key in required_keys {
            assert!(
                obj.contains_key(key),
                "ACC broadcasting.json missing key: {key}"
            );
        }
        Ok(())
    }

    #[test]
    fn ams2_player_json_has_shared_memory_and_telemetry_block() -> TestResult {
        let writer = writer_for("ams2")?;
        let temp = tempfile::tempdir()?;
        writer.write_config(temp.path(), &default_config())?;

        let path = temp
            .path()
            .join("Documents/Automobilista 2/UserData/player/player.json");
        let content = std::fs::read_to_string(&path)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;

        assert_eq!(
            value.get("sharedMemoryEnabled").and_then(|v| v.as_bool()),
            Some(true),
            "AMS2 should set sharedMemoryEnabled"
        );
        let telemetry = value
            .get("openRacingTelemetry")
            .and_then(|v| v.as_object())
            .ok_or("missing openRacingTelemetry block")?;
        assert_eq!(
            telemetry.get("enabled").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            telemetry.get("sharedMemoryMap").and_then(|v| v.as_str()),
            Some("$pcars2$")
        );
        Ok(())
    }

    #[test]
    fn rfactor2_config_has_shared_memory_maps() -> TestResult {
        let writer = writer_for("rfactor2")?;
        let temp = tempfile::tempdir()?;
        writer.write_config(temp.path(), &default_config())?;

        let path = temp
            .path()
            .join("UserData/player/OpenRacing.Telemetry.json");
        let content = std::fs::read_to_string(&path)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;

        assert_eq!(
            value
                .get("requiresSharedMemoryPlugin")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert!(
            value
                .get("telemetryMap")
                .and_then(|v| v.as_str())
                .is_some_and(|s| s.contains("rFactor2"))
        );
        assert!(
            value
                .get("scoringMap")
                .and_then(|v| v.as_str())
                .is_some_and(|s| s.contains("rFactor2"))
        );
        assert!(
            value
                .get("forceFeedbackMap")
                .and_then(|v| v.as_str())
                .is_some_and(|s| s.contains("rFactor2"))
        );
        Ok(())
    }

    #[test]
    fn eawrc_config_has_udp_packet_assignment() -> TestResult {
        let writer = writer_for("eawrc")?;
        let temp = tempfile::tempdir()?;
        writer.write_config(temp.path(), &default_config())?;

        let config_path = temp
            .path()
            .join("Documents/My Games/WRC/telemetry/config.json");
        let content = std::fs::read_to_string(&config_path)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;

        let assignments = value
            .get("udp")
            .and_then(|v| v.get("packetAssignments"))
            .and_then(|v| v.as_array())
            .ok_or("missing udp.packetAssignments")?;
        assert!(
            !assignments.is_empty(),
            "EAWRC should have at least one assignment"
        );

        let first = &assignments[0];
        assert_eq!(
            first.get("packetId").and_then(|v| v.as_str()),
            Some("session_update")
        );
        assert_eq!(
            first.get("structureId").and_then(|v| v.as_str()),
            Some("openracing")
        );
        Ok(())
    }

    #[test]
    fn eawrc_structure_definition_has_channels() -> TestResult {
        let writer = writer_for("eawrc")?;
        let temp = tempfile::tempdir()?;
        writer.write_config(temp.path(), &default_config())?;

        let structure_path = temp
            .path()
            .join("Documents/My Games/WRC/telemetry/udp/openracing.json");
        let content = std::fs::read_to_string(&structure_path)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;

        assert_eq!(value.get("id").and_then(|v| v.as_str()), Some("openracing"));
        let packets = value
            .get("packets")
            .and_then(|v| v.as_array())
            .ok_or("missing packets array")?;
        assert!(!packets.is_empty());
        let channels = packets[0]
            .get("channels")
            .and_then(|v| v.as_array())
            .ok_or("missing channels")?;
        assert!(
            channels.len() >= 3,
            "EAWRC structure should have at least 3 channels"
        );
        Ok(())
    }

    #[test]
    fn gt7_bridge_contract_references_salsa20() -> TestResult {
        let writer = writer_for("gran_turismo_7")?;
        let temp = tempfile::tempdir()?;
        writer.write_config(temp.path(), &default_config())?;

        let files = walkdir(temp.path())?;
        let json_file = files.first().ok_or("expected output file")?;
        let content = std::fs::read_to_string(json_file)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;
        assert_eq!(
            value.get("telemetry_protocol").and_then(|v| v.as_str()),
            Some("gt7_salsa20_udp")
        );
        Ok(())
    }

    #[test]
    fn assetto_corsa_contract_has_outgauge_setup_notes() -> TestResult {
        let writer = writer_for("assetto_corsa")?;
        let temp = tempfile::tempdir()?;
        writer.write_config(temp.path(), &default_config())?;

        let files = walkdir(temp.path())?;
        let json_file = files.first().ok_or("expected output file")?;
        let content = std::fs::read_to_string(json_file)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;

        assert_eq!(
            value.get("telemetry_protocol").and_then(|v| v.as_str()),
            Some("ac_outgauge_udp")
        );
        let notes = value
            .get("setup_notes")
            .and_then(|v| v.as_array())
            .ok_or("missing setup_notes")?;
        let notes_text: String = notes
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            notes_text.contains("OutGauge"),
            "AC setup notes should reference OutGauge"
        );
        assert!(
            notes_text.contains("Mode=2"),
            "AC setup notes should reference Mode=2"
        );
        Ok(())
    }

    #[test]
    fn forza_motorsport_contract_has_supported_formats() -> TestResult {
        let writer = writer_for("forza_motorsport")?;
        let temp = tempfile::tempdir()?;
        writer.write_config(temp.path(), &default_config())?;

        let files = walkdir(temp.path())?;
        let json_file = files.first().ok_or("expected output file")?;
        let content = std::fs::read_to_string(json_file)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;

        let formats = value
            .get("supported_formats")
            .and_then(|v| v.as_array())
            .ok_or("missing supported_formats")?;
        let format_strs: Vec<&str> = formats.iter().filter_map(|v| v.as_str()).collect();
        assert!(format_strs.contains(&"sled_232"));
        assert!(format_strs.contains(&"cardash_311"));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Backup / preservation: writers must preserve existing config fields
// ---------------------------------------------------------------------------

mod config_backup_preservation {
    use super::*;

    #[test]
    fn acc_preserves_connection_password_on_overwrite() -> TestResult {
        let writer = writer_for("acc")?;
        let temp = tempfile::tempdir()?;
        let path = temp
            .path()
            .join("Documents/Assetto Corsa Competizione/Config/broadcasting.json");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let seed = serde_json::json!({
            "updListenerPort": 1000,
            "udpListenerPort": 1000,
            "broadcastingPort": 1000,
            "connectionId": "existing_id",
            "connectionPassword": "my_secret",
            "commandPassword": "cmd_secret",
            "updateRateHz": 10,
            "userCustomKey": "preserve_this"
        });
        std::fs::write(&path, serde_json::to_string_pretty(&seed)?)?;

        writer.write_config(temp.path(), &default_config())?;

        let content = std::fs::read_to_string(&path)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;
        assert_eq!(
            value.get("connectionPassword").and_then(|v| v.as_str()),
            Some("my_secret"),
            "connectionPassword must be preserved"
        );
        assert_eq!(
            value.get("commandPassword").and_then(|v| v.as_str()),
            Some("cmd_secret"),
            "commandPassword must be preserved"
        );
        assert_eq!(
            value.get("userCustomKey").and_then(|v| v.as_str()),
            Some("preserve_this"),
            "unknown keys must be preserved"
        );
        Ok(())
    }

    #[test]
    fn ams2_preserves_existing_player_settings() -> TestResult {
        let writer = writer_for("ams2")?;
        let temp = tempfile::tempdir()?;
        let path = temp
            .path()
            .join("Documents/Automobilista 2/UserData/player/player.json");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let seed = serde_json::json!({
            "playerName": "Speed Racer",
            "assists": {"abs": true, "tc": false}
        });
        std::fs::write(&path, serde_json::to_string_pretty(&seed)?)?;

        writer.write_config(temp.path(), &default_config())?;

        let content = std::fs::read_to_string(&path)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;
        assert_eq!(
            value.get("playerName").and_then(|v| v.as_str()),
            Some("Speed Racer"),
            "AMS2 must preserve playerName"
        );
        assert!(
            value.get("assists").is_some(),
            "AMS2 must preserve assists block"
        );
        Ok(())
    }

    #[test]
    fn iracing_preserves_existing_ini_sections() -> TestResult {
        let writer = writer_for("iracing")?;
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("Documents/iRacing/app.ini");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let seed = "[Graphics]\nresolution=2560x1440\nfullscreen=1\n\n[Sound]\nvolume=75\n";
        std::fs::write(&path, seed)?;

        writer.write_config(temp.path(), &default_config())?;

        let content = std::fs::read_to_string(&path)?;
        assert!(content.contains("[Graphics]"), "must preserve [Graphics]");
        assert!(
            content.contains("resolution=2560x1440"),
            "must preserve resolution"
        );
        assert!(content.contains("fullscreen=1"), "must preserve fullscreen");
        assert!(content.contains("[Sound]"), "must preserve [Sound]");
        assert!(content.contains("volume=75"), "must preserve volume");
        assert!(
            content.contains("[Telemetry]"),
            "must add [Telemetry] section"
        );
        assert!(
            content.contains("telemetryDiskFile=1"),
            "must add telemetry key"
        );
        Ok(())
    }

    #[test]
    fn iracing_updates_existing_telemetry_value() -> TestResult {
        let writer = writer_for("iracing")?;
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("Documents/iRacing/app.ini");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Pre-existing config with telemetry disabled
        let seed = "[Telemetry]\ntelemetryDiskFile=0\n";
        std::fs::write(&path, seed)?;

        let config = TelemetryConfig {
            enabled: true,
            ..default_config()
        };
        let diffs = writer.write_config(temp.path(), &config)?;

        let content = std::fs::read_to_string(&path)?;
        assert!(
            content.contains("telemetryDiskFile=1"),
            "must update to enabled"
        );
        // The diff should be a Modify since we're updating an existing value
        let diff = diffs
            .iter()
            .find(|d| d.key == "telemetryDiskFile")
            .ok_or("missing telemetryDiskFile diff")?;
        assert_eq!(diff.operation, DiffOperation::Modify);
        assert_eq!(diff.old_value.as_deref(), Some("0"));
        assert_eq!(diff.new_value, "1");
        Ok(())
    }

    #[test]
    fn rfactor2_preserves_custom_fields_on_overwrite() -> TestResult {
        let writer = writer_for("rfactor2")?;
        let temp = tempfile::tempdir()?;
        let path = temp
            .path()
            .join("UserData/player/OpenRacing.Telemetry.json");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let seed = serde_json::json!({
            "enabled": false,
            "requiresSharedMemoryPlugin": true,
            "telemetryMap": "$rFactor2SMMP_Telemetry$",
            "scoringMap": "$rFactor2SMMP_Scoring$",
            "forceFeedbackMap": "$rFactor2SMMP_ForceFeedback$",
            "updateRateHz": 30,
            "userNote": "my custom note"
        });
        std::fs::write(&path, serde_json::to_string_pretty(&seed)?)?;

        writer.write_config(temp.path(), &default_config())?;

        let content = std::fs::read_to_string(&path)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;
        assert_eq!(
            value.get("userNote").and_then(|v| v.as_str()),
            Some("my custom note"),
            "rFactor2 must preserve custom fields"
        );
        Ok(())
    }

    #[test]
    fn overwrite_diff_captures_old_value_for_json_writers() -> TestResult {
        let json_games = ["acc", "ams2", "rfactor2"];
        let config = default_config();
        for game_id in json_games {
            let writer = writer_for(game_id)?;
            let temp = tempfile::tempdir()?;

            // First write
            let diffs1 = writer.write_config(temp.path(), &config)?;
            assert!(
                diffs1.iter().all(|d| d.old_value.is_none()),
                "{game_id}: first write should have no old_value"
            );

            // Second write
            let diffs2 = writer.write_config(temp.path(), &config)?;
            for diff in &diffs2 {
                if diff.operation == DiffOperation::Modify {
                    assert!(
                        diff.old_value.is_some(),
                        "{game_id}: Modify diff should capture old_value"
                    );
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Config validation: verify written configs are syntactically valid
// ---------------------------------------------------------------------------

mod config_syntactic_validation {
    use super::*;

    #[test]
    fn all_json_output_files_are_valid_json() -> TestResult {
        let config = default_config();
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let temp = tempfile::tempdir()?;
            writer.write_config(temp.path(), &config)?;

            let files = walkdir(temp.path())?;
            for file in &files {
                if file.extension().and_then(|e| e.to_str()) == Some("json") {
                    let content = std::fs::read_to_string(file)?;
                    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&content);
                    assert!(
                        parsed.is_ok(),
                        "{id}: {} should be valid JSON, error: {:?}",
                        file.display(),
                        parsed.err()
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn all_json_files_are_pretty_printed() -> TestResult {
        let config = default_config();
        let sample_games = ["acc", "rfactor2", "dirt5", "f1_25", "eawrc", "simhub"];
        for game_id in sample_games {
            let writer = writer_for(game_id)?;
            let temp = tempfile::tempdir()?;
            writer.write_config(temp.path(), &config)?;

            let files = walkdir(temp.path())?;
            for file in &files {
                if file.extension().and_then(|e| e.to_str()) == Some("json") {
                    let content = std::fs::read_to_string(file)?;
                    // Pretty-printed JSON has newlines and indentation
                    assert!(
                        content.contains('\n'),
                        "{game_id}: {} should be pretty-printed",
                        file.display()
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn iracing_ini_ends_with_newline() -> TestResult {
        let writer = writer_for("iracing")?;
        let temp = tempfile::tempdir()?;
        writer.write_config(temp.path(), &default_config())?;

        let content = std::fs::read_to_string(temp.path().join("Documents/iRacing/app.ini"))?;
        assert!(content.ends_with('\n'), "INI file should end with newline");
        Ok(())
    }

    #[test]
    fn all_diff_new_values_are_non_empty() -> TestResult {
        let config = default_config();
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let temp = tempfile::tempdir()?;
            let diffs = writer.write_config(temp.path(), &config)?;
            for diff in &diffs {
                assert!(
                    !diff.new_value.is_empty(),
                    "{id}: diff new_value must not be empty for key '{}'",
                    diff.key
                );
            }
        }
        Ok(())
    }

    #[test]
    fn all_diff_file_paths_are_non_empty() -> TestResult {
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
}

// ---------------------------------------------------------------------------
// Error handling: permission errors, read-only files, missing directories
// ---------------------------------------------------------------------------

mod error_handling {
    use super::*;

    #[test]
    fn validate_on_corrupted_json_for_multiple_games() -> TestResult {
        let games_and_paths: Vec<(&str, &str)> = vec![
            (
                "acc",
                "Documents/Assetto Corsa Competizione/Config/broadcasting.json",
            ),
            ("rfactor2", "UserData/player/OpenRacing.Telemetry.json"),
            (
                "ams2",
                "Documents/Automobilista 2/UserData/player/player.json",
            ),
        ];
        for (game_id, rel_path) in games_and_paths {
            let writer = writer_for(game_id)?;
            let temp = tempfile::tempdir()?;
            let config_path = temp.path().join(rel_path);
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&config_path, "{{{{INVALID JSON")?;
            let result = writer.validate_config(temp.path());
            if let Ok(valid) = result {
                assert!(
                    !valid,
                    "{game_id}: corrupted JSON should not validate as true"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn validate_on_wrong_json_structure() -> TestResult {
        // Write valid JSON but with wrong structure (array instead of object)
        let writer = writer_for("acc")?;
        let temp = tempfile::tempdir()?;
        let path = temp
            .path()
            .join("Documents/Assetto Corsa Competizione/Config/broadcasting.json");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, "[1, 2, 3]")?;
        let result = writer.validate_config(temp.path());
        if let Ok(valid) = result {
            assert!(!valid, "array JSON should not validate for ACC");
        }
        Ok(())
    }

    #[test]
    fn validate_on_empty_json_object() -> TestResult {
        // Write valid but empty JSON object
        let writer = writer_for("rfactor2")?;
        let temp = tempfile::tempdir()?;
        let path = temp
            .path()
            .join("UserData/player/OpenRacing.Telemetry.json");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, "{}")?;
        let valid = writer.validate_config(temp.path())?;
        assert!(!valid, "empty JSON object should not validate for rfactor2");
        Ok(())
    }

    #[test]
    fn validate_on_partial_ini_for_iracing() -> TestResult {
        let writer = writer_for("iracing")?;
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("Documents/iRacing/app.ini");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Has section but wrong key value
        std::fs::write(&path, "[Telemetry]\ntelemetryDiskFile=0\n")?;
        let valid = writer.validate_config(temp.path())?;
        assert!(
            !valid,
            "iracing with telemetryDiskFile=0 should not validate as true"
        );
        Ok(())
    }

    #[test]
    fn write_creates_deep_nested_directories() -> TestResult {
        let config = default_config();
        // EAWRC creates deeply nested paths
        let writer = writer_for("eawrc")?;
        let temp = tempfile::tempdir()?;
        // tempdir is empty, writer must create all intermediate dirs
        let result = writer.write_config(temp.path(), &config);
        assert!(result.is_ok(), "EAWRC should create nested directories");

        let config_path = temp
            .path()
            .join("Documents/My Games/WRC/telemetry/config.json");
        assert!(config_path.exists(), "config.json should exist");

        let structure_path = temp
            .path()
            .join("Documents/My Games/WRC/telemetry/udp/openracing.json");
        assert!(structure_path.exists(), "structure json should exist");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Round-trip: write config → read back → verify values match
// ---------------------------------------------------------------------------

mod round_trip_fidelity {
    use super::*;

    #[test]
    fn iracing_round_trip_enabled_flag() -> TestResult {
        for enabled in [true, false] {
            let writer = writer_for("iracing")?;
            let temp = tempfile::tempdir()?;
            let config = TelemetryConfig {
                enabled,
                ..default_config()
            };
            writer.write_config(temp.path(), &config)?;

            let content = std::fs::read_to_string(temp.path().join("Documents/iRacing/app.ini"))?;
            let expected_val = if enabled { "1" } else { "0" };
            assert!(
                content.contains(&format!("telemetryDiskFile={expected_val}")),
                "enabled={enabled}: expected telemetryDiskFile={expected_val}"
            );
        }
        Ok(())
    }

    #[test]
    fn acc_round_trip_port_and_rate() -> TestResult {
        let writer = writer_for("acc")?;
        let temp = tempfile::tempdir()?;
        let config = TelemetryConfig {
            update_rate_hz: 144,
            output_target: "127.0.0.1:7777".to_string(),
            ..default_config()
        };
        writer.write_config(temp.path(), &config)?;

        let path = temp
            .path()
            .join("Documents/Assetto Corsa Competizione/Config/broadcasting.json");
        let content = std::fs::read_to_string(&path)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;

        assert_eq!(
            value.get("updListenerPort").and_then(|v| v.as_u64()),
            Some(7777),
            "port should match config"
        );
        assert_eq!(
            value.get("updateRateHz").and_then(|v| v.as_u64()),
            Some(144),
            "rate should match config"
        );
        Ok(())
    }

    #[test]
    fn rfactor2_round_trip_update_rate() -> TestResult {
        let writer = writer_for("rfactor2")?;
        let temp = tempfile::tempdir()?;
        let config = TelemetryConfig {
            update_rate_hz: 200,
            ..default_config()
        };
        writer.write_config(temp.path(), &config)?;

        let path = temp
            .path()
            .join("UserData/player/OpenRacing.Telemetry.json");
        let content = std::fs::read_to_string(&path)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;
        assert_eq!(
            value.get("updateRateHz").and_then(|v| v.as_u64()),
            Some(200)
        );
        Ok(())
    }

    #[test]
    fn eawrc_round_trip_ip_and_port() -> TestResult {
        let writer = writer_for("eawrc")?;
        let temp = tempfile::tempdir()?;
        let config = TelemetryConfig {
            output_target: "192.168.1.42:12345".to_string(),
            ..default_config()
        };
        writer.write_config(temp.path(), &config)?;

        let config_path = temp
            .path()
            .join("Documents/My Games/WRC/telemetry/config.json");
        let content = std::fs::read_to_string(&config_path)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;

        let assignment = value
            .get("udp")
            .and_then(|v| v.get("packetAssignments"))
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .ok_or("missing assignment")?;

        assert_eq!(
            assignment.get("ip").and_then(|v| v.as_str()),
            Some("192.168.1.42")
        );
        assert_eq!(assignment.get("port").and_then(|v| v.as_u64()), Some(12345));
        Ok(())
    }

    #[test]
    fn bridge_contract_round_trip_port_for_all_udp_games() -> TestResult {
        let udp_bridge_games = [
            "dirt5",
            "dirt_rally_2",
            "dirt4",
            "f1",
            "rbr",
            "gran_turismo_7",
            "forza_motorsport",
            "wrc_generations",
            "nascar",
            "trackmania",
            "simhub",
        ];
        let config = TelemetryConfig {
            output_target: "127.0.0.1:44444".to_string(),
            ..default_config()
        };
        for game_id in udp_bridge_games {
            let writer = writer_for(game_id)?;
            let temp = tempfile::tempdir()?;
            writer.write_config(temp.path(), &config)?;

            let files = walkdir(temp.path())?;
            let json_file = files
                .iter()
                .find(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
                .ok_or(format!("{game_id}: no json file found"))?;
            let content = std::fs::read_to_string(json_file)?;
            let value: serde_json::Value = serde_json::from_str(&content)?;

            let port = value.get("udp_port").and_then(|v| v.as_u64());
            assert_eq!(port, Some(44444), "{game_id}: udp_port should be 44444");
        }
        Ok(())
    }

    #[test]
    fn bridge_contract_round_trip_enabled_flag() -> TestResult {
        let bridge_games = ["dirt5", "f1", "forza_motorsport", "rbr"];
        for enabled in [true, false] {
            let config = TelemetryConfig {
                enabled,
                ..default_config()
            };
            for game_id in bridge_games {
                let writer = writer_for(game_id)?;
                let temp = tempfile::tempdir()?;
                writer.write_config(temp.path(), &config)?;

                let files = walkdir(temp.path())?;
                let json_file = files
                    .iter()
                    .find(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
                    .ok_or(format!("{game_id}: no json file found"))?;
                let content = std::fs::read_to_string(json_file)?;
                let value: serde_json::Value = serde_json::from_str(&content)?;

                assert_eq!(
                    value.get("enabled").and_then(|v| v.as_bool()),
                    Some(enabled),
                    "{game_id}: enabled flag should be {enabled}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn bridge_contract_round_trip_update_rate() -> TestResult {
        let bridge_games = ["dirt5", "f1", "wrc_generations", "nascar"];
        let config = TelemetryConfig {
            update_rate_hz: 240,
            ..default_config()
        };
        for game_id in bridge_games {
            let writer = writer_for(game_id)?;
            let temp = tempfile::tempdir()?;
            writer.write_config(temp.path(), &config)?;

            let files = walkdir(temp.path())?;
            let json_file = files
                .iter()
                .find(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
                .ok_or(format!("{game_id}: no json file found"))?;
            let content = std::fs::read_to_string(json_file)?;
            let value: serde_json::Value = serde_json::from_str(&content)?;

            assert_eq!(
                value.get("update_rate_hz").and_then(|v| v.as_u64()),
                Some(240),
                "{game_id}: update_rate_hz should be 240"
            );
        }
        Ok(())
    }

    #[test]
    fn write_then_validate_then_rewrite_roundtrip() -> TestResult {
        let games = ["iracing", "acc", "rfactor2", "eawrc", "f1", "dirt5"];
        for game_id in games {
            let writer = writer_for(game_id)?;
            let temp = tempfile::tempdir()?;

            let config1 = TelemetryConfig {
                update_rate_hz: 30,
                ..default_config()
            };
            writer.write_config(temp.path(), &config1)?;
            assert!(
                writer.validate_config(temp.path())?,
                "{game_id}: should validate after first write"
            );

            let config2 = TelemetryConfig {
                update_rate_hz: 120,
                ..default_config()
            };
            writer.write_config(temp.path(), &config2)?;
            assert!(
                writer.validate_config(temp.path())?,
                "{game_id}: should validate after second write"
            );
        }
        Ok(())
    }

    #[test]
    fn expected_diffs_new_values_match_written_file_content() -> TestResult {
        let config = default_config();
        // Test with a few representative writers whose diffs contain "entire_file"
        let json_games = ["acc", "rfactor2", "dirt5"];
        for game_id in json_games {
            let writer = writer_for(game_id)?;
            let temp = tempfile::tempdir()?;

            let write_diffs = writer.write_config(temp.path(), &config)?;
            let expected_diffs = writer.get_expected_diffs(&config)?;

            for (wd, ed) in write_diffs.iter().zip(expected_diffs.iter()) {
                assert_eq!(
                    wd.new_value, ed.new_value,
                    "{game_id}: write diff and expected diff new_value must match"
                );
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Default port fallback: empty output_target uses game-specific defaults
// ---------------------------------------------------------------------------

mod default_port_fallback {
    use super::*;

    #[test]
    fn acc_defaults_to_port_9000() -> TestResult {
        let writer = writer_for("acc")?;
        let temp = tempfile::tempdir()?;
        let config = TelemetryConfig {
            output_target: String::new(),
            ..default_config()
        };
        writer.write_config(temp.path(), &config)?;

        let path = temp
            .path()
            .join("Documents/Assetto Corsa Competizione/Config/broadcasting.json");
        let content = std::fs::read_to_string(&path)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;
        assert_eq!(
            value.get("updListenerPort").and_then(|v| v.as_u64()),
            Some(9000)
        );
        Ok(())
    }

    #[test]
    fn eawrc_defaults_to_port_20778() -> TestResult {
        let writer = writer_for("eawrc")?;
        let temp = tempfile::tempdir()?;
        let config = TelemetryConfig {
            output_target: String::new(),
            ..default_config()
        };
        writer.write_config(temp.path(), &config)?;

        let config_path = temp
            .path()
            .join("Documents/My Games/WRC/telemetry/config.json");
        let content = std::fs::read_to_string(&config_path)?;
        assert!(
            content.contains("20778"),
            "EAWRC should default to port 20778"
        );
        Ok(())
    }

    #[test]
    fn dirt5_defaults_to_port_20777() -> TestResult {
        let writer = writer_for("dirt5")?;
        let temp = tempfile::tempdir()?;
        let config = TelemetryConfig {
            output_target: String::new(),
            ..default_config()
        };
        writer.write_config(temp.path(), &config)?;

        let files = walkdir(temp.path())?;
        let json_file = files.first().ok_or("no output file")?;
        let content = std::fs::read_to_string(json_file)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;
        assert_eq!(value.get("udp_port").and_then(|v| v.as_u64()), Some(20777));
        Ok(())
    }

    #[test]
    fn forza_motorsport_defaults_to_port_5300() -> TestResult {
        let writer = writer_for("forza_motorsport")?;
        let temp = tempfile::tempdir()?;
        let config = TelemetryConfig {
            output_target: String::new(),
            ..default_config()
        };
        writer.write_config(temp.path(), &config)?;

        let files = walkdir(temp.path())?;
        let json_file = files.first().ok_or("no output file")?;
        let content = std::fs::read_to_string(json_file)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;
        assert_eq!(value.get("udp_port").and_then(|v| v.as_u64()), Some(5300));
        Ok(())
    }

    #[test]
    fn gt7_defaults_to_port_33740() -> TestResult {
        let writer = writer_for("gran_turismo_7")?;
        let temp = tempfile::tempdir()?;
        let config = TelemetryConfig {
            output_target: String::new(),
            ..default_config()
        };
        writer.write_config(temp.path(), &config)?;

        let files = walkdir(temp.path())?;
        let json_file = files.first().ok_or("no output file")?;
        let content = std::fs::read_to_string(json_file)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;
        assert_eq!(value.get("udp_port").and_then(|v| v.as_u64()), Some(33740));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Game-specific protocol family cross-checks
// ---------------------------------------------------------------------------

mod protocol_family_validation {
    use super::*;

    #[test]
    fn codemasters_bridge_contracts_have_mode_field() -> TestResult {
        let codemasters_games = [
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
        for game_id in codemasters_games {
            let writer = writer_for(game_id)?;
            let temp = tempfile::tempdir()?;
            writer.write_config(temp.path(), &config)?;

            let files = walkdir(temp.path())?;
            let json_file = files
                .iter()
                .find(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
                .ok_or(format!("{game_id}: no json file"))?;
            let content = std::fs::read_to_string(json_file)?;
            let value: serde_json::Value = serde_json::from_str(&content)?;

            assert!(
                value.get("mode").is_some(),
                "{game_id}: Codemasters bridge contract should have 'mode' field"
            );
            assert_eq!(
                value.get("telemetry_protocol").and_then(|v| v.as_str()),
                Some("codemasters_udp"),
                "{game_id}: should use codemasters_udp protocol"
            );
        }
        Ok(())
    }

    #[test]
    fn forza_family_all_use_forza_data_out_protocol() -> TestResult {
        let forza_games = ["forza_motorsport", "forza_horizon_4", "forza_horizon_5"];
        let config = default_config();
        for game_id in forza_games {
            let writer = writer_for(game_id)?;
            let temp = tempfile::tempdir()?;
            writer.write_config(temp.path(), &config)?;

            let files = walkdir(temp.path())?;
            let json_file = files.first().ok_or(format!("{game_id}: no file"))?;
            let content = std::fs::read_to_string(json_file)?;
            let value: serde_json::Value = serde_json::from_str(&content)?;
            assert_eq!(
                value.get("telemetry_protocol").and_then(|v| v.as_str()),
                Some("forza_data_out_udp"),
                "{game_id}: should use forza_data_out_udp"
            );
        }
        Ok(())
    }

    #[test]
    fn bridge_game_ids_match_factory_ids() -> TestResult {
        let bridge_games = [
            "dirt5",
            "dirt_rally_2",
            "rbr",
            "gran_turismo_7",
            "forza_motorsport",
            "assetto_corsa",
        ];
        let config = default_config();
        for game_id in bridge_games {
            let writer = writer_for(game_id)?;
            let temp = tempfile::tempdir()?;
            writer.write_config(temp.path(), &config)?;

            let files = walkdir(temp.path())?;
            let json_file = files.first().ok_or(format!("{game_id}: no file"))?;
            let content = std::fs::read_to_string(json_file)?;
            let value: serde_json::Value = serde_json::from_str(&content)?;
            assert_eq!(
                value.get("game_id").and_then(|v| v.as_str()),
                Some(game_id),
                "{game_id}: contract game_id should match factory id"
            );
        }
        Ok(())
    }

    #[test]
    fn all_bridge_contracts_have_game_id_and_protocol() -> TestResult {
        let config = default_config();
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let temp = tempfile::tempdir()?;
            writer.write_config(temp.path(), &config)?;

            let files = walkdir(temp.path())?;
            for file in &files {
                if file.extension().and_then(|e| e.to_str()) == Some("json") {
                    let content = std::fs::read_to_string(file)?;
                    let value: serde_json::Value = serde_json::from_str(&content)?;
                    if let Some(obj) = value.as_object() {
                        // If it looks like a bridge contract, it should have these fields
                        if obj.contains_key("telemetry_protocol") {
                            assert!(
                                obj.contains_key("game_id"),
                                "{id}: bridge contract in {} missing game_id",
                                file.display()
                            );
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Idempotency and stability of written content
// ---------------------------------------------------------------------------

mod write_stability {
    use super::*;

    #[test]
    fn triple_write_produces_same_validate_result() -> TestResult {
        let config = default_config();
        let games = ["iracing", "acc", "rfactor2", "eawrc", "dirt5", "f1_25"];
        for game_id in games {
            let writer = writer_for(game_id)?;
            let temp = tempfile::tempdir()?;

            writer.write_config(temp.path(), &config)?;
            writer.write_config(temp.path(), &config)?;
            writer.write_config(temp.path(), &config)?;

            assert!(
                writer.validate_config(temp.path())?,
                "{game_id}: should still validate after triple write"
            );
        }
        Ok(())
    }

    #[test]
    fn changing_config_values_updates_written_file() -> TestResult {
        let writer = writer_for("acc")?;
        let temp = tempfile::tempdir()?;

        let config1 = TelemetryConfig {
            update_rate_hz: 30,
            output_target: "127.0.0.1:9000".to_string(),
            ..default_config()
        };
        writer.write_config(temp.path(), &config1)?;

        let config2 = TelemetryConfig {
            update_rate_hz: 250,
            output_target: "127.0.0.1:5555".to_string(),
            ..default_config()
        };
        writer.write_config(temp.path(), &config2)?;

        let path = temp
            .path()
            .join("Documents/Assetto Corsa Competizione/Config/broadcasting.json");
        let content = std::fs::read_to_string(&path)?;
        let value: serde_json::Value = serde_json::from_str(&content)?;
        assert_eq!(
            value.get("updateRateHz").and_then(|v| v.as_u64()),
            Some(250),
            "update rate should reflect second write"
        );
        assert_eq!(
            value.get("updListenerPort").and_then(|v| v.as_u64()),
            Some(5555),
            "port should reflect second write"
        );
        Ok(())
    }

    #[test]
    fn get_expected_diffs_deterministic_across_calls() -> TestResult {
        let config = default_config();
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let d1 = writer.get_expected_diffs(&config)?;
            let d2 = writer.get_expected_diffs(&config)?;
            let d3 = writer.get_expected_diffs(&config)?;
            assert_eq!(d1.len(), d2.len(), "{id}: diff count unstable");
            assert_eq!(d2.len(), d3.len(), "{id}: diff count unstable");
            for i in 0..d1.len() {
                assert_eq!(
                    d1[i].new_value, d2[i].new_value,
                    "{id}: diff[{i}] value unstable"
                );
                assert_eq!(
                    d2[i].new_value, d3[i].new_value,
                    "{id}: diff[{i}] value unstable"
                );
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Factory registry completeness checks
// ---------------------------------------------------------------------------

mod factory_completeness {
    use super::*;

    #[test]
    fn all_factory_writers_are_send_sync() -> TestResult {
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            fn assert_send_sync<T: Send + Sync>(_: &T) {}
            assert_send_sync(&writer);
            let _ = format!("{id} is Send + Sync");
        }
        Ok(())
    }

    #[test]
    fn every_factory_write_produces_non_empty_diffs() -> TestResult {
        let config = default_config();
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let temp = tempfile::tempdir()?;
            let diffs = writer.write_config(temp.path(), &config)?;
            assert!(
                !diffs.is_empty(),
                "{id}: write_config must produce at least one diff"
            );
        }
        Ok(())
    }

    #[test]
    fn factory_ids_do_not_contain_whitespace() -> TestResult {
        for (id, _) in config_writer_factories() {
            assert!(
                !id.contains(char::is_whitespace),
                "factory id '{id}' must not contain whitespace"
            );
        }
        Ok(())
    }

    #[test]
    fn factory_ids_match_well_known_set() -> TestResult {
        let ids: HashSet<&str> = config_writer_factories()
            .iter()
            .map(|(id, _)| *id)
            .collect();
        let expected = [
            "iracing",
            "acc",
            "ams2",
            "rfactor2",
            "eawrc",
            "f1",
            "f1_25",
            "dirt5",
            "dirt_rally_2",
            "rbr",
            "gran_turismo_7",
            "assetto_corsa",
            "forza_motorsport",
            "beamng_drive",
            "simhub",
        ];
        for e in expected {
            assert!(ids.contains(e), "missing expected factory id: {e}");
        }
        Ok(())
    }
}
