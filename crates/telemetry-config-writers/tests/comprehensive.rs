//! Comprehensive integration tests for racing-wheel-telemetry-config-writers.
//!
//! Covers: config writing for game formats, round-trip (write → read → compare),
//! factory registry, ConfigDiff/DiffOperation types, and TelemetryConfig serde.

use racing_wheel_telemetry_config_writers::{
    ConfigDiff, ConfigWriter, DiffOperation, TelemetryConfig, config_writer_factories,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn default_config() -> TelemetryConfig {
    TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "shared_memory".to_string(),
        output_target: "127.0.0.1:12345".to_string(),
        fields: vec!["ffb_scalar".to_string(), "rpm".to_string()],
        enable_high_rate_iracing_360hz: false,
    }
}

// ── Factory registry ────────────────────────────────────────────────────

#[test]
fn config_writer_factories_is_non_empty() -> TestResult {
    let factories = config_writer_factories();
    assert!(
        factories.len() >= 20,
        "expected >= 20 factories, got {}",
        factories.len()
    );
    Ok(())
}

#[test]
fn config_writer_factory_ids_are_unique() -> TestResult {
    let factories = config_writer_factories();
    let mut seen = std::collections::HashSet::new();
    for (id, _) in factories {
        assert!(seen.insert(*id), "duplicate factory id: {id}");
    }
    Ok(())
}

#[test]
fn each_factory_produces_a_writer() -> TestResult {
    for (id, factory) in config_writer_factories() {
        let writer = factory();
        // Verify the trait object is usable by calling get_expected_diffs
        let config = default_config();
        let _diffs = writer.get_expected_diffs(&config)?;
        // Just verifying it doesn't panic
        let _ = format!("factory {id} produced a writer");
    }
    Ok(())
}

#[test]
fn well_known_game_ids_present_in_factories() -> TestResult {
    let factories = config_writer_factories();
    let ids: Vec<&str> = factories.iter().map(|(id, _)| *id).collect();
    for expected in ["iracing", "acc", "eawrc", "ams2", "rfactor2", "dirt5"] {
        assert!(ids.contains(&expected), "missing factory: {expected}");
    }
    Ok(())
}

// ── DiffOperation / ConfigDiff ──────────────────────────────────────────

#[test]
fn diff_operation_equality() -> TestResult {
    assert_eq!(DiffOperation::Add, DiffOperation::Add);
    assert_eq!(DiffOperation::Modify, DiffOperation::Modify);
    assert_eq!(DiffOperation::Remove, DiffOperation::Remove);
    assert_ne!(DiffOperation::Add, DiffOperation::Modify);
    assert_ne!(DiffOperation::Modify, DiffOperation::Remove);
    Ok(())
}

#[test]
fn config_diff_serde_round_trip() -> TestResult {
    let diff = ConfigDiff {
        file_path: "some/path.ini".to_string(),
        section: Some("Telemetry".to_string()),
        key: "enabled".to_string(),
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
fn config_diff_with_none_section() -> TestResult {
    let diff = ConfigDiff {
        file_path: "contract.json".to_string(),
        section: None,
        key: "config".to_string(),
        old_value: None,
        new_value: "{}".to_string(),
        operation: DiffOperation::Add,
    };
    let json = serde_json::to_string(&diff)?;
    let decoded: ConfigDiff = serde_json::from_str(&json)?;
    assert_eq!(decoded.section, None);
    assert_eq!(decoded.old_value, None);
    Ok(())
}

// ── TelemetryConfig serde ───────────────────────────────────────────────

#[test]
fn telemetry_config_serde_round_trip() -> TestResult {
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 120,
        output_method: "udp".to_string(),
        output_target: "127.0.0.1:20778".to_string(),
        fields: vec!["ffb_scalar".to_string(), "rpm".to_string(), "gear".to_string()],
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
fn telemetry_config_high_rate_defaults_false() -> TestResult {
    let json = r#"{
        "enabled": true,
        "update_rate_hz": 60,
        "output_method": "udp",
        "output_target": "127.0.0.1:20777",
        "fields": []
    }"#;
    let config: TelemetryConfig = serde_json::from_str(json)?;
    assert!(!config.enable_high_rate_iracing_360hz);
    Ok(())
}

// ── Writer write → validate round-trips (using tempdir) ─────────────────

#[test]
fn iracing_write_and_validate_round_trip() -> TestResult {
    let writer = config_writer_factories()
        .iter()
        .find(|(id, _)| *id == "iracing")
        .map(|(_, f)| f())
        .ok_or_else(|| std::io::Error::other("iracing factory not found"))?;

    let temp_dir = tempfile::tempdir()?;
    let config = default_config();
    let diffs = writer.write_config(temp_dir.path(), &config)?;
    assert!(!diffs.is_empty());
    assert!(writer.validate_config(temp_dir.path())?);
    Ok(())
}

#[test]
fn acc_write_and_validate_round_trip() -> TestResult {
    let writer = config_writer_factories()
        .iter()
        .find(|(id, _)| *id == "acc")
        .map(|(_, f)| f())
        .ok_or_else(|| std::io::Error::other("acc factory not found"))?;

    let temp_dir = tempfile::tempdir()?;
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 100,
        output_method: "udp_broadcast".to_string(),
        output_target: "127.0.0.1:9000".to_string(),
        fields: vec!["speed_ms".to_string()],
        enable_high_rate_iracing_360hz: false,
    };
    let diffs = writer.write_config(temp_dir.path(), &config)?;
    assert!(!diffs.is_empty());
    assert!(writer.validate_config(temp_dir.path())?);
    Ok(())
}

#[test]
fn eawrc_write_and_validate_round_trip() -> TestResult {
    let writer = config_writer_factories()
        .iter()
        .find(|(id, _)| *id == "eawrc")
        .map(|(_, f)| f())
        .ok_or_else(|| std::io::Error::other("eawrc factory not found"))?;

    let temp_dir = tempfile::tempdir()?;
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 120,
        output_method: "udp_schema".to_string(),
        output_target: "127.0.0.1:20790".to_string(),
        fields: vec!["ffb_scalar".to_string(), "rpm".to_string()],
        enable_high_rate_iracing_360hz: false,
    };
    let diffs = writer.write_config(temp_dir.path(), &config)?;
    assert!(!diffs.is_empty());
    assert!(writer.validate_config(temp_dir.path())?);
    Ok(())
}

