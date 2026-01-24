//! Integration tests for wheelctl CLI
//!
//! These tests cover all major command workflows with error code validation
//! as required by the task specification.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use tempfile::TempDir;

/// Custom predicate to check if output is valid JSON
fn is_json() -> impl predicates::Predicate<[u8]> {
    predicates::function::function(|s: &[u8]| {
        if let Ok(text) = std::str::from_utf8(s) {
            serde_json::from_str::<Value>(text).is_ok()
        } else {
            false
        }
    })
}

/// Test helper to create a wheelctl command
fn wheelctl() -> Command {
    Command::cargo_bin("wheelctl").unwrap()
}

/// Test helper to create temporary profile
fn create_test_profile(dir: &TempDir, name: &str) -> std::path::PathBuf {
    let profile = serde_json::json!({
        "schema": "wheel.profile/1",
        "scope": {
            "game": "iracing",
            "car": "gt3"
        },
        "base": {
            "ffbGain": 0.75,
            "dorDeg": 540,
            "torqueCapNm": 8.0,
            "filters": {
                "reconstruction": 4,
                "friction": 0.12,
                "damper": 0.18,
                "inertia": 0.08,
                "notchFilters": [],
                "slewRate": 0.85,
                "curvePoints": [
                    {"input": 0.0, "output": 0.0},
                    {"input": 1.0, "output": 1.0}
                ]
            }
        }
    });

    let path = dir.path().join(format!("{}.json", name));
    fs::write(&path, serde_json::to_string_pretty(&profile).unwrap()).unwrap();
    path
}

#[test]
fn test_cli_help() {
    wheelctl()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Racing Wheel Software Suite"));
}

#[test]
fn test_cli_version() {
    wheelctl()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("wheelctl"));
}

