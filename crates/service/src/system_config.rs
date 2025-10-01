//! System-level configuration management and validation
//! 
//! Provides comprehensive configuration for the entire racing wheel system
//! with validation, migration, and feature flag support.

use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use tracing::{info, warn, debug};

/// Complete system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    /// Configuration schema version
    pub schema_version: String,
    /// Engine configuration
    pub engine: EngineConfig,
    /// Service configuration
    pub service: ServiceConfig,
    /// IPC configuration
    pub ipc: IpcConfig,
    /// Game integration configuration
    pub games: GameConfig,
    /// Safety configuration
    pub safety: SafetyConfig,
    /// Plugin configuration
    pub plugins: PluginConfig,
    /// Observability configuration
    pub observability: ObservabilityConfig,
    /// Development configuration
    pub development: DevelopmentConfig,
}

/// Engine-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    /// Target tick rate in Hz
    pub tick_rate_hz: u32,
    /// Maximum jitter tolerance in microseconds
    pub max_jitter_us: u32,
    /// Force FFB mode (for development)
    pub force_ffb_mode: Option<String>,
    /// Disable real-time scheduling
    pub disable_realtime: bool,
    /// RT thread CPU affinity
    pub rt_cpu_affinity: Option<Vec<usize>>,
    /// Memory lock all pages
    pub memory_lock_all: bool,
    /// Processing budget per tick in microseconds
    pub processing_budget_us: u32,
}

/// Service daemon configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    /// Service name
    pub service_name: String,
    /// Service display name
    pub service_display_name: String,
    /// Service description
    pub service_description: String,
    /// Health check interval in seconds
    pub health_check_interval: u64,
    /// Maximum restart attempts
    pub max_restart_attempts: u32,
    /// Restart delay in seconds
    pub restart_delay: u64,
    /// Enable automatic restart on failure
    pub auto_restart: bool,
    /// Graceful shutdown timeout in seconds
    pub shutdown_timeout: u64,
}

/// IPC configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcConfig {
    /// Transport type
    pub transport: TransportType,
    /// Bind address (for TCP transport)
    pub bind_address: Option<String>,
    /// Maximum concurrent connections
    pub max_connections: u32,
    /// Connection timeout in seconds
    pub connection_timeout: u64,
    /// Enable ACL restrictions
    pub enable_acl: bool,
    /// Message size limit in bytes
    pub max_message_size: usize,
}

/// Transport type for IPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransportType {
    /// Named pipes (Windows) / Unix domain sockets (Linux)
    Native,
    /// TCP (for development/testing)
    Tcp,
}

/// Game integration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameConfig {
    /// Auto-configuration enabled
    pub auto_configure: bool,
    /// Auto profile switching enabled
    pub auto_profile_switch: bool,
    /// Profile switch timeout in milliseconds
    pub profile_switch_timeout_ms: u32,
    /// Telemetry timeout in seconds
    pub telemetry_timeout_s: u32,
    /// Supported games configuration
    pub supported_games: HashMap<String, GameSupportConfig>,
}

/// Per-game support configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSupportConfig {
    /// Game executable names
    pub executables: Vec<String>,
    /// Telemetry method
    pub telemetry_method: String,
    /// Configuration file paths
    pub config_paths: Vec<String>,
    /// Auto-configuration enabled for this game
    pub auto_configure: bool,
}

/// Safety system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyConfig {
    /// Default safe torque limit in Nm
    pub default_safe_torque_nm: f32,
    /// Maximum allowed torque in Nm
    pub max_torque_nm: f32,
    /// Fault response timeout in milliseconds
    pub fault_response_timeout_ms: u32,
    /// Hands-off detection timeout in seconds
    pub hands_off_timeout_s: u32,
    /// Temperature warning threshold in Celsius
    pub temp_warning_c: u8,
    /// Temperature fault threshold in Celsius
    pub temp_fault_c: u8,
    /// Enable physical interlock requirement
    pub require_physical_interlock: bool,
}

/// Plugin system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    /// Enable plugin system
    pub enabled: bool,
    /// Plugin directory paths
    pub plugin_paths: Vec<String>,
    /// Auto-load plugins
    pub auto_load: bool,
    /// Plugin timeout in milliseconds
    pub timeout_ms: u32,
    /// Maximum plugin memory usage in MB
    pub max_memory_mb: u32,
    /// Enable native plugins (requires explicit approval)
    pub enable_native: bool,
}

/// Observability configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    /// Enable metrics collection
    pub enable_metrics: bool,
    /// Metrics export interval in seconds
    pub metrics_interval_s: u32,
    /// Enable tracing
    pub enable_tracing: bool,
    /// Tracing sample rate (0.0 to 1.0)
    pub tracing_sample_rate: f32,
    /// Enable blackbox recording
    pub enable_blackbox: bool,
    /// Blackbox retention in hours
    pub blackbox_retention_hours: u32,
    /// Health event stream rate in Hz
    pub health_stream_hz: u32,
}

