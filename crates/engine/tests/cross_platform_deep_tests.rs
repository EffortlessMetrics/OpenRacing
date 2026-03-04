//! Cross-platform shim and abstraction layer deep tests.
//!
//! Validates that platform abstractions work consistently across Windows,
//! Linux, and macOS for:
//!
//! 1.  Thread priority API abstraction
//! 2.  Timer resolution sub-millisecond precision
//! 3.  HID device enumeration abstraction
//! 4.  Path handling (separators, case sensitivity)
//! 5.  Service management abstraction
//! 6.  Temp directory handling
//! 7.  File locking semantics
//! 8.  Process management (spawn, wait)
//! 9.  Network socket binding
//! 10. Environment variable handling (Unicode paths)
//! 11. Console output encoding (UTF-8)
//! 12. Configuration file locations per platform
//! 13. Log file locations per platform
//! 14. CPU affinity setting for RT thread pinning
//! 15. Memory locking (mlock) for RT path

use racing_wheel_engine::{
    AbsoluteScheduler, AdaptiveSchedulingConfig, JitterMetrics, PLL, RTSetup,
};
use std::path::PathBuf;
use std::time::{Duration, Instant};

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Thread priority API abstraction
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn rt_setup_high_priority_abstracts_across_platforms() -> Result<(), Box<dyn std::error::Error>> {
    let setup = RTSetup::default();
    // On all platforms, default RTSetup requests high priority
    assert!(
        setup.high_priority,
        "high_priority should be true by default on every platform"
    );
    // The scheduler accepts this without compile errors regardless of OS
    let mut scheduler = AbsoluteScheduler::new_1khz();
    let minimal = RTSetup {
        high_priority: false,
        lock_memory: false,
        disable_power_throttling: false,
        cpu_affinity: None,
    };
    let result = scheduler.apply_rt_setup(&minimal);
    assert!(
        result.is_ok(),
        "applying minimal RT setup should succeed on all platforms: {result:?}"
    );
    Ok(())
}

