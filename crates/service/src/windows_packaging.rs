//! Windows packaging utilities for MSI installer support.
//!
//! Provides types and helpers for:
//! - MSI upgrade GUID management and validation
//! - Windows service installation/uninstallation configuration
//! - Registry key configuration for auto-start and application metadata
//! - Windows-specific path escaping for WiX/MSI properties

use std::fmt;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// MSI Upgrade GUID management
// ---------------------------------------------------------------------------

/// The stable UpgradeCode GUID used across all MSI releases.
///
/// This GUID **must never change** between versions — Windows Installer uses
/// it to detect existing installations and perform major upgrades. Changing
/// it would cause side-by-side installs instead of in-place upgrades.
///
/// Format: `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}` (RFC 4122 uppercase).
pub const MSI_UPGRADE_CODE: &str = "A1B2C3D4-E5F6-7890-ABCD-EF1234567890";

/// Windows service name registered with SCM.
pub const SERVICE_NAME: &str = "OpenRacingService";

/// Display name shown in `services.msc`.
pub const SERVICE_DISPLAY_NAME: &str = "OpenRacing Force Feedback Service";

/// Default installation directory under `%ProgramFiles%`.
pub const DEFAULT_INSTALL_DIR: &str = r"C:\Program Files\OpenRacing";

/// Registry root for application settings (`HKLM\SOFTWARE\OpenRacing`).
pub const REGISTRY_APP_KEY: &str = r"SOFTWARE\OpenRacing";

/// Registry key for the Windows service under `HKLM\SYSTEM`.
pub const REGISTRY_SERVICE_KEY: &str = r"SYSTEM\CurrentControlSet\Services\OpenRacingService";

/// MMCSS task registration path for real-time thread priority.
pub const REGISTRY_MMCSS_TASK_KEY: &str =
    r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\OpenRacing";

/// Minimum Windows version required (build 18362 = Windows 10 1903).
pub const MIN_WINDOWS_BUILD: u32 = 18362;

// ---------------------------------------------------------------------------
// GUID helpers
// ---------------------------------------------------------------------------

/// A validated MSI-style GUID (8-4-4-4-12 hex, uppercase).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsiGuid {
    value: String,
}

/// Error returned when a GUID string is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuidError {
    /// The GUID has the wrong length.
    InvalidLength(usize),
    /// The GUID contains characters that are not hex digits or hyphens.
    InvalidCharacter(char),
    /// Hyphens are not in the correct positions (8-4-4-4-12).
    InvalidFormat,
}

impl fmt::Display for GuidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GuidError::InvalidLength(len) => {
                write!(f, "GUID must be 36 characters (8-4-4-4-12), got {len}")
            }
            GuidError::InvalidCharacter(ch) => {
                write!(f, "GUID contains invalid character: '{ch}'")
            }
            GuidError::InvalidFormat => {
                write!(f, "GUID must follow 8-4-4-4-12 hex format")
            }
        }
    }
}

impl std::error::Error for GuidError {}

impl MsiGuid {
    /// Parse and validate a GUID string.
    ///
    /// Accepts with or without braces; normalises to uppercase without braces.
    pub fn parse(input: &str) -> Result<Self, GuidError> {
        let trimmed = input.trim_start_matches('{').trim_end_matches('}');

        if trimmed.len() != 36 {
            return Err(GuidError::InvalidLength(trimmed.len()));
        }

        // Validate 8-4-4-4-12 structure
        let expected_hyphens = [8, 13, 18, 23];
        for (i, ch) in trimmed.chars().enumerate() {
            if expected_hyphens.contains(&i) {
                if ch != '-' {
                    return Err(GuidError::InvalidFormat);
                }
            } else if !ch.is_ascii_hexdigit() {
                return Err(GuidError::InvalidCharacter(ch));
            }
        }

        Ok(Self {
            value: trimmed.to_ascii_uppercase(),
        })
    }

    /// Return the GUID without braces.
    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// Return the GUID wrapped in braces, as used by MSI properties.
    pub fn to_braced(&self) -> String {
        format!("{{{}}}", self.value)
    }
}

impl fmt::Display for MsiGuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.value)
    }
}

// ---------------------------------------------------------------------------
// Windows service configuration
// ---------------------------------------------------------------------------

