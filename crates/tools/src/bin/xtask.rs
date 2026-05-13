//! Repository maintenance entrypoint for `cargo xtask`.

#![deny(static_mut_refs)]
#![deny(unused_must_use)]

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::env;
use std::ffi::OsStr;
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

#[derive(Debug, Eq, PartialEq)]
struct Args {
    command: XtaskCommand,
}

#[derive(Debug, Eq, PartialEq)]
enum XtaskCommand {
    Badges { check: bool },
    RiprPr { check: bool },
    RiprReviewComments { check: bool },
    DocsSync { check: bool },
    CheckFilePolicy,
    Pr,
    Help,
}

fn main() -> ExitCode {
    match parse_args(env::args()).and_then(|args| run(args, &workspace_root_path())) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            stderr_line(format_args!("ERROR: {error}"));
            ExitCode::from(1)
        }
    }
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut iter = args.into_iter().skip(1);
    let command = match iter.next().as_deref() {
        Some("badges") => XtaskCommand::Badges {
            check: parse_check_flag(iter)?,
        },
        Some("ripr-pr") => XtaskCommand::RiprPr {
            check: parse_check_flag(iter)?,
        },
        Some("ripr-review-comments") => XtaskCommand::RiprReviewComments {
            check: parse_check_flag(iter)?,
        },
        Some("docs-sync") => XtaskCommand::DocsSync {
            check: parse_check_flag(iter)?,
        },
        Some("check-file-policy") => {
            reject_extra_args(iter)?;
            XtaskCommand::CheckFilePolicy
        }
        Some("pr") => {
            reject_extra_args(iter)?;
            XtaskCommand::Pr
        }
        Some("-h" | "--help") | None => XtaskCommand::Help,
        Some(other) => return Err(format!("unknown xtask command `{other}`\n{}", usage())),
    };

    Ok(Args { command })
}

fn parse_check_flag(iter: impl Iterator<Item = String>) -> Result<bool, String> {
    let mut check = false;
    for arg in iter {
        match arg.as_str() {
            "--check" => check = true,
            "-h" | "--help" => return Err(usage()),
            other => return Err(format!("unknown argument `{other}`\n{}", usage())),
        }
    }
    Ok(check)
}

fn reject_extra_args(mut iter: impl Iterator<Item = String>) -> Result<(), String> {
    if let Some(arg) = iter.next() {
        return Err(format!("unexpected argument `{arg}`\n{}", usage()));
    }
    Ok(())
}

fn usage() -> String {
    "Usage: cargo xtask <badges|ripr-pr|ripr-review-comments|docs-sync|check-file-policy|pr> [--check]".to_string()
}

fn run(args: Args, workspace_root: &Path) -> Result<(), String> {
    match args.command {
        XtaskCommand::Badges { check } => badges(workspace_root, check),
        XtaskCommand::RiprPr { check } => ripr_pr(workspace_root, check),
        XtaskCommand::RiprReviewComments { check } => ripr_review_comments(workspace_root, check),
        XtaskCommand::DocsSync { check } => docs_sync(workspace_root, check),
        XtaskCommand::CheckFilePolicy => check_file_policy(workspace_root),
        XtaskCommand::Pr => pr_fast_gate(workspace_root),
        XtaskCommand::Help => {
            stdout_line(format_args!("{}", usage()));
            Ok(())
        }
    }
}

fn workspace_root_path() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or(manifest_dir)
}

fn badges(workspace_root: &Path, check: bool) -> Result<(), String> {
    let target_dir = workspace_root.join(BADGE_ENDPOINT_TARGET_DIR);
    fs::create_dir_all(&target_dir)
        .map_err(|error| format!("failed to create {}: {error}", target_dir.display()))?;

    let ripr_plus = ripr_plus_badge(workspace_root)?;
    validate_shields_badge(&ripr_plus, Some("ripr+"))?;
    write_json_pretty(&target_dir.join("ripr-plus.json"), &ripr_plus)?;

    if check {
        compare_files(
            &workspace_root
                .join(BADGE_ENDPOINT_DIR)
                .join("ripr-plus.json"),
            &target_dir.join("ripr-plus.json"),
        )?;
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
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("test-efficiency.json") {
            return Ok(ShieldsEndpointBadge {
                schema_version: 1,
                label: "ripr+".to_string(),
                message: "test-efficiency-missing".to_string(),
                color: "yellow".to_string(),
            });
        }

        return Err(format!(
            "{ripr_bin} repo-badge-plus-shields failed: {stderr}"
        ));
    }

    serde_json::from_slice(&output.stdout)
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

    if let Some(expected_label) = expected_label {
        if badge.label != expected_label {
            return Err(format!(
                "badge label drifted: got `{}`, expected `{expected_label}`",
                badge.label
            ));
        }
    }

    if badge.message.trim().is_empty() {
        return Err(format!("badge `{}` has empty message", badge.label));
    }

    if badge.color.trim().is_empty() {
        return Err(format!("badge `{}` has empty color", badge.label));
    }

    Ok(())
}

