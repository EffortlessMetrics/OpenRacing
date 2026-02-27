//! Comprehensive tests for the plugin system

// Test helper functions to replace unwrap
#[track_caller]
fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
    match r {
        Ok(v) => v,
        Err(e) => panic!("unexpected Err: {e:?}"),
    }
}

#[track_caller]
fn must_some<T>(o: Option<T>, msg: &str) -> T {
    match o {
        Some(v) => v,
        None => panic!("{msg}"),
    }
}

use tempfile::tempdir;
use tokio::fs;
use uuid::Uuid;

use racing_wheel_plugins::*;

/// Test plugin manifest validation
#[tokio::test]
async fn test_manifest_validation() {
    let manifest = manifest::PluginManifest {
        id: Uuid::new_v4(),
        name: "Test Plugin".to_string(),
        version: "1.0.0".to_string(),
        description: "A test plugin".to_string(),
        author: "Test Author".to_string(),
        license: "MIT".to_string(),
        homepage: None,
        class: PluginClass::Safe,
        capabilities: vec![manifest::Capability::ReadTelemetry],
        operations: vec![manifest::PluginOperation::TelemetryProcessor],
        constraints: manifest::PluginConstraints {
            max_execution_time_us: 1000,
            max_memory_bytes: 1024 * 1024,
            update_rate_hz: 60,
            cpu_affinity: None,
        },
        entry_points: manifest::EntryPoints {
            wasm_module: Some("plugin.wasm".to_string()),
            native_library: None,
            main_function: "process".to_string(),
            init_function: Some("init".to_string()),
            cleanup_function: Some("cleanup".to_string()),
        },
        config_schema: None,
        signature: None,
    };

    let validator = manifest::ManifestValidator::default();
    assert!(validator.validate(&manifest).is_ok());
}

/// Test invalid capability for plugin class
#[tokio::test]
async fn test_invalid_capability() {
    let manifest = manifest::PluginManifest {
        id: Uuid::new_v4(),
        name: "Test Plugin".to_string(),
        version: "1.0.0".to_string(),
        description: "A test plugin".to_string(),
        author: "Test Author".to_string(),
        license: "MIT".to_string(),
        homepage: None,
        class: PluginClass::Safe,
        capabilities: vec![manifest::Capability::ProcessDsp], // Not allowed for Safe plugins
        operations: vec![manifest::PluginOperation::TelemetryProcessor],
        constraints: manifest::PluginConstraints {
            max_execution_time_us: 1000,
            max_memory_bytes: 1024 * 1024,
            update_rate_hz: 60,
            cpu_affinity: None,
        },
        entry_points: manifest::EntryPoints {
            wasm_module: Some("plugin.wasm".to_string()),
            native_library: None,
            main_function: "process".to_string(),
            init_function: None,
            cleanup_function: None,
        },
        config_schema: None,
        signature: None,
    };

    let validator = manifest::ManifestValidator::default();
    assert!(validator.validate(&manifest).is_err());
}

/// Test capability checker
#[test]
fn test_capability_checker() {
    let capabilities = vec![
        manifest::Capability::ReadTelemetry,
        manifest::Capability::FileSystem {
            paths: vec!["/tmp".to_string()],
        },
    ];

    let checker = capability::CapabilityChecker::new(capabilities);

    assert!(checker.check_telemetry_read().is_ok());
    assert!(checker.check_telemetry_modify().is_err());
    assert!(
        checker
            .check_file_access(std::path::Path::new("/tmp/test.txt"))
            .is_ok()
    );
    assert!(
        checker
            .check_file_access(std::path::Path::new("/etc/passwd"))
            .is_err()
    );
}

/// Test quarantine system
#[tokio::test]
async fn test_quarantine_system() {
    let mut manager = quarantine::QuarantineManager::new(quarantine::QuarantinePolicy {
        max_crashes: 2,
        ..Default::default()
    });

    let plugin_id = Uuid::new_v4();

    // First crash - should not quarantine
    must(manager.record_violation(
        plugin_id,
        quarantine::ViolationType::Crash,
        "Test crash 1".to_string(),
    ));
    assert!(!manager.is_quarantined(plugin_id));

    // Second crash - should quarantine
    must(manager.record_violation(
        plugin_id,
        quarantine::ViolationType::Crash,
        "Test crash 2".to_string(),
    ));
    assert!(manager.is_quarantined(plugin_id));
}

