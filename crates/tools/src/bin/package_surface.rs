//! Validate the OpenRacing workspace package surface policy.
//!
//! The checker intentionally lives in `openracing-tools` because it is a
//! repository-maintenance guardrail rather than product code.

#![deny(static_mut_refs)]
#![deny(unused_must_use)]

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Eq, PartialEq)]
struct Args {
    check: bool,
    policy: PathBuf,
    json_out: Option<PathBuf>,
    md_out: Option<PathBuf>,
    allow_current_names: bool,
    root: PathBuf,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            check: false,
            policy: PathBuf::from("policy/crate-boundaries.toml"),
            json_out: None,
            md_out: None,
            allow_current_names: true,
            root: PathBuf::from("."),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Policy {
    schema_version: u32,
    public: PackageList,
    internal: PackageList,
    #[serde(default)]
    temporary_names: BTreeMap<String, String>,
    #[serde(default)]
    collapse: Vec<CollapseEntry>,
}

#[derive(Debug, Deserialize)]
struct PackageList {
    packages: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CollapseEntry {
    from: String,
    to: String,
    owner: String,
    reason: String,
    #[serde(default)]
    transitional: bool,
}

#[derive(Debug, Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_members: Vec<String>,
    workspace_root: String,
    #[serde(default, rename = "metadata")]
    workspace_metadata: Value,
}

#[derive(Debug, Deserialize)]
struct Package {
    id: String,
    name: String,
    manifest_path: String,
    targets: Vec<Target>,
    dependencies: Vec<Dependency>,
    #[serde(default)]
    features: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    publish: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct Target {
    #[serde(default)]
    kind: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Dependency {
    name: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    path: Option<String>,
    req: String,
}

#[derive(Debug, Serialize, Clone, Eq, PartialEq)]
struct PackageSummary {
    name: String,
    manifest_path: String,
    publish: String,
    classification: String,
}

#[derive(Debug, Serialize, Clone, Eq, PartialEq)]
struct PathDependencyFinding {
    package: String,
    dependency: String,
    kind: String,
    req: String,
    severity: String,
    reason: String,
}

#[derive(Debug, Serialize, Eq, PartialEq)]
struct Report {
    success: bool,
    generated_at_utc: String,
    policy: String,
    public_packages: Vec<String>,
    internal_packages: Vec<String>,
    collapse_packages: Vec<String>,
    violations: Vec<String>,
    warnings: Vec<String>,
    workspace_members: Vec<PackageSummary>,
    publishable_packages: Vec<String>,
    path_dependency_findings: Vec<PathDependencyFinding>,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut iter = args.into_iter().skip(1);

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--check" => parsed.check = true,
            "--policy" => {
                parsed.policy = PathBuf::from(
                    iter.next()
                        .ok_or_else(|| "--policy requires a path".to_string())?,
                );
            }
            "--json-out" => {
                parsed.json_out = Some(PathBuf::from(
                    iter.next()
                        .ok_or_else(|| "--json-out requires a path".to_string())?,
                ));
            }
            "--md-out" => {
                parsed.md_out = Some(PathBuf::from(
                    iter.next()
                        .ok_or_else(|| "--md-out requires a path".to_string())?,
                ));
            }
            "--allow-current-names" => parsed.allow_current_names = true,
            "--root" => {
                parsed.root = PathBuf::from(
                    iter.next()
                        .ok_or_else(|| "--root requires a path".to_string())?,
                );
            }
            "-h" | "--help" => return Err(usage()),
            other => return Err(format!("unknown argument: {other}\n{}", usage())),
        }
    }

    Ok(parsed)
}

fn usage() -> String {
    "Usage: package-surface [--check] [--policy <path>] [--json-out <path>] [--md-out <path>] [--allow-current-names] [--root <path>]".to_string()
}

fn read_policy(path: &Path) -> Result<Policy> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read policy file {}", path.display()))?;
    let policy: Policy = toml::from_str(&content)
        .with_context(|| format!("failed to parse policy file {}", path.display()))?;
    if policy.schema_version != 1 {
        bail!(
            "unsupported crate-boundaries schema_version {}",
            policy.schema_version
        );
    }
    Ok(policy)
}

fn cargo_metadata(root: &Path) -> Result<Metadata> {
    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--locked")
        .arg("--format-version")
        .arg("1")
        .current_dir(root)
        .output()
        .context("failed to run cargo metadata")?;

    if !output.status.success() {
        bail!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    serde_json::from_slice(&output.stdout).context("failed to parse cargo metadata JSON")
}

fn package_manifest(path: &Path) -> Result<Value> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read manifest {}", path.display()))?;
    toml::from_str(&content).with_context(|| format!("failed to parse manifest {}", path.display()))
}

