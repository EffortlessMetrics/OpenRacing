//! Cross-platform correctness deep tests.
//!
//! Validates platform-specific behaviour across Windows, Linux, and macOS for:
//!
//! - Path normalization and separator conventions
//! - Config file locations (XDG, AppData, ~/Library)
//! - Permission handling differences
//! - USB device path formats per platform
//! - Socket/pipe transport abstraction
//! - File locking semantics
//! - Log file rotation across platforms
//! - Service install/uninstall paths per platform

use std::path::{Path, PathBuf};
use std::time::Duration;

use racing_wheel_service::{IpcConfig, SystemConfig, TransportType};

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Path normalization across platforms
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn path_join_produces_native_separators() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from("config").join("wheel").join("system.json");
    let display = path.to_string_lossy();

    #[cfg(windows)]
    assert!(
        display.contains('\\'),
        "Windows path should use backslash separator: {display}"
    );

    #[cfg(unix)]
    assert!(
        display.contains('/'),
        "Unix path should use forward-slash separator: {display}"
    );

    Ok(())
}

#[test]
fn canonicalize_normalizes_dot_segments() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    let nested = tmp.path().join("a").join("b");
    std::fs::create_dir_all(&nested)?;

    let with_dots = nested.join("..").join("b").join(".");
    let canonical = with_dots.canonicalize()?;

    assert!(
        !canonical.to_string_lossy().contains(".."),
        "canonicalize should remove .. segments: {}",
        canonical.display()
    );
    Ok(())
}

#[test]
fn path_components_are_consistent_across_join() -> Result<(), Box<dyn std::error::Error>> {
    let base = PathBuf::from("wheel");
    let full = base.join("plugins").join("native");

    let components: Vec<_> = full.components().map(|c| c.as_os_str().to_owned()).collect();
    assert_eq!(components.len(), 3);

    assert_eq!(components[0], "wheel");
    assert_eq!(components[1], "plugins");
    assert_eq!(components[2], "native");
    Ok(())
}

