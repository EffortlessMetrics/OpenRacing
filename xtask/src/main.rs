use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use glob::Pattern;
use serde::Deserialize;
use toml::Value;
use walkdir::{DirEntry, WalkDir};

const ROOT_CARGO: &str = "Cargo.toml";
const CLIPPY_POLICY: &str = "policy/clippy-lints.toml";
const CLIPPY_DEBT: &str = "policy/clippy-debt.toml";
const NO_PANIC_ALLOWLIST: &str = "policy/no-panic-allowlist.toml";
const NON_RUST_ALLOWLIST: &str = "policy/non-rust-allowlist.toml";
const CLIPPY_CONFIG: &str = "clippy.toml";

const TEST_CARVEOUTS: &[&str] = &[
    "allow-unwrap-in-tests",
    "allow-expect-in-tests",
    "allow-panic-in-tests",
    "allow-indexing-slicing-in-tests",
    "allow-dbg-in-tests",
];

const PANIC_FAMILIES: &[(&str, &[&str])] = &[
    ("unwrap", &[".unwrap("]),
    ("expect", &[".expect("]),
    ("panic", &["panic!("]),
    ("todo", &["todo!("]),
    ("unimplemented", &["unimplemented!("]),
    ("unreachable", &["unreachable!("]),
];

const NON_RUST_EXTENSIONS: &[&str] = &[
    "c", "h", "hpp", "js", "nix", "ps1", "py", "sh", "ts", "tsx", "wxs",
];

#[derive(Debug, Deserialize)]
struct ClippyLedger {
    schema: u32,
    msrv: String,
    policy: ClippyPolicy,
    #[serde(default)]
    lint: Vec<LintEntry>,
    #[serde(default)]
    planned: Vec<PlannedLint>,
}

#[derive(Debug, Deserialize)]
struct ClippyPolicy {
    panic_free_tests: bool,
    allow_test_carveouts: bool,
    suppression_style: String,
    blanket_categories: bool,
}

