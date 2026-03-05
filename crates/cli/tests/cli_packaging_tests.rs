//! CLI packaging validation tests.
//!
//! Verifies that the wheelctl binary produces correctly formatted help output,
//! version information matching workspace metadata, and expected exit codes
//! for packaging and installer sanity checks.

#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn wheelctl() -> Result<Command, Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("wheelctl")?;
    cmd.env_remove("WHEELCTL_ENDPOINT");
    Ok(cmd)
}

// =========================================================================
// 1. Help output format
// =========================================================================

#[test]
fn help_flag_exits_zero() -> TestResult {
    wheelctl()?.arg("--help").assert().success();
    Ok(())
}

#[test]
fn help_output_contains_binary_name() -> TestResult {
    wheelctl()?
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("wheelctl"));
    Ok(())
}

#[test]
fn help_output_contains_usage_line() -> TestResult {
    wheelctl()?
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage:").or(predicate::str::contains("usage:")));
    Ok(())
}

#[test]
fn help_output_lists_subcommands_section() -> TestResult {
    wheelctl()?
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Commands:").or(predicate::str::contains("SUBCOMMANDS:")));
    Ok(())
}

#[test]
fn help_output_mentions_json_flag() -> TestResult {
    wheelctl()?
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--json"));
    Ok(())
}

// =========================================================================
// 2. Version output
// =========================================================================

#[test]
fn version_flag_exits_zero() -> TestResult {
    wheelctl()?.arg("--version").assert().success();
    Ok(())
}

#[test]
fn version_output_contains_semver_pattern() -> TestResult {
    let output = wheelctl()?.arg("--version").output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Check for N.N.N pattern manually
    let has_semver = stdout.split('.').count() >= 3 && stdout.chars().any(|c| c.is_ascii_digit());
    assert!(
        has_semver,
        "Version output should contain semver pattern, got: {stdout}"
    );
    Ok(())
}

#[test]
fn version_output_matches_workspace_version() -> TestResult {
    let output = wheelctl()?.arg("--version").output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Read workspace version from Cargo.toml
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .ok_or("cannot find repo root")?;
    let cargo_toml = std::fs::read_to_string(root.join("Cargo.toml"))?;

    // Parse version manually: look for `version = "X.Y.Z"`
    let workspace_version = cargo_toml
        .lines()
        .find(|l| l.trim().starts_with("version") && l.contains('=') && l.contains('"'))
        .and_then(|l| {
            let start = l.find('"')? + 1;
            let end = l[start..].find('"')? + start;
            Some(&l[start..end])
        })
        .ok_or("No version found in root Cargo.toml")?;

    assert!(
        stdout.contains(workspace_version),
        "wheelctl --version output '{stdout}' should contain workspace version '{workspace_version}'"
    );
    Ok(())
}

// =========================================================================
// 3. Exit codes for invalid usage
// =========================================================================

#[test]
fn no_args_exits_nonzero() -> TestResult {
    wheelctl()?.assert().failure();
    Ok(())
}

#[test]
fn unknown_subcommand_exits_nonzero() -> TestResult {
    wheelctl()?
        .arg("nonexistent-command-xyz")
        .assert()
        .failure();
    Ok(())
}

#[test]
fn no_args_shows_usage_hint() -> TestResult {
    wheelctl()?
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage").or(predicate::str::contains("usage")));
    Ok(())
}

// =========================================================================
// 4. Key subcommands are registered
// =========================================================================

#[test]
fn help_lists_device_subcommand() -> TestResult {
    wheelctl()?
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("device"));
    Ok(())
}

#[test]
fn help_lists_profile_subcommand() -> TestResult {
    wheelctl()?
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("profile"));
    Ok(())
}

#[test]
fn help_lists_status_or_diag_subcommand() -> TestResult {
    wheelctl()?.arg("--help").assert().success().stdout(
        predicate::str::contains("status")
            .or(predicate::str::contains("diag"))
            .or(predicate::str::contains("diagnostic")),
    );
    Ok(())
}
