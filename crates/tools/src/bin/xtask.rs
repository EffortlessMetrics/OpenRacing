//! Repository automation entry point for `cargo xtask`.
//!
//! This binary intentionally keeps public badge endpoints repo-scoped and PR
//! evidence diff-scoped. Generated public endpoint JSON is committed under
//! `badges/`; detailed reports stay under `target/` or CI artifacts.

#![deny(static_mut_refs)]
#![deny(unused_must_use)]

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const BADGE_ENDPOINT_DIR: &str = "badges";
const BADGE_ENDPOINT_TARGET_DIR: &str = "target/xtask/badges";
const RIPR_PR_DIR: &str = "target/ripr/pr";
const RIPR_REVIEW_DIR: &str = "target/ripr/review";

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
struct ShieldsEndpointBadge {
    #[serde(rename = "schemaVersion")]
    schema_version: u8,
    label: String,
    message: String,
    color: String,
}

#[derive(Debug, PartialEq, Eq)]
enum CommandKind {
    Badges { check: bool },
    RiprPr { check: bool },
    RiprReviewComments { check: bool },
    CheckFilePolicy,
    DocsSync { check: bool },
    Pr,
    Help,
}

fn main() -> ExitCode {
    match run(env::args_os()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            stderr_line(format_args!("xtask: {error}"));
            ExitCode::FAILURE
        }
    }
}

fn run(args: impl IntoIterator<Item = OsString>) -> Result<(), String> {
    match parse_args(args)? {
        CommandKind::Badges { check } => badges(check),
        CommandKind::RiprPr { check } => ripr_pr(check),
        CommandKind::RiprReviewComments { check } => ripr_review_comments(check),
        CommandKind::CheckFilePolicy => run_status_command(
            "python",
            ["scripts/policy_file.py"],
            &workspace_root_path()?,
            "file policy check",
        ),
        CommandKind::DocsSync { check } => {
            let mut args = vec![
                "run",
                "-p",
                "openracing-tools",
                "--bin",
                "yaml-sync-check",
                "--",
            ];
            if check {
                args.push("--check");
            }
            run_status_command("cargo", args, &workspace_root_path()?, "docs sync check")
        }
        CommandKind::Pr => pr_gate(),
        CommandKind::Help => {
            stdout_line(format_args!("{}", usage()));
            Ok(())
        }
    }
}

fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<CommandKind, String> {
    let args = args
        .into_iter()
        .skip(1)
        .map(|arg| {
            arg.into_string()
                .map_err(|_| "arguments must be valid UTF-8".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;

    let Some(command) = args.first().map(String::as_str) else {
        return Ok(CommandKind::Help);
    };

    let check = args.iter().skip(1).any(|arg| arg == "--check");
    if args.iter().skip(1).any(|arg| arg != "--check") {
        return Err(format!("unknown option for `{command}`\n{}", usage()));
    }

    match command {
        "badges" => Ok(CommandKind::Badges { check }),
        "ripr-pr" => Ok(CommandKind::RiprPr { check }),
        "ripr-review-comments" => Ok(CommandKind::RiprReviewComments { check }),
        "check-file-policy" => Ok(CommandKind::CheckFilePolicy),
        "docs-sync" => Ok(CommandKind::DocsSync { check }),
        "pr" => Ok(CommandKind::Pr),
        "-h" | "--help" | "help" => Ok(CommandKind::Help),
        other => Err(format!("unknown command `{other}`\n{}", usage())),
    }
}

fn usage() -> &'static str {
    "Usage: cargo xtask <command> [--check]\n\nCommands:\n  badges                 Generate public Shields endpoint JSON\n  badges --check         Verify committed badge endpoint drift\n  ripr-pr                Produce PR-scoped RIPR exposure evidence\n  ripr-pr --check        Verify RIPR PR evidence output contract\n  ripr-review-comments   Produce RIPR review guidance\n  ripr-review-comments --check\n                         Verify RIPR review guidance output contract\n  docs-sync --check      Verify duplicated generated docs/data stay synchronized\n  check-file-policy      Validate non-Rust file policy\n  pr                     Run the fast local PR gate"
}

fn badges(check: bool) -> Result<(), String> {
    let workspace_root = workspace_root_path()?;
    let target_dir = workspace_root.join(BADGE_ENDPOINT_TARGET_DIR);
    fs::create_dir_all(&target_dir)
        .map_err(|error| format!("failed to create {}: {error}", target_dir.display()))?;

    let ripr_plus = ripr_plus_badge(&workspace_root)?;
    validate_shields_badge(&ripr_plus, Some("ripr+"))?;
    write_json_pretty(&target_dir.join("ripr-plus.json"), &ripr_plus)?;

    if check {
        let committed = workspace_root
            .join(BADGE_ENDPOINT_DIR)
            .join("ripr-plus.json");
        let generated = target_dir.join("ripr-plus.json");
        compare_files(&committed, &generated)?;
        stdout_line(format_args!("badges: committed endpoints are current"));
        return Ok(());
    }

    let committed_dir = workspace_root.join(BADGE_ENDPOINT_DIR);
    fs::create_dir_all(&committed_dir)
        .map_err(|error| format!("failed to create {}: {error}", committed_dir.display()))?;
    fs::copy(
        target_dir.join("ripr-plus.json"),
        committed_dir.join("ripr-plus.json"),
    )
    .map_err(|error| format!("failed to refresh ripr-plus.json: {error}"))?;
    stdout_line(format_args!(
        "badges: refreshed public endpoint JSON under badges/"
    ));
    Ok(())
}

fn ripr_plus_badge(workspace_root: &Path) -> Result<ShieldsEndpointBadge, String> {
    let ripr_bin = env::var("RIPR_BIN").unwrap_or_else(|_| "ripr".to_string());
    let output = Command::new(&ripr_bin)
        .arg("check")
        .arg("--root")
        .arg(workspace_root)
        .arg("--format")
        .arg("repo-badge-plus-shields")
        .current_dir(workspace_root)
        .output()
        .map_err(|error| format!("failed to run {ripr_bin}: {error}"))?;

    if !output.status.success() {
        return Err(format!(
            "{ripr_bin} repo-badge-plus-shields failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("{ripr_bin} emitted invalid JSON: {error}"))?;
    validate_shields_value_shape(&value)?;
    serde_json::from_value(value)
        .map_err(|error| format!("{ripr_bin} emitted invalid Shields endpoint JSON: {error}"))
}

fn validate_shields_badge(
    badge: &ShieldsEndpointBadge,
    expected_label: Option<&str>,
) -> Result<(), String> {
    if badge.schema_version != 1 {
        return Err(format!(
            "badge `{}` has unsupported schemaVersion",
            badge.label
        ));
    }

    if let Some(expected_label) = expected_label
        && badge.label != expected_label
    {
        return Err(format!(
            "badge label drifted: got `{}`, expected `{expected_label}`",
            badge.label
        ));
    }

    if badge.message.trim().is_empty() {
        return Err(format!("badge `{}` has empty message", badge.label));
    }

    if badge.color.trim().is_empty() {
        return Err(format!("badge `{}` has empty color", badge.label));
    }

    Ok(())
}

fn validate_shields_value_shape(value: &Value) -> Result<(), String> {
    let Some(object) = value.as_object() else {
        return Err("badge endpoint JSON must be an object".to_string());
    };
    let expected = BTreeSet::from(["schemaVersion", "label", "message", "color"]);
    let actual = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(format!(
            "badge endpoint JSON keys drifted: got {actual:?}, expected {expected:?}"
        ));
    }
    Ok(())
}

fn ripr_pr(check: bool) -> Result<(), String> {
    let workspace_root = workspace_root_path()?;
    let out_dir = workspace_root.join(RIPR_PR_DIR);
    if check {
        verify_json_file(&out_dir.join("repo-exposure.json"))?;
        verify_nonempty_file(&out_dir.join("repo-exposure.md"))?;
        stdout_line(format_args!("ripr-pr: output contract is valid"));
        return Ok(());
    }

    fs::create_dir_all(&out_dir)
        .map_err(|error| format!("failed to create {}: {error}", out_dir.display()))?;
    let ripr_bin = env::var("RIPR_BIN").unwrap_or_else(|_| "ripr".to_string());
    run_capture_command(
        Command::new(&ripr_bin)
            .arg("check")
            .arg("--root")
            .arg(&workspace_root)
            .arg("--format")
            .arg("repo-exposure-json")
            .current_dir(&workspace_root),
        &out_dir.join("repo-exposure.json"),
        "RIPR repo exposure JSON",
    )?;
    verify_json_file(&out_dir.join("repo-exposure.json"))?;
    run_capture_command(
        Command::new(&ripr_bin)
            .arg("check")
            .arg("--root")
            .arg(&workspace_root)
            .arg("--format")
            .arg("repo-exposure-md")
            .current_dir(&workspace_root),
        &out_dir.join("repo-exposure.md"),
        "RIPR repo exposure Markdown",
    )?;
    verify_nonempty_file(&out_dir.join("repo-exposure.md"))?;
    stdout_line(format_args!("ripr-pr: wrote evidence under target/ripr/pr"));
    Ok(())
}

fn ripr_review_comments(check: bool) -> Result<(), String> {
    let workspace_root = workspace_root_path()?;
    let out_dir = workspace_root.join(RIPR_REVIEW_DIR);
    let json_path = out_dir.join("comments.json");
    let markdown_path = out_dir.join("comments.md");
    if check {
        verify_json_file(&json_path)?;
        verify_nonempty_file(&markdown_path)?;
        stdout_line(format_args!(
            "ripr-review-comments: output contract is valid"
        ));
        return Ok(());
    }

    fs::create_dir_all(&out_dir)
        .map_err(|error| format!("failed to create {}: {error}", out_dir.display()))?;
    let ripr_bin = env::var("RIPR_BIN").unwrap_or_else(|_| "ripr".to_string());
    let base = env::var("RIPR_BASE").unwrap_or_else(|_| "origin/main".to_string());
    let head = env::var("RIPR_HEAD").unwrap_or_else(|_| "HEAD".to_string());
    run_status_command_with(
        Command::new(&ripr_bin)
            .arg("review-comments")
            .arg("--root")
            .arg(&workspace_root)
            .arg("--base")
            .arg(&base)
            .arg("--head")
            .arg(&head)
            .arg("--out")
            .arg(&json_path)
            .current_dir(&workspace_root),
        "RIPR review comments",
    )?;
    verify_json_file(&json_path)?;
    verify_nonempty_file(&markdown_path)?;
    stdout_line(format_args!(
        "ripr-review-comments: wrote guidance under target/ripr/review"
    ));
    Ok(())
}

fn pr_gate() -> Result<(), String> {
    let root = workspace_root_path()?;
    run_status_command("git", ["diff", "--check"], &root, "whitespace check")?;
    run_status_command(
        "cargo",
        ["xtask", "badges", "--check"],
        &root,
        "badge check",
    )?;
    run_status_command(
        "cargo",
        ["xtask", "check-file-policy"],
        &root,
        "file policy check",
    )?;
    run_status_command(
        "cargo",
        ["xtask", "docs-sync", "--check"],
        &root,
        "docs sync check",
    )
}

fn run_capture_command(
    command: &mut Command,
    output_path: &Path,
    label: &str,
) -> Result<(), String> {
    let output = command
        .output()
        .map_err(|error| format!("failed to run {label}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{label} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    fs::write(output_path, output.stdout)
        .map_err(|error| format!("failed to write {}: {error}", output_path.display()))
}

fn run_status_command<I, S>(program: &str, args: I, cwd: &Path, label: &str) -> Result<(), String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    run_status_command_with(Command::new(program).args(args).current_dir(cwd), label)
}

fn run_status_command_with(command: &mut Command, label: &str) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|error| format!("failed to run {label}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{label} failed with status {status}"))
    }
}

fn workspace_root_path() -> Result<PathBuf, String> {
    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--no-deps")
        .arg("--format-version")
        .arg("1")
        .output()
        .map_err(|error| format!("failed to run cargo metadata: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("cargo metadata emitted invalid JSON: {error}"))?;
    let root = value
        .get("workspace_root")
        .and_then(Value::as_str)
        .ok_or_else(|| "cargo metadata did not include workspace_root".to_string())?;
    Ok(PathBuf::from(root))
}

fn write_json_pretty(path: &Path, badge: &ShieldsEndpointBadge) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(badge)
        .map_err(|error| format!("failed to serialize {}: {error}", path.display()))?;
    let mut with_newline = bytes;
    with_newline.push(b'\n');
    fs::write(path, with_newline)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn compare_files(committed: &Path, generated: &Path) -> Result<(), String> {
    let committed_bytes = fs::read(committed)
        .map_err(|error| format!("failed to read {}: {error}", committed.display()))?;
    let generated_bytes = fs::read(generated)
        .map_err(|error| format!("failed to read {}: {error}", generated.display()))?;
    if committed_bytes == generated_bytes {
        Ok(())
    } else {
        Err(format!(
            "{} is out of date; run `cargo xtask badges`",
            committed.display()
        ))
    }
}

fn verify_json_file(path: &Path) -> Result<(), String> {
    let bytes =
        fs::read(path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    if bytes.is_empty() {
        return Err(format!("{} is empty", path.display()));
    }
    serde_json::from_slice::<Value>(&bytes)
        .map_err(|error| format!("{} contains invalid JSON: {error}", path.display()))?;
    Ok(())
}

fn verify_nonempty_file(path: &Path) -> Result<(), String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("failed to stat {}: {error}", path.display()))?;
    if metadata.len() == 0 {
        Err(format!("{} is empty", path.display()))
    } else {
        Ok(())
    }
}

fn stdout_line(args: std::fmt::Arguments<'_>) {
    let mut stdout = io::stdout().lock();
    let _ = writeln!(stdout, "{args}");
}

fn stderr_line(args: std::fmt::Arguments<'_>) {
    let mut stderr = io::stderr().lock();
    let _ = writeln!(stderr, "{args}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ripr_plus_badge_shape_is_stable() -> Result<(), String> {
        let badge = ShieldsEndpointBadge {
            schema_version: 1,
            label: "ripr+".to_string(),
            message: "0".to_string(),
            color: "brightgreen".to_string(),
        };

        validate_shields_badge(&badge, Some("ripr+"))
    }

    #[test]
    fn shields_endpoint_rejects_extra_keys() -> Result<(), String> {
        let value = serde_json::json!({
            "schemaVersion": 1,
            "label": "ripr+",
            "message": "0",
            "color": "brightgreen",
            "extra": true
        });

        if validate_shields_value_shape(&value).is_err() {
            Ok(())
        } else {
            Err("extra endpoint keys should be rejected".to_string())
        }
    }

    #[test]
    fn parse_badges_check() -> Result<(), String> {
        let parsed = parse_args(["xtask", "badges", "--check"].map(OsString::from))?;
        if parsed == (CommandKind::Badges { check: true }) {
            Ok(())
        } else {
            Err(format!("unexpected parse result: {parsed:?}"))
        }
    }
}
