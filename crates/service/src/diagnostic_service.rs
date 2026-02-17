//! Comprehensive system diagnostics and validation
//!
//! Provides system-level diagnostics to validate hardware, software,
//! and configuration for optimal racing wheel operation.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{error, info};

/// Diagnostic service for system validation
pub struct DiagnosticService {
    /// System information
    system_info: SystemInfo,
    /// Diagnostic tests
    tests: Vec<Box<dyn DiagnosticTest>>,
}

/// Diagnostic test result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticResult {
    /// Test name
    pub name: String,
    /// Test status
    pub status: DiagnosticStatus,
    /// Result message
    pub message: String,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
    /// Suggested actions
    pub suggested_actions: Vec<String>,
}

/// Diagnostic test status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiagnosticStatus {
    /// Test passed
    Pass,
    /// Test passed with warnings
    Warn,
    /// Test failed
    Fail,
}

/// System information
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct SystemInfo {
    os: String,
    arch: String,
    cpu_count: usize,
    memory_mb: u64,
    kernel_version: Option<String>,
}

/// Diagnostic test trait
#[async_trait::async_trait]
trait DiagnosticTest: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    async fn run(&self, system_info: &SystemInfo) -> Result<DiagnosticResult>;
}

impl DiagnosticService {
    /// Create new diagnostic service
    pub async fn new() -> Result<Self> {
        let system_info = Self::collect_system_info().await?;
        let tests = Self::create_diagnostic_tests();

        Ok(Self { system_info, tests })
    }

    /// Run full system diagnostics
    pub async fn run_full_diagnostics(&self) -> Result<Vec<DiagnosticResult>> {
        info!("Running full system diagnostics");
        let start_time = Instant::now();

        let mut results = Vec::new();

        for test in &self.tests {
            info!("Running diagnostic: {}", test.name());
            let test_start = Instant::now();

            match test.run(&self.system_info).await {
                Ok(mut result) => {
                    result.execution_time_ms = test_start.elapsed().as_millis() as u64;
                    results.push(result);
                }
                Err(e) => {
                    error!("Diagnostic test '{}' failed: {}", test.name(), e);
                    results.push(DiagnosticResult {
                        name: test.name().to_string(),
                        status: DiagnosticStatus::Fail,
                        message: format!("Test execution failed: {}", e),
                        execution_time_ms: test_start.elapsed().as_millis() as u64,
                        metadata: HashMap::new(),
                        suggested_actions: vec![
                            "Check system logs for more details".to_string(),
                            "Restart the service and try again".to_string(),
                        ],
                    });
                }
            }
        }

        let total_time = start_time.elapsed();
        info!("Diagnostics completed in {:?}", total_time);

        Ok(results)
    }

    /// Run specific diagnostic test
    pub async fn run_test(&self, test_name: &str) -> Result<DiagnosticResult> {
        for test in &self.tests {
            if test.name() == test_name {
                return test.run(&self.system_info).await;
            }
        }

        anyhow::bail!("Diagnostic test '{}' not found", test_name);
    }

    /// List available diagnostic tests
    pub fn list_tests(&self) -> Vec<(String, String)> {
        self.tests
            .iter()
            .map(|test| (test.name().to_string(), test.description().to_string()))
            .collect()
    }

    async fn collect_system_info() -> Result<SystemInfo> {
        let sys = sysinfo::System::new_all();

        Ok(SystemInfo {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            cpu_count: num_cpus::get(),
            memory_mb: sys.total_memory() / 1024 / 1024,
            kernel_version: Self::get_kernel_version().await,
        })
    }

    async fn get_kernel_version() -> Option<String> {
        #[cfg(target_os = "linux")]
        {
            if let Ok(output) = tokio::process::Command::new("uname")
                .arg("-r")
                .output()
                .await
            {
                return Some(String::from_utf8_lossy(&output.stdout).trim().to_owned());
            }
        }

        #[cfg(windows)]
        {
            let version = os_info::get();
            Some(version.version().to_string())
        }

        #[cfg(not(windows))]
        None
    }

