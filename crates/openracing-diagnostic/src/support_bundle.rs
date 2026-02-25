//! Support Bundle Generation
//!
//! Creates comprehensive diagnostic packages (<25MB for 2-minute capture)
//! with logs, profiles, system info, and recent recordings.
//!
//! # Example
//!
//! ```no_run
//! use openracing_diagnostic::{SupportBundle, SupportBundleConfig, HealthEventData};
//! use std::path::Path;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let config = SupportBundleConfig::default();
//! let mut bundle = SupportBundle::new(config);
//!
//! // Add health events
//! let event = HealthEventData {
//!     timestamp_ns: 0,
//!     device_id: "device-001".to_string(),
//!     event_type: "DeviceConnected".to_string(),
//!     context: serde_json::json!({"test": true}),
//! };
//! bundle.add_health_events(&[event])?;
//!
//! // Add system information
//! bundle.add_system_info()?;
//!
//! // Generate bundle
//! bundle.generate(Path::new("./support_bundle.zip"))?;
//! # Ok(())
//! # }
//! ```

use crate::error::{DiagnosticError, DiagnosticResult};
use crate::streams::HealthEventData;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{File, read_dir},
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use sysinfo::System;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

/// Support bundle configuration
#[derive(Debug, Clone)]
pub struct SupportBundleConfig {
    /// Include log files
    pub include_logs: bool,
    /// Include profile configurations
    pub include_profiles: bool,
    /// Include system information
    pub include_system_info: bool,
    /// Include recent blackbox recordings
    pub include_recent_recordings: bool,
    /// Maximum bundle size in MB
    pub max_bundle_size_mb: u64,
}

impl Default for SupportBundleConfig {
    fn default() -> Self {
        Self {
            include_logs: true,
            include_profiles: true,
            include_system_info: true,
            include_recent_recordings: true,
            max_bundle_size_mb: 25,
        }
    }
}

/// System information collected for diagnostics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    /// Operating system information
    pub os_info: OsInfo,
    /// Hardware information
    pub hardware_info: HardwareInfo,
    /// Process information
    pub process_info: ProcessInfo,
    /// Environment variables (filtered)
    pub environment: HashMap<String, String>,
    /// Timestamp when collected
    pub collected_at: SystemTime,
}

/// Operating system information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsInfo {
    /// OS name
    pub name: String,
    /// OS version
    pub version: String,
    /// Kernel version
    pub kernel_version: String,
    /// Hostname
    pub hostname: String,
    /// System uptime in seconds
    pub uptime_seconds: u64,
}

/// Hardware information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    /// CPU information
    pub cpu_info: CpuInfo,
    /// Memory information
    pub memory_info: MemoryInfo,
    /// Disk information
    pub disk_info: Vec<DiskInfo>,
    /// Network information
    pub network_info: Vec<NetworkInfo>,
}

/// CPU information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    /// CPU brand string
    pub brand: String,
    /// CPU frequency in MHz
    pub frequency_mhz: u64,
    /// Number of CPU cores
    pub core_count: usize,
    /// CPU usage percentage
    pub usage_percent: f32,
}

/// Memory information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInfo {
    /// Total memory in MB
    pub total_mb: u64,
    /// Available memory in MB
    pub available_mb: u64,
    /// Used memory in MB
    pub used_mb: u64,
    /// Memory usage percentage
    pub usage_percent: f64,
}

/// Disk information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskInfo {
    /// Disk name
    pub name: String,
    /// Mount point
    pub mount_point: String,
    /// Total disk space in GB
    pub total_gb: u64,
    /// Available disk space in GB
    pub available_gb: u64,
    /// Disk usage percentage
    pub usage_percent: f64,
}

/// Network information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    /// Interface name
    pub name: String,
    /// Bytes received
    pub bytes_received: u64,
    /// Bytes transmitted
    pub bytes_transmitted: u64,
    /// Packets received
    pub packets_received: u64,
    /// Packets transmitted
    pub packets_transmitted: u64,
}