#[derive(Debug, Deserialize)]
struct LintEntry {
    name: String,
    level: String,
    status: String,
    class: String,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct PlannedLint {
    name: String,
    level: String,
    activate_when_msrv: String,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct ClippyDebtLedger {
    schema: u32,
    #[serde(default)]
    debt: Vec<DebtEntry>,
}

#[derive(Debug, Deserialize)]
struct DebtEntry {
    lint: String,
    path: String,
    owner: String,
    reason: String,
    expires: String,
}

#[derive(Debug, Deserialize)]
struct NoPanicAllowlist {
    schema_version: String,
    #[serde(default)]
    allow: Vec<PanicAllow>,
}

#[derive(Debug, Deserialize)]
struct PanicAllow {
    path: String,
    family: String,
    classification: String,
    owner: String,
    explanation: String,
    expires: Option<String>,
    selector: PanicSelector,
    last_seen: Option<LastSeen>,
}

#[derive(Debug, Deserialize)]
struct PanicSelector {
    kind: String,
    container: Option<String>,
    callee: Option<String>,
    receiver_fingerprint: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LastSeen {
    line: u32,
    column: u32,
}

#[derive(Debug, Deserialize)]
struct NonRustAllowlist {
    schema_version: String,
    #[serde(default)]
    allow: Vec<NonRustAllow>,
}

#[derive(Debug, Deserialize)]
struct NonRustAllow {
    path: Option<String>,
    glob: Option<String>,
    kind: String,
    owner: String,
    reason: String,
    surface: String,
    classification: String,
    #[serde(default)]
    covered_by: Vec<String>,
    expires: Option<String>,
}

#[derive(Debug)]
struct PanicOccurrence {
    path: String,
    family: String,
    line: usize,
    column: usize,
    selector: String,
}

fn main() -> Result<()> {
    let mut args = env::args();
    let _program = args.next();
    let command = args.next().unwrap_or_else(|| "help".to_owned());

    match command.as_str() {
        "check-lint-policy" => check_lint_policy(),
        "check-no-panic-family" => check_no_panic_family(),
        "check-file-policy" => check_file_policy(),
        "policy-report" => policy_report(),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        unknown => bail!("unknown xtask command `{unknown}`; run `cargo xtask --help`"),
    }
}

fn print_help() {
    println!("Usage: cargo xtask <command>");
    println!();
    println!("Commands:");
    println!("  check-lint-policy       Verify Clippy policy manifest, ledger, and debt schema");
    println!(
        "  check-no-panic-family   Scan Rust files for panic-family calls not in policy allowlist"
    );
    println!("  check-file-policy       Verify non-Rust policy allowlist coverage");
    println!("  policy-report           Print counts for lint, panic, and file policy surfaces");
}

fn check_lint_policy() -> Result<()> {
    let root = read_toml(ROOT_CARGO)?;
    let ledger: ClippyLedger = read_toml(CLIPPY_POLICY)?;
    validate_clippy_ledger_shape(&ledger)?;
    validate_msrv(&root, &ledger.msrv)?;
    validate_workspace_lints(&root, &ledger)?;
    validate_members_inherit_lints(&root)?;
    validate_clippy_config()?;
    validate_clippy_debt()?;
    println!(
        "lint policy OK: {} active lints, {} planned flips",
        ledger.lint.len(),
        ledger.planned.len()
    );
    Ok(())
}

fn check_no_panic_family() -> Result<()> {
    let allowlist: NoPanicAllowlist = read_toml(NO_PANIC_ALLOWLIST)?;
    validate_no_panic_allowlist(&allowlist)?;
    let occurrences = scan_panic_family()?;
    let mut allowed = BTreeSet::new();
    for allow in &allowlist.allow {
        allowed.insert(panic_key(
            &allow.path,
            &allow.family,
            &selector_key(&allow.selector),
        ));
    }

    let mut violations = Vec::new();
    for occurrence in &occurrences {
        let key = panic_key(&occurrence.path, &occurrence.family, &occurrence.selector);
        if !allowed.contains(&key) {
            violations.push(format!(
                "{}:{}:{}: unallowlisted {} occurrence ({})",
                occurrence.path,
                occurrence.line,
                occurrence.column,
                occurrence.family,
                occurrence.selector
            ));
        }
    }

    let mut stale = Vec::new();
    let occurrence_keys: BTreeSet<_> = occurrences
        .iter()
        .map(|occurrence| panic_key(&occurrence.path, &occurrence.family, &occurrence.selector))
        .collect();
    for allow in &allowlist.allow {
        let key = panic_key(&allow.path, &allow.family, &selector_key(&allow.selector));
        if !occurrence_keys.contains(&key) {
            stale.push(format!(
                "{}: stale panic allowlist selector for {}",
                allow.path, allow.family
            ));
        }
    }

    if violations.is_empty() && stale.is_empty() {
        println!(
            "no-panic policy OK: {} occurrences covered by {} allowlist entries",
            occurrences.len(),
            allowlist.allow.len()
        );
        return Ok(());
    }

    for message in violations.iter().chain(stale.iter()) {
        eprintln!("{message}");
    }
    bail!(
        "no-panic policy found {} violation(s) and {} stale allowlist entrie(s)",
        violations.len(),
        stale.len()
    )
}

fn check_file_policy() -> Result<()> {
    let allowlist: NonRustAllowlist = read_toml(NON_RUST_ALLOWLIST)?;
    validate_non_rust_allowlist(&allowlist)?;
    let files = non_rust_programming_files()?;
    let mut uncovered = Vec::new();
    for file in &files {
        if !non_rust_allowed(file, &allowlist.allow)? {
            uncovered.push(file.clone());
        }
    }

    if uncovered.is_empty() {
        println!(
            "file policy OK: {} non-Rust programming file(s) covered",
            files.len()
        );
        return Ok(());
    }

    for file in &uncovered {
        eprintln!(
            "{}: non-Rust programming file lacks policy allowlist coverage",
            file.display()
        );
    }
    bail!(
        "file policy found {} uncovered non-Rust file(s)",
        uncovered.len()
    )
}

fn policy_report() -> Result<()> {
    let ledger: ClippyLedger = read_toml(CLIPPY_POLICY)?;
    let debt: ClippyDebtLedger = read_toml(CLIPPY_DEBT)?;
    let panic_allowlist: NoPanicAllowlist = read_toml(NO_PANIC_ALLOWLIST)?;
    let non_rust_allowlist: NonRustAllowlist = read_toml(NON_RUST_ALLOWLIST)?;
    let panic_occurrences = scan_panic_family()?;
    let non_rust_files = non_rust_programming_files()?;

    println!("clippy active lints: {}", ledger.lint.len());
    println!("clippy planned flips: {}", ledger.planned.len());
    println!("clippy debt entries: {}", debt.debt.len());
    for (class, count) in count_by_class(&ledger) {
        println!("clippy class {class}: {count}");
    }
    println!("panic-family occurrences: {}", panic_occurrences.len());
    println!("panic allowlist entries: {}", panic_allowlist.allow.len());
    println!("non-Rust programming files: {}", non_rust_files.len());
    println!(
        "non-Rust allowlist entries: {}",
        non_rust_allowlist.allow.len()
    );
    Ok(())
}

fn read_toml<T>(path: &str) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let contents = fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
    toml::from_str(&contents).with_context(|| format!("parsing {path}"))
}

fn validate_clippy_ledger_shape(ledger: &ClippyLedger) -> Result<()> {
    if ledger.schema != 1 {
        bail!("{CLIPPY_POLICY}: expected schema = 1");
    }
    if !ledger.policy.panic_free_tests {
        bail!("{CLIPPY_POLICY}: policy.panic_free_tests must be true");
    }
    if ledger.policy.allow_test_carveouts {
        bail!("{CLIPPY_POLICY}: policy.allow_test_carveouts must be false");
    }
    if ledger.policy.suppression_style != "expect-with-reason" {
        bail!("{CLIPPY_POLICY}: policy.suppression_style must be expect-with-reason");
    }
    if ledger.policy.blanket_categories {
        bail!("{CLIPPY_POLICY}: policy.blanket_categories must be false");
    }

    for lint in &ledger.lint {
        require_non_empty(&lint.name, "lint.name")?;
        require_non_empty(&lint.level, &format!("{}.level", lint.name))?;
        require_non_empty(&lint.status, &format!("{}.status", lint.name))?;
        require_non_empty(&lint.class, &format!("{}.class", lint.name))?;
        require_non_empty(&lint.reason, &format!("{}.reason", lint.name))?;
        if lint.status != "active" {
            bail!(
                "{}: only active lint entries belong in [[lint]]; use [[planned]] for future flips",
                lint.name
            );
        }
    }

    for planned in &ledger.planned {
        require_non_empty(&planned.name, "planned.name")?;
        require_non_empty(&planned.level, &format!("{}.level", planned.name))?;
        require_non_empty(
            &planned.activate_when_msrv,
            &format!("{}.activate_when_msrv", planned.name),
        )?;
        require_non_empty(&planned.reason, &format!("{}.reason", planned.name))?;
    }
    Ok(())
}

fn validate_msrv(root: &Value, policy_msrv: &str) -> Result<()> {
    let rust_version = root
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("rust-version"))
        .and_then(Value::as_str)
        .context("workspace.package.rust-version is missing")?;
    if rust_version != policy_msrv {
        bail!(
            "workspace.package.rust-version `{rust_version}` does not match {CLIPPY_POLICY} msrv `{policy_msrv}`"
        );
    }
    Ok(())
}

fn validate_workspace_lints(root: &Value, ledger: &ClippyLedger) -> Result<()> {
    let workspace_lints = root
        .get("workspace")
        .and_then(|workspace| workspace.get("lints"))
        .context("[workspace.lints] is missing from Cargo.toml")?;

    let rust_lints = workspace_lints
        .get("rust")
        .and_then(Value::as_table)
        .context("[workspace.lints.rust] is missing from Cargo.toml")?;
    for (name, level) in [
        ("unsafe_code", "forbid"),
        ("unsafe_op_in_unsafe_fn", "deny"),
        ("unused_must_use", "deny"),
        ("unexpected_cfgs", "warn"),
        ("const_item_interior_mutations", "deny"),
        ("function_casts_as_integer", "deny"),
    ] {
        validate_lint_level(rust_lints, name, level, "workspace.lints.rust")?;
    }

    let clippy_lints = workspace_lints
        .get("clippy")
        .and_then(Value::as_table)
        .context("[workspace.lints.clippy] is missing from Cargo.toml")?;
    for lint in ledger.lint.iter().filter(|lint| lint.status == "active") {
        let clippy_name = lint.name.strip_prefix("clippy::").unwrap_or(&lint.name);
        validate_lint_level(
            clippy_lints,
            clippy_name,
            &lint.level,
            "workspace.lints.clippy",
        )?;
    }

    let msrv = parse_minor_version(&ledger.msrv)?;
    for planned in &ledger.planned {
        let planned_msrv = parse_minor_version(&planned.activate_when_msrv)?;
        if planned_msrv > msrv {
            let clippy_name = planned
                .name
                .strip_prefix("clippy::")
                .unwrap_or(&planned.name);
            if clippy_lints.contains_key(clippy_name) {
                bail!(
                    "{} is planned for MSRV {} but is already active",
                    planned.name,
                    planned.activate_when_msrv
                );
            }
        }
    }
    Ok(())
}

fn validate_lint_level(
    table: &toml::map::Map<String, Value>,
    name: &str,
    expected: &str,
    scope: &str,
) -> Result<()> {
    let actual = table
        .get(name)
        .and_then(Value::as_str)
        .with_context(|| format!("{scope}.{name} is missing"))?;
    if actual != expected {
        bail!("{scope}.{name} = `{actual}` but policy requires `{expected}`");
    }
    Ok(())
}

fn validate_members_inherit_lints(root: &Value) -> Result<()> {
    let members = workspace_members(root)?;
    let mut missing = Vec::new();
    for member in members {
        let manifest = Path::new(&member).join("Cargo.toml");
        let value: Value = read_toml(path_str(&manifest)?)?;
        let inherits = value
            .get("lints")
            .and_then(|lints| lints.get("workspace"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !inherits {
            missing.push(manifest.display().to_string());
        }
    }
    if !missing.is_empty() {
        for manifest in &missing {
            eprintln!("{manifest}: missing [lints] workspace = true");
        }
        bail!(
            "{} workspace member(s) do not inherit workspace lints",
            missing.len()
        );
    }
    Ok(())
}

fn validate_clippy_config() -> Result<()> {
    let contents =
        fs::read_to_string(CLIPPY_CONFIG).with_context(|| format!("reading {CLIPPY_CONFIG}"))?;
    for carveout in TEST_CARVEOUTS {
        if contents
            .lines()
            .any(|line| line.trim_start().starts_with(carveout))
        {
            bail!("{CLIPPY_CONFIG}: test carveout `{carveout}` is forbidden");
        }
    }
    Ok(())
}

fn validate_clippy_debt() -> Result<()> {
    let debt: ClippyDebtLedger = read_toml(CLIPPY_DEBT)?;
    if debt.schema != 1 {
        bail!("{CLIPPY_DEBT}: expected schema = 1");
    }
    for entry in &debt.debt {
        require_non_empty(&entry.lint, "debt.lint")?;
        require_non_empty(&entry.path, "debt.path")?;
        require_non_empty(&entry.owner, "debt.owner")?;
        require_non_empty(&entry.reason, "debt.reason")?;
        require_non_empty(&entry.expires, "debt.expires")?;
        ensure_not_expired(
            &entry.expires,
            &format!("debt {} at {}", entry.lint, entry.path),
        )?;
    }
    Ok(())
}

fn validate_no_panic_allowlist(allowlist: &NoPanicAllowlist) -> Result<()> {
    if allowlist.schema_version != "0.3" {
        bail!("{NO_PANIC_ALLOWLIST}: expected schema_version = 0.3");
    }
    for entry in &allowlist.allow {
        require_non_empty(&entry.path, "allow.path")?;
        require_non_empty(&entry.family, "allow.family")?;
        require_non_empty(&entry.classification, "allow.classification")?;
        require_non_empty(&entry.owner, "allow.owner")?;
        require_non_empty(&entry.explanation, "allow.explanation")?;
        require_non_empty(&entry.selector.kind, "allow.selector.kind")?;
        if let Some(callee) = &entry.selector.callee {
            require_non_empty(callee, "allow.selector.callee")?;
        }
        if let Some(container) = &entry.selector.container {
            require_non_empty(container, "allow.selector.container")?;
        }
        if let Some(receiver_fingerprint) = &entry.selector.receiver_fingerprint {
            require_non_empty(receiver_fingerprint, "allow.selector.receiver_fingerprint")?;
        }
        if let Some(last_seen) = &entry.last_seen
            && (last_seen.line == 0 || last_seen.column == 0)
        {
            bail!("{}: last_seen line and column are 1-indexed", entry.path);
        }
        if let Some(expires) = &entry.expires {
            ensure_not_expired(
                expires,
                &format!("panic allow {} at {}", entry.family, entry.path),
            )?;
        }
    }
    Ok(())
}

fn validate_non_rust_allowlist(allowlist: &NonRustAllowlist) -> Result<()> {
    if allowlist.schema_version != "1.0" {
        bail!("{NON_RUST_ALLOWLIST}: expected schema_version = 1.0");
    }
    for entry in &allowlist.allow {
        if entry.path.is_none() == entry.glob.is_none() {
            bail!("{NON_RUST_ALLOWLIST}: each entry must have exactly one of path or glob");
        }
        require_non_empty(&entry.kind, "allow.kind")?;
        require_non_empty(&entry.owner, "allow.owner")?;
        require_non_empty(&entry.reason, "allow.reason")?;
        require_non_empty(&entry.surface, "allow.surface")?;
        require_non_empty(&entry.classification, "allow.classification")?;
        if matches!(
            entry.classification.as_str(),
            "production" | "test" | "tooling"
        ) && entry.covered_by.is_empty()
        {
            bail!(
                "{}: production/test/tooling non-Rust entries require covered_by",
                entry.kind
            );
        }
        if let Some(expires) = &entry.expires {
            ensure_not_expired(expires, &format!("non-Rust allow {}", entry.kind))?;
        }
        if let Some(glob) = &entry.glob {
            Pattern::new(glob)
                .with_context(|| format!("invalid non-Rust allowlist glob `{glob}`"))?;
        }
    }
    Ok(())
}

fn scan_panic_family() -> Result<Vec<PanicOccurrence>> {
    let mut occurrences = Vec::new();
    for entry in rust_files()? {
        let contents =
            fs::read_to_string(&entry).with_context(|| format!("reading {}", entry.display()))?;
        let path = normalize_path(&entry);
        for (line_index, line) in contents.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            let searchable = strip_string_literals(line);
            for (family, needles) in PANIC_FAMILIES {
                for needle in *needles {
                    if let Some(column_index) = searchable.find(needle) {
                        let selector = if needle.starts_with('.') {
                            format!("method_call:{family}")
                        } else {
                            format!("macro_call:{family}")
                        };
                        occurrences.push(PanicOccurrence {
                            path: path.clone(),
                            family: (*family).to_owned(),
                            line: line_index.saturating_add(1),
                            column: column_index.saturating_add(1),
                            selector,
                        });
                    }
                }
            }
        }
    }
    Ok(occurrences)
}

fn strip_string_literals(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut in_string = false;
    let mut escaped = false;

    for ch in line.chars() {
        if in_string {
            if escaped {
                escaped = false;
                output.push(' ');
                continue;
            }
            if ch == '\\' {
                escaped = true;
                output.push(' ');
                continue;
            }
            if ch == '"' {
                in_string = false;
            }
            output.push(' ');
            continue;
        }

        if ch == '"' {
            in_string = true;
            output.push(' ');
        } else {
            output.push(ch);
        }
    }

    output
}

fn rust_files() -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in repo_walk() {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(entry.path().to_path_buf());
        }
    }
    Ok(files)
}

