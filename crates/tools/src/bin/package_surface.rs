//! Check OpenRacing workspace package-surface policy.
//!
//! This is the first rail for crate-surface consolidation: it validates the
//! policy map, publish allowlist, internal package status, and obvious package
//! dependency hazards before later PRs move code into module families.

#![deny(static_mut_refs)]
#![deny(unused_must_use)]

use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, PartialEq, Eq)]
struct Args {
    check: bool,
    policy: PathBuf,
    json_out: Option<PathBuf>,
    md_out: Option<PathBuf>,
    allow_current_names: bool,
}

#[derive(Debug)]
struct Policy {
    public_packages: BTreeSet<String>,
    internal_packages: BTreeSet<String>,
    collapse: BTreeMap<String, CollapseEntry>,
    current_public_names: BTreeMap<String, String>,
}

#[derive(Debug)]
struct CollapseEntry {
    to: String,
    owner: String,
    reason: String,
    transitional_note: Option<String>,
}

#[derive(Debug)]
struct WorkspacePackage {
    name: String,
    publish: Option<Vec<String>>,
    features: BTreeSet<String>,
    has_lib: bool,
    dependencies: Vec<Dependency>,
}

#[derive(Debug)]
struct Dependency {
    name: String,
    kind: DependencyKind,
    source: Option<String>,
    req: String,
    path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DependencyKind {
    Normal,
    Build,
    Development,
    Other,
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
    path_dependency_findings: Vec<String>,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut check = false;
    let mut policy = PathBuf::from("policy/crate-boundaries.toml");
    let mut json_out = None;
    let mut md_out = None;
    let mut allow_current_names = true;
    let mut iter = args.into_iter().skip(1);

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--check" => check = true,
            "--policy" => {
                policy = PathBuf::from(next_value(&mut iter, "--policy")?);
            }
            "--json-out" => {
                json_out = Some(PathBuf::from(next_value(&mut iter, "--json-out")?));
            }
            "--md-out" => {
                md_out = Some(PathBuf::from(next_value(&mut iter, "--md-out")?));
            }
            "--allow-current-names" => allow_current_names = true,
            "--strict-final-names" => allow_current_names = false,
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

fn next_value(iter: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    iter.next()
        .ok_or_else(|| format!("{flag} requires a path\n{}", usage()))
}

fn usage() -> String {
    "Usage: package-surface [--check] [--policy <path>] [--json-out <path>] [--md-out <path>] [--allow-current-names] [--strict-final-names]".to_string()
}

fn parse_policy(path: &Path) -> Result<Policy, String> {
    let content = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;

    let public_packages = parse_section_array(&content, "public", "packages")?;
    let internal_packages = parse_section_array(&content, "internal", "packages")?;
    let current_public_names = parse_current_public_names(&content);
    let collapse = parse_collapse_entries(&content)?;

    Ok(Policy {
        public_packages: public_packages.into_iter().collect(),
        internal_packages: internal_packages.into_iter().collect(),
        collapse,
        current_public_names,
    })
}

fn parse_section_array(content: &str, section: &str, key: &str) -> Result<Vec<String>, String> {
    let header = format!("[{section}]");
    let start = content
        .find(&header)
        .ok_or_else(|| format!("policy missing {header}"))?;
    let rest = &content[start + header.len()..];
    let end = rest.find("\n[").map_or(rest.len(), |index| index + 1);
    parse_array_values(&rest[..end], key)
}

fn parse_array_values(block: &str, key: &str) -> Result<Vec<String>, String> {
    let marker = format!("{key} = [");
    let start = block
        .find(&marker)
        .ok_or_else(|| format!("policy missing {key} array"))?;
    let after = &block[start + marker.len()..];
    let end = after
        .find(']')
        .ok_or_else(|| format!("policy {key} array is not closed"))?;
    let mut values = Vec::new();
    for line in after[..end].lines() {
        let trimmed = line.trim().trim_end_matches(',').trim();
        if let Some(value) = parse_quoted(trimmed) {
            values.push(value);
        }
    }
    Ok(values)
}

fn parse_current_public_names(content: &str) -> BTreeMap<String, String> {
    let mut aliases = BTreeMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("{ current = ") {
            continue;
        }
        let current = parse_inline_field(trimmed, "current");
        let target = parse_inline_field(trimmed, "target");
        if let (Some(current), Some(target)) = (current, target) {
            aliases.insert(current, target);
        }
    }
    aliases
}

fn parse_inline_field(line: &str, key: &str) -> Option<String> {
    let marker = format!("{key} = ");
    let start = line.find(&marker)?;
    let after = &line[start + marker.len()..];
    parse_quoted(after.trim_start())
}

fn parse_collapse_entries(content: &str) -> Result<BTreeMap<String, CollapseEntry>, String> {
    let mut entries = BTreeMap::new();
    for block in content.split("[[collapse]]").skip(1) {
        let from = parse_assignment(block, "from")
            .ok_or_else(|| "collapse entry missing from".to_string())?;
        let to =
            parse_assignment(block, "to").ok_or_else(|| format!("collapse {from} missing to"))?;
        let owner = parse_assignment(block, "owner")
            .ok_or_else(|| format!("collapse {from} missing owner"))?;
        let reason = parse_assignment(block, "reason")
            .ok_or_else(|| format!("collapse {from} missing reason"))?;
        let transitional_note = parse_assignment(block, "transitional_note");
        entries.insert(
            from,
            CollapseEntry {
                to,
                owner,
                reason,
                transitional_note,
            },
        );
    }
    Ok(entries)
}

fn parse_assignment(block: &str, key: &str) -> Option<String> {
    let marker = format!("{key} = ");
    for line in block.lines() {
        let trimmed = line.trim();
        if let Some(after) = trimmed.strip_prefix(&marker) {
            return parse_quoted(after);
        }
    }
    None
}

fn parse_quoted(value: &str) -> Option<String> {
    let rest = value.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn cargo_metadata() -> Result<Value, String> {
    let output = Command::new("cargo")
        .args(["metadata", "--locked", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|error| format!("failed to run cargo metadata: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed with status {}:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("failed to parse cargo metadata JSON: {error}"))
}

fn workspace_packages(metadata: &Value) -> Result<Vec<WorkspacePackage>, String> {
    let packages = metadata
        .get("packages")
        .and_then(Value::as_array)
        .ok_or_else(|| "cargo metadata missing packages".to_string())?;
    let mut result = Vec::new();
    for package in packages {
        let name = string_field(package, "name")?;
        let publish = parse_publish(package);
        let features = package
            .get("features")
            .and_then(Value::as_object)
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default();
        let has_lib = package
            .get("targets")
            .and_then(Value::as_array)
            .is_some_and(|targets| targets.iter().any(is_lib_target));
        let dependencies = parse_dependencies(package)?;
        result.push(WorkspacePackage {
            name,
            publish,
            features,
            has_lib,
            dependencies,
        });
    }
    result.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(result)
}

fn string_field(value: &Value, key: &str) -> Result<String, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("cargo metadata package missing {key}"))
}

fn parse_publish(package: &Value) -> Option<Vec<String>> {
    match package.get("publish") {
        Some(Value::Null) | None => None,
        Some(Value::Array(values)) => Some(
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect(),
        ),
        _ => None,
    }
}

fn is_lib_target(target: &Value) -> bool {
    target
        .get("kind")
        .and_then(Value::as_array)
        .is_some_and(|kinds| kinds.iter().any(|kind| kind.as_str() == Some("lib")))
}

fn parse_dependencies(package: &Value) -> Result<Vec<Dependency>, String> {
    let dependencies = package
        .get("dependencies")
        .and_then(Value::as_array)
        .ok_or_else(|| "cargo metadata package missing dependencies".to_string())?;
    let mut result = Vec::new();
    for dep in dependencies {
        let name = string_field(dep, "name")?;
        let source = dep
            .get("source")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let req = dep
            .get("req")
            .and_then(Value::as_str)
            .unwrap_or("*")
            .to_string();
        let path = dep
            .get("path")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let kind = match dep.get("kind").and_then(Value::as_str) {
            None => DependencyKind::Normal,
            Some("build") => DependencyKind::Build,
            Some("dev") => DependencyKind::Development,
            Some(_) => DependencyKind::Other,
        };
        result.push(Dependency {
            name,
            kind,
            source,
            req,
            path,
        });
    }
    Ok(result)
}

fn workspace_publish_allow(metadata: &Value) -> BTreeSet<String> {
    metadata
        .get("metadata")
        .and_then(|metadata| metadata.get("publish"))
        .and_then(|publish| publish.get("allow"))
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn validate(
    policy_path: &Path,
    policy: &Policy,
    metadata: &Value,
    allow_current_names: bool,
) -> Result<Report, String> {
    let packages = workspace_packages(metadata)?;
    let package_names = packages
        .iter()
        .map(|p| p.name.clone())
        .collect::<BTreeSet<_>>();
    let publish_allow = workspace_publish_allow(metadata);
    let mut violations = Vec::new();
    let mut warnings = Vec::new();
    let mut path_dependency_findings = Vec::new();

    for allowed in publish_allow.difference(&policy.public_packages) {
        violations.push(format!(
            "workspace publish allowlist contains {allowed}, but policy public packages do not"
        ));
    }
    for public in policy.public_packages.difference(&publish_allow) {
        violations.push(format!(
            "policy public package {public} is missing from [workspace.metadata.publish].allow"
        ));
    }

    if metadata
        .get("workspace_default_members")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty)
    {
        warnings.push("workspace.default-members is missing".to_string());
    }

    let public_current_names = effective_public_names(policy, allow_current_names);
    let classified = classified_names(policy, &public_current_names);

    for package in &packages {
        if !classified.contains(&package.name) {
            violations.push(format!(
                "workspace package {} is not public, internal, collapse, or temporary",
                package.name
            ));
        }

        if policy.internal_packages.contains(&package.name)
            && package
                .publish
                .as_ref()
                .is_none_or(|values| !values.is_empty())
        {
            violations.push(format!(
                "internal package {} must set publish = false",
                package.name
            ));
        }

        if let Some(entry) = policy.collapse.get(&package.name) {
            if package
                .publish
                .as_ref()
                .is_none_or(|values| !values.is_empty())
                && entry.transitional_note.is_none()
            {
                violations.push(format!(
                    "collapse package {} is publishable and lacks a transitional_note",
                    package.name
                ));
            }
            if entry.reason.trim().is_empty()
                || entry.owner.trim().is_empty()
                || entry.to.trim().is_empty()
            {
                violations.push(format!(
                    "collapse package {} has an incomplete policy entry",
                    package.name
                ));
            }
        }

        if package
            .publish
            .as_ref()
            .is_none_or(|values| !values.is_empty())
            && !public_current_names.contains(&package.name)
            && !policy.collapse.contains_key(&package.name)
        {
            warnings.push(format!(
                "package {} has publish=true/default but is not in the public allowlist",
                package.name
            ));
        }

        if package.name.starts_with("racing-wheel-") {
            warnings.push(format!(
                "package {} still uses the pre-naming-spine racing-wheel-* prefix",
                package.name
            ));
        }

        if public_current_names.contains(&package.name) && package.features.len() > 12 {
            warnings.push(format!(
                "public package {} has {} features; review whether feature surface is too broad",
                package.name,
                package.features.len()
            ));
        }

        for feature in &package.features {
            if policy.collapse.contains_key(feature) {
                warnings.push(format!(
                    "package {} has feature {feature}, which matches a former microcrate name",
                    package.name
                ));
            }
        }
    }

    for package in &packages {
        let is_public = public_current_names.contains(&package.name);
        if !is_public {
            continue;
        }
        for dep in package
            .dependencies
            .iter()
            .filter(|dep| dep.kind == DependencyKind::Normal || dep.kind == DependencyKind::Build)
        {
            if dep.source.is_none() && dep.path.is_some() && dep.req == "*" {
                let finding = format!(
                    "public package {} has path-only {:?} dependency {} without a version requirement",
                    package.name, dep.kind, dep.name
                );
                if allow_current_names {
                    warnings.push(format!("transitional: {finding}"));
                } else {
                    violations.push(finding.clone());
                }
                path_dependency_findings.push(finding);
            }
            if package.has_lib
                && policy.internal_packages.contains(&dep.name)
                && dep.name != "workspace-hack"
            {
                violations.push(format!(
                    "public library package {} depends on internal/tool/test package {}",
                    package.name, dep.name
                ));
            }
        }
    }

    let workspace_members = package_names.into_iter().collect::<Vec<_>>();
    let publishable_packages = packages
        .iter()
        .filter(|package| {
            package
                .publish
                .as_ref()
                .is_none_or(|values| !values.is_empty())
        })
        .map(|package| package.name.clone())
        .collect::<Vec<_>>();
    let public_packages = packages
        .iter()
        .filter(|package| public_current_names.contains(&package.name))
        .map(|package| package.name.clone())
        .collect::<Vec<_>>();
    let internal_packages = packages
        .iter()
        .filter(|package| policy.internal_packages.contains(&package.name))
        .map(|package| package.name.clone())
        .collect::<Vec<_>>();
    let collapse_packages = packages
        .iter()
        .filter(|package| policy.collapse.contains_key(&package.name))
        .map(|package| package.name.clone())
        .collect::<Vec<_>>();

    Ok(Report {
        success: violations.is_empty(),
        generated_at_utc: generated_at_utc(),
        policy: policy_path.display().to_string(),
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

fn effective_public_names(policy: &Policy, allow_current_names: bool) -> BTreeSet<String> {
    let mut names = policy.public_packages.clone();
    if allow_current_names {
        names.extend(policy.current_public_names.keys().cloned());
    }
    names
}

fn classified_names(policy: &Policy, public_current_names: &BTreeSet<String>) -> BTreeSet<String> {
    let mut names = public_current_names.clone();
    names.extend(policy.internal_packages.iter().cloned());
    names.extend(policy.collapse.keys().cloned());
    names
}

fn generated_at_utc() -> String {
    match Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
    {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => "unknown".to_string(),
    }
}

fn write_reports(
    report: &Report,
    json_out: &Option<PathBuf>,
    md_out: &Option<PathBuf>,
) -> Result<(), String> {
    if let Some(path) = json_out {
        write_parent(path)?;
        let json = serde_json::to_string_pretty(report)
            .map_err(|error| format!("failed to render JSON report: {error}"))?;
        fs::write(path, format!("{json}\n"))
            .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
    }
    if let Some(path) = md_out {
        write_parent(path)?;
        fs::write(path, markdown_report(report))
            .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
    }
    Ok(())
}

fn write_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    Ok(())
}

fn markdown_report(report: &Report) -> String {
    let mut out = String::new();
    out.push_str("# Package Surface Report\n\n");
    out.push_str(&format!("- Success: `{}`\n", report.success));
    out.push_str(&format!("- Generated: `{}`\n", report.generated_at_utc));
    out.push_str(&format!("- Policy: `{}`\n\n", report.policy));
    append_list(&mut out, "Violations", &report.violations);
    append_list(&mut out, "Warnings", &report.warnings);
    append_list(&mut out, "Public Packages", &report.public_packages);
    append_list(&mut out, "Internal Packages", &report.internal_packages);
    append_list(&mut out, "Collapse Packages", &report.collapse_packages);
    append_list(
        &mut out,
        "Path Dependency Findings",
        &report.path_dependency_findings,
    );
    out
}

fn append_list(out: &mut String, heading: &str, values: &[String]) {
    out.push_str(&format!("## {heading}\n\n"));
    if values.is_empty() {
        out.push_str("- _none_\n\n");
        return;
    }
    for value in values {
        out.push_str(&format!("- {value}\n"));
    }
    out.push('\n');
}

fn print_report(report: &Report) {
    stdout_line(format_args!(
        "package-surface: {} violation(s), {} warning(s)",
        report.violations.len(),
        report.warnings.len()
    ));
    for violation in &report.violations {
        stderr_line(format_args!("VIOLATION: {violation}"));
    }
    for warning in &report.warnings {
        stderr_line(format_args!("WARNING: {warning}"));
    }
}

fn run(args: Args) -> Result<std::process::ExitCode, String> {
    let policy = parse_policy(&args.policy)?;
    let metadata = cargo_metadata()?;
    let report = validate(&args.policy, &policy, &metadata, args.allow_current_names)?;
    write_reports(&report, &args.json_out, &args.md_out)?;
    print_report(&report);

    if args.check && !report.success {
        return Ok(std::process::ExitCode::from(1));
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
            stderr_line(format_args!("ERROR: {error}"));
            std::process::ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_policy_sections() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let policy_path = temp.path().join("policy.toml");
        fs::write(
            &policy_path,
            r#"
schema_version = 1
[public]
packages = ["openracing", "wheelctl"]
[internal]
packages = ["openracing-tools"]
[transitional]
current_public_names = [
  { current = "racing-wheel-engine", target = "openracing-engine" },
]
[[collapse]]
from = "openracing-filters"
to = "openracing-ffb::filters"
owner = "openracing-ffb"
reason = "FFB implementation kernel."
transitional_note = "Temporary."
"#,
        )?;

        let policy = parse_policy(&policy_path)?;

        assert!(policy.public_packages.contains("openracing"));
        assert!(policy.internal_packages.contains("openracing-tools"));
        assert_eq!(
            policy.current_public_names.get("racing-wheel-engine"),
            Some(&"openracing-engine".to_string())
        );
        assert!(policy.collapse.contains_key("openracing-filters"));
        Ok(())
    }

    #[test]
    fn detects_unclassified_workspace_package() -> Result<(), Box<dyn std::error::Error>> {
        let policy = Policy {
            public_packages: BTreeSet::from(["openracing".to_string()]),
            internal_packages: BTreeSet::new(),
            collapse: BTreeMap::new(),
            current_public_names: BTreeMap::new(),
        };
        let metadata = serde_json::json!({
            "metadata": { "publish": { "allow": ["openracing"] } },
            "workspace_default_members": ["path+file:///repo/openracing#openracing@0.1.0"],
            "packages": [{
                "name": "surprise",
                "publish": null,
                "features": {},
                "targets": [],
                "dependencies": []
            }]
        });

        let report = validate(
            Path::new("policy/crate-boundaries.toml"),
            &policy,
            &metadata,
            true,
        )?;

        assert!(!report.success);
        assert!(
            report
                .violations
                .iter()
                .any(|violation| violation.contains("surprise"))
        );
        Ok(())
    }

    #[test]
    fn flags_internal_package_without_publish_false() -> Result<(), Box<dyn std::error::Error>> {
        let policy = Policy {
            public_packages: BTreeSet::from(["openracing".to_string()]),
            internal_packages: BTreeSet::from(["openracing-tools".to_string()]),
            collapse: BTreeMap::new(),
            current_public_names: BTreeMap::new(),
        };
        let metadata = serde_json::json!({
            "metadata": { "publish": { "allow": ["openracing"] } },
            "workspace_default_members": ["path+file:///repo/openracing#openracing@0.1.0"],
            "packages": [{
                "name": "openracing-tools",
                "publish": null,
                "features": {},
                "targets": [],
                "dependencies": []
            }]
        });

        let report = validate(
            Path::new("policy/crate-boundaries.toml"),
            &policy,
            &metadata,
            true,
        )?;

        assert!(!report.success);
        assert!(
            report
                .violations
                .iter()
                .any(|violation| violation.contains("publish = false"))
        );
        Ok(())
    }

    #[test]
    fn parse_args_accepts_report_outputs() -> Result<(), Box<dyn std::error::Error>> {
        let args = parse_args(vec![
            "package-surface".to_string(),
            "--check".to_string(),
            "--policy".to_string(),
            "policy/custom.toml".to_string(),
            "--json-out".to_string(),
            "target/report.json".to_string(),
            "--md-out".to_string(),
            "target/report.md".to_string(),
        ])?;

        assert!(args.check);
        assert_eq!(args.policy, PathBuf::from("policy/custom.toml"));
        assert_eq!(args.json_out, Some(PathBuf::from("target/report.json")));
        assert_eq!(args.md_out, Some(PathBuf::from("target/report.md")));
        Ok(())
    }
}
