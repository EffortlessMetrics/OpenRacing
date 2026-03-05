//! Plugin orchestration tests.
//!
//! These tests cover:
//! - Plugin registry CRUD (catalog add, remove, search, version management)
//! - Plugin scheduling (manifest validation, constraint enforcement)
//! - Plugin resource budgets (execution time, memory, update rate limits)
//! - Multi-plugin coordination (quarantine, failure tracking, capability checks)

use racing_wheel_plugins::capability::CapabilityChecker;
use racing_wheel_plugins::manifest::{
    Capability, EntryPoints, ManifestValidator, PluginConstraints, PluginManifest, PluginOperation,
};
use racing_wheel_plugins::quarantine::{
    FailureTracker, QuarantineManager, QuarantinePolicy, ViolationType,
};
use racing_wheel_plugins::registry::{
    PluginCatalog, PluginId, PluginMetadata, VersionCompatibility, check_compatibility,
};
use racing_wheel_plugins::{PluginClass, PluginError};

use uuid::Uuid;

// ===================================================================
// Helper: build a valid test manifest
// ===================================================================

fn test_manifest(class: PluginClass) -> PluginManifest {
    PluginManifest {
        id: Uuid::new_v4(),
        name: "Test Plugin".to_string(),
        version: "1.0.0".to_string(),
        description: "A test plugin".to_string(),
        author: "Test Author".to_string(),
        license: "MIT".to_string(),
        homepage: None,
        class,
        capabilities: vec![Capability::ReadTelemetry],
        operations: vec![PluginOperation::TelemetryProcessor],
        constraints: PluginConstraints {
            max_execution_time_us: 100,
            max_memory_bytes: 1024 * 1024,
            update_rate_hz: 60,
            cpu_affinity: None,
        },
        entry_points: EntryPoints {
            wasm_module: Some("plugin.wasm".to_string()),
            native_library: None,
            main_function: "process".to_string(),
            init_function: Some("init".to_string()),
            cleanup_function: Some("cleanup".to_string()),
        },
        config_schema: None,
        signature: None,
    }
}

fn test_metadata(name: &str, version: &str) -> PluginMetadata {
    PluginMetadata::new(
        name,
        semver::Version::parse(version).unwrap_or_else(|_| semver::Version::new(1, 0, 0)),
        "Test Author",
        format!("Description for {name}"),
        "MIT",
    )
}

// ===================================================================
// Plugin registry CRUD
// ===================================================================

#[test]
fn catalog_starts_empty() {
    let catalog = PluginCatalog::new();
    assert_eq!(catalog.plugin_count(), 0);
    assert_eq!(catalog.version_count(), 0);
}

#[test]
fn catalog_add_and_get_plugin() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    let meta = test_metadata("FFB Filter", "1.0.0");
    let id = meta.id.clone();

    catalog.add_plugin(meta)?;
    assert_eq!(catalog.plugin_count(), 1);

    let retrieved = catalog.get_plugin(&id, None);
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap_or_else(|| unreachable!());
    assert_eq!(retrieved.name, "FFB Filter");
    Ok(())
}

#[test]
fn catalog_add_multiple_versions() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    let v1 = test_metadata("FFB Filter", "1.0.0");
    let id = v1.id.clone();
    catalog.add_plugin(v1)?;

    let mut v2 = test_metadata("FFB Filter", "1.2.0");
    v2.id = id.clone();
    catalog.add_plugin(v2)?;

    assert_eq!(catalog.plugin_count(), 1);
    assert_eq!(catalog.version_count(), 2);

    // Latest version should be 1.2.0
    let latest = catalog.get_plugin(&id, None);
    assert!(latest.is_some());
    assert_eq!(
        latest.unwrap_or_else(|| unreachable!()).version,
        semver::Version::new(1, 2, 0)
    );
    Ok(())
}