/// Describes how the Windows service should be registered with SCM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsServiceConfig {
    /// Service name (internal SCM identifier).
    pub service_name: String,
    /// Display name shown in the Services console.
    pub display_name: String,
    /// Human-readable description.
    pub description: String,
    /// Service start type.
    pub start_type: ServiceStartType,
    /// Account under which the service runs.
    pub account: ServiceAccount,
    /// Additional command-line arguments passed to the service binary.
    pub arguments: Vec<String>,
    /// Recovery action on first failure.
    pub first_failure_action: RecoveryAction,
    /// Recovery action on second failure.
    pub second_failure_action: RecoveryAction,
    /// Recovery action on subsequent failures.
    pub third_failure_action: RecoveryAction,
    /// Delay in seconds before restarting after failure.
    pub restart_delay_seconds: u32,
    /// Days after which the failure counter resets.
    pub reset_period_days: u32,
}

/// SCM start type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceStartType {
    /// Start automatically on boot (before user logon).
    Auto,
    /// Start automatically, but delayed (after other auto-start services).
    DelayedAuto,
    /// Start only when explicitly requested.
    Demand,
    /// Service is disabled and cannot be started.
    Disabled,
}

impl fmt::Display for ServiceStartType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServiceStartType::Auto => write!(f, "auto"),
            ServiceStartType::DelayedAuto => write!(f, "delayed-auto"),
            ServiceStartType::Demand => write!(f, "demand"),
            ServiceStartType::Disabled => write!(f, "disabled"),
        }
    }
}

/// Account under which the service runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceAccount {
    /// The built-in LocalSystem account (full system privileges).
    LocalSystem,
    /// LocalService — reduced privileges, network access as anonymous.
    LocalService,
    /// NetworkService — reduced privileges, network access as machine account.
    NetworkService,
    /// A specific user account.
    User(String),
}

impl fmt::Display for ServiceAccount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServiceAccount::LocalSystem => write!(f, "LocalSystem"),
            ServiceAccount::LocalService => write!(f, r"NT AUTHORITY\LocalService"),
            ServiceAccount::NetworkService => write!(f, r"NT AUTHORITY\NetworkService"),
            ServiceAccount::User(name) => write!(f, "{name}"),
        }
    }
}

/// Recovery action taken when the service fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Do nothing.
    None,
    /// Restart the service.
    Restart,
    /// Reboot the computer.
    Reboot,
    /// Run a configured command.
    RunCommand,
}

impl fmt::Display for RecoveryAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecoveryAction::None => write!(f, "none"),
            RecoveryAction::Restart => write!(f, "restart"),
            RecoveryAction::Reboot => write!(f, "reboot"),
            RecoveryAction::RunCommand => write!(f, "run"),
        }
    }
}

impl Default for WindowsServiceConfig {
    fn default() -> Self {
        Self {
            service_name: SERVICE_NAME.to_string(),
            display_name: SERVICE_DISPLAY_NAME.to_string(),
            description: "Provides real-time force feedback processing for racing wheels at 1kHz."
                .to_string(),
            start_type: ServiceStartType::Auto,
            account: ServiceAccount::LocalSystem,
            arguments: vec![
                "--service".to_string(),
                "--config".to_string(),
                r"[ConfigFolder]service.toml".to_string(),
            ],
            first_failure_action: RecoveryAction::Restart,
            second_failure_action: RecoveryAction::Restart,
            third_failure_action: RecoveryAction::None,
            restart_delay_seconds: 5,
            reset_period_days: 1,
        }
    }
}

impl WindowsServiceConfig {
    /// Build the `sc create` command line for manual service registration.
    pub fn sc_create_command(&self, exe_path: &Path) -> String {
        let bin = escape_windows_path(exe_path);
        let start = match self.start_type {
            ServiceStartType::Auto | ServiceStartType::DelayedAuto => "auto",
            ServiceStartType::Demand => "demand",
            ServiceStartType::Disabled => "disabled",
        };
        format!(
            "sc create {name} binPath= \"{bin}\" start= {start} obj= {account} DisplayName= \"{display}\"",
            name = self.service_name,
            account = self.account,
            display = self.display_name,
        )
    }

    /// Build the `sc delete` command line for manual service removal.
    pub fn sc_delete_command(&self) -> String {
        format!("sc delete {}", self.service_name)
    }

