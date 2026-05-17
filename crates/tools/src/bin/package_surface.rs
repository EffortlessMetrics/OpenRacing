//! Validate OpenRacing workspace package-surface policy.

#![deny(static_mut_refs)]
#![deny(unused_must_use)]

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    check: bool,
    policy: PathBuf,
    json_out: Option<PathBuf>,
    md_out: Option<PathBuf>,
    allow_current_names: bool,
}

#[derive(Debug, Deserialize)]
struct Policy {
    schema_version: u32,
    public: PackageList,
    internal: PackageList,
    #[serde(default)]
    temporary: PackageList,
    #[serde(default)]
    collapse: Vec<Collapse>,
}

#[derive(Debug, Default, Deserialize)]
struct PackageList {
    #[serde(default)]
    packages: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Collapse {
    from: String,
    to: String,
    owner: String,
    reason: String,
    #[serde(default)]
    transitional_note: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_members: Vec<String>,
    workspace_root: PathBuf,
    metadata: Value,
}

#[derive(Debug, Deserialize)]
struct Package {
    id: String,
    name: String,
    #[serde(default)]
    publish: Option<Vec<String>>,
    #[serde(default)]
    dependencies: Vec<Dependency>,
    #[serde(default)]
    targets: Vec<Target>,
    #[serde(default)]
    features: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct Dependency {
    name: String,
    #[serde(default)]
    source: Option<String>,
    req: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    path: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct Target {
    #[serde(default)]
    kind: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Report {
    success: bool,
    generated_at_utc: String,
    policy: String,
    public_packages: Vec<String>,
    internal_packages: Vec<String>,
    collapse_packages: Vec<String>,
    violations: Vec<String>,
    warnings: Vec<String>,
    workspace_members: Vec<String>,
    publishable_packages: Vec<String>,
    path_dependency_findings: Vec<PathDependencyFinding>,
}

#[derive(Debug, Serialize)]
struct PathDependencyFinding {
    package: String,
    dependency: String,
    kind: String,
    requirement: String,
    path: String,
    severity: String,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> std::result::Result<Args, String> {
    let mut check = false;
    let mut policy = PathBuf::from("policy/crate-boundaries.toml");
    let mut json_out = None;
    let mut md_out = None;
    let mut allow_current_names = false;
    let mut iter = args.into_iter().skip(1);

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--check" => check = true,
            "--policy" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--policy requires a path".to_string())?;
                policy = PathBuf::from(value);
            }
            "--json-out" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--json-out requires a path".to_string())?;
                json_out = Some(PathBuf::from(value));
            }
            "--md-out" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--md-out requires a path".to_string())?;
                md_out = Some(PathBuf::from(value));
            }
            "--allow-current-names" => allow_current_names = true,
            "-h" | "--help" => return Err(usage()),
            other => return Err(format!("unknown argument: {other}\n{}", usage())),
        }
    }

    Ok(Args {
        check,
        policy,
        json_out,
        md_out,
        allow_current_names,
    })
}

fn usage() -> String {
    "Usage: package-surface [--check] [--policy <path>] [--json-out <path>] [--md-out <path>] [--allow-current-names]".to_string()
}

fn read_policy(path: &Path) -> Result<Policy> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read policy file {}", path.display()))?;
    let policy = toml::from_str::<Policy>(&content)
        .with_context(|| format!("failed to parse policy file {}", path.display()))?;
    if policy.schema_version != 1 {
        bail!(
            "unsupported policy schema_version {}; expected 1",
            policy.schema_version
        );
    }
    Ok(policy)
}

