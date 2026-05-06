use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

type XResult<T> = Result<T, Box<dyn Error>>;

const CLIPPY_TEST_CARVEOUTS: &[&str] = &[
    "allow-unwrap-in-tests",
    "allow-expect-in-tests",
    "allow-panic-in-tests",
    "allow-indexing-slicing-in-tests",
    "allow-dbg-in-tests",
];
const REQUIRED_RUST_LINTS: &[(&str, &str)] = &[
    ("unsafe_code", "forbid"),
    ("unsafe_op_in_unsafe_fn", "deny"),
    ("unused_must_use", "deny"),
    ("unexpected_cfgs", "warn"),
    ("const_item_interior_mutations", "deny"),
    ("function_casts_as_integer", "deny"),
];
const REQUIRED_CLIPPY_LINTS: &[(&str, &str)] = &[
    ("dbg_macro", "deny"),
    ("todo", "deny"),
    ("unimplemented", "deny"),
    ("panic", "deny"),
    ("unreachable", "deny"),
    ("unwrap_used", "deny"),
    ("expect_used", "deny"),
    ("get_unwrap", "deny"),
    ("unwrap_in_result", "deny"),
    ("panic_in_result_fn", "deny"),
    ("string_slice", "deny"),
    ("indexing_slicing", "deny"),
    ("out_of_bounds_indexing", "deny"),
    ("unchecked_time_subtraction", "deny"),
    ("char_indices_as_byte_indices", "deny"),
    ("sliced_string_as_bytes", "deny"),
    ("index_refutable_slice", "deny"),
    ("let_underscore_future", "deny"),
    ("let_underscore_must_use", "deny"),
    ("let_underscore_lock", "deny"),
    ("unused_result_ok", "deny"),
    ("map_err_ignore", "deny"),
    ("assertions_on_result_states", "deny"),
    ("lines_filter_map_ok", "deny"),
    ("await_holding_lock", "deny"),
    ("await_holding_refcell_ref", "deny"),
    ("await_holding_invalid_type", "deny"),
    ("future_not_send", "warn"),
    ("large_futures", "warn"),
    ("arc_with_non_send_sync", "deny"),
    ("rc_mutex", "deny"),
    ("mut_mutex_lock", "deny"),
    ("readonly_write_lock", "deny"),
    ("mem_forget", "deny"),
    ("forget_non_drop", "deny"),
    ("drop_non_drop", "deny"),
    ("undocumented_unsafe_blocks", "deny"),
    ("multiple_unsafe_ops_per_block", "deny"),
    ("repr_packed_without_abi", "deny"),
    ("float_cmp", "deny"),
    ("float_cmp_const", "deny"),
    ("float_equality_without_abs", "deny"),
    ("lossy_float_literal", "deny"),
    ("cast_sign_loss", "deny"),
    ("cast_possible_wrap", "warn"),
    ("cast_possible_truncation", "warn"),
    ("cast_precision_loss", "warn"),
    ("invalid_upcast_comparisons", "deny"),
    ("cast_abs_to_unsigned", "deny"),
    ("cast_enum_truncation", "deny"),
    ("cast_nan_to_int", "deny"),
    ("manual_midpoint", "warn"),
    ("manual_is_multiple_of", "warn"),
    ("manual_div_ceil", "warn"),
    ("arithmetic_side_effects", "warn"),
    ("suspicious_open_options", "deny"),
    ("nonsensical_open_options", "deny"),
    ("ineffective_open_options", "deny"),
    ("path_buf_push_overwrite", "deny"),
    ("join_absolute_paths", "deny"),
    ("read_line_without_trim", "warn"),
    ("exit", "deny"),
    ("iter_not_returning_iterator", "deny"),
    ("expl_impl_clone_on_copy", "deny"),
    ("infallible_try_from", "deny"),
    ("fallible_impl_from", "deny"),
    ("error_impl_error", "deny"),
    ("result_unit_err", "warn"),
    ("result_large_err", "warn"),
    ("format_in_format_args", "deny"),
    ("to_string_in_format_args", "deny"),
    ("unused_format_specs", "deny"),
    ("unnecessary_debug_formatting", "warn"),
    ("uninlined_format_args", "warn"),
    ("manual_let_else", "warn"),
    ("manual_ok_or", "warn"),
    ("manual_strip", "warn"),
    ("manual_split_once", "warn"),
    ("manual_is_variant_and", "warn"),
    ("filter_map_next", "warn"),
    ("flat_map_option", "warn"),
    ("match_result_ok", "deny"),
    ("cloned_instead_of_copied", "warn"),
    ("iter_cloned_collect", "warn"),
    ("iter_overeager_cloned", "warn"),
    ("needless_collect", "warn"),
    ("redundant_closure", "warn"),
    ("redundant_closure_for_method_calls", "warn"),
    ("missing_panics_doc", "deny"),
    ("missing_errors_doc", "warn"),
    ("allow_attributes", "deny"),
    ("allow_attributes_without_reason", "deny"),
    ("blanket_clippy_restriction_lints", "deny"),
    ("ignore_without_reason", "deny"),
    ("should_panic_without_expect", "deny"),
];
const PLANNED_LINTS: &[&str] = &[
    "clippy::same_length_and_capacity",
    "clippy::manual_ilog2",
    "clippy::decimal_bitwise_operands",
    "clippy::needless_type_cast",
    "clippy::disallowed_fields",
    "clippy::manual_checked_ops",
    "clippy::manual_take",
    "clippy::manual_pop_if",
    "clippy::duration_suboptimal_units",
    "clippy::unnecessary_trailing_comma",
];
const NON_RUST_EXTENSIONS: &[&str] = &["c", "css", "html", "js", "nix", "ps1", "py", "sh", "wxs"];
const IGNORED_DIRS: &[&str] = &[".git", "target"];

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("policy check failed: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> XResult<()> {
    let mut args = env::args();
    let _program = args.next();
    let command = args.next().unwrap_or_else(|| "help".to_owned());
    match command.as_str() {
        "check-lint-policy" => check_lint_policy(),
        "check-file-policy" => check_file_policy(),
        "check-no-panic-family" => check_no_panic_family(),
        "policy-report" => policy_report(),
        _ => {
            print_help();
            Ok(())
        }
    }
}