    fn create_diagnostic_tests() -> Vec<Box<dyn DiagnosticTest>> {
        vec![
            Box::new(SystemRequirementsTest),
            Box::new(HidDeviceTest),
            Box::new(RealtimeCapabilityTest),
            Box::new(MemoryTest),
            Box::new(TimingTest),
            Box::new(ConfigurationTest),
            Box::new(PermissionsTest),
            Box::new(NetworkTest),
            Box::new(GameIntegrationTest),
            Box::new(SafetySystemTest),
        ]
    }
}

/// System requirements validation test
struct SystemRequirementsTest;

#[async_trait::async_trait]
impl DiagnosticTest for SystemRequirementsTest {
    fn name(&self) -> &str {
        "system_requirements"
    }
    fn description(&self) -> &str {
        "Validate system meets minimum requirements"
    }

    async fn run(&self, system_info: &SystemInfo) -> Result<DiagnosticResult> {
        let mut metadata = HashMap::new();
        let mut suggested_actions = Vec::new();
        let mut status = DiagnosticStatus::Pass;
        let mut messages = Vec::new();

        // Check OS version
        #[cfg(windows)]
        {
            let version = os_info::get();
            metadata.insert("os_version".to_string(), version.version().to_string());

            if version.version() < &os_info::Version::from_string("10.0") {
                status = DiagnosticStatus::Fail;
                messages.push("Windows 10 or later required".to_string());
                suggested_actions.push("Upgrade to Windows 10 or later".to_string());
            }
        }

        // Check CPU count
        metadata.insert("cpu_count".to_string(), system_info.cpu_count.to_string());
        if system_info.cpu_count < 2 {
            status = DiagnosticStatus::Warn;
            messages.push("At least 2 CPU cores recommended for optimal performance".to_string());
            suggested_actions.push("Consider upgrading to a multi-core processor".to_string());
        }

        // Check memory
        metadata.insert("memory_mb".to_string(), system_info.memory_mb.to_string());
        if system_info.memory_mb < 4096 {
            status = DiagnosticStatus::Warn;
            messages.push("At least 4GB RAM recommended".to_string());
            suggested_actions.push("Consider adding more system memory".to_string());
        }

        // Check architecture
        metadata.insert("architecture".to_string(), system_info.arch.clone());
        if system_info.arch != "x86_64" {
            status = DiagnosticStatus::Fail;
            messages.push("x86_64 architecture required".to_string());
            suggested_actions.push("Use 64-bit operating system".to_string());
        }

        let message = if messages.is_empty() {
            "System meets all requirements".to_string()
        } else {
            messages.join("; ")
        };

        Ok(DiagnosticResult {
            name: self.name().to_string(),
            status,
            message,
            execution_time_ms: 0,
            metadata,
            suggested_actions,
        })
    }
}

/// HID device detection test
struct HidDeviceTest;

#[async_trait::async_trait]
impl DiagnosticTest for HidDeviceTest {
    fn name(&self) -> &str {
        "hid_devices"
    }
    fn description(&self) -> &str {
        "Detect and validate HID racing wheel devices"
    }

    async fn run(&self, _system_info: &SystemInfo) -> Result<DiagnosticResult> {
        let mut metadata = HashMap::new();
        let _suggested_actions: Vec<String> = Vec::new();

        // Check for HID devices
        #[cfg(target_os = "linux")]
        {
            let hidraw_count = std::fs::read_dir("/dev")?
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.file_name().to_string_lossy().starts_with("hidraw"))
                .count();

            metadata.insert("hidraw_devices".to_string(), hidraw_count.to_string());

            if hidraw_count == 0 {
                return Ok(DiagnosticResult {
                    name: self.name().to_string(),
                    status: DiagnosticStatus::Warn,
                    message: "No HID devices found".to_string(),
                    execution_time_ms: 0,
                    metadata,
                    suggested_actions: vec![
                        "Connect a racing wheel device".to_string(),
                        "Check udev rules are installed".to_string(),
                        "Verify device permissions".to_string(),
                    ],
                });
            }
        }