#[test]
fn catalog_remove_specific_version() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    let v1 = test_metadata("Filter", "1.0.0");
    let id = v1.id.clone();
    catalog.add_plugin(v1)?;

    let mut v2 = test_metadata("Filter", "2.0.0");
    v2.id = id.clone();
    catalog.add_plugin(v2)?;

    let removed = catalog.remove_plugin(&id, Some(&semver::Version::new(1, 0, 0)));
    assert!(removed);
    assert_eq!(catalog.version_count(), 1);

    // The remaining version should be 2.0.0
    let remaining = catalog.get_plugin(&id, None);
    assert!(remaining.is_some());
    assert_eq!(
        remaining.unwrap_or_else(|| unreachable!()).version,
        semver::Version::new(2, 0, 0)
    );
    Ok(())
}

#[test]
fn catalog_remove_all_versions() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    let meta = test_metadata("Filter", "1.0.0");
    let id = meta.id.clone();
    catalog.add_plugin(meta)?;

    let removed = catalog.remove_plugin(&id, None);
    assert!(removed);
    assert_eq!(catalog.plugin_count(), 0);
    assert!(!catalog.contains(&id));
    Ok(())
}

#[test]
fn catalog_remove_nonexistent_returns_false() {
    let mut catalog = PluginCatalog::new();
    let id = PluginId::new();
    assert!(!catalog.remove_plugin(&id, None));
}

#[test]
fn catalog_search_by_name() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    catalog.add_plugin(test_metadata("FFB Low Pass", "1.0.0"))?;
    catalog.add_plugin(test_metadata("LED Mapper", "1.0.0"))?;
    catalog.add_plugin(test_metadata("FFB High Pass", "2.0.0"))?;

    let results = catalog.search("FFB");
    assert_eq!(results.len(), 2);
    Ok(())
}

#[test]
fn catalog_search_case_insensitive() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    catalog.add_plugin(test_metadata("MyPlugin", "1.0.0"))?;

    assert_eq!(catalog.search("myplugin").len(), 1);
    assert_eq!(catalog.search("MYPLUGIN").len(), 1);
    Ok(())
}

#[test]
fn catalog_search_empty_query_returns_all() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    catalog.add_plugin(test_metadata("A Plugin", "1.0.0"))?;
    catalog.add_plugin(test_metadata("B Plugin", "1.0.0"))?;

    // Empty string matches everything (substring match)
    let results = catalog.search("");
    assert_eq!(results.len(), 2);
    Ok(())
}

#[test]
fn catalog_list_all_returns_latest_versions() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    let v1 = test_metadata("Plugin", "1.0.0");
    let id = v1.id.clone();
    catalog.add_plugin(v1)?;

    let mut v2 = test_metadata("Plugin", "2.0.0");
    v2.id = id.clone();
    catalog.add_plugin(v2)?;

    let all = catalog.list_all();
    assert_eq!(all.len(), 1);
    assert_eq!(
        all[0].version,
        semver::Version::new(2, 0, 0),
        "list_all must return latest version"
    );
    Ok(())
}

#[test]
fn catalog_contains_version_check() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    let meta = test_metadata("Plugin", "1.5.0");
    let id = meta.id.clone();
    catalog.add_plugin(meta)?;

    assert!(catalog.contains_version(&id, &semver::Version::new(1, 5, 0)));
    assert!(!catalog.contains_version(&id, &semver::Version::new(1, 0, 0)));
    Ok(())
}

#[test]
fn catalog_metadata_validation_rejects_empty_name() {
    let mut catalog = PluginCatalog::new();
    let meta = PluginMetadata::new("", semver::Version::new(1, 0, 0), "Author", "Desc", "MIT");
    assert!(catalog.add_plugin(meta).is_err());
}

#[test]
fn catalog_metadata_validation_rejects_empty_author() {
    let mut catalog = PluginCatalog::new();
    let meta = PluginMetadata::new("Name", semver::Version::new(1, 0, 0), "", "Desc", "MIT");
    assert!(catalog.add_plugin(meta).is_err());
}

// ===================================================================
// Version compatibility (semver)
// ===================================================================