fn print_help() {
    println!(
        "usage: cargo xtask <check-lint-policy|check-file-policy|check-no-panic-family|policy-report>"
    );
}

fn check_lint_policy() -> XResult<()> {
    let mut failures = Vec::new();
    let root_manifest = read_to_string(Path::new("Cargo.toml"), &mut failures);
    let clippy_policy = read_to_string(Path::new("policy/clippy-lints.toml"), &mut failures);
    let clippy_debt = read_to_string(Path::new("policy/clippy-debt.toml"), &mut failures);
    let clippy_config = read_to_string(Path::new("clippy.toml"), &mut failures);

    require_contains(
        &root_manifest,
        "rust-version = \"1.93\"",
        "workspace MSRV must be 1.93",
        &mut failures,
    );
    require_contains(
        &clippy_policy,
        "msrv = \"1.93\"",
        "policy MSRV must be 1.93",
        &mut failures,
    );
    for (lint, level) in REQUIRED_RUST_LINTS {
        require_contains(
            &root_manifest,
            &format!("{lint} = \"{level}\""),
            "missing required rust lint",
            &mut failures,
        );
    }
    for (lint, level) in REQUIRED_CLIPPY_LINTS {
        require_contains(
            &root_manifest,
            &format!("{lint} = \"{level}\""),
            "missing required clippy lint",
            &mut failures,
        );
        require_contains(
            &clippy_policy,
            &format!("name = \"clippy::{lint}\""),
            "missing active lint ledger entry",
            &mut failures,
        );
    }
    for lint in PLANNED_LINTS {
        require_contains(
            &clippy_policy,
            &format!("name = \"{lint}\""),
            "missing planned lint ledger entry",
            &mut failures,
        );
        if root_manifest.contains(&format!("{} = ", lint.trim_start_matches("clippy::"))) {
            failures.push(format!(
                "planned lint {lint} is active before the MSRV bump"
            ));
        }
    }
    for carveout in CLIPPY_TEST_CARVEOUTS {
        if clippy_config.contains(carveout) {
            failures.push(format!(
                "clippy.toml contains forbidden test carveout `{carveout}`"
            ));
        }
    }
    require_contains(
        &clippy_policy,
        "panic_free_tests = true",
        "policy must require panic-free tests",
        &mut failures,
    );
    require_contains(
        &clippy_policy,
        "allow_test_carveouts = false",
        "policy must forbid test carveouts",
        &mut failures,
    );
    require_contains(
        &clippy_policy,
        "suppression_style = \"expect-with-reason\"",
        "policy must require expect-with-reason",
        &mut failures,
    );

    for manifest in cargo_manifests()? {
        let manifest_text = fs::read_to_string(&manifest)?;
        if !manifest_text.contains("[lints]\nworkspace = true") {
            failures.push(format!(
                "{} must inherit workspace lints",
                manifest.display()
            ));
        }
    }
    validate_debt_entries(&clippy_debt, "debt", &mut failures);

    finish("check-lint-policy", failures)
}