fn cargo_metadata() -> Result<Metadata> {
    let output = Command::new("cargo")
        .args(["metadata", "--locked", "--format-version", "1", "--no-deps"])
        .output()
        .context("failed to execute cargo metadata")?;

    if !output.status.success() {
        bail!(
            "cargo metadata failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    serde_json::from_slice::<Metadata>(&output.stdout).context("failed to parse cargo metadata")
}

fn read_root_manifest(root: &Path) -> Result<toml::Value> {
    let manifest = root.join("Cargo.toml");
    let content = fs::read_to_string(&manifest)
        .with_context(|| format!("failed to read {}", manifest.display()))?;
    toml::from_str(&content).with_context(|| format!("failed to parse {}", manifest.display()))
}

fn string_array(value: Option<&toml::Value>) -> Vec<String> {
    value
        .and_then(toml::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(toml::Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn workspace_publish_allow(metadata: &Metadata, root_manifest: &toml::Value) -> Vec<String> {
    if let Some(items) = metadata
        .metadata
        .get("publish")
        .and_then(|publish| publish.get("allow"))
        .and_then(Value::as_array)
    {
        return items
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect();
    }

    string_array(
        root_manifest
            .get("workspace")
            .and_then(|workspace| workspace.get("metadata"))
            .and_then(|metadata| metadata.get("publish"))
            .and_then(|publish| publish.get("allow")),
    )
}

fn default_members(root_manifest: &toml::Value) -> Vec<String> {
    string_array(
        root_manifest
            .get("workspace")
            .and_then(|workspace| workspace.get("default-members")),
    )
}

fn is_publishable(package: &Package) -> bool {
    package.publish.is_none()
}

fn is_library_package(package: &Package) -> bool {
    package
        .targets
        .iter()
        .any(|target| target.kind.iter().any(|kind| kind == "lib"))
}

fn is_normal_or_build(dep: &Dependency) -> bool {
    matches!(dep.kind.as_deref(), None | Some("build"))
}

fn package_by_id(metadata: &Metadata) -> BTreeMap<&str, &Package> {
    metadata
        .packages
        .iter()
        .map(|package| (package.id.as_str(), package))
        .collect()
}

fn set(items: &[String]) -> BTreeSet<String> {
    items.iter().cloned().collect()
}

fn generated_at_utc() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => format!("{}s since Unix epoch", duration.as_secs()),
        Err(_) => "before Unix epoch".to_string(),
    }
}

fn analyze(args: &Args) -> Result<Report> {
    let policy = read_policy(&args.policy)?;
    let metadata = cargo_metadata()?;
    let root_manifest = read_root_manifest(&metadata.workspace_root)?;

    let public = set(&policy.public.packages);
    let internal = set(&policy.internal.packages);
    let temporary = set(&policy.temporary.packages);
    let collapse_from = policy
        .collapse
        .iter()
        .map(|entry| entry.from.clone())
        .collect::<BTreeSet<_>>();
    let former_microcrates = collapse_from.clone();
    let allow = workspace_publish_allow(&metadata, &root_manifest);
    let allow_set = set(&allow);
    let default_members = default_members(&root_manifest);
    let packages_by_id = package_by_id(&metadata);
    let workspace_packages = metadata
        .workspace_members
        .iter()
        .filter_map(|id| packages_by_id.get(id.as_str()).copied())
        .collect::<Vec<_>>();
    let workspace_names = workspace_packages
        .iter()
        .map(|package| package.name.clone())
        .collect::<BTreeSet<_>>();

    let mut violations = Vec::new();
    let mut warnings = Vec::new();
    let mut path_dependency_findings = Vec::new();

    if default_members.is_empty() {
        warnings.push("workspace.default-members is missing".to_string());
    }

    for package in &workspace_packages {
        let name = &package.name;
        let classified = public.contains(name)
            || internal.contains(name)
            || collapse_from.contains(name)
            || temporary.contains(name);
        if !classified {
            violations.push(format!(
                "workspace package `{name}` is not classified as public, internal, collapse, or temporary"
            ));
        }

        if internal.contains(name) && is_publishable(package) {
            violations.push(format!(
                "internal package `{name}` must set publish = false"
            ));
        }

        if collapse_from.contains(name) && is_publishable(package) {
            let has_note = policy
                .collapse
                .iter()
                .any(|entry| entry.from == *name && entry.transitional_note.is_some());
            if !has_note {
                violations.push(format!(
                    "collapse package `{name}` is publishable and lacks a transitional note"
                ));
            }
        }

        if is_publishable(package) && !allow_set.contains(name) {
            warnings.push(format!(
                "package `{name}` has publish=true/default but is not in [workspace.metadata.publish].allow"
            ));
        }

        if name.starts_with("racing-wheel-") {
            warnings.push(format!(
                "package `{name}` still uses the pre-naming-spine racing-wheel-* prefix"
            ));
        }

        if package.features.len() > 12 && public.contains(name) {
            warnings.push(format!(
                "public package `{name}` has {} features (>12)",
                package.features.len()
            ));
        }

        for feature in package.features.keys() {
            if former_microcrates.contains(feature) {
                warnings.push(format!(
                    "package `{name}` has feature `{feature}` matching a former microcrate name"
                ));
            }
        }

        if public.contains(name) {
            for dep in &package.dependencies {
                if dep.source.is_none() && dep.path.is_some() && is_normal_or_build(dep) {
                    let severity = if dep.req == "*" { "warning" } else { "info" };
                    path_dependency_findings.push(PathDependencyFinding {
                        package: name.clone(),
                        dependency: dep.name.clone(),
                        kind: dep.kind.clone().unwrap_or_else(|| "normal".to_string()),
                        requirement: dep.req.clone(),
                        path: dep
                            .path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_default(),
                        severity: severity.to_string(),
                    });
                    if dep.req == "*" {
                        warnings.push(format!(
                            "public package `{name}` has transitional path-only {} dependency `{}` without a version requirement",
                            dep.kind.clone().unwrap_or_else(|| "normal".to_string()),
                            dep.name
                        ));
                    }
                }
            }
        }

        if public.contains(name) && is_library_package(package) {
            for dep in &package.dependencies {
                if is_normal_or_build(dep)
                    && internal.contains(&dep.name)
                    && dep.name != "workspace-hack"
                {
                    violations.push(format!(
                        "public library package `{name}` depends on internal/tool/test package `{}`",
                        dep.name
                    ));
                }
            }
        }
    }

    for package_name in &allow_set {
        if !public.contains(package_name) {
            violations.push(format!(
                "publish allowlist package `{package_name}` is missing from policy public packages"
            ));
        }
    }

    for package_name in &public {
        if !allow_set.contains(package_name) {
            violations.push(format!(
                "policy public package `{package_name}` is missing from [workspace.metadata.publish].allow"
            ));
        }
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

    if !args.allow_current_names {
        for package_name in &workspace_names {
            if package_name.starts_with("racing-wheel-")
                && (collapse_from.contains(package_name) || temporary.contains(package_name))
            {
                warnings.push(format!(
                    "`{package_name}` is accepted as a current name; pass --allow-current-names to document the transition explicitly"
                ));
            }
        }
    }

    let mut publishable_packages = workspace_packages
        .iter()
        .filter(|package| is_publishable(package))
        .map(|package| package.name.clone())
        .collect::<Vec<_>>();
    publishable_packages.sort();

    let mut collapse_packages = policy
        .collapse
        .iter()
        .map(|entry| entry.from.clone())
        .collect::<Vec<_>>();
    collapse_packages.sort();

    let mut public_packages = policy.public.packages.clone();
    public_packages.sort();
    let mut internal_packages = policy.internal.packages.clone();
    internal_packages.sort();
    let mut workspace_members = workspace_names.into_iter().collect::<Vec<_>>();
    violations.sort();
    warnings.sort();
    warnings.dedup();
    workspace_members.sort();

    Ok(Report {
        success: violations.is_empty(),
        generated_at_utc: generated_at_utc(),
        policy: args.policy.display().to_string(),
        public_packages,
        internal_packages,
        collapse_packages,
        violations,
        warnings,
        workspace_members,
        publishable_packages,
        path_dependency_findings,
    })
}

fn render_markdown(report: &Report) -> String {
    let mut output = String::new();
    output.push_str("# Package Surface Report\n\n");
    output.push_str(&format!("- Success: `{}`\n", report.success));
    output.push_str(&format!("- Generated: `{}`\n", report.generated_at_utc));
    output.push_str(&format!("- Policy: `{}`\n\n", report.policy));

    push_list(&mut output, "Violations", &report.violations);
    push_list(&mut output, "Warnings", &report.warnings);
    push_list(&mut output, "Public Packages", &report.public_packages);
    push_list(&mut output, "Internal Packages", &report.internal_packages);
    push_list(&mut output, "Collapse Packages", &report.collapse_packages);
    push_list(&mut output, "Workspace Members", &report.workspace_members);
    push_list(
        &mut output,
        "Publishable Packages",
        &report.publishable_packages,
    );

    output.push_str("## Path Dependency Findings\n\n");
    if report.path_dependency_findings.is_empty() {
        output.push_str("None.\n");
    } else {
        for finding in &report.path_dependency_findings {
            output.push_str(&format!(
                "- `{}` -> `{}` ({}, req `{}`, {}, `{}`)\n",
                finding.package,
                finding.dependency,
                finding.kind,
                finding.requirement,
                finding.severity,
                finding.path
            ));
        }
    }
    output
}

fn push_list(output: &mut String, heading: &str, items: &[String]) {
    output.push_str(&format!("## {heading}\n\n"));
    if items.is_empty() {
        output.push_str("None.\n\n");
        return;
    }
    for item in items {
        output.push_str(&format!("- {item}\n"));
    }
    output.push('\n');
}

fn write_report(path: &Path, content: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))
}

fn run(args: Args) -> Result<std::process::ExitCode> {
    let report = analyze(&args)?;

    if let Some(path) = &args.json_out {
        let json = serde_json::to_vec_pretty(&report).context("failed to serialize JSON report")?;
        write_report(path, &json)?;
    }

    if let Some(path) = &args.md_out {
        write_report(path, render_markdown(&report).as_bytes())?;
    }

    stdout_line(format_args!(
        "package-surface: {} violation(s), {} warning(s)",
        report.violations.len(),
        report.warnings.len()
    ));
    for violation in &report.violations {
        stdout_line(format_args!("VIOLATION: {violation}"));
    }
    for warning in &report.warnings {
        stdout_line(format_args!("WARNING: {warning}"));
    }

    if args.check && !report.success {
        return Ok(std::process::ExitCode::FAILURE);
    }
    Ok(std::process::ExitCode::SUCCESS)
}

fn stdout_line(args: std::fmt::Arguments<'_>) {
    let mut stdout = io::stdout().lock();
    let _ = writeln!(stdout, "{args}");
}

fn stderr_line(args: std::fmt::Arguments<'_>) {
    let mut stderr = io::stderr().lock();
    let _ = writeln!(stderr, "{args}");
}

fn main() -> std::process::ExitCode {
    let args = match parse_args(env::args()) {
        Ok(args) => args,
        Err(message) => {
            stderr_line(format_args!("{message}"));
            return std::process::ExitCode::from(2);
        }
    };

    match run(args) {
        Ok(code) => code,
        Err(error) => {
            stderr_line(format_args!("ERROR: {error:#}"));
            std::process::ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_accepts_all_options() -> Result<()> {
        let args = parse_args([
            "package-surface".to_string(),
            "--check".to_string(),
            "--policy".to_string(),
            "custom.toml".to_string(),
            "--json-out".to_string(),
            "report.json".to_string(),
            "--md-out".to_string(),
            "report.md".to_string(),
            "--allow-current-names".to_string(),
        ])
        .map_err(anyhow::Error::msg)?;

        assert!(args.check);
        assert_eq!(args.policy, PathBuf::from("custom.toml"));
        assert_eq!(args.json_out, Some(PathBuf::from("report.json")));
        assert_eq!(args.md_out, Some(PathBuf::from("report.md")));
        assert!(args.allow_current_names);
        Ok(())
    }

    #[test]
    fn markdown_renders_empty_sections() {
        let report = Report {
            success: true,
            generated_at_utc: "now".to_string(),
            policy: "policy.toml".to_string(),
            public_packages: Vec::new(),
            internal_packages: Vec::new(),
            collapse_packages: Vec::new(),
            violations: Vec::new(),
            warnings: Vec::new(),
            workspace_members: Vec::new(),
            publishable_packages: Vec::new(),
            path_dependency_findings: Vec::new(),
        };

        let markdown = render_markdown(&report);

        assert!(markdown.contains("# Package Surface Report"));
        assert!(markdown.contains("## Violations"));
        assert!(markdown.contains("None."));
    }

    #[test]
    fn publish_false_is_not_publishable() {
        let package = Package {
            id: "example".to_string(),
            name: "example".to_string(),
            publish: Some(Vec::new()),
            dependencies: Vec::new(),
            targets: Vec::new(),
            features: BTreeMap::new(),
        };

        assert!(!is_publishable(&package));
    }
}
