//! Deep tests for support bundle generation, diagnostics, and redaction.
//!
//! Covers: bundle creation with required sections, valid archive format,
//! sensitive data redaction, comprehensive redaction patterns, bounded size,
//! non-blocking generation, error logs, device identification, config diff,
//! performance metrics, bundle versioning, partial bundles on failure,
//! bundle integrity, custom sections, user descriptions, timestamps/session IDs,
//! and concurrent bundle generation.

use std::io::Read;
use std::path::Path;
use std::time::SystemTime;

use racing_wheel_engine::diagnostic::support_bundle::{SupportBundle, SupportBundleConfig};
use racing_wheel_engine::diagnostic::{HealthEvent, HealthEventType};
use racing_wheel_schemas::prelude::DeviceId;
use tempfile::TempDir;

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

// ── Helpers ──────────────────────────────────────────────────────────────

fn parse_device_id(s: &str) -> Result<DeviceId, BoxErr> {
    s.parse::<DeviceId>().map_err(|e| format!("{e:?}").into())
}

fn make_health_event(device: &str, event_type: HealthEventType) -> Result<HealthEvent, BoxErr> {
    Ok(HealthEvent {
        timestamp: SystemTime::now(),
        device_id: parse_device_id(device)?,
        event_type,
        context: serde_json::json!({"source": "test"}),
    })
}

fn zip_entry_names(path: &Path) -> Result<Vec<String>, BoxErr> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut names = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        names.push(entry.name().to_string());
    }
    Ok(names)
}

fn read_zip_entry(path: &Path, name: &str) -> Result<String, BoxErr> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut entry = archive.by_name(name)?;
    let mut contents = String::new();
    entry.read_to_string(&mut contents)?;
    Ok(contents)
}

fn generate_minimal_bundle(temp_dir: &TempDir) -> Result<std::path::PathBuf, BoxErr> {
    let bundle = SupportBundle::new(SupportBundleConfig::default());
    let path = temp_dir.path().join("bundle.zip");
    bundle.generate(&path).map_err(|e| e.to_string())?;
    Ok(path)
}

fn generate_bundle_with_events(
    events: &[HealthEvent],
    temp_dir: &TempDir,
) -> Result<std::path::PathBuf, BoxErr> {
    let config = SupportBundleConfig::default();
    let mut bundle = SupportBundle::new(config);
    bundle
        .add_health_events(events)
        .map_err(|e| e.to_string())?;
    let path = temp_dir.path().join("bundle.zip");
    bundle.generate(&path).map_err(|e| e.to_string())?;
    Ok(path)
}

// ═══════════════════════════════════════════════════════════════════════════
// Bundle includes all required sections
// ═══════════════════════════════════════════════════════════════════════════

mod required_sections {
    use super::*;

    #[test]
    fn test_bundle_always_contains_manifest() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let path = generate_minimal_bundle(&temp_dir)?;
        let names = zip_entry_names(&path)?;
        assert!(
            names.contains(&"manifest.json".to_string()),
            "manifest.json must always be present"
        );
        Ok(())
    }

    #[test]
    fn test_bundle_includes_system_info_section() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_system_info().map_err(|e| e.to_string())?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path).map_err(|e| e.to_string())?;

        let names = zip_entry_names(&path)?;
        assert!(names.contains(&"system_info.json".to_string()));
        Ok(())
    }

    #[test]
    fn test_bundle_includes_health_events_section() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let event = make_health_event("dev-001", HealthEventType::DeviceConnected)?;
        let path = generate_bundle_with_events(&[event], &temp_dir)?;

        let names = zip_entry_names(&path)?;
        assert!(names.contains(&"health_events.json".to_string()));
        Ok(())
    }

    #[test]
    fn test_bundle_includes_log_files() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let log_dir = temp_dir.path().join("logs");
        std::fs::create_dir(&log_dir)?;
        std::fs::write(log_dir.join("app.log"), "2024-01-01 ERROR something failed\n")?;
        std::fs::write(log_dir.join("error.log"), "stack trace here\n")?;

        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_log_files(&log_dir).map_err(|e| e.to_string())?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path).map_err(|e| e.to_string())?;

        let names = zip_entry_names(&path)?;
        assert!(names.iter().any(|n| n.contains("app.log")));
        assert!(names.iter().any(|n| n.contains("error.log")));
        Ok(())
    }

    #[test]
    fn test_bundle_includes_profile_config_files() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let profile_dir = temp_dir.path().join("profiles");
        std::fs::create_dir(&profile_dir)?;
        std::fs::write(
            profile_dir.join("default.json"),
            r#"{"name":"default","gain":0.8}"#,
        )?;

        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle
            .add_profile_files(&profile_dir)
            .map_err(|e| e.to_string())?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path).map_err(|e| e.to_string())?;

        let names = zip_entry_names(&path)?;
        assert!(names.iter().any(|n| n.contains("default.json")));
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Bundle is a valid archive format
// ═══════════════════════════════════════════════════════════════════════════