/// Test WASM plugin host
#[tokio::test]
async fn test_wasm_plugin_host() {
    let host = wasm::WasmPluginHost::new().expect("Failed to create WASM host");

    // Test with a mock manifest (no actual WASM file)
    let manifest = manifest::PluginManifest {
        id: Uuid::new_v4(),
        name: "Mock WASM Plugin".to_string(),
        version: "1.0.0".to_string(),
        description: "A mock WASM plugin for testing".to_string(),
        author: "Test".to_string(),
        license: "MIT".to_string(),
        homepage: None,
        class: PluginClass::Safe,
        capabilities: vec![manifest::Capability::ReadTelemetry],
        operations: vec![manifest::PluginOperation::TelemetryProcessor],
        constraints: manifest::PluginConstraints {
            max_execution_time_us: 5000,
            max_memory_bytes: 16 * 1024 * 1024,
            update_rate_hz: 60,
            cpu_affinity: None,
        },
        entry_points: manifest::EntryPoints {
            wasm_module: Some("mock.wasm".to_string()),
            native_library: None,
            main_function: "process".to_string(),
            init_function: None,
            cleanup_function: None,
        },
        config_schema: None,
        signature: None,
    };

    // This will fail because we don't have an actual WASM file, but tests the interface
    let result = host
        .load_plugin(manifest, std::path::Path::new("nonexistent.wasm"))
        .await;
    assert!(result.is_err());
}

/// Test plugin host system
#[tokio::test]
async fn test_plugin_host_system() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let plugin_dir = temp_dir.path().to_path_buf();

    // Create a mock plugin directory structure
    let plugin_subdir = plugin_dir.join("test-plugin");
    must(fs::create_dir_all(&plugin_subdir).await);

    // Create a mock manifest
    let manifest = manifest::PluginManifest {
        id: Uuid::new_v4(),
        name: "Test Plugin".to_string(),
        version: "1.0.0".to_string(),
        description: "A test plugin".to_string(),
        author: "Test Author".to_string(),
        license: "MIT".to_string(),
        homepage: None,
        class: PluginClass::Safe,
        capabilities: vec![manifest::Capability::ReadTelemetry],
        operations: vec![manifest::PluginOperation::TelemetryProcessor],
        constraints: manifest::PluginConstraints {
            max_execution_time_us: 1000,
            max_memory_bytes: 1024 * 1024,
            update_rate_hz: 60,
            cpu_affinity: None,
        },
        entry_points: manifest::EntryPoints {
            wasm_module: Some("plugin.wasm".to_string()),
            native_library: None,
            main_function: "process".to_string(),
            init_function: None,
            cleanup_function: None,
        },
        config_schema: None,
        signature: None,
    };

    // Write manifest file
    let manifest_content = must(serde_yaml::to_string(&manifest));
    must(fs::write(plugin_subdir.join("plugin.yaml"), manifest_content).await);

    // Create empty WASM file
    must(fs::write(plugin_subdir.join("plugin.wasm"), b"mock wasm content").await);

    // Create plugin host
    let host = host::PluginHost::new(plugin_dir)
        .await
        .expect("Failed to create plugin host");

    // Check that plugin was discovered
    let registry = host.get_registry().await;
    assert_eq!(registry.len(), 1);

    let (_plugin_id, entry) = must_some(registry.iter().next(), "expected plugin in registry");
    assert_eq!(entry.manifest.name, "Test Plugin");
    assert!(!entry.is_loaded);
    assert!(entry.is_enabled);
}

/// Test budget violation detection
#[tokio::test]
async fn test_budget_violation_detection() {
    let mut manager = quarantine::QuarantineManager::new(quarantine::QuarantinePolicy {
        max_budget_violations: 3,
        ..Default::default()
    });

    let plugin_id = Uuid::new_v4();

    // Record budget violations
    for i in 0..2 {
        let _ = manager.record_violation(
            plugin_id,
            quarantine::ViolationType::BudgetViolation,
            format!("Budget violation {}", i + 1),
        );
        assert!(!manager.is_quarantined(plugin_id));
    }

    // Third violation should trigger quarantine
    let _ = manager.record_violation(
        plugin_id,
        quarantine::ViolationType::BudgetViolation,
        "Budget violation 3".to_string(),
    );
    assert!(manager.is_quarantined(plugin_id));
}

/// Test plugin statistics tracking
#[test]
fn test_plugin_statistics() {
    let mut tracker = quarantine::FailureTracker::new();
    let plugin_id = Uuid::new_v4();

    // Record successful executions
    tracker.record_execution(plugin_id, 100, true);
    tracker.record_execution(plugin_id, 200, true);
    tracker.record_execution(plugin_id, 150, false); // One failure

    let stats = must_some(tracker.get_stats(plugin_id), "expected stats for plugin");
    assert_eq!(stats.executions, 3);
    assert_eq!(stats.crashes, 1);
    assert_eq!(stats.max_time_us, 200);
    assert!((stats.avg_time_us - 150.0).abs() < 0.1);
}

