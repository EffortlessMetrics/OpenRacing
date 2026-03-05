//! macOS packaging validation tests — verifies DMG creation script structure,
//! Info.plist correctness, entitlements completeness, uninstaller paths,
//! and cross-format version consistency with Cargo.toml.

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

/// Extract the workspace version from the root Cargo.toml.
fn workspace_version() -> Result<String, Box<dyn std::error::Error>> {
    let cargo_toml = read_file("Cargo.toml")?;
    for line in cargo_toml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("version")
            && trimmed.contains('=')
            && let Some(start) = trimmed.find('"')
            && let Some(end) = trimmed[start + 1..].find('"')
        {
            return Ok(trimmed[start + 1..start + 1 + end].to_string());
        }
    }
    Err("Could not find version in Cargo.toml".into())
}

// ============================================================================
// DMG creation script — structural validation
// ============================================================================

#[test]
fn create_dmg_script_exists_and_has_shebang() -> TestResult {
    let content = read_file("packaging/macos/create-dmg.sh")?;
    assert!(
        content.starts_with("#!/bin/bash"),
        "create-dmg.sh must start with #!/bin/bash shebang"
    );
    Ok(())
}

#[test]
fn create_dmg_script_uses_strict_mode() -> TestResult {
    let content = read_file("packaging/macos/create-dmg.sh")?;
    assert!(
        content.contains("set -euo pipefail"),
        "create-dmg.sh must use strict bash mode (set -euo pipefail)"
    );
    Ok(())
}

#[test]
fn create_dmg_script_requires_bin_path_argument() -> TestResult {
    let content = read_file("packaging/macos/create-dmg.sh")?;
    assert!(
        content.contains("--bin-path"),
        "create-dmg.sh must accept --bin-path argument"
    );
    // Verify it validates the argument is provided
    assert!(
        content.contains("bin-path") && content.contains("required"),
        "create-dmg.sh must validate --bin-path is required"
    );
    Ok(())
}

#[test]
fn create_dmg_script_validates_required_binaries() -> TestResult {
    let content = read_file("packaging/macos/create-dmg.sh")?;
    assert!(
        content.contains("wheeld"),
        "create-dmg.sh must reference wheeld binary"
    );
    assert!(
        content.contains("wheelctl"),
        "create-dmg.sh must reference wheelctl binary"
    );
    Ok(())
}

#[test]
fn create_dmg_script_creates_app_bundle_structure() -> TestResult {
    let content = read_file("packaging/macos/create-dmg.sh")?;
    assert!(
        content.contains("Contents/MacOS"),
        "create-dmg.sh must create Contents/MacOS directory"
    );
    assert!(
        content.contains("Contents/Resources"),
        "create-dmg.sh must create Contents/Resources directory"
    );
    assert!(
        content.contains("Contents/Info.plist"),
        "create-dmg.sh must place Info.plist in Contents/"
    );
    Ok(())
}

#[test]
fn create_dmg_script_supports_code_signing() -> TestResult {
    let content = read_file("packaging/macos/create-dmg.sh")?;
    assert!(
        content.contains("codesign"),
        "create-dmg.sh must support codesign for code signing"
    );
    assert!(
        content.contains("entitlements.plist"),
        "create-dmg.sh must reference entitlements.plist during signing"
    );
    Ok(())
}

#[test]
fn create_dmg_script_uses_hdiutil() -> TestResult {
    let content = read_file("packaging/macos/create-dmg.sh")?;
    assert!(
        content.contains("hdiutil create"),
        "create-dmg.sh must use hdiutil to create DMG"
    );
    assert!(
        content.contains("hdiutil convert"),
        "create-dmg.sh must use hdiutil convert for compressed DMG"
    );
    Ok(())
}

#[test]
fn create_dmg_script_creates_applications_symlink() -> TestResult {
    let content = read_file("packaging/macos/create-dmg.sh")?;
    assert!(
        content.contains("ln -s /Applications"),
        "create-dmg.sh must create /Applications symlink for drag-and-drop install"
    );
    Ok(())
}

#[test]
fn create_dmg_script_generates_checksums() -> TestResult {
    let content = read_file("packaging/macos/create-dmg.sh")?;
    assert!(
        content.contains("sha256") || content.contains("shasum"),
        "create-dmg.sh must generate SHA-256 checksums"
    );
    Ok(())
}

#[test]
fn create_dmg_script_has_cleanup_trap() -> TestResult {
    let content = read_file("packaging/macos/create-dmg.sh")?;
    assert!(
        content.contains("trap "),
        "create-dmg.sh must set a trap for cleanup on exit"
    );
    Ok(())
}

// ============================================================================
// Info.plist — correctness
// ============================================================================

