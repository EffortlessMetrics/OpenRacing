use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("policy check failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args();
    let _program = args.next();
    let command = match args.next() {
        Some(command) => command,
        None => "check-lint-policy".to_owned(),
    };

    match command.as_str() {
        "check-lint-policy" => check_lint_policy(),
        "policy-report" => policy_report(),
        _ => Err(format!(
            "unknown xtask command '{command}'. Supported commands: check-lint-policy, policy-report"
        )
        .into()),
    }
}

fn check_lint_policy() -> Result<(), Box<dyn Error>> {
    let root = repo_root()?;
    let cargo_toml = read_to_string(root.join("Cargo.toml"))?;
    let policy_toml = read_to_string(root.join("policy/clippy-lints.toml"))?;
    let clippy_toml = read_to_string(root.join("clippy.toml"))?;
    let debt_toml = read_to_string(root.join("policy/clippy-debt.toml"))?;

    let mut errors = Vec::new();

    let workspace_msrv = value_for_key(&cargo_toml, "rust-version");
    let policy_msrv = value_for_key(&policy_toml, "msrv");
    if workspace_msrv != policy_msrv {
        errors.push(format!(
            "workspace MSRV ({}) does not match policy MSRV ({})",
            display_opt(&workspace_msrv),
            display_opt(&policy_msrv)
        ));
    }

    let cargo_lints = parse_workspace_lints(&cargo_toml);
    let policy_lints = parse_policy_lints(&policy_toml);
    let active_policy_lints = policy_lints.active_lints();
    if cargo_lints != active_policy_lints {
        let cargo_only = difference(&cargo_lints, &active_policy_lints);
        let policy_only = difference(&active_policy_lints, &cargo_lints);
        if !cargo_only.is_empty() {
            errors.push(format!(
                "active lints in Cargo.toml but not policy/clippy-lints.toml: {}",
                cargo_only.join(", ")
            ));
        }
        if !policy_only.is_empty() {
            errors.push(format!(
                "active lints in policy/clippy-lints.toml but not Cargo.toml: {}",
                policy_only.join(", ")
            ));
        }
    }

    let members = parse_workspace_members(&cargo_toml);
    for member in members {
        let manifest = root.join(&member).join("Cargo.toml");
        if !manifest.exists() {
            errors.push(format!("workspace member '{member}' is missing Cargo.toml"));
            continue;
        }
        match read_to_string(&manifest) {
            Ok(contents) => {
                if !has_workspace_lints(&contents) {
                    errors.push(format!(
                        "workspace member '{}' does not inherit [lints] workspace = true",
                        manifest.display()
                    ));
                }
            }
            Err(error) => errors.push(format!("failed to read {}: {error}", manifest.display())),
        }
    }

    let clippy_policy_text = uncommented_text(&clippy_toml);
    for forbidden in [
        "allow-unwrap-in-tests",
        "allow-expect-in-tests",
        "allow-panic-in-tests",
        "allow-indexing-slicing-in-tests",
        "allow-dbg-in-tests",
    ] {
        if clippy_policy_text.contains(forbidden) {
            errors.push(format!(
                "clippy.toml must not contain test carveout '{forbidden}'"
            ));
        }
    }

    let msrv = policy_msrv.unwrap_or_default();
    for planned in policy_lints.planned {
        if version_less_than(&msrv, &planned.activate_when_msrv)
            && cargo_lints.contains(&planned.name)
        {
            errors.push(format!(
                "planned lint '{}' is active before MSRV {}",
                planned.name, planned.activate_when_msrv
            ));
        }
    }

    validate_debt(&debt_toml, &mut errors)?;

    if errors.is_empty() {
        println!(
            "lint policy OK: {} active lints governed",
            cargo_lints.len()
        );
        Ok(())
    } else {
        Err(errors.join("\n").into())
    }
}