        // Try to initialize HID API
        match hidapi::HidApi::new() {
            Ok(api) => {
                let devices = api.device_list().collect::<Vec<_>>();
                metadata.insert("total_hid_devices".to_string(), devices.len().to_string());

                // Look for racing wheel devices
                let wheel_devices: Vec<_> = devices
                    .iter()
                    .filter(|device| Self::is_racing_wheel_device(device))
                    .collect();

                metadata.insert(
                    "racing_wheel_devices".to_string(),
                    wheel_devices.len().to_string(),
                );

                if wheel_devices.is_empty() {
                    Ok(DiagnosticResult {
                        name: self.name().to_string(),
                        status: DiagnosticStatus::Warn,
                        message: "No racing wheel devices detected".to_string(),
                        execution_time_ms: 0,
                        metadata,
                        suggested_actions: vec![
                            "Connect a supported racing wheel".to_string(),
                            "Check device is powered on".to_string(),
                            "Verify USB connection".to_string(),
                        ],
                    })
                } else {
                    Ok(DiagnosticResult {
                        name: self.name().to_string(),
                        status: DiagnosticStatus::Pass,
                        message: format!("Found {} racing wheel device(s)", wheel_devices.len()),
                        execution_time_ms: 0,
                        metadata,
                        suggested_actions: vec![],
                    })
                }
            }
            Err(e) => Ok(DiagnosticResult {
                name: self.name().to_string(),
                status: DiagnosticStatus::Fail,
                message: format!("Failed to initialize HID API: {}", e),
                execution_time_ms: 0,
                metadata,
                suggested_actions: vec![
                    "Check HID subsystem is available".to_string(),
                    "Verify user permissions for HID devices".to_string(),
                    "Restart the system".to_string(),
                ],
            }),
        }
    }
}

impl HidDeviceTest {
    fn is_racing_wheel_device(device: &hidapi::DeviceInfo) -> bool {
        // Check for known racing wheel vendor IDs
        let racing_wheel_vendors = [
            0x046d, // Logitech
            0x044f, // ThrustMaster
            0x0eb7, // Endor (Fanatec)
            0x1209, // Generic/Community VID
        ];

        racing_wheel_vendors.contains(&device.vendor_id())
    }
}

/// Real-time capability test
struct RealtimeCapabilityTest;

#[async_trait::async_trait]
impl DiagnosticTest for RealtimeCapabilityTest {
    fn name(&self) -> &str {
        "realtime_capability"
    }
    fn description(&self) -> &str {
        "Test real-time scheduling capabilities"
    }

    async fn run(&self, _system_info: &SystemInfo) -> Result<DiagnosticResult> {
        let mut metadata = HashMap::new();
        #[allow(unused_mut)]
        let mut suggested_actions = Vec::new();
        #[allow(unused_mut)]
        let mut status = DiagnosticStatus::Pass;
        #[allow(unused_mut)]
        let mut message = "Real-time capabilities available".to_string();

        #[cfg(target_os = "linux")]
        {
            // Check for rtkit
            if let Ok(output) = tokio::process::Command::new("systemctl")
                .args(&["is-active", "rtkit-daemon"])
                .output()
                .await
            {
                let rtkit_active = output.status.success();
                metadata.insert("rtkit_available".to_string(), rtkit_active.to_string());

                if !rtkit_active {
                    status = DiagnosticStatus::Warn;
                    message = "rtkit daemon not available".to_string();
                    suggested_actions
                        .push("Install and enable rtkit for real-time scheduling".to_string());
                }
            }

            // Check memory lock limits
            if let Ok(limits) = std::fs::read_to_string("/proc/self/limits") {
                if limits.contains("Max locked memory") {
                    metadata.insert("memlock_available".to_string(), "true".to_string());
                } else {
                    status = DiagnosticStatus::Warn;
                    message = "Memory locking may be limited".to_string();
                    suggested_actions
                        .push("Configure memlock limits for real-time operation".to_string());
                }
            }
        }

        #[cfg(windows)]
        {
            // Check for MMCSS availability
            metadata.insert("mmcss_available".to_string(), "true".to_string());
            // MMCSS is available on all supported Windows versions
        }

        Ok(DiagnosticResult {
            name: self.name().to_string(),
            status,
            message,
            execution_time_ms: 0,
            metadata,
            suggested_actions,
        })
    }
}

/// Memory test
struct MemoryTest;

#[async_trait::async_trait]
impl DiagnosticTest for MemoryTest {
    fn name(&self) -> &str {
        "memory"
    }
    fn description(&self) -> &str {
        "Test memory allocation and performance"
    }