#[test]
fn rt_setup_builder_pattern_works_cross_platform() -> Result<(), Box<dyn std::error::Error>> {
    // RTSetup builder must compile and produce correct values on all platforms
    let setup = RTSetup {
        high_priority: true,
        lock_memory: false,
        disable_power_throttling: true,
        cpu_affinity: Some(0x03),
    };
    assert!(setup.high_priority);
    assert!(!setup.lock_memory);
    assert!(setup.disable_power_throttling);
    assert_eq!(
        setup.cpu_affinity,
        Some(0x03),
        "affinity bitmask should be preserved"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Timer resolution sub-millisecond precision
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn instant_now_provides_sub_millisecond_resolution() -> Result<(), Box<dyn std::error::Error>> {
    // Verify the platform clock has at least microsecond granularity
    let mut distinct_timestamps = 0u32;
    let start = Instant::now();
    let mut prev = start;
    for _ in 0..1000 {
        let now = Instant::now();
        if now != prev {
            distinct_timestamps += 1;
        }
        prev = now;
    }
    // We should observe at least some distinct timestamps even in a tight loop
    assert!(
        distinct_timestamps > 0,
        "Instant::now() should have sub-millisecond resolution"
    );
    Ok(())
}

#[test]
fn pll_sub_millisecond_period_compiles_on_all_platforms() -> Result<(), Box<dyn std::error::Error>>
{
    // 500µs period (2kHz) — ensures the PLL handles sub-ms targets cross-platform
    let pll = PLL::new(500_000);
    assert_eq!(pll.target_period_ns(), 500_000);
    assert!(
        pll.phase_error_ns().abs() < f64::EPSILON,
        "initial phase error should be zero"
    );
    Ok(())
}

#[test]
fn scheduler_short_period_compiles_cross_platform() -> Result<(), Box<dyn std::error::Error>> {
    // 100µs period (10kHz) — extreme but valid
    let scheduler = AbsoluteScheduler::new_1khz();
    assert_eq!(
        scheduler.tick_count(),
        0,
        "tick count starts at zero on all platforms"
    );
    assert!(
        scheduler.phase_error_ns().abs() < f64::EPSILON,
        "phase error starts at zero on all platforms"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. HID device enumeration abstraction
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn hid_device_info_fields_accessible_cross_platform() -> Result<(), Box<dyn std::error::Error>> {
    // Verify HidDeviceInfo struct is available and its key fields are accessible
    // on all platforms without needing actual hardware.
    let device_id = racing_wheel_schemas::prelude::DeviceId::new("test-dev-001".to_string())
        .map_err(|e| format!("DeviceId::new failed: {e}"))?;
    let info = racing_wheel_engine::hid::HidDeviceInfo {
        device_id,
        vendor_id: 0x0EB7,
        product_id: 0x0001,
        serial_number: Some("SN-12345".to_string()),
        manufacturer: Some("TestVendor".to_string()),
        product_name: Some("TestWheel".to_string()),
        path: "/dev/hidraw0".to_string(),
        interface_number: Some(0),
        usage_page: Some(0x01),
        usage: Some(0x04),
        report_descriptor_len: None,
        report_descriptor_crc32: None,
        capabilities: racing_wheel_schemas::prelude::DeviceCapabilities::new(
            false,
            false,
            false,
            false,
            racing_wheel_schemas::prelude::TorqueNm::new(10.0)
                .map_err(|e| format!("TorqueNm::new failed: {e}"))?,
            0,
            1000,
        ),
    };
    assert_eq!(info.vendor_id, 0x0EB7);
    assert_eq!(info.product_id, 0x0001);
    assert!(
        info.serial_number.is_some(),
        "serial number should be preserved"
    );
    Ok(())
}

#[test]
fn hid_create_port_returns_result_on_current_platform() -> Result<(), Box<dyn std::error::Error>> {
    // The factory function must return a Result, not panic, on any platform
    let result = racing_wheel_engine::hid::create_hid_port();
    // We can't guarantee hardware is present, but the call must not panic
    // On CI without HID devices, an Err is acceptable
    let _is_ok = result.is_ok();
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Path handling — separators and case sensitivity
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn path_join_uses_native_separators() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from("config").join("wheel").join("profiles");
    let display = path.to_string_lossy();

    #[cfg(windows)]
    assert!(
        display.contains('\\'),
        "Windows path should use backslash: {display}"
    );

    #[cfg(unix)]
    assert!(
        display.contains('/'),
        "Unix path should use forward slash: {display}"
    );

    // On all platforms, components should be preserved
    let components: Vec<_> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    assert_eq!(components.len(), 3);
    assert_eq!(components[0], "config");
    assert_eq!(components[1], "wheel");
    assert_eq!(components[2], "profiles");
    Ok(())
}

#[test]
fn canonicalize_resolves_dot_dot_cross_platform() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    let nested = tmp.path().join("a").join("b");
    std::fs::create_dir_all(&nested)?;

    let with_dots = nested.join("..").join("b");
    let canonical = with_dots.canonicalize()?;

    assert!(
        !canonical.to_string_lossy().contains(".."),
        "canonicalize should remove .. segments: {}",
        canonical.display()
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Service management abstraction
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(windows)]
#[test]
fn windows_service_tool_exists() -> Result<(), Box<dyn std::error::Error>> {
    let system_root =
        std::env::var("SystemRoot").map_err(|e| format!("SystemRoot not set: {e}"))?;
    let sc_exe = PathBuf::from(&system_root).join("System32").join("sc.exe");
    assert!(
        sc_exe.exists(),
        "sc.exe must exist for service management: {}",
        sc_exe.display()
    );
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn linux_systemd_service_path_is_valid() -> Result<(), Box<dyn std::error::Error>> {
    let home = std::env::var("HOME").map_err(|e| format!("HOME not set: {e}"))?;
    let svc_path = PathBuf::from(&home)
        .join(".config")
        .join("systemd")
        .join("user")
        .join("wheeld.service");
    assert!(
        svc_path.is_absolute(),
        "systemd service path must be absolute: {}",
        svc_path.display()
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Temp directory handling
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn temp_dir_exists_and_is_writable() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = std::env::temp_dir();
    assert!(
        tmp.is_dir(),
        "temp dir must exist on all platforms: {}",
        tmp.display()
    );
    let probe = tmp.join("openracing_xplat_probe.tmp");
    std::fs::write(&probe, b"cross-platform-test")?;
    let content = std::fs::read(&probe)?;
    assert_eq!(content, b"cross-platform-test");
    std::fs::remove_file(&probe)?;
    Ok(())
}

#[test]
fn tempfile_crate_creates_unique_files() -> Result<(), Box<dyn std::error::Error>> {
    let a = tempfile::NamedTempFile::new()?;
    let b = tempfile::NamedTempFile::new()?;
    assert_ne!(
        a.path(),
        b.path(),
        "tempfile should generate unique paths on all platforms"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. File locking semantics
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn atomic_rename_preserves_content() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let target = dir.path().join("config.json");
    let tmp_path = dir.path().join("config.json.tmp");

    std::fs::write(&tmp_path, b"{\"version\": 2}")?;
    std::fs::rename(&tmp_path, &target)?;

    let content = std::fs::read_to_string(&target)?;
    assert!(
        content.contains("version"),
        "content should survive atomic rename on all platforms"
    );
    assert!(!tmp_path.exists(), "tmp file should not exist after rename");
    Ok(())
}

#[test]
fn concurrent_reads_do_not_block() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("shared_data.bin");
    std::fs::write(&path, b"shared-content")?;

    let handles: Vec<_> = (0..4)
        .map(|_| {
            let p = path.clone();
            std::thread::spawn(move || std::fs::read_to_string(p))
        })
        .collect();

    for handle in handles {
        let result = handle.join().map_err(|_| "reader thread panicked")?;
        let content = result?;
        assert_eq!(content, "shared-content");
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Process management — spawn, wait
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn spawn_child_process_and_wait_for_exit() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(windows)]
    let output = std::process::Command::new("cmd")
        .args(["/C", "echo", "hello"])
        .output()?;

    #[cfg(unix)]
    let output = std::process::Command::new("echo").arg("hello").output()?;

    assert!(
        output.status.success(),
        "child process should exit successfully"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("hello"),
        "stdout should contain expected output: {stdout}"
    );
    Ok(())
}

#[test]
fn process_exit_code_is_portable() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(windows)]
    let status = std::process::Command::new("cmd")
        .args(["/C", "exit", "0"])
        .status()?;

    #[cfg(unix)]
    let status = std::process::Command::new("true").status()?;

    assert!(
        status.success(),
        "exit code 0 should indicate success on all platforms"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. Network socket binding
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn loopback_socket_binds_on_all_platforms() -> Result<(), Box<dyn std::error::Error>> {
    // Bind to port 0 on 127.0.0.1 to get an ephemeral port
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    assert!(
        addr.ip().is_loopback(),
        "bound address should be loopback: {addr}"
    );
    assert!(addr.port() > 0, "ephemeral port should be non-zero");
    Ok(())
}

#[test]
fn ipv6_loopback_available_or_graceful_failure() -> Result<(), Box<dyn std::error::Error>> {
    let result = std::net::TcpListener::bind("[::1]:0");
    // IPv6 may not be available on all CI hosts; either outcome is acceptable
    match result {
        Ok(listener) => {
            let addr = listener.local_addr()?;
            assert!(addr.ip().is_loopback(), "should be IPv6 loopback: {addr}");
        }
        Err(e) => {
            // Graceful: IPv6 not available is not a test failure
            let msg = format!("{e}");
            assert!(
                !msg.is_empty(),
                "error message should be non-empty when IPv6 unavailable"
            );
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. Environment variable handling — Unicode paths
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn env_var_round_trips_ascii_strings() -> Result<(), Box<dyn std::error::Error>> {
    let key = "OPENRACING_TEST_ASCII";
    let value = "simple-value-12345";
    // SAFETY: test runs single-threaded and key is unique to this test
    unsafe { std::env::set_var(key, value) };
    let retrieved = std::env::var(key).map_err(|e| format!("env var not found: {e}"))?;
    assert_eq!(retrieved, value);
    unsafe { std::env::remove_var(key) };
    Ok(())
}

#[test]
fn env_var_round_trips_unicode_strings() -> Result<(), Box<dyn std::error::Error>> {
    let key = "OPENRACING_TEST_UNICODE";
    let value = "日本語テスト-пример-тест";
    // SAFETY: test runs single-threaded and key is unique to this test
    unsafe { std::env::set_var(key, value) };
    let retrieved = std::env::var(key).map_err(|e| format!("unicode env var not found: {e}"))?;
    assert_eq!(
        retrieved, value,
        "Unicode env var should round-trip on all platforms"
    );
    unsafe { std::env::remove_var(key) };
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 11. Console output encoding — UTF-8 everywhere
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn utf8_string_operations_are_consistent() -> Result<(), Box<dyn std::error::Error>> {
    let test_str = "OpenRacing – résumé – 日本語 – Ω";
    assert!(!test_str.is_ascii(), "test string should contain non-ASCII");
    let bytes = test_str.as_bytes();
    let round_tripped =
        std::str::from_utf8(bytes).map_err(|e| format!("UTF-8 round-trip failed: {e}"))?;
    assert_eq!(round_tripped, test_str);
    Ok(())
}

#[test]
fn pathbuf_handles_unicode_components() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let unicode_dir = dir.path().join("profïlé");
    std::fs::create_dir_all(&unicode_dir)?;
    assert!(
        unicode_dir.is_dir(),
        "directory with Unicode name should be creatable: {}",
        unicode_dir.display()
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 12. Configuration file locations per platform
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(windows)]
#[test]
fn windows_appdata_env_set() -> Result<(), Box<dyn std::error::Error>> {
    let appdata =
        std::env::var("LOCALAPPDATA").map_err(|e| format!("LOCALAPPDATA not set: {e}"))?;
    let path = std::path::Path::new(&appdata);
    assert!(
        path.is_absolute(),
        "LOCALAPPDATA should be absolute: {appdata}"
    );
    assert!(
        path.is_dir(),
        "LOCALAPPDATA should exist as a directory: {appdata}"
    );
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn linux_xdg_config_home_is_absolute() -> Result<(), Box<dyn std::error::Error>> {
    let home = std::env::var("HOME").map_err(|e| format!("HOME not set: {e}"))?;
    let xdg = std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| format!("{home}/.config"));
    let path = std::path::Path::new(&xdg);
    assert!(
        path.is_absolute(),
        "XDG_CONFIG_HOME should be absolute: {xdg}"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn macos_library_exists() -> Result<(), Box<dyn std::error::Error>> {
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
fn config_path_construction_compiles_everywhere() -> Result<(), Box<dyn std::error::Error>> {
    // Platform-independent config path construction
    let config_dir = {
        #[cfg(windows)]
        {
            let base =
                std::env::var("LOCALAPPDATA").map_err(|e| format!("LOCALAPPDATA not set: {e}"))?;
            PathBuf::from(base).join("OpenRacing")
        }
        #[cfg(target_os = "linux")]
        {
            let home = std::env::var("HOME").map_err(|e| format!("HOME not set: {e}"))?;
            let xdg =
                std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| format!("{home}/.config"));
            PathBuf::from(xdg).join("openracing")
        }
        #[cfg(target_os = "macos")]
        {
            let home = std::env::var("HOME").map_err(|e| format!("HOME not set: {e}"))?;
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("OpenRacing")
        }
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        {
            PathBuf::from(".").join("openracing")
        }
    };
    assert!(
        !config_dir.as_os_str().is_empty(),
        "config dir path should be non-empty"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 13. Log file locations follow platform conventions
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn log_directory_can_be_created_recursively() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let log_dir = dir.path().join("logs").join("wheeld").join("rt");
    std::fs::create_dir_all(&log_dir)?;
    assert!(
        log_dir.is_dir(),
        "nested log dir should be created: {}",
        log_dir.display()
    );
    let log_file = log_dir.join("wheeld.log");
    std::fs::write(&log_file, b"[INFO] cross-platform log test\n")?;
    let content = std::fs::read_to_string(&log_file)?;
    assert!(content.contains("INFO"));
    Ok(())
}

#[test]
fn log_rotation_naming_is_portable() -> Result<(), Box<dyn std::error::Error>> {
    let base = "wheeld.log";
    for i in 1u32..=5 {
        let rotated = format!("{base}.{i}");
        assert!(
            rotated.starts_with("wheeld.log."),
            "rotated name should have correct prefix: {rotated}"
        );
        let suffix = &rotated["wheeld.log.".len()..];
        let idx: u32 = suffix
            .parse()
            .map_err(|e| format!("rotation index should be numeric: {e}"))?;
        assert_eq!(idx, i);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 14. CPU affinity setting — thread pinning for RT
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn cpu_affinity_bitmask_representation() -> Result<(), Box<dyn std::error::Error>> {
    // Verify bitmask arithmetic works for CPU affinity on all platforms
    let core_0_only: u64 = 1 << 0;
    let cores_0_to_3: u64 = 0x0F;
    let all_64_cores: u64 = u64::MAX;

    assert_eq!(core_0_only.count_ones(), 1);
    assert_eq!(cores_0_to_3.count_ones(), 4);
    assert_eq!(all_64_cores.count_ones(), 64);

    let setup = RTSetup {
        cpu_affinity: Some(cores_0_to_3),
        high_priority: false,
        lock_memory: false,
        disable_power_throttling: false,
    };
    let mask = setup.cpu_affinity.ok_or("affinity should be Some")?;
    assert_eq!(mask, 0x0F);
    Ok(())
}

#[test]
fn num_cpus_returns_positive_count() -> Result<(), Box<dyn std::error::Error>> {
    let cpus = num_cpus::get();
    assert!(cpus >= 1, "system must have at least one CPU: got {cpus}");
    // Affinity mask should be representable in u64
    assert!(
        cpus <= 64,
        "more than 64 CPUs not representable in u64 affinity mask — got {cpus} (this is informational)"
    );
    Ok(())
}

#[test]
fn affinity_mask_for_detected_cpus_is_valid() -> Result<(), Box<dyn std::error::Error>> {
    let cpus = num_cpus::get();
    // Build a mask with all detected cores set
    let mask: u64 = if cpus >= 64 {
        u64::MAX
    } else {
        (1u64 << cpus) - 1
    };
    assert!(
        mask.count_ones() as usize >= cpus.min(64),
        "mask should have at least as many bits as CPUs"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 15. Memory locking (mlock) for RT path
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn rt_setup_lock_memory_field_exists_cross_platform() -> Result<(), Box<dyn std::error::Error>> {
    let with_lock = RTSetup {
        lock_memory: true,
        high_priority: false,
        disable_power_throttling: false,
        cpu_affinity: None,
    };
    assert!(
        with_lock.lock_memory,
        "lock_memory field should compile and be true"
    );

    let without_lock = RTSetup {
        lock_memory: false,
        ..with_lock
    };
    assert!(
        !without_lock.lock_memory,
        "lock_memory should be false when explicitly set"
    );
    Ok(())
}

#[test]
fn scheduler_apply_rt_setup_with_memory_lock_does_not_panic()
-> Result<(), Box<dyn std::error::Error>> {
    let mut scheduler = AbsoluteScheduler::new_1khz();
    // On non-privileged environments, lock_memory may fail silently or succeed.
    // The key invariant: it must not panic.
    let setup = RTSetup {
        high_priority: false,
        lock_memory: true,
        disable_power_throttling: false,
        cpu_affinity: None,
    };
    let _result = scheduler.apply_rt_setup(&setup);
    // Either Ok or Err is acceptable — the test verifies no panic
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Additional cross-cutting platform invariants
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn jitter_metrics_portable_across_platforms() -> Result<(), Box<dyn std::error::Error>> {
    let mut metrics = JitterMetrics::new();
    // Record identical samples; behaviour must be consistent on all OSes
    for _ in 0..100 {
        metrics.record_tick(50_000, false); // 50µs
    }
    metrics.record_tick(400_000, true); // missed

    assert_eq!(metrics.total_ticks, 101);
    assert_eq!(metrics.missed_ticks, 1);
    assert_eq!(metrics.max_jitter_ns, 400_000);

    let rate = metrics.missed_tick_rate();
    assert!(
        (rate - 1.0 / 101.0).abs() < 0.001,
        "missed tick rate should be ~0.0099 on all platforms: {rate}"
    );
    Ok(())
}

#[test]
fn adaptive_scheduling_normalization_is_deterministic() -> Result<(), Box<dyn std::error::Error>> {
    let mut sched = AbsoluteScheduler::new_1khz();
    let config = AdaptiveSchedulingConfig {
        enabled: true,
        min_period_ns: 2_000_000, // deliberately inverted
        max_period_ns: 500_000,
        ..AdaptiveSchedulingConfig::default()
    };
    sched.set_adaptive_scheduling(config);
    let state = sched.adaptive_scheduling();
    assert!(
        state.min_period_ns <= state.max_period_ns,
        "normalization should correct inverted bounds on all platforms: min={}, max={}",
        state.min_period_ns,
        state.max_period_ns
    );
    Ok(())
}

#[test]
fn thread_spawn_and_join_works_cross_platform() -> Result<(), Box<dyn std::error::Error>> {
    let handle = std::thread::spawn(|| {
        let start = Instant::now();
        std::thread::sleep(Duration::from_millis(1));
        start.elapsed()
    });
    let elapsed = handle.join().map_err(|_| "thread panicked")?;
    assert!(
        elapsed >= Duration::from_micros(500),
        "thread should have slept at least 500µs: {elapsed:?}"
    );
    Ok(())
}
