//! Packaging validation tests — verifies udev rules, packaging directory
//! structure, and cross-references VID/PID constants from HID protocol crates.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

/// Return the repository root (two levels up from the integration-tests crate).
fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| manifest.clone())
}

/// Parse all VID/PID pairs from the udev rules file (hidraw section only).
/// Returns `(vid_pid_pairs, vendor_wide_vids)`.
fn parse_udev_rules(content: &str) -> (HashSet<(String, String)>, HashSet<String>) {
    let vid_pid_re = regex::Regex::new(
        r#"ATTRS\{idVendor\}=="([0-9a-fA-F]{4})".*?ATTRS\{idProduct\}=="([0-9a-fA-F]{4})""#,
    )
    .expect("regex");
    let hidraw_vid_re =
        regex::Regex::new(r#"SUBSYSTEM=="hidraw".*ATTRS\{idVendor\}=="([0-9a-fA-F]{4})""#)
            .expect("regex");

    let mut pairs = HashSet::new();
    let mut vendor_wide = HashSet::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(caps) = vid_pid_re.captures(line) {
            let vid = caps.get(1).map(|m| m.as_str().to_lowercase());
            let pid = caps.get(2).map(|m| m.as_str().to_lowercase());
            if let (Some(v), Some(p)) = (vid, pid) {
                pairs.insert((v, p));
            }
        } else if let Some(caps) = hidraw_vid_re.captures(line)
            && let Some(vid) = caps.get(1).map(|m| m.as_str().to_lowercase())
        {
            // hidraw line with VID but no PID match → vendor-wide rule
            vendor_wide.insert(vid);
        }
    }
    (pairs, vendor_wide)
}