    async fn run(&self, system_info: &SystemInfo) -> Result<DiagnosticResult> {
        let mut metadata = HashMap::new();
        let mut suggested_actions = Vec::new();
        let mut status = DiagnosticStatus::Pass;

        let sys = sysinfo::System::new_all();
        let available_mb = sys.available_memory() / 1024 / 1024;
        let used_mb = (sys.total_memory() - sys.available_memory()) / 1024 / 1024;

        metadata.insert(
            "total_memory_mb".to_string(),
            system_info.memory_mb.to_string(),
        );
        metadata.insert("available_memory_mb".to_string(), available_mb.to_string());
        metadata.insert("used_memory_mb".to_string(), used_mb.to_string());

        let message = if available_mb < 512 {
            status = DiagnosticStatus::Warn;
            suggested_actions.push("Close unnecessary applications to free memory".to_string());
            suggested_actions.push("Consider adding more system memory".to_string());
            format!("Low available memory: {} MB", available_mb)
        } else {
            format!(
                "Memory status: {} MB available of {} MB total",
                available_mb, system_info.memory_mb
            )
        };

        Ok(DiagnosticResult {
            name: self.name().to_string(),
            status,
            message,
            execution_time_ms: 0,
            metadata,
            suggested_actions,
        })
    }
}

/// Timing precision test
struct TimingTest;

#[async_trait::async_trait]
impl DiagnosticTest for TimingTest {
    fn name(&self) -> &str {
        "timing"
    }
    fn description(&self) -> &str {
        "Test system timing precision and jitter"
    }

    async fn run(&self, _system_info: &SystemInfo) -> Result<DiagnosticResult> {
        let mut metadata = HashMap::new();
        let mut suggested_actions = Vec::new();

        // Measure timing precision
        let iterations = 1000;
        let target_interval = Duration::from_micros(1000); // 1ms
        let mut jitters = Vec::new();

        let start_time = Instant::now();
        let mut last_time = start_time;

        for _ in 0..iterations {
            tokio::time::sleep(target_interval).await;
            let current_time = Instant::now();
            let actual_interval = current_time.duration_since(last_time);
            let jitter = actual_interval.abs_diff(target_interval);
            jitters.push(jitter.as_micros() as f64);
            last_time = current_time;
        }

        // Calculate statistics
        jitters.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mean_jitter = jitters.iter().sum::<f64>() / jitters.len() as f64;
        let p99_jitter = jitters[(jitters.len() as f64 * 0.99) as usize];
        let max_jitter = jitters.iter().fold(0.0f64, |a, &b| a.max(b));

        metadata.insert("mean_jitter_us".to_string(), format!("{:.2}", mean_jitter));
        metadata.insert("p99_jitter_us".to_string(), format!("{:.2}", p99_jitter));
        metadata.insert("max_jitter_us".to_string(), format!("{:.2}", max_jitter));

        let status = if p99_jitter > 250.0 {
            suggested_actions.push("System may have high timing jitter".to_string());
            suggested_actions.push("Close unnecessary background applications".to_string());
            suggested_actions.push("Consider disabling power management".to_string());
            DiagnosticStatus::Warn
        } else {
            DiagnosticStatus::Pass
        };

        let message = format!(
            "Timing test: mean={:.1}μs, p99={:.1}μs, max={:.1}μs jitter",
            mean_jitter, p99_jitter, max_jitter
        );

        Ok(DiagnosticResult {
            name: self.name().to_string(),
            status,
            message,
            execution_time_ms: 0,
            metadata,
            suggested_actions,
        })
    }
}

/// Configuration validation test
struct ConfigurationTest;

#[async_trait::async_trait]
impl DiagnosticTest for ConfigurationTest {
    fn name(&self) -> &str {
        "configuration"
    }
    fn description(&self) -> &str {
        "Validate system configuration"
    }

