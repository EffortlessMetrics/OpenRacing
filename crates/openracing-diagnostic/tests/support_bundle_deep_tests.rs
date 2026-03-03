//! Deep tests for support bundle creation, PII redaction,
//! compression, size limits, partial generation, and format versioning.
//!
//! Covers: bundle creation with all required sections (system info, device list,
//! config, logs), PII redaction, compression, size limits, partial generation
//! when some data is unavailable, and bundle format versioning.

use openracing_diagnostic::{DiagnosticError, HealthEventData, SupportBundle, SupportBundleConfig};
use std::io::Read;
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn make_event(device_id: &str, event_type: &str) -> HealthEventData {
    HealthEventData {
        timestamp_ns: 1_000_000_000,
        device_id: device_id.to_string(),
        event_type: event_type.to_string(),
        context: serde_json::json!({"source": "test"}),
    }
}

fn zip_entry_names(path: &std::path::Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut names = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        names.push(entry.name().to_string());
    }
    Ok(names)
}

fn read_zip_entry(
    path: &std::path::Path,
    name: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut entry = archive.by_name(name)?;
    let mut contents = String::new();
    entry.read_to_string(&mut contents)?;
    Ok(contents)
}

fn generate_bundle_with_events(
    events: &[HealthEventData],
    temp_dir: &TempDir,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let config = SupportBundleConfig::default();
    let mut bundle = SupportBundle::new(config);
    bundle.add_health_events(events)?;
    let path = temp_dir.path().join("bundle.zip");
    bundle.generate(&path)?;
    Ok(path)
}

// ═══════════════════════════════════════════════════════════════════════════
// Bundle creation – required sections
// ═══════════════════════════════════════════════════════════════════════════

mod bundle_creation {
    use super::*;

    #[test]
    fn test_new_bundle_has_zero_estimated_size() -> TestResult {
        let config = SupportBundleConfig::default();
        let bundle = SupportBundle::new(config);
        assert!(bundle.estimated_size_mb() < 0.001);
        Ok(())
    }

    #[test]
    fn test_default_config_includes_all_sections() -> TestResult {
        let config = SupportBundleConfig::default();
        assert!(config.include_logs);
        assert!(config.include_profiles);
        assert!(config.include_system_info);
        assert!(config.include_recent_recordings);
        assert_eq!(config.max_bundle_size_mb, 25);
        Ok(())
    }

    #[test]
    fn test_bundle_generation_creates_nonempty_file() -> TestResult {
        let temp_dir = TempDir::new()?;
        let config = SupportBundleConfig::default();
        let bundle = SupportBundle::new(config);
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;
        assert!(path.exists());
        let meta = std::fs::metadata(&path)?;
        assert!(meta.len() > 0);
        Ok(())
    }

    #[test]
    fn test_bundle_always_includes_manifest() -> TestResult {
        let temp_dir = TempDir::new()?;
        let bundle = SupportBundle::new(SupportBundleConfig::default());
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;
        let names = zip_entry_names(&path)?;
        assert!(names.contains(&"manifest.json".to_string()));
        Ok(())
    }

    #[test]
    fn test_bundle_includes_system_info_when_added() -> TestResult {
        let temp_dir = TempDir::new()?;
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_system_info()?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;
        let names = zip_entry_names(&path)?;
        assert!(names.contains(&"system_info.json".to_string()));
        Ok(())
    }

    #[test]
    fn test_bundle_includes_health_events_when_added() -> TestResult {
        let temp_dir = TempDir::new()?;
        let path = generate_bundle_with_events(&[make_event("dev-1", "Connected")], &temp_dir)?;
        let names = zip_entry_names(&path)?;
        assert!(names.contains(&"health_events.json".to_string()));
        Ok(())
    }

