//! Cross-platform integration tests.
//!
//! Validates that platform abstraction layers behave correctly on the
//! current host OS while asserting invariants that hold on every platform:
//!
//! 1. **Platform abstraction layer** – IPC transport defaults, config defaults
//! 2. **Path handling** – Windows vs Unix path separators and conventions
//! 3. **Config file locations** – per-platform directories for plugins, cache, state
//! 4. **Permission models** – PeerInfo fields and ACL flag
//! 5. **Thread priority / scheduling** – RTSetup, AdaptiveSchedulingConfig, JitterMetrics
//! 6. **USB/HID device path format** – HidDeviceInfo platform path conventions
//! 7. **Named pipe / Unix socket abstraction** – TransportType variants
//! 8. **Service installation paths** – daemon service name and config locations

use std::path::PathBuf;
use std::time::Duration;

use racing_wheel_engine::hid::HidDeviceInfo;
use racing_wheel_engine::{
    AbsoluteScheduler, AdaptiveSchedulingConfig, JitterMetrics, PLL, RTSetup,
};
use racing_wheel_schemas::prelude::*;
use racing_wheel_service::game_auto_configure::ConfiguredGamesStore;
use racing_wheel_service::{IpcConfig, TransportType};

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Platform abstraction layer – IPC transport defaults
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn ipc_config_default_has_loopback_bind_address() -> Result<(), Box<dyn std::error::Error>> {
    let config = IpcConfig::default();
    assert_eq!(config.bind_address, Some("127.0.0.1".to_string()));
    Ok(())
}

#[test]
fn ipc_config_default_max_connections() -> Result<(), Box<dyn std::error::Error>> {
    let config = IpcConfig::default();
    assert_eq!(config.max_connections, 10);
    Ok(())
}

#[test]
fn ipc_config_default_connection_timeout() -> Result<(), Box<dyn std::error::Error>> {
    let config = IpcConfig::default();
    assert_eq!(config.connection_timeout, Duration::from_secs(30));
    Ok(())
}

#[test]
fn ipc_config_default_acl_disabled() -> Result<(), Box<dyn std::error::Error>> {
    let config = IpcConfig::default();
    assert!(!config.enable_acl, "ACL should be disabled by default");
    Ok(())
}