fn ripr_pr(workspace_root: &Path, check: bool) -> Result<(), String> {
    let output_dir = workspace_root.join(RIPR_PR_DIR);
    if check {
        validate_ripr_pr_contract(&output_dir)?;
        stdout_line(format_args!("ripr-pr: output contract is valid"));
        return Ok(());
    }

    fs::create_dir_all(&output_dir)
        .map_err(|error| format!("failed to create {}: {error}", output_dir.display()))?;
    let json_path = output_dir.join("repo-exposure.json");
    let md_path = output_dir.join("repo-exposure.md");
    let ripr_bin = env::var("RIPR_BIN").unwrap_or_else(|_| "ripr".to_string());

    let mut used_ripr = false;
    if command_exists(&ripr_bin, workspace_root) && env::var("CI").is_ok() {
        let json_output = Command::new(&ripr_bin)
            .arg("check")
            .arg("--root")
            .arg(".")
            .arg("--format")
            .arg("repo-exposure-json")
            .current_dir(workspace_root)
            .output()
            .map_err(|error| format!("failed to run {ripr_bin}: {error}"))?;
        if json_output.status.success() {
            fs::write(&json_path, &json_output.stdout)
                .map_err(|error| format!("failed to write {}: {error}", json_path.display()))?;
            let md_output = Command::new(&ripr_bin)
                .arg("check")
                .arg("--root")
                .arg(".")
                .arg("--format")
                .arg("repo-exposure-md")
                .current_dir(workspace_root)
                .output()
                .map_err(|error| format!("failed to run {ripr_bin}: {error}"))?;
            if md_output.status.success() {
                fs::write(&md_path, &md_output.stdout)
                    .map_err(|error| format!("failed to write {}: {error}", md_path.display()))?;
            } else {
                ensure_markdown_sibling(&json_path, &md_path, "# RIPR PR Evidence")?;
            }
            used_ripr = true;
        }
    }

    if !used_ripr {
        write_json_pretty(
            &json_path,
            &json!({
                "schemaVersion": 1,
                "tool": "ripr",
                "scope": "pull_request",
                "status": "not_run",
                "warnings": ["ripr PR evidence was not run locally; CI runs ripr with full repository context"]
            }),
        )?;
        fs::write(
            &md_path,
            "# RIPR PR Evidence\n\nRIPR evidence was not produced locally. CI runs `ripr` with full repository context and stores diff-scoped artifacts here.\n",
        )
        .map_err(|error| format!("failed to write {}: {error}", md_path.display()))?;
    }

    validate_ripr_pr_contract(&output_dir)
}

