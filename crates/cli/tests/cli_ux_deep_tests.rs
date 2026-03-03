//! Deep CLI user-experience tests for `wheelctl`.
//!
//! These tests exercise the *user-facing* behaviour of the compiled binary:
//! help text quality, error messages, exit codes, output format flags,
//! shell completion generation, colour control, verbose diagnostics,
//! progressive disclosure, and flag consistency.
//!
//! Every test returns `Result` — no `unwrap()` / `expect()`.

#![allow(deprecated)] // cargo_bin deprecation warnings

use assert_cmd::Command;
use predicates::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a `wheelctl` [`Command`] with the service endpoint cleared so that
/// tests do not accidentally talk to a running daemon.
fn wheelctl() -> Result<Command, Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("wheelctl")?;
    cmd.env_remove("WHEELCTL_ENDPOINT");
    Ok(cmd)
}

// ===========================================================================
// 1. Every subcommand has working --help
// ===========================================================================

mod help_works {
    use super::*;

    #[test]
    fn root_help_exits_zero() -> TestResult {
        wheelctl()?.arg("--help").assert().success();
        Ok(())
    }

    #[test]
    fn device_help_exits_zero() -> TestResult {
        wheelctl()?.args(["device", "--help"]).assert().success();
        Ok(())
    }

    #[test]
    fn profile_help_exits_zero() -> TestResult {
        wheelctl()?.args(["profile", "--help"]).assert().success();
        Ok(())
    }

    #[test]
    fn plugin_help_exits_zero() -> TestResult {
        wheelctl()?.args(["plugin", "--help"]).assert().success();
        Ok(())
    }

    #[test]
    fn diag_help_exits_zero() -> TestResult {
        wheelctl()?.args(["diag", "--help"]).assert().success();
        Ok(())
    }

    #[test]
    fn game_help_exits_zero() -> TestResult {
        wheelctl()?.args(["game", "--help"]).assert().success();
        Ok(())
    }

    #[test]
    fn telemetry_help_exits_zero() -> TestResult {
        wheelctl()?
            .args(["telemetry", "--help"])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn safety_help_exits_zero() -> TestResult {
        wheelctl()?.args(["safety", "--help"]).assert().success();
        Ok(())
    }

    #[test]
    fn health_help_exits_zero() -> TestResult {
        wheelctl()?.args(["health", "--help"]).assert().success();
        Ok(())
    }
}

// ===========================================================================
// 2. Help text includes usage information
// ===========================================================================

mod help_text_quality {
    use super::*;

    #[test]
    fn root_help_includes_usage_line() -> TestResult {
        let out = wheelctl()?.arg("--help").output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("Usage:") || s.contains("usage:"),
            "root --help should contain a Usage line: {s}"
        );
        Ok(())
    }

    #[test]
    fn root_help_mentions_json_flag() -> TestResult {
        let out = wheelctl()?.arg("--help").output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("--json"),
            "root --help should document --json: {s}"
        );
        Ok(())
    }

    #[test]
    fn root_help_mentions_verbose_flag() -> TestResult {
        let out = wheelctl()?.arg("--help").output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(
            s.contains("--verbose") || s.contains("-v"),
            "root --help should document -v/--verbose: {s}"
        );
        Ok(())
    }

    #[test]
    fn device_help_lists_subcommands() -> TestResult {
        let out = wheelctl()?.args(["device", "--help"]).output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        for sub in &["list", "status", "calibrate", "reset"] {
            assert!(
                s.contains(sub),
                "device --help should list subcommand '{sub}': {s}"
            );
        }
        Ok(())
    }

    #[test]
    fn profile_help_lists_subcommands() -> TestResult {
        let out = wheelctl()?.args(["profile", "--help"]).output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        for sub in &["list", "show", "apply", "create", "edit", "validate", "export", "import"] {
            assert!(
                s.contains(sub),
                "profile --help should list subcommand '{sub}': {s}"
            );
        }
        Ok(())
    }
}

// ===========================================================================
// 3. Invalid subcommands produce helpful suggestions
// ===========================================================================

mod invalid_subcommands {
    use super::*;