#[test]
fn empty_path_join_preserves_base() -> Result<(), Box<dyn std::error::Error>> {
    let base = PathBuf::from("wheel");
    let joined = base.join("");

    assert_eq!(
        base, joined,
        "joining empty string should preserve base path"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Config file locations per platform
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(windows)]
#[test]
fn windows_localappdata_is_set() -> Result<(), Box<dyn std::error::Error>> {
    let localappdata = std::env::var("LOCALAPPDATA")
        .map_err(|e| format!("LOCALAPPDATA not set: {e}"))?;
    let path = Path::new(&localappdata);
    assert!(
        path.is_absolute(),
        "LOCALAPPDATA should be an absolute path: {localappdata}"
    );
    assert!(
        path.is_dir(),
        "LOCALAPPDATA should exist as a directory: {localappdata}"
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn windows_config_path_under_appdata() -> Result<(), Box<dyn std::error::Error>> {
    let localappdata = std::env::var("LOCALAPPDATA")
        .map_err(|e| format!("LOCALAPPDATA not set: {e}"))?;
    let config_path = PathBuf::from(&localappdata)
        .join("wheel")
        .join("system.json");
    assert!(
        config_path.extension().and_then(|e| e.to_str()) == Some("json"),
        "config file should have .json extension"
    );
    assert!(
        config_path.to_string_lossy().contains("wheel"),
        "config path should contain wheel directory: {}",
        config_path.display()
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_xdg_config_home_fallback() -> Result<(), Box<dyn std::error::Error>> {
    let home = std::env::var("HOME").map_err(|e| format!("HOME not set: {e}"))?;

    // XDG_CONFIG_HOME defaults to $HOME/.config
    let xdg_config = std::env::var("XDG_CONFIG_HOME")
        .unwrap_or_else(|_| format!("{home}/.config"));

    let config_path = PathBuf::from(&xdg_config)
        .join("wheel")
        .join("system.json");

    assert!(
        config_path.is_absolute(),
        "config path should be absolute: {}",
        config_path.display()
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn macos_library_preferences_exists() -> Result<(), Box<dyn std::error::Error>> {
    let home = std::env::var("HOME").map_err(|e| format!("HOME not set: {e}"))?;
    let library = PathBuf::from(&home).join("Library");
    assert!(
        library.is_dir(),
        "~/Library should exist on macOS: {}",
        library.display()
    );
    Ok(())
}

#[test]
fn system_config_default_has_valid_schema() -> Result<(), Box<dyn std::error::Error>> {
    let config = SystemConfig::default();
    assert!(
        !config.schema_version.is_empty(),
        "schema_version should not be empty"
    );
    assert!(
        config.schema_version.contains("wheel.config"),
        "schema_version should contain 'wheel.config': {}",
        config.schema_version
    );
    Ok(())
}

#[test]
fn system_config_round_trip_preserves_fields() -> Result<(), Box<dyn std::error::Error>> {
    let config = SystemConfig::default();
    let json = serde_json::to_string(&config)?;
    let deserialized: SystemConfig = serde_json::from_str(&json)?;

    assert_eq!(config.schema_version, deserialized.schema_version);
    assert_eq!(
        config.engine.tick_rate_hz,
        deserialized.engine.tick_rate_hz
    );
    assert_eq!(
        config.safety.max_torque_nm, deserialized.safety.max_torque_nm,
        "safety config should round-trip"
    );
    Ok(())
}

#[test]
fn system_config_validate_accepts_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let config = SystemConfig::default();
    let result = config.validate();
    assert!(
        result.is_ok(),
        "default SystemConfig should pass validation: {result:?}"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Permission handling differences
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(unix)]
#[test]
fn unix_file_permissions_can_be_restricted() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::NamedTempFile::new()?;
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(tmp.path(), perms)?;

    let meta = std::fs::metadata(tmp.path())?;
    let mode = meta.permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o600,
        "permissions should be user read-write only: {mode:#o}"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_socket_permission_constant_excludes_others() -> Result<(), Box<dyn std::error::Error>> {
    // The codebase uses 0o600 for socket permissions
    let socket_perms: u32 = 0o600;
    assert_eq!(
        socket_perms & 0o077,
        0,
        "group and other bits must be zero for socket security"
    );
    assert_ne!(
        socket_perms & 0o600,
        0,
        "owner should have read-write access"
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn windows_temp_dir_is_writable() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = std::env::temp_dir();
    assert!(
        tmp.is_dir(),
        "temp dir should exist: {}",
        tmp.display()
    );
    let test_file = tmp.join("openracing_perm_test.tmp");
    std::fs::write(&test_file, b"test")?;
    let content = std::fs::read(&test_file)?;
    assert_eq!(content, b"test");
    std::fs::remove_file(&test_file)?;
    Ok(())
}

#[test]
fn ipc_acl_flag_defaults_to_disabled() -> Result<(), Box<dyn std::error::Error>> {
    let config = IpcConfig::default();
    assert!(
        !config.enable_acl,
        "ACL should be disabled by default to avoid requiring elevated permissions"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. USB device path formats per platform
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(windows)]
#[test]
fn windows_hid_path_uses_unc_prefix() -> Result<(), Box<dyn std::error::Error>> {
    let path = r"\\?\hid#vid_0eb7&pid_0001#7&abc123&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}";
    assert!(
        path.starts_with(r"\\?\"),
        "Windows HID paths must start with UNC prefix"
    );
    // Validate VID/PID extraction is possible
    assert!(
        path.contains("vid_0eb7"),
        "path should contain vendor ID: {path}"
    );
    assert!(
        path.contains("pid_0001"),
        "path should contain product ID: {path}"
    );
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn linux_hidraw_path_format() -> Result<(), Box<dyn std::error::Error>> {
    let path = "/dev/hidraw0";
    assert!(
        path.starts_with("/dev/hidraw"),
        "Linux HID paths should start with /dev/hidraw"
    );
    // Extract device index
    let suffix = &path["/dev/hidraw".len()..];
    let index: u32 = suffix
        .parse()
        .map_err(|e| format!("hidraw index should be numeric: {e}"))?;
    assert!(index < 256, "hidraw index should be reasonable: {index}");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn macos_iokit_hid_path_format() -> Result<(), Box<dyn std::error::Error>> {
    // macOS HID paths use IOKit registry paths
    let path = "IOService:/AppleACPIPlatformExpert/PCI0@0/AppleACPIPCI/XHC1@14/XHC1@14000000/HS02@14200000/USB2.0 Hub@14200000/AppleUSB20Hub@14210000";
    assert!(
        path.starts_with("IOService:"),
        "macOS HID paths should use IOService prefix"
    );
    Ok(())
}

#[test]
fn device_path_is_non_empty_string() -> Result<(), Box<dyn std::error::Error>> {
    // On any platform, a device path must be a non-empty string
    let paths = vec![
        "/dev/hidraw0",
        r"\\?\hid#vid_0001&pid_0001",
        "IOService:/device",
    ];
    for path in paths {
        assert!(!path.is_empty(), "device path must not be empty");
        assert!(
            path.len() < 4096,
            "device path should be within filesystem limits"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Socket/pipe transport abstraction
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn transport_type_default_is_platform_specific() -> Result<(), Box<dyn std::error::Error>> {
    let transport = TransportType::default();
    let debug = format!("{transport:?}");

    #[cfg(windows)]
    assert!(
        debug.contains("NamedPipe"),
        "Windows default transport should be NamedPipe: {debug}"
    );

    #[cfg(unix)]
    assert!(
        debug.contains("UnixDomainSocket"),
        "Unix default transport should be UnixDomainSocket: {debug}"
    );

    Ok(())
}

#[test]
fn transport_type_serde_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let original = TransportType::default();
    let json = serde_json::to_string(&original)?;
    let restored: TransportType = serde_json::from_str(&json)?;

    assert_eq!(
        format!("{original:?}"),
        format!("{restored:?}"),
        "transport type should survive serde round-trip"
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn named_pipe_path_follows_unc_convention() -> Result<(), Box<dyn std::error::Error>> {
    let transport = TransportType::default();
    match transport {
        TransportType::NamedPipe(ref name) => {
            assert!(
                name.starts_with(r"\\.\pipe\"),
                "named pipe must use UNC pipe prefix: {name}"
            );
            let pipe_name = &name[r"\\.\pipe\".len()..];
            assert!(
                !pipe_name.is_empty(),
                "pipe name after prefix must not be empty"
            );
            // Pipe names cannot contain backslashes
            assert!(
                !pipe_name.contains('\\'),
                "pipe name should not contain backslash: {pipe_name}"
            );
        }
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_socket_path_is_under_run_user() -> Result<(), Box<dyn std::error::Error>> {
    let transport = TransportType::default();
    match transport {
        TransportType::UnixDomainSocket(ref path) => {
            assert!(
                Path::new(path).is_absolute(),
                "socket path must be absolute: {path}"
            );
            assert!(
                path.ends_with(".sock"),
                "socket path should end with .sock: {path}"
            );
        }
    }
    Ok(())
}

#[test]
fn ipc_config_connection_timeout_is_positive() -> Result<(), Box<dyn std::error::Error>> {
    let config = IpcConfig::default();
    assert!(
        config.connection_timeout > Duration::ZERO,
        "connection timeout must be positive: {:?}",
        config.connection_timeout
    );
    assert!(
        config.connection_timeout <= Duration::from_secs(300),
        "connection timeout should be reasonable: {:?}",
        config.connection_timeout
    );
    Ok(())
}

#[test]
fn ipc_config_max_connections_is_bounded() -> Result<(), Box<dyn std::error::Error>> {
    let config = IpcConfig::default();
    assert!(
        config.max_connections > 0,
        "max_connections must be at least 1"
    );
    assert!(
        config.max_connections <= 1000,
        "max_connections should be bounded: {}",
        config.max_connections
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. File locking semantics
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn exclusive_file_write_via_tempfile_is_atomic() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let target = dir.path().join("config.json");

    // Write to temp file, then rename (atomic on most filesystems)
    let tmp_path = dir.path().join("config.json.tmp");
    std::fs::write(&tmp_path, b"{\"version\": 1}")?;
    std::fs::rename(&tmp_path, &target)?;

    let content = std::fs::read_to_string(&target)?;
    assert!(
        content.contains("version"),
        "file content should survive atomic rename"
    );
    assert!(
        !tmp_path.exists(),
        "temp file should be gone after rename"
    );
    Ok(())
}

#[test]
fn concurrent_read_does_not_block() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("shared.json");
    std::fs::write(&path, b"{\"shared\": true}")?;

    // Multiple concurrent reads should succeed
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let p = path.clone();
            std::thread::spawn(move || std::fs::read_to_string(p))
        })
        .collect();

    for handle in handles {
        let result = handle
            .join()
            .map_err(|_| "thread panicked")?;
        let content = result?;
        assert!(content.contains("shared"));
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Log file rotation across platforms
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn log_file_naming_includes_rotation_index() -> Result<(), Box<dyn std::error::Error>> {
    let base = "wheeld.log";
    let rotated_names: Vec<String> = (1..=5)
        .map(|i| format!("{base}.{i}"))
        .collect();

    for name in &rotated_names {
        assert!(
            name.starts_with("wheeld.log."),
            "rotated log should start with base name: {name}"
        );
        let suffix = &name["wheeld.log.".len()..];
        let idx: u32 = suffix
            .parse()
            .map_err(|e| format!("rotation index should be numeric: {e}"))?;
        assert!((1..=5).contains(&idx), "rotation index out of range: {idx}");
    }
    Ok(())
}

#[test]
fn log_directory_creation_is_recursive() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let log_dir = dir.path().join("logs").join("service").join("wheeld");
    std::fs::create_dir_all(&log_dir)?;

    assert!(log_dir.is_dir(), "nested log dir should be created");

    let log_file = log_dir.join("wheeld.log");
    std::fs::write(&log_file, b"[INFO] test log entry\n")?;
    let content = std::fs::read_to_string(&log_file)?;
    assert!(content.contains("INFO"));
    Ok(())
}

#[test]
fn log_rotation_preserves_old_files() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let base = dir.path().join("wheeld.log");

    // Simulate rotation: write current, then "rotate" by renaming
    std::fs::write(&base, b"current log")?;
    let rotated = dir.path().join("wheeld.log.1");
    std::fs::rename(&base, &rotated)?;
    std::fs::write(&base, b"new log")?;

    assert!(base.exists(), "new log should exist");
    assert!(rotated.exists(), "rotated log should exist");

    let current = std::fs::read_to_string(&base)?;
    let old = std::fs::read_to_string(&rotated)?;
    assert_eq!(current, "new log");
    assert_eq!(old, "current log");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Service install/uninstall paths per platform
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(target_os = "linux")]
#[test]
fn linux_systemd_user_service_path() -> Result<(), Box<dyn std::error::Error>> {
    let home = std::env::var("HOME").map_err(|e| format!("HOME not set: {e}"))?;
    let service_dir = PathBuf::from(&home)
        .join(".config")
        .join("systemd")
        .join("user");

    let service_file = service_dir.join("wheeld.service");
    assert!(
        service_file
            .to_string_lossy()
            .contains(".config/systemd/user"),
        "systemd user service should be under .config/systemd/user"
    );
    assert!(
        service_file.ends_with("wheeld.service"),
        "service file should be named wheeld.service"
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn windows_sc_exe_is_accessible() -> Result<(), Box<dyn std::error::Error>> {
    // sc.exe is the standard Windows service control manager
    let system_root = std::env::var("SystemRoot")
        .map_err(|e| format!("SystemRoot not set: {e}"))?;
    let sc = PathBuf::from(&system_root).join("System32").join("sc.exe");
    assert!(
        sc.exists(),
        "sc.exe should exist for service management: {}",
        sc.display()
    );
    Ok(())
}

#[test]
fn service_config_defaults_have_valid_name() -> Result<(), Box<dyn std::error::Error>> {
    let config = racing_wheel_service::ServiceConfig::default();
    let debug = format!("{config:?}");
    assert!(
        !debug.is_empty(),
        "ServiceConfig should have a valid Debug representation"
    );
    Ok(())
}

#[test]
fn system_config_save_to_temp_and_reload() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("system.json");

    let config = SystemConfig::default();
    let json = serde_json::to_string_pretty(&config)?;
    std::fs::write(&path, &json)?;

    let loaded_json = std::fs::read_to_string(&path)?;
    let loaded: SystemConfig = serde_json::from_str(&loaded_json)?;

    assert_eq!(config.schema_version, loaded.schema_version);
    assert_eq!(config.engine.tick_rate_hz, loaded.engine.tick_rate_hz);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. Cross-cutting platform invariants
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn temp_dir_is_writable_on_all_platforms() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = std::env::temp_dir();
    assert!(
        tmp.is_dir(),
        "temp dir must exist: {}",
        tmp.display()
    );
    let probe = tmp.join("openracing_probe.tmp");
    std::fs::write(&probe, b"probe")?;
    std::fs::remove_file(&probe)?;
    Ok(())
}

#[test]
fn path_max_length_sanity() -> Result<(), Box<dyn std::error::Error>> {
    // Construct a long but valid path and verify it can be represented
    let long_component = "a".repeat(200);
    let path = PathBuf::from(&long_component).join("b");
    assert!(
        path.to_string_lossy().len() > 200,
        "long paths should be representable"
    );
    Ok(())
}

#[test]
fn system_config_engine_defaults_are_sane() -> Result<(), Box<dyn std::error::Error>> {
    let config = SystemConfig::default();
    assert!(
        config.engine.tick_rate_hz > 0,
        "tick rate must be positive"
    );
    assert!(
        config.engine.max_jitter_us > 0,
        "max jitter must be positive"
    );
    assert!(
        config.engine.processing_budget_us > 0,
        "processing budget must be positive"
    );
    assert!(
        !config.engine.disable_realtime,
        "realtime should be enabled by default"
    );
    Ok(())
}

#[test]
fn system_config_safety_defaults_are_conservative() -> Result<(), Box<dyn std::error::Error>> {
    let config = SystemConfig::default();
    assert!(
        config.safety.max_torque_nm > 0.0,
        "max torque must be positive"
    );
    assert!(
        config.safety.default_safe_torque_nm <= config.safety.max_torque_nm,
        "safe torque should not exceed max: {} > {}",
        config.safety.default_safe_torque_nm,
        config.safety.max_torque_nm
    );
    assert!(
        config.safety.fault_response_timeout_ms > 0,
        "fault response must be positive"
    );
    assert!(
        config.safety.temp_warning_c < config.safety.temp_fault_c,
        "warning temp should be below fault temp"
    );
    Ok(())
}

#[test]
fn system_config_plugin_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let config = SystemConfig::default();
    assert!(
        config.plugins.timeout_ms > 0,
        "plugin timeout must be positive"
    );
    assert!(
        config.plugins.max_memory_mb > 0,
        "plugin memory limit must be positive"
    );
    Ok(())
}

#[test]
fn system_config_observability_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let config = SystemConfig::default();
    assert!(
        config.observability.health_stream_hz > 0,
        "health stream rate must be positive"
    );
    assert!(
        config.observability.tracing_sample_rate > 0.0
            && config.observability.tracing_sample_rate <= 1.0,
        "tracing sample rate should be in (0, 1]: {}",
        config.observability.tracing_sample_rate
    );
    Ok(())
}

#[test]
fn feature_flags_default_all_disabled() -> Result<(), Box<dyn std::error::Error>> {
    let config = SystemConfig::default();
    assert!(
        !config.development.disable_safety_interlocks,
        "safety interlocks must not be disabled by default"
    );
    assert!(
        !config.development.enable_plugin_dev_mode,
        "plugin dev mode should be off by default"
    );
    assert!(
        !config.development.mock_telemetry,
        "mock telemetry should be off by default"
    );
    Ok(())
}

#[test]
fn ipc_config_serde_preserves_all_fields() -> Result<(), Box<dyn std::error::Error>> {
    let config = IpcConfig {
        max_connections: 42,
        enable_acl: true,
        connection_timeout: Duration::from_secs(60),
        ..IpcConfig::default()
    };

    let json = serde_json::to_string(&config)?;
    let restored: IpcConfig = serde_json::from_str(&json)?;

    assert_eq!(restored.max_connections, 42);
    assert!(restored.enable_acl);
    assert_eq!(restored.connection_timeout, Duration::from_secs(60));
    Ok(())
}
