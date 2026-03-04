//! Deep CLI command integration tests.
//!
//! Tests command output structure, help text, version format,
//! error messages, JSON/verbose/quiet modes, and piping compatibility.

#![allow(deprecated)]

use assert_cmd::Command;
use serde_json::Value;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a wheelctl Command, returning Result to avoid unwrap/expect.
fn try_wheelctl() -> Result<Command, Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("wheelctl")?;
    cmd.env_remove("WHEELCTL_ENDPOINT");
    Ok(cmd)
}

fn parse_json(bytes: &[u8]) -> Result<Value, Box<dyn std::error::Error>> {
    let v: Value = serde_json::from_slice(bytes)?;
    Ok(v)
}

// ---------------------------------------------------------------------------
// Help text for every top-level command
// ---------------------------------------------------------------------------

mod help_text {
    use super::*;

    #[test]
    fn root_help_contains_all_subcommands() -> TestResult {
        let output = try_wheelctl()?.arg("--help").output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for subcmd in &[
            "device",
            "profile",
            "plugin",
            "diag",
            "game",
            "telemetry",
            "safety",
            "completion",
            "health",
        ] {
            assert!(
                stdout.contains(subcmd),
                "root --help should mention '{subcmd}'"
            );
        }
        Ok(())
    }

    #[test]
    fn root_help_mentions_json_and_verbose() -> TestResult {
        let output = try_wheelctl()?.arg("--help").output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("--json"), "should mention --json flag");
        assert!(
            stdout.contains("--verbose") || stdout.contains("-v"),
            "should mention verbose flag"
        );
        Ok(())
    }

    #[test]
    fn device_help_lists_all_subcommands() -> TestResult {
        let output = try_wheelctl()?.args(["device", "--help"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for sub in &["list", "status", "calibrate", "reset"] {
            assert!(stdout.contains(sub), "device --help should mention '{sub}'");
        }
        Ok(())
    }

    #[test]
    fn profile_help_lists_all_subcommands() -> TestResult {
        let output = try_wheelctl()?.args(["profile", "--help"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for sub in &[
            "list", "show", "apply", "create", "edit", "validate", "export", "import",
        ] {
            assert!(
                stdout.contains(sub),
                "profile --help should mention '{sub}'"
            );
        }
        Ok(())
    }

    #[test]
    fn plugin_help_lists_all_subcommands() -> TestResult {
        let output = try_wheelctl()?.args(["plugin", "--help"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for sub in &["list", "search", "install", "uninstall", "info", "verify"] {
            assert!(stdout.contains(sub), "plugin --help should mention '{sub}'");
        }
        Ok(())
    }

    #[test]
    fn diag_help_lists_all_subcommands() -> TestResult {
        let output = try_wheelctl()?.args(["diag", "--help"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for sub in &["test", "record", "replay", "support", "metrics"] {
            assert!(stdout.contains(sub), "diag --help should mention '{sub}'");
        }
        Ok(())
    }

    #[test]
    fn game_help_lists_all_subcommands() -> TestResult {
        let output = try_wheelctl()?.args(["game", "--help"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for sub in &["list", "configure", "status", "test"] {
            assert!(stdout.contains(sub), "game --help should mention '{sub}'");
        }
        Ok(())
    }

    #[test]
    fn telemetry_help_lists_all_subcommands() -> TestResult {
        let output = try_wheelctl()?.args(["telemetry", "--help"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for sub in &["probe", "capture"] {
            assert!(
                stdout.contains(sub),
                "telemetry --help should mention '{sub}'"
            );
        }
        Ok(())
    }

    #[test]
    fn safety_help_lists_all_subcommands() -> TestResult {
        let output = try_wheelctl()?.args(["safety", "--help"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for sub in &["enable", "stop", "status", "limit"] {
            assert!(stdout.contains(sub), "safety --help should mention '{sub}'");
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Version output format
// ---------------------------------------------------------------------------

mod version_output {
    use super::*;

    #[test]
    fn version_flag_succeeds() -> TestResult {
        try_wheelctl()?.arg("--version").assert().success();
        Ok(())
    }

    #[test]
    fn version_output_contains_binary_name() -> TestResult {
        let output = try_wheelctl()?.arg("--version").output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("wheelctl"),
            "version output should contain binary name"
        );
        Ok(())
    }

    #[test]
    fn version_output_contains_semver_like_pattern() -> TestResult {
        let output = try_wheelctl()?.arg("--version").output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Should contain at least "X.Y.Z" pattern
        let has_version = stdout.split_whitespace().any(|word| {
            let parts: Vec<&str> = word.split('.').collect();
            parts.len() >= 2 && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()))
        });
        assert!(
            has_version,
            "version output should have semver-like number: {stdout}"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Invalid arguments produce proper error messages
// ---------------------------------------------------------------------------

mod invalid_args {
    use super::*;

    #[test]
    fn no_args_shows_usage() -> TestResult {
        let output = try_wheelctl()?.output()?;
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("Usage") || stderr.contains("usage"),
            "empty args should show usage: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn unknown_command_mentions_input() -> TestResult {
        let output = try_wheelctl()?.arg("xyzzy").output()?;
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("xyzzy") || stderr.contains("invalid"),
            "stderr should reference the bad command: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn unknown_device_subcommand_error() -> TestResult {
        let output = try_wheelctl()?.args(["device", "dance"]).output()?;
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("dance") || stderr.contains("invalid"),
            "stderr should reference unknown subcommand: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn missing_required_arg_device_status() -> TestResult {
        let output = try_wheelctl()?.args(["device", "status"]).output()?;
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("DEVICE") || stderr.contains("required"),
            "should report missing required arg: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn invalid_calibration_type() -> TestResult {
        let output = try_wheelctl()?
            .args(["device", "calibrate", "w1", "backflip"])
            .output()?;
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("invalid value"),
            "should report invalid calibration type: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn non_numeric_torque_limit() -> TestResult {
        let output = try_wheelctl()?
            .args(["safety", "limit", "w1", "abc"])
            .output()?;
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("invalid value"),
            "should report non-numeric torque: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn missing_telemetry_capture_out() -> TestResult {
        let output = try_wheelctl()?
            .args(["telemetry", "capture", "--game", "acc"])
            .output()?;
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("--out"),
            "should report missing --out flag: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn missing_telemetry_probe_game() -> TestResult {
        let output = try_wheelctl()?.args(["telemetry", "probe"]).output()?;
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("--game"),
            "should report missing --game flag: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn invalid_test_type() -> TestResult {
        let output = try_wheelctl()?
            .args(["diag", "test", "--device", "w1", "quantum"])
            .output()?;
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("invalid value") || stderr.contains("quantum"),
            "should report invalid test type: {stderr}"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// JSON output mode for all commands that support it
// ---------------------------------------------------------------------------

mod json_output {
    use super::*;

    /// Commands that should produce valid JSON with --json flag
    const JSON_COMMANDS: &[&[&str]] = &[
        &["device", "list"],
        &["device", "status", "wheel-001"],
        &["device", "list", "--detailed"],
        &["profile", "list"],
        &["game", "list"],
        &["game", "status"],
        &["safety", "status"],
        &["health"],
        &["diag", "metrics"],
        &["plugin", "list"],
    ];

    #[test]
    fn all_json_commands_produce_valid_json() -> TestResult {
        for cmd_args in JSON_COMMANDS {
            let mut args = vec!["--json"];
            args.extend_from_slice(cmd_args);

            let output = try_wheelctl()?.args(&args).output()?;
            assert!(output.status.success(), "command {:?} should succeed", args);

            let json = parse_json(&output.stdout)?;
            assert!(
                json.is_object(),
                "JSON output for {:?} should be an object",
                args
            );
        }
        Ok(())
    }

    #[test]
    fn json_output_has_success_field() -> TestResult {
        for cmd_args in JSON_COMMANDS {
            let mut args = vec!["--json"];
            args.extend_from_slice(cmd_args);

            let output = try_wheelctl()?.args(&args).output()?;
            if output.status.success() {
                let json = parse_json(&output.stdout)?;
                assert_eq!(
                    json.get("success").and_then(Value::as_bool),
                    Some(true),
                    "command {:?} JSON should have success: true",
                    args
                );
            }
        }
        Ok(())
    }

    #[test]
    fn device_list_json_has_devices_array() -> TestResult {
        let output = try_wheelctl()?
            .args(["--json", "device", "list"])
            .output()?;
        let json = parse_json(&output.stdout)?;
        assert!(
            json.get("devices").and_then(Value::as_array).is_some(),
            "should have 'devices' array"
        );
        Ok(())
    }

    #[test]
    fn device_status_json_has_status_object() -> TestResult {
        let output = try_wheelctl()?
            .args(["--json", "device", "status", "wheel-001"])
            .output()?;
        let json = parse_json(&output.stdout)?;
        assert!(json.get("status").is_some(), "should have 'status' field");
        Ok(())
    }

    #[test]
    fn game_status_json_has_game_status() -> TestResult {
        let output = try_wheelctl()?
            .args(["--json", "game", "status"])
            .output()?;
        let json = parse_json(&output.stdout)?;
        assert!(
            json.get("game_status").is_some(),
            "should have 'game_status' field"
        );
        Ok(())
    }

    #[test]
    fn json_flag_works_after_subcommand() -> TestResult {
        let output = try_wheelctl()?
            .args(["device", "list", "--json"])
            .output()?;
        assert!(output.status.success());
        let _json = parse_json(&output.stdout)?;
        Ok(())
    }

    #[test]
    fn json_flag_works_with_detailed() -> TestResult {
        let output = try_wheelctl()?
            .args(["device", "list", "--detailed", "--json"])
            .output()?;
        assert!(output.status.success());
        let json = parse_json(&output.stdout)?;
        assert!(json.get("devices").is_some());
        Ok(())
    }

    #[test]
    fn json_error_output_has_error_field() -> TestResult {
        let output = try_wheelctl()?
            .args(["--json", "device", "status", "nonexistent-device-xyz"])
            .output()?;
        assert!(!output.status.success());
        let json = parse_json(&output.stdout)?;
        assert_eq!(
            json.get("success").and_then(Value::as_bool),
            Some(false),
            "error JSON should have success: false"
        );
        assert!(json.get("error").is_some(), "should have 'error' field");
        Ok(())
    }

    #[test]
    fn json_error_has_message_field() -> TestResult {
        let output = try_wheelctl()?
            .args(["--json", "device", "status", "nonexistent-device-xyz"])
            .output()?;
        let json = parse_json(&output.stdout)?;
        let error_msg = json
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str);
        assert!(error_msg.is_some(), "error should have 'message' field");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Verbose/quiet modes
// ---------------------------------------------------------------------------

mod verbose_quiet {
    use super::*;

    #[test]
    fn single_verbose_flag_accepted() -> TestResult {
        try_wheelctl()?
            .args(["-v", "device", "list"])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn double_verbose_flag_accepted() -> TestResult {
        try_wheelctl()?
            .args(["-vv", "device", "list"])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn triple_verbose_flag_accepted() -> TestResult {
        try_wheelctl()?
            .args(["-vvv", "device", "list"])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn max_verbose_flag_accepted() -> TestResult {
        try_wheelctl()?
            .args(["-vvvv", "device", "list"])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn verbose_long_form_accepted() -> TestResult {
        try_wheelctl()?
            .args(["--verbose", "--verbose", "device", "list"])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn verbose_does_not_change_json_structure() -> TestResult {
        let quiet_output = try_wheelctl()?
            .args(["--json", "device", "list"])
            .output()?;
        let verbose_output = try_wheelctl()?
            .args(["-vvv", "--json", "device", "list"])
            .output()?;

        let quiet_json = parse_json(&quiet_output.stdout)?;
        let verbose_json = parse_json(&verbose_output.stdout)?;

        // Both should have the same top-level keys
        let quiet_keys: Vec<_> = quiet_json
            .as_object()
            .map(|o| o.keys().collect::<Vec<_>>())
            .unwrap_or_default();
        let verbose_keys: Vec<_> = verbose_json
            .as_object()
            .map(|o| o.keys().collect::<Vec<_>>())
            .unwrap_or_default();
        assert_eq!(
            quiet_keys, verbose_keys,
            "JSON structure should match regardless of verbosity"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Command piping compatibility
// ---------------------------------------------------------------------------

mod piping_compatibility {
    use super::*;

    #[test]
    fn stdout_is_valid_utf8() -> TestResult {
        let output = try_wheelctl()?.args(["device", "list"]).output()?;
        let _ = String::from_utf8(output.stdout)?;
        Ok(())
    }

    #[test]
    fn json_output_ends_with_newline() -> TestResult {
        let output = try_wheelctl()?
            .args(["--json", "device", "list"])
            .output()?;
        let stdout = String::from_utf8(output.stdout)?;
        assert!(
            stdout.ends_with('\n'),
            "JSON output should end with newline for piping"
        );
        Ok(())
    }

    #[test]
    fn json_output_is_single_document() -> TestResult {
        let output = try_wheelctl()?
            .args(["--json", "device", "list"])
            .output()?;
        let stdout = String::from_utf8(output.stdout)?;
        let trimmed = stdout.trim();
        // Should parse as exactly one JSON value
        let _: Value = serde_json::from_str(trimmed)?;
        Ok(())
    }

    #[test]
    fn completion_output_is_non_empty() -> TestResult {
        for shell in &["bash", "zsh", "fish", "powershell"] {
            let output = try_wheelctl()?.args(["completion", shell]).output()?;
            assert!(output.status.success());
            assert!(
                !output.stdout.is_empty(),
                "completion for {shell} should produce output"
            );
        }
        Ok(())
    }

    #[test]
    fn help_output_goes_to_stdout() -> TestResult {
        let output = try_wheelctl()?.arg("--help").output()?;
        assert!(output.status.success());
        assert!(!output.stdout.is_empty(), "help should go to stdout");
        Ok(())
    }

    #[test]
    fn error_output_goes_to_stderr_or_stdout_json() -> TestResult {
        // Plain-mode errors go to stderr
        let output = try_wheelctl()?.arg("xyzzy").output()?;
        assert!(!output.status.success());
        assert!(
            !output.stderr.is_empty(),
            "plain error should write to stderr"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Valid output structure for each command category
// ---------------------------------------------------------------------------

mod output_structure {
    use super::*;

    #[test]
    fn device_list_human_mentions_devices_header() -> TestResult {
        let output = try_wheelctl()?.args(["device", "list"]).output()?;
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("Device") || stdout.contains("device"),
            "device list should mention devices: {stdout}"
        );
        Ok(())
    }

    #[test]
    fn device_list_detailed_shows_extra_info() -> TestResult {
        let output = try_wheelctl()?
            .args(["device", "list", "--detailed"])
            .output()?;
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("Max Torque") || stdout.contains("Capabilities"),
            "detailed list should show capabilities: {stdout}"
        );
        Ok(())
    }

    #[test]
    fn game_list_human_shows_games() -> TestResult {
        let output = try_wheelctl()?.args(["game", "list"]).output()?;
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("Game") || stdout.contains("game") || stdout.contains("Supported"),
            "game list should mention games: {stdout}"
        );
        Ok(())
    }

    #[test]
    fn safety_status_human_shows_status() -> TestResult {
        let output = try_wheelctl()?.args(["safety", "status"]).output()?;
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("Safety") || stdout.contains("safety") || stdout.contains("Status"),
            "safety status should show status info: {stdout}"
        );
        Ok(())
    }

    #[test]
    fn health_human_shows_status() -> TestResult {
        let output = try_wheelctl()?.args(["health"]).output()?;
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("Health") || stdout.contains("Status"),
            "health should show status: {stdout}"
        );
        Ok(())
    }

    #[test]
    fn device_status_with_known_id_shows_device_info() -> TestResult {
        let output = try_wheelctl()?
            .args(["device", "status", "wheel-001"])
            .output()?;
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("Device") || stdout.contains("wheel-001"),
            "status should show device info: {stdout}"
        );
        Ok(())
    }
}