#[test]
fn rfactor2_write_and_validate_round_trip() -> TestResult {
    let writer = config_writer_factories()
        .iter()
        .find(|(id, _)| *id == "rfactor2")
        .map(|(_, f)| f())
        .ok_or_else(|| std::io::Error::other("rfactor2 factory not found"))?;

    let temp_dir = tempfile::tempdir()?;
    let config = default_config();
    let diffs = writer.write_config(temp_dir.path(), &config)?;
    assert!(!diffs.is_empty());
    assert!(writer.validate_config(temp_dir.path())?);
    Ok(())
}

#[test]
fn dirt5_write_and_validate_round_trip() -> TestResult {
    let writer = config_writer_factories()
        .iter()
        .find(|(id, _)| *id == "dirt5")
        .map(|(_, f)| f())
        .ok_or_else(|| std::io::Error::other("dirt5 factory not found"))?;

    let temp_dir = tempfile::tempdir()?;
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 120,
        output_method: "udp_custom_codemasters".to_string(),
        output_target: "127.0.0.1:20777".to_string(),
        fields: vec!["rpm".to_string(), "speed_ms".to_string()],
        enable_high_rate_iracing_360hz: false,
    };
    let diffs = writer.write_config(temp_dir.path(), &config)?;
    assert!(!diffs.is_empty());
    assert!(writer.validate_config(temp_dir.path())?);
    Ok(())
}

#[test]
fn ams2_write_and_validate_round_trip() -> TestResult {
    let writer = config_writer_factories()
        .iter()
        .find(|(id, _)| *id == "ams2")
        .map(|(_, f)| f())
        .ok_or_else(|| std::io::Error::other("ams2 factory not found"))?;

    let temp_dir = tempfile::tempdir()?;
    let config = default_config();
    let diffs = writer.write_config(temp_dir.path(), &config)?;
    assert!(!diffs.is_empty());
    assert!(writer.validate_config(temp_dir.path())?);
    Ok(())
}

// ── get_expected_diffs ──────────────────────────────────────────────────

#[test]
fn get_expected_diffs_matches_write_diffs_count() -> TestResult {
    let writer = config_writer_factories()
        .iter()
        .find(|(id, _)| *id == "dirt5")
        .map(|(_, f)| f())
        .ok_or_else(|| std::io::Error::other("dirt5 factory not found"))?;

    let temp_dir = tempfile::tempdir()?;
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 120,
        output_method: "udp_custom_codemasters".to_string(),
        output_target: "127.0.0.1:20777".to_string(),
        fields: vec!["rpm".to_string()],
        enable_high_rate_iracing_360hz: false,
    };
    let write_diffs = writer.write_config(temp_dir.path(), &config)?;
    let expected_diffs = writer.get_expected_diffs(&config)?;
    assert_eq!(write_diffs.len(), expected_diffs.len());
    Ok(())
}

// ── iRacing 360hz ───────────────────────────────────────────────────────

#[test]
fn iracing_360hz_produces_two_diffs() -> TestResult {
    let writer = config_writer_factories()
        .iter()
        .find(|(id, _)| *id == "iracing")
        .map(|(_, f)| f())
        .ok_or_else(|| std::io::Error::other("iracing factory not found"))?;

    let temp_dir = tempfile::tempdir()?;
    let config = TelemetryConfig {
        enabled: true,
        update_rate_hz: 60,
        output_method: "shared_memory".to_string(),
        output_target: "127.0.0.1:12345".to_string(),
        fields: vec!["ffb_scalar".to_string()],
        enable_high_rate_iracing_360hz: true,
    };
    let diffs = writer.write_config(temp_dir.path(), &config)?;
    assert_eq!(diffs.len(), 2);
    assert!(diffs.iter().any(|d| d.key == "irsdkLog360Hz"));
    Ok(())
}

// ── Validate on missing config returns false ────────────────────────────

#[test]
fn validate_on_empty_dir_returns_false() -> TestResult {
    let temp_dir = tempfile::tempdir()?;
    for (id, factory) in config_writer_factories() {
        let writer = factory();
        let result = writer.validate_config(temp_dir.path())?;
        assert!(!result, "factory {id} should return false on empty dir");
    }
    Ok(())
}

// ── ConfigWriter trait is object safe ───────────────────────────────────

#[test]
fn config_writer_trait_is_object_safe() -> TestResult {
    let factories = config_writer_factories();
    let writers: Vec<Box<dyn ConfigWriter + Send + Sync>> =
        factories.iter().map(|(_, f)| f()).collect();
    assert!(!writers.is_empty());
    Ok(())
}