#[test]
fn same_version_is_compatible() {
    let v = semver::Version::new(1, 0, 0);
    assert_eq!(
        check_compatibility(&v, &v),
        VersionCompatibility::Compatible
    );
}

#[test]
fn higher_minor_is_compatible() {
    let required = semver::Version::new(1, 0, 0);
    let available = semver::Version::new(1, 2, 0);
    assert_eq!(
        check_compatibility(&required, &available),
        VersionCompatibility::Compatible
    );
}

#[test]
fn higher_patch_is_compatible() {
    let required = semver::Version::new(1, 0, 0);
    let available = semver::Version::new(1, 0, 5);
    assert_eq!(
        check_compatibility(&required, &available),
        VersionCompatibility::Compatible
    );
}

#[test]
fn lower_minor_is_incompatible() {
    let required = semver::Version::new(1, 5, 0);
    let available = semver::Version::new(1, 2, 0);
    assert_eq!(
        check_compatibility(&required, &available),
        VersionCompatibility::Incompatible
    );
}

#[test]
fn different_major_is_incompatible() {
    let required = semver::Version::new(1, 0, 0);
    let available = semver::Version::new(2, 0, 0);
    assert_eq!(
        check_compatibility(&required, &available),
        VersionCompatibility::Incompatible
    );
}

#[test]
fn pre_release_requires_exact_match() {
    let v1 =
        semver::Version::parse("1.0.0-alpha").unwrap_or_else(|_| semver::Version::new(1, 0, 0));
    let v2 = semver::Version::parse("1.0.0-beta").unwrap_or_else(|_| semver::Version::new(1, 0, 0));

    assert_eq!(
        check_compatibility(&v1, &v1),
        VersionCompatibility::Compatible
    );
    assert_eq!(
        check_compatibility(&v1, &v2),
        VersionCompatibility::Incompatible
    );
}

#[test]
fn zero_major_requires_exact_minor() {
    let required = semver::Version::new(0, 1, 0);
    let available = semver::Version::new(0, 2, 0);
    assert_eq!(
        check_compatibility(&required, &available),
        VersionCompatibility::Incompatible
    );
}

#[test]
fn zero_major_same_minor_higher_patch_is_compatible() {
    let required = semver::Version::new(0, 1, 0);
    let available = semver::Version::new(0, 1, 5);
    assert_eq!(
        check_compatibility(&required, &available),
        VersionCompatibility::Compatible
    );
}

#[test]
fn find_compatible_version_returns_highest() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();

    let v1_0 = test_metadata("Plugin", "1.0.0");
    let id = v1_0.id.clone();
    catalog.add_plugin(v1_0)?;

    let mut v1_5 = test_metadata("Plugin", "1.5.0");
    v1_5.id = id.clone();
    catalog.add_plugin(v1_5)?;

    let mut v2_0 = test_metadata("Plugin", "2.0.0");
    v2_0.id = id.clone();
    catalog.add_plugin(v2_0)?;

    let required = semver::Version::new(1, 0, 0);
    let compatible = catalog.find_compatible_version(&id, &required);
    assert!(compatible.is_some());
    assert_eq!(
        compatible.unwrap_or_else(|| unreachable!()).version,
        semver::Version::new(1, 5, 0)
    );
    Ok(())
}

// ===================================================================
// Plugin scheduling (manifest validation)
// ===================================================================

#[test]
fn safe_plugin_within_limits_passes() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let manifest = test_manifest(PluginClass::Safe);
    validator.validate(&manifest)
}

#[test]
fn fast_plugin_within_limits_passes() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut manifest = test_manifest(PluginClass::Fast);
    manifest.constraints.max_execution_time_us = 100;
    manifest.constraints.max_memory_bytes = 2 * 1024 * 1024;
    manifest.constraints.update_rate_hz = 1000;
    validator.validate(&manifest)
}