fn root_manifest_has_default_members(root: &Path) -> Result<bool> {
    let manifest = package_manifest(&root.join("Cargo.toml"))?;
    Ok(manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("default-members"))
        .is_some())
}

fn publish_allowlist(metadata: &Metadata) -> BTreeSet<String> {
    metadata
        .workspace_metadata
        .get("publish")
        .and_then(|publish| publish.get("allow"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect()
}

fn is_publishable(package: &Package) -> bool {
    package
        .publish
        .as_ref()
        .is_none_or(|publish| !publish.is_empty())
}

fn publish_label(package: &Package) -> String {
    match &package.publish {
        None => "true".to_string(),
        Some(values) if values.is_empty() => "false".to_string(),
        Some(values) => format!("registries:{values:?}"),
    }
}

fn has_library_target(package: &Package) -> bool {
    package
        .targets
        .iter()
        .any(|target| target.kind.iter().any(|kind| kind == "lib"))
}

fn normal_or_build(kind: &Option<String>) -> bool {
    kind.as_deref().is_none_or(|value| value == "build")
}

fn classify_package(
    name: &str,
    public: &BTreeSet<String>,
    current_names: &BTreeSet<String>,
    internal: &BTreeSet<String>,
    collapse: &BTreeSet<String>,
) -> Option<&'static str> {
    if public.contains(name) {
        Some("public")
    } else if current_names.contains(name) {
        Some("temporary-public-name")
    } else if internal.contains(name) {
        Some("internal")
    } else if collapse.contains(name) {
        Some("collapse")
    } else {
        None
    }
}

fn analyze(args: &Args) -> Result<Report> {
    let policy_path = args.root.join(&args.policy);
    let policy = read_policy(&policy_path)?;
    let metadata = cargo_metadata(&args.root)?;

    let workspace_member_ids: BTreeSet<_> = metadata.workspace_members.iter().collect();
    let workspace_packages: Vec<_> = metadata
        .packages
        .iter()
        .filter(|package| workspace_member_ids.contains(&package.id))
        .collect();

    let public: BTreeSet<_> = policy.public.packages.iter().cloned().collect();
    let internal: BTreeSet<_> = policy.internal.packages.iter().cloned().collect();
    let collapse: BTreeSet<_> = policy
        .collapse
        .iter()
        .map(|entry| entry.from.clone())
        .collect();
    let current_names: BTreeSet<_> = if args.allow_current_names {
        policy.temporary_names.values().cloned().collect()
    } else {
        BTreeSet::new()
    };
    let current_to_public: BTreeMap<_, _> = policy
        .temporary_names
        .iter()
        .map(|(public_name, current_name)| (current_name.clone(), public_name.clone()))
        .collect();
    let allow = publish_allowlist(&metadata);

    let mut violations = Vec::new();
    let mut warnings = Vec::new();
    let mut summaries = Vec::new();
    let mut publishable = Vec::new();
    let mut path_findings = Vec::new();

    if allow != public {
        for missing in public.difference(&allow) {
            violations.push(format!(
                "policy public package `{missing}` is missing from [workspace.metadata.publish].allow"
            ));
        }
        for extra in allow.difference(&public) {
            violations.push(format!(
                "publish allowlist package `{extra}` is missing from policy public packages"
            ));
        }
    }

    if !root_manifest_has_default_members(Path::new(&metadata.workspace_root))? {
        warnings.push("workspace.default-members is missing".to_string());
    }

    for entry in &policy.collapse {
        if entry.to.trim().is_empty()
            || entry.owner.trim().is_empty()
            || entry.reason.trim().is_empty()
        {
            violations.push(format!(
                "collapse entry `{}` must include non-empty to, owner, and reason fields",
                entry.from
            ));
        }
    }

    for package in &workspace_packages {
        let classification =
            classify_package(&package.name, &public, &current_names, &internal, &collapse);
        let classification_label = classification.unwrap_or("unclassified").to_string();
        let publishable_package = is_publishable(package);

        if publishable_package {
            publishable.push(package.name.clone());
        }

        summaries.push(PackageSummary {
            name: package.name.clone(),
            manifest_path: package.manifest_path.clone(),
            publish: publish_label(package),
            classification: classification_label.clone(),
        });

        if classification.is_none() {
            violations.push(format!(
                "workspace member `{}` is not classified as public, internal, collapse, or temporary",
                package.name
            ));
        }

        if internal.contains(&package.name) && publishable_package {
            violations.push(format!(
                "internal package `{}` must set publish = false",
                package.name
            ));
        }

        if collapse.contains(&package.name) && publishable_package {
            let transitional = policy
                .collapse
                .iter()
                .find(|entry| entry.from == package.name)
                .is_some_and(|entry| entry.transitional);
            if transitional {
                warnings.push(format!(
                    "collapse package `{}` remains publishable during the transition",
                    package.name
                ));
            } else {
                violations.push(format!(
                    "collapse package `{}` has publish=true and no transitional note",
                    package.name
                ));
            }
        }

        if publishable_package
            && !allow.contains(&package.name)
            && !current_to_public.contains_key(&package.name)
        {
            warnings.push(format!(
                "package `{}` has publish=true but is not in the publish allowlist",
                package.name
            ));
        }

        if package.name.starts_with("racing-wheel-") {
            warnings.push(format!(
                "package `{}` still uses the racing-wheel-* naming spine",
                package.name
            ));
        }

        if classification_label == "public" || classification_label == "temporary-public-name" {
            let strict_public_checks = classification_label == "public";
            if package.features.len() > 12 {
                warnings.push(format!(
                    "public package `{}` has {} features (>12)",
                    package.name,
                    package.features.len()
                ));
            }

            for feature in package.features.keys() {
                if collapse.contains(feature) || current_names.contains(feature) {
                    warnings.push(format!(
                        "public package `{}` has feature `{}` matching a former microcrate/current package name",
                        package.name, feature
                    ));
                }
            }

            let is_library = has_library_target(package);
            for dep in &package.dependencies {
                if dep.path.is_some() && normal_or_build(&dep.kind) && dep.req == "*" {
                    let severity = if !strict_public_checks
                        || public.contains(&dep.name)
                        || collapse.contains(&dep.name)
                        || internal.contains(&dep.name)
                        || current_to_public.contains_key(&dep.name)
                    {
                        "warning"
                    } else {
                        "violation"
                    };
                    let reason = if severity == "warning" {
                        "path-only dependency is transitional until the collapse/naming PR lands"
                    } else {
                        "public package has a normal/build path dependency without a version"
                    };
                    if severity == "violation" {
                        violations.push(format!(
                            "public package `{}` has path-only dependency `{}` without a version",
                            package.name, dep.name
                        ));
                    } else {
                        warnings.push(format!(
                            "public package `{}` has transitional path-only dependency `{}` without a version",
                            package.name, dep.name
                        ));
                    }
                    path_findings.push(PathDependencyFinding {
                        package: package.name.clone(),
                        dependency: dep.name.clone(),
                        kind: dep.kind.clone().unwrap_or_else(|| "normal".to_string()),
                        req: dep.req.clone(),
                        severity: severity.to_string(),
                        reason: reason.to_string(),
                    });
                }

                if strict_public_checks
                    && is_library
                    && normal_or_build(&dep.kind)
                    && internal.contains(&dep.name)
                    && dep.name != "workspace-hack"
                {
                    violations.push(format!(
                        "public library package `{}` depends on internal/tool/test package `{}`",
                        package.name, dep.name
                    ));
                }
            }
        }
    }

    summaries.sort_by(|left, right| left.name.cmp(&right.name));
    publishable.sort();
    warnings.sort();
    warnings.dedup();
    violations.sort();
    violations.dedup();

    Ok(Report {
        success: violations.is_empty(),
        generated_at_utc: Utc::now().to_rfc3339(),
        policy: args.policy.display().to_string(),
        public_packages: policy.public.packages,
        internal_packages: policy.internal.packages,
        collapse_packages: policy
            .collapse
            .into_iter()
            .map(|entry| entry.from)
            .collect(),
        violations,
        warnings,
        workspace_members: summaries,
        publishable_packages: publishable,
        path_dependency_findings: path_findings,
    })
}

fn write_json(path: &Path, report: &Report) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create output directory {}", parent.display()))?;
    }
    let content =
        serde_json::to_string_pretty(report).context("failed to serialize JSON report")?;
    fs::write(path, format!("{content}\n"))
        .with_context(|| format!("failed to write JSON report {}", path.display()))
}

