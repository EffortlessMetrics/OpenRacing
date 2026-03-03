//! Output format tests for wheelctl CLI.
//!
//! Covers: JSON output mode, table output formatting, verbose mode,
//! color output control (NO_COLOR env), and piped output behaviour.

#![allow(deprecated)]

use assert_cmd::Command;
use serde_json::Value;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn wheelctl() -> Result<Command, Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("wheelctl")?;
    cmd.env_remove("WHEELCTL_ENDPOINT");
    Ok(cmd)
}

// ===========================================================================
// 1. JSON output mode
// ===========================================================================

mod json_output {
    use super::*;

    #[test]
    fn device_list_json_is_valid() -> TestResult {
        let output = wheelctl()?.args(["--json", "device", "list"]).output()?;
        assert!(output.status.success(), "device list --json should succeed");
        let v: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(v["success"], Value::Bool(true));
        assert!(v["devices"].is_array(), "devices should be an array");
        Ok(())
    }

    #[test]
    fn device_list_json_has_device_fields() -> TestResult {
        let output = wheelctl()?.args(["--json", "device", "list"]).output()?;
        let v: Value = serde_json::from_slice(&output.stdout)?;
        if let Some(arr) = v["devices"].as_array()
            && let Some(first) = arr.first()
        {
            assert!(first["id"].is_string(), "device should have an id");
            assert!(first["name"].is_string(), "device should have a name");
        }
        Ok(())
    }

    #[test]
    fn profile_list_json_is_valid() -> TestResult {
        let output = wheelctl()?.args(["--json", "profile", "list"]).output()?;
        assert!(output.status.success());
        let v: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(v["success"], Value::Bool(true));
        Ok(())
    }

    #[test]
    fn game_list_json_is_valid() -> TestResult {
        let output = wheelctl()?.args(["--json", "game", "list"]).output()?;
        assert!(output.status.success());
        let v: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(v["success"], Value::Bool(true));
        Ok(())
    }

    #[test]
    fn plugin_list_json_is_valid() -> TestResult {
        let output = wheelctl()?.args(["--json", "plugin", "list"]).output()?;
        assert!(output.status.success());
        let v: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(v["success"], Value::Bool(true));
        Ok(())
    }

    #[test]
    fn diag_metrics_json_is_valid() -> TestResult {
        let output = wheelctl()?.args(["--json", "diag", "metrics"]).output()?;
        assert!(output.status.success());
        let v: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(v["success"], Value::Bool(true));
        Ok(())
    }

    #[test]
    fn health_json_is_valid() -> TestResult {
        let output = wheelctl()?.args(["--json", "health"]).output()?;
        assert!(output.status.success());
        let v: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(v["success"], Value::Bool(true));
        Ok(())
    }

    #[test]
    fn safety_status_json_is_valid() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "safety", "status"])
            .output()?;
        assert!(output.status.success());
        let v: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(v["success"], Value::Bool(true));
        Ok(())
    }

    #[test]
    fn game_status_json_is_valid() -> TestResult {
        let output = wheelctl()?
            .args(["--json", "game", "status"])
            .output()?;
        assert!(output.status.success());
        let v: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(v["success"], Value::Bool(true));
        Ok(())
    }

    #[test]
    fn json_flag_position_after_subcommand_is_accepted() -> TestResult {
        // --json is a global flag, so it should work in any position
        let output = wheelctl()?
            .args(["device", "--json", "list"])
            .output()?;
        assert!(
            output.status.success(),
            "global --json flag should be accepted after subcommand"
        );
        let v: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(v["success"], Value::Bool(true));
        Ok(())
    }
}

// ===========================================================================
// 2. Table / human-readable output
// ===========================================================================

mod human_output {
    use super::*;

    #[test]
    fn device_list_without_json_is_human_readable() -> TestResult {
        let output = wheelctl()?.args(["device", "list"]).output()?;
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Human output should NOT be valid JSON
        let json_parse: Result<Value, _> = serde_json::from_str(&stdout);
        assert!(
            json_parse.is_err(),
            "human output should not be valid JSON: {stdout}"
        );
        Ok(())
    }

    #[test]
    fn profile_list_human_output_exists() -> TestResult {
        let output = wheelctl()?.args(["profile", "list"]).output()?;
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(!stdout.is_empty(), "human output should not be empty");
        Ok(())
    }

    #[test]
    fn game_list_human_output_exists() -> TestResult {
        let output = wheelctl()?.args(["game", "list"]).output()?;
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(!stdout.is_empty(), "game list human output should not be empty");
        Ok(())
    }
}

// ===========================================================================
// 3. Verbose mode
// ===========================================================================

mod verbose_mode {
    use super::*;

    #[test]
    fn single_verbose_flag_accepted() -> TestResult {
        wheelctl()?
            .args(["-v", "device", "list"])
            .assert()
            .success();
        Ok(())
    }

    #[test]
    fn double_verbose_flag_accepted() -> TestResult {
        wheelctl()?
            .args(["-vv", "device", "list"])
            .assert()
            .success();
        Ok(())
    }
}

// ===========================================================================
// 4. NO_COLOR env suppresses colored output
// ===========================================================================

mod color_control {
    use super::*;

    #[test]
    fn no_color_env_suppresses_ansi_in_device_list() -> TestResult {
        let output = wheelctl()?
            .env("NO_COLOR", "1")
            .args(["device", "list"])
            .output()?;
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        // ANSI escape sequences start with \x1b[
        assert!(
            !stdout.contains("\x1b["),
            "NO_COLOR=1 should suppress ANSI escapes: {stdout}"
        );
        Ok(())
    }

    #[test]
    fn piped_output_suppresses_ansi() -> TestResult {
        // assert_cmd captures stdout, which means it's a pipe; the `colored`
        // crate disables color when stdout is not a TTY.
        let output = wheelctl()?.args(["device", "list"]).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        // When output is captured (non-TTY), colored crate usually omits ANSI
        assert!(
            !stdout.contains("\x1b["),
            "piped output should have no ANSI escapes: {stdout}"
        );
        Ok(())
    }
}
