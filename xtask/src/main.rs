use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

const BADGE_ENDPOINT_DIR: &str = "badges";
const BADGE_ENDPOINT_TARGET_DIR: &str = "target/xtask/badges";
const RIPR_PR_DIR: &str = "target/ripr/pr";
const RIPR_REVIEW_DIR: &str = "target/ripr/review";
const RIPR_REPORTS_DIR: &str = "target/ripr/reports";

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
struct ShieldsEndpointBadge {
    #[serde(rename = "schemaVersion")]
    schema_version: u8,
    label: String,
    message: String,
    color: String,
}

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        print_usage();
        return Ok(());
    };

    let remaining = args.collect::<Vec<_>>();
    match command.as_str() {
        "badges" => badges(has_check_flag(&remaining)),
        "ripr-pr" => ripr_pr(has_check_flag(&remaining)),
        "ripr-review-comments" => ripr_review_comments(has_check_flag(&remaining)),
        "check-file-policy" => run_python_script("scripts/policy_file.py", &[]),
        "docs-sync" => docs_sync(has_check_flag(&remaining)),
        "test-efficiency-report" => test_efficiency_report(),
        "pr" => pr_summary(),
        other => bail!("unknown xtask command `{other}`"),
    }
}

fn print_usage() {
    println!(
        "usage: cargo xtask <badges|ripr-pr|ripr-review-comments|test-efficiency-report|check-file-policy|docs-sync|pr> [--check]"
    );
}

fn has_check_flag(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--check")
}

fn workspace_root_path() -> Result<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow!("xtask manifest has no parent workspace root"))
}

fn badges(check: bool) -> Result<()> {
    let workspace_root = workspace_root_path()?;
    let target_dir = workspace_root.join(BADGE_ENDPOINT_TARGET_DIR);
    fs::create_dir_all(&target_dir)
        .with_context(|| format!("creating {}", target_dir.display()))?;

    ensure_test_efficiency_report(&workspace_root)?;
    let ripr_plus = ripr_plus_badge(&workspace_root)?;
    validate_shields_badge(&ripr_plus, Some("ripr+"))?;
    write_json_pretty(&target_dir.join("ripr-plus.json"), &ripr_plus)?;

    if check {
        let committed_dir = workspace_root.join(BADGE_ENDPOINT_DIR);
        compare_files(
            &committed_dir.join("ripr-plus.json"),
            &target_dir.join("ripr-plus.json"),
        )?;
        println!("badges: committed endpoints are current");
        return Ok(());
    }

    let committed_dir = workspace_root.join(BADGE_ENDPOINT_DIR);
    fs::create_dir_all(&committed_dir)
        .with_context(|| format!("creating {}", committed_dir.display()))?;
    fs::copy(
        target_dir.join("ripr-plus.json"),
        committed_dir.join("ripr-plus.json"),
    )
    .with_context(|| "copying ripr-plus badge endpoint")?;

    println!("badges: refreshed public endpoint JSON under badges/");
    Ok(())
}

fn ensure_test_efficiency_report(workspace_root: &Path) -> Result<()> {
    let report_path = workspace_root
        .join(RIPR_REPORTS_DIR)
        .join("test-efficiency.json");
    if report_path.exists() {
        return Ok(());
    }
    write_minimal_test_efficiency_report(&report_path)
}

fn test_efficiency_report() -> Result<()> {
    let workspace_root = workspace_root_path()?;
    let report_path = workspace_root
        .join(RIPR_REPORTS_DIR)
        .join("test-efficiency.json");
    write_minimal_test_efficiency_report(&report_path)?;
    println!(
        "test-efficiency-report: wrote bootstrap report to {}",
        report_path.display()
    );
    Ok(())
}

fn write_minimal_test_efficiency_report(report_path: &Path) -> Result<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }

    let report = serde_json::json!({
        "schema_version": "0.1",
        "metrics": {
            "tests_scanned": 0
        },
        "tests": []
    });
    write_json_pretty(report_path, &report)
}