#[test]
fn safe_plugin_cannot_use_process_dsp() {
    let validator = ManifestValidator::default();
    let mut manifest = test_manifest(PluginClass::Safe);
    manifest.capabilities = vec![Capability::ProcessDsp];
    assert!(validator.validate(&manifest).is_err());
}

#[test]
fn fast_plugin_can_use_process_dsp() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut manifest = test_manifest(PluginClass::Fast);
    manifest.capabilities = vec![Capability::ProcessDsp];
    manifest.constraints.max_execution_time_us = 100;
    manifest.constraints.max_memory_bytes = 2 * 1024 * 1024;
    manifest.constraints.update_rate_hz = 1000;
    validator.validate(&manifest)
}

#[test]
fn empty_name_rejected() {
    let validator = ManifestValidator::default();
    let mut manifest = test_manifest(PluginClass::Safe);
    manifest.name = String::new();
    assert!(validator.validate(&manifest).is_err());
}

#[test]
fn empty_author_rejected() {
    let validator = ManifestValidator::default();
    let mut manifest = test_manifest(PluginClass::Safe);
    manifest.author = String::new();
    assert!(validator.validate(&manifest).is_err());
}

// ===================================================================
// Plugin resource budgets
// ===================================================================

#[test]
fn safe_execution_time_at_limit_passes() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut manifest = test_manifest(PluginClass::Safe);
    manifest.constraints.max_execution_time_us = 5000; // exact limit
    validator.validate(&manifest)
}

#[test]
fn safe_execution_time_over_limit_fails() {
    let validator = ManifestValidator::default();
    let mut manifest = test_manifest(PluginClass::Safe);
    manifest.constraints.max_execution_time_us = 5001;
    assert!(validator.validate(&manifest).is_err());
}

#[test]
fn safe_memory_at_limit_passes() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut manifest = test_manifest(PluginClass::Safe);
    manifest.constraints.max_memory_bytes = 16 * 1024 * 1024;
    validator.validate(&manifest)
}

#[test]
fn safe_memory_over_limit_fails() {
    let validator = ManifestValidator::default();
    let mut manifest = test_manifest(PluginClass::Safe);
    manifest.constraints.max_memory_bytes = 16 * 1024 * 1024 + 1;
    assert!(validator.validate(&manifest).is_err());
}

#[test]
fn safe_update_rate_at_limit_passes() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut manifest = test_manifest(PluginClass::Safe);
    manifest.constraints.update_rate_hz = 200;
    validator.validate(&manifest)
}

#[test]
fn safe_update_rate_over_limit_fails() {
    let validator = ManifestValidator::default();
    let mut manifest = test_manifest(PluginClass::Safe);
    manifest.constraints.update_rate_hz = 201;
    assert!(validator.validate(&manifest).is_err());
}

#[test]
fn fast_execution_time_at_limit_passes() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut manifest = test_manifest(PluginClass::Fast);
    manifest.constraints.max_execution_time_us = 200;
    manifest.constraints.max_memory_bytes = 1024 * 1024;
    manifest.constraints.update_rate_hz = 1000;
    validator.validate(&manifest)
}

#[test]
fn fast_execution_time_over_limit_fails() {
    let validator = ManifestValidator::default();
    let mut manifest = test_manifest(PluginClass::Fast);
    manifest.constraints.max_execution_time_us = 201;
    manifest.constraints.max_memory_bytes = 1024 * 1024;
    manifest.constraints.update_rate_hz = 1000;
    assert!(validator.validate(&manifest).is_err());
}

#[test]
fn fast_memory_over_limit_fails() {
    let validator = ManifestValidator::default();
    let mut manifest = test_manifest(PluginClass::Fast);
    manifest.constraints.max_execution_time_us = 100;
    manifest.constraints.max_memory_bytes = 5 * 1024 * 1024; // 4MB limit
    manifest.constraints.update_rate_hz = 1000;
    assert!(validator.validate(&manifest).is_err());
}

// ===================================================================
// Multi-plugin coordination (quarantine)
// ===================================================================