fn policy_report() -> Result<(), Box<dyn Error>> {
    let root = repo_root()?;
    let policy_toml = read_to_string(root.join("policy/clippy-lints.toml"))?;
    let debt_toml = read_to_string(root.join("policy/clippy-debt.toml"))?;
    let no_panic_toml = read_to_string(root.join("policy/no-panic-allowlist.toml"))?;
    let non_rust_toml = read_to_string(root.join("policy/non-rust-allowlist.toml"))?;
    let policy = parse_policy_lints(&policy_toml);

    println!("lint policy report");
    println!("  active lints: {}", policy.active_lints().len());
    println!("  planned lints: {}", policy.planned.len());
    println!("  debt entries: {}", count_tables(&debt_toml, "debt"));
    println!(
        "  no-panic allowlist entries: {}",
        count_tables(&no_panic_toml, "allow")
    );
    println!(
        "  non-rust allowlist entries: {}",
        count_tables(&non_rust_toml, "allow")
    );
    Ok(())
}

fn repo_root() -> Result<PathBuf, Box<dyn Error>> {
    let current = std::env::current_dir()?;
    if current.join("Cargo.toml").exists() {
        Ok(current)
    } else {
        Err("run xtask from the repository root".into())
    }
}

fn read_to_string(path: impl AsRef<Path>) -> Result<String, Box<dyn Error>> {
    Ok(fs::read_to_string(path)?)
}

fn parse_workspace_lints(contents: &str) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    let mut section = String::new();
    for raw_line in contents.lines() {
        let line = strip_comment(raw_line).trim();
        match line {
            "[workspace.lints.rust]" => {
                section = "rust".to_owned();
                continue;
            }
            "[workspace.lints.clippy]" => {
                section = "clippy".to_owned();
                continue;
            }
            _ if line.starts_with('[') => section.clear(),
            _ => {}
        }
        if section.is_empty() || line.is_empty() {
            continue;
        }
        if let Some((key, _value)) = line.split_once('=') {
            let key = key.trim();
            if section == "clippy" {
                result.insert(format!("clippy::{key}"));
            } else {
                result.insert(key.to_owned());
            }
        }
    }
    result
}

#[derive(Default)]
struct PolicyLints {
    lints: Vec<PolicyLint>,
    planned: Vec<PlannedLint>,
}

#[derive(Default)]
struct PolicyLint {
    name: String,
    status: String,
}

#[derive(Default)]
struct PlannedLint {
    name: String,
    activate_when_msrv: String,
}

impl PolicyLints {
    fn active_lints(&self) -> BTreeSet<String> {
        let mut result = BTreeSet::new();
        for lint in &self.lints {
            if lint.status == "active" {
                result.insert(lint.name.clone());
            }
        }
        result
    }
}

fn parse_policy_lints(contents: &str) -> PolicyLints {
    let mut policy = PolicyLints::default();
    let mut current = BTreeMap::new();
    let mut in_lint = false;

    for raw_line in contents.lines() {
        let line = strip_comment(raw_line).trim();
        if line == "[[lint]]" {
            finish_policy_lint(&mut policy, &mut current, in_lint);
            in_lint = true;
            continue;
        }
        if !in_lint || line.is_empty() || line.starts_with('[') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            current.insert(key.trim().to_owned(), trim_toml_string(value));
        }
    }
    finish_policy_lint(&mut policy, &mut current, in_lint);
    policy
}

fn finish_policy_lint(
    policy: &mut PolicyLints,
    current: &mut BTreeMap<String, String>,
    should_finish: bool,
) {
    if !should_finish {
        return;
    }
    let name = current.remove("name").unwrap_or_default();
    let status = current.remove("status").unwrap_or_default();
    let activate_when_msrv = current.remove("activate_when_msrv").unwrap_or_default();
    if status == "planned" {
        policy.planned.push(PlannedLint {
            name,
            activate_when_msrv,
        });
    } else {
        policy.lints.push(PolicyLint { name, status });
    }
    current.clear();
}

fn parse_workspace_members(contents: &str) -> Vec<String> {
    let mut members = Vec::new();
    let mut in_members = false;
    for raw_line in contents.lines() {
        let line = strip_comment(raw_line).trim();
        if line.starts_with("members") && line.contains('[') {
            in_members = true;
        }
        if in_members {
            for part in line.split(',') {
                let member = trim_toml_string(part);
                if !member.is_empty() && member != "members = [" && member != "]" {
                    members.push(member);
                }
            }
            if line.contains(']') {
                break;
            }
        }
    }
    members
}

