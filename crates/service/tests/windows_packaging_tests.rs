//! Integration tests for Windows packaging utilities.
//!
//! Validates MSI GUID management, Windows service configuration,
//! registry key generation, path escaping, and WiX XML consistency.

use racing_wheel_service::windows_packaging::{
    self, GuidError, MsiGuid, RecoveryAction, RegistryData, ServiceAccount, ServiceStartType,
    WindowsServiceConfig,
};
use std::path::{Path, PathBuf};

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

/// Resolve the workspace root from CARGO_MANIFEST_DIR (crates/service).
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent() // crates/
        .and_then(|p| p.parent()) // workspace root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Read the WiX source file from the workspace.
fn read_wxs() -> Result<String, BoxErr> {
    let path = workspace_root()
        .join("packaging")
        .join("windows")
        .join("wheel-suite.wxs");
    Ok(std::fs::read_to_string(&path)?)
}

// =========================================================================
// 1. MSI Upgrade GUID invariants
// =========================================================================

#[test]
fn upgrade_code_constant_is_valid_guid() -> Result<(), BoxErr> {
    let g = MsiGuid::parse(windows_packaging::MSI_UPGRADE_CODE)?;
    assert_eq!(g.as_str().len(), 36);
    Ok(())
}

#[test]
fn upgrade_code_matches_wxs_value() -> Result<(), BoxErr> {
    // The UpgradeCode in wheel-suite.wxs must equal MSI_UPGRADE_CODE.
    // Read the WiX file and extract UpgradeCode attribute.
    let wxs_content = read_wxs()?;
    let upgrade_code_attr = wxs_content
        .lines()
        .find(|line| line.contains("UpgradeCode="))
        .ok_or("UpgradeCode attribute not found in WiX file")?;

    // Extract the GUID value between quotes
    let start = upgrade_code_attr
        .find("UpgradeCode=\"")
        .ok_or("UpgradeCode= not found")?
        + "UpgradeCode=\"".len();
    let end = upgrade_code_attr[start..]
        .find('"')
        .ok_or("Closing quote not found")?
        + start;
    let wxs_guid = &upgrade_code_attr[start..end];

    windows_packaging::validate_upgrade_guid(wxs_guid).map_err(|e| -> BoxErr { e.into() })?;
    Ok(())
}

#[test]
fn guid_braced_roundtrip() -> Result<(), BoxErr> {
    let g = MsiGuid::parse(windows_packaging::MSI_UPGRADE_CODE)?;
    let braced = g.to_braced();
    let g2 = MsiGuid::parse(&braced)?;
    assert_eq!(g, g2);
    Ok(())
}

#[test]
fn guid_rejects_empty_string() {
    let result = MsiGuid::parse("");
    assert!(result.is_err());
}

#[test]
fn guid_rejects_all_hyphens() {
    let result = MsiGuid::parse("--------");
    assert!(result.is_err());
}

#[test]
fn guid_display_shows_uppercase() -> Result<(), BoxErr> {
    let g = MsiGuid::parse("a1b2c3d4-e5f6-7890-abcd-ef1234567890")?;
    let display = format!("{g}");
    assert_eq!(display, "A1B2C3D4-E5F6-7890-ABCD-EF1234567890");
    Ok(())
}

#[test]
fn guid_error_variants_display() {
    let e1 = GuidError::InvalidLength(5);
    assert!(e1.to_string().contains('5'));

    let e2 = GuidError::InvalidCharacter('Z');
    assert!(e2.to_string().contains('Z'));

    let e3 = GuidError::InvalidFormat;
    assert!(!e3.to_string().is_empty());
}

// =========================================================================
// 2. Windows service configuration
// =========================================================================

#[test]
fn default_service_config_validates() -> Result<(), BoxErr> {
    let cfg = WindowsServiceConfig::default();
    cfg.validate().map_err(|e| -> BoxErr { e.into() })?;
    assert_eq!(cfg.service_name, windows_packaging::SERVICE_NAME);
    assert_eq!(cfg.start_type, ServiceStartType::Auto);
    assert_eq!(cfg.account, ServiceAccount::LocalSystem);
    Ok(())
}

#[test]
fn service_config_rejects_empty_name() {
    let cfg = WindowsServiceConfig {
        service_name: String::new(),
        ..WindowsServiceConfig::default()
    };
    assert!(cfg.validate().is_err());
}