fn markdown_report(report: &Report) -> String {
    let mut out = String::new();
    out.push_str("# Package Surface Report\n\n");
    out.push_str(&format!("* Success: `{}`\n", report.success));
    out.push_str(&format!("* Generated: `{}`\n", report.generated_at_utc));
    out.push_str(&format!("* Policy: `{}`\n\n", report.policy));

    out.push_str("## Violations\n\n");
    if report.violations.is_empty() {
        out.push_str("None.\n\n");
    } else {
        for violation in &report.violations {
            out.push_str(&format!("* {violation}\n"));
        }
        out.push('\n');
    }

    out.push_str("## Warnings\n\n");
    if report.warnings.is_empty() {
        out.push_str("None.\n\n");
    } else {
        for warning in &report.warnings {
            out.push_str(&format!("* {warning}\n"));
        }
        out.push('\n');
    }

    out.push_str("## Workspace Members\n\n");
    out.push_str("| Package | Classification | Publish |\n");
    out.push_str("| --- | --- | --- |\n");
    for package in &report.workspace_members {
        out.push_str(&format!(
            "| `{}` | `{}` | `{}` |\n",
            package.name, package.classification, package.publish
        ));
    }
    out.push('\n');

    out.push_str("## Path Dependency Findings\n\n");
    if report.path_dependency_findings.is_empty() {
        out.push_str("None.\n");
    } else {
        out.push_str("| Package | Dependency | Kind | Severity | Reason |\n");
        out.push_str("| --- | --- | --- | --- | --- |\n");
        for finding in &report.path_dependency_findings {
            out.push_str(&format!(
                "| `{}` | `{}` | `{}` | `{}` | {} |\n",
                finding.package, finding.dependency, finding.kind, finding.severity, finding.reason
            ));
        }
    }

    out
}

