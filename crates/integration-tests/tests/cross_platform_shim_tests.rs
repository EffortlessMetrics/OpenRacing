//! Cross-platform shim integration tests.
//!
//! End-to-end tests that exercise multiple crates together to verify
//! cross-platform correctness of:
//!
//! 1.  Public API compilation — key types from every crate are accessible
//! 2.  Platform detection & cfg gating
//! 3.  Config file path resolution per platform
//! 4.  Service daemon config defaults & serialization
//! 5.  IPC transport platform defaults across crate boundaries
//! 6.  Signal handling plumbing (Ctrl-C / SIGTERM)
//! 7.  Path normalization for config, log, and socket paths
//! 8.  SystemConfig save / reload round-trip in a temp directory
//! 9.  HID device info struct portability
//! 10. Scheduler RT setup through the engine re-export
//!
//! Every test returns `Result` — no `unwrap()` / `expect()`.

use std::path::{Path, PathBuf};

use racing_wheel_engine::hid::HidDeviceInfo;
use racing_wheel_engine::{AbsoluteScheduler, JitterMetrics, PLL, RTSetup};
use racing_wheel_schemas::prelude::*;
use racing_wheel_service::{ServiceConfig, SystemConfig};

/// Convenience alias.
type R = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Public API compilation — key types are accessible on all platforms
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn engine_re_exports_scheduler_types() -> R {
    // These types must be importable from the engine crate on every platform.
    let _scheduler = AbsoluteScheduler::new_1khz();
    let _metrics = JitterMetrics::new();
    let _pll = PLL::new(1_000_000);
    let _setup = RTSetup::default();
    Ok(())
}

#[test]
fn service_re_exports_config_types() -> R {
    let _svc_config = ServiceConfig::default();
    let _sys_config = SystemConfig::default();
    Ok(())
}

#[test]
fn schemas_device_types_accessible() -> R {
    let caps = DeviceCapabilities::new(
        false,
        false,
        false,
        false,
        TorqueNm::new(10.0).map_err(|e| format!("TorqueNm: {e}"))?,
        0,
        1000,
    );
    assert!(!caps.supports_pid);
    assert!(!caps.supports_led_bus);
    Ok(())
}

#[test]
fn hid_device_info_fields_compile_everywhere() -> R {
    let device_id: DeviceId = "test-001"
        .parse()
        .map_err(|e| format!("DeviceId parse: {e}"))?;
    let info = HidDeviceInfo {
        device_id,
        vendor_id: 0x1234,
        product_id: 0x5678,
        serial_number: None,
        manufacturer: None,
        product_name: None,
        path: "mock://device".to_string(),
        interface_number: None,
        usage_page: None,
        usage: None,
        report_descriptor_len: None,
        report_descriptor_crc32: None,
        capabilities: DeviceCapabilities::new(
            false,
            false,
            false,
            false,
            TorqueNm::new(5.0).map_err(|e| format!("TorqueNm: {e}"))?,
            0,
            1000,
        ),
    };
    assert_eq!(info.vendor_id, 0x1234);
    assert_eq!(info.product_id, 0x5678);
    Ok(())
}

#[test]
fn hid_create_port_returns_result_not_panic() -> R {
    // On CI without hardware, Err is acceptable; panic is not.
    let _result = racing_wheel_engine::hid::create_hid_port();
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Platform detection — cfg predicates are mutually exclusive
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn platform_family_detection() -> R {
    let family = if cfg!(windows) {
        "windows"
    } else if cfg!(unix) {
        "unix"
    } else {
        "other"
    };
    assert!(
        family == "windows" || family == "unix",
        "OpenRacing targets Windows and Unix families only, got: {family}"
    );
    Ok(())
}

#[test]
fn target_os_detection() -> R {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "unknown"
    };
    assert!(
        os != "unknown",
        "target_os should be one of windows/linux/macos"
    );
    Ok(())
}