fn non_rust_programming_files() -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in repo_walk() {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let Some(extension) = entry.path().extension().and_then(|ext| ext.to_str()) else {
            continue;
        };
        if NON_RUST_EXTENSIONS.contains(&extension) {
            files.push(entry.path().to_path_buf());
        }
    }
    Ok(files)
}

fn repo_walk() -> impl Iterator<Item = walkdir::Result<DirEntry>> {
    WalkDir::new(".")
        .into_iter()
        .filter_entry(|entry| !is_ignored_entry(entry))
}

fn is_ignored_entry(entry: &DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }
    let name = entry.file_name().to_string_lossy();
    matches!(
        name.as_ref(),
        ".git" | "target" | ".cargo" | ".idea" | ".vscode" | "third_party" | "fuzz"
    )
}

fn non_rust_allowed(file: &Path, entries: &[NonRustAllow]) -> Result<bool> {
    let normalized = normalize_path(file);
    for entry in entries {
        if let Some(path) = &entry.path
            && path == &normalized
        {
            return Ok(true);
        }
        if let Some(glob) = &entry.glob {
            let pattern = Pattern::new(glob)
                .with_context(|| format!("invalid non-Rust allowlist glob `{glob}`"))?;
            if pattern.matches(&normalized) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn workspace_members(root: &Value) -> Result<Vec<String>> {
    let members = root
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(Value::as_array)
        .context("workspace.members is missing")?;
    let mut result = Vec::new();
    for member in members {
        let member = member
            .as_str()
            .context("workspace.members must contain only strings")?;
        result.push(member.to_owned());
    }
    Ok(result)
}

fn require_non_empty(value: &str, field: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{field} must not be empty");
    }
    Ok(())
}

fn ensure_not_expired(expires: &str, label: &str) -> Result<()> {
    let today = Utc::now().date_naive().to_string();
    if expires < today.as_str() {
        bail!("{label} expired on {expires}");
    }
    Ok(())
}

fn parse_minor_version(version: &str) -> Result<u32> {
    let mut parts = version.split('.');
    let _major = parts.next().context("MSRV is missing major version")?;
    let minor = parts.next().context("MSRV is missing minor version")?;
    minor
        .parse::<u32>()
        .with_context(|| format!("invalid MSRV minor version in `{version}`"))
}

fn selector_key(selector: &PanicSelector) -> String {
    let mut pieces = vec![format!("kind={}", selector.kind)];
    if let Some(container) = &selector.container {
        pieces.push(format!("container={container}"));
    }
    if let Some(callee) = &selector.callee {
        pieces.push(format!("callee={callee}"));
    }
    if let Some(receiver_fingerprint) = &selector.receiver_fingerprint {
        pieces.push(format!("receiver={receiver_fingerprint}"));
    }
    pieces.join(";")
}

fn panic_key(path: &str, family: &str, selector: &str) -> String {
    format!("{path}|{family}|{selector}")
}

fn normalize_path(path: &Path) -> String {
    let stripped = path.strip_prefix(".").unwrap_or(path);
    let normalized = stripped.to_string_lossy().replace('\\', "/");
    normalized
        .strip_prefix("./")
        .unwrap_or(&normalized)
        .to_owned()
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("path is not valid UTF-8: {}", path.display()))
}

fn count_by_class(ledger: &ClippyLedger) -> BTreeMap<&str, usize> {
    let mut counts = BTreeMap::new();
    for lint in &ledger.lint {
        let count = counts.entry(lint.class.as_str()).or_insert(0usize);
        *count = count.saturating_add(1);
    }
    counts
}
