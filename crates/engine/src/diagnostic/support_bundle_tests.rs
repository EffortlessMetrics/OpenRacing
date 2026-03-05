//! Comprehensive tests for support bundle generation and diagnostic export.
//!
//! Covers: bundle creation, redaction/sanitization, format validation,
//! diagnostic snapshot inclusion, log capture, device state, config sanitization,
//! size limits, error handling, reproducibility, and property-based redaction tests.

use super::support_bundle::*;
use super::*;
use proptest::prelude::*;
use racing_wheel_schemas::prelude::DeviceId;
use std::collections::HashMap;
use std::fs::{self, write};
use std::io::Read;
use std::time::SystemTime;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[track_caller]
fn must_parse<T: std::str::FromStr>(s: &str) -> T
where
    <T as std::str::FromStr>::Err: std::fmt::Debug,
{
    match s.parse::<T>() {
        Ok(v) => v,
        Err(e) => panic!("parse failed for {s:?}: {e:?}"),
    }
}

fn make_health_events(count: usize) -> Vec<HealthEvent> {
    let device_id = must_parse::<DeviceId>("test-device");
    (0..count)
        .map(|i| {
            let event_type = match i % 4 {
                0 => HealthEventType::DeviceConnected,
                1 => HealthEventType::DeviceDisconnected,
                2 => HealthEventType::PerformanceDegradation {
                    metric: "jitter".into(),
                    value: i as f64 * 0.001,
                },
                _ => HealthEventType::ConfigurationChange {
                    change_type: "profile_update".into(),
                },
            };
            HealthEvent {
                timestamp: SystemTime::now(),
                device_id: device_id.clone(),
                event_type,
                context: serde_json::json!({"iter": i}),
            }
        })
        .collect()
}

