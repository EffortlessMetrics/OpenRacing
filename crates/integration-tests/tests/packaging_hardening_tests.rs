//! Packaging hardening tests — cross-file consistency, udev rule structural
//! validation, hwdb↔udev VID cross-references, modprobe quirk coverage,
//! and install script completeness checks.
//!
//! Complements `packaging_validation_tests.rs` with deeper structural checks
//! that verify internal consistency *across* packaging artifacts.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Return the repository root (two levels up from the integration-tests crate).
fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| manifest.clone())
}

fn read_file(rel_path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let path = repo_root().join(rel_path);
    let content =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    Ok(content)
}

// ============================================================================
// Udev rules — structural consistency
// ============================================================================

#[test]
fn udev_rules_hidraw_lines_have_balanced_quotes() -> TestResult {
    let content = read_file("packaging/linux/99-racing-wheel-suite.rules")?;
    let mut errors = Vec::new();
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let quote_count = trimmed.chars().filter(|&c| c == '"').count();
        if quote_count % 2 != 0 {
            errors.push(format!("Line {}: unbalanced quotes ({quote_count})", i + 1));
        }
    }
    assert!(
        errors.is_empty(),
        "Udev rules have unbalanced quotes:\n{}",
        errors.join("\n")
    );
    Ok(())
}

#[test]
fn udev_rules_hidraw_lines_use_consistent_group() -> TestResult {
    let content = read_file("packaging/linux/99-racing-wheel-suite.rules")?;
    let group_re = regex::Regex::new(r#"GROUP="([^"]*)""#)?;
    let mut groups_found = HashSet::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if !trimmed.contains("SUBSYSTEM==\"hidraw\"") {
            continue;
        }
        if let Some(caps) = group_re.captures(trimmed)
            && let Some(g) = caps.get(1)
        {
            groups_found.insert(g.as_str().to_string());
        }
    }
    assert!(
        groups_found.len() <= 1,
        "Hidraw rules should use a single consistent GROUP value, found: {groups_found:?}"
    );
    assert!(
        groups_found.contains("input"),
        "Hidraw rules GROUP should be \"input\""
    );
    Ok(())
}

#[test]
fn udev_rules_hidraw_lines_all_have_uaccess_tag() -> TestResult {
    let content = read_file("packaging/linux/99-racing-wheel-suite.rules")?;
    let mut missing_uaccess = Vec::new();
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if !trimmed.contains("SUBSYSTEM==\"hidraw\"") {
            continue;
        }
        if !trimmed.contains("uaccess") {
            missing_uaccess.push(format!("Line {}: {}", i + 1, trimmed));
        }
    }
    assert!(
        missing_uaccess.is_empty(),
        "Hidraw rules missing TAG+=\"uaccess\":\n{}",
        missing_uaccess.join("\n")
    );
    Ok(())
}

#[test]
fn udev_rules_hidraw_lines_use_consistent_mode() -> TestResult {
    let content = read_file("packaging/linux/99-racing-wheel-suite.rules")?;
    let mode_re = regex::Regex::new(r#"MODE="([^"]*)""#)?;
    let mut modes_found = HashSet::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if !trimmed.contains("SUBSYSTEM==\"hidraw\"") {
            continue;
        }
        if let Some(caps) = mode_re.captures(trimmed)
            && let Some(m) = caps.get(1)
        {
            modes_found.insert(m.as_str().to_string());
        }
    }
    assert!(
        modes_found.len() <= 1,
        "Hidraw rules should use a single consistent MODE, found: {modes_found:?}"
    );
    Ok(())
}

#[test]
fn udev_rules_no_trailing_whitespace() -> TestResult {
    let content = read_file("packaging/linux/99-racing-wheel-suite.rules")?;
    let mut offending = Vec::new();
    for (i, line) in content.lines().enumerate() {
        if line != line.trim_end() {
            offending.push(i + 1);
        }
    }
    assert!(
        offending.is_empty(),
        "Udev rules have trailing whitespace on lines: {offending:?}"
    );
    Ok(())
}

#[test]
fn udev_rules_commas_separate_key_value_pairs() -> TestResult {
    let content = read_file("packaging/linux/99-racing-wheel-suite.rules")?;
    let mut errors = Vec::new();
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Udev rules use commas to separate fields
        let field_count = trimmed.split(',').count();
        if field_count < 2 {
            // A minimal rule should have at least match + assignment
            // but some lines (like RUN or ENV) may be single-field
            continue;
        }
        // Verify no double commas
        if trimmed.contains(",,") {
            errors.push(format!("Line {}: double comma", i + 1));
        }
    }
    assert!(
        errors.is_empty(),
        "Udev rules have comma issues:\n{}",
        errors.join("\n")
    );
    Ok(())
}