#[test]
fn new_plugin_is_not_quarantined() {
    let mut manager = QuarantineManager::new(QuarantinePolicy::default());
    let id = Uuid::new_v4();
    assert!(!manager.is_quarantined(id));
}

#[test]
fn single_crash_does_not_quarantine() -> Result<(), PluginError> {
    let mut manager = QuarantineManager::new(QuarantinePolicy::default());
    let id = Uuid::new_v4();
    manager.record_violation(id, ViolationType::Crash, "crash".to_string())?;
    assert!(!manager.is_quarantined(id));
    Ok(())
}

#[test]
fn three_crashes_trigger_quarantine() -> Result<(), PluginError> {
    let policy = QuarantinePolicy {
        max_crashes: 3,
        ..QuarantinePolicy::default()
    };
    let mut manager = QuarantineManager::new(policy);
    let id = Uuid::new_v4();

    for i in 0..3 {
        manager.record_violation(id, ViolationType::Crash, format!("crash {i}"))?;
    }
    assert!(manager.is_quarantined(id));
    Ok(())
}

#[test]
fn budget_violations_trigger_quarantine() -> Result<(), PluginError> {
    let policy = QuarantinePolicy {
        max_budget_violations: 3,
        ..QuarantinePolicy::default()
    };
    let mut manager = QuarantineManager::new(policy);
    let id = Uuid::new_v4();

    for i in 0..3 {
        manager.record_violation(id, ViolationType::BudgetViolation, format!("budget {i}"))?;
    }
    assert!(manager.is_quarantined(id));
    Ok(())
}

#[test]
fn manual_quarantine_and_release() -> Result<(), PluginError> {
    let mut manager = QuarantineManager::new(QuarantinePolicy::default());
    let id = Uuid::new_v4();

    manager.manual_quarantine(id, 60)?;
    assert!(manager.is_quarantined(id));

    manager.release_from_quarantine(id)?;
    assert!(!manager.is_quarantined(id));
    Ok(())
}

#[test]
fn release_unknown_plugin_fails() {
    let mut manager = QuarantineManager::new(QuarantinePolicy::default());
    let id = Uuid::new_v4();
    assert!(manager.release_from_quarantine(id).is_err());
}

#[test]
fn quarantine_tracks_crash_and_budget_totals() -> Result<(), PluginError> {
    let mut manager = QuarantineManager::new(QuarantinePolicy::default());
    let id = Uuid::new_v4();

    manager.record_violation(id, ViolationType::Crash, "c".to_string())?;
    manager.record_violation(id, ViolationType::BudgetViolation, "b".to_string())?;

    let state = manager.get_quarantine_state(id);
    assert!(state.is_some());
    let state = state.unwrap_or_else(|| unreachable!());
    assert_eq!(state.total_crashes, 1);
    assert_eq!(state.total_budget_violations, 1);
    Ok(())
}

#[test]
fn capability_violation_does_not_increment_crash_counter() -> Result<(), PluginError> {
    let mut manager = QuarantineManager::new(QuarantinePolicy::default());
    let id = Uuid::new_v4();

    manager.record_violation(id, ViolationType::CapabilityViolation, "cap".to_string())?;

    let state = manager
        .get_quarantine_state(id)
        .unwrap_or_else(|| unreachable!());
    assert_eq!(state.total_crashes, 0);
    assert_eq!(state.total_budget_violations, 0);
    Ok(())
}

#[test]
fn quarantine_stats_empty_by_default() {
    let manager = QuarantineManager::new(QuarantinePolicy::default());
    assert!(manager.get_quarantine_stats().is_empty());
}

#[test]
fn quarantine_policy_serialization_roundtrip() -> Result<(), serde_json::Error> {
    let policy = QuarantinePolicy::default();
    let json = serde_json::to_string(&policy)?;
    let restored: QuarantinePolicy = serde_json::from_str(&json)?;
    assert_eq!(restored.max_crashes, policy.max_crashes);
    assert_eq!(restored.max_budget_violations, policy.max_budget_violations);
    Ok(())
}