fn write_markdown(path: &Path, report: &Report) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create output directory {}", parent.display()))?;
    }
    fs::write(path, markdown_report(report))
        .with_context(|| format!("failed to write Markdown report {}", path.display()))
}

fn run(args: Args) -> Result<ExitCode> {
    let report = analyze(&args)?;

    if let Some(path) = &args.json_out {
        write_json(path, &report)?;
    }
    if let Some(path) = &args.md_out {
        write_markdown(path, &report)?;
    }

    stdout_line(format_args!(
        "package-surface: {} violation(s), {} warning(s)",
        report.violations.len(),
        report.warnings.len()
    ));
    for violation in &report.violations {
        stderr_line(format_args!("VIOLATION: {violation}"));
    }
    for warning in &report.warnings {
        stdout_line(format_args!("WARNING: {warning}"));
    }

    if args.check && !report.success {
        Ok(ExitCode::from(1))
    } else {
        Ok(ExitCode::SUCCESS)
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

fn main() -> ExitCode {
    let args = match parse_args(env::args()) {
        Ok(args) => args,
        Err(message) => {
            stderr_line(format_args!("{message}"));
            return ExitCode::from(2);
        }
    };

    match run(args) {
        Ok(code) => code,
        Err(error) => {
            stderr_line(format_args!("ERROR: {error:#}"));
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_defaults_to_policy_checking_inputs() -> Result<()> {
        let args = parse_args(vec!["package-surface".to_string()]).map_err(anyhow::Error::msg)?;
        assert_eq!(args.policy, PathBuf::from("policy/crate-boundaries.toml"));
        assert!(args.allow_current_names);
        assert!(!args.check);
        Ok(())
    }

    #[test]
    fn parse_args_accepts_report_outputs() -> Result<()> {
        let args = parse_args(vec![
            "package-surface".to_string(),
            "--check".to_string(),
            "--policy".to_string(),
            "policy/custom.toml".to_string(),
            "--json-out".to_string(),
            "target/report.json".to_string(),
            "--md-out".to_string(),
            "target/report.md".to_string(),
        ])
        .map_err(anyhow::Error::msg)?;
        assert!(args.check);
        assert_eq!(args.policy, PathBuf::from("policy/custom.toml"));
        assert_eq!(args.json_out, Some(PathBuf::from("target/report.json")));
        assert_eq!(args.md_out, Some(PathBuf::from("target/report.md")));
        Ok(())
    }

    #[test]
    fn markdown_report_lists_violations_and_members() {
        let report = Report {
            success: false,
            generated_at_utc: "2026-05-17T00:00:00Z".to_string(),
            policy: "policy/crate-boundaries.toml".to_string(),
            public_packages: vec!["openracing".to_string()],
            internal_packages: vec!["openracing-tools".to_string()],
            collapse_packages: vec!["openracing-scheduler".to_string()],
            violations: vec!["example violation".to_string()],
            warnings: vec!["example warning".to_string()],
            workspace_members: vec![PackageSummary {
                name: "openracing-tools".to_string(),
                manifest_path: "crates/tools/Cargo.toml".to_string(),
                publish: "false".to_string(),
                classification: "internal".to_string(),
            }],
            publishable_packages: Vec::new(),
            path_dependency_findings: Vec::new(),
        };

        let markdown = markdown_report(&report);
        assert!(markdown.contains("example violation"));
        assert!(markdown.contains("openracing-tools"));
    }

    #[test]
    fn publish_false_is_not_publishable() {
        let package = Package {
            id: "pkg".to_string(),
            name: "pkg".to_string(),
            manifest_path: "Cargo.toml".to_string(),
            targets: Vec::new(),
            dependencies: Vec::new(),
            features: BTreeMap::new(),
            publish: Some(Vec::new()),
        };
        assert!(!is_publishable(&package));
    }
}