// ============================================================================
// Cross-file VID consistency: hwdb ↔ udev rules
// ============================================================================

/// Extract all uppercase VIDs from the hwdb file.
fn hwdb_vids(content: &str) -> HashSet<String> {
    let re = regex::Regex::new(r"(?i)id-input:modalias:input:\*v([0-9A-Fa-f]{4})p")
        .expect("hwdb vid regex");
    let mut vids = HashSet::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(caps) = re.captures(trimmed)
            && let Some(vid) = caps.get(1)
        {
            vids.insert(vid.as_str().to_lowercase());
        }
    }
    vids
}

/// Extract all VIDs from the udev rules file.
fn udev_vids(content: &str) -> HashSet<String> {
    let re = regex::Regex::new(r#"ATTRS\{idVendor\}=="([0-9a-fA-F]{4})""#).expect("udev vid regex");
    let mut vids = HashSet::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(caps) = re.captures(trimmed)
            && let Some(vid) = caps.get(1)
        {
            vids.insert(vid.as_str().to_lowercase());
        }
    }
    vids
}

#[test]
fn hwdb_vids_mostly_overlap_udev_rule_vids() -> TestResult {
    let hwdb = read_file("packaging/linux/99-racing-wheel-suite.hwdb")?;
    let udev = read_file("packaging/linux/99-racing-wheel-suite.rules")?;

    let hw_vids = hwdb_vids(&hwdb);
    let ud_vids = udev_vids(&udev);

    let overlap = hw_vids.intersection(&ud_vids).count();
    // The hwdb may include VIDs for devices that don't need hidraw access
    // (e.g. third-party Xbox controllers, Guillemot, FlashFire), so it can
    // be a superset. Verify strong overlap: at least 80% of hwdb VIDs are
    // also in the udev rules.
    let ratio = overlap as f64 / hw_vids.len().max(1) as f64;
    assert!(
        ratio >= 0.80,
        "Less than 80% of hwdb VIDs found in udev rules ({overlap}/{} = {:.0}%)",
        hw_vids.len(),
        ratio * 100.0
    );
    Ok(())
}

// ============================================================================
// Modprobe quirks VID cross-reference
// ============================================================================

#[test]
fn modprobe_quirk_vids_present_in_udev_rules() -> TestResult {
    let quirks = read_file("packaging/linux/90-racing-wheel-quirks.conf")?;
    let udev = read_file("packaging/linux/99-racing-wheel-suite.rules")?;

    // Parse VIDs from the quirks line: 0xVID:0xPID:0xQUIRK
    let re = regex::Regex::new(r"0x([0-9A-Fa-f]{4}):0x[0-9A-Fa-f]{4}:0x[0-9A-Fa-f]{4}")?;
    let mut quirk_vids = HashSet::new();
    for caps in re.captures_iter(&quirks) {
        if let Some(vid) = caps.get(1) {
            quirk_vids.insert(vid.as_str().to_lowercase());
        }
    }

    let ud_vids = udev_vids(&udev);
    let missing: Vec<_> = quirk_vids.difference(&ud_vids).collect();
    assert!(
        missing.is_empty(),
        "Modprobe quirk VIDs not in udev rules: {missing:?}"
    );
    Ok(())
}

#[test]
fn modprobe_quirk_vids_present_in_hwdb() -> TestResult {
    let quirks = read_file("packaging/linux/90-racing-wheel-quirks.conf")?;
    let hwdb_content = read_file("packaging/linux/99-racing-wheel-suite.hwdb")?;

    let re = regex::Regex::new(r"0x([0-9A-Fa-f]{4}):0x[0-9A-Fa-f]{4}:0x[0-9A-Fa-f]{4}")?;
    let mut quirk_vids = HashSet::new();
    for caps in re.captures_iter(&quirks) {
        if let Some(vid) = caps.get(1) {
            quirk_vids.insert(vid.as_str().to_lowercase());
        }
    }

    let hw_vids = hwdb_vids(&hwdb_content);
    let missing: Vec<_> = quirk_vids.difference(&hw_vids).collect();
    assert!(
        missing.is_empty(),
        "Modprobe quirk VIDs not in hwdb: {missing:?}"
    );
    Ok(())
}