// ===================================================================
// Failure tracking
// ===================================================================

#[test]
fn failure_tracker_starts_empty() {
    let tracker = FailureTracker::new();
    assert!(tracker.get_stats(Uuid::new_v4()).is_none());
}

#[test]
fn failure_tracker_records_executions() {
    let mut tracker = FailureTracker::new();
    let id = Uuid::new_v4();

    tracker.record_execution(id, 100, true);
    tracker.record_execution(id, 200, true);
    tracker.record_execution(id, 300, false);

    let stats = tracker.get_stats(id).unwrap_or_else(|| unreachable!());
    assert_eq!(stats.executions, 3);
    assert_eq!(stats.total_time_us, 600);
    assert_eq!(stats.max_time_us, 300);
    assert_eq!(stats.crashes, 1);
    assert!((stats.avg_time_us - 200.0).abs() < f64::EPSILON);
}

#[test]
fn failure_tracker_max_tracks_peak() {
    let mut tracker = FailureTracker::new();
    let id = Uuid::new_v4();

    tracker.record_execution(id, 500, true);
    tracker.record_execution(id, 100, true);
    tracker.record_execution(id, 300, true);

    assert_eq!(
        tracker
            .get_stats(id)
            .unwrap_or_else(|| unreachable!())
            .max_time_us,
        500
    );
}

#[test]
fn failure_tracker_independent_per_plugin() {
    let mut tracker = FailureTracker::new();
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();

    tracker.record_execution(id1, 100, true);
    tracker.record_execution(id2, 200, false);

    let s1 = tracker.get_stats(id1).unwrap_or_else(|| unreachable!());
    let s2 = tracker.get_stats(id2).unwrap_or_else(|| unreachable!());

    assert_eq!(s1.executions, 1);
    assert_eq!(s1.crashes, 0);
    assert_eq!(s2.executions, 1);
    assert_eq!(s2.crashes, 1);
}

// ===================================================================
// Capability enforcement
// ===================================================================

#[test]
fn capability_checker_grants_declared_caps() {
    let checker = CapabilityChecker::new(vec![Capability::ReadTelemetry, Capability::ControlLeds]);
    assert!(checker.check_telemetry_read().is_ok());
    assert!(checker.check_led_control().is_ok());
}

#[test]
fn capability_checker_denies_undeclared_caps() {
    let checker = CapabilityChecker::new(vec![Capability::ReadTelemetry]);
    assert!(checker.check_telemetry_modify().is_err());
    assert!(checker.check_led_control().is_err());
    assert!(checker.check_dsp_processing().is_err());
    assert!(checker.check_inter_plugin_comm().is_err());
}

#[test]
fn capability_checker_file_access_allowed_path() {
    let checker = CapabilityChecker::new(vec![Capability::FileSystem {
        paths: vec!["/tmp/plugins".to_string()],
    }]);
    assert!(
        checker
            .check_file_access(std::path::Path::new("/tmp/plugins/data.bin"))
            .is_ok()
    );
}

#[test]
fn capability_checker_file_access_disallowed_path() {
    let checker = CapabilityChecker::new(vec![Capability::FileSystem {
        paths: vec!["/tmp/plugins".to_string()],
    }]);
    assert!(
        checker
            .check_file_access(std::path::Path::new("/etc/passwd"))
            .is_err()
    );
}

#[test]
fn capability_checker_network_access_allowed_host() {
    let checker = CapabilityChecker::new(vec![Capability::Network {
        hosts: vec!["api.openracing.io".to_string()],
    }]);
    assert!(checker.check_network_access("api.openracing.io").is_ok());
}

#[test]
fn capability_checker_network_access_denied_host() {
    let checker = CapabilityChecker::new(vec![Capability::Network {
        hosts: vec!["api.openracing.io".to_string()],
    }]);
    assert!(checker.check_network_access("malicious.com").is_err());
}

