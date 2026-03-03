//! Deep plugin security and isolation tests.
//!
//! Covers: capability enforcement, capability escalation prevention,
//! resource limits, signing verification, WASM sandboxing, native isolation,
//! plugin lifecycle, hot-reload, and ABI version compatibility.

use std::path::Path;

use tempfile::TempDir;
use uuid::Uuid;

use openracing_crypto::TrustLevel;
use openracing_crypto::ed25519::{Ed25519Signer, KeyPair};
use openracing_crypto::trust_store::TrustStore;
use openracing_crypto::verification::ContentType;

use racing_wheel_plugins::capability::CapabilityChecker;
use racing_wheel_plugins::manifest::{
    Capability, EntryPoints, ManifestValidator, PluginConstraints, PluginManifest, PluginOperation,
};
use racing_wheel_plugins::native::{
    AbiCheckResult, CURRENT_ABI_VERSION, SignatureVerificationConfig, SignatureVerifier,
    check_abi_compatibility,
};
use racing_wheel_plugins::quarantine::{QuarantineManager, QuarantinePolicy, ViolationType};
use racing_wheel_plugins::registry::{VersionCompatibility, check_compatibility};
use racing_wheel_plugins::wasm::{ResourceLimits, WasmRuntime};
use racing_wheel_plugins::{PluginClass, PluginError};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_manifest(class: PluginClass) -> PluginManifest {
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

/// WAT: minimal passthrough plugin.
const PASSTHROUGH_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
    )
)
"#;

/// WAT: plugin that traps on process (simulates crash).
const TRAP_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        unreachable
    )
)
"#;

/// WAT: plugin with init + shutdown + process (lifecycle).
const LIFECYCLE_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "init") (result i32)
        i32.const 0
    )
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
    )
    (func (export "shutdown")
        nop
    )
)
"#;

/// WAT: infinite loop to exhaust fuel budget.
const INFINITE_LOOP_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        (loop $lp
            br $lp
        )
        local.get 0
    )
)
"#;

/// WAT: updated passthrough that doubles output (for hot-reload).
const DOUBLE_OUTPUT_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
        local.get 0
        f32.add
    )
)
"#;

fn compile_wat(wat: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(wat)?;
    Ok(wasm)
}

// ===================================================================
// 1. Capability Enforcement
// ===================================================================

#[test]
fn capability_file_access_beyond_grant_denied() -> Result<(), Box<dyn std::error::Error>> {
    let checker = CapabilityChecker::new(vec![
        Capability::ReadTelemetry,
        Capability::FileSystem {
            paths: vec!["/plugins/data".to_string()],
        },
    ]);

    // Allowed path
    assert!(
        checker
            .check_file_access(Path::new("/plugins/data/config.json"))
            .is_ok()
    );

    // Path outside grant is denied
    let result = checker.check_file_access(Path::new("/etc/passwd"));
    assert!(result.is_err());
    let err = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        err.contains("FileSystem") || err.contains("Capability"),
        "expected capability violation, got: {err}"
    );

    // Parent traversal outside grant is denied
    let result = checker.check_file_access(Path::new("/home/user/secret.key"));
    assert!(result.is_err());

    Ok(())
}

// ===================================================================
// 2. Capability Escalation Prevention
// ===================================================================

#[test]
fn capability_escalation_plugin_cannot_elevate_permissions() -> Result<(), PluginError> {
    // Plugin granted only ReadTelemetry cannot access DSP, LED, modify, etc.
    let checker = CapabilityChecker::new(vec![Capability::ReadTelemetry]);

    assert!(checker.check_telemetry_read().is_ok());
    assert!(checker.check_telemetry_modify().is_err());
    assert!(checker.check_led_control().is_err());
    assert!(checker.check_dsp_processing().is_err());
    assert!(checker.check_inter_plugin_comm().is_err());
    assert!(checker.check_network_access("any-host.com").is_err());

    // Manifest validator rejects capabilities that are not allowed for the class.
    // Safe plugins cannot request ProcessDsp
    let validator = ManifestValidator::default();
    let mut manifest = make_manifest(PluginClass::Safe);
    manifest.capabilities = vec![Capability::ProcessDsp];
    let result = validator.validate(&manifest);
    assert!(result.is_err());

    // Safe plugins cannot request Network
    let mut manifest = make_manifest(PluginClass::Safe);
    manifest.capabilities = vec![Capability::Network {
        hosts: vec!["evil.com".to_string()],
    }];
    assert!(validator.validate(&manifest).is_err());

    // Safe plugins cannot request FileSystem
    let mut manifest = make_manifest(PluginClass::Safe);
    manifest.capabilities = vec![Capability::FileSystem {
        paths: vec!["/".to_string()],
    }];
    assert!(validator.validate(&manifest).is_err());

    Ok(())
}