/// Build a fully-populated `SupportBundle`, generate the ZIP, and return
/// `(zip_path, TempDir)`. The `TempDir` keeps temp files alive.
fn build_full_bundle() -> Result<(std::path::PathBuf, TempDir), String> {
    let temp_dir = TempDir::new().map_err(|e| format!("tempdir: {e}"))?;

    // Logs
    let log_dir = temp_dir.path().join("logs");
    fs::create_dir_all(&log_dir).map_err(|e| format!("mkdir logs: {e}"))?;
    write(log_dir.join("app.log"), "INFO: started\nWARN: slow tick").map_err(|e| e.to_string())?;
    write(log_dir.join("error.log"), "ERROR: device timeout").map_err(|e| e.to_string())?;

    // Profiles
    let profile_dir = temp_dir.path().join("profiles");
    fs::create_dir_all(&profile_dir).map_err(|e| format!("mkdir profiles: {e}"))?;
    write(
        profile_dir.join("default.json"),
        r#"{"ffb_gain":0.8,"dor_deg":900}"#,
    )
    .map_err(|e| e.to_string())?;
    write(profile_dir.join("iracing.profile"), r#"{"game":"iracing"}"#)
        .map_err(|e| e.to_string())?;

    // Recordings dir (empty is fine – tests bundle without .wbb files)
    let rec_dir = temp_dir.path().join("recordings");
    fs::create_dir_all(&rec_dir).map_err(|e| format!("mkdir recordings: {e}"))?;

    let config = SupportBundleConfig {
        include_logs: true,
        include_profiles: true,
        include_system_info: true,
        include_recent_recordings: true,
        max_bundle_size_mb: 25,
    };
    let mut bundle = SupportBundle::new(config);
    bundle.add_health_events(&make_health_events(10))?;
    bundle.add_system_info()?;
    bundle.add_log_files(&log_dir)?;
    bundle.add_profile_files(&profile_dir)?;
    bundle.add_recent_recordings(&rec_dir)?;

    let zip_path = temp_dir.path().join("bundle.zip");
    bundle.generate(&zip_path)?;
    Ok((zip_path, temp_dir))
}

/// Open a generated ZIP and return a map of entry-name → decompressed bytes.
fn read_zip_entries(path: &std::path::Path) -> Result<HashMap<String, Vec<u8>>, String> {
    let file = fs::File::open(path).map_err(|e| format!("open: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("zip: {e}"))?;
    let mut entries: HashMap<String, Vec<u8>> = HashMap::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("entry {i}: {e}"))?;
        let name = entry.name().to_string();
        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .map_err(|e| format!("read {name}: {e}"))?;
        entries.insert(name, buf);
    }
    Ok(entries)
}

// =========================================================================
// 1. Bundle creation – contains all required sections
// =========================================================================

#[test]
fn test_bundle_contains_manifest() -> Result<(), String> {
    let (zip_path, _td) = build_full_bundle()?;
    let entries = read_zip_entries(&zip_path)?;
    assert!(
        entries.contains_key("manifest.json"),
        "manifest.json missing"
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&entries["manifest.json"]).map_err(|e| e.to_string())?;
    assert_eq!(manifest["bundle_version"], "1.0");
    Ok(())
}

#[test]
fn test_bundle_contains_system_info() -> Result<(), String> {
    let (zip_path, _td) = build_full_bundle()?;
    let entries = read_zip_entries(&zip_path)?;
    assert!(
        entries.contains_key("system_info.json"),
        "system_info.json missing"
    );
    let info: SystemInfo =
        serde_json::from_slice(&entries["system_info.json"]).map_err(|e| e.to_string())?;
    assert!(!info.os_info.name.is_empty());
    assert!(info.hardware_info.cpu_info.core_count > 0);
    assert!(info.hardware_info.memory_info.total_mb > 0);
    Ok(())
}

#[test]
fn test_bundle_contains_health_events() -> Result<(), String> {
    let (zip_path, _td) = build_full_bundle()?;
    let entries = read_zip_entries(&zip_path)?;
    assert!(
        entries.contains_key("health_events.json"),
        "health_events.json missing"
    );
    let events: Vec<HealthEvent> =
        serde_json::from_slice(&entries["health_events.json"]).map_err(|e| e.to_string())?;
    assert_eq!(events.len(), 10);
    Ok(())
}

#[test]
fn test_bundle_contains_log_files() -> Result<(), String> {
    let (zip_path, _td) = build_full_bundle()?;
    let entries = read_zip_entries(&zip_path)?;
    assert!(entries.contains_key("logs/app.log"), "logs/app.log missing");
    assert!(
        entries.contains_key("logs/error.log"),
        "logs/error.log missing"
    );
    Ok(())
}

#[test]
fn test_bundle_contains_profiles() -> Result<(), String> {
    let (zip_path, _td) = build_full_bundle()?;
    let entries = read_zip_entries(&zip_path)?;
    assert!(
        entries.contains_key("profiles/default.json"),
        "profiles/default.json missing"
    );
    assert!(
        entries.contains_key("profiles/iracing.profile"),
        "profiles/iracing.profile missing"
    );
    Ok(())
}

#[test]
fn test_bundle_manifest_counts_match() -> Result<(), String> {
    let (zip_path, _td) = build_full_bundle()?;
    let entries = read_zip_entries(&zip_path)?;
    let manifest: serde_json::Value =
        serde_json::from_slice(&entries["manifest.json"]).map_err(|e| e.to_string())?;

    let log_count = manifest["contents"]["log_files_count"]
        .as_u64()
        .ok_or("missing log_files_count")?;
    let profile_count = manifest["contents"]["profile_files_count"]
        .as_u64()
        .ok_or("missing profile_files_count")?;
    let he_count = manifest["contents"]["health_events_count"]
        .as_u64()
        .ok_or("missing health_events_count")?;

    assert_eq!(log_count, 2);
    assert_eq!(profile_count, 2);
    assert_eq!(he_count, 10);
    Ok(())
}

// =========================================================================
// 2. Redaction / sanitization – sensitive fields masked
// =========================================================================

#[test]
fn test_env_var_password_filtered() -> Result<(), String> {
    assert!(!SupportBundle::is_safe_env_var("MY_PASSWORD"));
    assert!(!SupportBundle::is_safe_env_var("DB_PASSWORD_123"));
    Ok(())
}

#[test]
fn test_env_var_secret_filtered() -> Result<(), String> {
    assert!(!SupportBundle::is_safe_env_var("SECRET_KEY"));
    assert!(!SupportBundle::is_safe_env_var("MY_SECRET"));
    Ok(())
}

#[test]
fn test_env_var_token_filtered() -> Result<(), String> {
    assert!(!SupportBundle::is_safe_env_var("API_TOKEN"));
    assert!(!SupportBundle::is_safe_env_var("AUTH_TOKEN_V2"));
    Ok(())
}

#[test]
fn test_env_var_credential_filtered() -> Result<(), String> {
    assert!(!SupportBundle::is_safe_env_var("DATABASE_CREDENTIAL"));
    assert!(!SupportBundle::is_safe_env_var("CREDENTIAL_FILE"));
    Ok(())
}

#[test]
fn test_env_var_key_filtered() -> Result<(), String> {
    assert!(!SupportBundle::is_safe_env_var("ENCRYPTION_KEY"));
    assert!(!SupportBundle::is_safe_env_var("PRIVATE_KEY_PATH"));
    Ok(())
}

#[test]
fn test_safe_env_vars_allowed() -> Result<(), String> {
    assert!(SupportBundle::is_safe_env_var("CARGO_PKG_NAME"));
    assert!(SupportBundle::is_safe_env_var("RUST_LOG"));
    assert!(SupportBundle::is_safe_env_var("PATH"));
    assert!(SupportBundle::is_safe_env_var("HOME"));
    assert!(SupportBundle::is_safe_env_var("USERNAME"));
    assert!(SupportBundle::is_safe_env_var("COMPUTERNAME"));
    assert!(SupportBundle::is_safe_env_var("OS"));
    assert!(SupportBundle::is_safe_env_var("PROCESSOR_ARCHITECTURE"));
    Ok(())
}

#[test]
fn test_system_info_env_vars_exclude_sensitive() -> Result<(), String> {
    let info = SupportBundle::collect_system_info()?;
    for key in info.environment.keys() {
        let upper = key.to_uppercase();
        assert!(
            !upper.contains("PASSWORD"),
            "env contains PASSWORD key: {key}"
        );
        assert!(!upper.contains("SECRET"), "env contains SECRET key: {key}");
        assert!(!upper.contains("TOKEN"), "env contains TOKEN key: {key}");
        assert!(
            !upper.contains("CREDENTIAL"),
            "env contains CREDENTIAL key: {key}"
        );
    }
    Ok(())
}

// =========================================================================
// 3. Bundle format – valid ZIP structure
// =========================================================================

#[test]
fn test_bundle_is_valid_zip() -> Result<(), String> {
    let (zip_path, _td) = build_full_bundle()?;
    let file = fs::File::open(&zip_path).map_err(|e| e.to_string())?;
    let archive = zip::ZipArchive::new(file).map_err(|e| format!("invalid zip: {e}"))?;
    assert!(!archive.is_empty(), "zip has no entries");
    Ok(())
}

#[test]
fn test_bundle_json_entries_parse() -> Result<(), String> {
    let (zip_path, _td) = build_full_bundle()?;
    let entries = read_zip_entries(&zip_path)?;
    for (name, data) in &entries {
        if name.ends_with(".json") {
            let _: serde_json::Value =
                serde_json::from_slice(data).map_err(|e| format!("{name} bad JSON: {e}"))?;
        }
    }
    Ok(())
}

#[test]
fn test_empty_bundle_generates_valid_zip() -> Result<(), String> {
    let td = TempDir::new().map_err(|e| e.to_string())?;
    let config = SupportBundleConfig {
        include_logs: false,
        include_profiles: false,
        include_system_info: false,
        include_recent_recordings: false,
        max_bundle_size_mb: 1,
    };
    let bundle = SupportBundle::new(config);
    let zip_path = td.path().join("empty_bundle.zip");
    bundle.generate(&zip_path)?;

    let entries = read_zip_entries(&zip_path)?;
    // Should at least have manifest
    assert!(entries.contains_key("manifest.json"));
    Ok(())
}

// =========================================================================
// 4. Diagnostic snapshot inclusion – metrics present and correct
// =========================================================================

#[test]
fn test_diagnostic_service_health_events_in_bundle() -> Result<(), String> {
    let td = TempDir::new().map_err(|e| e.to_string())?;
    let config = DiagnosticConfig {
        enable_recording: true,
        max_recording_duration_s: 10,
        recording_dir: td.path().join("rec"),
        max_file_size_bytes: 1024 * 1024,
        compression_level: 1,
        enable_stream_a: true,
        enable_stream_b: true,
        enable_stream_c: true,
    };
    let mut service = DiagnosticService::new(config)?;

    let device_id = must_parse::<DeviceId>("diag-device");
    for i in 0..5 {
        service.record_health_event(HealthEvent {
            timestamp: SystemTime::now(),
            device_id: device_id.clone(),
            event_type: HealthEventType::PerformanceDegradation {
                metric: "jitter".into(),
                value: i as f64 * 0.1,
            },
            context: serde_json::json!({"i": i}),
        });
    }

    let bundle_path = td.path().join("diag_bundle.zip");
    service.generate_support_bundle(&bundle_path)?;

    assert!(bundle_path.exists());
    let entries = read_zip_entries(&bundle_path)?;
    assert!(entries.contains_key("health_events.json"));

    let events: Vec<HealthEvent> =
        serde_json::from_slice(&entries["health_events.json"]).map_err(|e| e.to_string())?;
    assert_eq!(events.len(), 5);
    Ok(())
}

#[test]
fn test_health_event_types_preserved_in_bundle() -> Result<(), String> {
    let td = TempDir::new().map_err(|e| e.to_string())?;
    let config = SupportBundleConfig::default();
    let mut bundle = SupportBundle::new(config);

    let device_id = must_parse::<DeviceId>("test-device");
    let events = vec![
        HealthEvent {
            timestamp: SystemTime::now(),
            device_id: device_id.clone(),
            event_type: HealthEventType::DeviceConnected,
            context: serde_json::json!({}),
        },
        HealthEvent {
            timestamp: SystemTime::now(),
            device_id: device_id.clone(),
            event_type: HealthEventType::SafetyFault {
                fault_type: crate::safety::FaultType::ThermalLimit,
            },
            context: serde_json::json!({}),
        },
        HealthEvent {
            timestamp: SystemTime::now(),
            device_id: device_id.clone(),
            event_type: HealthEventType::ResourceWarning {
                resource: "cpu".into(),
                usage: 95.0,
            },
            context: serde_json::json!({}),
        },
    ];
    bundle.add_health_events(&events)?;

    let zip_path = td.path().join("events.zip");
    bundle.generate(&zip_path)?;

    let entries = read_zip_entries(&zip_path)?;
    let parsed: Vec<serde_json::Value> =
        serde_json::from_slice(&entries["health_events.json"]).map_err(|e| e.to_string())?;
    assert_eq!(parsed.len(), 3);
    Ok(())
}

// =========================================================================
// 5. Log capture – recent logs included with truncation
// =========================================================================

#[test]
fn test_log_files_included_in_bundle() -> Result<(), String> {
    let (zip_path, _td) = build_full_bundle()?;
    let entries = read_zip_entries(&zip_path)?;

    let app_log = std::str::from_utf8(&entries["logs/app.log"]).map_err(|e| e.to_string())?;
    assert!(app_log.contains("INFO: started"));
    assert!(app_log.contains("WARN: slow tick"));
    Ok(())
}

#[test]
fn test_non_log_files_excluded() -> Result<(), String> {
    let td = TempDir::new().map_err(|e| e.to_string())?;
    let log_dir = td.path().join("logs");
    fs::create_dir_all(&log_dir).map_err(|e| e.to_string())?;
    write(log_dir.join("app.log"), "log data").map_err(|e| e.to_string())?;
    write(log_dir.join("readme.txt"), "not a log").map_err(|e| e.to_string())?;
    write(log_dir.join("data.csv"), "1,2,3").map_err(|e| e.to_string())?;

    let config = SupportBundleConfig::default();
    let mut bundle = SupportBundle::new(config);
    bundle.add_log_files(&log_dir)?;

    let zip_path = td.path().join("logs.zip");
    bundle.generate(&zip_path)?;

    let entries = read_zip_entries(&zip_path)?;
    assert!(entries.contains_key("logs/app.log"));
    assert!(!entries.contains_key("logs/readme.txt"));
    assert!(!entries.contains_key("logs/data.csv"));
    Ok(())
}

#[test]
fn test_missing_log_dir_ok() -> Result<(), String> {
    let td = TempDir::new().map_err(|e| e.to_string())?;
    let config = SupportBundleConfig::default();
    let mut bundle = SupportBundle::new(config);
    let result = bundle.add_log_files(&td.path().join("nonexistent_logs"));
    assert!(result.is_ok());
    Ok(())
}

#[test]
fn test_large_log_file_skipped_when_exceeds_limit() -> Result<(), String> {
    let td = TempDir::new().map_err(|e| e.to_string())?;
    let log_dir = td.path().join("logs");
    fs::create_dir_all(&log_dir).map_err(|e| e.to_string())?;

    // Create a log file larger than the 1 MB bundle limit
    let big_content = "x".repeat(2 * 1024 * 1024);
    write(log_dir.join("huge.log"), &big_content).map_err(|e| e.to_string())?;
    write(log_dir.join("small.log"), "tiny").map_err(|e| e.to_string())?;

    let config = SupportBundleConfig {
        max_bundle_size_mb: 1,
        ..Default::default()
    };
    let mut bundle = SupportBundle::new(config);
    bundle.add_log_files(&log_dir)?;

    // The huge log should have been skipped; the small one should remain
    let zip_path = td.path().join("limited.zip");
    bundle.generate(&zip_path)?;

    let entries = read_zip_entries(&zip_path)?;
    assert!(
        !entries.contains_key("logs/huge.log"),
        "huge log should be skipped"
    );
    assert!(entries.contains_key("logs/small.log"));
    Ok(())
}

// =========================================================================
// 6. Device state – connected device info captured
// =========================================================================

#[test]
fn test_device_events_round_trip() -> Result<(), String> {
    let td = TempDir::new().map_err(|e| e.to_string())?;
    let config = SupportBundleConfig::default();
    let mut bundle = SupportBundle::new(config);

    let device_id = must_parse::<DeviceId>("moza-r9-v2");
    let events = vec![
        HealthEvent {
            timestamp: SystemTime::now(),
            device_id: device_id.clone(),
            event_type: HealthEventType::DeviceConnected,
            context: serde_json::json!({"firmware": "1.2.3", "vendor": "MOZA"}),
        },
        HealthEvent {
            timestamp: SystemTime::now(),
            device_id: device_id.clone(),
            event_type: HealthEventType::DeviceDisconnected,
            context: serde_json::json!({"reason": "usb_reset"}),
        },
    ];
    bundle.add_health_events(&events)?;

    let zip_path = td.path().join("device.zip");
    bundle.generate(&zip_path)?;
    let entries = read_zip_entries(&zip_path)?;

    let parsed: Vec<HealthEvent> =
        serde_json::from_slice(&entries["health_events.json"]).map_err(|e| e.to_string())?;
    assert_eq!(parsed.len(), 2);

    // Verify context data survives round-trip
    let ctx0 = &parsed[0].context;
    assert_eq!(ctx0["firmware"], "1.2.3");
    assert_eq!(ctx0["vendor"], "MOZA");
    Ok(())
}

// =========================================================================
// 7. Config sanitization – passwords, keys, tokens removed
// =========================================================================

#[test]
fn test_profile_files_discoverable() -> Result<(), String> {
    let td = TempDir::new().map_err(|e| e.to_string())?;
    write(td.path().join("good.json"), "{}").map_err(|e| e.to_string())?;
    write(td.path().join("good.profile"), "{}").map_err(|e| e.to_string())?;
    write(td.path().join("bad.txt"), "nope").map_err(|e| e.to_string())?;
    write(td.path().join("bad.toml"), "nope").map_err(|e| e.to_string())?;

    let files = SupportBundle::find_profile_files(td.path())?;
    assert_eq!(files.len(), 2);
    Ok(())
}

#[test]
fn test_env_filter_case_insensitive() -> Result<(), String> {
    // The filter upper-cases, so mixed-case should still be caught
    assert!(!SupportBundle::is_safe_env_var("my_Password_store"));
    assert!(!SupportBundle::is_safe_env_var("SecReT_value"));
    assert!(!SupportBundle::is_safe_env_var("api_token"));
    Ok(())
}

// =========================================================================
// 8. Bundle size limits – large data truncated, bundle within max
// =========================================================================

#[test]
fn test_size_limit_prevents_oversized_health_events() -> Result<(), String> {
    let config = SupportBundleConfig {
        max_bundle_size_mb: 1,
        ..Default::default()
    };
    let mut bundle = SupportBundle::new(config);

    let device_id = must_parse::<DeviceId>("test-device");
    let big_ctx = serde_json::json!({"blob": "y".repeat(2 * 1024 * 1024)});
    let events = vec![HealthEvent {
        timestamp: SystemTime::now(),
        device_id,
        event_type: HealthEventType::DeviceConnected,
        context: big_ctx,
    }];

    let result = bundle.add_health_events(&events);
    assert!(result.is_err(), "should reject oversized events");
    Ok(())
}

#[test]
fn test_bundle_under_max_size() -> Result<(), String> {
    let (zip_path, _td) = build_full_bundle()?;
    let meta = fs::metadata(&zip_path).map_err(|e| e.to_string())?;
    // Default limit is 25 MB
    assert!(
        meta.len() < 25 * 1024 * 1024,
        "bundle {} bytes exceeds 25 MB",
        meta.len()
    );
    Ok(())
}

#[test]
fn test_estimated_size_grows_with_data() -> Result<(), String> {
    let config = SupportBundleConfig::default();
    let mut bundle = SupportBundle::new(config);

    let size_before = bundle.estimated_size_mb();
    bundle.add_system_info()?;
    let size_after = bundle.estimated_size_mb();
    assert!(
        size_after > size_before,
        "estimated size should grow after adding system info"
    );
    Ok(())
}

#[test]
fn test_recordings_stop_adding_at_size_limit() -> Result<(), String> {
    let td = TempDir::new().map_err(|e| e.to_string())?;
    let rec_dir = td.path().join("rec");
    fs::create_dir_all(&rec_dir).map_err(|e| e.to_string())?;

    // Create several fake .wbb files
    for i in 0..5 {
        let content = vec![0u8; 300 * 1024]; // 300 KB each
        write(rec_dir.join(format!("recording_{i}.wbb")), &content).map_err(|e| e.to_string())?;
    }

    let config = SupportBundleConfig {
        max_bundle_size_mb: 1,
        ..Default::default()
    };
    let mut bundle = SupportBundle::new(config);
    bundle.add_recent_recordings(&rec_dir)?;

    // With 1 MB limit, not all 5 × 300 KB recordings should fit
    let zip_path = td.path().join("limited_rec.zip");
    bundle.generate(&zip_path)?;

    let entries = read_zip_entries(&zip_path)?;
    let rec_entries: Vec<_> = entries
        .keys()
        .filter(|k| k.starts_with("recordings/"))
        .collect();
    assert!(
        rec_entries.len() < 5,
        "expected fewer than 5 recordings, got {}",
        rec_entries.len()
    );
    Ok(())
}

// =========================================================================
// 9. Error handling – bundle gen succeeds even if some data unavailable
// =========================================================================

#[test]
fn test_bundle_ok_without_logs() -> Result<(), String> {
    let td = TempDir::new().map_err(|e| e.to_string())?;
    let config = SupportBundleConfig {
        include_logs: false,
        ..Default::default()
    };
    let mut bundle = SupportBundle::new(config);
    bundle.add_health_events(&make_health_events(2))?;
    let zip_path = td.path().join("nologs.zip");
    bundle.generate(&zip_path)?;
    assert!(zip_path.exists());
    Ok(())
}

#[test]
fn test_bundle_ok_without_system_info() -> Result<(), String> {
    let td = TempDir::new().map_err(|e| e.to_string())?;
    let config = SupportBundleConfig {
        include_system_info: false,
        ..Default::default()
    };
    let mut bundle = SupportBundle::new(config);
    bundle.add_health_events(&make_health_events(2))?;
    let zip_path = td.path().join("nosysinfo.zip");
    bundle.generate(&zip_path)?;

    let entries = read_zip_entries(&zip_path)?;
    assert!(!entries.contains_key("system_info.json"));
    Ok(())
}

#[test]
fn test_bundle_ok_without_profiles() -> Result<(), String> {
    let td = TempDir::new().map_err(|e| e.to_string())?;
    let config = SupportBundleConfig {
        include_profiles: false,
        ..Default::default()
    };
    let bundle = SupportBundle::new(config);
    let zip_path = td.path().join("noprofiles.zip");
    bundle.generate(&zip_path)?;
    assert!(zip_path.exists());
    Ok(())
}

#[test]
fn test_bundle_ok_without_recordings() -> Result<(), String> {
    let td = TempDir::new().map_err(|e| e.to_string())?;
    let config = SupportBundleConfig {
        include_recent_recordings: false,
        ..Default::default()
    };
    let bundle = SupportBundle::new(config);
    let zip_path = td.path().join("norec.zip");
    bundle.generate(&zip_path)?;
    assert!(zip_path.exists());
    Ok(())
}

#[test]
fn test_bundle_with_empty_health_events() -> Result<(), String> {
    let td = TempDir::new().map_err(|e| e.to_string())?;
    let config = SupportBundleConfig::default();
    let bundle = SupportBundle::new(config);

    let zip_path = td.path().join("empty_he.zip");
    bundle.generate(&zip_path)?;

    let entries = read_zip_entries(&zip_path)?;
    // No health_events.json when events list is empty
    assert!(!entries.contains_key("health_events.json"));
    Ok(())
}

#[test]
fn test_missing_recording_dir_ok() -> Result<(), String> {
    let td = TempDir::new().map_err(|e| e.to_string())?;
    let config = SupportBundleConfig::default();
    let mut bundle = SupportBundle::new(config);
    let result = bundle.add_recent_recordings(&td.path().join("does_not_exist"));
    assert!(result.is_ok());
    Ok(())
}

#[test]
fn test_missing_profile_dir_ok() -> Result<(), String> {
    let td = TempDir::new().map_err(|e| e.to_string())?;
    let config = SupportBundleConfig::default();
    let mut bundle = SupportBundle::new(config);
    let result = bundle.add_profile_files(&td.path().join("no_profiles"));
    assert!(result.is_ok());
    Ok(())
}

// =========================================================================
// 10. Reproducibility – same state → deterministic bundles
// =========================================================================

#[test]
fn test_bundle_structure_deterministic() -> Result<(), String> {
    // Generate two bundles with identical inputs and verify they contain
    // the same set of ZIP entries (timestamps will differ, so we compare keys).
    let td = TempDir::new().map_err(|e| e.to_string())?;

    let log_dir = td.path().join("logs");
    fs::create_dir_all(&log_dir).map_err(|e| e.to_string())?;
    write(log_dir.join("test.log"), "deterministic").map_err(|e| e.to_string())?;

    let make_bundle = |name: &str| -> Result<HashMap<String, Vec<u8>>, String> {
        let config = SupportBundleConfig {
            include_logs: true,
            include_profiles: false,
            include_system_info: false,
            include_recent_recordings: false,
            max_bundle_size_mb: 10,
        };
        let mut bundle = SupportBundle::new(config);
        bundle.add_health_events(&make_health_events(3))?;
        bundle.add_log_files(&log_dir)?;
        let p = td.path().join(name);
        bundle.generate(&p)?;
        read_zip_entries(&p)
    };

    let entries1 = make_bundle("bundle1.zip")?;
    let entries2 = make_bundle("bundle2.zip")?;

    // Same entry names
    let mut keys1: Vec<_> = entries1.keys().collect();
    let mut keys2: Vec<_> = entries2.keys().collect();
    keys1.sort();
    keys2.sort();
    assert_eq!(keys1, keys2, "bundle entry names should match");

    // Log file content should be byte-identical
    assert_eq!(entries1["logs/test.log"], entries2["logs/test.log"]);
    Ok(())
}

// =========================================================================
// 11. Diagnostic service integration – uptime, recording gating
// =========================================================================

#[test]
fn test_diagnostic_service_uptime() -> Result<(), String> {
    let td = TempDir::new().map_err(|e| e.to_string())?;
    let config = DiagnosticConfig {
        enable_recording: false,
        recording_dir: td.path().to_path_buf(),
        ..DiagnosticConfig::default()
    };
    let service = DiagnosticService::new(config)?;
    let uptime = service.uptime();
    assert!(uptime.as_secs() < 5, "uptime should be near zero");
    Ok(())
}

#[test]
fn test_health_event_pruning() -> Result<(), String> {
    let td = TempDir::new().map_err(|e| e.to_string())?;
    let config = DiagnosticConfig {
        enable_recording: false,
        recording_dir: td.path().to_path_buf(),
        ..DiagnosticConfig::default()
    };
    let mut service = DiagnosticService::new(config)?;
    let device_id = must_parse::<DeviceId>("pruning-device");

    // Push 1100 events (exceeds 1000 cap)
    for i in 0..1100 {
        service.record_health_event(HealthEvent {
            timestamp: SystemTime::now(),
            device_id: device_id.clone(),
            event_type: HealthEventType::PerformanceDegradation {
                metric: "tick".into(),
                value: i as f64,
            },
            context: serde_json::json!({}),
        });
    }

    // After pruning the buffer should have ≤600 events (1100 - 500 drain once)
    let recent = service.get_recent_health_events(10000);
    assert!(
        recent.len() <= 600,
        "expected ≤600 events after pruning, got {}",
        recent.len()
    );
    Ok(())
}

// =========================================================================
// 12. Property-based tests – redaction never leaks sensitive data
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn prop_password_vars_always_rejected(
        prefix in "[A-Z_]{0,10}",
    ) {
        let key = format!("{prefix}PASSWORD");
        // Should be rejected unless it starts with an explicit safe prefix
        // that overrides (like CARGO_ or RUST_)
        let is_cargo_or_rust = key.starts_with("CARGO_") || key.starts_with("RUST_");
        if !is_cargo_or_rust {
            prop_assert!(!SupportBundle::is_safe_env_var(&key),
                "PASSWORD key was allowed: {}", key);
        }
    }

    #[test]
    fn prop_secret_vars_always_rejected(
        prefix in "[A-Z_]{0,10}",
    ) {
        let key = format!("{prefix}SECRET");
        let is_cargo_or_rust = key.starts_with("CARGO_") || key.starts_with("RUST_");
        if !is_cargo_or_rust {
            prop_assert!(!SupportBundle::is_safe_env_var(&key),
                "SECRET key was allowed: {}", key);
        }
    }

    #[test]
    fn prop_token_vars_always_rejected(
        prefix in "[A-Z_]{0,10}",
    ) {
        let key = format!("{prefix}TOKEN");
        let is_cargo_or_rust = key.starts_with("CARGO_") || key.starts_with("RUST_");
        if !is_cargo_or_rust {
            prop_assert!(!SupportBundle::is_safe_env_var(&key),
                "TOKEN key was allowed: {}", key);
        }
    }

    #[test]
    fn prop_credential_vars_always_rejected(
        prefix in "[A-Z_]{0,10}",
    ) {
        let key = format!("{prefix}CREDENTIAL");
        let is_cargo_or_rust = key.starts_with("CARGO_") || key.starts_with("RUST_");
        if !is_cargo_or_rust {
            prop_assert!(!SupportBundle::is_safe_env_var(&key),
                "CREDENTIAL key was allowed: {}", key);
        }
    }

    #[test]
    fn prop_key_vars_always_rejected(
        prefix in "[A-Z_]{1,10}",
    ) {
        // Ensure the prefix itself doesn't start with a safe override
        let key = format!("{prefix}_KEY");
        let upper = key.to_uppercase();
        let is_safe_prefix = upper.starts_with("CARGO_") || upper.starts_with("RUST_")
            || upper.starts_with("PATH") || upper.starts_with("HOME")
            || upper.starts_with("USER") || upper.starts_with("COMPUTERNAME");
        if !is_safe_prefix {
            prop_assert!(!SupportBundle::is_safe_env_var(&key),
                "KEY var was allowed: {}", key);
        }
    }

    #[test]
    fn prop_cargo_prefix_always_safe(
        suffix in "[A-Z_]{1,10}",
    ) {
        let key = format!("CARGO_{suffix}");
        prop_assert!(SupportBundle::is_safe_env_var(&key),
            "CARGO_ key was rejected: {}", key);
    }

    #[test]
    fn prop_rust_prefix_always_safe(
        suffix in "[A-Z_]{1,10}",
    ) {
        let key = format!("RUST_{suffix}");
        prop_assert!(SupportBundle::is_safe_env_var(&key),
            "RUST_ key was rejected: {}", key);
    }

    #[test]
    fn prop_system_info_never_contains_sensitive_env(
        _dummy in 0u8..1u8, // Run once per case to exercise nondeterminism
    ) {
        let info = SupportBundle::collect_system_info()
            .map_err(|e| TestCaseError::fail(format!("collect_system_info: {e}")))?;
        for key in info.environment.keys() {
            let upper = key.to_uppercase();
            prop_assert!(!upper.contains("PASSWORD"), "leaked PASSWORD in key: {}", key);
            prop_assert!(!upper.contains("SECRET"), "leaked SECRET in key: {}", key);
            prop_assert!(!upper.contains("TOKEN"), "leaked TOKEN in key: {}", key);
            prop_assert!(!upper.contains("CREDENTIAL"), "leaked CREDENTIAL in key: {}", key);
        }
    }
}