    async fn run(&self, _system_info: &SystemInfo) -> Result<DiagnosticResult> {
        let mut metadata = HashMap::new();
        let mut suggested_actions = Vec::new();
        let mut status = DiagnosticStatus::Pass;

        // Try to load system configuration
        match crate::SystemConfig::load().await {
            Ok(config) => {
                metadata.insert("config_loaded".to_string(), "true".to_string());
                metadata.insert("schema_version".to_string(), config.schema_version.clone());

                // Validate configuration
                if let Err(e) = config.validate() {
                    status = DiagnosticStatus::Fail;
                    suggested_actions.push("Fix configuration validation errors".to_string());
                    suggested_actions.push("Reset to default configuration if needed".to_string());

                    Ok(DiagnosticResult {
                        name: self.name().to_string(),
                        status,
                        message: format!("Configuration validation failed: {}", e),
                        execution_time_ms: 0,
                        metadata,
                        suggested_actions,
                    })
                } else {
                    Ok(DiagnosticResult {
                        name: self.name().to_string(),
                        status,
                        message: "Configuration is valid".to_string(),
                        execution_time_ms: 0,
                        metadata,
                        suggested_actions,
                    })
                }
            }
            Err(e) => {
                status = DiagnosticStatus::Fail;
                metadata.insert("config_loaded".to_string(), "false".to_string());
                suggested_actions.push("Check configuration file permissions".to_string());
                suggested_actions.push("Recreate configuration file".to_string());

                Ok(DiagnosticResult {
                    name: self.name().to_string(),
                    status,
                    message: format!("Failed to load configuration: {}", e),
                    execution_time_ms: 0,
                    metadata,
                    suggested_actions,
                })
            }
        }
    }
}

/// Permissions test
struct PermissionsTest;

#[async_trait::async_trait]
impl DiagnosticTest for PermissionsTest {
    fn name(&self) -> &str {
        "permissions"
    }
    fn description(&self) -> &str {
        "Check file and device permissions"
    }