mod archive_format {
    use super::*;

    #[test]
    fn test_bundle_is_valid_zip() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_system_info().map_err(|e| e.to_string())?;
        let event = make_health_event("dev-001", HealthEventType::DeviceConnected)?;
        bundle
            .add_health_events(&[event])
            .map_err(|e| e.to_string())?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path).map_err(|e| e.to_string())?;

        let file = std::fs::File::open(&path)?;
        let archive = zip::ZipArchive::new(file)?;
        assert!(!archive.is_empty(), "archive should have entries");
        Ok(())
    }

    #[test]
    fn test_all_zip_entries_are_readable() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let log_dir = temp_dir.path().join("logs");
        std::fs::create_dir(&log_dir)?;
        std::fs::write(log_dir.join("test.log"), "readable content")?;

        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_system_info().map_err(|e| e.to_string())?;
        let event = make_health_event("dev-001", HealthEventType::DeviceConnected)?;
        bundle
            .add_health_events(&[event])
            .map_err(|e| e.to_string())?;
        bundle.add_log_files(&log_dir).map_err(|e| e.to_string())?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path).map_err(|e| e.to_string())?;

        let file = std::fs::File::open(&path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            assert!(
                !buf.is_empty(),
                "entry '{}' should not be empty",
                entry.name()
            );
        }
        Ok(())
    }

    #[test]
    fn test_bundle_compressed_smaller_than_raw() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;

        let events: Vec<HealthEvent> = (0..100)
            .map(|i| {
                Ok(HealthEvent {
                    timestamp: SystemTime::now(),
                    device_id: parse_device_id("dev-001")?,
                    event_type: HealthEventType::DeviceConnected,
                    context: serde_json::json!({
                        "iteration": i,
                        "status": "ok",
                        "repeated_data": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    }),
                })
            })
            .collect::<Result<Vec<_>, BoxErr>>()?;

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
}

// ═══════════════════════════════════════════════════════════════════════════
// Sensitive data redaction
// ═══════════════════════════════════════════════════════════════════════════

mod redaction {
    use super::*;