// ============================================================================
// Install script completeness
// ============================================================================

#[test]
fn install_script_has_help_option() -> TestResult {
    let content = read_file("packaging/linux/install.sh")?;
    assert!(
        content.contains("--help"),
        "Install script should support --help"
    );
    Ok(())
}

#[test]
fn install_script_references_all_packaging_files() -> TestResult {
    let content = read_file("packaging/linux/install.sh")?;
    let expected_refs = [
        "99-racing-wheel-suite.rules",
        "90-racing-wheel-quirks.conf",
        "wheeld.service.template",
    ];
    let mut missing = Vec::new();
    for f in &expected_refs {
        if !content.contains(f) {
            missing.push(*f);
        }
    }
    assert!(
        missing.is_empty(),
        "Install script missing references to: {missing:?}"
    );
    Ok(())
}

#[test]
fn install_script_checks_user_groups() -> TestResult {
    let content = read_file("packaging/linux/install.sh")?;
    assert!(
        content.contains("input"),
        "Install script should check/warn about 'input' group"
    );
    assert!(
        content.contains("plugdev"),
        "Install script should check/warn about 'plugdev' group"
    );
    Ok(())
}

#[test]
fn install_script_installs_both_binaries() -> TestResult {
    let content = read_file("packaging/linux/install.sh")?;
    assert!(
        content.contains("wheeld"),
        "Install script must install wheeld"
    );
    assert!(
        content.contains("wheelctl"),
        "Install script must install wheelctl"
    );
    Ok(())
}

// ============================================================================
// Systemd service template — directive validation
// ============================================================================

#[test]
fn systemd_service_template_no_trailing_whitespace() -> TestResult {
    let content = read_file("packaging/linux/wheeld.service.template")?;
    let mut offending = Vec::new();
    for (i, line) in content.lines().enumerate() {
        if line != line.trim_end() {
            offending.push(i + 1);
        }
    }
    assert!(
        offending.is_empty(),
        "Service template has trailing whitespace on lines: {offending:?}"
    );
    Ok(())
}

#[test]
fn systemd_service_template_directives_use_valid_names() -> TestResult {
    let content = read_file("packaging/linux/wheeld.service.template")?;
    // Known valid systemd directive prefixes
    let valid_prefixes = [
        "Description",
        "Documentation",
        "After",
        "Wants",
        "Type",
        "ExecStart",
        "ExecReload",
        "Restart",
        "RestartSec",
        "TimeoutStartSec",
        "TimeoutStopSec",
        "NoNewPrivileges",
        "ProtectSystem",
        "ProtectHome",
        "PrivateTmp",
        "PrivateDevices",
        "ProtectHostname",
        "ProtectClock",
        "ProtectKernelTunables",
        "ProtectKernelModules",
        "ProtectKernelLogs",
        "ProtectControlGroups",
        "RestrictAddressFamilies",
        "RestrictNamespaces",
        "LockPersonality",
        "MemoryDenyWriteExecute",
        "RestrictRealtime",
        "RestrictSUIDSGID",
        "RemoveIPC",
        "SupplementaryGroups",
        "AmbientCapabilities",
        "WorkingDirectory",
        "StateDirectory",
        "ConfigurationDirectory",
        "LogsDirectory",
        "RuntimeDirectory",
        "Environment",
        "WantedBy",
    ];

    let mut errors = Vec::new();
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('[') {
            continue;
        }
        // Must be Key=Value
        if !trimmed.contains('=') {
            errors.push(format!("Line {}: not a key=value pair: {trimmed}", i + 1));
            continue;
        }
        let key = trimmed.split('=').next().unwrap_or("");
        if !valid_prefixes.contains(&key) {
            errors.push(format!("Line {}: unknown directive '{key}'", i + 1));
        }
    }
    assert!(
        errors.is_empty(),
        "Service template has invalid directives:\n{}",
        errors.join("\n")
    );
    Ok(())
}

#[test]
fn systemd_service_template_timeouts_are_reasonable() -> TestResult {
    let content = read_file("packaging/linux/wheeld.service.template")?;
    let timeout_re = regex::Regex::new(r"Timeout(Start|Stop)Sec=(\d+)")?;
    for caps in timeout_re.captures_iter(&content) {
        if let Some(val) = caps.get(2) {
            let secs: u32 = val.as_str().parse()?;
            assert!(
                secs <= 120,
                "Timeout{}Sec={secs} seems unreasonably high (>120s)",
                caps.get(1).map(|m| m.as_str()).unwrap_or("")
            );
            assert!(
                secs >= 5,
                "Timeout{}Sec={secs} seems too low (<5s)",
                caps.get(1).map(|m| m.as_str()).unwrap_or("")
            );
        }
    }
    Ok(())
}

