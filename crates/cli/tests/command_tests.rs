//! Command-level tests for the wheelctl CLI binary.
//!
//! These tests exercise argument parsing, help text, error output,
//! and output formatting through the compiled binary using `assert_cmd`.
#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn wheelctl() -> TestResult<Command> {
    let mut cmd = Command::cargo_bin("wheelctl")?;
    // Ensure we never hit a real service endpoint
    cmd.env_remove("WHEELCTL_ENDPOINT");
    Ok(cmd)
}

fn parse_json_stdout(cmd: &mut Command) -> TestResult {
    let output = cmd.output()?;
    assert!(output.status.success(), "command should succeed");
    let _: Value = serde_json::from_slice(&output.stdout)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Help text generation
// ---------------------------------------------------------------------------

#[test]
fn help_mentions_json_flag() -> TestResult {
    wheelctl()?
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--json"));
    Ok(())
}

#[test]
fn help_mentions_verbose_flag() -> TestResult {
    wheelctl()?
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--verbose"));
    Ok(())
}

#[test]
fn device_help_lists_calibrate_and_reset() -> TestResult {
    wheelctl()?
        .args(["device", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("calibrate"))
        .stdout(predicate::str::contains("reset"));
    Ok(())
}

#[test]
fn profile_help_lists_export_and_import() -> TestResult {
    wheelctl()?
        .args(["profile", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("export"))
        .stdout(predicate::str::contains("import"));
    Ok(())
}

#[test]
fn plugin_help_lists_install_and_uninstall() -> TestResult {
    wheelctl()?
        .args(["plugin", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("install"))
        .stdout(predicate::str::contains("uninstall"));
    Ok(())
}

#[test]
fn safety_help_lists_enable_and_stop() -> TestResult {
    wheelctl()?
        .args(["safety", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("enable"))
        .stdout(predicate::str::contains("stop"));
    Ok(())
}

#[test]
fn diag_help_lists_all_subcommands() -> TestResult {
    wheelctl()?
        .args(["diag", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("test"))
        .stdout(predicate::str::contains("record"))
        .stdout(predicate::str::contains("replay"))
        .stdout(predicate::str::contains("support"))
        .stdout(predicate::str::contains("metrics"));
    Ok(())
}

#[test]
fn game_help_lists_configure_and_test() -> TestResult {
    wheelctl()?
        .args(["game", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("configure"))
        .stdout(predicate::str::contains("test"));
    Ok(())
}

#[test]
fn telemetry_help_lists_probe_and_capture() -> TestResult {
    wheelctl()?
        .args(["telemetry", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("probe"))
        .stdout(predicate::str::contains("capture"));
    Ok(())
}

// ---------------------------------------------------------------------------
// Error messages for invalid arguments
// ---------------------------------------------------------------------------

#[test]
fn empty_args_shows_usage_error() -> TestResult {
    wheelctl()?
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
    Ok(())
}

#[test]
fn unknown_top_level_command_error_message() -> TestResult {
    let output = wheelctl()?
        .args(["frobnicate"])
        .output()
        ?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // clap should mention the invalid value
    assert!(
        stderr.contains("frobnicate") || stderr.contains("invalid"),
        "stderr should reference the bad command: {}",
        stderr
    );
    Ok(())
}

#[test]
fn unknown_device_subcommand_stderr() -> TestResult {
    let output = wheelctl()?
        .args(["device", "fly"])
        .output()
        ?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("fly") || stderr.contains("invalid"),
        "stderr should reference the bad subcommand: {}",
        stderr
    );
    Ok(())
}

#[test]
fn missing_required_arg_produces_error() -> TestResult {
    // `device status` requires a device arg
    wheelctl()?
        .args(["device", "status"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("<DEVICE>").or(predicate::str::contains("required")));
    Ok(())
}

#[test]
fn invalid_torque_value_produces_error() -> TestResult {
    // torque must be numeric
    wheelctl()?
        .args(["safety", "limit", "dev1", "not_a_number"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value"));
    Ok(())
}

#[test]
fn invalid_calibration_type_error() -> TestResult {
    wheelctl()?
        .args(["device", "calibrate", "w1", "moonwalk"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value"));
    Ok(())
}

#[test]
fn missing_capture_out_flag() -> TestResult {
    // --out is required for telemetry capture
    wheelctl()?
        .args(["telemetry", "capture", "--game", "acc"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--out"));
    Ok(())
}

#[test]
fn missing_probe_game_flag() -> TestResult {
    // --game is required for telemetry probe
    wheelctl()?
        .args(["telemetry", "probe"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--game"));
    Ok(())
}

// ---------------------------------------------------------------------------
// Output formatting: JSON
// ---------------------------------------------------------------------------

#[test]
fn device_list_json_has_success_field() -> TestResult {
    let output = wheelctl()?.args(["--json", "device", "list"]).output()?;
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(json["success"], true);
    assert!(json["devices"].is_array());
    Ok(())
}

#[test]
fn device_status_json_has_status_field() -> TestResult {
    let output = wheelctl()?
        .args(["--json", "device", "status", "wheel-001"])
        .output()?;
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(json["success"], true);
    assert!(json.get("status").is_some());
    Ok(())
}

#[test]
fn game_status_json_structure() -> TestResult {
    let output = wheelctl()?.args(["--json", "game", "status"]).output()?;
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(json["success"], true);
    assert!(json.get("game_status").is_some());
    Ok(())
}

#[test]
fn health_json_structure() -> TestResult {
    parse_json_stdout(wheelctl()?.args(["--json", "health"]))
}

#[test]
fn diag_metrics_json_structure() -> TestResult {
    parse_json_stdout(wheelctl()?.args(["--json", "diag", "metrics"]))
}

#[test]
fn safety_status_json_structure() -> TestResult {
    parse_json_stdout(wheelctl()?.args(["--json", "safety", "status"]))
}

#[test]
fn profile_list_json_structure() -> TestResult {
    parse_json_stdout(wheelctl()?.args(["--json", "profile", "list"]))
}

#[test]
fn plugin_list_json_structure() -> TestResult {
    parse_json_stdout(wheelctl()?.args(["--json", "plugin", "list"]))
}

// ---------------------------------------------------------------------------
// Output formatting: plain/human
// ---------------------------------------------------------------------------

#[test]
fn device_list_human_contains_device_names() -> TestResult {
    wheelctl()?
        .args(["device", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Fanatec DD Pro"));
    Ok(())
}

#[test]
fn device_list_detailed_shows_capabilities() -> TestResult {
    wheelctl()?
        .args(["device", "list", "--detailed"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Max Torque"));
    Ok(())
}

#[test]
fn game_list_human_mentions_supported_games() -> TestResult {
    wheelctl()?
        .args(["game", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Supported Games"));
    Ok(())
}

// ---------------------------------------------------------------------------
// Error JSON formatting
// ---------------------------------------------------------------------------

#[test]
fn device_not_found_json_error() -> TestResult {
    let output = wheelctl()?
        .args(["--json", "device", "status", "nonexistent-device"])
        .output()?;
    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(json["success"], false);
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("not found")
    );
    Ok(())
}

#[test]
fn service_unavailable_json_error() -> TestResult {
    let output = wheelctl()?
        .env("WHEELCTL_ENDPOINT", "http://invalid:99999")
        .args(["--json", "device", "list"])
        .output()?;
    assert!(!output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(json["success"], false);
    Ok(())
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

#[test]
fn completion_all_shells() -> TestResult {
    for shell in &["bash", "zsh", "fish", "powershell"] {
        wheelctl()?.args(["completion", shell]).assert().success();
    }
    Ok(())
}

#[test]
fn multiple_verbose_flags_accepted() -> TestResult {
    wheelctl()?
        .args(["-vvvv", "device", "list"])
        .assert()
        .success();
    Ok(())
}

#[test]
fn json_flag_after_deepest_subcommand() -> TestResult {
    // Ensure --json works even when placed after the subcommand args
    let output = wheelctl()?
        .args(["device", "list", "--detailed", "--json"])
        .output()?;
    assert!(output.status.success());
    let _: Value = serde_json::from_slice(&output.stdout)?;
    Ok(())
}

#[test]
fn error_exit_code_for_device_not_found_is_2() -> TestResult {
    wheelctl()?
        .args(["device", "status", "no-such-device"])
        .assert()
        .failure()
        .code(2);
    Ok(())
}

#[test]
fn error_exit_code_for_service_unavailable_is_5() -> TestResult {
    wheelctl()?
        .env("WHEELCTL_ENDPOINT", "http://invalid:99999")
        .args(["device", "list"])
        .assert()
        .failure()
        .code(5);
    Ok(())
}