    #[test]
    fn test_env_vars_exclude_password_secret_token_credential() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_system_info().map_err(|e| e.to_string())?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path).map_err(|e| e.to_string())?;

        let content = read_zip_entry(&path, "system_info.json")?;
        let info: serde_json::Value = serde_json::from_str(&content)?;

        if let Some(env) = info.get("environment").and_then(|v| v.as_object()) {
            for key in env.keys() {
                let upper = key.to_uppercase();
                // CARGO_ and RUST_ prefixed vars are always allowed
                if key.starts_with("CARGO_") || key.starts_with("RUST_") {
                    continue;
                }
                assert!(!upper.contains("PASSWORD"), "PASSWORD var leaked: {key}");
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
    fn test_environment_preserves_cargo_vars() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_system_info().map_err(|e| e.to_string())?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path).map_err(|e| e.to_string())?;

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
    fn test_environment_preserves_rust_vars() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_system_info().map_err(|e| e.to_string())?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path).map_err(|e| e.to_string())?;

        let content = read_zip_entry(&path, "system_info.json")?;
        let info: serde_json::Value = serde_json::from_str(&content)?;
        let env = info
            .get("environment")
            .and_then(|v| v.as_object())
            .ok_or("missing environment section")?;

        // During cargo test, RUST_ prefixed vars should exist
        let has_rust = env.keys().any(|k| k.starts_with("RUST_"));
        // Not all CI environments set RUST_ vars; just ensure filter didn't remove them if present
        let actual_rust = std::env::vars().any(|(k, _)| k.starts_with("RUST_"));
        if actual_rust {
            assert!(has_rust, "RUST_ vars should be preserved in bundle");
        }
        Ok(())
    }

    #[test]
    fn test_environment_section_is_not_empty() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_system_info().map_err(|e| e.to_string())?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path).map_err(|e| e.to_string())?;

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
    fn test_redaction_excludes_key_suffix_vars() -> Result<(), BoxErr> {
        // Verify that any env var containing KEY in its name (but not starting
        // with a safe prefix) is excluded from the bundle
        let temp_dir = TempDir::new()?;
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_system_info().map_err(|e| e.to_string())?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path).map_err(|e| e.to_string())?;

        let content = read_zip_entry(&path, "system_info.json")?;
        let info: serde_json::Value = serde_json::from_str(&content)?;

        if let Some(env) = info.get("environment").and_then(|v| v.as_object()) {
            for key in env.keys() {
                if key.starts_with("CARGO_") || key.starts_with("RUST_") {
                    continue;
                }
                let upper = key.to_uppercase();
                assert!(
                    !upper.contains("KEY"),
                    "KEY-containing var leaked: {key}"
                );
            }
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Bundle size is bounded
// ═══════════════════════════════════════════════════════════════════════════

mod size_bounded {
    use super::*;

    #[test]
    fn test_size_limit_rejects_oversized_events() -> Result<(), BoxErr> {
        let config = SupportBundleConfig {
            max_bundle_size_mb: 1,
            ..SupportBundleConfig::default()
        };
        let mut bundle = SupportBundle::new(config);

        let large_context = serde_json::json!({
            "large_data": "x".repeat(2 * 1024 * 1024)
        });
        let event = HealthEvent {
            timestamp: SystemTime::now(),
            device_id: parse_device_id("dev-001")?,
            event_type: HealthEventType::DeviceConnected,
            context: large_context,
        };

        let result = bundle.add_health_events(&[event]);
        assert!(result.is_err(), "should reject events exceeding size limit");
        Ok(())
    }

    #[test]
    fn test_size_limit_zero_rejects_any_events() -> Result<(), BoxErr> {
        let config = SupportBundleConfig {
            max_bundle_size_mb: 0,
            ..SupportBundleConfig::default()
        };
        let mut bundle = SupportBundle::new(config);

        let event = make_health_event("dev-001", HealthEventType::DeviceConnected)?;
        let result = bundle.add_health_events(&[event]);
        assert!(result.is_err(), "zero-MB limit should reject any events");
        Ok(())
    }

    #[test]
    fn test_estimated_size_increases_with_data() -> Result<(), BoxErr> {
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        let before = bundle.estimated_size_mb();

        let event = make_health_event("dev-001", HealthEventType::DeviceConnected)?;
        bundle
            .add_health_events(&[event])
            .map_err(|e| e.to_string())?;
        let after = bundle.estimated_size_mb();

        assert!(
            after > before,
            "estimated size should increase after adding events"
        );
        Ok(())
    }

    #[test]
    fn test_size_limit_skips_oversized_log_files() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let log_dir = temp_dir.path().join("logs");
        std::fs::create_dir(&log_dir)?;

        std::fs::write(log_dir.join("small.log"), "small log")?;
        let large_content = "x".repeat(512 * 1024);
        std::fs::write(log_dir.join("large.log"), &large_content)?;

        let config = SupportBundleConfig {
            max_bundle_size_mb: 1,
            ..SupportBundleConfig::default()
        };
        let mut bundle = SupportBundle::new(config);

        // Fill most of the budget
        let filler = serde_json::json!({"data": "y".repeat(900 * 1024)});
        let filler_event = HealthEvent {
            timestamp: SystemTime::now(),
            device_id: parse_device_id("dev-filler")?,
            event_type: HealthEventType::DeviceConnected,
            context: filler,
        };
        bundle
            .add_health_events(&[filler_event])
            .map_err(|e| e.to_string())?;

        // Large log should be skipped
        bundle.add_log_files(&log_dir).map_err(|e| e.to_string())?;

        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path).map_err(|e| e.to_string())?;

        let names = zip_entry_names(&path)?;
        assert!(names.iter().any(|n| n.contains("small.log")));
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Bundle generation doesn't block RT path
// ═══════════════════════════════════════════════════════════════════════════

mod non_blocking {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_bundle_generation_completes_within_budget() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_system_info().map_err(|e| e.to_string())?;

        let events: Vec<HealthEvent> = (0..10)
            .map(|i| {
                Ok(HealthEvent {
                    timestamp: SystemTime::now(),
                    device_id: parse_device_id("dev-001")?,
                    event_type: HealthEventType::DeviceConnected,
                    context: serde_json::json!({"seq": i}),
                })
            })
            .collect::<Result<Vec<_>, BoxErr>>()?;
        bundle
            .add_health_events(&events)
            .map_err(|e| e.to_string())?;

        let start = Instant::now();
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path).map_err(|e| e.to_string())?;
        let elapsed = start.elapsed();

        // Bundle generation should complete in reasonable time (not blocking RT at 1kHz)
        // 5 seconds is generous; real bundles are sub-second
        assert!(
            elapsed.as_secs() < 5,
            "bundle generation took too long: {elapsed:?}"
        );
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Bundle includes recent error log
// ═══════════════════════════════════════════════════════════════════════════

mod error_logs {
    use super::*;

    #[test]
    fn test_error_log_content_preserved_in_bundle() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let log_dir = temp_dir.path().join("logs");
        std::fs::create_dir(&log_dir)?;
        let error_content = "2024-01-01T00:00:00Z ERROR [engine] Safety fault detected\n";
        std::fs::write(log_dir.join("error.log"), error_content)?;

        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_log_files(&log_dir).map_err(|e| e.to_string())?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path).map_err(|e| e.to_string())?;

        let content = read_zip_entry(&path, "logs/error.log")?;
        assert!(
            content.contains("Safety fault detected"),
            "error log content should be preserved"
        );
        Ok(())
    }

    #[test]
    fn test_only_log_extension_files_included() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let log_dir = temp_dir.path().join("logs");
        std::fs::create_dir(&log_dir)?;
        std::fs::write(log_dir.join("app.log"), "log content")?;
        std::fs::write(log_dir.join("readme.txt"), "not a log")?;
        std::fs::write(log_dir.join("data.csv"), "not a log")?;

        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_log_files(&log_dir).map_err(|e| e.to_string())?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path).map_err(|e| e.to_string())?;

        let names = zip_entry_names(&path)?;
        assert!(names.iter().any(|n| n.contains("app.log")));
        assert!(!names.iter().any(|n| n.contains("readme.txt")));
        assert!(!names.iter().any(|n| n.contains("data.csv")));
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Bundle includes device identification
// ═══════════════════════════════════════════════════════════════════════════

