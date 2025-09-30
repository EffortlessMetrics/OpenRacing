//! Support Bundle Generation
//!
//! Creates comprehensive diagnostic packages (<25MB for 2-minute capture)
//! with logs, profiles, system info, and recent recordings as specified in DIAG-03.

use super::HealthEvent;
use std::{
    fs::{File, read_dir, read_to_string},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
    collections::HashMap,
};
use serde::{Serialize, Deserialize};
use zip::{ZipWriter, write::FileOptions, CompressionMethod};
use sysinfo::System;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsInfo {
    pub name: String,
    pub version: String,
    pub kernel_version: String,
    pub hostname: String,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    pub cpu_info: CpuInfo,
    pub memory_info: MemoryInfo,
    pub disk_info: Vec<DiskInfo>,
    pub network_info: Vec<NetworkInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    pub brand: String,
    pub frequency_mhz: u64,
    pub core_count: usize,
    pub usage_percent: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInfo {
    pub total_mb: u64,
    pub available_mb: u64,
    pub used_mb: u64,
    pub usage_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskInfo {
    pub name: String,
    pub mount_point: String,
    pub total_gb: u64,
    pub available_gb: u64,
    pub usage_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    pub name: String,
    pub bytes_received: u64,
    pub bytes_transmitted: u64,
    pub packets_received: u64,
    pub packets_transmitted: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub memory_usage_mb: u64,
    pub cpu_usage_percent: f32,
    pub thread_count: usize,
    pub start_time: SystemTime,
}

/// Support bundle generator
pub struct SupportBundle {
    config: SupportBundleConfig,
    system_info: Option<SystemInfo>,
    health_events: Vec<HealthEvent>,
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
    pub fn add_health_events(&mut self, events: &[HealthEvent]) -> Result<(), String> {
        self.health_events.extend_from_slice(events);
        
        // Estimate size (rough approximation)
        let estimated_size = events.len() * 1024; // ~1KB per event
        self.current_size_bytes += estimated_size as u64;
        
        self.check_size_limit()?;
        Ok(())
    }

    /// Collect and add system information
    pub fn add_system_info(&mut self) -> Result<(), String> {
        if !self.config.include_system_info {
            return Ok(());
        }

        let system_info = Self::collect_system_info()?;
        
        // Estimate size
        let estimated_size = 50 * 1024; // ~50KB for system info
        self.current_size_bytes += estimated_size;
        
        self.system_info = Some(system_info);
        self.check_size_limit()?;
        Ok(())
    }

    /// Add log files from specified directory
    pub fn add_log_files(&mut self, log_dir: &Path) -> Result<(), String> {
        if !self.config.include_logs {
            return Ok(());
        }

        let log_files = Self::find_log_files(log_dir)?;
        
        for log_file in log_files {
            let file_size = std::fs::metadata(&log_file)
                .map_err(|e| format!("Failed to get log file size: {}", e))?
                .len();
            
            // Skip very large log files to stay within size limit
            if self.current_size_bytes + file_size > self.max_size_bytes() {
                continue;
            }
            
            self.log_files.push(log_file);
            self.current_size_bytes += file_size;
        }

        self.check_size_limit()?;
        Ok(())
    }

    /// Add profile files from specified directory
    pub fn add_profile_files(&mut self, profile_dir: &Path) -> Result<(), String> {
        if !self.config.include_profiles {
            return Ok(());
        }

        let profile_files = Self::find_profile_files(profile_dir)?;
        
        for profile_file in profile_files {
            let file_size = std::fs::metadata(&profile_file)
                .map_err(|e| format!("Failed to get profile file size: {}", e))?
                .len();
            
            if self.current_size_bytes + file_size > self.max_size_bytes() {
                continue;
            }
            
            self.profile_files.push(profile_file);
            self.current_size_bytes += file_size;
        }

        self.check_size_limit()?;
        Ok(())
    }

    /// Add recent blackbox recordings
    pub fn add_recent_recordings(&mut self, recording_dir: &Path) -> Result<(), String> {
        if !self.config.include_recent_recordings {
            return Ok(());
        }

        let mut recordings = Self::find_recent_recordings(recording_dir, 5)?; // Last 5 recordings
        
        // Sort by modification time (newest first)
        recordings.sort_by_key(|path| {
            std::fs::metadata(path)
                .and_then(|m| m.modified())
                .unwrap_or(UNIX_EPOCH)
        });
        recordings.reverse();

        for recording_file in recordings {
            let file_size = std::fs::metadata(&recording_file)
                .map_err(|e| format!("Failed to get recording file size: {}", e))?
                .len();
            
            // Recordings can be large, so be more selective
            if self.current_size_bytes + file_size > self.max_size_bytes() {
                break; // Stop adding recordings if we'd exceed limit
            }
            
            self.recording_files.push(recording_file);
            self.current_size_bytes += file_size;
        }

        Ok(())
    }

    /// Generate the support bundle ZIP file
    pub fn generate(&self, output_path: &Path) -> Result<(), String> {
        let file = File::create(output_path)
            .map_err(|e| format!("Failed to create bundle file: {}", e))?;
        
        let mut zip = ZipWriter::new(file);
        let options = FileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(6));

        // Add manifest
        self.add_manifest(&mut zip, &options)?;

        // Add system info
        if let Some(ref system_info) = self.system_info {
            self.add_system_info_to_zip(&mut zip, &options, system_info)?;
        }

        // Add health events
        if !self.health_events.is_empty() {
            self.add_health_events_to_zip(&mut zip, &options)?;
        }

        // Add log files
        for log_file in &self.log_files {
            self.add_file_to_zip(&mut zip, &options, log_file, "logs/")?;
        }

        // Add profile files
        for profile_file in &self.profile_files {
            self.add_file_to_zip(&mut zip, &options, profile_file, "profiles/")?;
        }

        // Add recording files
        for recording_file in &self.recording_files {
            self.add_file_to_zip(&mut zip, &options, recording_file, "recordings/")?;
        }

        zip.finish()
            .map_err(|e| format!("Failed to finalize bundle: {}", e))?;

        Ok(())
    }

    /// Collect system information
    fn collect_system_info() -> Result<SystemInfo, String> {
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
            brand: cpu.map(|c| c.brand().to_string()).unwrap_or_else(|| "Unknown".to_string()),
            frequency_mhz: cpu.map(|c| c.frequency()).unwrap_or(0),
            core_count: system.cpus().len(),
            usage_percent: system.global_cpu_info().cpu_usage(),
        };

        let total_memory = system.total_memory();
        let available_memory = system.available_memory();
        let used_memory = total_memory - available_memory;
        
        let memory_info = MemoryInfo {
            total_mb: total_memory / 1024 / 1024,
            available_mb: available_memory / 1024 / 1024,
            used_mb: used_memory / 1024 / 1024,
            usage_percent: (used_memory as f64 / total_memory as f64) * 100.0,
        };

        // Note: Simplified disk and network info for compatibility
        let disk_info: Vec<DiskInfo> = Vec::new(); // Simplified for now
        let network_info: Vec<NetworkInfo> = Vec::new(); // Simplified for now

        let hardware_info = HardwareInfo {
            cpu_info,
            memory_info,
            disk_info,
            network_info,
        };

        // Get current process info
        let current_pid = std::process::id();
        let process = system.process(sysinfo::Pid::from(current_pid as usize));
        
        let process_info = ProcessInfo {
            pid: current_pid,
            memory_usage_mb: process.map(|p| p.memory() / 1024 / 1024).unwrap_or(0),
            cpu_usage_percent: process.map(|p| p.cpu_usage()).unwrap_or(0.0),
            thread_count: 1, // Simplified
            start_time: SystemTime::now(), // Simplified
        };

        // Collect filtered environment variables
        let mut environment = HashMap::new();
        for (key, value) in std::env::vars() {
            // Only include safe, non-sensitive environment variables
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

    /// Check if environment variable is safe to include
    fn is_safe_env_var(key: &str) -> bool {
        let safe_prefixes = ["CARGO_", "RUST_", "PATH", "HOME", "USER", "USERNAME", "COMPUTERNAME"];
        let unsafe_keys = ["PASSWORD", "SECRET", "TOKEN", "KEY", "CREDENTIAL"];
        
        // Include if it starts with a safe prefix
        if safe_prefixes.iter().any(|prefix| key.starts_with(prefix)) {
            return true;
        }
        
        // Exclude if it contains unsafe keywords
        if unsafe_keys.iter().any(|unsafe_key| key.to_uppercase().contains(unsafe_key)) {
            return false;
        }
        
        // Include common system variables
        matches!(key, "PATH" | "HOME" | "USER" | "USERNAME" | "COMPUTERNAME" | "OS" | "PROCESSOR_ARCHITECTURE")
    }

    /// Find log files in directory
    fn find_log_files(log_dir: &Path) -> Result<Vec<PathBuf>, String> {
        if !log_dir.exists() {
            return Ok(Vec::new());
        }

        let mut log_files = Vec::new();
        
        for entry in read_dir(log_dir).map_err(|e| format!("Failed to read log directory: {}", e))? {
            let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
            let path = entry.path();
            
            if path.is_file() {
                if let Some(extension) = path.extension() {
                    if extension == "log" || extension == "txt" {
                        log_files.push(path);
                    }
                }
            }
        }

        Ok(log_files)
    }

    /// Find profile files in directory
    fn find_profile_files(profile_dir: &Path) -> Result<Vec<PathBuf>, String> {
        if !profile_dir.exists() {
            return Ok(Vec::new());
        }

        let mut profile_files = Vec::new();
        
        for entry in read_dir(profile_dir).map_err(|e| format!("Failed to read profile directory: {}", e))? {
            let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
            let path = entry.path();
            
            if path.is_file() {
                if let Some(extension) = path.extension() {
                    if extension == "json" || extension == "profile" {
                        profile_files.push(path);
                    }
                }
            }
        }

        Ok(profile_files)
    }

    /// Find recent recording files
    fn find_recent_recordings(recording_dir: &Path, max_count: usize) -> Result<Vec<PathBuf>, String> {
        if !recording_dir.exists() {
            return Ok(Vec::new());
        }

        let mut recordings = Vec::new();
        
        for entry in read_dir(recording_dir).map_err(|e| format!("Failed to read recording directory: {}", e))? {
            let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
            let path = entry.path();
            
            if path.is_file() {
                if let Some(extension) = path.extension() {
                    if extension == "wbb" {
                        recordings.push(path);
                    }
                }
            }
        }

        // Sort by modification time and take most recent
        recordings.sort_by_key(|path| {
            std::fs::metadata(path)
                .and_then(|m| m.modified())
                .unwrap_or(UNIX_EPOCH)
        });
        recordings.reverse();
        recordings.truncate(max_count);

        Ok(recordings)
    }

    /// Add manifest to ZIP
    fn add_manifest(&self, zip: &mut ZipWriter<File>, options: &FileOptions) -> Result<(), String> {
        let manifest = serde_json::json!({
            "bundle_version": "1.0",
            "created_at": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
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
            .map_err(|e| format!("Failed to start manifest file: {}", e))?;
        
        zip.write_all(manifest.to_string().as_bytes())
            .map_err(|e| format!("Failed to write manifest: {}", e))?;

        Ok(())
    }

    /// Add system info to ZIP
    fn add_system_info_to_zip(&self, zip: &mut ZipWriter<File>, options: &FileOptions, system_info: &SystemInfo) -> Result<(), String> {
        let json = serde_json::to_string_pretty(system_info)
            .map_err(|e| format!("Failed to serialize system info: {}", e))?;

        zip.start_file("system_info.json", *options)
            .map_err(|e| format!("Failed to start system info file: {}", e))?;
        
        zip.write_all(json.as_bytes())
            .map_err(|e| format!("Failed to write system info: {}", e))?;

        Ok(())
    }

    /// Add health events to ZIP
    fn add_health_events_to_zip(&self, zip: &mut ZipWriter<File>, options: &FileOptions) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&self.health_events)
            .map_err(|e| format!("Failed to serialize health events: {}", e))?;

        zip.start_file("health_events.json", *options)
            .map_err(|e| format!("Failed to start health events file: {}", e))?;
        
        zip.write_all(json.as_bytes())
            .map_err(|e| format!("Failed to write health events: {}", e))?;

        Ok(())
    }

    /// Add file to ZIP with specified prefix
    fn add_file_to_zip(&self, zip: &mut ZipWriter<File>, options: &FileOptions, file_path: &Path, prefix: &str) -> Result<(), String> {
        let file_name = file_path.file_name()
            .ok_or("Invalid file name")?
            .to_string_lossy();
        
        let zip_path = format!("{}{}", prefix, file_name);
        
        zip.start_file(&zip_path, *options)
            .map_err(|e| format!("Failed to start file in ZIP: {}", e))?;

        let content = read_to_string(file_path)
            .map_err(|e| format!("Failed to read file {}: {}", file_path.display(), e))?;
        
        zip.write_all(content.as_bytes())
            .map_err(|e| format!("Failed to write file to ZIP: {}", e))?;

        Ok(())
    }

    /// Check if current size exceeds limit
    fn check_size_limit(&self) -> Result<(), String> {
        if self.current_size_bytes > self.max_size_bytes() {
            return Err(format!("Bundle size limit exceeded: {} MB > {} MB", 
                              self.current_size_bytes / 1024 / 1024,
                              self.config.max_bundle_size_mb));
        }
        Ok(())
    }

    /// Get maximum size in bytes
    fn max_size_bytes(&self) -> u64 {
        self.config.max_bundle_size_mb * 1024 * 1024
    }

    /// Get current bundle size estimate
    pub fn estimated_size_mb(&self) -> f64 {
        self.current_size_bytes as f64 / 1024.0 / 1024.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs::write;
    

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
        
        let device_id = DeviceId::new("test-device".to_string()).unwrap();
        let events = vec![
            HealthEvent {
                timestamp: SystemTime::now(),
                device_id: device_id.clone(),
                event_type: super::super::HealthEventType::DeviceConnected,
                context: serde_json::json!({}),
            },
            HealthEvent {
                timestamp: SystemTime::now(),
                device_id,
                event_type: super::super::HealthEventType::DeviceDisconnected,
                context: serde_json::json!({}),
            },
        ];
        
        let result = bundle.add_health_events(&events);
        assert!(result.is_ok());
        assert_eq!(bundle.health_events.len(), 2);
    }

    #[test]
    fn test_log_file_discovery() {
        let temp_dir = TempDir::new().unwrap();
        
        // Create some test log files
        write(temp_dir.path().join("app.log"), "Test log content").unwrap();
        write(temp_dir.path().join("error.log"), "Error log content").unwrap();
        write(temp_dir.path().join("not_a_log.txt"), "Not a log").unwrap();
        write(temp_dir.path().join("config.json"), "Config file").unwrap();
        
        let log_files = SupportBundle::find_log_files(temp_dir.path()).unwrap();
        
        // Should find .log files but not others
        assert_eq!(log_files.len(), 2);
        assert!(log_files.iter().any(|p| p.file_name().unwrap() == "app.log"));
        assert!(log_files.iter().any(|p| p.file_name().unwrap() == "error.log"));
    }

    #[test]
    fn test_bundle_generation() {
        let temp_dir = TempDir::new().unwrap();
        let config = SupportBundleConfig {
            max_bundle_size_mb: 1, // Small limit for test
            ..Default::default()
        };
        
        let mut bundle = SupportBundle::new(config);
        
        // Add some test data
        bundle.add_system_info().unwrap();
        
        let device_id = DeviceId::new("test-device".to_string()).unwrap();
        let events = vec![HealthEvent {
            timestamp: SystemTime::now(),
            device_id,
            event_type: super::super::HealthEventType::DeviceConnected,
            context: serde_json::json!({"test": true}),
        }];
        bundle.add_health_events(&events).unwrap();
        
        // Generate bundle
        let bundle_path = temp_dir.path().join("support_bundle.zip");
        let result = bundle.generate(&bundle_path);
        assert!(result.is_ok());
        
        // Verify file was created
        assert!(bundle_path.exists());
        let metadata = std::fs::metadata(&bundle_path).unwrap();
        assert!(metadata.len() > 0);
    }

    #[test]
    fn test_size_limit_enforcement() {
        let config = SupportBundleConfig {
            max_bundle_size_mb: 1, // Very small limit
            ..Default::default()
        };
        
        let mut bundle = SupportBundle::new(config);
        
        // Try to add too much data
        let device_id = DeviceId::new("test-device".to_string()).unwrap();
        let large_context = serde_json::json!({
            "large_data": "x".repeat(2 * 1024 * 1024) // 2MB of data
        });
        
        let events = vec![HealthEvent {
            timestamp: SystemTime::now(),
            device_id,
            event_type: super::super::HealthEventType::DeviceConnected,
            context: large_context,
        }];
        
        // This should exceed the size limit
        let result = bundle.add_health_events(&events);
        assert!(result.is_err());
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
        assert!(!SupportBundle::is_safe_env_var("DATABASE_CREDENTIAL"));
    }

    #[test]
    fn test_profile_file_discovery() {
        let temp_dir = TempDir::new().unwrap();
        
        // Create test profile files
        write(temp_dir.path().join("global.profile.json"), "{}").unwrap();
        write(temp_dir.path().join("game.json"), "{}").unwrap();
        write(temp_dir.path().join("settings.profile"), "{}").unwrap();
        write(temp_dir.path().join("readme.txt"), "Not a profile").unwrap();
        
        let profile_files = SupportBundle::find_profile_files(temp_dir.path()).unwrap();
        
        // Should find .json and .profile files
        assert_eq!(profile_files.len(), 3);
    }
}