/// Process information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    /// Process ID
    pub pid: u32,
    /// Memory usage in MB
    pub memory_usage_mb: u64,
    /// CPU usage percentage
    pub cpu_usage_percent: f32,
    /// Thread count
    pub thread_count: usize,
    /// Process start time
    pub start_time: SystemTime,
}

/// Support bundle generator
pub struct SupportBundle {
    config: SupportBundleConfig,
    system_info: Option<SystemInfo>,
    health_events: Vec<HealthEventData>,
    log_files: Vec<PathBuf>,
    profile_files: Vec<PathBuf>,
    recording_files: Vec<PathBuf>,
    current_size_bytes: u64,
}

impl SupportBundle {
    /// Create new support bundle generator
    pub fn new(config: SupportBundleConfig) -> Self {
        Self {
            config,
            system_info: None,
            health_events: Vec::new(),
            log_files: Vec::new(),
            profile_files: Vec::new(),
            recording_files: Vec::new(),
            current_size_bytes: 0,
        }
    }

    /// Add health events to bundle
    pub fn add_health_events(&mut self, events: &[HealthEventData]) -> DiagnosticResult<()> {
        let mut estimated_size = 0u64;
        for event in events {
            let bytes = serde_json::to_vec(event)
                .map_err(|e| DiagnosticError::Serialization(e.to_string()))?;
            estimated_size = estimated_size.saturating_add(bytes.len() as u64);
        }

        if self.current_size_bytes.saturating_add(estimated_size) > self.max_size_bytes() {
            return Err(DiagnosticError::SizeLimit(format!(
                "Bundle size limit exceeded: {} MB > {} MB",
                (self.current_size_bytes + estimated_size) / 1024 / 1024,
                self.config.max_bundle_size_mb
            )));
        }

        self.health_events.extend_from_slice(events);
        self.current_size_bytes += estimated_size;
        Ok(())
    }

    /// Collect and add system information
    pub fn add_system_info(&mut self) -> DiagnosticResult<()> {
        if !self.config.include_system_info {
            return Ok(());
        }

        let system_info = Self::collect_system_info()?;
        let estimated_size = 50 * 1024;
        self.current_size_bytes += estimated_size;
        self.system_info = Some(system_info);
        Ok(())
    }

    /// Add log files from specified directory
    pub fn add_log_files(&mut self, log_dir: &Path) -> DiagnosticResult<()> {
        if !self.config.include_logs {
            return Ok(());
        }

        let log_files = Self::find_files(log_dir, &["log"])?;

        for log_file in log_files {
            let file_size = std::fs::metadata(&log_file)
                .map_err(|e| DiagnosticError::Io(e.to_string()))?
                .len();

            if self.current_size_bytes + file_size > self.max_size_bytes() {
                continue;
            }

            self.log_files.push(log_file);
            self.current_size_bytes += file_size;
        }

        Ok(())
    }

    /// Add profile files from specified directory
    pub fn add_profile_files(&mut self, profile_dir: &Path) -> DiagnosticResult<()> {
        if !self.config.include_profiles {
            return Ok(());
        }

        let profile_files = Self::find_files(profile_dir, &["json", "profile"])?;

        for profile_file in profile_files {
            let file_size = std::fs::metadata(&profile_file)
                .map_err(|e| DiagnosticError::Io(e.to_string()))?
                .len();

            if self.current_size_bytes + file_size > self.max_size_bytes() {
                continue;
            }

            self.profile_files.push(profile_file);
            self.current_size_bytes += file_size;
        }

        Ok(())
    }

    /// Add recent blackbox recordings
    pub fn add_recent_recordings(&mut self, recording_dir: &Path) -> DiagnosticResult<()> {
        if !self.config.include_recent_recordings {
            return Ok(());
        }

        let mut recordings = Self::find_files(recording_dir, &["wbb"])?;

        recordings.sort_by_key(|path| {
            std::fs::metadata(path)
                .and_then(|m| m.modified())
                .unwrap_or(UNIX_EPOCH)
        });
        recordings.reverse();
        recordings.truncate(5);

        for recording_file in recordings {
            let file_size = std::fs::metadata(&recording_file)
                .map_err(|e| DiagnosticError::Io(e.to_string()))?
                .len();

            if self.current_size_bytes + file_size > self.max_size_bytes() {
                break;
            }

            self.recording_files.push(recording_file);
            self.current_size_bytes += file_size;
        }

        Ok(())
    }