/// Test quarantine escalation
#[tokio::test]
async fn test_quarantine_escalation() {
    let mut manager = quarantine::QuarantineManager::new(quarantine::QuarantinePolicy {
        max_crashes: 1,
        quarantine_duration_minutes: 10,
        ..Default::default()
    });

    let plugin_id = Uuid::new_v4();

    // First quarantine
    let _ = manager.record_violation(
        plugin_id,
        quarantine::ViolationType::Crash,
        "First crash".to_string(),
    );

    let state = must_some(
        manager.get_quarantine_state(plugin_id),
        "expected quarantine state",
    );
    assert_eq!(state.escalation_level, 1);

    // Release and trigger again
    must(manager.release_from_quarantine(plugin_id));
    let _ = manager.record_violation(
        plugin_id,
        quarantine::ViolationType::Crash,
        "Second crash".to_string(),
    );

    let state = must_some(
        manager.get_quarantine_state(plugin_id),
        "expected quarantine state",
    );
    assert_eq!(state.escalation_level, 2);
}

/// Test plugin context validation
#[test]
fn test_plugin_context() {
    let context = PluginContext {
        plugin_id: Uuid::new_v4(),
        class: PluginClass::Safe,
        update_rate_hz: 60,
        budget_us: 5000,
        capabilities: vec!["ReadTelemetry".to_string()],
    };

    assert_eq!(context.class, PluginClass::Safe);
    assert_eq!(context.update_rate_hz, 60);
    assert_eq!(context.budget_us, 5000);
}

// ============================================================
// Additional Plugin System Tests
// ============================================================

/// WAT for a minimal valid plugin (passthrough process).
const PASSTHROUGH_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
    )
)
"#;

/// WAT for a plugin whose process function hits an unreachable trap.
const TRAP_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        unreachable
    )
)
"#;

/// WASM sandbox isolation: a plugin granted only ReadTelemetry cannot use DSP or LED.
#[test]
fn test_wasm_sandbox_capability_isolation_read_only() {
    use racing_wheel_plugins::capability::CapabilityChecker;
    use racing_wheel_plugins::manifest::Capability;

    let checker = CapabilityChecker::new(vec![Capability::ReadTelemetry]);

    assert!(checker.check_telemetry_read().is_ok());
    assert!(checker.check_telemetry_modify().is_err());
    assert!(checker.check_dsp_processing().is_err());
    assert!(checker.check_led_control().is_err());
}

/// WASM sandbox: filesystem capability limits access to declared paths only.
#[test]
fn test_wasm_sandbox_filesystem_path_enforcement() {
    use racing_wheel_plugins::capability::CapabilityChecker;
    use racing_wheel_plugins::manifest::Capability;

    let checker = CapabilityChecker::new(vec![Capability::FileSystem {
        paths: vec!["/tmp/plugins".to_string()],
    }]);

    assert!(
        checker
            .check_file_access(std::path::Path::new("/tmp/plugins/data.bin"))
            .is_ok()
    );
    assert!(
        checker
            .check_file_access(std::path::Path::new("/etc/shadow"))
            .is_err()
    );
    assert!(
        checker
            .check_file_access(std::path::Path::new("/tmp/other/file.txt"))
            .is_err()
    );
}

/// WASM sandbox: network capability is denied when not granted.
#[test]
fn test_wasm_sandbox_network_capability_enforcement() {
    use racing_wheel_plugins::capability::CapabilityChecker;
    use racing_wheel_plugins::manifest::Capability;

    // No Network capability granted
    let checker = CapabilityChecker::new(vec![Capability::ReadTelemetry]);
    assert!(checker.check_network_access("telemetry.example.com").is_err());

    // With specific host granted
    let checker_with_net = CapabilityChecker::new(vec![Capability::Network {
        hosts: vec!["telemetry.example.com".to_string()],
    }]);
    assert!(
        checker_with_net
            .check_network_access("telemetry.example.com")
            .is_ok()
    );
    assert!(
        checker_with_net
            .check_network_access("malicious.example.com")
            .is_err()
    );
}