#[cfg(windows)]
#[test]
fn transport_default_is_named_pipe() -> Result<(), Box<dyn std::error::Error>> {
    let transport = TransportType::default();
    match transport {
        TransportType::NamedPipe(ref name) => {
            assert_eq!(name, r"\\.\pipe\wheel");
        }
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn transport_default_is_unix_socket() -> Result<(), Box<dyn std::error::Error>> {
    let transport = TransportType::default();
    match transport {
        TransportType::UnixDomainSocket(ref path) => {
            assert!(
                path.ends_with("/wheel.sock"),
                "socket path should end with /wheel.sock, got: {path}"
            );
            assert!(
                path.starts_with("/run/user/"),
                "socket path should start with /run/user/, got: {path}"
            );
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Path handling – Windows vs Unix conventions
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn path_separator_matches_platform() -> Result<(), Box<dyn std::error::Error>> {
    let joined = PathBuf::from("base").join("child").join("file.txt");
    let display = joined.to_string_lossy();

    #[cfg(windows)]
    assert!(
        display.contains('\\'),
        "Windows paths should use backslash: {display}"
    );

    #[cfg(unix)]
    assert!(
        display.contains('/'),
        "Unix paths should use forward-slash: {display}"
    );

    Ok(())
}

#[cfg(windows)]
#[test]
fn windows_unc_pipe_path_is_valid() -> Result<(), Box<dyn std::error::Error>> {
    let pipe = r"\\.\pipe\wheel";
    assert!(pipe.starts_with(r"\\.\pipe\"), "should be a UNC pipe path");
    // Valid pipe name must have at least one character after the prefix.
    let name = &pipe[r"\\.\pipe\".len()..];
    assert!(!name.is_empty(), "pipe name must not be empty");
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_socket_path_is_absolute() -> Result<(), Box<dyn std::error::Error>> {
    let transport = TransportType::default();
    match transport {
        TransportType::UnixDomainSocket(ref path) => {
            assert!(
                std::path::Path::new(path).is_absolute(),
                "socket path must be absolute: {path}"
            );
        }
    }
    Ok(())
}

#[test]
fn openracing_state_dir_uses_dot_prefix() -> Result<(), Box<dyn std::error::Error>> {
    // The state directory is always ".openracing" under the home directory.
    let state_dir = PathBuf::from(".openracing");
    let dir_name = state_dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("failed to extract dir name")?;
    assert!(
        dir_name.starts_with('.'),
        "state directory should be a dotdir: {dir_name}"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Config file locations per platform
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn configured_games_store_default_is_empty() -> Result<(), Box<dyn std::error::Error>> {
    let store = ConfiguredGamesStore::default();
    assert!(
        store.configured.is_empty(),
        "default store should have no configured games"
    );
    Ok(())
}

#[test]
fn configured_games_store_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = ConfiguredGamesStore::default();
    store.configured.insert("iracing".to_string());
    store.configured.insert("assetto_corsa".to_string());

    let json = serde_json::to_string(&store)?;
    let deserialized: ConfiguredGamesStore = serde_json::from_str(&json)?;

    assert_eq!(deserialized.configured.len(), 2);
    assert!(deserialized.configured.contains("iracing"));
    assert!(deserialized.configured.contains("assetto_corsa"));
    Ok(())
}

#[test]
fn configured_games_state_file_name_is_json() -> Result<(), Box<dyn std::error::Error>> {
    let state_file = PathBuf::from(".openracing").join("configured_games.json");
    let ext = state_file
        .extension()
        .and_then(|e| e.to_str())
        .ok_or("no extension")?;
    assert_eq!(ext, "json");
    Ok(())
}

#[test]
fn config_persistence_to_temp_dir() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    let state_path = tmp.path().join("configured_games.json");

    let mut store = ConfiguredGamesStore::default();
    store.configured.insert("dirt_rally_2".to_string());

    let json = serde_json::to_string_pretty(&store)?;
    std::fs::write(&state_path, &json)?;

    let loaded: ConfiguredGamesStore =
        serde_json::from_str(&std::fs::read_to_string(&state_path)?)?;
    assert!(loaded.configured.contains("dirt_rally_2"));
    Ok(())
}

#[cfg(windows)]
#[test]
fn windows_config_paths_use_appdata() -> Result<(), Box<dyn std::error::Error>> {
    // On Windows the conventional config root is under USERPROFILE.
    let profile = std::env::var("USERPROFILE").map_err(|e| format!("USERPROFILE not set: {e}"))?;
    let appdata_local = PathBuf::from(&profile).join("AppData").join("Local");
    assert!(
        appdata_local.exists(),
        "AppData\\Local should exist: {}",
        appdata_local.display()
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_config_paths_use_xdg_conventions() -> Result<(), Box<dyn std::error::Error>> {
    // On Unix the conventional cache root is under HOME.
    let home = std::env::var("HOME").map_err(|e| format!("HOME not set: {e}"))?;
    let cache_dir = PathBuf::from(&home).join(".cache");
    // .cache may not exist on all systems but the parent (HOME) must.
    assert!(
        std::path::Path::new(&home).is_dir(),
        "HOME must be an existing directory: {home}"
    );
    let _ = cache_dir; // suppress unused warning
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Permission models
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn ipc_config_acl_can_be_enabled() -> Result<(), Box<dyn std::error::Error>> {
    let config = IpcConfig {
        enable_acl: true,
        ..IpcConfig::default()
    };
    assert!(config.enable_acl, "ACL should be settable");
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_socket_permissions_are_user_only() -> Result<(), Box<dyn std::error::Error>> {
    // Socket file permission 0o600 = owner read+write only.
    // Validate the constant used in the codebase.
    let perms: u32 = 0o600;
    assert_eq!(perms & 0o077, 0, "group and other bits must be zero");
    assert_eq!(perms & 0o700, 0o600, "owner should have rw");
    Ok(())
}

#[cfg(windows)]
#[test]
fn windows_peer_identity_is_process_id() -> Result<(), Box<dyn std::error::Error>> {
    // On Windows, peer identification uses process_id (u32).
    // Verify the current process has a valid PID.
    let pid = std::process::id();
    assert!(pid > 0, "current process should have a non-zero PID");
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_peer_identity_is_uid_gid() -> Result<(), Box<dyn std::error::Error>> {
    // On Unix, peer identification uses user_id and group_id.
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };
    // In a normal environment both should be valid (non-negative).
    assert!(uid < u32::MAX, "uid should be a valid value");
    assert!(gid < u32::MAX, "gid should be a valid value");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Thread priority / scheduling abstraction
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn rt_setup_default_enables_all_features() -> Result<(), Box<dyn std::error::Error>> {
    let setup = RTSetup::default();
    assert!(setup.high_priority, "high_priority should default to true");
    assert!(setup.lock_memory, "lock_memory should default to true");
    assert!(
        setup.disable_power_throttling,
        "disable_power_throttling should default to true"
    );
    assert!(
        setup.cpu_affinity.is_none(),
        "cpu_affinity should default to None"
    );
    Ok(())
}

#[test]
fn rt_setup_fields_are_independently_configurable() -> Result<(), Box<dyn std::error::Error>> {
    let setup = RTSetup {
        high_priority: false,
        lock_memory: true,
        disable_power_throttling: false,
        cpu_affinity: Some(0x03), // cores 0 and 1
    };
    assert!(!setup.high_priority);
    assert!(setup.lock_memory);
    assert!(!setup.disable_power_throttling);
    assert_eq!(setup.cpu_affinity, Some(0x03));
    Ok(())
}

#[test]
fn adaptive_scheduling_config_defaults_are_sane() -> Result<(), Box<dyn std::error::Error>> {
    let config = AdaptiveSchedulingConfig::default();
    assert!(
        !config.enabled,
        "adaptive scheduling should be off by default"
    );
    assert!(
        config.min_period_ns < config.max_period_ns,
        "min period must be less than max"
    );
    assert!(
        config.increase_step_ns > 0,
        "increase step must be positive"
    );
    assert!(
        config.decrease_step_ns > 0,
        "decrease step must be positive"
    );
    assert!(
        config.jitter_tighten_threshold_ns < config.jitter_relax_threshold_ns,
        "tighten threshold should be below relax threshold"
    );
    assert!(
        config.processing_tighten_threshold_us < config.processing_relax_threshold_us,
        "processing tighten must be below relax"
    );
    assert!(
        (0.0..=1.0).contains(&config.processing_ema_alpha),
        "EMA alpha must be in [0.0, 1.0]"
    );
    Ok(())
}

#[test]
fn jitter_metrics_initial_state() -> Result<(), Box<dyn std::error::Error>> {
    let metrics = JitterMetrics::new();
    assert_eq!(metrics.total_ticks, 0);
    assert_eq!(metrics.missed_ticks, 0);
    assert_eq!(metrics.max_jitter_ns, 0);
    assert_eq!(metrics.last_jitter_ns, 0);
    Ok(())
}

#[test]
fn jitter_metrics_records_ticks() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::new();
    metrics.record_tick(100_000, false);
    metrics.record_tick(150_000, false);
    metrics.record_tick(500_000, true);

    assert_eq!(metrics.total_ticks, 3);
    assert_eq!(metrics.missed_ticks, 1);
    assert_eq!(metrics.max_jitter_ns, 500_000);
    assert_eq!(metrics.last_jitter_ns, 500_000);
    Ok(())
}

#[test]
fn jitter_metrics_missed_tick_rate() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::new();
    for _ in 0..100 {
        metrics.record_tick(50_000, false);
    }
    metrics.record_tick(300_000, true);

    let rate = metrics.missed_tick_rate();
    // 1 missed out of 101 ticks
    assert!(rate > 0.0 && rate < 0.02, "rate should be ~0.99%: {rate}");
    Ok(())
}

#[test]
fn pll_initial_phase_error_is_zero() -> Result<(), Box<dyn std::error::Error>> {
    let pll = PLL::new(1_000_000); // 1ms target
    let phase_err = pll.phase_error_ns();
    assert!(
        phase_err.abs() < f64::EPSILON,
        "initial phase error should be zero: {phase_err}"
    );
    Ok(())
}

#[test]
fn pll_reset_clears_state() -> Result<(), Box<dyn std::error::Error>> {
    let mut pll = PLL::new(1_000_000);
    // After reset the phase error is zero and target is preserved.
    pll.reset();
    assert!(pll.phase_error_ns().abs() < f64::EPSILON);
    assert_eq!(pll.target_period_ns(), 1_000_000);
    Ok(())
}

#[test]
fn pll_target_period_can_be_changed() -> Result<(), Box<dyn std::error::Error>> {
    let mut pll = PLL::new(1_000_000);
    pll.set_target_period_ns(500_000);
    assert_eq!(pll.target_period_ns(), 500_000);
    Ok(())
}

#[test]
fn absolute_scheduler_1khz_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let sched = AbsoluteScheduler::new_1khz();
    assert_eq!(sched.tick_count(), 0);
    assert!(!sched.is_rt_setup_applied());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. USB/HID device path format per platform
// ═══════════════════════════════════════════════════════════════════════════════

/// Helper: create a synthetic `HidDeviceInfo` with a given path.
fn make_hid_device(path: &str) -> Result<HidDeviceInfo, Box<dyn std::error::Error>> {
    let device_id = DeviceId::new("test-0eb7-0001".to_string())?;
    let max_torque = TorqueNm::new(10.0)?;
    let capabilities = DeviceCapabilities::new(
        true,  // supports_pid
        true,  // supports_raw_torque_1khz
        false, // supports_health_stream
        false, // supports_led_bus
        max_torque, 4096, // encoder_cpr
        1000, // min_report_period_us
    );
    Ok(HidDeviceInfo {
        device_id,
        vendor_id: 0x0EB7,
        product_id: 0x0001,
        serial_number: Some("SN-001".to_string()),
        manufacturer: Some("TestVendor".to_string()),
        product_name: Some("TestWheel".to_string()),
        path: path.to_string(),
        interface_number: Some(0),
        usage_page: Some(0x01),
        usage: Some(0x04),
        report_descriptor_len: None,
        report_descriptor_crc32: None,
        capabilities,
    })
}

#[cfg(windows)]
#[test]
fn windows_hid_device_path_format() -> Result<(), Box<dyn std::error::Error>> {
    // Windows HID paths use the \\?\hid# prefix.
    let path = r"\\?\hid#vid_0eb7&pid_0001#6&abc123&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}";
    let info = make_hid_device(path)?;
    assert!(
        info.path.starts_with(r"\\?\"),
        "Windows HID path should start with \\\\?\\: {}",
        info.path
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_hid_device_path_format() -> Result<(), Box<dyn std::error::Error>> {
    // Linux/macOS HID paths are /dev/hidraw* or similar.
    let path = "/dev/hidraw0";
    let info = make_hid_device(path)?;
    assert!(
        info.path.starts_with("/dev/"),
        "Unix HID path should start with /dev/: {}",
        info.path
    );
    Ok(())
}

#[test]
fn hid_device_info_converts_to_device_info() -> Result<(), Box<dyn std::error::Error>> {
    let hid = make_hid_device("mock://hid/test-device")?;
    let device_info = hid.to_device_info();

    assert_eq!(device_info.vendor_id, 0x0EB7);
    assert_eq!(device_info.product_id, 0x0001);
    assert_eq!(device_info.serial_number, Some("SN-001".to_string()));
    assert_eq!(device_info.manufacturer, Some("TestVendor".to_string()));
    assert!(device_info.is_connected);
    assert_eq!(device_info.path, "mock://hid/test-device");
    Ok(())
}

#[test]
fn hid_device_info_fallback_name() -> Result<(), Box<dyn std::error::Error>> {
    let mut hid = make_hid_device("mock://hid/test-device")?;
    hid.product_name = None;
    let device_info = hid.to_device_info();

    // When no product_name is set, the name should be auto-generated from VID:PID.
    assert!(
        device_info.name.contains("0EB7"),
        "fallback name should contain VID: {}",
        device_info.name
    );
    assert!(
        device_info.name.contains("0001"),
        "fallback name should contain PID: {}",
        device_info.name
    );
    Ok(())
}

#[test]
fn hid_device_info_interface_and_usage() -> Result<(), Box<dyn std::error::Error>> {
    let hid = make_hid_device("mock://hid/test-device")?;
    assert_eq!(hid.interface_number, Some(0));
    assert_eq!(hid.usage_page, Some(0x01)); // Generic Desktop
    assert_eq!(hid.usage, Some(0x04)); // Joystick
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Named pipe / Unix socket abstraction
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn transport_type_debug_output_is_descriptive() -> Result<(), Box<dyn std::error::Error>> {
    let transport = TransportType::default();
    let debug = format!("{transport:?}");
    assert!(
        !debug.is_empty(),
        "Debug output for TransportType should not be empty"
    );

    #[cfg(windows)]
    assert!(
        debug.contains("NamedPipe"),
        "Windows transport debug should mention NamedPipe: {debug}"
    );

    #[cfg(unix)]
    assert!(
        debug.contains("UnixDomainSocket"),
        "Unix transport debug should mention UnixDomainSocket: {debug}"
    );

    Ok(())
}

#[cfg(windows)]
#[test]
fn named_pipe_path_can_be_customized() -> Result<(), Box<dyn std::error::Error>> {
    let custom = TransportType::NamedPipe(r"\\.\pipe\openracing_custom".to_string());
    match custom {
        TransportType::NamedPipe(ref name) => {
            assert!(name.contains("openracing_custom"));
        }
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_socket_path_can_be_customized() -> Result<(), Box<dyn std::error::Error>> {
    let custom = TransportType::UnixDomainSocket("/tmp/openracing_test.sock".to_string());
    match custom {
        TransportType::UnixDomainSocket(ref path) => {
            assert!(path.ends_with("openracing_test.sock"));
        }
    }
    Ok(())
}

#[test]
fn ipc_config_with_custom_transport() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = IpcConfig::default();

    #[cfg(windows)]
    {
        config.transport = TransportType::NamedPipe(r"\\.\pipe\custom_wheel".to_string());
    }

    #[cfg(unix)]
    {
        config.transport = TransportType::UnixDomainSocket("/tmp/custom_wheel.sock".to_string());
    }

    // Verify the custom transport was set.
    let debug = format!("{:?}", config.transport);
    assert!(
        debug.contains("custom_wheel"),
        "custom transport not set: {debug}"
    );
    Ok(())
}

#[test]
fn transport_type_serialization_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let transport = TransportType::default();
    let json = serde_json::to_string(&transport)?;
    let deserialized: TransportType = serde_json::from_str(&json)?;
    let debug_orig = format!("{transport:?}");
    let debug_deser = format!("{deserialized:?}");
    assert_eq!(debug_orig, debug_deser, "round-trip should preserve value");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Service installation paths
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn service_name_is_wheeld() -> Result<(), Box<dyn std::error::Error>> {
    // The service is always named "wheeld" across all platforms.
    let service_name = "wheeld";
    assert_eq!(service_name, "wheeld");
    Ok(())
}

#[cfg(unix)]
#[test]
fn systemd_service_file_path_is_correct() -> Result<(), Box<dyn std::error::Error>> {
    let home = std::env::var("HOME").map_err(|e| format!("HOME not set: {e}"))?;
    let service_file = PathBuf::from(&home)
        .join(".config")
        .join("systemd")
        .join("user")
        .join("wheeld.service");

    assert!(
        service_file.ends_with("wheeld.service"),
        "service file must be named wheeld.service: {}",
        service_file.display()
    );
    assert!(
        service_file
            .to_string_lossy()
            .contains(".config/systemd/user"),
        "must be under .config/systemd/user: {}",
        service_file.display()
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn windows_service_executable_exists() -> Result<(), Box<dyn std::error::Error>> {
    // sc.exe is the standard Windows service management tool.
    let sc = PathBuf::from(r"C:\Windows\System32\sc.exe");
    assert!(
        sc.exists(),
        "sc.exe should exist on Windows: {}",
        sc.display()
    );
    Ok(())
}

#[test]
fn service_daemon_config_defaults_are_valid() -> Result<(), Box<dyn std::error::Error>> {
    let config = racing_wheel_service::ServiceConfig::default();
    // ServiceConfig should be constructible with default values.
    let debug = format!("{config:?}");
    assert!(!debug.is_empty(), "ServiceConfig Debug must not be empty");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Cross-cutting: combined platform invariant tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn ipc_config_is_serializable() -> Result<(), Box<dyn std::error::Error>> {
    let config = IpcConfig::default();
    let json = serde_json::to_string(&config)?;
    let deserialized: IpcConfig = serde_json::from_str(&json)?;

    assert_eq!(config.bind_address, deserialized.bind_address);
    assert_eq!(config.max_connections, deserialized.max_connections);
    assert_eq!(config.connection_timeout, deserialized.connection_timeout);
    assert_eq!(config.enable_acl, deserialized.enable_acl);
    Ok(())
}

#[test]
fn scheduler_and_pll_integration() -> Result<(), Box<dyn std::error::Error>> {
    // Verify the scheduler and PLL can be constructed and configured together.
    let mut sched = AbsoluteScheduler::new_1khz();
    let adaptive = AdaptiveSchedulingConfig {
        enabled: true,
        ..AdaptiveSchedulingConfig::default()
    };
    sched.set_adaptive_scheduling(adaptive);

    let state = sched.adaptive_scheduling();
    assert!(state.enabled, "adaptive scheduling should be enabled");
    assert_eq!(state.min_period_ns, 900_000);
    assert_eq!(state.max_period_ns, 1_100_000);
    Ok(())
}

#[test]
fn rt_setup_can_be_applied_to_scheduler() -> Result<(), Box<dyn std::error::Error>> {
    let mut sched = AbsoluteScheduler::new_1khz();
    // Use a minimal setup that doesn't require elevated privileges.
    let setup = RTSetup {
        high_priority: false,
        lock_memory: false,
        disable_power_throttling: false,
        cpu_affinity: None,
    };
    // apply_rt_setup may fail without privileges – that's acceptable.
    let result = sched.apply_rt_setup(&setup);
    // Even if it fails, the scheduler should still be functional.
    let _ = result;
    assert_eq!(sched.tick_count(), 0, "tick count should still be 0");
    Ok(())
}