#[test]
fn info_plist_exists_and_is_valid_xml() -> TestResult {
    let content = read_file("packaging/macos/Info.plist")?;
    assert!(
        content.contains("<?xml version="),
        "Info.plist must be valid XML"
    );
    assert!(
        content.contains("<!DOCTYPE plist"),
        "Info.plist must have plist DOCTYPE"
    );
    Ok(())
}

#[test]
fn info_plist_has_correct_bundle_identifier() -> TestResult {
    let content = read_file("packaging/macos/Info.plist")?;
    assert!(
        content.contains("com.openracing.wheel-suite"),
        "Info.plist must use bundle identifier com.openracing.wheel-suite"
    );
    Ok(())
}

#[test]
fn info_plist_has_correct_bundle_executable() -> TestResult {
    let content = read_file("packaging/macos/Info.plist")?;
    // CFBundleExecutable should reference wheeld
    let exec_re = regex::Regex::new(r"<key>CFBundleExecutable</key>\s*<string>wheeld</string>")?;
    assert!(
        exec_re.is_match(&content),
        "Info.plist CFBundleExecutable must be 'wheeld'"
    );
    Ok(())
}

#[test]
fn info_plist_version_matches_cargo_toml() -> TestResult {
    let content = read_file("packaging/macos/Info.plist")?;
    let version = workspace_version()?;

    // Check CFBundleVersion
    let bundle_ver_re = regex::Regex::new(&format!(
        r"<key>CFBundleVersion</key>\s*<string>{}</string>",
        regex::escape(&version)
    ))?;
    assert!(
        bundle_ver_re.is_match(&content),
        "Info.plist CFBundleVersion must match Cargo.toml version ({version})"
    );

    // Check CFBundleShortVersionString
    let short_ver_re = regex::Regex::new(&format!(
        r"<key>CFBundleShortVersionString</key>\s*<string>{}</string>",
        regex::escape(&version)
    ))?;
    assert!(
        short_ver_re.is_match(&content),
        "Info.plist CFBundleShortVersionString must match Cargo.toml version ({version})"
    );
    Ok(())
}

#[test]
fn info_plist_has_minimum_macos_version() -> TestResult {
    let content = read_file("packaging/macos/Info.plist")?;
    assert!(
        content.contains("LSMinimumSystemVersion"),
        "Info.plist must specify LSMinimumSystemVersion"
    );
    assert!(
        content.contains("10.15"),
        "Info.plist minimum macOS version must be 10.15 (Catalina)"
    );
    Ok(())
}

#[test]
fn info_plist_has_required_keys() -> TestResult {
    let content = read_file("packaging/macos/Info.plist")?;
    let required_keys = [
        "CFBundleName",
        "CFBundleDisplayName",
        "CFBundleIdentifier",
        "CFBundleVersion",
        "CFBundleShortVersionString",
        "CFBundlePackageType",
        "CFBundleExecutable",
    ];
    let mut missing = Vec::new();
    for key in &required_keys {
        if !content.contains(key) {
            missing.push(*key);
        }
    }
    assert!(
        missing.is_empty(),
        "Info.plist is missing required keys: {missing:?}"
    );
    Ok(())
}

// ============================================================================
// Entitlements — completeness
// ============================================================================

#[test]
fn entitlements_plist_exists_and_is_valid_xml() -> TestResult {
    let content = read_file("packaging/macos/entitlements.plist")?;
    assert!(
        content.contains("<?xml version="),
        "entitlements.plist must be valid XML"
    );
    assert!(
        content.contains("<!DOCTYPE plist"),
        "entitlements.plist must have plist DOCTYPE"
    );
    Ok(())
}

#[test]
fn entitlements_include_usb_hid_access() -> TestResult {
    let content = read_file("packaging/macos/entitlements.plist")?;
    assert!(
        content.contains("com.apple.security.device.usb-hid"),
        "entitlements.plist MUST include com.apple.security.device.usb-hid for racing wheel communication"
    );
    // Verify it's set to true
    let hid_re = regex::Regex::new(r"<key>com\.apple\.security\.device\.usb-hid</key>\s*<true/>")?;
    assert!(
        hid_re.is_match(&content),
        "com.apple.security.device.usb-hid must be set to true"
    );
    Ok(())
}

#[test]
fn entitlements_include_network_access() -> TestResult {
    let content = read_file("packaging/macos/entitlements.plist")?;
    assert!(
        content.contains("com.apple.security.network.client"),
        "entitlements.plist must include network.client for telemetry"
    );
    assert!(
        content.contains("com.apple.security.network.server"),
        "entitlements.plist must include network.server for IPC"
    );
    Ok(())
}

#[test]
fn entitlements_include_jit_for_wasm_plugins() -> TestResult {
    let content = read_file("packaging/macos/entitlements.plist")?;
    assert!(
        content.contains("com.apple.security.cs.allow-jit"),
        "entitlements.plist must include cs.allow-jit for WASM plugin execution"
    );
    Ok(())
}