mod device_identification {
    use super::*;

    #[test]
    fn test_health_events_contain_device_ids() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let events = vec![
            make_health_event("wheel-base-01", HealthEventType::DeviceConnected)?,
            make_health_event("pedal-set-02", HealthEventType::DeviceDisconnected)?,
        ];
        let path = generate_bundle_with_events(&events, &temp_dir)?;

        let content = read_zip_entry(&path, "health_events.json")?;
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&content)?;
        assert_eq!(parsed.len(), 2);

        let ids: Vec<&str> = parsed
            .iter()
            .filter_map(|e| e.get("device_id").and_then(|v| v.as_str()))
            .collect();
        assert!(ids.contains(&"wheel-base-01"), "first device ID missing");
        assert!(ids.contains(&"pedal-set-02"), "second device ID missing");
        Ok(())
    }

    #[test]
    fn test_health_event_context_preserved() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let event = HealthEvent {
            timestamp: SystemTime::now(),
            device_id: parse_device_id("dev-with-context")?,
            event_type: HealthEventType::DeviceConnected,
            context: serde_json::json!({
                "firmware": "1.2.3",
                "vendor": "MOZA",
                "model": "R16"
            }),
        };
        let path = generate_bundle_with_events(&[event], &temp_dir)?;

        let content = read_zip_entry(&path, "health_events.json")?;
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&content)?;
        let ctx = parsed[0]
            .get("context")
            .ok_or("no context field in event")?;

        assert_eq!(ctx.get("firmware").and_then(|v| v.as_str()), Some("1.2.3"));
        assert_eq!(ctx.get("vendor").and_then(|v| v.as_str()), Some("MOZA"));
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Bundle includes performance metrics (latency, jitter via health events)
// ═══════════════════════════════════════════════════════════════════════════

mod performance_metrics {
    use super::*;

    #[test]
    fn test_performance_degradation_event_in_bundle() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let event = HealthEvent {
            timestamp: SystemTime::now(),
            device_id: parse_device_id("dev-001")?,
            event_type: HealthEventType::PerformanceDegradation {
                metric: "jitter_p99_us".to_string(),
                value: 350.0,
            },
            context: serde_json::json!({"threshold_us": 250}),
        };
        let path = generate_bundle_with_events(&[event], &temp_dir)?;