// ===================================================================
// 3. Resource Limits — fuel exhaustion terminates plugin
// ===================================================================

#[test]
fn resource_limits_fuel_exhaustion_terminates_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let wasm_bytes = compile_wat(INFINITE_LOOP_WAT)?;

    // Very low fuel so the loop exhausts quickly
    let limits = ResourceLimits::default().with_fuel(1_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;

    let id = Uuid::new_v4();
    runtime.load_plugin_from_bytes(id, &wasm_bytes, vec![Capability::ReadTelemetry])?;

    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err(), "infinite-loop plugin must be terminated");

    // Plugin should now be disabled
    let disabled = runtime.is_plugin_disabled(&id)?;
    assert!(disabled, "plugin must be disabled after fuel exhaustion");

    Ok(())
}

// ===================================================================
// 4. Signing Verification — unsigned native plugin rejected
// ===================================================================

#[test]
fn signing_unsigned_native_plugin_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let plugin_path = temp_dir.path().join("unsigned_plugin.dll");
    std::fs::write(&plugin_path, b"fake DLL content")?;

    let trust_store = TrustStore::new_in_memory();
    let config = SignatureVerificationConfig::strict();
    let verifier = SignatureVerifier::new(&trust_store, config);

    let result = verifier.verify(&plugin_path);
    assert!(
        result.is_err(),
        "unsigned plugin must be rejected in strict mode"
    );

    let err_msg = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        err_msg.to_lowercase().contains("unsigned"),
        "error should mention unsigned: {err_msg}"
    );

    Ok(())
}

// ===================================================================
// 5. Signing Verification — tampered signature detected
// ===================================================================

#[test]
fn signing_tampered_signature_detected() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let plugin_path = temp_dir.path().join("tampered_plugin.dll");
    let plugin_content = b"real DLL content for signing";
    std::fs::write(&plugin_path, plugin_content)?;

    let keypair = KeyPair::generate()?;
    let metadata = Ed25519Signer::sign_with_metadata(
        plugin_content,
        &keypair,
        "Test Signer",
        ContentType::Plugin,
        None,
    )?;

    // Write valid signature file
    let sig_path = plugin_path.with_extension("dll.sig");
    let sig_json = serde_json::to_string_pretty(&metadata)?;
    std::fs::write(&sig_path, &sig_json)?;

    // Now tamper with the plugin content after signing
    std::fs::write(&plugin_path, b"TAMPERED DLL content!!!")?;

    // Create trust store with the signer's key
    let mut trust_store = TrustStore::new_in_memory();
    trust_store.add_key(
        keypair.public_key.clone(),
        TrustLevel::Trusted,
        Some("test key".to_string()),
    )?;

    let config = SignatureVerificationConfig::strict();
    let verifier = SignatureVerifier::new(&trust_store, config);
    let result = verifier.verify(&plugin_path);

    assert!(
        result.is_err(),
        "tampered plugin must fail signature verification"
    );
    let err_msg = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        err_msg.to_lowercase().contains("signature")
            || err_msg.to_lowercase().contains("verification"),
        "error should mention signature failure: {err_msg}"
    );

    Ok(())
}

// ===================================================================
// 6. WASM Sandbox — cannot access host filesystem
// ===================================================================

#[test]
fn wasm_sandbox_no_host_filesystem_access() -> Result<(), Box<dyn std::error::Error>> {
    // A WASM plugin loaded with no FileSystem capability cannot access host FS.
    // The CapabilityChecker should deny all file access.
    let checker = CapabilityChecker::new(vec![Capability::ReadTelemetry]);

    // Any filesystem path should be denied
    assert!(checker.check_file_access(Path::new("/tmp")).is_err());
    assert!(checker.check_file_access(Path::new("/home")).is_err());
    assert!(checker.check_file_access(Path::new("C:\\Windows")).is_err());
    assert!(
        checker
            .check_file_access(Path::new("/proc/self/status"))
            .is_err()
    );

    // Even with FileSystem capability, only granted paths work
    let restricted = CapabilityChecker::new(vec![Capability::FileSystem {
        paths: vec!["/sandbox".to_string()],
    }]);
    assert!(
        restricted
            .check_file_access(Path::new("/sandbox/file.txt"))
            .is_ok()
    );
    assert!(
        restricted
            .check_file_access(Path::new("/etc/hosts"))
            .is_err()
    );

    Ok(())
}