#[test]
fn test_completion_generation() {
    wheelctl()
        .args(&["completion", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("_wheelctl"));
}

// Device Management Tests

#[test]
fn test_device_list_human_output() {
    wheelctl()
        .args(&["device", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Connected Devices"));
}

#[test]
fn test_device_list_json_output() {
    wheelctl()
        .args(&["--json", "device", "list"])
        .assert()
        .success()
        .stdout(is_json());

    // Verify JSON structure
    let output = wheelctl()
        .args(&["--json", "device", "list"])
        .output()
        .unwrap();

    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["success"], true);
    assert!(json["devices"].is_array());
}

#[test]
fn test_device_list_detailed() {
    wheelctl()
        .args(&["device", "list", "--detailed"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Capabilities"));
}

#[test]
fn test_device_status() {
    wheelctl()
        .args(&["device", "status", "wheel-001"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Device:"));
}

#[test]
fn test_device_status_json() {
    wheelctl()
        .args(&["--json", "device", "status", "wheel-001"])
        .assert()
        .success()
        .stdout(is_json());
}

#[test]
fn test_device_not_found_error() {
    wheelctl()
        .args(&["device", "status", "nonexistent-device"])
        .assert()
        .failure()
        .code(2); // Device not found error code
}

#[test]
fn test_device_calibrate() {
    wheelctl()
        .args(&["device", "calibrate", "wheel-001", "center", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("calibration"));
}

#[test]
fn test_device_reset() {
    wheelctl()
        .args(&["device", "reset", "wheel-001", "--force"])
        .assert()
        .success()
        .stdout(predicate::str::contains("reset"));
}

// Profile Management Tests

#[test]
fn test_profile_list() {
    wheelctl().args(&["profile", "list"]).assert().success();
}

#[test]
fn test_profile_list_json() {
    wheelctl()
        .args(&["--json", "profile", "list"])
        .assert()
        .success()
        .stdout(is_json());
}

#[test]
fn test_profile_create_and_validate() {
    let temp_dir = TempDir::new().unwrap();
    let profile_path = temp_dir.path().join("test_profile.json");

    // Create profile
    wheelctl()
        .args(&[
            "profile",
            "create",
            profile_path.to_str().unwrap(),
            "--game",
            "iracing",
            "--car",
            "gt3",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Profile created"));

    // Validate profile
    wheelctl()
        .args(&["profile", "validate", profile_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("valid"));
}

#[test]
fn test_profile_show() {
    let temp_dir = TempDir::new().unwrap();
    let profile_path = create_test_profile(&temp_dir, "test");

    wheelctl()
        .args(&["profile", "show", profile_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Profile Schema"));
}

#[test]
fn test_profile_show_json() {
    let temp_dir = TempDir::new().unwrap();
    let profile_path = create_test_profile(&temp_dir, "test");

    wheelctl()
        .args(&["--json", "profile", "show", profile_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(is_json());
}

#[test]
fn test_profile_apply() {
    let temp_dir = TempDir::new().unwrap();
    let profile_path = create_test_profile(&temp_dir, "test");

    wheelctl()
        .args(&[
            "profile",
            "apply",
            "wheel-001",
            profile_path.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("applied"));
}

#[test]
fn test_profile_not_found_error() {
    wheelctl()
        .args(&["profile", "show", "nonexistent.json"])
        .assert()
        .failure()
        .code(3); // Profile not found error code
}

#[test]
fn test_profile_validation_error() {
    let temp_dir = TempDir::new().unwrap();
    let invalid_profile = temp_dir.path().join("invalid.json");
    fs::write(&invalid_profile, "{ invalid json }").unwrap();

    wheelctl()
        .args(&["profile", "validate", invalid_profile.to_str().unwrap()])
        .assert()
        .failure()
        .code(4); // Validation error code
}

#[test]
fn test_profile_edit_field() {
    let temp_dir = TempDir::new().unwrap();
    let profile_path = create_test_profile(&temp_dir, "test");

    wheelctl()
        .args(&[
            "profile",
            "edit",
            profile_path.to_str().unwrap(),
            "--field",
            "base.ffbGain",
            "--value",
            "0.8",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("updated"));
}

#[test]
fn test_profile_export_import() {
    let temp_dir = TempDir::new().unwrap();
    let profile_path = create_test_profile(&temp_dir, "test");
    let export_path = temp_dir.path().join("exported.json");
    let import_path = temp_dir.path().join("imported.json");

    // Export profile
    wheelctl()
        .args(&[
            "profile",
            "export",
            profile_path.to_str().unwrap(),
            "--output",
            export_path.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("exported"));

    // Import profile
    wheelctl()
        .args(&[
            "profile",
            "import",
            export_path.to_str().unwrap(),
            "--target",
            import_path.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("imported"));
}

// Diagnostic Tests

#[test]
fn test_diag_test() {
    wheelctl()
        .args(&["diag", "test", "--device", "wheel-001"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Diagnostic Results"));
}

#[test]
fn test_diag_test_json() {
    wheelctl()
        .args(&["--json", "diag", "test", "--device", "wheel-001"])
        .assert()
        .success()
        .stdout(is_json());
}

#[test]
fn test_diag_record() {
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("test.wbb");

    wheelctl()
        .args(&[
            "diag",
            "record",
            "wheel-001",
            "--duration",
            "1",
            "--output",
            output_path.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("recorded"));
}

#[test]
fn test_diag_replay() {
    let temp_dir = TempDir::new().unwrap();
    let blackbox_path = temp_dir.path().join("test.wbb");
    fs::write(&blackbox_path, "WBB1\x00\x00\x00\x00Mock blackbox data").unwrap();

    wheelctl()
        .args(&["diag", "replay", blackbox_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Replay completed"));
}

#[test]
fn test_diag_support_bundle() {
    let temp_dir = TempDir::new().unwrap();
    let bundle_path = temp_dir.path().join("support.zip");

    wheelctl()
        .args(&["diag", "support", "--output", bundle_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Support bundle created"));
}

#[test]
fn test_diag_metrics() {
    wheelctl()
        .args(&["diag", "metrics"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Diagnostics"));
}

// Game Integration Tests

#[test]
fn test_game_list() {
    wheelctl()
        .args(&["game", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Supported Games"));
}

#[test]
fn test_game_list_detailed() {
    wheelctl()
        .args(&["game", "list", "--detailed"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Features"));
}

#[test]
fn test_game_configure() {
    wheelctl()
        .args(&[
            "game",
            "configure",
            "iracing",
            "--path",
            "/test/path",
            "--auto",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Configured"));
}

#[test]
fn test_game_status() {
    wheelctl()
        .args(&["game", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Game Status"));
}

#[test]
fn test_game_test_telemetry() {
    wheelctl()
        .args(&["game", "test", "iracing", "--duration", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Test Results"));
}

// Safety Tests

#[test]
fn test_safety_status() {
    wheelctl()
        .args(&["safety", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Safety Status"));
}

#[test]
fn test_safety_enable_high_torque() {
    wheelctl()
        .args(&["safety", "enable", "wheel-001", "--force"])
        .assert()
        .success()
        .stdout(predicate::str::contains("High torque mode enabled"));
}

#[test]
fn test_safety_emergency_stop() {
    wheelctl()
        .args(&["safety", "stop"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Emergency stop"));
}

#[test]
fn test_safety_set_limit() {
    wheelctl()
        .args(&["safety", "limit", "wheel-001", "5.0"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Torque limit set"));
}

#[test]
fn test_safety_invalid_limit() {
    wheelctl()
        .args(&["safety", "limit", "wheel-001", "50.0"])
        .assert()
        .failure()
        .code(4); // Validation error
}

// Health Monitoring Tests

#[test]
fn test_health_status() {
    wheelctl()
        .args(&["health"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Service Health Status"));
}

#[test]
fn test_health_status_json() {
    wheelctl()
        .args(&["--json", "health"])
        .assert()
        .success()
        .stdout(is_json());
}

// Error Code Tests

#[test]
fn test_service_unavailable_error() {
    wheelctl()
        .env("WHEELCTL_ENDPOINT", "http://invalid:99999")
        .args(&["device", "list"])
        .assert()
        .failure()
        .code(5); // Service unavailable error code
}

#[test]
fn test_invalid_command() {
    wheelctl()
        .args(&["invalid", "command"])
        .assert()
        .failure()
        .code(1); // General error code
}

// JSON Output Validation Tests

#[test]
fn test_all_commands_support_json() {
    let commands = vec![
        vec!["device", "list"],
        vec!["profile", "list"],
        vec!["game", "list"],
        vec!["safety", "status"],
        vec!["health"],
        vec!["diag", "metrics"],
    ];

    for cmd in commands {
        let mut full_cmd = vec!["--json"];
        full_cmd.extend(cmd.iter());

        wheelctl()
            .args(&full_cmd)
            .assert()
            .success()
            .stdout(is_json());
    }
}

// Verbose Logging Tests

#[test]
fn test_verbose_logging() {
    wheelctl()
        .args(&["-v", "device", "list"])
        .assert()
        .success();

    wheelctl()
        .args(&["-vv", "device", "list"])
        .assert()
        .success();

    wheelctl()
        .args(&["-vvv", "device", "list"])
        .assert()
        .success();
}

// End-to-End Workflow Tests

#[test]
fn test_complete_profile_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let profile_path = temp_dir.path().join("workflow_test.json");

    // Create profile
    wheelctl()
        .args(&[
            "profile",
            "create",
            profile_path.to_str().unwrap(),
            "--game",
            "iracing",
        ])
        .assert()
        .success();

    // Validate profile
    wheelctl()
        .args(&["profile", "validate", profile_path.to_str().unwrap()])
        .assert()
        .success();

    // Edit profile
    wheelctl()
        .args(&[
            "profile",
            "edit",
            profile_path.to_str().unwrap(),
            "--field",
            "base.ffbGain",
            "--value",
            "0.9",
        ])
        .assert()
        .success();

    // Apply profile
    wheelctl()
        .args(&[
            "profile",
            "apply",
            "wheel-001",
            profile_path.to_str().unwrap(),
        ])
        .assert()
        .success();
}

#[test]
fn test_complete_diagnostic_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let blackbox_path = temp_dir.path().join("diag_test.wbb");
    let support_path = temp_dir.path().join("support_test.zip");

    // Run diagnostics
    wheelctl()
        .args(&["diag", "test", "--device", "wheel-001"])
        .assert()
        .success();

    // Record blackbox
    wheelctl()
        .args(&[
            "diag",
            "record",
            "wheel-001",
            "--duration",
            "1",
            "--output",
            blackbox_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Generate support bundle
    wheelctl()
        .args(&[
            "diag",
            "support",
            "--output",
            support_path.to_str().unwrap(),
        ])
        .assert()
        .success();
}