    /// Validate that the configuration is internally consistent.
    pub fn validate(&self) -> Result<(), String> {
        if self.service_name.is_empty() {
            return Err("Service name must not be empty".to_string());
        }
        if self.service_name.len() > 256 {
            return Err("Service name must not exceed 256 characters".to_string());
        }
        if self.service_name.contains('/') || self.service_name.contains('\\') {
            return Err("Service name must not contain path separators".to_string());
        }
        if self.display_name.is_empty() {
            return Err("Display name must not be empty".to_string());
        }
        if self.restart_delay_seconds == 0 && self.first_failure_action == RecoveryAction::Restart {
            return Err("Restart delay must be >0 when restart recovery is configured".to_string());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Registry configuration
// ---------------------------------------------------------------------------

/// A set of registry values that the installer should write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryEntry {
    /// Full registry key path (e.g. `SOFTWARE\OpenRacing`).
    pub key: String,
    /// Value name.
    pub name: String,
    /// Value data.
    pub data: RegistryData,
}

/// Typed registry value data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryData {
    /// REG_SZ
    String(String),
    /// REG_DWORD
    Dword(u32),
}

/// Build the standard set of registry entries the installer should create.
pub fn default_registry_entries(install_dir: &Path, version: &str) -> Vec<RegistryEntry> {
    let install_str = escape_windows_path(install_dir);
    vec![
        // Application metadata
        RegistryEntry {
            key: REGISTRY_APP_KEY.to_string(),
            name: "InstallPath".to_string(),
            data: RegistryData::String(install_str.clone()),
        },
        RegistryEntry {
            key: REGISTRY_APP_KEY.to_string(),
            name: "Version".to_string(),
            data: RegistryData::String(version.to_string()),
        },
        RegistryEntry {
            key: REGISTRY_APP_KEY.to_string(),
            name: "ServiceName".to_string(),
            data: RegistryData::String(SERVICE_NAME.to_string()),
        },
        // Device access
        RegistryEntry {
            key: format!("{REGISTRY_APP_KEY}\\DeviceAccess"),
            name: "Enabled".to_string(),
            data: RegistryData::Dword(1),
        },
        // Auto-start entry (Run key so the UI tray app starts on logon)
        RegistryEntry {
            key: r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run".to_string(),
            name: "OpenRacing".to_string(),
            data: RegistryData::String(format!(
                "\"{}\\bin\\openracing.exe\" --minimized",
                install_str
            )),
        },
    ]
}

/// Build the MMCSS real-time priority registry entries.
pub fn mmcss_registry_entries() -> Vec<RegistryEntry> {
    vec![
        RegistryEntry {
            key: REGISTRY_MMCSS_TASK_KEY.to_string(),
            name: "Affinity".to_string(),
            data: RegistryData::Dword(0),
        },
        RegistryEntry {
            key: REGISTRY_MMCSS_TASK_KEY.to_string(),
            name: "Background Only".to_string(),
            data: RegistryData::String("False".to_string()),
        },
        RegistryEntry {
            key: REGISTRY_MMCSS_TASK_KEY.to_string(),
            name: "Clock Rate".to_string(),
            data: RegistryData::Dword(10000),
        },
        RegistryEntry {
            key: REGISTRY_MMCSS_TASK_KEY.to_string(),
            name: "GPU Priority".to_string(),
            data: RegistryData::Dword(8),
        },
        RegistryEntry {
            key: REGISTRY_MMCSS_TASK_KEY.to_string(),
            name: "Priority".to_string(),
            data: RegistryData::Dword(6),
        },
        RegistryEntry {
            key: REGISTRY_MMCSS_TASK_KEY.to_string(),
            name: "Scheduling Category".to_string(),
            data: RegistryData::String("High".to_string()),
        },
        RegistryEntry {
            key: REGISTRY_MMCSS_TASK_KEY.to_string(),
            name: "SFIO Priority".to_string(),
            data: RegistryData::String("High".to_string()),
        },
        RegistryEntry {
            key: REGISTRY_MMCSS_TASK_KEY.to_string(),
            name: "Latency Sensitive".to_string(),
            data: RegistryData::String("True".to_string()),
        },
    ]
}

// ---------------------------------------------------------------------------
// Windows path escaping
// ---------------------------------------------------------------------------

/// Escape a filesystem path for use in Windows registry values and MSI properties.
///
/// - Normalises forward slashes to backslashes.
/// - Strips any trailing backslash (required by some MSI contexts).
/// - Preserves UNC paths (`\\server\share`).
pub fn escape_windows_path(path: &Path) -> String {
    let s = path.to_string_lossy();
    let normalised = s.replace('/', "\\");
    normalised.trim_end_matches('\\').to_string()
}

/// Escape a path for embedding inside WiX XML attribute values.
///
/// WiX uses `[Property]` syntax for installer properties, but literal
/// backslashes and quotes must be handled correctly.
pub fn escape_wix_path(path: &Path) -> String {
    let escaped = escape_windows_path(path);
    // In WiX XML attributes, ampersands and angle brackets must be escaped.
    escaped
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Build the full install directory path from individual components.
///
/// Returns `program_files\OpenRacing` by default.
pub fn default_install_path() -> PathBuf {
    PathBuf::from(r"C:\Program Files\OpenRacing")
}

/// Build a path to the `ProgramData` directory for service state.
pub fn program_data_path() -> PathBuf {
    PathBuf::from(r"C:\ProgramData\OpenRacing")
}

// ---------------------------------------------------------------------------
// Installer validation helpers
// ---------------------------------------------------------------------------

/// Check whether the current WiX configuration uses the canonical upgrade GUID.
///
/// Returns `Ok(())` if the given GUID matches [`MSI_UPGRADE_CODE`], or an
/// error message describing the mismatch.
pub fn validate_upgrade_guid(guid_in_wxs: &str) -> Result<(), String> {
    let parsed = MsiGuid::parse(guid_in_wxs).map_err(|e| format!("Invalid GUID: {e}"))?;
    let canonical = MsiGuid::parse(MSI_UPGRADE_CODE).map_err(|e| format!("Internal error: {e}"))?;
    if parsed != canonical {
        return Err(format!(
            "UpgradeCode mismatch: WiX has '{}', expected '{}'",
            parsed, canonical
        ));
    }
    Ok(())
}

/// Verify that all expected directories exist in the installer layout.
pub fn validate_installer_layout(root: &Path) -> Vec<String> {
    let required_dirs = ["bin", "config", "profiles", "plugins", "logs", "docs"];
    let mut errors = Vec::new();
    for dir in &required_dirs {
        let p = root.join(dir);
        if !p.is_dir() {
            errors.push(format!("Missing required directory: {}", p.display()));
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // GUID parsing
    // -----------------------------------------------------------------------

    #[test]
    fn guid_parse_valid() -> Result<(), Box<dyn std::error::Error>> {
        let g = MsiGuid::parse("A1B2C3D4-E5F6-7890-ABCD-EF1234567890")?;
        assert_eq!(g.as_str(), "A1B2C3D4-E5F6-7890-ABCD-EF1234567890");
        Ok(())
    }

    #[test]
    fn guid_parse_with_braces() -> Result<(), Box<dyn std::error::Error>> {
        let g = MsiGuid::parse("{a1b2c3d4-e5f6-7890-abcd-ef1234567890}")?;
        assert_eq!(g.as_str(), "A1B2C3D4-E5F6-7890-ABCD-EF1234567890");
        assert_eq!(g.to_braced(), "{A1B2C3D4-E5F6-7890-ABCD-EF1234567890}");
        Ok(())
    }

    #[test]
    fn guid_parse_lowercase_normalised() -> Result<(), Box<dyn std::error::Error>> {
        let g = MsiGuid::parse("a1b2c3d4-e5f6-7890-abcd-ef1234567890")?;
        assert_eq!(g.as_str(), "A1B2C3D4-E5F6-7890-ABCD-EF1234567890");
        Ok(())
    }

    #[test]
    fn guid_parse_invalid_length() {
        let result = MsiGuid::parse("A1B2C3D4-E5F6");
        assert!(result.is_err());
        let err = result.err();
        assert!(
            matches!(err, Some(GuidError::InvalidLength(_))),
            "Expected InvalidLength, got {err:?}"
        );
    }

    #[test]
    fn guid_parse_invalid_character() {
        let result = MsiGuid::parse("G1B2C3D4-E5F6-7890-ABCD-EF1234567890");
        assert!(result.is_err());
        let err = result.err();
        assert!(
            matches!(err, Some(GuidError::InvalidCharacter('G'))),
            "Expected InvalidCharacter('G'), got {err:?}"
        );
    }

    #[test]
    fn guid_parse_missing_hyphen() {
        let result = MsiGuid::parse("A1B2C3D4XE5F6-7890-ABCD-EF1234567890");
        assert!(result.is_err());
        let err = result.err();
        assert!(
            matches!(err, Some(GuidError::InvalidFormat)),
            "Expected InvalidFormat, got {err:?}"
        );
    }

    // -----------------------------------------------------------------------
    // MSI_UPGRADE_CODE is itself valid
    // -----------------------------------------------------------------------

    #[test]
    fn msi_upgrade_code_is_valid_guid() -> Result<(), Box<dyn std::error::Error>> {
        let g = MsiGuid::parse(MSI_UPGRADE_CODE)?;
        assert_eq!(g.as_str(), MSI_UPGRADE_CODE);
        Ok(())
    }

    #[test]
    fn validate_upgrade_guid_matches_canonical() -> Result<(), Box<dyn std::error::Error>> {
        validate_upgrade_guid(MSI_UPGRADE_CODE).map_err(|e| e.into())
    }

    #[test]
    fn validate_upgrade_guid_rejects_mismatch() {
        let result = validate_upgrade_guid("00000000-0000-0000-0000-000000000000");
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Windows path escaping
    // -----------------------------------------------------------------------

    #[test]
    fn escape_path_forward_slashes() {
        let p = Path::new("C:/Program Files/OpenRacing/bin");
        assert_eq!(escape_windows_path(p), r"C:\Program Files\OpenRacing\bin");
    }

    #[test]
    fn escape_path_trailing_backslash() {
        let p = Path::new(r"C:\OpenRacing\");
        assert_eq!(escape_windows_path(p), r"C:\OpenRacing");
    }

    #[test]
    fn escape_path_unc() {
        let p = Path::new(r"\\server\share\OpenRacing");
        let escaped = escape_windows_path(p);
        assert!(
            escaped.starts_with(r"\\"),
            "UNC prefix preserved: {escaped}"
        );
    }

    #[test]
    fn escape_wix_path_special_chars() {
        let p = Path::new(r"C:\Users\R&D\OpenRacing");
        let escaped = escape_wix_path(p);
        assert!(escaped.contains("&amp;"), "Ampersand escaped: {escaped}");
        assert!(!escaped.contains("&D"), "Raw ampersand gone: {escaped}");
    }

    // -----------------------------------------------------------------------
    // Default install / ProgramData paths
    // -----------------------------------------------------------------------

    #[test]
    fn default_install_path_has_expected_components() {
        let p = default_install_path();
        assert!(
            p.ends_with("OpenRacing"),
            "Install path should end with OpenRacing"
        );
    }

    #[test]
    fn program_data_path_has_expected_components() {
        let p = program_data_path();
        assert!(
            p.ends_with("OpenRacing"),
            "ProgramData path should end with OpenRacing"
        );
    }

    // -----------------------------------------------------------------------
    // WindowsServiceConfig
    // -----------------------------------------------------------------------

    #[test]
    fn default_service_config_is_valid() -> Result<(), Box<dyn std::error::Error>> {
        let cfg = WindowsServiceConfig::default();
        cfg.validate().map_err(|e| e.into())
    }

    #[test]
    fn service_config_empty_name_rejected() {
        let cfg = WindowsServiceConfig {
            service_name: String::new(),
            ..WindowsServiceConfig::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn service_config_name_with_slash_rejected() {
        let cfg = WindowsServiceConfig {
            service_name: "Open/Racing".to_string(),
            ..WindowsServiceConfig::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn service_config_zero_restart_delay_with_restart_action_rejected() {
        let cfg = WindowsServiceConfig {
            restart_delay_seconds: 0,
            first_failure_action: RecoveryAction::Restart,
            ..WindowsServiceConfig::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn service_config_sc_create_command_format() {
        let cfg = WindowsServiceConfig::default();
        let cmd = cfg.sc_create_command(Path::new(r"C:\Program Files\OpenRacing\bin\wheeld.exe"));
        assert!(cmd.contains("sc create OpenRacingService"));
        assert!(cmd.contains("binPath="));
        assert!(cmd.contains("start= auto"));
        assert!(cmd.contains("LocalSystem"));
    }

    #[test]
    fn service_config_sc_delete_command_format() {
        let cfg = WindowsServiceConfig::default();
        let cmd = cfg.sc_delete_command();
        assert_eq!(cmd, "sc delete OpenRacingService");
    }

    // -----------------------------------------------------------------------
    // Registry entries
    // -----------------------------------------------------------------------

    #[test]
    fn default_registry_entries_include_install_path() {
        let entries = default_registry_entries(Path::new(r"C:\Program Files\OpenRacing"), "1.2.3");
        let install_path_entry = entries.iter().find(|e| e.name == "InstallPath");
        assert!(
            install_path_entry.is_some(),
            "Should contain InstallPath entry"
        );
        let entry = install_path_entry.as_ref();
        assert!(entry.is_some());
    }

    #[test]
    fn default_registry_entries_include_version() {
        let entries = default_registry_entries(Path::new(r"C:\OpenRacing"), "0.5.0");
        let version_entry = entries.iter().find(|e| e.name == "Version");
        assert!(version_entry.is_some(), "Should contain Version entry");
        if let Some(entry) = version_entry {
            assert_eq!(entry.data, RegistryData::String("0.5.0".to_string()));
        }
    }

    #[test]
    fn default_registry_entries_include_auto_start() {
        let entries = default_registry_entries(Path::new(r"C:\OpenRacing"), "1.0.0");
        let auto_start = entries
            .iter()
            .find(|e| e.key.contains("CurrentVersion\\Run") && e.name == "OpenRacing");
        assert!(auto_start.is_some(), "Should contain auto-start Run entry");
    }

    #[test]
    fn mmcss_entries_contain_required_values() {
        let entries = mmcss_registry_entries();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"Priority"));
        assert!(names.contains(&"Scheduling Category"));
        assert!(names.contains(&"Latency Sensitive"));
        assert!(names.contains(&"Clock Rate"));
    }

    // -----------------------------------------------------------------------
    // Installer layout validation
    // -----------------------------------------------------------------------

    #[test]
    fn validate_layout_reports_missing_dirs() {
        let tmp = std::env::temp_dir().join("openracing_test_layout_missing");
        // Ensure it doesn't exist (or is empty)
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::create_dir_all(&tmp);

        let errors = validate_installer_layout(&tmp);
        assert!(
            !errors.is_empty(),
            "Should report errors for missing directories"
        );
        assert!(errors.len() >= 6, "Should report all missing dirs");

        // Clean up
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn validate_layout_passes_with_all_dirs() {
        let tmp = std::env::temp_dir().join("openracing_test_layout_ok");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::create_dir_all(&tmp);

        for dir in &["bin", "config", "profiles", "plugins", "logs", "docs"] {
            let _ = std::fs::create_dir_all(tmp.join(dir));
        }

        let errors = validate_installer_layout(&tmp);
        assert!(errors.is_empty(), "No errors expected: {errors:?}");

        // Clean up
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // -----------------------------------------------------------------------
    // Display impls
    // -----------------------------------------------------------------------

    #[test]
    fn service_start_type_display() {
        assert_eq!(ServiceStartType::Auto.to_string(), "auto");
        assert_eq!(ServiceStartType::DelayedAuto.to_string(), "delayed-auto");
        assert_eq!(ServiceStartType::Demand.to_string(), "demand");
        assert_eq!(ServiceStartType::Disabled.to_string(), "disabled");
    }

    #[test]
    fn service_account_display() {
        assert_eq!(ServiceAccount::LocalSystem.to_string(), "LocalSystem");
        assert_eq!(
            ServiceAccount::LocalService.to_string(),
            r"NT AUTHORITY\LocalService"
        );
        assert_eq!(
            ServiceAccount::User("admin".to_string()).to_string(),
            "admin"
        );
    }

    #[test]
    fn recovery_action_display() {
        assert_eq!(RecoveryAction::None.to_string(), "none");
        assert_eq!(RecoveryAction::Restart.to_string(), "restart");
        assert_eq!(RecoveryAction::Reboot.to_string(), "reboot");
        assert_eq!(RecoveryAction::RunCommand.to_string(), "run");
    }

    #[test]
    fn guid_error_display() {
        assert!(GuidError::InvalidLength(10).to_string().contains("10"));
        assert!(GuidError::InvalidCharacter('Z').to_string().contains("'Z'"));
        assert!(GuidError::InvalidFormat.to_string().contains("8-4-4-4-12"));
    }
}