// ===================================================================
// 7. WASM Sandbox — cannot make network calls
// ===================================================================

#[test]
fn wasm_sandbox_no_network_calls() -> Result<(), Box<dyn std::error::Error>> {
    // Plugin with no Network capability cannot access any host
    let checker = CapabilityChecker::new(vec![Capability::ReadTelemetry]);

    assert!(checker.check_network_access("api.example.com").is_err());
    assert!(checker.check_network_access("localhost").is_err());
    assert!(checker.check_network_access("127.0.0.1").is_err());

    // With Network capability, only specified hosts are allowed
    let restricted = CapabilityChecker::new(vec![Capability::Network {
        hosts: vec!["api.openracing.io".to_string()],
    }]);
    assert!(restricted.check_network_access("api.openracing.io").is_ok());
    assert!(restricted.check_network_access("evil.com").is_err());
    assert!(restricted.check_network_access("localhost").is_err());

    Ok(())
}

// ===================================================================
// 8. Native Isolation — crash doesn't take down host
// ===================================================================

#[test]
fn native_isolation_wasm_crash_does_not_take_down_host() -> Result<(), Box<dyn std::error::Error>> {
    let trap_bytes = compile_wat(TRAP_WAT)?;
    let pass_bytes = compile_wat(PASSTHROUGH_WAT)?;

    let mut runtime = WasmRuntime::new()?;

    let crash_id = Uuid::new_v4();
    let healthy_id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(crash_id, &trap_bytes, vec![Capability::ReadTelemetry])?;
    runtime.load_plugin_from_bytes(healthy_id, &pass_bytes, vec![Capability::ReadTelemetry])?;

    // Crashing plugin produces an error
    let result = runtime.process(&crash_id, 1.0, 0.001);
    assert!(result.is_err(), "crashing plugin must return error");

    // Healthy plugin continues to work after the crash
    let output = runtime.process(&healthy_id, 42.0, 0.001)?;
    assert!(
        (output - 42.0).abs() < f32::EPSILON,
        "healthy plugin must still function after sibling crash"
    );

    // Crashing plugin is now disabled
    let disabled = runtime.is_plugin_disabled(&crash_id)?;
    assert!(disabled, "crashed plugin must be disabled");

    // Healthy plugin is not disabled
    let disabled = runtime.is_plugin_disabled(&healthy_id)?;
    assert!(!disabled, "healthy plugin must not be disabled");

    Ok(())
}

// ===================================================================
// 9. Plugin Lifecycle — load → init → run → unload → cleanup
// ===================================================================

#[test]
fn plugin_lifecycle_load_init_run_unload() -> Result<(), Box<dyn std::error::Error>> {
    let wasm_bytes = compile_wat(LIFECYCLE_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    // Load (includes init)
    runtime.load_plugin_from_bytes(id, &wasm_bytes, vec![Capability::ReadTelemetry])?;
    assert!(runtime.has_plugin(&id));
    assert!(runtime.is_plugin_initialized(&id)?);

    // Run
    let output = runtime.process(&id, 5.0, 0.001)?;
    assert!((output - 5.0).abs() < f32::EPSILON);

    // Check stats recorded
    let (count, _avg) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count, 1, "one process call should be recorded");

    // Unload (calls shutdown internally)
    runtime.unload_plugin(&id)?;
    assert!(!runtime.has_plugin(&id));

    // After unload, process should fail
    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err(), "process after unload must fail");

    Ok(())
}

// ===================================================================
// 10. Hot-Reload — preserves state or cleanly resets
// ===================================================================

#[test]
fn hot_reload_preserves_state() -> Result<(), Box<dyn std::error::Error>> {
    let v1_bytes = compile_wat(PASSTHROUGH_WAT)?;
    let v2_bytes = compile_wat(DOUBLE_OUTPUT_WAT)?;

    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    // Load v1
    runtime.load_plugin_from_bytes(id, &v1_bytes, vec![Capability::ReadTelemetry])?;

    // Process a few times to accumulate stats
    for _ in 0..5 {
        let _ = runtime.process(&id, 1.0, 0.001)?;
    }
    let (count_before, _) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count_before, 5);

    // Hot-reload to v2
    runtime.reload_plugin(&id, &v2_bytes, vec![Capability::ReadTelemetry])?;

    // Stats should be preserved
    let (count_after, _) = runtime.get_plugin_stats(&id)?;
    assert_eq!(
        count_after, count_before,
        "process count must be preserved across hot-reload"
    );

    // New behavior should be active (v2 doubles)
    let output = runtime.process(&id, 3.0, 0.001)?;
    assert!(
        (output - 6.0).abs() < f32::EPSILON,
        "hot-reloaded plugin must use new code"
    );

    Ok(())
}