    /// Generate the support bundle ZIP file
    pub fn generate(&self, output_path: &Path) -> DiagnosticResult<()> {
        let file = File::create(output_path).map_err(|e| DiagnosticError::Io(e.to_string()))?;

        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(6));

        self.add_manifest(&mut zip, &options)?;

        if let Some(ref system_info) = self.system_info {
            self.add_system_info_to_zip(&mut zip, &options, system_info)?;
        }

        if !self.health_events.is_empty() {
            self.add_health_events_to_zip(&mut zip, &options)?;
        }

        for log_file in &self.log_files {
            self.add_file_to_zip(&mut zip, &options, log_file, "logs/")?;
        }

        for profile_file in &self.profile_files {
            self.add_file_to_zip(&mut zip, &options, profile_file, "profiles/")?;
        }

        for recording_file in &self.recording_files {
            self.add_file_to_zip(&mut zip, &options, recording_file, "recordings/")?;
        }

        zip.finish()
            .map_err(|e| DiagnosticError::Io(e.to_string()))?;

        Ok(())
    }

    /// Get current bundle size estimate in MB
    pub fn estimated_size_mb(&self) -> f64 {
        self.current_size_bytes as f64 / 1024.0 / 1024.0
    }

    fn max_size_bytes(&self) -> u64 {
        self.config.max_bundle_size_mb * 1024 * 1024
    }

    fn collect_system_info() -> DiagnosticResult<SystemInfo> {
        let mut system = System::new_all();
        system.refresh_all();

        let os_info = OsInfo {
            name: System::name().unwrap_or_else(|| "Unknown".to_string()),
            version: System::os_version().unwrap_or_else(|| "Unknown".to_string()),
            kernel_version: System::kernel_version().unwrap_or_else(|| "Unknown".to_string()),
            hostname: System::host_name().unwrap_or_else(|| "Unknown".to_string()),
            uptime_seconds: System::uptime(),
        };

        let cpu = system.cpus().first();
        let cpu_info = CpuInfo {
            brand: cpu
                .map(|c| c.brand().to_string())
                .unwrap_or_else(|| "Unknown".to_string()),
            frequency_mhz: cpu.map(|c| c.frequency()).unwrap_or(0),
            core_count: system.cpus().len(),
            usage_percent: system.global_cpu_usage(),
        };

        let total_memory = system.total_memory();
        let available_memory = system.available_memory();
        let used_memory = total_memory - available_memory;

        let memory_info = MemoryInfo {
            total_mb: total_memory / 1024 / 1024,
            available_mb: available_memory / 1024 / 1024,
            used_mb: used_memory / 1024 / 1024,
            usage_percent: if total_memory > 0 {
                (used_memory as f64 / total_memory as f64) * 100.0
            } else {
                0.0
            },
        };

        let hardware_info = HardwareInfo {
            cpu_info,
            memory_info,
            disk_info: Vec::new(),
            network_info: Vec::new(),
        };

        let current_pid = std::process::id();
        let process = system.process(sysinfo::Pid::from(current_pid as usize));

        let process_info = ProcessInfo {
            pid: current_pid,
            memory_usage_mb: process.map(|p| p.memory() / 1024 / 1024).unwrap_or(0),
            cpu_usage_percent: process.map(|p| p.cpu_usage()).unwrap_or(0.0),
            thread_count: 1,
            start_time: SystemTime::now(),
        };

        let mut environment = HashMap::new();
        for (key, value) in std::env::vars() {
            if Self::is_safe_env_var(&key) {
                environment.insert(key, value);
            }
        }

        Ok(SystemInfo {
            os_info,
            hardware_info,
            process_info,
            environment,
            collected_at: SystemTime::now(),
        })
    }

    fn is_safe_env_var(key: &str) -> bool {
        let safe_prefixes = [
            "CARGO_",
            "RUST_",
            "PATH",
            "HOME",
            "USER",
            "USERNAME",
            "COMPUTERNAME",
        ];
        let unsafe_keys = ["PASSWORD", "SECRET", "TOKEN", "KEY", "CREDENTIAL"];

        if safe_prefixes.iter().any(|prefix| key.starts_with(prefix)) {
            return true;
        }

        if unsafe_keys
            .iter()
            .any(|unsafe_key| key.to_uppercase().contains(unsafe_key))
        {
            return false;
        }

        matches!(
            key,
            "PATH"
                | "HOME"
                | "USER"
                | "USERNAME"
                | "COMPUTERNAME"
                | "OS"
                | "PROCESSOR_ARCHITECTURE"
        )
    }

    fn find_files(dir: &Path, extensions: &[&str]) -> DiagnosticResult<Vec<PathBuf>> {
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut files = Vec::new();

        for entry in read_dir(dir).map_err(|e| DiagnosticError::Io(e.to_string()))? {
            let entry = entry.map_err(|e| DiagnosticError::Io(e.to_string()))?;
            let path = entry.path();

            if path.is_file()
                && let Some(extension) = path.extension()
                && extensions
                    .iter()
                    .any(|ext| extension.to_string_lossy() == *ext)
            {
                files.push(path);
            }
        }

        Ok(files)
    }

    fn add_manifest(
        &self,
        zip: &mut ZipWriter<File>,
        options: &SimpleFileOptions,
    ) -> DiagnosticResult<()> {
        let manifest = serde_json::json!({
            "bundle_version": "1.0",
            "created_at": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO).as_secs(),
            "config": {
                "include_logs": self.config.include_logs,
                "include_profiles": self.config.include_profiles,
                "include_system_info": self.config.include_system_info,
                "include_recent_recordings": self.config.include_recent_recordings,
                "max_bundle_size_mb": self.config.max_bundle_size_mb,
            },
            "contents": {
                "health_events_count": self.health_events.len(),
                "log_files_count": self.log_files.len(),
                "profile_files_count": self.profile_files.len(),
                "recording_files_count": self.recording_files.len(),
            }
        });

        zip.start_file("manifest.json", *options)
            .map_err(|e| DiagnosticError::Io(e.to_string()))?;

        zip.write_all(manifest.to_string().as_bytes())
            .map_err(|e| DiagnosticError::Io(e.to_string()))?;

        Ok(())
    }

    fn add_system_info_to_zip(
        &self,
        zip: &mut ZipWriter<File>,
        options: &SimpleFileOptions,
        system_info: &SystemInfo,
    ) -> DiagnosticResult<()> {
        let json = serde_json::to_string_pretty(system_info)
            .map_err(|e| DiagnosticError::Serialization(e.to_string()))?;

        zip.start_file("system_info.json", *options)
            .map_err(|e| DiagnosticError::Io(e.to_string()))?;

        zip.write_all(json.as_bytes())
            .map_err(|e| DiagnosticError::Io(e.to_string()))?;

        Ok(())
    }

    fn add_health_events_to_zip(
        &self,
        zip: &mut ZipWriter<File>,
        options: &SimpleFileOptions,
    ) -> DiagnosticResult<()> {
        let json = serde_json::to_string_pretty(&self.health_events)
            .map_err(|e| DiagnosticError::Serialization(e.to_string()))?;

        zip.start_file("health_events.json", *options)
            .map_err(|e| DiagnosticError::Io(e.to_string()))?;

        zip.write_all(json.as_bytes())
            .map_err(|e| DiagnosticError::Io(e.to_string()))?;

        Ok(())
    }

    fn add_file_to_zip(
        &self,
        zip: &mut ZipWriter<File>,
        options: &SimpleFileOptions,
        file_path: &Path,
        prefix: &str,
    ) -> DiagnosticResult<()> {
        let file_name = file_path
            .file_name()
            .ok_or_else(|| DiagnosticError::Validation("Invalid file name".to_string()))?
            .to_string_lossy();

        let zip_path = format!("{}{}", prefix, file_name);

        zip.start_file(&zip_path, *options)
            .map_err(|e| DiagnosticError::Io(e.to_string()))?;

        let content = std::fs::read(file_path).map_err(|e| DiagnosticError::Io(e.to_string()))?;

        zip.write_all(&content)
            .map_err(|e| DiagnosticError::Io(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs::write;
    use tempfile::TempDir;

    #[test]
    fn test_support_bundle_creation() {
        let config = SupportBundleConfig::default();
        let bundle = SupportBundle::new(config);

        assert_eq!(bundle.health_events.len(), 0);
        assert_eq!(bundle.log_files.len(), 0);
        assert!(bundle.system_info.is_none());
    }

    #[test]
    fn test_system_info_collection() {
        let system_info = SupportBundle::collect_system_info();
        assert!(system_info.is_ok());

        let system_info = system_info.unwrap();
        assert!(!system_info.os_info.name.is_empty());
        assert!(system_info.hardware_info.cpu_info.core_count > 0);
        assert!(system_info.hardware_info.memory_info.total_mb > 0);
    }

    #[test]
    fn test_health_events_addition() {
        let config = SupportBundleConfig::default();
        let mut bundle = SupportBundle::new(config);

        let events = vec![
            HealthEventData {
                timestamp_ns: 0,
                device_id: "test-device".to_string(),
                event_type: "DeviceConnected".to_string(),
                context: serde_json::json!({}),
            },
            HealthEventData {
                timestamp_ns: 0,
                device_id: "test-device".to_string(),
                event_type: "DeviceDisconnected".to_string(),
                context: serde_json::json!({}),
            },
        ];

        bundle.add_health_events(&events).unwrap();
        assert_eq!(bundle.health_events.len(), 2);
    }

    #[test]
    fn test_log_file_discovery() {
        let temp_dir = TempDir::new().unwrap();

        write(temp_dir.path().join("app.log"), "Test log content").unwrap();
        write(temp_dir.path().join("error.log"), "Error log content").unwrap();
        write(temp_dir.path().join("not_a_log.txt"), "Not a log").unwrap();

        let log_files = SupportBundle::find_files(temp_dir.path(), &["log"]).unwrap();

        assert_eq!(log_files.len(), 2);
    }

    #[test]
    fn test_bundle_generation() {
        let temp_dir = TempDir::new().unwrap();
        let config = SupportBundleConfig {
            max_bundle_size_mb: 1,
            ..Default::default()
        };

        let mut bundle = SupportBundle::new(config);
        bundle.add_system_info().unwrap();

        let events = vec![HealthEventData {
            timestamp_ns: 0,
            device_id: "test-device".to_string(),
            event_type: "DeviceConnected".to_string(),
            context: serde_json::json!({"test": true}),
        }];
        bundle.add_health_events(&events).unwrap();

        let bundle_path = temp_dir.path().join("support_bundle.zip");
        bundle.generate(&bundle_path).unwrap();

        assert!(bundle_path.exists());
        let metadata = std::fs::metadata(&bundle_path).unwrap();
        assert!(metadata.len() > 0);
    }

    #[test]
    fn test_safe_env_var_filtering() {
        assert!(SupportBundle::is_safe_env_var("CARGO_PKG_NAME"));
        assert!(SupportBundle::is_safe_env_var("RUST_LOG"));
        assert!(SupportBundle::is_safe_env_var("PATH"));
        assert!(SupportBundle::is_safe_env_var("HOME"));

        assert!(!SupportBundle::is_safe_env_var("PASSWORD"));
        assert!(!SupportBundle::is_safe_env_var("SECRET_KEY"));
        assert!(!SupportBundle::is_safe_env_var("API_TOKEN"));
    }

    #[test]
    fn test_size_limit_enforcement() {
        let config = SupportBundleConfig {
            max_bundle_size_mb: 1,
            ..Default::default()
        };

        let mut bundle = SupportBundle::new(config);

        let large_context = serde_json::json!({
            "large_data": "x".repeat(2 * 1024 * 1024)
        });

        let events = vec![HealthEventData {
            timestamp_ns: 0,
            device_id: "test-device".to_string(),
            event_type: "DeviceConnected".to_string(),
            context: large_context,
        }];

        let result = bundle.add_health_events(&events);
        assert!(result.is_err());
    }
}