fn has_workspace_lints(contents: &str) -> bool {
    let mut in_lints = false;
    for raw_line in contents.lines() {
        let line = strip_comment(raw_line).trim();
        match line {
            "[lints]" => in_lints = true,
            _ if line.starts_with('[') => in_lints = false,
            "workspace = true" if in_lints => return true,
            _ => {}
        }
    }
    false
}

fn validate_debt(contents: &str, errors: &mut Vec<String>) -> Result<(), Box<dyn Error>> {
    let today = today_yyyy_mm_dd()?;
    let mut current = BTreeMap::new();
    let mut seen = false;
    for raw_line in contents.lines() {
        let line = strip_comment(raw_line).trim();
        if line == "[[debt]]" {
            finish_debt(&mut current, errors, &today, seen);
            seen = true;
            continue;
        }
        if !seen || line.is_empty() || line.starts_with('[') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            current.insert(key.trim().to_owned(), trim_toml_string(value));
        }
    }
    finish_debt(&mut current, errors, &today, seen);
    Ok(())
}

fn finish_debt(
    current: &mut BTreeMap<String, String>,
    errors: &mut Vec<String>,
    today: &str,
    should_finish: bool,
) {
    if !should_finish {
        return;
    }
    for required in ["lint", "path", "owner", "reason", "expires"] {
        if current.get(required).is_none_or(String::is_empty) {
            errors.push(format!("clippy debt entry missing '{required}'"));
        }
    }
    if let Some(expires) = current.get("expires")
        && expires.as_str() <= today
    {
        let lint = current.get("lint").map_or("<unknown>", String::as_str);
        let path = current.get("path").map_or("<unknown>", String::as_str);
        errors.push(format!(
            "clippy debt entry for {lint} at {path} expired on {expires}"
        ));
    }
    current.clear();
}

fn value_for_key(contents: &str, target: &str) -> Option<String> {
    for raw_line in contents.lines() {
        let line = strip_comment(raw_line).trim();
        if let Some((key, value)) = line.split_once('=')
            && key.trim() == target
        {
            return Some(trim_toml_string(value));
        }
    }
    None
}

fn trim_toml_string(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(',')
        .trim()
        .trim_matches('"')
        .to_owned()
}

fn strip_comment(line: &str) -> &str {
    line.split_once('#')
        .map_or(line, |(before, _comment)| before)
}

fn uncommented_text(contents: &str) -> String {
    contents
        .lines()
        .map(strip_comment)
        .collect::<Vec<_>>()
        .join("\n")
}

fn display_opt(value: &Option<String>) -> &str {
    match value.as_deref() {
        Some(value) => value,
        None => "<missing>",
    }
}

fn difference(left: &BTreeSet<String>, right: &BTreeSet<String>) -> Vec<String> {
    left.difference(right).cloned().collect()
}

fn version_less_than(left: &str, right: &str) -> bool {
    let mut left_parts = left.split('.').filter_map(|part| part.parse::<u64>().ok());
    let mut right_parts = right.split('.').filter_map(|part| part.parse::<u64>().ok());
    loop {
        let left_part = left_parts.next();
        let right_part = right_parts.next();
        match (left_part, right_part) {
            (Some(left_value), Some(right_value)) if left_value < right_value => return true,
            (Some(left_value), Some(right_value)) if left_value > right_value => return false,
            (Some(_), Some(_)) => {}
            (None, Some(right_value)) if right_value > 0 => return true,
            (Some(left_value), None) if left_value > 0 => return false,
            (None, None) => return false,
            _ => {}
        }
    }
}

fn count_tables(contents: &str, name: &str) -> usize {
    let marker = format!("[[{name}]]");
    contents
        .lines()
        .filter(|line| strip_comment(line).trim() == marker)
        .count()
}

fn today_yyyy_mm_dd() -> Result<String, Box<dyn Error>> {
    let duration = SystemTime::now().duration_since(UNIX_EPOCH)?;
    let days = duration.as_secs() / 86_400;
    let days = i64::try_from(days)?;
    let (year, month, day) = civil_from_days(days);
    Ok(format!("{year:04}-{month:02}-{day:02}"))
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "civil date conversion operates on bounded UNIX-day values for expiry checks"
)]
fn civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year, m, d)
}