#[test]
fn service_config_rejects_name_with_backslash() {
    let cfg = WindowsServiceConfig {
        service_name: r"Open\Racing".to_string(),
        ..WindowsServiceConfig::default()
    };
    assert!(cfg.validate().is_err());
}

#[test]
fn service_config_rejects_name_exceeding_256_chars() {
    let cfg = WindowsServiceConfig {
        service_name: "A".repeat(257),
        ..WindowsServiceConfig::default()
    };
    assert!(cfg.validate().is_err());
}

#[test]
fn service_config_allows_name_at_256_chars() -> Result<(), BoxErr> {
    let cfg = WindowsServiceConfig {
        service_name: "A".repeat(256),
        ..WindowsServiceConfig::default()
    };
    cfg.validate().map_err(|e| -> BoxErr { e.into() })?;
    Ok(())
}

#[test]
fn service_config_rejects_empty_display_name() {
    let cfg = WindowsServiceConfig {
        display_name: String::new(),
        ..WindowsServiceConfig::default()
    };
    assert!(cfg.validate().is_err());
}

#[test]
fn service_config_zero_delay_with_none_action_ok() -> Result<(), BoxErr> {
    let cfg = WindowsServiceConfig {
        restart_delay_seconds: 0,
        first_failure_action: RecoveryAction::None,
        second_failure_action: RecoveryAction::None,
        third_failure_action: RecoveryAction::None,
        ..WindowsServiceConfig::default()
    };
    cfg.validate().map_err(|e| -> BoxErr { e.into() })?;
    Ok(())
}

#[test]
fn sc_create_command_contains_exe_path() {
    let cfg = WindowsServiceConfig::default();
    let cmd = cfg.sc_create_command(Path::new(r"C:\Program Files\OpenRacing\bin\wheeld.exe"));
    assert!(cmd.contains(r"C:\Program Files\OpenRacing\bin\wheeld.exe"));
    assert!(cmd.contains("sc create"));
}

#[test]
fn sc_delete_command_uses_service_name() {
    let cfg = WindowsServiceConfig {
        service_name: "MyCustomService".to_string(),
        ..WindowsServiceConfig::default()
    };
    let cmd = cfg.sc_delete_command();
    assert_eq!(cmd, "sc delete MyCustomService");
}

// =========================================================================
// 3. Service start type display
// =========================================================================

#[test]
fn service_start_types_all_display_correctly() {
    let cases = [
        (ServiceStartType::Auto, "auto"),
        (ServiceStartType::DelayedAuto, "delayed-auto"),
        (ServiceStartType::Demand, "demand"),
        (ServiceStartType::Disabled, "disabled"),
    ];
    for (variant, expected) in &cases {
        assert_eq!(variant.to_string(), *expected);
    }
}

// =========================================================================
// 4. Service account display
// =========================================================================

#[test]
fn service_accounts_display_correctly() {
    assert_eq!(ServiceAccount::LocalSystem.to_string(), "LocalSystem");
    assert_eq!(
        ServiceAccount::LocalService.to_string(),
        r"NT AUTHORITY\LocalService"
    );
    assert_eq!(
        ServiceAccount::NetworkService.to_string(),
        r"NT AUTHORITY\NetworkService"
    );
    assert_eq!(
        ServiceAccount::User("svc_openracing".to_string()).to_string(),
        "svc_openracing"
    );
}

// =========================================================================
// 5. Recovery action display
// =========================================================================

#[test]
fn recovery_actions_display_correctly() {
    let cases = [
        (RecoveryAction::None, "none"),
        (RecoveryAction::Restart, "restart"),
        (RecoveryAction::Reboot, "reboot"),
        (RecoveryAction::RunCommand, "run"),
    ];
    for (variant, expected) in &cases {
        assert_eq!(variant.to_string(), *expected);
    }
}

// =========================================================================
// 6. Registry entry generation
// =========================================================================

#[test]
fn default_registry_entries_has_install_path() {
    let entries = windows_packaging::default_registry_entries(
        Path::new(r"C:\Program Files\OpenRacing"),
        "1.0.0",
    );
    let found = entries
        .iter()
        .any(|e| e.name == "InstallPath" && e.key == windows_packaging::REGISTRY_APP_KEY);
    assert!(found, "InstallPath entry not found");
}