/// Development configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevelopmentConfig {
    /// Enable development features
    pub enable_dev_features: bool,
    /// Enable debug logging
    pub enable_debug_logging: bool,
    /// Enable virtual devices
    pub enable_virtual_devices: bool,
    /// Disable safety interlocks (DANGEROUS)
    pub disable_safety_interlocks: bool,
    /// Enable plugin development mode
    pub enable_plugin_dev_mode: bool,
    /// Mock telemetry data
    pub mock_telemetry: bool,
}

/// Feature flags for runtime behavior
#[derive(Debug, Clone)]
pub struct FeatureFlags {
    /// Disable real-time scheduling
    pub disable_realtime: bool,
    /// Force specific FFB mode
    pub force_ffb_mode: Option<String>,
    /// Enable development features
    pub enable_dev_features: bool,
    /// Enable debug logging
    pub enable_debug_logging: bool,
    /// Enable virtual devices
    pub enable_virtual_devices: bool,
    /// Disable safety interlocks
    pub disable_safety_interlocks: bool,
    /// Enable plugin development mode
    pub enable_plugin_dev_mode: bool,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            schema_version: "wheel.config/1".to_string(),
            engine: EngineConfig::default(),
            service: ServiceConfig::default(),
            ipc: IpcConfig::default(),
            games: GameConfig::default(),
            safety: SafetyConfig::default(),
            plugins: PluginConfig::default(),
            observability: ObservabilityConfig::default(),
            development: DevelopmentConfig::default(),
        }
    }
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            tick_rate_hz: 1000,
            max_jitter_us: 250,
            force_ffb_mode: None,
            disable_realtime: false,
            rt_cpu_affinity: None,
            memory_lock_all: true,
            processing_budget_us: 200,
        }
    }
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            service_name: "wheeld".to_string(),
            service_display_name: "Racing Wheel Service".to_string(),
            service_description: "Racing wheel hardware management and force feedback service".to_string(),
            health_check_interval: 30,
            max_restart_attempts: 3,
            restart_delay: 5,
            auto_restart: true,
            shutdown_timeout: 30,
        }
    }
}

impl Default for IpcConfig {
    fn default() -> Self {
        Self {
            transport: TransportType::Native,
            bind_address: None,
            max_connections: 10,
            connection_timeout: 30,
            enable_acl: true,
            max_message_size: 1024 * 1024, // 1MB
        }
    }
}

impl Default for GameConfig {
    fn default() -> Self {
        let mut supported_games = HashMap::new();
        
        // iRacing configuration
        supported_games.insert("iracing".to_string(), GameSupportConfig {
            executables: vec!["iRacingSim64DX11.exe".to_string(), "iRacingSim64.exe".to_string()],
            telemetry_method: "shared_memory".to_string(),
            config_paths: vec![
                "Documents/iRacing/app.ini".to_string(),
            ],
            auto_configure: true,
        });
        
        // Assetto Corsa Competizione configuration
        supported_games.insert("acc".to_string(), GameSupportConfig {
            executables: vec!["AC2-Win64-Shipping.exe".to_string()],
            telemetry_method: "udp_broadcast".to_string(),
            config_paths: vec![
                "Documents/Assetto Corsa Competizione/Config/broadcasting.json".to_string(),
            ],
            auto_configure: true,
        });
        
        Self {
            auto_configure: true,
            auto_profile_switch: true,
            profile_switch_timeout_ms: 500,
            telemetry_timeout_s: 5,
            supported_games,
        }
    }
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            default_safe_torque_nm: 5.0,
            max_torque_nm: 25.0,
            fault_response_timeout_ms: 50,
            hands_off_timeout_s: 5,
            temp_warning_c: 70,
            temp_fault_c: 80,
            require_physical_interlock: true,
        }
    }
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            plugin_paths: vec![
                "plugins/safe".to_string(),
                "plugins/native".to_string(),
            ],
            auto_load: true,
            timeout_ms: 100,
            max_memory_mb: 64,
            enable_native: false,
        }
    }
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            enable_metrics: true,
            metrics_interval_s: 60,
            enable_tracing: true,
            tracing_sample_rate: 0.1,
            enable_blackbox: true,
            blackbox_retention_hours: 24,
            health_stream_hz: 10,
        }
    }
}

impl Default for DevelopmentConfig {
    fn default() -> Self {
        Self {
            enable_dev_features: false,
            enable_debug_logging: false,
            enable_virtual_devices: false,
            disable_safety_interlocks: false,
            enable_plugin_dev_mode: false,
            mock_telemetry: false,
        }
    }
}

impl SystemConfig {
    /// Load configuration from default location
    pub async fn load() -> Result<Self> {
        let config_path = Self::default_config_path()?;
        Self::load_from_path(&config_path).await
    }
    
    /// Load configuration from specific path
    pub async fn load_from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        
        if !path.exists() {
            info!("Config file not found at {:?}, creating default", path);
            let config = Self::default();
            config.save_to_path(path).await?;
            return Ok(config);
        }
        
        let content = tokio::fs::read_to_string(path).await
            .with_context(|| format!("Failed to read config file: {:?}", path))?;
        