#[test]
fn systemd_service_restricts_address_families() -> TestResult {
    let content = read_file("packaging/linux/wheeld.service.template")?;
    assert!(
        content.contains("RestrictAddressFamilies="),
        "Service should restrict address families"
    );
    // Must allow AF_UNIX for IPC
    assert!(
        content.contains("AF_UNIX"),
        "Service must allow AF_UNIX for IPC"
    );
    Ok(())
}

// ============================================================================
// Package metadata — license and repository consistency
// ============================================================================

#[test]
fn workspace_license_field_present() -> TestResult {
    let cargo = read_file("Cargo.toml")?;
    assert!(
        cargo.contains("license = \"MIT OR Apache-2.0\""),
        "Workspace must define dual MIT/Apache-2.0 license"
    );
    Ok(())
}

#[test]
fn workspace_repository_url_present() -> TestResult {
    let cargo = read_file("Cargo.toml")?;
    assert!(
        cargo.contains("repository = \"https://"),
        "Workspace must define repository URL"
    );
    Ok(())
}

#[test]
fn service_and_cli_use_workspace_metadata() -> TestResult {
    let service = read_file("crates/service/Cargo.toml")?;
    let cli = read_file("crates/cli/Cargo.toml")?;

    for (name, content) in [("service", &service), ("cli", &cli)] {
        assert!(
            content.contains("version.workspace = true"),
            "{name} crate must use workspace version"
        );
        assert!(
            content.contains("edition.workspace = true"),
            "{name} crate must use workspace edition"
        );
        assert!(
            content.contains("license.workspace = true"),
            "{name} crate must use workspace license"
        );
    }
    Ok(())
}

// ============================================================================
// Hwdb structural validation
// ============================================================================

#[test]
fn hwdb_no_trailing_whitespace() -> TestResult {
    let content = read_file("packaging/linux/99-racing-wheel-suite.hwdb")?;
    let mut offending = Vec::new();
    for (i, line) in content.lines().enumerate() {
        if line != line.trim_end() {
            offending.push(i + 1);
        }
    }
    assert!(
        offending.is_empty(),
        "hwdb file has trailing whitespace on lines: {offending:?}"
    );
    Ok(())
}

#[test]
fn hwdb_entries_always_pair_accelerometer_and_joystick() -> TestResult {
    let content = read_file("packaging/linux/99-racing-wheel-suite.hwdb")?;
    // Each id-input line should be followed by exactly:
    //   " ID_INPUT_ACCELEROMETER=0\n ID_INPUT_JOYSTICK=1"
    let modalias_re = regex::Regex::new(r"^id-input:modalias:")?;

    let lines: Vec<&str> = content.lines().collect();
    let mut errors = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if !modalias_re.is_match(trimmed) {
            continue;
        }
        // Next two non-empty lines should be the properties
        let next1 = lines.get(i + 1).map(|l| l.trim()).unwrap_or("");
        let next2 = lines.get(i + 2).map(|l| l.trim()).unwrap_or("");

        if next1 != "ID_INPUT_ACCELEROMETER=0" {
            errors.push(format!(
                "Line {}: expected ID_INPUT_ACCELEROMETER=0 after modalias, got '{next1}'",
                i + 2
            ));
        }
        if next2 != "ID_INPUT_JOYSTICK=1" {
            errors.push(format!(
                "Line {}: expected ID_INPUT_JOYSTICK=1 after modalias, got '{next2}'",
                i + 3
            ));
        }
    }
    assert!(
        errors.is_empty(),
        "hwdb entry pairing errors:\n{}",
        errors.join("\n")
    );
    Ok(())
}

// ============================================================================
// Windows packaging consistency
// ============================================================================

#[test]
fn wix_manifest_includes_license_file() -> TestResult {
    let wxs = read_file("packaging/windows/wheel-suite.wxs")?;
    // WXS should bundle the license file for the installer
    assert!(
        wxs.contains("LICENSE") || wxs.contains("License"),
        "WXS should reference a LICENSE file component"
    );
    Ok(())
}

#[test]
fn windows_readme_exists() -> TestResult {
    let path = repo_root().join("packaging/windows/README.md");
    assert!(
        path.exists(),
        "packaging/windows/README.md should exist for build instructions"
    );
    Ok(())
}