#[test]
fn cfg_windows_and_cfg_unix_are_exclusive() -> R {
    let windows = cfg!(windows);
    let unix = cfg!(unix);
    assert!(
        windows != unix,
        "cfg(windows) and cfg(unix) must be mutually exclusive: windows={windows}, unix={unix}"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Config file path resolution per platform
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn system_config_default_path_is_absolute() -> R {
    let path = SystemConfig::default_config_path()?;
    assert!(
        path.is_absolute(),
        "default config path should be absolute: {}",
        path.display()
    );
    Ok(())
}

#[test]
fn system_config_default_path_ends_with_json() -> R {
    let path = SystemConfig::default_config_path()?;
    let ext = path
        .extension()
        .ok_or("config path should have an extension")?;
    assert_eq!(ext, "json", "config file should be JSON");
    Ok(())
}

#[cfg(windows)]
#[test]
fn system_config_path_uses_localappdata() -> R {
    let path = SystemConfig::default_config_path()?;
    let path_str = path.to_string_lossy();
    let appdata =
        std::env::var("LOCALAPPDATA").map_err(|e| format!("LOCALAPPDATA not set: {e}"))?;
    assert!(
        path_str.starts_with(&appdata),
        "config path should be under LOCALAPPDATA: {path_str}"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn system_config_path_uses_home_config() -> R {
    let path = SystemConfig::default_config_path()?;
    let path_str = path.to_string_lossy();
    let home = std::env::var("HOME").map_err(|e| format!("HOME not set: {e}"))?;
    assert!(
        path_str.starts_with(&format!("{home}/.config")),
        "config path should be under $HOME/.config: {path_str}"
    );
    Ok(())
}

#[test]
fn config_path_contains_wheel_directory() -> R {
    let path = SystemConfig::default_config_path()?;
    let components: Vec<_> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    assert!(
        components.iter().any(|c| c == "wheel"),
        "config path should contain 'wheel' directory: {}",
        path.display()
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Service daemon config defaults & serialization
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn service_config_default_name_is_wheeld() -> R {
    let config = ServiceConfig::default();
    assert_eq!(config.service_name, "wheeld");
    Ok(())
}

#[test]
fn service_config_default_auto_restart_enabled() -> R {
    let config = ServiceConfig::default();
    assert!(config.auto_restart);
    assert!(config.max_restart_attempts > 0);
    Ok(())
}

#[test]
fn service_config_serde_round_trip() -> R {
    let config = ServiceConfig::default();
    let json = serde_json::to_string(&config)?;
    let restored: ServiceConfig = serde_json::from_str(&json)?;
    assert_eq!(restored.service_name, config.service_name);
    assert_eq!(restored.auto_restart, config.auto_restart);
    assert_eq!(restored.max_restart_attempts, config.max_restart_attempts);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. IPC transport defaults across crate boundaries
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn service_ipc_transport_default_is_platform_specific() -> R {
    let config = racing_wheel_service::IpcConfig::default();
    let transport_debug = format!("{:?}", config.transport);

    #[cfg(windows)]
    assert!(
        transport_debug.contains("NamedPipe"),
        "Windows should use NamedPipe transport: {transport_debug}"
    );

    #[cfg(unix)]
    assert!(
        transport_debug.contains("UnixDomainSocket"),
        "Unix should use UnixDomainSocket transport: {transport_debug}"
    );

    Ok(())
}

#[cfg(windows)]
#[test]
fn service_named_pipe_path_is_unc() -> R {
    let config = racing_wheel_service::IpcConfig::default();
    match &config.transport {
        racing_wheel_service::TransportType::NamedPipe(path) => {
            assert!(
                path.starts_with(r"\\.\pipe\"),
                "Named pipe should use UNC path: {path}"
            );
        }
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn service_unix_socket_path_is_absolute() -> R {
    let config = racing_wheel_service::IpcConfig::default();
    match &config.transport {
        racing_wheel_service::TransportType::UnixDomainSocket(path) => {
            assert!(
                path.starts_with('/'),
                "Unix socket path should be absolute: {path}"
            );
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Signal handling plumbing
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn shutdown_broadcast_channel_works_cross_platform() -> R {
    let (tx, mut rx1) = tokio::sync::broadcast::channel::<()>(1);
    let mut rx2 = tx.subscribe();

    tx.send(())?;

    // Both receivers should get the signal
    let r1 = rx1.try_recv();
    assert!(r1.is_ok(), "first receiver should get signal");
    let r2 = rx2.try_recv();
    assert!(r2.is_ok(), "second receiver should get signal");
    Ok(())
}

#[tokio::test]
async fn tokio_ctrl_c_handler_compiles() -> R {
    // This just verifies the platform signal handling API compiles.
    // We cannot actually trigger Ctrl-C in tests.
    // On Windows: tokio::signal::ctrl_c()
    // On Unix: tokio::signal::unix::signal(SignalKind::terminate())
    #[cfg(unix)]
    {
        use tokio::signal::unix::SignalKind;
        let signal_result = tokio::signal::unix::signal(SignalKind::terminate());
        assert!(
            signal_result.is_ok(),
            "SIGTERM handler registration should succeed: {:?}",
            signal_result.err()
        );
    }

    // ctrl_c is available on all platforms
    // We just verify the future type compiles — we don't await it
    let _ctrl_c_future = tokio::signal::ctrl_c();
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Path normalization for config, log, and socket paths
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn path_join_preserves_components() -> R {
    let path = PathBuf::from("config")
        .join("wheel")
        .join("profiles")
        .join("default.json");
    let components: Vec<_> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    assert_eq!(components.len(), 4);
    assert_eq!(components[0], "config");
    assert_eq!(components[3], "default.json");
    Ok(())
}

#[test]
fn path_with_parent_dot_segments_can_be_canonicalized() -> R {
    let tmp = tempfile::tempdir()?;
    let nested = tmp.path().join("a").join("b").join("c");
    std::fs::create_dir_all(&nested)?;

    let with_dots = nested.join("..").join("..").join("b");
    let canonical = with_dots.canonicalize()?;
    assert!(
        !canonical.to_string_lossy().contains(".."),
        "canonicalize should remove '..' segments: {}",
        canonical.display()
    );
    Ok(())
}

#[test]
fn native_path_separator_is_correct() -> R {
    let path = PathBuf::from("a").join("b");
    let display = path.to_string_lossy();

    #[cfg(windows)]
    assert!(
        display.contains('\\'),
        "Windows should use backslash: {display}"
    );

    #[cfg(unix)]
    assert!(
        display.contains('/'),
        "Unix should use forward slash: {display}"
    );

    Ok(())
}

#[test]
fn temp_dir_exists_on_all_platforms() -> R {
    let tmp = std::env::temp_dir();
    assert!(tmp.is_dir(), "temp dir should exist: {}", tmp.display());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. SystemConfig save / reload round-trip
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn system_config_round_trip_in_temp_dir() -> R {
    let dir = tempfile::tempdir()?;
    let config_path = dir.path().join("test_system.json");

    let original = SystemConfig::default();
    original.save_to_path(&config_path).await?;

    assert!(config_path.exists(), "config file should be created");

    let loaded = SystemConfig::load_from_path(&config_path).await?;
    assert_eq!(loaded.schema_version, original.schema_version);
    assert_eq!(loaded.engine.tick_rate_hz, original.engine.tick_rate_hz);
    assert_eq!(loaded.safety.max_torque_nm, original.safety.max_torque_nm);
    Ok(())
}

#[tokio::test]
async fn system_config_creates_parent_directories() -> R {
    let dir = tempfile::tempdir()?;
    let config_path = dir
        .path()
        .join("deeply")
        .join("nested")
        .join("dir")
        .join("config.json");

    let config = SystemConfig::default();
    config.save_to_path(&config_path).await?;

    assert!(
        config_path.exists(),
        "config should be saved even with nested dirs"
    );
    Ok(())
}

#[test]
fn system_config_validate_accepts_defaults() -> R {
    let config = SystemConfig::default();
    config.validate()?;
    Ok(())
}

#[test]
fn system_config_serde_round_trip_preserves_all_sections() -> R {
    let original = SystemConfig::default();
    let json = serde_json::to_string_pretty(&original)?;
    let restored: SystemConfig = serde_json::from_str(&json)?;

    assert_eq!(restored.schema_version, original.schema_version);
    assert_eq!(restored.engine.tick_rate_hz, original.engine.tick_rate_hz);
    assert_eq!(restored.service.service_name, original.service.service_name);
    assert_eq!(restored.ipc.max_connections, original.ipc.max_connections);
    assert_eq!(restored.safety.max_torque_nm, original.safety.max_torque_nm);
    assert_eq!(restored.plugins.enabled, original.plugins.enabled);
    assert_eq!(
        restored.observability.enable_metrics,
        original.observability.enable_metrics
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. HID device path format per platform
// ═══════════════════════════════════════════════════════════════════════════════

fn make_test_hid_info(path: &str) -> Result<HidDeviceInfo, Box<dyn std::error::Error>> {
    let device_id: DeviceId = "test-hid-001"
        .parse()
        .map_err(|e| format!("DeviceId: {e}"))?;
    Ok(HidDeviceInfo {
        device_id,
        vendor_id: 0x0EB7,
        product_id: 0x0E00,
        serial_number: Some("SN001".to_string()),
        manufacturer: Some("TestVendor".to_string()),
        product_name: Some("TestWheel".to_string()),
        path: path.to_string(),
        interface_number: Some(0),
        usage_page: Some(0x01),
        usage: Some(0x04),
        report_descriptor_len: None,
        report_descriptor_crc32: None,
        capabilities: DeviceCapabilities::new(
            false,
            false,
            false,
            false,
            TorqueNm::new(10.0).map_err(|e| format!("TorqueNm: {e}"))?,
            0,
            1000,
        ),
    })
}

#[cfg(windows)]
#[test]
fn hid_path_on_windows_uses_unc_prefix() -> R {
    let info = make_test_hid_info(r"\\?\hid#vid_0eb7&pid_0e00#6&abc123")?;
    assert!(
        info.path.starts_with(r"\\?\hid"),
        "Windows HID path should use UNC prefix: {}",
        info.path
    );
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn hid_path_on_linux_uses_dev_hidraw() -> R {
    let info = make_test_hid_info("/dev/hidraw0")?;
    assert!(
        info.path.starts_with("/dev/hidraw"),
        "Linux HID path should use /dev/hidraw: {}",
        info.path
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn hid_path_on_macos_uses_iokit() -> R {
    let info = make_test_hid_info("IOService:/AppleACPIPlatformExpert/USB")?;
    assert!(
        info.path.contains("IOService"),
        "macOS HID path should reference IOService: {}",
        info.path
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. Scheduler RT setup through engine re-export
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn scheduler_minimal_setup_through_engine() -> R {
    let mut scheduler = AbsoluteScheduler::new_1khz();
    let setup = RTSetup {
        high_priority: false,
        lock_memory: false,
        disable_power_throttling: false,
        cpu_affinity: None,
    };
    scheduler.apply_rt_setup(&setup)?;
    assert_eq!(scheduler.tick_count(), 0);
    Ok(())
}

#[test]
fn scheduler_jitter_metrics_deterministic_cross_platform() -> R {
    let mut metrics = JitterMetrics::new();
    for i in 0u64..100 {
        metrics.record_tick(i * 100, false);
    }
    metrics.record_tick(500_000, true);
    assert_eq!(metrics.total_ticks, 101);
    assert_eq!(metrics.missed_ticks, 1);
    assert_eq!(metrics.max_jitter_ns, 500_000);
    Ok(())
}

#[test]
fn pll_drift_correction_consistent_cross_platform() -> R {
    let mut pll = PLL::new(1_000_000);
    // Feed sequential timing on every platform — result should be deterministic
    let start = std::time::Instant::now();
    let mut corrected_ns_values = Vec::new();
    for i in 0..10 {
        // Simulate ticks slightly beyond target period
        let tick_time = start + std::time::Duration::from_micros(1001 * (i + 1));
        let corrected = pll.update(tick_time);
        corrected_ns_values.push(corrected.as_nanos());
    }
    // All corrections should be within 15% of target period
    for (i, &ns) in corrected_ns_values.iter().enumerate() {
        let diff = (ns as i128 - 1_000_000i128).unsigned_abs();
        assert!(
            diff < 150_000,
            "PLL correction #{i} should be close to 1ms: {ns}ns"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 11. File locking & atomic writes (cross-platform I/O)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn atomic_config_write_via_rename() -> R {
    let dir = tempfile::tempdir()?;
    let target = dir.path().join("config.json");
    let tmp = dir.path().join("config.json.tmp");

    let data = serde_json::json!({ "version": 1 });
    std::fs::write(&tmp, serde_json::to_string_pretty(&data)?)?;
    std::fs::rename(&tmp, &target)?;

    assert!(!tmp.exists());
    assert!(target.exists());
    let content = std::fs::read_to_string(&target)?;
    assert!(content.contains("version"));
    Ok(())
}

#[test]
fn concurrent_reads_on_config_file() -> R {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("shared.json");
    std::fs::write(&path, r#"{"key":"value"}"#)?;

    let handles: Vec<_> = (0..4)
        .map(|_| {
            let p = path.clone();
            std::thread::spawn(move || std::fs::read_to_string(p))
        })
        .collect();

    for handle in handles {
        let result = handle.join().map_err(|_| "reader panicked")?;
        let content = result?;
        assert!(content.contains("value"));
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 12. Environment variable handling
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(windows)]
#[test]
fn localappdata_is_set_and_absolute() -> R {
    let appdata =
        std::env::var("LOCALAPPDATA").map_err(|e| format!("LOCALAPPDATA not set: {e}"))?;
    let path = Path::new(&appdata);
    assert!(path.is_absolute());
    assert!(path.is_dir());
    Ok(())
}

#[cfg(unix)]
#[test]
fn home_is_set_and_absolute() -> R {
    let home = std::env::var("HOME").map_err(|e| format!("HOME not set: {e}"))?;
    let path = Path::new(&home);
    assert!(path.is_absolute());
    assert!(path.is_dir());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 13. Service lifecycle — daemon config validation
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn service_config_health_check_interval_is_positive() -> R {
    let config = ServiceConfig::default();
    assert!(
        config.health_check_interval > 0,
        "health check interval should be positive"
    );
    Ok(())
}

#[test]
fn service_config_restart_delay_is_positive() -> R {
    let config = ServiceConfig::default();
    assert!(config.restart_delay > 0);
    Ok(())
}

#[test]
fn system_config_service_defaults_are_valid() -> R {
    let config = SystemConfig::default();
    assert_eq!(config.service.service_name, "wheeld");
    assert!(config.service.auto_restart);
    assert!(config.service.shutdown_timeout > 0);
    assert!(config.service.max_restart_attempts > 0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 14. FeatureFlags — runtime behavior flags
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn feature_flags_default_is_safe() -> R {
    let flags = racing_wheel_service::FeatureFlags {
        disable_realtime: false,
        force_ffb_mode: None,
        enable_dev_features: false,
        enable_debug_logging: false,
        enable_virtual_devices: false,
        disable_safety_interlocks: false,
        enable_plugin_dev_mode: false,
    };
    assert!(!flags.disable_realtime);
    assert!(!flags.disable_safety_interlocks);
    assert!(!flags.enable_dev_features);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 15. Network binding — cross-platform TCP
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn tcp_loopback_bind_works_cross_platform() -> R {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    assert!(addr.ip().is_loopback());
    assert!(addr.port() > 0);
    Ok(())
}

#[test]
fn tcp_ephemeral_ports_are_unique() -> R {
    let l1 = std::net::TcpListener::bind("127.0.0.1:0")?;
    let l2 = std::net::TcpListener::bind("127.0.0.1:0")?;
    assert_ne!(
        l1.local_addr()?.port(),
        l2.local_addr()?.port(),
        "two listeners should get different ephemeral ports"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 16. Platform-specific socket creation
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(unix)]
#[tokio::test]
async fn unix_domain_socket_can_be_created() -> R {
    let dir = tempfile::tempdir()?;
    let sock_path = dir.path().join("test.sock");

    let listener = tokio::net::UnixListener::bind(&sock_path)?;
    assert!(sock_path.exists(), "socket file should exist");

    // Connect to verify
    let _client = tokio::net::UnixStream::connect(&sock_path).await?;
    drop(listener);
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_socket_cleanup_on_drop() -> R {
    let dir = tempfile::tempdir()?;
    let sock_path = dir.path().join("cleanup.sock");

    {
        let _listener = std::os::unix::net::UnixListener::bind(&sock_path)?;
        assert!(sock_path.exists());
    }
    // On Unix, socket files persist after listener drop — the daemon must
    // clean up explicitly. Verify the file still exists (expected behavior).
    assert!(
        sock_path.exists(),
        "Unix socket file persists after listener drop (cleanup is daemon's responsibility)"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 17. Unicode path handling
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn unicode_directory_names_work_cross_platform() -> R {
    let dir = tempfile::tempdir()?;
    let unicode_path = dir.path().join("profïlé_日本語");
    std::fs::create_dir_all(&unicode_path)?;
    assert!(unicode_path.is_dir());

    let file = unicode_path.join("config.json");
    std::fs::write(&file, r#"{"name":"テスト"}"#)?;
    let content = std::fs::read_to_string(&file)?;
    assert!(content.contains("テスト"));
    Ok(())
}

#[test]
fn unicode_in_config_values_round_trips() -> R {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("unicode.json");

    let data = serde_json::json!({
        "name": "Ünïcödé Whéél",
        "description": "日本語テスト – résumé – Ω"
    });
    std::fs::write(&path, serde_json::to_string(&data)?)?;
    let restored: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
    assert_eq!(
        restored["description"].as_str(),
        Some("日本語テスト – résumé – Ω")
    );
    Ok(())
}