#[test]
fn hot_reload_failure_keeps_old_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let v1_bytes = compile_wat(PASSTHROUGH_WAT)?;
    let invalid_bytes = b"this is not valid wasm";

    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &v1_bytes, vec![Capability::ReadTelemetry])?;

    // Attempt reload with invalid WASM — should fail
    let result = runtime.reload_plugin(&id, invalid_bytes, vec![Capability::ReadTelemetry]);
    assert!(result.is_err(), "reload with invalid WASM must fail");

    // Old plugin must still work
    let output = runtime.process(&id, 10.0, 0.001)?;
    assert!(
        (output - 10.0).abs() < f32::EPSILON,
        "old plugin must remain active after failed reload"
    );

    Ok(())
}

// ===================================================================
// 11. Version Compatibility — incompatible ABI rejected
// ===================================================================

#[test]
fn version_incompatible_abi_rejected() -> Result<(), Box<dyn std::error::Error>> {
    // Current version is compatible
    assert_eq!(
        check_abi_compatibility(CURRENT_ABI_VERSION),
        AbiCheckResult::Compatible
    );

    // Version 0 is incompatible (assuming CURRENT is 1)
    let result = check_abi_compatibility(0);
    assert!(
        matches!(result, AbiCheckResult::Mismatch { .. }),
        "ABI version 0 must be rejected"
    );

    // Far-future version is incompatible
    let result = check_abi_compatibility(999);
    assert!(
        matches!(result, AbiCheckResult::Mismatch { expected, actual } if expected == CURRENT_ABI_VERSION && actual == 999),
        "ABI version 999 must be rejected"
    );

    // Semver compatibility: different major is incompatible
    let required = semver::Version::new(1, 0, 0);
    let available = semver::Version::new(2, 0, 0);
    assert_eq!(
        check_compatibility(&required, &available),
        VersionCompatibility::Incompatible
    );

    // Same major, higher minor is compatible
    let required = semver::Version::new(1, 0, 0);
    let available = semver::Version::new(1, 2, 0);
    assert_eq!(
        check_compatibility(&required, &available),
        VersionCompatibility::Compatible
    );

    // Available lower than required is incompatible
    let required = semver::Version::new(1, 5, 0);
    let available = semver::Version::new(1, 2, 0);
    assert_eq!(
        check_compatibility(&required, &available),
        VersionCompatibility::Incompatible
    );

    Ok(())
}

// ===================================================================
// Supplementary: Quarantine escalation from violations
// ===================================================================

#[test]
fn quarantine_budget_violations_escalate() -> Result<(), Box<dyn std::error::Error>> {
    let policy = QuarantinePolicy {
        max_crashes: 3,
        max_budget_violations: 3,
        violation_window_minutes: 60,
        quarantine_duration_minutes: 60,
        max_escalation_levels: 5,
    };
    let mut manager = QuarantineManager::new(policy);
    let plugin_id = Uuid::new_v4();

    // Record violations up to the threshold
    for i in 0..3 {
        manager.record_violation(
            plugin_id,
            ViolationType::BudgetViolation,
            format!("violation {i}"),
        )?;
    }

    // Plugin should now be quarantined
    assert!(
        manager.is_quarantined(plugin_id),
        "plugin must be quarantined after exceeding budget violation threshold"
    );

    Ok(())
}

#[test]
fn signing_permissive_allows_unsigned() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let plugin_path = temp_dir.path().join("dev_plugin.dll");
    std::fs::write(&plugin_path, b"dev plugin content")?;

    let trust_store = TrustStore::new_in_memory();
    let config = SignatureVerificationConfig::permissive();
    let verifier = SignatureVerifier::new(&trust_store, config);

    let result = verifier.verify(&plugin_path)?;
    assert!(!result.is_signed);
    assert!(
        result.verified,
        "permissive mode must allow unsigned plugins"
    );

    Ok(())
}
