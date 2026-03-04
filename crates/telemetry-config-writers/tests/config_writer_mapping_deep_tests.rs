//! Deep tests for config writer parameter mapping, round-trips, and edge cases.
//!
//! Covers per-game writer format output, parameter mapping accuracy,
//! round-trip verification (export → import equivalence), and edge
//! values in config output.

use racing_wheel_telemetry_config_writers::{
    ConfigWriter, TelemetryConfig, config_writer_factories,
};

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
// Per-game format output verification
// ---------------------------------------------------------------------------

mod per_game_format_output {
    use super::*;

    #[test]
    fn iracing_writes_ini_format() -> TestResult {
        let writer = writer_for("iracing")?;
        let temp = tempfile::tempdir()?;
        writer.write_config(temp.path(), &default_config())?;

        let ini_path = temp.path().join("Documents/iRacing/app.ini");
        assert!(ini_path.exists(), "iracing must write app.ini");
        let content = std::fs::read_to_string(&ini_path)?;
        assert!(content.contains("[Telemetry]"));
        assert!(content.contains("telemetryDiskFile=1"));
        Ok(())
    }

    #[test]
    fn acc_writes_json_format() -> TestResult {
        let writer = writer_for("acc")?;
        let temp = tempfile::tempdir()?;
        writer.write_config(temp.path(), &default_config())?;

        let json_path = temp
            .path()
            .join("Documents/Assetto Corsa Competizione/Config/broadcasting.json");
        assert!(json_path.exists(), "acc must write broadcasting.json");
        let content = std::fs::read_to_string(&json_path)?;
        let parsed: serde_json::Value = serde_json::from_str(&content)?;
        assert!(parsed.is_object());
        Ok(())
    }

    #[test]
    fn rfactor2_writes_json_bridge() -> TestResult {
        let writer = writer_for("rfactor2")?;
        let temp = tempfile::tempdir()?;
        writer.write_config(temp.path(), &default_config())?;

        let files = walkdir(temp.path())?;
        assert!(!files.is_empty(), "rfactor2 must produce output files");
        // At least one file should be JSON
        let has_json = files
            .iter()
            .any(|f| f.extension().and_then(|e| e.to_str()) == Some("json"));
        assert!(has_json, "rfactor2 should produce a JSON config");
        Ok(())
    }

    #[test]
    fn simhub_writer_produces_bridge_contract() -> TestResult {
        let writer = writer_for("simhub")?;
        let temp = tempfile::tempdir()?;
        writer.write_config(temp.path(), &default_config())?;

        let files = walkdir(temp.path())?;
        assert!(!files.is_empty(), "simhub must produce output files");
        Ok(())
    }

    #[test]
    fn forza_motorsport_writer_creates_files() -> TestResult {
        let writer = writer_for("forza_motorsport")?;
        let temp = tempfile::tempdir()?;
        writer.write_config(temp.path(), &default_config())?;

        let files = walkdir(temp.path())?;
        assert!(!files.is_empty(), "forza_motorsport must produce output");
        Ok(())
    }