#[test]
fn default_registry_entries_has_version() {
    let entries = windows_packaging::default_registry_entries(Path::new(r"C:\OpenRacing"), "2.1.0");
    let found = entries.iter().find(|e| e.name == "Version");
    assert!(found.is_some());
    if let Some(entry) = found {
        assert_eq!(entry.data, RegistryData::String("2.1.0".to_string()));
    }
}

#[test]
fn default_registry_entries_has_auto_start_run_key() {
    let entries = windows_packaging::default_registry_entries(Path::new(r"C:\OpenRacing"), "1.0.0");
    let run_key = entries
        .iter()
        .find(|e| e.key.contains(r"CurrentVersion\Run") && e.name == "OpenRacing");
    assert!(run_key.is_some(), "Auto-start Run key not found");
    if let Some(entry) = run_key {
        match &entry.data {
            RegistryData::String(val) => {
                assert!(
                    val.contains("openracing.exe"),
                    "Run value should reference openracing.exe"
                );
                assert!(
                    val.contains("--minimized"),
                    "Run value should include --minimized flag"
                );
            }
            _ => panic!("Run key should be a string value"),
        }
    }
}

#[test]
fn default_registry_entries_has_device_access() {
    let entries = windows_packaging::default_registry_entries(Path::new(r"C:\OpenRacing"), "1.0.0");
    let da = entries
        .iter()
        .find(|e| e.name == "Enabled" && e.key.contains("DeviceAccess"));
    assert!(da.is_some(), "DeviceAccess\\Enabled entry not found");
    if let Some(entry) = da {
        assert_eq!(entry.data, RegistryData::Dword(1));
    }
}

#[test]
fn mmcss_entries_has_all_required_values() {
    let entries = windows_packaging::mmcss_registry_entries();
    let required = [
        "Affinity",
        "Background Only",
        "Clock Rate",
        "GPU Priority",
        "Priority",
        "Scheduling Category",
        "SFIO Priority",
        "Latency Sensitive",
    ];
    for name in &required {
        let found = entries.iter().any(|e| e.name == *name);
        assert!(found, "MMCSS entry '{name}' not found");
    }
}

#[test]
fn mmcss_entries_all_use_correct_key() {
    let entries = windows_packaging::mmcss_registry_entries();
    for entry in &entries {
        assert_eq!(
            entry.key,
            windows_packaging::REGISTRY_MMCSS_TASK_KEY,
            "MMCSS entry '{}' has wrong key",
            entry.name
        );
    }
}

#[test]
fn mmcss_priority_is_6() {
    let entries = windows_packaging::mmcss_registry_entries();
    let priority = entries.iter().find(|e| e.name == "Priority");
    assert!(priority.is_some());
    if let Some(entry) = priority {
        assert_eq!(entry.data, RegistryData::Dword(6));
    }
}

// =========================================================================
// 7. Windows path escaping
// =========================================================================

#[test]
fn escape_forward_slashes_to_backslashes() {
    let p = Path::new("C:/Users/test/OpenRacing");
    assert_eq!(
        windows_packaging::escape_windows_path(p),
        r"C:\Users\test\OpenRacing"
    );
}