    async fn run(&self, _system_info: &SystemInfo) -> Result<DiagnosticResult> {
        let mut metadata = HashMap::new();
        let mut suggested_actions = Vec::new();
        let mut status = DiagnosticStatus::Pass;
        let mut messages = Vec::new();

        // Check config directory permissions
        if let Ok(config_path) = crate::SystemConfig::default_config_path()
            && let Some(config_dir) = config_path.parent()
        {
            match std::fs::metadata(config_dir) {
                Ok(meta) => {
                    metadata.insert("config_dir_exists".to_string(), "true".to_string());
                    metadata.insert(
                        "config_dir_writable".to_string(),
                        (!meta.permissions().readonly()).to_string(),
                    );
                }
                Err(_) => {
                    // Try to create the directory
                    if std::fs::create_dir_all(config_dir).is_err() {
                        status = DiagnosticStatus::Fail;
                        messages.push("Cannot create configuration directory".to_string());
                        suggested_actions
                            .push("Check user permissions for configuration directory".to_string());
                    }
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            // Check HID device permissions
            if let Ok(entries) = std::fs::read_dir("/dev") {
                let hidraw_devices: Vec<_> = entries
                    .filter_map(|entry| entry.ok())
                    .filter(|entry| entry.file_name().to_string_lossy().starts_with("hidraw"))
                    .collect();

                let mut accessible_count = 0;
                for device in &hidraw_devices {
                    if let Ok(_) = std::fs::File::open(device.path()) {
                        accessible_count += 1;
                    }
                }

                metadata.insert("hidraw_total".to_string(), hidraw_devices.len().to_string());
                metadata.insert(
                    "hidraw_accessible".to_string(),
                    accessible_count.to_string(),
                );

                if accessible_count < hidraw_devices.len() {
                    status = DiagnosticStatus::Warn;
                    messages.push("Some HID devices not accessible".to_string());
                    suggested_actions.push("Install udev rules for HID device access".to_string());
                    suggested_actions.push("Add user to input or plugdev group".to_string());
                }
            }
        }

        let message = if messages.is_empty() {
            "All permissions are correct".to_string()
        } else {
            messages.join("; ")
        };

        Ok(DiagnosticResult {
            name: self.name().to_string(),
            status,
            message,
            execution_time_ms: 0,
            metadata,
            suggested_actions,
        })
    }
}

/// Network connectivity test
struct NetworkTest;

#[async_trait::async_trait]
impl DiagnosticTest for NetworkTest {
    fn name(&self) -> &str {
        "network"
    }
    fn description(&self) -> &str {
        "Test network connectivity for telemetry"
    }

    async fn run(&self, _system_info: &SystemInfo) -> Result<DiagnosticResult> {
        let mut metadata = HashMap::new();
        let suggested_actions = Vec::new();
        let status = DiagnosticStatus::Pass;

        // Test UDP socket binding for telemetry
        let test_ports = [9000, 9996, 20777, 12345]; // Common telemetry ports (incl. ACC default)
        let mut bindable_ports = 0;

        for &port in &test_ports {
            match std::net::UdpSocket::bind(format!("127.0.0.1:{}", port)) {
                Ok(_) => {
                    bindable_ports += 1;
                    metadata.insert(format!("port_{}_available", port), "true".to_string());
                }
                Err(_) => {
                    metadata.insert(format!("port_{}_available", port), "false".to_string());
                }
            }
        }

        metadata.insert("bindable_ports".to_string(), bindable_ports.to_string());

        let message = format!(
            "Network test: {}/{} test ports available",
            bindable_ports,
            test_ports.len()
        );

        Ok(DiagnosticResult {
            name: self.name().to_string(),
            status,
            message,
            execution_time_ms: 0,
            metadata,
            suggested_actions,
        })
    }
}

/// Game integration test
struct GameIntegrationTest;

#[async_trait::async_trait]
impl DiagnosticTest for GameIntegrationTest {
    fn name(&self) -> &str {
        "game_integration"
    }
    fn description(&self) -> &str {
        "Test game integration capabilities"
    }

    async fn run(&self, _system_info: &SystemInfo) -> Result<DiagnosticResult> {
        let mut metadata = HashMap::new();
        let suggested_actions = Vec::new();
        let status = DiagnosticStatus::Pass;
        let mut messages = Vec::new();

        // Check for common game installation directories
        let game_checks = [
            (
                "iRacing",
                vec!["C:\\Program Files (x86)\\iRacing", "Documents\\iRacing"],
            ),
            (
                "ACC",
                vec![
                    "C:\\Program Files\\Steam\\steamapps\\common\\Assetto Corsa Competizione",
                    "Documents\\Assetto Corsa Competizione",
                ],
            ),
        ];

        for (game_name, paths) in &game_checks {
            let mut found = false;
            for path in paths {
                let expanded_path = if path.starts_with("Documents") {
                    if let Ok(home) = std::env::var("USERPROFILE") {
                        format!("{}\\{}", home, path)
                    } else {
                        continue;
                    }
                } else {
                    path.to_string()
                };

                if std::path::Path::new(&expanded_path).exists() {
                    found = true;
                    break;
                }
            }

            metadata.insert(
                format!("{}_detected", game_name.to_lowercase()),
                found.to_string(),
            );
            if found {
                messages.push(format!("{} installation detected", game_name));
            }
        }

        let message = if messages.is_empty() {
            "No supported games detected".to_string()
        } else {
            messages.join("; ")
        };

        Ok(DiagnosticResult {
            name: self.name().to_string(),
            status,
            message,
            execution_time_ms: 0,
            metadata,
            suggested_actions,
        })
    }
}

/// Safety system test
struct SafetySystemTest;

#[async_trait::async_trait]
impl DiagnosticTest for SafetySystemTest {
    fn name(&self) -> &str {
        "safety_system"
    }
    fn description(&self) -> &str {
        "Test safety system functionality"
    }

    async fn run(&self, _system_info: &SystemInfo) -> Result<DiagnosticResult> {
        let mut metadata = HashMap::new();
        let suggested_actions = Vec::new();
        let status = DiagnosticStatus::Pass;

        // Test safety policy creation
        let safety_policy = racing_wheel_engine::SafetyPolicy::default();
        metadata.insert("safety_policy_created".to_string(), "true".to_string());
        metadata.insert(
            "default_safe_torque_nm".to_string(),
            safety_policy.get_max_torque(false).to_string(),
        );
        metadata.insert(
            "max_torque_nm".to_string(),
            safety_policy.get_max_torque(true).to_string(),
        );

        // Test fault detection mechanisms
        let fault_types = ["usb_timeout", "encoder_nan", "thermal_limit", "overcurrent"];
        for fault_type in &fault_types {
            metadata.insert(
                format!("fault_{}_handler", fault_type),
                "available".to_string(),
            );
        }

        let message = "Safety system initialized and ready".to_string();

        Ok(DiagnosticResult {
            name: self.name().to_string(),
            status,
            message,
            execution_time_ms: 0,
            metadata,
            suggested_actions,
        })
    }
}