    #[test]
    fn eawrc_writer_creates_files() -> TestResult {
        let writer = writer_for("eawrc")?;
        let temp = tempfile::tempdir()?;
        writer.write_config(temp.path(), &default_config())?;

        let files = walkdir(temp.path())?;
        assert!(!files.is_empty(), "eawrc must produce output");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Parameter mapping accuracy
// ---------------------------------------------------------------------------

mod parameter_mapping {
    use super::*;

    #[test]
    fn acc_maps_update_rate_to_json() -> TestResult {
        let writer = writer_for("acc")?;
        let temp = tempfile::tempdir()?;
        let config = TelemetryConfig {
            update_rate_hz: 120,
            ..default_config()
        };
        writer.write_config(temp.path(), &config)?;

        let json_path = temp
            .path()
            .join("Documents/Assetto Corsa Competizione/Config/broadcasting.json");
        let content = std::fs::read_to_string(&json_path)?;
        let parsed: serde_json::Value = serde_json::from_str(&content)?;

        let rate = parsed
            .get("updateRateHz")
            .and_then(|v| v.as_u64())
            .ok_or("missing updateRateHz")?;
        assert_eq!(rate, 120, "ACC should map update_rate_hz to updateRateHz");
        Ok(())
    }

    #[test]
    fn acc_maps_port_from_output_target() -> TestResult {
        let writer = writer_for("acc")?;
        let temp = tempfile::tempdir()?;
        let config = TelemetryConfig {
            output_target: "127.0.0.1:12345".to_string(),
            ..default_config()
        };
        writer.write_config(temp.path(), &config)?;

        let json_path = temp
            .path()
            .join("Documents/Assetto Corsa Competizione/Config/broadcasting.json");
        let content = std::fs::read_to_string(&json_path)?;
        let parsed: serde_json::Value = serde_json::from_str(&content)?;

        let port = parsed
            .get("updListenerPort")
            .or_else(|| parsed.get("udpListenerPort"))
            .and_then(|v| v.as_u64())
            .ok_or("missing listener port")?;
        assert_eq!(port, 12345, "ACC should extract port from output_target");
        Ok(())
    }

    #[test]
    fn iracing_maps_enabled_false_to_zero() -> TestResult {
        let writer = writer_for("iracing")?;
        let temp = tempfile::tempdir()?;
        let config = TelemetryConfig {
            enabled: false,
            ..default_config()
        };
        let diffs = writer.write_config(temp.path(), &config)?;

        let diff = diffs
            .iter()
            .find(|d| d.key == "telemetryDiskFile")
            .ok_or("missing telemetryDiskFile diff")?;
        assert_eq!(diff.new_value, "0");
        Ok(())
    }

    #[test]
    fn iracing_maps_enabled_true_to_one() -> TestResult {
        let writer = writer_for("iracing")?;
        let temp = tempfile::tempdir()?;
        let diffs = writer.write_config(temp.path(), &default_config())?;

        let diff = diffs
            .iter()
            .find(|d| d.key == "telemetryDiskFile")
            .ok_or("missing telemetryDiskFile diff")?;
        assert_eq!(diff.new_value, "1");
        Ok(())
    }

    #[test]
    fn expected_diffs_match_actual_diff_keys() -> TestResult {
        let config = default_config();
        let representative = ["iracing", "acc", "rfactor2", "f1", "eawrc"];
        for game_id in representative {
            let writer = writer_for(game_id)?;
            let temp = tempfile::tempdir()?;

            let expected = writer.get_expected_diffs(&config)?;
            let actual = writer.write_config(temp.path(), &config)?;

            let expected_keys: std::collections::HashSet<_> =
                expected.iter().map(|d| &*d.key).collect();
            let actual_keys: std::collections::HashSet<_> =
                actual.iter().map(|d| &*d.key).collect();

            assert_eq!(
                expected_keys, actual_keys,
                "{game_id}: expected and actual diff keys must match"
            );
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Round-trip: export → import produces equivalent config
// ---------------------------------------------------------------------------

mod round_trip {
    use super::*;

    #[test]
    fn iracing_write_then_validate_is_true() -> TestResult {
        let writer = writer_for("iracing")?;
        let temp = tempfile::tempdir()?;
        writer.write_config(temp.path(), &default_config())?;

        let valid = writer.validate_config(temp.path())?;
        assert!(valid, "iracing config should validate after write");
        Ok(())
    }

    #[test]
    fn acc_write_then_validate_is_true() -> TestResult {
        let writer = writer_for("acc")?;
        let temp = tempfile::tempdir()?;
        writer.write_config(temp.path(), &default_config())?;

        let valid = writer.validate_config(temp.path())?;
        assert!(valid, "acc config should validate after write");
        Ok(())
    }

    #[test]
    fn all_writers_validate_after_write() -> TestResult {
        let config = default_config();
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let temp = tempfile::tempdir()?;
            writer.write_config(temp.path(), &config)?;

            let valid = writer.validate_config(temp.path())?;
            assert!(
                valid,
                "{id}: validate_config should return true after write_config"
            );
        }
        Ok(())
    }

    #[test]
    fn double_write_produces_stable_output() -> TestResult {
        let config = default_config();
        for game_id in ["iracing", "acc", "rfactor2", "eawrc"] {
            let writer = writer_for(game_id)?;

            let temp1 = tempfile::tempdir()?;
            writer.write_config(temp1.path(), &config)?;

            let temp2 = tempfile::tempdir()?;
            writer.write_config(temp2.path(), &config)?;

            let files1 = walkdir(temp1.path())?;
            let files2 = walkdir(temp2.path())?;
            assert_eq!(
                files1.len(),
                files2.len(),
                "{game_id}: same config should produce same number of files"
            );
        }
        Ok(())
    }

    #[test]
    fn overwrite_then_validate_still_true() -> TestResult {
        let writer = writer_for("iracing")?;
        let temp = tempfile::tempdir()?;
        let config = default_config();

        writer.write_config(temp.path(), &config)?;
        writer.write_config(temp.path(), &config)?;

        let valid = writer.validate_config(temp.path())?;
        assert!(valid, "iracing should validate after overwrite");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Edge values in config output
// ---------------------------------------------------------------------------

mod edge_values {
    use super::*;

    #[test]
    fn max_u32_update_rate_does_not_panic() -> TestResult {
        let config = TelemetryConfig {
            update_rate_hz: u32::MAX,
            ..default_config()
        };
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let temp = tempfile::tempdir()?;
            let result = writer.write_config(temp.path(), &config);
            assert!(result.is_ok(), "{id}: max update_rate should not panic");
        }
        Ok(())
    }

    #[test]
    fn empty_fields_vec_does_not_error() -> TestResult {
        let config = TelemetryConfig {
            fields: vec![],
            ..default_config()
        };
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let temp = tempfile::tempdir()?;
            let result = writer.write_config(temp.path(), &config);
            assert!(result.is_ok(), "{id}: empty fields should not error");
        }
        Ok(())
    }

    #[test]
    fn special_chars_in_output_method_does_not_panic() -> TestResult {
        let config = TelemetryConfig {
            output_method: "udp://特殊文字".to_string(),
            ..default_config()
        };
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let temp = tempfile::tempdir()?;
            let result = writer.write_config(temp.path(), &config);
            assert!(
                result.is_ok(),
                "{id}: special chars in method should not error"
            );
        }
        Ok(())
    }

    #[test]
    fn disabled_config_still_writes_files() -> TestResult {
        let config = TelemetryConfig {
            enabled: false,
            ..default_config()
        };
        for (id, factory) in config_writer_factories() {
            let writer = factory();
            let temp = tempfile::tempdir()?;
            writer.write_config(temp.path(), &config)?;

            let files = walkdir(temp.path())?;
            assert!(
                !files.is_empty(),
                "{id}: even disabled config should produce files"
            );
        }
        Ok(())
    }
}