    #[test]
    fn typo_subcommand_suggests_correction() -> TestResult {
        let assert = wheelctl()?.arg("devic").assert().failure();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        // clap typically suggests the correct subcommand or says "not found"
        assert!(
            stderr.contains("device")
                || stderr.to_lowercase().contains("similar")
                || stderr.to_lowercase().contains("did you mean"),
            "typo should produce helpful suggestion in stderr: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn completely_unknown_subcommand_reports_error() -> TestResult {
        wheelctl()?
            .arg("xyzzy")
            .assert()
            .failure()
            .stderr(predicate::str::contains("unrecognized").or(
                predicate::str::contains("not recognized")
                    .or(predicate::str::contains("invalid")),
            ));
        Ok(())
    }

    #[test]
    fn unknown_nested_subcommand_reports_error() -> TestResult {
        let assert = wheelctl()?.args(["device", "frobnicate"]).assert().failure();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            stderr.to_lowercase().contains("invalid")
                || stderr.to_lowercase().contains("unrecognized")
                || stderr.contains("list")
                || stderr.contains("status"),
            "unknown nested subcommand should show available options: {stderr}"
        );
        Ok(())
    }
}

// ===========================================================================
// 4. Required arguments missing → clear error with expected usage
// ===========================================================================

mod missing_required_args {
    use super::*;

    #[test]
    fn device_status_requires_device_arg() -> TestResult {
        let assert = wheelctl()?.args(["device", "status"]).assert().failure();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            stderr.contains("<DEVICE>")
                || stderr.contains("<device>")
                || stderr.to_lowercase().contains("required"),
            "missing device arg should show usage: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn profile_show_requires_profile_arg() -> TestResult {
        let assert = wheelctl()?.args(["profile", "show"]).assert().failure();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            stderr.contains("<PROFILE>")
                || stderr.contains("<profile>")
                || stderr.to_lowercase().contains("required"),
            "missing profile arg should show usage: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn device_calibrate_requires_device_and_type() -> TestResult {
        let assert = wheelctl()?
            .args(["device", "calibrate"])
            .assert()
            .failure();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            stderr.to_lowercase().contains("required")
                || stderr.contains("<DEVICE>")
                || stderr.contains("<device>"),
            "missing calibrate args should explain required args: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn telemetry_capture_requires_game_and_out() -> TestResult {
        let assert = wheelctl()?
            .args(["telemetry", "capture"])
            .assert()
            .failure();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            stderr.to_lowercase().contains("required")
                || stderr.contains("--game")
                || stderr.contains("--out"),
            "telemetry capture should explain required flags: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn safety_limit_requires_device_and_torque() -> TestResult {
        let assert = wheelctl()?.args(["safety", "limit"]).assert().failure();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            stderr.to_lowercase().contains("required")
                || stderr.contains("<DEVICE>")
                || stderr.contains("<TORQUE>"),
            "safety limit should explain required args: {stderr}"
        );
        Ok(())
    }
}

// ===========================================================================
// 5. Output format flags (--json) work from any position
// ===========================================================================

mod output_format_flags {
    use super::*;

    #[test]
    fn json_flag_before_subcommand_accepted() -> TestResult {
        // `--json device list` should parse without error (may fail at runtime
        // connecting to the service, but the parse itself succeeds).
        let assert = wheelctl()?
            .args(["--json", "device", "list"])
            .assert();
        // We only care that clap did not reject the flags — exit may be non-zero
        // because the service is not running, but stderr should NOT be a
        // flag-parsing error.
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            !stderr.contains("unexpected argument"),
            "--json before subcommand should be accepted: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn json_flag_after_subcommand_accepted() -> TestResult {
        let assert = wheelctl()?
            .args(["device", "list", "--json"])
            .assert();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            !stderr.contains("unexpected argument"),
            "--json after subcommand should be accepted: {stderr}"
        );
        Ok(())
    }
}

// ===========================================================================
// 6. Color output can be disabled via NO_COLOR env
// ===========================================================================

mod color_control {
    use super::*;

    #[test]
    fn no_color_env_disables_ansi_in_help() -> TestResult {
        let out = wheelctl()?.env("NO_COLOR", "1").arg("--help").output()?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        // ANSI escape codes start with ESC (0x1B) or \x1b[
        assert!(
            !stdout.contains("\x1b["),
            "NO_COLOR=1 should suppress ANSI escape codes in help output"
        );
        Ok(())
    }

    #[test]
    fn help_without_no_color_still_succeeds() -> TestResult {
        wheelctl()?.env_remove("NO_COLOR").arg("--help").assert().success();
        Ok(())
    }
}

// ===========================================================================
// 7. Version output includes expected information
// ===========================================================================

mod version_output {
    use super::*;

    #[test]
    fn version_flag_exits_zero() -> TestResult {
        wheelctl()?.arg("--version").assert().success();
        Ok(())
    }