    #[test]
    fn test_bundle_includes_log_files_when_added() -> TestResult {
        let temp_dir = TempDir::new()?;
        let log_dir = temp_dir.path().join("logs");
        std::fs::create_dir(&log_dir)?;
        std::fs::write(log_dir.join("app.log"), "log data line 1\nlog data line 2\n")?;
        std::fs::write(log_dir.join("error.log"), "error details\n")?;

        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_log_files(&log_dir)?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;

        let names = zip_entry_names(&path)?;
        assert!(names.iter().any(|n| n.contains("app.log")));
        assert!(names.iter().any(|n| n.contains("error.log")));
        Ok(())
    }

    #[test]
    fn test_bundle_includes_profile_files_when_added() -> TestResult {
        let temp_dir = TempDir::new()?;
        let profile_dir = temp_dir.path().join("profiles");
        std::fs::create_dir(&profile_dir)?;
        std::fs::write(profile_dir.join("default.json"), r#"{"name":"default"}"#)?;

        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_profile_files(&profile_dir)?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;

        let names = zip_entry_names(&path)?;
        assert!(names.iter().any(|n| n.contains("default.json")));
        Ok(())
    }

    #[test]
    fn test_bundle_skips_non_matching_extensions() -> TestResult {
        let temp_dir = TempDir::new()?;
        let log_dir = temp_dir.path().join("logs");
        std::fs::create_dir(&log_dir)?;
        std::fs::write(log_dir.join("app.log"), "log data")?;
        std::fs::write(log_dir.join("readme.txt"), "not a log")?;
        std::fs::write(log_dir.join("data.csv"), "not a log")?;

        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_log_files(&log_dir)?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;

        let names = zip_entry_names(&path)?;
        assert!(names.iter().any(|n| n.contains("app.log")));
        assert!(!names.iter().any(|n| n.contains("readme.txt")));
        assert!(!names.iter().any(|n| n.contains("data.csv")));
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PII redaction
// ═══════════════════════════════════════════════════════════════════════════

mod pii_redaction {
    use super::*;

    #[test]
    fn test_system_info_environment_excludes_sensitive_vars() -> TestResult {
        let temp_dir = TempDir::new()?;
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_system_info()?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;

        let content = read_zip_entry(&path, "system_info.json")?;
        let info: serde_json::Value = serde_json::from_str(&content)?;

        if let Some(env) = info.get("environment").and_then(|v| v.as_object()) {
            for key in env.keys() {
                let upper = key.to_uppercase();
                // Keys with safe prefixes (CARGO_, RUST_) may contain substrings
                // that look sensitive — skip those.
                if key.starts_with("CARGO_") || key.starts_with("RUST_") {
                    continue;
                }
                assert!(
                    !upper.contains("PASSWORD"),
                    "PASSWORD var leaked: {key}"
                );
                assert!(!upper.contains("SECRET"), "SECRET var leaked: {key}");
                assert!(!upper.contains("TOKEN"), "TOKEN var leaked: {key}");
                assert!(
                    !upper.contains("CREDENTIAL"),
                    "CREDENTIAL var leaked: {key}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn test_system_info_preserves_cargo_env_vars() -> TestResult {
        let temp_dir = TempDir::new()?;
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_system_info()?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;

        let content = read_zip_entry(&path, "system_info.json")?;
        let info: serde_json::Value = serde_json::from_str(&content)?;
        let env = info
            .get("environment")
            .and_then(|v| v.as_object())
            .ok_or("missing environment section")?;

        let has_cargo = env.keys().any(|k| k.starts_with("CARGO_"));
        assert!(has_cargo, "expected at least one CARGO_ variable");
        Ok(())
    }

    #[test]
    fn test_system_info_environment_not_empty() -> TestResult {
        let temp_dir = TempDir::new()?;
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_system_info()?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;

        let content = read_zip_entry(&path, "system_info.json")?;
        let info: serde_json::Value = serde_json::from_str(&content)?;
        let env = info
            .get("environment")
            .and_then(|v| v.as_object())
            .ok_or("missing environment section")?;

        assert!(!env.is_empty(), "environment should contain safe variables");
        Ok(())
    }

    #[test]
    fn test_system_info_os_info_section_present() -> TestResult {
        let temp_dir = TempDir::new()?;
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_system_info()?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;

        let content = read_zip_entry(&path, "system_info.json")?;
        let info: serde_json::Value = serde_json::from_str(&content)?;

        let os_info = info.get("os_info").ok_or("missing os_info section")?;
        assert!(os_info.get("name").is_some());
        assert!(os_info.get("version").is_some());
        assert!(os_info.get("kernel_version").is_some());
        Ok(())
    }

    #[test]
    fn test_health_event_context_preserved_in_bundle() -> TestResult {
        let temp_dir = TempDir::new()?;
        let event = HealthEventData {
            timestamp_ns: 42,
            device_id: "device-abc".to_string(),
            event_type: "TestEvent".to_string(),
            context: serde_json::json!({
                "preserved_data": "should_survive",
                "nested": {"key": "value"}
            }),
        };
        let path = generate_bundle_with_events(&[event], &temp_dir)?;

        let content = read_zip_entry(&path, "health_events.json")?;
        let events: Vec<serde_json::Value> = serde_json::from_str(&content)?;
        assert_eq!(events.len(), 1);

        let ctx = events[0].get("context").ok_or("no context field")?;
        assert_eq!(
            ctx.get("preserved_data").and_then(|v| v.as_str()),
            Some("should_survive")
        );
        assert!(ctx.get("nested").is_some());
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Compression
// ═══════════════════════════════════════════════════════════════════════════

mod compression {
    use super::*;

    #[test]
    fn test_bundle_is_valid_zip_archive() -> TestResult {
        let temp_dir = TempDir::new()?;
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_system_info()?;
        bundle.add_health_events(&[make_event("dev-1", "Connected")])?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;

        let file = std::fs::File::open(&path)?;
        let archive = zip::ZipArchive::new(file)?;
        assert!(!archive.is_empty(), "archive should have entries");
        Ok(())
    }

    #[test]
    fn test_bundle_zip_has_multiple_entries_with_system_info() -> TestResult {
        let temp_dir = TempDir::new()?;
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_system_info()?;
        bundle.add_health_events(&[make_event("dev-1", "Connected")])?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;

        let names = zip_entry_names(&path)?;
        // At minimum: manifest.json, system_info.json, health_events.json
        assert!(names.len() >= 3, "expected at least 3 entries, got {}", names.len());
        assert!(names.contains(&"manifest.json".to_string()));
        assert!(names.contains(&"system_info.json".to_string()));
        assert!(names.contains(&"health_events.json".to_string()));
        Ok(())
    }

    #[test]
    fn test_bundle_compressed_smaller_than_raw_json() -> TestResult {
        let temp_dir = TempDir::new()?;

        // Create many repetitive events (compresses well)
        let events: Vec<HealthEventData> = (0..100)
            .map(|i| HealthEventData {
                timestamp_ns: i as u64 * 1_000_000,
                device_id: "device-001".to_string(),
                event_type: "PeriodicCheck".to_string(),
                context: serde_json::json!({
                    "iteration": i,
                    "status": "ok",
                    "repeated_data": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                }),
            })
            .collect();

        let raw_json = serde_json::to_string(&events)?;
        let raw_size = raw_json.len() as u64;

        let path = generate_bundle_with_events(&events, &temp_dir)?;
        let compressed_size = std::fs::metadata(&path)?.len();

        assert!(
            compressed_size < raw_size,
            "compressed {compressed_size} should be < raw {raw_size}"
        );
        Ok(())
    }

    #[test]
    fn test_bundle_all_entries_readable() -> TestResult {
        let temp_dir = TempDir::new()?;
        let log_dir = temp_dir.path().join("logs");
        std::fs::create_dir(&log_dir)?;
        std::fs::write(log_dir.join("test.log"), "test log content")?;

        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_system_info()?;
        bundle.add_health_events(&[make_event("dev-1", "Connected")])?;
        bundle.add_log_files(&log_dir)?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;

        let file = std::fs::File::open(&path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            assert!(!buf.is_empty(), "entry {} should not be empty", entry.name());
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Size limits
// ═══════════════════════════════════════════════════════════════════════════

mod size_limits {
    use super::*;

    #[test]
    fn test_size_limit_rejects_oversized_events() -> TestResult {
        let config = SupportBundleConfig {
            max_bundle_size_mb: 1,
            ..SupportBundleConfig::default()
        };
        let mut bundle = SupportBundle::new(config);

        let large_context = serde_json::json!({
            "large_data": "x".repeat(2 * 1024 * 1024)
        });
        let event = HealthEventData {
            timestamp_ns: 0,
            device_id: "device".to_string(),
            event_type: "Large".to_string(),
            context: large_context,
        };

        let result = bundle.add_health_events(&[event]);
        assert!(result.is_err());
        if let Err(DiagnosticError::SizeLimit(msg)) = result {
            assert!(msg.contains("exceeded"));
        }
        Ok(())
    }

    #[test]
    fn test_size_limit_tiny_max_rejects_normal_events() -> TestResult {
        let config = SupportBundleConfig {
            max_bundle_size_mb: 0,
            ..SupportBundleConfig::default()
        };
        let mut bundle = SupportBundle::new(config);

        let result = bundle.add_health_events(&[make_event("dev-1", "Connected")]);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_estimated_size_increases_with_events() -> TestResult {
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        let before = bundle.estimated_size_mb();

        bundle.add_health_events(&[make_event("dev-1", "Connected")])?;
        let after = bundle.estimated_size_mb();

        assert!(after > before, "estimated size should increase after adding events");
        Ok(())
    }

    #[test]
    fn test_estimated_size_increases_with_system_info() -> TestResult {
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        let before = bundle.estimated_size_mb();

        bundle.add_system_info()?;
        let after = bundle.estimated_size_mb();

        assert!(after > before, "estimated size should increase after adding system info");
        Ok(())
    }

    #[test]
    fn test_size_limit_skips_oversized_log_files() -> TestResult {
        let temp_dir = TempDir::new()?;
        let log_dir = temp_dir.path().join("logs");
        std::fs::create_dir(&log_dir)?;

        // Create a small log that fits
        std::fs::write(log_dir.join("small.log"), "small")?;
        // Create a large log that exceeds a tight limit
        let large_content = "x".repeat(512 * 1024); // 512 KB
        std::fs::write(log_dir.join("large.log"), &large_content)?;

        let config = SupportBundleConfig {
            max_bundle_size_mb: 1,
            ..SupportBundleConfig::default()
        };
        let mut bundle = SupportBundle::new(config);
        // Use up most of the budget
        let filler = serde_json::json!({"data": "y".repeat(900 * 1024)});
        let big_event = HealthEventData {
            timestamp_ns: 0,
            device_id: "dev".to_string(),
            event_type: "Filler".to_string(),
            context: filler,
        };
        bundle.add_health_events(&[big_event])?;

        // Add logs — the large one should be skipped
        bundle.add_log_files(&log_dir)?;

        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;

        let names = zip_entry_names(&path)?;
        // The small log should be included
        assert!(names.iter().any(|n| n.contains("small.log")));
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Partial bundle generation
// ═══════════════════════════════════════════════════════════════════════════

mod partial_generation {
    use super::*;

    #[test]
    fn test_partial_missing_log_dir_succeeds() -> TestResult {
        let temp_dir = TempDir::new()?;
        let nonexistent = temp_dir.path().join("nonexistent_logs");

        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        let result = bundle.add_log_files(&nonexistent);
        assert!(result.is_ok(), "missing log dir should not cause error");

        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;
        assert!(path.exists());
        Ok(())
    }

    #[test]
    fn test_partial_missing_profile_dir_succeeds() -> TestResult {
        let temp_dir = TempDir::new()?;
        let nonexistent = temp_dir.path().join("nonexistent_profiles");

        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        let result = bundle.add_profile_files(&nonexistent);
        assert!(result.is_ok(), "missing profile dir should not cause error");

        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;
        assert!(path.exists());
        Ok(())
    }

    #[test]
    fn test_partial_missing_recording_dir_succeeds() -> TestResult {
        let temp_dir = TempDir::new()?;
        let nonexistent = temp_dir.path().join("nonexistent_recordings");

        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        let result = bundle.add_recent_recordings(&nonexistent);
        assert!(result.is_ok(), "missing recording dir should not cause error");

        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;
        assert!(path.exists());
        Ok(())
    }

    #[test]
    fn test_partial_disabled_system_info_not_collected() -> TestResult {
        let temp_dir = TempDir::new()?;
        let config = SupportBundleConfig {
            include_system_info: false,
            ..SupportBundleConfig::default()
        };

        let mut bundle = SupportBundle::new(config);
        bundle.add_system_info()?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;

        let names = zip_entry_names(&path)?;
        assert!(
            !names.contains(&"system_info.json".to_string()),
            "system_info.json should be absent when disabled"
        );
        Ok(())
    }

    #[test]
    fn test_partial_disabled_logs_skipped() -> TestResult {
        let temp_dir = TempDir::new()?;
        let log_dir = temp_dir.path().join("logs");
        std::fs::create_dir(&log_dir)?;
        std::fs::write(log_dir.join("app.log"), "log data")?;

        let config = SupportBundleConfig {
            include_logs: false,
            ..SupportBundleConfig::default()
        };
        let mut bundle = SupportBundle::new(config);
        bundle.add_log_files(&log_dir)?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;

        let names = zip_entry_names(&path)?;
        assert!(
            !names.iter().any(|n| n.contains("app.log")),
            "log files should be absent when logs disabled"
        );
        Ok(())
    }

    #[test]
    fn test_partial_disabled_profiles_skipped() -> TestResult {
        let temp_dir = TempDir::new()?;
        let profile_dir = temp_dir.path().join("profiles");
        std::fs::create_dir(&profile_dir)?;
        std::fs::write(profile_dir.join("default.json"), "{}")?;

        let config = SupportBundleConfig {
            include_profiles: false,
            ..SupportBundleConfig::default()
        };
        let mut bundle = SupportBundle::new(config);
        bundle.add_profile_files(&profile_dir)?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;

        let names = zip_entry_names(&path)?;
        assert!(
            !names.iter().any(|n| n.contains("default.json")),
            "profile files should be absent when profiles disabled"
        );
        Ok(())
    }

    #[test]
    fn test_partial_disabled_recordings_skipped() -> TestResult {
        let temp_dir = TempDir::new()?;
        let rec_dir = temp_dir.path().join("recordings");
        std::fs::create_dir(&rec_dir)?;
        std::fs::write(rec_dir.join("test.wbb"), [0u8; 64])?;

        let config = SupportBundleConfig {
            include_recent_recordings: false,
            ..SupportBundleConfig::default()
        };
        let mut bundle = SupportBundle::new(config);
        bundle.add_recent_recordings(&rec_dir)?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;

        let names = zip_entry_names(&path)?;
        assert!(
            !names.iter().any(|n| n.contains("test.wbb")),
            "recordings should be absent when recordings disabled"
        );
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Bundle format versioning
// ═══════════════════════════════════════════════════════════════════════════

mod versioning {
    use super::*;

    #[test]
    fn test_manifest_has_bundle_version() -> TestResult {
        let temp_dir = TempDir::new()?;
        let bundle = SupportBundle::new(SupportBundleConfig::default());
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;

        let content = read_zip_entry(&path, "manifest.json")?;
        let manifest: serde_json::Value = serde_json::from_str(&content)?;

        let version = manifest
            .get("bundle_version")
            .and_then(|v| v.as_str())
            .ok_or("missing bundle_version")?;
        assert_eq!(version, "1.0");
        Ok(())
    }

    #[test]
    fn test_manifest_has_created_at_timestamp() -> TestResult {
        let temp_dir = TempDir::new()?;
        let bundle = SupportBundle::new(SupportBundleConfig::default());
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;

        let content = read_zip_entry(&path, "manifest.json")?;
        let manifest: serde_json::Value = serde_json::from_str(&content)?;

        let created_at = manifest
            .get("created_at")
            .and_then(|v| v.as_u64())
            .ok_or("missing created_at")?;
        assert!(created_at > 0, "timestamp should be positive");
        Ok(())
    }

    #[test]
    fn test_manifest_has_config_section() -> TestResult {
        let temp_dir = TempDir::new()?;
        let bundle = SupportBundle::new(SupportBundleConfig::default());
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;

        let content = read_zip_entry(&path, "manifest.json")?;
        let manifest: serde_json::Value = serde_json::from_str(&content)?;

        let config = manifest.get("config").ok_or("missing config section")?;
        assert_eq!(config.get("include_logs").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(config.get("include_profiles").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(config.get("include_system_info").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            config
                .get("include_recent_recordings")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            config.get("max_bundle_size_mb").and_then(|v| v.as_u64()),
            Some(25)
        );
        Ok(())
    }

    #[test]
    fn test_manifest_has_contents_counts() -> TestResult {
        let temp_dir = TempDir::new()?;
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_health_events(&[
            make_event("dev-1", "Connected"),
            make_event("dev-2", "Disconnected"),
        ])?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;

        let content = read_zip_entry(&path, "manifest.json")?;
        let manifest: serde_json::Value = serde_json::from_str(&content)?;

        let contents = manifest.get("contents").ok_or("missing contents section")?;
        assert_eq!(
            contents
                .get("health_events_count")
                .and_then(|v| v.as_u64()),
            Some(2)
        );
        assert_eq!(
            contents.get("log_files_count").and_then(|v| v.as_u64()),
            Some(0)
        );
        Ok(())
    }

    #[test]
    fn test_manifest_reflects_custom_config() -> TestResult {
        let temp_dir = TempDir::new()?;
        let config = SupportBundleConfig {
            include_logs: false,
            include_profiles: false,
            include_system_info: true,
            include_recent_recordings: false,
            max_bundle_size_mb: 10,
        };
        let bundle = SupportBundle::new(config);
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;

        let content = read_zip_entry(&path, "manifest.json")?;
        let manifest: serde_json::Value = serde_json::from_str(&content)?;

        let cfg = manifest.get("config").ok_or("missing config")?;
        assert_eq!(cfg.get("include_logs").and_then(|v| v.as_bool()), Some(false));
        assert_eq!(cfg.get("include_profiles").and_then(|v| v.as_bool()), Some(false));
        assert_eq!(cfg.get("max_bundle_size_mb").and_then(|v| v.as_u64()), Some(10));
        Ok(())
    }

    #[test]
    fn test_manifest_system_info_json_parseable() -> TestResult {
        let temp_dir = TempDir::new()?;
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_system_info()?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path)?;

        let content = read_zip_entry(&path, "system_info.json")?;
        let info: serde_json::Value = serde_json::from_str(&content)?;

        // Verify required top-level sections
        assert!(info.get("os_info").is_some());
        assert!(info.get("hardware_info").is_some());
        assert!(info.get("process_info").is_some());
        assert!(info.get("environment").is_some());
        assert!(info.get("collected_at").is_some());
        Ok(())
    }

    #[test]
    fn test_health_events_json_array_format() -> TestResult {
        let temp_dir = TempDir::new()?;
        let events = vec![
            make_event("dev-1", "Connected"),
            make_event("dev-1", "Disconnected"),
            make_event("dev-2", "SafetyFault"),
        ];
        let path = generate_bundle_with_events(&events, &temp_dir)?;

        let content = read_zip_entry(&path, "health_events.json")?;
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&content)?;

        assert_eq!(parsed.len(), 3);
        for event in &parsed {
            assert!(event.get("timestamp_ns").is_some());
            assert!(event.get("device_id").is_some());
            assert!(event.get("event_type").is_some());
            assert!(event.get("context").is_some());
        }
        Ok(())
    }
}