#[test]
fn escape_strips_trailing_backslash() {
    let p = Path::new(r"C:\OpenRacing\");
    assert_eq!(windows_packaging::escape_windows_path(p), r"C:\OpenRacing");
}

#[test]
fn escape_preserves_unc_prefix() {
    let p = Path::new(r"\\server\share\OpenRacing");
    let escaped = windows_packaging::escape_windows_path(p);
    assert!(escaped.starts_with(r"\\"));
}

#[test]
fn escape_wix_ampersand() {
    let p = Path::new(r"C:\R&D\OpenRacing");
    let escaped = windows_packaging::escape_wix_path(p);
    assert!(escaped.contains("&amp;"));
    assert!(!escaped.contains("&D"));
}

#[test]
fn escape_wix_angle_brackets() {
    let p = Path::new(r"C:\test<dir>\OpenRacing");
    let escaped = windows_packaging::escape_wix_path(p);
    assert!(escaped.contains("&lt;"));
    assert!(escaped.contains("&gt;"));
}

#[test]
fn escape_wix_quotes() {
    // Path with a literal quote (unusual but should be handled)
    let raw = r#"C:\test"dir\OpenRacing"#;
    let p = Path::new(raw);
    let escaped = windows_packaging::escape_wix_path(p);
    assert!(escaped.contains("&quot;"));
}

// =========================================================================
// 8. Default paths
// =========================================================================

#[test]
fn default_install_path_ends_with_openracing() {
    let p = windows_packaging::default_install_path();
    let escaped = windows_packaging::escape_windows_path(&p);
    assert!(
        escaped.ends_with(r"Program Files\OpenRacing"),
        "Install path should end with Program Files\\OpenRacing, got: {escaped}"
    );
}

#[test]
fn program_data_path_ends_with_openracing() {
    let p = windows_packaging::program_data_path();
    let escaped = windows_packaging::escape_windows_path(&p);
    assert!(
        escaped.ends_with(r"ProgramData\OpenRacing"),
        "ProgramData path should end with ProgramData\\OpenRacing, got: {escaped}"
    );
}

#[test]
fn default_paths_use_backslashes() {
    let install = windows_packaging::default_install_path();
    let pd = windows_packaging::program_data_path();
    let install_str = install.to_string_lossy();
    let pd_str = pd.to_string_lossy();
    // On Windows these should naturally use backslashes
    assert!(
        !install_str.contains('/'),
        "Install path should not contain forward slashes: {install_str}"
    );
    assert!(
        !pd_str.contains('/'),
        "ProgramData path should not contain forward slashes: {pd_str}"
    );
}

// =========================================================================
// 9. Installer layout validation
// =========================================================================

#[test]
fn validate_layout_reports_all_missing_dirs() -> Result<(), BoxErr> {
    let tmp = tempfile::tempdir()?;
    let errors = windows_packaging::validate_installer_layout(tmp.path());
    // Should report missing bin, config, profiles, plugins, logs, docs
    assert!(
        errors.len() >= 6,
        "Expected >=6 missing dirs, got {}",
        errors.len()
    );
    Ok(())
}

#[test]
fn validate_layout_passes_with_all_dirs() -> Result<(), BoxErr> {
    let tmp = tempfile::tempdir()?;
    for dir in &["bin", "config", "profiles", "plugins", "logs", "docs"] {
        std::fs::create_dir_all(tmp.path().join(dir))?;
    }
    let errors = windows_packaging::validate_installer_layout(tmp.path());
    assert!(errors.is_empty(), "Unexpected errors: {errors:?}");
    Ok(())
}

#[test]
fn validate_layout_partial_dirs() -> Result<(), BoxErr> {
    let tmp = tempfile::tempdir()?;
    std::fs::create_dir_all(tmp.path().join("bin"))?;
    std::fs::create_dir_all(tmp.path().join("config"))?;
    let errors = windows_packaging::validate_installer_layout(tmp.path());
    // Should report exactly 4 missing dirs: profiles, plugins, logs, docs
    assert_eq!(errors.len(), 4, "Expected 4 errors: {errors:?}");
    Ok(())
}

// =========================================================================
// 10. Constants consistency with WiX file
// =========================================================================

#[test]
fn service_name_constant_matches_wxs() -> Result<(), BoxErr> {
    let wxs_content = read_wxs()?;
    assert!(
        wxs_content.contains(windows_packaging::SERVICE_NAME),
        "WiX file should reference SERVICE_NAME = '{}'",
        windows_packaging::SERVICE_NAME
    );
    Ok(())
}

#[test]
fn service_display_name_constant_matches_wxs() -> Result<(), BoxErr> {
    let wxs_content = read_wxs()?;
    assert!(
        wxs_content.contains(windows_packaging::SERVICE_DISPLAY_NAME),
        "WiX file should reference SERVICE_DISPLAY_NAME"
    );
    Ok(())
}

#[test]
fn registry_app_key_appears_in_wxs() -> Result<(), BoxErr> {
    let wxs_content = read_wxs()?;
    // The WiX file uses partial key paths like "SOFTWARE\OpenRacing"
    assert!(
        wxs_content.contains("SOFTWARE\\OpenRacing"),
        "WiX file should contain SOFTWARE\\OpenRacing registry path"
    );
    Ok(())
}

#[test]
fn mmcss_task_key_appears_in_wxs() -> Result<(), BoxErr> {
    let wxs_content = read_wxs()?;
    assert!(
        wxs_content.contains("Multimedia\\SystemProfile\\Tasks\\OpenRacing")
            || wxs_content.contains("Multimedia/SystemProfile/Tasks/OpenRacing"),
        "WiX file should contain MMCSS task registration path"
    );
    Ok(())
}

// =========================================================================
// 11. WiX file structural integrity
// =========================================================================

#[test]
fn wxs_file_is_valid_xml() -> Result<(), BoxErr> {
    let content = read_wxs()?;
    // Basic structural check: starts with XML declaration, has matching Wix tags
    assert!(
        content.starts_with("<?xml"),
        "WiX file should start with XML declaration"
    );
    assert!(
        content.contains("<Wix"),
        "WiX file should contain <Wix root element"
    );
    assert!(
        content.contains("</Wix>"),
        "WiX file should have closing </Wix> tag"
    );
    Ok(())
}

#[test]
fn wxs_file_has_major_upgrade() -> Result<(), BoxErr> {
    let content = read_wxs()?;
    assert!(
        content.contains("<MajorUpgrade"),
        "WiX file should have MajorUpgrade element for proper upgrade handling"
    );
    assert!(
        content.contains("DowngradeErrorMessage"),
        "MajorUpgrade should have a DowngradeErrorMessage"
    );
    Ok(())
}

#[test]
fn wxs_file_has_service_install() -> Result<(), BoxErr> {
    let content = read_wxs()?;
    assert!(
        content.contains("<ServiceInstall"),
        "WiX file should have ServiceInstall element"
    );
    assert!(
        content.contains("<ServiceControl"),
        "WiX file should have ServiceControl element"
    );
    Ok(())
}

#[test]
fn wxs_service_configured_for_auto_start() -> Result<(), BoxErr> {
    let content = read_wxs()?;
    // The ServiceInstall should have Start="auto"
    assert!(
        content.contains(r#"Start="auto""#),
        "ServiceInstall should have Start=\"auto\" for automatic startup"
    );
    Ok(())
}

#[test]
fn wxs_service_has_recovery_config() -> Result<(), BoxErr> {
    let content = read_wxs()?;
    assert!(
        content.contains("ServiceConfig") || content.contains("util:ServiceConfig"),
        "WiX file should have service recovery configuration"
    );
    assert!(
        content.contains("FirstFailureActionType"),
        "Service recovery should configure first failure action"
    );
    Ok(())
}

#[test]
fn wxs_service_stops_on_uninstall() -> Result<(), BoxErr> {
    let content = read_wxs()?;
    assert!(
        content.contains(r#"Remove="uninstall"#),
        "ServiceControl should remove service on uninstall"
    );
    assert!(
        content.contains(r#"Stop="both"#) || content.contains(r#"Stop="uninstall"#),
        "ServiceControl should stop service during uninstall or both"
    );
    Ok(())
}

#[test]
fn wxs_has_environment_path() -> Result<(), BoxErr> {
    let content = read_wxs()?;
    assert!(
        content.contains("<Environment") && content.contains("PATH"),
        "WiX file should add bin directory to PATH"
    );
    Ok(())
}

#[test]
fn wxs_has_firewall_exception() -> Result<(), BoxErr> {
    let content = read_wxs()?;
    assert!(
        content.contains("FirewallException"),
        "WiX file should have firewall exception for service IPC"
    );
    Ok(())
}

#[test]
fn wxs_has_all_cleanup_components() -> Result<(), BoxErr> {
    let content = read_wxs()?;
    assert!(
        content.contains("RemoveFolder"),
        "WiX file should clean up directories on uninstall"
    );
    assert!(
        content.contains("ForceDeleteOnUninstall"),
        "WiX file should force-delete registry keys on uninstall"
    );
    Ok(())
}

// =========================================================================
// 12. WiX version condition
// =========================================================================

#[test]
fn wxs_has_windows_version_condition() -> Result<(), BoxErr> {
    let content = read_wxs()?;
    assert!(
        content.contains("<Condition") && content.contains("VersionNT"),
        "WiX file should have a Windows version condition"
    );
    Ok(())
}