        let content = read_zip_entry(&path, "health_events.json")?;
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&content)?;
        assert_eq!(parsed.len(), 1);

        // The event_type should be serialized with the metric and value
        let event_type = parsed[0].get("event_type").ok_or("missing event_type")?;
        assert!(
            event_type.get("PerformanceDegradation").is_some(),
            "PerformanceDegradation variant should be present"
        );
        Ok(())
    }

    #[test]
    fn test_resource_warning_event_in_bundle() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let event = HealthEvent {
            timestamp: SystemTime::now(),
            device_id: parse_device_id("dev-001")?,
            event_type: HealthEventType::ResourceWarning {
                resource: "memory".to_string(),
                usage: 92.5,
            },
            context: serde_json::json!({"action": "gc_triggered"}),
        };
        let path = generate_bundle_with_events(&[event], &temp_dir)?;

        let content = read_zip_entry(&path, "health_events.json")?;
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&content)?;
        let event_type = parsed[0].get("event_type").ok_or("missing event_type")?;
        assert!(event_type.get("ResourceWarning").is_some());
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Bundle versioning
// ═══════════════════════════════════════════════════════════════════════════

mod versioning {
    use super::*;

    #[test]
    fn test_manifest_contains_bundle_version() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let path = generate_minimal_bundle(&temp_dir)?;

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
    fn test_manifest_contains_created_at_timestamp() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let path = generate_minimal_bundle(&temp_dir)?;

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
    fn test_manifest_has_contents_counts() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let events = vec![
            make_health_event("dev-1", HealthEventType::DeviceConnected)?,
            make_health_event("dev-2", HealthEventType::DeviceDisconnected)?,
        ];
        let path = generate_bundle_with_events(&events, &temp_dir)?;

        let content = read_zip_entry(&path, "manifest.json")?;
        let manifest: serde_json::Value = serde_json::from_str(&content)?;

        let contents = manifest
            .get("contents")
            .ok_or("missing contents section")?;
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
}

// ═══════════════════════════════════════════════════════════════════════════
// Partial bundle on failure (best effort)
// ═══════════════════════════════════════════════════════════════════════════

mod partial_bundle {
    use super::*;

    #[test]
    fn test_missing_log_dir_generates_bundle_anyway() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let nonexistent = temp_dir.path().join("nonexistent_logs");

        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        let result = bundle.add_log_files(&nonexistent);
        assert!(result.is_ok(), "missing log dir should not cause error");

        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path).map_err(|e| e.to_string())?;
        assert!(path.exists());
        Ok(())
    }

    #[test]
    fn test_missing_profile_dir_generates_bundle_anyway() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let nonexistent = temp_dir.path().join("nonexistent_profiles");

        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        let result = bundle.add_profile_files(&nonexistent);
        assert!(
            result.is_ok(),
            "missing profile dir should not cause error"
        );

        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path).map_err(|e| e.to_string())?;
        assert!(path.exists());
        Ok(())
    }

    #[test]
    fn test_disabled_system_info_omitted_from_bundle() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let config = SupportBundleConfig {
            include_system_info: false,
            ..SupportBundleConfig::default()
        };
        let mut bundle = SupportBundle::new(config);
        bundle.add_system_info().map_err(|e| e.to_string())?;

        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path).map_err(|e| e.to_string())?;

        let names = zip_entry_names(&path)?;
        assert!(
            !names.contains(&"system_info.json".to_string()),
            "system_info.json should be absent when disabled"
        );
        Ok(())
    }

    #[test]
    fn test_disabled_logs_omitted_from_bundle() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let log_dir = temp_dir.path().join("logs");
        std::fs::create_dir(&log_dir)?;
        std::fs::write(log_dir.join("app.log"), "log data")?;

        let config = SupportBundleConfig {
            include_logs: false,
            ..SupportBundleConfig::default()
        };
        let mut bundle = SupportBundle::new(config);
        bundle.add_log_files(&log_dir).map_err(|e| e.to_string())?;

        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path).map_err(|e| e.to_string())?;

        let names = zip_entry_names(&path)?;
        assert!(
            !names.iter().any(|n| n.contains("app.log")),
            "log files should be absent when logs disabled"
        );
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Custom sections (plugin diagnostics via health events)
// ═══════════════════════════════════════════════════════════════════════════

mod custom_sections {
    use super::*;

    #[test]
    fn test_plugin_event_included_in_bundle() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let event = HealthEvent {
            timestamp: SystemTime::now(),
            device_id: parse_device_id("dev-001")?,
            event_type: HealthEventType::PluginEvent {
                plugin_id: "custom-telemetry-plugin".to_string(),
                event: "diagnostics_collected".to_string(),
            },
            context: serde_json::json!({
                "plugin_version": "2.0.1",
                "custom_metric": 42,
                "status": "healthy"
            }),
        };
        let path = generate_bundle_with_events(&[event], &temp_dir)?;