#[test]
fn empty_capabilities_denies_everything() {
    let checker = CapabilityChecker::new(vec![]);
    assert!(checker.check_telemetry_read().is_err());
    assert!(checker.check_telemetry_modify().is_err());
    assert!(checker.check_led_control().is_err());
    assert!(checker.check_dsp_processing().is_err());
    assert!(checker.check_inter_plugin_comm().is_err());
}

// ===================================================================
// Manifest serialization roundtrips
// ===================================================================

#[test]
fn manifest_json_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = test_manifest(PluginClass::Safe);
    let json = serde_json::to_string(&manifest)?;
    let restored: PluginManifest = serde_json::from_str(&json)?;
    assert_eq!(restored.name, manifest.name);
    assert_eq!(restored.version, manifest.version);
    assert_eq!(restored.class, manifest.class);
    assert_eq!(restored.capabilities, manifest.capabilities);
    Ok(())
}

#[test]
fn manifest_yaml_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = test_manifest(PluginClass::Fast);
    let yaml = serde_yaml::to_string(&manifest)?;
    let restored: PluginManifest = serde_yaml::from_str(&yaml)?;
    assert_eq!(restored.name, manifest.name);
    assert_eq!(restored.author, manifest.author);
    Ok(())
}

#[test]
fn plugin_metadata_json_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let meta = test_metadata("Round Trip", "3.1.4")
        .with_homepage("https://example.com")
        .with_capabilities(vec![Capability::ReadTelemetry, Capability::ControlLeds])
        .with_signature_fingerprint("abc123");

    let json = serde_json::to_string(&meta)?;
    let restored: PluginMetadata = serde_json::from_str(&json)?;
    assert_eq!(restored.name, meta.name);
    assert_eq!(restored.version, meta.version);
    assert_eq!(restored.homepage, meta.homepage);
    assert_eq!(restored.capabilities, meta.capabilities);
    assert_eq!(restored.signature_fingerprint, meta.signature_fingerprint);
    Ok(())
}

// ===================================================================
// Plugin class specific constraints
// ===================================================================

#[test]
fn safe_plugin_multiple_capabilities() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut manifest = test_manifest(PluginClass::Safe);
    manifest.capabilities = vec![
        Capability::ReadTelemetry,
        Capability::ModifyTelemetry,
        Capability::ControlLeds,
        Capability::InterPluginComm,
    ];
    validator.validate(&manifest)
}

#[test]
fn safe_plugin_network_capability_rejected() {
    let validator = ManifestValidator::default();
    let mut manifest = test_manifest(PluginClass::Safe);
    manifest.capabilities = vec![Capability::Network {
        hosts: vec!["example.com".to_string()],
    }];
    assert!(validator.validate(&manifest).is_err());
}

#[test]
fn safe_plugin_filesystem_capability_rejected() {
    let validator = ManifestValidator::default();
    let mut manifest = test_manifest(PluginClass::Safe);
    manifest.capabilities = vec![Capability::FileSystem {
        paths: vec!["/tmp".to_string()],
    }];
    assert!(validator.validate(&manifest).is_err());
}

#[test]
fn plugin_operation_variants_serialize() -> Result<(), Box<dyn std::error::Error>> {
    let ops = vec![
        PluginOperation::TelemetryProcessor,
        PluginOperation::LedMapper,
        PluginOperation::DspFilter,
        PluginOperation::TelemetrySource,
    ];
    let json = serde_json::to_string(&ops)?;
    let restored: Vec<PluginOperation> = serde_json::from_str(&json)?;
    assert_eq!(ops, restored);
    Ok(())
}

#[test]
fn version_compatibility_display() {
    assert_eq!(
        format!("{}", VersionCompatibility::Compatible),
        "compatible"
    );
    assert_eq!(
        format!("{}", VersionCompatibility::Incompatible),
        "incompatible"
    );
    assert_eq!(format!("{}", VersionCompatibility::Unknown), "unknown");
}