fn check_file_policy() -> XResult<()> {
    let mut failures = Vec::new();
    let policy_text = read_to_string(Path::new("policy/non-rust-allowlist.toml"), &mut failures);
    let entries = parse_allow_entries(&policy_text);
    validate_allow_entries(&entries, &mut failures);
    for path in repo_files(Path::new("."))? {
        if is_non_rust_programming_file(&path) && !entries.iter().any(|entry| entry.matches(&path))
        {
            failures.push(format!(
                "{} is a non-Rust programming file without policy coverage",
                path.display()
            ));
        }
    }
    finish("check-file-policy", failures)
}

fn check_no_panic_family() -> XResult<()> {
    let mut failures = Vec::new();
    let policy_text = read_to_string(Path::new("policy/no-panic-allowlist.toml"), &mut failures);
    let entries = parse_allow_entries(&policy_text);
    validate_allow_entries(&entries, &mut failures);
    require_contains(
        &policy_text,
        "schema_version = \"0.3\"",
        "no-panic allowlist schema must be 0.3",
        &mut failures,
    );
    finish("check-no-panic-family", failures)
}

fn policy_report() -> XResult<()> {
    check_lint_policy()?;
    check_file_policy()?;
    check_no_panic_family()?;
    println!("policy-report: all policy ledgers are structurally valid");
    Ok(())
}

fn read_to_string(path: &Path, failures: &mut Vec<String>) -> String {
    match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => {
            failures.push(format!("failed to read {}: {err}", path.display()));
            String::new()
        }
    }
}

fn require_contains(haystack: &str, needle: &str, message: &str, failures: &mut Vec<String>) {
    if !haystack.contains(needle) {
        failures.push(format!("{message}: `{needle}`"));
    }
}

fn finish(name: &str, failures: Vec<String>) -> XResult<()> {
    if failures.is_empty() {
        println!("{name}: ok");
        return Ok(());
    }
    Err(failures.join("\n").into())
}

fn cargo_manifests() -> XResult<Vec<PathBuf>> {
    let mut manifests = Vec::new();
    for entry in fs::read_dir("crates")? {
        let manifest = entry?.path().join("Cargo.toml");
        if manifest.exists() {
            manifests.push(manifest);
        }
    }
    let workspace_hack = PathBuf::from("workspace-hack/Cargo.toml");
    if workspace_hack.exists() {
        manifests.push(workspace_hack);
    }
    manifests.sort();
    Ok(manifests)
}

#[derive(Debug, Default)]
struct AllowEntry {
    path: Option<String>,
    glob: Option<String>,
    fields: Vec<String>,
}

impl AllowEntry {
    fn matches(&self, path: &Path) -> bool {
        let candidate = normalize_path(path);
        if self.path.as_deref() == Some(candidate.as_str()) {
            return true;
        }
        match &self.glob {
            Some(glob) => glob_matches(glob, &candidate),
            None => false,
        }
    }