    #[test]
    fn version_output_contains_binary_name() -> TestResult {
        let out = wheelctl()?.arg("--version").output()?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("wheelctl"),
            "--version should include binary name: {stdout}"
        );
        Ok(())
    }

    #[test]
    fn version_output_contains_semver() -> TestResult {
        let out = wheelctl()?.arg("--version").output()?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        // Semver: at least MAJOR.MINOR.PATCH
        assert!(
            stdout.contains('.'),
            "--version should contain a dotted version number: {stdout}"
        );
        Ok(())
    }
}

// ===========================================================================
// 8. Verbose flag increases diagnostic output
// ===========================================================================

mod verbose_flag {
    use super::*;

    #[test]
    fn verbose_flag_accepted_globally() -> TestResult {
        // Should not fail on flag parsing; runtime failure is acceptable.
        let assert = wheelctl()?
            .args(["-v", "device", "list"])
            .assert();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            !stderr.contains("unexpected argument"),
            "-v should be accepted globally: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn double_verbose_flag_accepted() -> TestResult {
        let assert = wheelctl()?
            .args(["-vv", "device", "list"])
            .assert();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            !stderr.contains("unexpected argument"),
            "-vv should be accepted: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn triple_verbose_flag_accepted() -> TestResult {
        let assert = wheelctl()?
            .args(["-vvv", "device", "list"])
            .assert();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            !stderr.contains("unexpected argument"),
            "-vvv should be accepted: {stderr}"
        );
        Ok(())
    }
}

// ===========================================================================
// 9. Exit codes are consistent
// ===========================================================================

mod exit_codes {
    use super::*;

    #[test]
    fn help_exits_zero() -> TestResult {
        wheelctl()?.arg("--help").assert().code(0);
        Ok(())
    }

    #[test]
    fn version_exits_zero() -> TestResult {
        wheelctl()?.arg("--version").assert().code(0);
        Ok(())
    }

    #[test]
    fn unknown_flag_exits_nonzero() -> TestResult {
        wheelctl()?
            .arg("--nonexistent-flag")
            .assert()
            .failure();
        Ok(())
    }

    #[test]
    fn missing_required_arg_exits_nonzero() -> TestResult {
        wheelctl()?
            .args(["device", "status"])
            .assert()
            .failure();
        Ok(())
    }
}

// ===========================================================================
// 10. Shell completion generation works for all shells
// ===========================================================================

mod shell_completions {
    use super::*;

    #[test]
    fn bash_completion_produces_output() -> TestResult {
        let out = wheelctl()?.args(["completion", "bash"]).output()?;
        assert!(
            out.status.success(),
            "completion bash should succeed"
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            !stdout.is_empty(),
            "bash completion should produce non-empty output"
        );
        Ok(())
    }

    #[test]
    fn zsh_completion_produces_output() -> TestResult {
        let out = wheelctl()?.args(["completion", "zsh"]).output()?;
        assert!(
            out.status.success(),
            "completion zsh should succeed"
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            !stdout.is_empty(),
            "zsh completion should produce non-empty output"
        );
        Ok(())
    }

    #[test]
    fn fish_completion_produces_output() -> TestResult {
        let out = wheelctl()?.args(["completion", "fish"]).output()?;
        assert!(
            out.status.success(),
            "completion fish should succeed"
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            !stdout.is_empty(),
            "fish completion should produce non-empty output"
        );
        Ok(())
    }

    #[test]
    fn powershell_completion_produces_output() -> TestResult {
        let out = wheelctl()?.args(["completion", "powershell"]).output()?;
        assert!(
            out.status.success(),
            "completion powershell should succeed"
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            !stdout.is_empty(),
            "powershell completion should produce non-empty output"
        );
        Ok(())
    }

    #[test]
    fn invalid_shell_name_is_rejected() -> TestResult {
        wheelctl()?
            .args(["completion", "ksh"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("invalid value").or(
                predicate::str::contains("possible values"),
            ));
        Ok(())
    }

    #[test]
    fn bash_completion_mentions_subcommands() -> TestResult {
        let out = wheelctl()?.args(["completion", "bash"]).output()?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("device") || stdout.contains("wheelctl"),
            "bash completion should reference subcommands or binary name: (len={})",
            stdout.len()
        );
        Ok(())
    }
}

// ===========================================================================
// 11. Configuration file path override via env
// ===========================================================================

mod config_override {
    use super::*;