// =========================================================================
// 13. Additional edge cases
// =========================================================================

#[test]
fn test_default_support_bundle_config() -> Result<(), String> {
    let config = SupportBundleConfig::default();
    assert!(config.include_logs);
    assert!(config.include_profiles);
    assert!(config.include_system_info);
    assert!(config.include_recent_recordings);
    assert_eq!(config.max_bundle_size_mb, 25);
    Ok(())
}

#[test]
fn test_system_info_has_process_info() -> Result<(), String> {
    let info = SupportBundle::collect_system_info()?;
    assert!(info.process_info.pid > 0);
    Ok(())
}

#[test]
fn test_system_info_collected_at_recent() -> Result<(), String> {
    let before = SystemTime::now();
    let info = SupportBundle::collect_system_info()?;
    let after = SystemTime::now();

    // collected_at should be between before and after
    assert!(info.collected_at >= before);
    assert!(info.collected_at <= after);
    Ok(())
}

#[test]
fn test_add_health_events_increments_size() -> Result<(), String> {
    let config = SupportBundleConfig::default();
    let mut bundle = SupportBundle::new(config);

    let before = bundle.estimated_size_mb();
    bundle.add_health_events(&make_health_events(50))?;
    let after = bundle.estimated_size_mb();
    assert!(after > before);
    Ok(())
}

#[test]
fn test_find_log_files_empty_dir() -> Result<(), String> {
    let td = TempDir::new().map_err(|e| e.to_string())?;
    let files = SupportBundle::find_log_files(td.path())?;
    assert!(files.is_empty());
    Ok(())
}

#[test]
fn test_find_recent_recordings_max_count() -> Result<(), String> {
    let td = TempDir::new().map_err(|e| e.to_string())?;
    // Create 10 fake .wbb files
    for i in 0..10 {
        write(td.path().join(format!("rec_{i}.wbb")), "fake").map_err(|e| e.to_string())?;
    }
    let found = SupportBundle::find_recent_recordings(td.path(), 3)?;
    assert_eq!(found.len(), 3, "should truncate to max_count");
    Ok(())
}