fn ripr_plus_badge(workspace_root: &Path) -> Result<ShieldsEndpointBadge> {
    let ripr_bin = env::var("RIPR_BIN").unwrap_or_else(|_| "ripr".to_string());
    let output = Command::new(&ripr_bin)
        .arg("check")
        .arg("--root")
        .arg(workspace_root)
        .arg("--format")
        .arg("repo-badge-plus-shields")
        .current_dir(workspace_root)
        .output()
        .with_context(|| format!("running {ripr_bin} for repo-scoped ripr+ badge"))?;

    if !output.status.success() {
        bail!(
            "{ripr_bin} repo-badge-plus-shields failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    serde_json::from_slice(&output.stdout)
        .with_context(|| format!("{ripr_bin} emitted invalid Shields endpoint JSON"))
}

fn validate_shields_badge(
    badge: &ShieldsEndpointBadge,
    expected_label: Option<&str>,
) -> Result<()> {
    if badge.schema_version != 1 {
        bail!("badge `{}` has unsupported schemaVersion", badge.label);
    }

    if let Some(expected_label) = expected_label
        && badge.label != expected_label
    {
        bail!(
            "badge label drifted: got `{}`, expected `{expected_label}`",
            badge.label
        );
    }

    if badge.message.trim().is_empty() {
        bail!("badge `{}` has empty message", badge.label);
    }

    if badge.color.trim().is_empty() {
        bail!("badge `{}` has empty color", badge.label);
    }

    Ok(())
}

fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut data = serde_json::to_string_pretty(value)?;
    data.push('\n');
    fs::write(path, data).with_context(|| format!("writing {}", path.display()))
}

fn compare_files(committed: &Path, generated: &Path) -> Result<()> {
    let committed_bytes = fs::read(committed)
        .with_context(|| format!("reading committed badge endpoint {}", committed.display()))?;
    let generated_bytes = fs::read(generated)
        .with_context(|| format!("reading generated badge endpoint {}", generated.display()))?;
    if committed_bytes != generated_bytes {
        bail!(
            "badge endpoint drift: {} differs from {}",
            committed.display(),
            generated.display()
        );
    }
    Ok(())
}

fn ripr_pr(check: bool) -> Result<()> {
    let workspace_root = workspace_root_path()?;
    let out_dir = workspace_root.join(RIPR_PR_DIR);

    if check {
        validate_ripr_pr_contract(&out_dir)?;
        println!("ripr-pr: output contract is valid");
        return Ok(());
    }

    fs::create_dir_all(&out_dir).with_context(|| format!("creating {}", out_dir.display()))?;
    let ripr_bin = ripr_bin();

    run_ripr_capture(
        &ripr_bin,
        &workspace_root,
        &[
            OsString::from("check"),
            OsString::from("--root"),
            workspace_root.as_os_str().to_os_string(),
            OsString::from("--format"),
            OsString::from("repo-exposure-json"),
        ],
        &out_dir.join("repo-exposure.json"),
    )?;

    run_ripr_capture(
        &ripr_bin,
        &workspace_root,
        &[
            OsString::from("check"),
            OsString::from("--root"),
            workspace_root.as_os_str().to_os_string(),
            OsString::from("--format"),
            OsString::from("repo-exposure-md"),
        ],
        &out_dir.join("repo-exposure.md"),
    )?;

    validate_ripr_pr_contract(&out_dir)
}

fn ripr_review_comments(check: bool) -> Result<()> {
    let workspace_root = workspace_root_path()?;
    let out_dir = workspace_root.join(RIPR_REVIEW_DIR);
    let comments_json = out_dir.join("comments.json");
    let comments_md = out_dir.join("comments.md");

    if check {
        validate_json_file(&comments_json)?;
        validate_nonempty_file(&comments_md)?;
        println!("ripr-review-comments: output contract is valid");
        return Ok(());
    }

    fs::create_dir_all(&out_dir).with_context(|| format!("creating {}", out_dir.display()))?;
    let ripr_bin = ripr_bin();
    let status = Command::new(&ripr_bin)
        .arg("review-comments")
        .arg("--root")
        .arg(&workspace_root)
        .arg("--base")
        .arg(ripr_base())
        .arg("--head")
        .arg(ripr_head())
        .arg("--out")
        .arg(&comments_json)
        .current_dir(&workspace_root)
        .status()
        .with_context(|| format!("running {ripr_bin} review-comments"))?;

    if !status.success() {
        bail!("{ripr_bin} review-comments failed");
    }

    validate_json_file(&comments_json)?;
    validate_nonempty_file(&comments_md)?;
    Ok(())
}

fn ripr_bin() -> String {
    env::var("RIPR_BIN").unwrap_or_else(|_| "ripr".to_string())
}

fn ripr_base() -> String {
    env::var("RIPR_BASE").unwrap_or_else(|_| "origin/main".to_string())
}

fn ripr_head() -> String {
    env::var("RIPR_HEAD").unwrap_or_else(|_| "HEAD".to_string())
}

fn run_ripr_capture(
    ripr_bin: &str,
    workspace_root: &Path,
    args: &[OsString],
    output_path: &Path,
) -> Result<()> {
    let output = Command::new(ripr_bin)
        .args(args)
        .current_dir(workspace_root)
        .output()
        .with_context(|| format!("running {ripr_bin}"))?;

    if !output.status.success() {
        bail!(
            "{ripr_bin} failed while producing {}: {}",
            output_path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fs::write(output_path, output.stdout)
        .with_context(|| format!("writing {}", output_path.display()))
}

fn validate_ripr_pr_contract(out_dir: &Path) -> Result<()> {
    validate_json_file(&out_dir.join("repo-exposure.json"))?;
    validate_nonempty_file(&out_dir.join("repo-exposure.md"))
}

fn validate_json_file(path: &Path) -> Result<()> {
    validate_nonempty_file(path)?;
    let data = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let _: serde_json::Value = serde_json::from_slice(&data)
        .with_context(|| format!("invalid JSON in {}", path.display()))?;
    Ok(())
}

fn validate_nonempty_file(path: &Path) -> Result<()> {
    let metadata = fs::metadata(path).with_context(|| format!("missing {}", path.display()))?;
    if metadata.len() == 0 {
        bail!("{} is empty", path.display());
    }
    Ok(())
}

fn run_python_script(script: &str, args: &[&str]) -> Result<()> {
    let workspace_root = workspace_root_path()?;
    let status = Command::new(python_bin())
        .arg(workspace_root.join(script))
        .args(args)
        .current_dir(&workspace_root)
        .status()
        .with_context(|| format!("running {script}"))?;
    if !status.success() {
        bail!("{script} failed");
    }
    Ok(())
}

fn python_bin() -> String {
    env::var("PYTHON").unwrap_or_else(|_| "python3".to_string())
}

fn docs_sync(_check: bool) -> Result<()> {
    let workspace_root = workspace_root_path()?;
    let validate = Command::new("cargo")
        .arg("run")
        .arg("-p")
        .arg("openracing-tools")
        .arg("--bin")
        .arg("validate-adr")
        .arg("--")
        .current_dir(&workspace_root)
        .status()
        .context("running ADR validation")?;
    if !validate.success() {
        bail!("ADR validation failed");
    }

    let generate = Command::new("cargo")
        .arg("run")
        .arg("-p")
        .arg("openracing-tools")
        .arg("--bin")
        .arg("generate-docs-index")
        .arg("--")
        .current_dir(&workspace_root)
        .status()
        .context("running docs index generation")?;
    if !generate.success() {
        bail!("docs index generation failed");
    }
    Ok(())
}

fn pr_summary() -> Result<()> {
    println!("xtask pr: run repository PR gates documented in AGENTS.md");
    badges(true)?;
    run_python_script("scripts/policy_file.py", &[])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ripr_plus_badge_shape_is_stable() -> Result<()> {
        let badge = ShieldsEndpointBadge {
            schema_version: 1,
            label: "ripr+".to_string(),
            message: "0".to_string(),
            color: "brightgreen".to_string(),
        };

        validate_shields_badge(&badge, Some("ripr+"))
    }

    #[test]
    fn rejects_empty_badge_message() -> Result<()> {
        let badge = ShieldsEndpointBadge {
            schema_version: 1,
            label: "ripr+".to_string(),
            message: " ".to_string(),
            color: "brightgreen".to_string(),
        };

        assert!(validate_shields_badge(&badge, Some("ripr+")).is_err());
        Ok(())
    }
}