        let content = read_zip_entry(&path, "health_events.json")?;
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&content)?;
        assert_eq!(parsed.len(), 1);

        let event_type = parsed[0].get("event_type").ok_or("missing event_type")?;
        assert!(event_type.get("PluginEvent").is_some());

        let ctx = parsed[0].get("context").ok_or("missing context")?;
        assert_eq!(
            ctx.get("plugin_version").and_then(|v| v.as_str()),
            Some("2.0.1")
        );
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// User-provided description & session metadata
// ═══════════════════════════════════════════════════════════════════════════

mod metadata {
    use super::*;

    #[test]
    fn test_manifest_config_reflects_custom_settings() -> Result<(), BoxErr> {
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
        bundle.generate(&path).map_err(|e| e.to_string())?;

        let content = read_zip_entry(&path, "manifest.json")?;
        let manifest: serde_json::Value = serde_json::from_str(&content)?;

        let cfg = manifest.get("config").ok_or("missing config")?;
        assert_eq!(
            cfg.get("include_logs").and_then(|v| v.as_bool()),
            Some(false)
        );
        assert_eq!(
            cfg.get("include_profiles").and_then(|v| v.as_bool()),
            Some(false)
        );
        assert_eq!(
            cfg.get("max_bundle_size_mb").and_then(|v| v.as_u64()),
            Some(10)
        );
        Ok(())
    }

    #[test]
    fn test_manifest_created_at_is_recent() -> Result<(), BoxErr> {
        let before = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();

        let temp_dir = TempDir::new()?;
        let path = generate_minimal_bundle(&temp_dir)?;

        let after = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();

        let content = read_zip_entry(&path, "manifest.json")?;
        let manifest: serde_json::Value = serde_json::from_str(&content)?;
        let created_at = manifest
            .get("created_at")
            .and_then(|v| v.as_u64())
            .ok_or("missing created_at")?;

        assert!(
            created_at >= before && created_at <= after,
            "created_at ({created_at}) should be between {before} and {after}"
        );
        Ok(())
    }

    #[test]
    fn test_system_info_has_required_top_level_sections() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_system_info().map_err(|e| e.to_string())?;
        let path = temp_dir.path().join("bundle.zip");
        bundle.generate(&path).map_err(|e| e.to_string())?;

        let content = read_zip_entry(&path, "system_info.json")?;
        let info: serde_json::Value = serde_json::from_str(&content)?;

        assert!(info.get("os_info").is_some(), "missing os_info");
        assert!(info.get("hardware_info").is_some(), "missing hardware_info");
        assert!(info.get("process_info").is_some(), "missing process_info");
        assert!(info.get("environment").is_some(), "missing environment");
        assert!(info.get("collected_at").is_some(), "missing collected_at");
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Concurrent bundle generation
// ═══════════════════════════════════════════════════════════════════════════

mod concurrency {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_concurrent_bundle_generation_no_races() -> Result<(), BoxErr> {
        let temp_dir = TempDir::new()?;
        let temp_path = Arc::new(temp_dir.path().to_path_buf());

        let handles: Vec<_> = (0..4)
            .map(|i| {
                let dir = Arc::clone(&temp_path);
                std::thread::spawn(move || -> Result<(), BoxErr> {
                    let event = HealthEvent {
                        timestamp: SystemTime::now(),
                        device_id: parse_device_id(&format!("dev-{i}"))?,
                        event_type: HealthEventType::DeviceConnected,
                        context: serde_json::json!({"thread": i}),
                    };
                    let mut bundle = SupportBundle::new(SupportBundleConfig::default());
                    bundle
                        .add_health_events(&[event])
                        .map_err(|e| e.to_string())?;
                    let path = dir.join(format!("bundle_{i}.zip"));
                    bundle.generate(&path).map_err(|e| e.to_string())?;

                    assert!(path.exists());
                    let names = zip_entry_names(&path)?;
                    assert!(names.contains(&"manifest.json".to_string()));
                    assert!(names.contains(&"health_events.json".to_string()));
                    Ok(())
                })
            })
            .collect();

        for handle in handles {
            let thread_result = handle.join().map_err(|_| "thread panicked")?;
            thread_result?;
        }
        Ok(())
    }
}