/// Known VID/PID pairs that MUST appear in udev rules (from HID protocol crates).
/// Each entry: (vid, pid, description).
fn known_vid_pids() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        // Logitech (0x046D)
        ("046d", "c24f", "Logitech G29 PS"),
        ("046d", "c29b", "Logitech G27"),
        ("046d", "c298", "Logitech Driving Force Pro"),
        ("046d", "c299", "Logitech G25"),
        ("046d", "c29a", "Logitech Driving Force GT"),
        ("046d", "c262", "Logitech G920"),
        ("046d", "c266", "Logitech G923"),
        ("046d", "c267", "Logitech G923 PS"),
        ("046d", "c26e", "Logitech G923 Xbox"),
        ("046d", "c26d", "Logitech G923 Xbox Alt"),
        ("046d", "c268", "Logitech G PRO"),
        ("046d", "c272", "Logitech G PRO Xbox"),
        ("046d", "c295", "Logitech MOMO"),
        ("046d", "c294", "Logitech Driving Force EX"),
        ("046d", "c29c", "Logitech Speed Force Wireless"),
        ("046d", "ca03", "Logitech MOMO 2"),
        ("046d", "c293", "Logitech WingMan Formula Force GP"),
        ("046d", "c291", "Logitech WingMan Formula Force"),
        ("046d", "ca04", "Logitech Vibration Wheel"),
        // Thrustmaster (0x044F)
        ("044f", "b65d", "Thrustmaster FFB Wheel Generic"),
        ("044f", "b677", "Thrustmaster T150"),
        ("044f", "b65e", "Thrustmaster T500 RS"),
        ("044f", "b66d", "Thrustmaster T300 RS PS4"),
        ("044f", "b66e", "Thrustmaster T300 RS"),
        ("044f", "b66f", "Thrustmaster T300 RS GT"),
        ("044f", "b669", "Thrustmaster TX Racing"),
        ("044f", "b67f", "Thrustmaster TMX"),
        ("044f", "b696", "Thrustmaster T248"),
        ("044f", "b69a", "Thrustmaster T248X"),
        ("044f", "b689", "Thrustmaster TS-PC Racer"),
        ("044f", "b692", "Thrustmaster TS-XW"),
        ("044f", "b691", "Thrustmaster TS-XW GIP"),
        ("044f", "b681", "Thrustmaster T-GT II GT"),
        ("044f", "b69b", "Thrustmaster T818"),
        ("044f", "b664", "Thrustmaster TX Racing Orig"),
        ("044f", "b668", "Thrustmaster T80"),
        ("044f", "b371", "Thrustmaster T-LCM"),
        ("044f", "b68f", "Thrustmaster TPR Pedals"),
        // Fanatec — vendor-wide rule
        // Moza — vendor-wide rule
        // Simagic legacy (0x0483)
        ("0483", "0522", "Simagic Alpha/M10 legacy"),
        // VRS (0x0483)
        ("0483", "a355", "VRS DirectForce Pro"),
        ("0483", "a356", "VRS DirectForce Pro V2"),
        ("0483", "a357", "VRS Pedals V1"),
        ("0483", "a358", "VRS Pedals V2"),
        ("0483", "a359", "VRS Handbrake"),
        ("0483", "a35a", "VRS Shifter"),
        ("0483", "a3be", "VRS Pedals"),
        ("0483", "a44c", "VRS R295"),
        // Cube Controls (0x0483, provisional)
        ("0483", "0c73", "Cube Controls GT Pro"),
        ("0483", "0c74", "Cube Controls Formula Pro"),
        ("0483", "0c75", "Cube Controls CSX3"),
        // Simucube (0x16D0)
        ("16d0", "0d5a", "Simucube 1"),
        ("16d0", "0d5f", "Simucube 2 Ultimate"),
        ("16d0", "0d60", "Simucube 2 Pro"),
        ("16d0", "0d61", "Simucube 2 Sport"),
        ("16d0", "0d66", "Simucube ActivePedal"),
        ("16d0", "0d63", "Simucube Wireless Wheel"),
        // Heusinkveld (0x30B7)
        ("30b7", "1001", "Heusinkveld Sprint"),
        ("30b7", "1002", "Heusinkveld Handbrake V2"),
        ("30b7", "1003", "Heusinkveld Ultimate"),
        // Heusinkveld legacy (0x04D8)
        ("04d8", "f6d0", "Heusinkveld Legacy Sprint"),
        ("04d8", "f6d2", "Heusinkveld Legacy Ultimate"),
        ("04d8", "f6d3", "Heusinkveld Pro"),
        // Heusinkveld Handbrake V1 (0x10C4)
        ("10c4", "8b82", "Heusinkveld Handbrake V1"),
        // Heusinkveld Shifter (0xA020)
        ("a020", "3142", "Heusinkveld Sequential Shifter"),
        // Cammus (0x3416)
        ("3416", "0301", "Cammus C5"),
        ("3416", "0302", "Cammus C12"),
        ("3416", "1018", "Cammus CP5 Pedals"),
        ("3416", "1019", "Cammus LC100 Pedals"),
        // Asetek (0x2433)
        ("2433", "f300", "Asetek Invicta"),
        ("2433", "f301", "Asetek Forte"),
        ("2433", "f303", "Asetek La Prima"),
        ("2433", "f306", "Asetek Tony Kanaan"),
        ("2433", "f100", "Asetek Invicta Pedals"),
        ("2433", "f101", "Asetek Forte Pedals"),
        ("2433", "f102", "Asetek La Prima Pedals"),
        // OpenFFBoard (0x1209)
        ("1209", "ffb0", "OpenFFBoard"),
        ("1209", "ffb1", "OpenFFBoard Alt"),
        ("1209", "1bbd", "Generic Button Box"),
        // FFBeast (0x045B)
        ("045b", "58f9", "FFBeast Joystick"),
        ("045b", "5968", "FFBeast Rudder"),
        ("045b", "59d7", "FFBeast Wheel"),
        // Granite Devices (0x1D50)
        ("1d50", "6050", "IONI Simucube 1"),
        ("1d50", "6051", "IONI Premium Simucube 2"),
        ("1d50", "6052", "ARGON Simucube Sport"),
        // Leo Bodnar (0x1DD2)
        ("1dd2", "000e", "Leo Bodnar Wheel Interface"),
        ("1dd2", "000c", "Leo Bodnar BBI-32"),
        ("1dd2", "0001", "Leo Bodnar USB Joystick"),
        ("1dd2", "000f", "Leo Bodnar FFB Joystick"),
        ("1dd2", "1301", "Leo Bodnar SLI-Pro"),
        ("1dd2", "100c", "Leo Bodnar Pedals"),
        ("1dd2", "22d0", "Leo Bodnar LC Pedals"),
        // PXN (0x11FF)
        ("11ff", "3245", "PXN V10"),
        ("11ff", "1212", "PXN V12"),
        ("11ff", "1112", "PXN V12 Lite"),
        ("11ff", "1211", "PXN V12 Lite SE"),
        ("11ff", "2141", "PXN GT987"),
        // AccuForce (0x1FC9)
        ("1fc9", "804c", "AccuForce Pro"),
    ]
}

/// VIDs covered by vendor-wide udev rules (no PID filter).
fn vendor_wide_vids() -> Vec<&'static str> {
    vec![
        "0eb7", // Fanatec
        "346e", // Moza
        "3670", // Simagic EVO
    ]
}

#[test]
fn udev_rules_file_exists() -> Result<(), Box<dyn std::error::Error>> {
    let path = repo_root().join("packaging/linux/99-racing-wheel-suite.rules");
    assert!(
        path.exists(),
        "udev rules file not found at: {}",
        path.display()
    );
    Ok(())
}