fn ripr_review_comments(workspace_root: &Path, check: bool) -> Result<(), String> {
    let output_dir = workspace_root.join(RIPR_REVIEW_DIR);
    if check {
        validate_json_file(&output_dir.join("comments.json"))?;
        require_non_empty_file(&output_dir.join("comments.md"))?;
        stdout_line(format_args!(
            "ripr-review-comments: output contract is valid"
        ));
        return Ok(());
    }

    fs::create_dir_all(&output_dir)
        .map_err(|error| format!("failed to create {}: {error}", output_dir.display()))?;
    let json_path = output_dir.join("comments.json");
    let md_path = output_dir.join("comments.md");
    let ripr_bin = env::var("RIPR_BIN").unwrap_or_else(|_| "ripr".to_string());

    if command_exists(&ripr_bin, workspace_root) {
        let output = Command::new(&ripr_bin)
            .arg("review-comments")
            .arg("--root")
            .arg(".")
            .arg("--base")
            .arg(env::var("RIPR_BASE").unwrap_or_else(|_| "origin/main".to_string()))
            .arg("--head")
            .arg(env::var("RIPR_HEAD").unwrap_or_else(|_| "HEAD".to_string()))
            .arg("--out")
            .arg("target/ripr/review/comments.json")
            .current_dir(workspace_root)
            .output()
            .map_err(|error| format!("failed to run {ripr_bin}: {error}"))?;
        if output.status.success() {
            ensure_markdown_sibling(&json_path, &md_path, "# RIPR Review Guidance")?;
        } else {
            write_ripr_review_placeholder(
                &json_path,
                &md_path,
                &format!(
                    "{ripr_bin} review-comments did not produce local guidance: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            )?;
        }
    } else {
        write_ripr_review_placeholder(&json_path, &md_path, "ripr was not available locally")?;
    }

    validate_json_file(&json_path)?;
    require_non_empty_file(&md_path)
}

fn write_ripr_review_placeholder(
    json_path: &Path,
    md_path: &Path,
    warning: &str,
) -> Result<(), String> {
    write_json_pretty(
        json_path,
        &json!({
            "schemaVersion": 1,
            "comments": [],
            "summary_only": [],
            "suppressed": [],
            "warnings": [warning]
        }),
    )?;
    fs::write(
        md_path,
        format!(
            "# RIPR Review Guidance\n\nNo RIPR review guidance was produced locally. {warning}\n"
        ),
    )
    .map_err(|error| format!("failed to write {}: {error}", md_path.display()))
}

fn docs_sync(workspace_root: &Path, check: bool) -> Result<(), String> {
    let mut commands = vec![
        vec![
            "run",
            "-p",
            "openracing-tools",
            "--bin",
            "validate-adr",
            "--",
            "--verbose",
        ],
        vec![
            "run",
            "-p",
            "openracing-tools",
            "--bin",
            "generate-docs-index",
            "--",
        ],
    ];
    if check {
        commands.push(vec![
            "run",
            "-p",
            "openracing-tools",
            "--bin",
            "yaml-sync-check",
            "--",
            "--check",
        ]);
    }
    for args in commands {
        run_cargo(workspace_root, &args)?;
    }
    Ok(())
}

fn check_file_policy(workspace_root: &Path) -> Result<(), String> {
    run_program(workspace_root, "python", &["scripts/policy_file.py"])
}

fn pr_fast_gate(workspace_root: &Path) -> Result<(), String> {
    docs_sync(workspace_root, true)?;
    check_file_policy(workspace_root)?;
    run_program(workspace_root, "git", &["diff", "--check"])
}

fn validate_ripr_pr_contract(output_dir: &Path) -> Result<(), String> {
    validate_json_file(&output_dir.join("repo-exposure.json"))?;
    require_non_empty_file(&output_dir.join("repo-exposure.md"))
}

fn validate_json_file(path: &Path) -> Result<Value, String> {
    let content = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    serde_json::from_str(&content)
        .map_err(|error| format!("{} is invalid JSON: {error}", path.display()))
}

fn require_non_empty_file(path: &Path) -> Result<(), String> {
    let content = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    if content.trim().is_empty() {
        return Err(format!("{} is empty", path.display()));
    }
    Ok(())
}

fn ensure_markdown_sibling(json_path: &Path, md_path: &Path, heading: &str) -> Result<(), String> {
    if md_path.exists() {
        return require_non_empty_file(md_path);
    }
    let json = fs::read_to_string(json_path)
        .map_err(|error| format!("failed to read {}: {error}", json_path.display()))?;
    fs::write(md_path, format!("{heading}\n\n```json\n{json}\n```\n"))
        .map_err(|error| format!("failed to write {}: {error}", md_path.display()))
}

fn write_json_pretty(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let content = serde_json::to_string_pretty(value)
        .map_err(|error| format!("failed to serialize {}: {error}", path.display()))?;
    fs::write(path, format!("{content}\n"))
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn compare_files(committed: &Path, generated: &Path) -> Result<(), String> {
    let committed_content = fs::read_to_string(committed)
        .map_err(|error| format!("failed to read committed {}: {error}", committed.display()))?;
    let generated_content = fs::read_to_string(generated)
        .map_err(|error| format!("failed to read generated {}: {error}", generated.display()))?;
    if committed_content != generated_content {
        return Err(format!(
            "badge endpoint drift: {} differs from {}",
            committed.display(),
            generated.display()
        ));
    }
    Ok(())
}

fn command_exists(program: &str, workspace_root: &Path) -> bool {
    Command::new(program)
        .arg("--help")
        .current_dir(workspace_root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

fn run_cargo(workspace_root: &Path, args: &[&str]) -> Result<(), String> {
    run_program(workspace_root, "cargo", args)
}

fn run_program(workspace_root: &Path, program: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new(program)
        .args(args.iter().map(OsStr::new))
        .current_dir(workspace_root)
        .status()
        .map_err(|error| format!("failed to run {program}: {error}"))?;
    if !status.success() {
        return Err(format!("{program} {} failed", args.join(" ")));
    }
    Ok(())
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
    fn rejects_badge_label_drift() {
        let badge = ShieldsEndpointBadge {
            schema_version: 1,
            label: "ripr".to_string(),
            message: "0".to_string(),
            color: "brightgreen".to_string(),
        };

        assert!(validate_shields_badge(&badge, Some("ripr+")).is_err());
    }

    #[test]
    fn parses_check_flag_for_badges() -> Result<(), String> {
        let args = parse_args(["xtask", "badges", "--check"].map(str::to_string))?;
        assert_eq!(args.command, XtaskCommand::Badges { check: true });
        Ok(())
    }
}