    #[test]
    fn endpoint_env_is_accepted() -> TestResult {
        // WHEELCTL_ENDPOINT env should be recognised without a parse error.
        let assert = wheelctl()?
            .env("WHEELCTL_ENDPOINT", "http://localhost:9999")
            .args(["device", "list"])
            .assert();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            !stderr.contains("unexpected argument"),
            "WHEELCTL_ENDPOINT env should be accepted: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn endpoint_flag_is_accepted() -> TestResult {
        let assert = wheelctl()?
            .args(["--endpoint", "http://localhost:9999", "device", "list"])
            .assert();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            !stderr.contains("unexpected argument"),
            "--endpoint flag should be accepted: {stderr}"
        );
        Ok(())
    }
}

// ===========================================================================
// 12. Progressive disclosure: simple commands are short, advanced flags exist
// ===========================================================================

mod progressive_disclosure {
    use super::*;

    #[test]
    fn device_list_works_with_zero_flags() -> TestResult {
        // Simplest form: `wheelctl device list` — should not require any flags.
        let assert = wheelctl()?
            .args(["device", "list"])
            .assert();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            !stderr.to_lowercase().contains("required"),
            "device list should work with no extra flags: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn profile_list_works_with_zero_flags() -> TestResult {
        let assert = wheelctl()?
            .args(["profile", "list"])
            .assert();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            !stderr.to_lowercase().contains("required"),
            "profile list should work with no extra flags: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn device_list_accepts_detailed_flag() -> TestResult {
        let assert = wheelctl()?
            .args(["device", "list", "--detailed"])
            .assert();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            !stderr.contains("unexpected argument"),
            "--detailed should be accepted for device list: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn global_flags_documented_at_root_level() -> TestResult {
        let out = wheelctl()?.arg("--help").output()?;
        let s = String::from_utf8_lossy(&out.stdout);
        // Global flags should be visible in root help
        assert!(s.contains("--json"), "root help should show --json");
        assert!(
            s.contains("--verbose") || s.contains("-v"),
            "root help should show -v/--verbose"
        );
        Ok(())
    }
}

// ===========================================================================
// 13. Telemetry probe/capture flag validation
// ===========================================================================

mod telemetry_flag_validation {
    use super::*;

    #[test]
    fn telemetry_probe_requires_game_flag() -> TestResult {
        let assert = wheelctl()?
            .args(["telemetry", "probe"])
            .assert()
            .failure();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            stderr.contains("--game") || stderr.to_lowercase().contains("required"),
            "telemetry probe should require --game: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn telemetry_capture_requires_out_flag() -> TestResult {
        let assert = wheelctl()?
            .args(["telemetry", "capture", "--game", "iracing"])
            .assert()
            .failure();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            stderr.contains("--out") || stderr.to_lowercase().contains("required"),
            "telemetry capture should require --out: {stderr}"
        );
        Ok(())
    }
}

// ===========================================================================
// 14. Plugin subcommand argument validation
// ===========================================================================

mod plugin_arg_validation {
    use super::*;

    #[test]
    fn plugin_install_requires_plugin_id() -> TestResult {
        let assert = wheelctl()?
            .args(["plugin", "install"])
            .assert()
            .failure();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            stderr.to_lowercase().contains("required")
                || stderr.contains("<PLUGIN_ID>")
                || stderr.contains("<plugin_id>"),
            "plugin install should require plugin_id: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn plugin_search_requires_query() -> TestResult {
        let assert = wheelctl()?
            .args(["plugin", "search"])
            .assert()
            .failure();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            stderr.to_lowercase().contains("required")
                || stderr.contains("<QUERY>")
                || stderr.contains("<query>"),
            "plugin search should require query: {stderr}"
        );
        Ok(())
    }
}

// ===========================================================================
// 15. Safety subcommand flag consistency
// ===========================================================================

mod safety_flag_consistency {
    use super::*;

    #[test]
    fn safety_enable_requires_device() -> TestResult {
        let assert = wheelctl()?
            .args(["safety", "enable"])
            .assert()
            .failure();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            stderr.to_lowercase().contains("required")
                || stderr.contains("<DEVICE>")
                || stderr.contains("<device>"),
            "safety enable should require device: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn safety_stop_works_without_device() -> TestResult {
        // `safety stop` should accept no device (means "all devices").
        let assert = wheelctl()?
            .args(["safety", "stop"])
            .assert();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            !stderr.to_lowercase().contains("required"),
            "safety stop should work without specifying a device: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn safety_enable_accepts_force_flag() -> TestResult {
        let assert = wheelctl()?
            .args(["safety", "enable", "wheel-001", "--force"])
            .assert();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(
            !stderr.contains("unexpected argument"),
            "--force should be accepted for safety enable: {stderr}"
        );
        Ok(())
    }
}