/// Native plugin signature verification: strict config rejects unsigned plugins.
#[test]
fn test_native_plugin_signature_config_strict_rejects_unsigned() {
    use racing_wheel_plugins::native::NativePluginConfig;

    let strict = NativePluginConfig::strict();
    assert!(
        !strict.allow_unsigned,
        "Strict mode must not allow unsigned plugins"
    );
    assert!(
        strict.require_signatures,
        "Strict mode must require signatures"
    );

    let dev = NativePluginConfig::development();
    assert!(
        dev.allow_unsigned,
        "Development mode should allow unsigned plugins"
    );
    assert!(
        !dev.require_signatures,
        "Development mode should not require signatures"
    );
}

/// Native plugin load rejects an unsigned library file under strict (default) config.
#[tokio::test]
async fn test_native_plugin_load_rejects_unsigned_library() {
    use racing_wheel_plugins::native::NativePluginHost;

    let temp_dir = must(tempdir());
    let plugin_subdir = temp_dir.path().join("unsigned-native-plugin");
    must(fs::create_dir_all(&plugin_subdir).await);

    let lib_name = if cfg!(windows) {
        "plugin.dll"
    } else if cfg!(target_os = "macos") {
        "plugin.dylib"
    } else {
        "plugin.so"
    };

    // Write a fake (non-signed) library file — no companion .sig file
    must(fs::write(plugin_subdir.join(lib_name), b"not a real library").await);

    // new_with_defaults uses strict config (allow_unsigned = false)
    let host = NativePluginHost::new_with_defaults();

    let fake_path = plugin_subdir.join(lib_name);
    let result = host
        .load_plugin(uuid::Uuid::new_v4(), "unsigned-test".to_string(), &fake_path, 1000)
        .await;

    assert!(
        result.is_err(),
        "Should reject an unsigned native plugin under strict config"
    );
}

/// Crash isolation: a trapping WASM plugin does not affect other loaded plugins.
#[test]
fn test_plugin_crash_isolation_does_not_affect_sibling() -> Result<(), Box<dyn std::error::Error>>
{
    use racing_wheel_plugins::wasm::{PluginId, WasmRuntime};

    let crash_bytes = wat::parse_str(TRAP_WAT)?;
    let good_bytes = wat::parse_str(PASSTHROUGH_WAT)?;

    let mut runtime = WasmRuntime::new()?;

    let crash_id = PluginId::new_v4();
    let good_id = PluginId::new_v4();

    runtime.load_plugin_from_bytes(crash_id, &crash_bytes, vec![])?;
    runtime.load_plugin_from_bytes(good_id, &good_bytes, vec![])?;

    // Execute the crashing plugin — expect an error (trap)
    let crash_result = runtime.process(&crash_id, 0.5, 0.001);
    assert!(
        crash_result.is_err(),
        "Trapping plugin must return an error"
    );

    // The sibling plugin must still be executable
    let good_result = runtime.process(&good_id, 0.5, 0.001)?;
    assert!(
        good_result.is_finite(),
        "Sibling plugin must still produce a finite output"
    );

    Ok(())
}

/// Plugin lifecycle: load → process → unload without leaving orphaned state.
#[test]
fn test_plugin_lifecycle_load_process_unload() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_plugins::wasm::{PluginId, WasmRuntime};

    let wasm_bytes = wat::parse_str(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let plugin_id = PluginId::new_v4();

    // Load
    runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])?;
    assert!(
        runtime.has_plugin(&plugin_id),
        "Plugin should be present after load"
    );

    // Process
    let output = runtime.process(&plugin_id, 0.75, 0.001)?;
    assert!(
        (output - 0.75).abs() < f32::EPSILON,
        "Passthrough plugin should return the input unchanged"
    );

    // Unload
    runtime.unload_plugin(&plugin_id)?;
    assert!(
        !runtime.has_plugin(&plugin_id),
        "Plugin should be absent after unload"
    );

    Ok(())
}

/// Integration test for complete plugin workflow
#[tokio::test]
async fn test_plugin_workflow_integration() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let plugin_dir = temp_dir.path().to_path_buf();

    // Create plugin host
    let host = host::PluginHost::new(plugin_dir)
        .await
        .expect("Failed to create plugin host");

    // Test that empty directory works
    let registry = host.get_registry().await;
    assert_eq!(registry.len(), 0);

    // Test quarantine stats
    let quarantine_stats = host.get_quarantine_stats().await;
    assert_eq!(quarantine_stats.len(), 0);

    // Test loading all plugins (should be no-op)
    host.load_all_plugins()
        .await
        .expect("Failed to load all plugins");

    // Test unloading all plugins (should be no-op)
    host.unload_all_plugins()
        .await
        .expect("Failed to unload all plugins");
}