#[test]
fn udev_rules_all_known_vids_present() -> Result<(), Box<dyn std::error::Error>> {
    let path = repo_root().join("packaging/linux/99-racing-wheel-suite.rules");
    let content = fs::read_to_string(&path)?;
    let (pairs, wide_vids) = parse_udev_rules(&content);

    let mut missing = Vec::new();
    for (vid, pid, desc) in known_vid_pids() {
        if wide_vids.contains(vid) {
            continue;
        }
        if !pairs.contains(&(vid.to_string(), pid.to_string())) {
            missing.push(format!("  {vid}:{pid} ({desc})"));
        }
    }

    assert!(
        missing.is_empty(),
        "Missing VID/PID pairs in udev rules:\n{}",
        missing.join("\n")
    );
    Ok(())
}

#[test]
fn udev_rules_vendor_wide_vids_present() -> Result<(), Box<dyn std::error::Error>> {
    let path = repo_root().join("packaging/linux/99-racing-wheel-suite.rules");
    let content = fs::read_to_string(&path)?;
    let (_pairs, wide_vids) = parse_udev_rules(&content);

    for vid in vendor_wide_vids() {
        assert!(
            wide_vids.contains(vid),
            "Vendor-wide VID {vid} not found in udev rules"
        );
    }
    Ok(())
}

#[test]
fn udev_rules_syntax_valid() -> Result<(), Box<dyn std::error::Error>> {
    let path = repo_root().join("packaging/linux/99-racing-wheel-suite.rules");
    let content = fs::read_to_string(&path)?;

    let vid_re = regex::Regex::new(r#"ATTRS\{idVendor\}=="([^"]*)""#)?;
    let pid_re = regex::Regex::new(r#"ATTRS\{idProduct\}=="([^"]*)""#)?;
    let hex4_re = regex::Regex::new(r"^[0-9a-fA-F]{4}$")?;

    let mut errors = Vec::new();
    for (lineno, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Every non-comment line must have an operator
        if !trimmed.contains("==") && !trimmed.contains("+=") && !trimmed.contains('=') {
            errors.push(format!("Line {}: no assignment operator", lineno + 1));
        }
        // Validate VID format
        for caps in vid_re.captures_iter(trimmed) {
            if let Some(val) = caps.get(1)
                && !hex4_re.is_match(val.as_str())
            {
                errors.push(format!(
                    "Line {}: invalid VID '{}'",
                    lineno + 1,
                    val.as_str()
                ));
            }
        }
        // Validate PID format
        for caps in pid_re.captures_iter(trimmed) {
            if let Some(val) = caps.get(1)
                && !hex4_re.is_match(val.as_str())
            {
                errors.push(format!(
                    "Line {}: invalid PID '{}'",
                    lineno + 1,
                    val.as_str()
                ));
            }
        }
    }

    assert!(
        errors.is_empty(),
        "udev rules syntax errors:\n{}",
        errors.join("\n")
    );
    Ok(())
}

#[test]
fn udev_rules_no_duplicate_hidraw_entries() -> Result<(), Box<dyn std::error::Error>> {
    let path = repo_root().join("packaging/linux/99-racing-wheel-suite.rules");
    let content = fs::read_to_string(&path)?;

    let re = regex::Regex::new(
        r#"SUBSYSTEM=="hidraw".*ATTRS\{idVendor\}=="([0-9a-fA-F]{4})".*ATTRS\{idProduct\}=="([0-9a-fA-F]{4})""#,
    )?;

    let mut seen = HashSet::new();
    let mut dupes = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(caps) = re.captures(trimmed) {
            let vid = caps.get(1).map(|m| m.as_str().to_lowercase());
            let pid = caps.get(2).map(|m| m.as_str().to_lowercase());
            if let (Some(v), Some(p)) = (vid, pid) {
                let key = format!("{v}:{p}");
                if !seen.insert(key.clone()) {
                    dupes.push(key);
                }
            }
        }
    }

    assert!(
        dupes.is_empty(),
        "Duplicate hidraw VID:PID entries: {:?}",
        dupes
    );
    Ok(())
}

#[test]
fn packaging_linux_directory_complete() -> Result<(), Box<dyn std::error::Error>> {
    let root = repo_root();
    let linux_dir = root.join("packaging/linux");
    assert!(linux_dir.exists(), "packaging/linux/ directory missing");

    let required_files = [
        "99-racing-wheel-suite.rules",
        "90-racing-wheel-quirks.conf",
        "install.sh",
        "wheeld.service.template",
    ];
    for f in &required_files {
        let path = linux_dir.join(f);
        assert!(path.exists(), "Missing Linux packaging file: {f}");
    }
    Ok(())
}

#[test]
fn packaging_windows_directory_exists() -> Result<(), Box<dyn std::error::Error>> {
    let root = repo_root();
    let win_dir = root.join("packaging/windows");
    assert!(win_dir.exists(), "packaging/windows/ directory missing");

    // Check for MSI stub or build script
    let has_wix = win_dir.join("wheel-suite.wxs").exists();
    let has_build = win_dir.join("build-msi.ps1").exists();
    assert!(
        has_wix || has_build,
        "packaging/windows/ must contain wheel-suite.wxs or build-msi.ps1"
    );
    Ok(())
}