    fn has_field(&self, field: &str) -> bool {
        self.fields.iter().any(|candidate| candidate == field)
    }
}

fn parse_allow_entries(text: &str) -> Vec<AllowEntry> {
    let mut entries = Vec::new();
    let mut current: Option<AllowEntry> = None;
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line == "[[allow]]" {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            current = Some(AllowEntry::default());
            continue;
        }
        let Some(entry) = current.as_mut() else {
            continue;
        };
        if let Some((key, value)) = line.split_once('=') {
            let clean_key = key.trim().to_owned();
            entry.fields.push(clean_key.clone());
            if clean_key == "path" {
                entry.path = parse_string_value(value);
            } else if clean_key == "glob" {
                entry.glob = parse_string_value(value);
            }
        }
    }
    if let Some(entry) = current {
        entries.push(entry);
    }
    entries
}

fn parse_string_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let without_prefix = trimmed.strip_prefix('"')?;
    let (result, _) = without_prefix.split_once('"')?;
    Some(result.to_owned())
}

fn validate_allow_entries(entries: &[AllowEntry], failures: &mut Vec<String>) {
    for entry in entries {
        if entry.path.is_none() && entry.glob.is_none() {
            failures.push("allow entry must include path or glob".to_owned());
        }
        for field in ["kind", "owner", "reason", "surface", "classification"] {
            if !entry.has_field(field) {
                failures.push(format!(
                    "allow entry {:?} is missing `{field}`",
                    entry.path.as_ref().or(entry.glob.as_ref())
                ));
            }
        }
    }
}

fn validate_debt_entries(text: &str, table: &str, failures: &mut Vec<String>) {
    let mut current = Vec::new();
    let mut inside_entry = false;
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line == format!("[[{table}]]") {
            validate_debt_fields(&current, failures);
            current.clear();
            inside_entry = true;
        } else if inside_entry && let Some((key, _)) = line.split_once('=') {
            current.push(key.trim().to_owned());
        }
    }
    validate_debt_fields(&current, failures);
}

fn validate_debt_fields(fields: &[String], failures: &mut Vec<String>) {
    if fields.is_empty() {
        return;
    }
    for field in ["lint", "path", "owner", "reason", "expires"] {
        if !fields.iter().any(|candidate| candidate == field) {
            failures.push(format!("debt entry is missing `{field}`"));
        }
    }
}

fn repo_files(path: &Path) -> XResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_files(path, &mut files)?;
    Ok(files)
}

fn collect_files(path: &Path, files: &mut Vec<PathBuf>) -> XResult<()> {
    if should_skip_path(path) {
        return Ok(());
    }
    if path.is_file() {
        files.push(path.to_path_buf());
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        collect_files(&entry?.path(), files)?;
    }
    Ok(())
}

fn should_skip_path(path: &Path) -> bool {
    path.components().any(|component| {
        let text = component.as_os_str().to_string_lossy();
        IGNORED_DIRS.iter().any(|ignored| text == *ignored)
    })
}

fn is_non_rust_programming_file(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|ext| NON_RUST_EXTENSIONS.contains(&ext))
}

fn normalize_path(path: &Path) -> String {
    path.strip_prefix(".")
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn glob_matches(glob: &str, path: &str) -> bool {
    if glob == "**" {
        return true;
    }
    if let Some(prefix) = glob.strip_suffix("/**") {
        return path == prefix || path.starts_with(&format!("{prefix}/"));
    }
    if let Some(prefix) = glob.strip_suffix("/**/*") {
        return path.starts_with(&format!("{prefix}/"));
    }
    if let Some((prefix, suffix)) = glob.split_once("**/*") {
        return path.starts_with(prefix) && path.ends_with(suffix);
    }
    if let Some((prefix, suffix)) = glob.split_once('*') {
        return path.starts_with(prefix) && path.ends_with(suffix);
    }
    path == glob
}