        let config: SystemConfig = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {:?}", path))?;
        
        debug!("Loaded config from {:?}", path);
        Ok(config)
    }
    
    /// Save configuration to default location
    pub async fn save(&self) -> Result<()> {
        let config_path = Self::default_config_path()?;
        self.save_to_path(&config_path).await
    }
    
    /// Save configuration to specific path
    pub async fn save_to_path<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await
                .context("Failed to create config directory")?;
        }
        
        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize config")?;
        
        tokio::fs::write(path, content).await
            .with_context(|| format!("Failed to write config file: {:?}", path))?;
        
        debug!("Saved config to {:?}", path);
        Ok(())
    }
    
    /// Get default configuration file path
    pub fn default_config_path() -> Result<PathBuf> {
        let config_dir = if cfg!(windows) {
            std::env::var("LOCALAPPDATA")
                .context("LOCALAPPDATA environment variable not set")?
        } else {
            format!("{}/.config", std::env::var("HOME")
                .context("HOME environment variable not set")?)
        };
        
        Ok(PathBuf::from(config_dir)
            .join("wheel")
            .join("system.json"))
    }
    
    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        // Validate schema version
        if !self.schema_version.starts_with("wheel.config/") {
            anyhow::bail!("Invalid schema version: {}", self.schema_version);
        }
        
        // Validate engine configuration
        if self.engine.tick_rate_hz == 0 || self.engine.tick_rate_hz > 10000 {
            anyhow::bail!("Invalid tick rate: {} Hz", self.engine.tick_rate_hz);
        }
        
        if self.engine.max_jitter_us > 1000 {
            anyhow::bail!("Invalid max jitter: {} μs", self.engine.max_jitter_us);
        }
        
        // Validate safety configuration
        if self.safety.default_safe_torque_nm <= 0.0 || self.safety.default_safe_torque_nm > self.safety.max_torque_nm {
            anyhow::bail!("Invalid safe torque: {} Nm", self.safety.default_safe_torque_nm);
        }
        
        if self.safety.max_torque_nm <= 0.0 || self.safety.max_torque_nm > 50.0 {
            anyhow::bail!("Invalid max torque: {} Nm", self.safety.max_torque_nm);
        }
        
        if self.safety.fault_response_timeout_ms == 0 || self.safety.fault_response_timeout_ms > 1000 {
            anyhow::bail!("Invalid fault response timeout: {} ms", self.safety.fault_response_timeout_ms);
        }
        
        // Validate IPC configuration
        if self.ipc.max_connections == 0 || self.ipc.max_connections > 1000 {
            anyhow::bail!("Invalid max connections: {}", self.ipc.max_connections);
        }
        
        // Validate observability configuration
        if self.observability.tracing_sample_rate < 0.0 || self.observability.tracing_sample_rate > 1.0 {
            anyhow::bail!("Invalid tracing sample rate: {}", self.observability.tracing_sample_rate);
        }
        
        // Warn about dangerous development settings
        if self.development.disable_safety_interlocks {
            warn!("DANGER: Safety interlocks are disabled!");
        }
        
        Ok(())
    }
    
    /// Migrate configuration to current schema version
    pub fn migrate(&mut self) -> Result<bool> {
        let current_version = "wheel.config/1";
        
        if self.schema_version == current_version {
            return Ok(false); // No migration needed
        }
        
        info!("Migrating config from {} to {}", self.schema_version, current_version);
        
        // Add migration logic here as schema evolves
        match self.schema_version.as_str() {
            "wheel.config/0" => {
                // Example migration from v0 to v1
                // Add new fields with defaults, remove deprecated fields
                self.schema_version = current_version.to_string();
            }
            _ => {
                anyhow::bail!("Unsupported config schema version: {}", self.schema_version);
            }
        }
        
        Ok(true) // Migration performed
    }
}

impl ServiceConfig {
    /// Create ServiceConfig from SystemConfig
    pub fn from_system_config(system_config: &SystemConfig) -> crate::ServiceConfig {
        crate::ServiceConfig {
            service_name: system_config.service.service_name.clone(),
            service_display_name: system_config.service.service_display_name.clone(),
            service_description: system_config.service.service_description.clone(),
            ipc: crate::IpcConfig {
                transport: match system_config.ipc.transport {
                    TransportType::Native => crate::TransportType::default(),
                    TransportType::Tcp => {
                        // For TCP, we'll use the default platform transport since TCP isn't available
                        // in the simplified IPC implementation
                        crate::TransportType::default()
                    },
                },
                bind_address: system_config.ipc.bind_address.clone(),
                max_connections: system_config.ipc.max_connections,
                connection_timeout: std::time::Duration::from_secs(system_config.ipc.connection_timeout),
                enable_acl: system_config.ipc.enable_acl,
            },
            health_check_interval: system_config.service.health_check_interval,
            max_restart_attempts: system_config.service.max_restart_attempts,
            restart_delay: system_config.service.restart_delay,
            auto_restart: system_config.service.auto_restart,
        }
    }
}