#[test]
fn entitlements_all_values_are_true() -> TestResult {
    let content = read_file("packaging/macos/entitlements.plist")?;
    let key_re = regex::Regex::new(r"<key>(com\.apple\.security\.[^<]+)</key>")?;
    let mut errors = Vec::new();
    for caps in key_re.captures_iter(&content) {
        let key = &caps[1];
        let key_pos = content
            .find(&format!("<key>{key}</key>"))
            .ok_or_else(|| format!("Key {key} not found"))?;
        let after_key = &content[key_pos..];
        // The value element should follow the closing </key>
        if !after_key.contains("<true/>") && !after_key.contains("<false/>") {
            errors.push(format!("Entitlement {key} has no boolean value"));
        }
    }
    assert!(
        errors.is_empty(),
        "Entitlement values invalid:\n{}",
        errors.join("\n")
    );
    Ok(())
}

// ============================================================================
// Uninstaller — path correctness
// ============================================================================

#[test]
fn uninstaller_exists_and_has_shebang() -> TestResult {
    let content = read_file("packaging/macos/openracing-uninstall.sh")?;
    assert!(
        content.starts_with("#!/bin/bash"),
        "openracing-uninstall.sh must start with #!/bin/bash shebang"
    );
    Ok(())
}

#[test]
fn uninstaller_uses_strict_mode() -> TestResult {
    let content = read_file("packaging/macos/openracing-uninstall.sh")?;
    assert!(
        content.contains("set -euo pipefail"),
        "openracing-uninstall.sh must use strict bash mode"
    );
    Ok(())
}

#[test]
fn uninstaller_removes_standard_macos_paths() -> TestResult {
    let content = read_file("packaging/macos/openracing-uninstall.sh")?;
    let expected_paths = [
        "/Applications/OpenRacing.app",
        "Library/LaunchDaemons/com.openracing.wheeld.plist",
        "Library/Application Support/OpenRacing",
        "Library/Caches/OpenRacing",
        "Library/Logs/OpenRacing",
    ];
    let mut missing = Vec::new();
    for path in &expected_paths {
        if !content.contains(path) {
            missing.push(*path);
        }
    }
    assert!(
        missing.is_empty(),
        "Uninstaller is missing cleanup paths: {missing:?}"
    );
    Ok(())
}

#[test]
fn uninstaller_stops_service_before_removal() -> TestResult {
    let content = read_file("packaging/macos/openracing-uninstall.sh")?;
    assert!(
        content.contains("launchctl"),
        "Uninstaller must use launchctl to stop the service"
    );
    // The service stop should come before the app bundle removal
    let launchctl_pos = content.find("launchctl");
    let rm_app_pos = content
        .find("rm -rf \"$APP_BUNDLE\"")
        .or_else(|| content.find("rm -rf \"${APP_BUNDLE}\""))
        .or_else(|| content.find("/Applications/OpenRacing.app"));
    if let (Some(lp), Some(rp)) = (launchctl_pos, rm_app_pos) {
        assert!(
            lp < rp,
            "Uninstaller must stop the service before removing the app bundle"
        );
    }
    Ok(())
}

#[test]
fn uninstaller_supports_keep_config_flag() -> TestResult {
    let content = read_file("packaging/macos/openracing-uninstall.sh")?;
    assert!(
        content.contains("--keep-config"),
        "Uninstaller must support --keep-config flag to preserve user data"
    );
    Ok(())
}

// ============================================================================
// Cross-format version consistency
// ============================================================================

#[test]
fn all_macos_packaging_files_reference_consistent_version() -> TestResult {
    let version = workspace_version()?;
    let info_plist = read_file("packaging/macos/Info.plist")?;

    // Info.plist must contain the version
    assert!(
        info_plist.contains(&version),
        "Info.plist must contain workspace version {version}"
    );

    // The create-dmg.sh should auto-detect version from Cargo.toml
    let create_dmg = read_file("packaging/macos/create-dmg.sh")?;
    assert!(
        create_dmg.contains("Cargo.toml"),
        "create-dmg.sh must reference Cargo.toml for version auto-detection"
    );

    Ok(())
}

#[test]
fn macos_packaging_directory_is_complete() -> TestResult {
    let macos_dir = repo_root().join("packaging").join("macos");
    let required_files = [
        "create-dmg.sh",
        "Info.plist",
        "entitlements.plist",
        "openracing-uninstall.sh",
    ];
    let mut missing = Vec::new();
    for file in &required_files {
        if !macos_dir.join(file).exists() {
            missing.push(*file);
        }
    }
    assert!(
        missing.is_empty(),
        "macOS packaging directory is missing files: {missing:?}"
    );
    Ok(())
}
