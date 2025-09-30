//! Port traits for clean architecture boundaries
//!
//! This module defines the port interfaces that separate the domain layer
//! from infrastructure concerns. These traits define contracts for external
//! dependencies without coupling to specific implementations.

use crate::{RTResult, DeviceEvent, TelemetryData, DeviceInfo};
use racing_wheel_schemas::prelude::*;
use tokio::sync::mpsc;
use async_trait::async_trait;

/// HID device abstraction for real-time operations
/// 
/// This trait defines the contract for communicating with racing wheel hardware
/// at the lowest level. Implementations must be RT-safe for write operations.
pub trait HidDevice: Send + Sync {
    /// Write force feedback report (RT-safe, non-blocking)
    /// 
    /// This method MUST be real-time safe:
    /// - No heap allocations
    /// - No blocking system calls
    /// - No locks that can block
    /// - Execution time must be bounded and predictable
    fn write_ffb_report(&mut self, torque_nm: f32, seq: u16) -> RTResult;
    
    /// Read device telemetry (non-RT, async)
    /// 
    /// This method is called from non-RT threads and can perform
    /// blocking I/O operations.
    fn read_telemetry(&mut self) -> Option<TelemetryData>;
    
    /// Get device capabilities (cached, RT-safe)
    fn capabilities(&self) -> &racing_wheel_schemas::DeviceCapabilities;
    
    /// Get device info (cached, RT-safe)
    fn device_info(&self) -> &DeviceInfo;
    
    /// Check if device is connected (RT-safe)
    fn is_connected(&self) -> bool;
    
    /// Get device health status (non-RT)
    fn health_status(&self) -> DeviceHealthStatus;
}

/// Device health status information
#[derive(Debug, Clone)]
pub struct DeviceHealthStatus {
    pub temperature_c: u8,
    pub fault_flags: u8,
    pub hands_on: bool,
    pub last_communication: std::time::Instant,
    pub communication_errors: u32,
}

/// HID port abstraction for device enumeration and management
/// 
/// This trait defines the contract for discovering and opening HID devices.
/// It abstracts platform-specific device enumeration and connection logic.
#[async_trait]
pub trait HidPort: Send + Sync {
    /// List all available racing wheel devices
    /// 
    /// Returns a list of device information for all compatible racing wheels
    /// currently connected to the system.
    async fn list_devices(&self) -> Result<Vec<DeviceInfo>, Box<dyn std::error::Error>>;
    
    /// Open a device by ID for communication
    /// 
    /// Returns a HidDevice instance that can be used for real-time communication
    /// with the specified device.
    async fn open_device(&self, id: &DeviceId) -> Result<Box<dyn HidDevice>, Box<dyn std::error::Error>>;
    
    /// Monitor for device connect/disconnect events
    /// 
    /// Returns a receiver that will receive events when devices are connected
    /// or disconnected from the system.
    async fn monitor_devices(&self) -> Result<mpsc::Receiver<DeviceEvent>, Box<dyn std::error::Error>>;
    
    /// Refresh device list (force re-enumeration)
    async fn refresh_devices(&self) -> Result<(), Box<dyn std::error::Error>>;
}

/// Telemetry data from racing games
#[derive(Debug, Clone)]
pub struct NormalizedTelemetry {
    /// Force feedback scalar from game (-1.0 to 1.0)
    pub ffb_scalar: f32,
    
    /// Engine RPM
    pub rpm: f32,
    
    /// Vehicle speed in m/s
    pub speed_ms: f32,
    
    /// Tire slip ratio (0.0 = no slip, 1.0 = full slip)
    pub slip_ratio: f32,
    
    /// Current gear (-1 = reverse, 0 = neutral, 1+ = forward gears)
    pub gear: i8,
    
    /// Racing flags and status
    pub flags: TelemetryFlags,
    
    /// Car identifier (if available)
    pub car_id: Option<String>,
    
    /// Track identifier (if available)
    pub track_id: Option<String>,
    
    /// Timestamp when telemetry was captured
    pub timestamp: std::time::Instant,
}

/// Racing flags and status information
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TelemetryFlags {
    pub yellow_flag: bool,
    pub red_flag: bool,
    pub blue_flag: bool,
    pub checkered_flag: bool,
    pub pit_limiter: bool,
    pub drs_enabled: bool,
    pub ers_available: bool,
    pub in_pit: bool,
}

/// Telemetry port abstraction for game integration
/// 
/// This trait defines the contract for receiving telemetry data from racing games.
/// Implementations handle game-specific protocols and normalize the data.
#[async_trait]
pub trait TelemetryPort: Send + Sync {
    /// Get the game identifier this port handles
    fn game_id(&self) -> &str;
    
    /// Configure the game for telemetry output
    /// 
    /// This method should modify game configuration files to enable
    /// telemetry output in the format expected by this port.
    async fn configure_game(&self, install_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>>;
    
    /// Start monitoring for telemetry data
    /// 
    /// Returns a receiver that will receive normalized telemetry data
    /// from the game at the game's update rate.
    async fn start_monitoring(&self) -> Result<mpsc::Receiver<NormalizedTelemetry>, Box<dyn std::error::Error>>;
    
    /// Stop monitoring telemetry data
    async fn stop_monitoring(&self) -> Result<(), Box<dyn std::error::Error>>;
    
    /// Check if telemetry is currently active
    fn is_monitoring(&self) -> bool;
    
    /// Get telemetry statistics
    fn get_statistics(&self) -> TelemetryStatistics;
    
    /// Validate game installation and telemetry configuration
    async fn validate_configuration(&self, install_path: &std::path::Path) -> Result<ConfigurationStatus, Box<dyn std::error::Error>>;
}

/// Telemetry statistics for monitoring health
#[derive(Debug, Clone, Default)]
pub struct TelemetryStatistics {
    pub packets_received: u64,
    pub packets_dropped: u64,
    pub last_packet_time: Option<std::time::Instant>,
    pub average_rate_hz: f32,
    pub connection_errors: u32,
}

/// Configuration validation status
#[derive(Debug, Clone)]
pub struct ConfigurationStatus {
    pub is_valid: bool,
    pub game_version: Option<String>,
    pub telemetry_enabled: bool,
    pub expected_config_changes: Vec<ConfigChange>,
    pub issues: Vec<String>,
}

/// Configuration change description
#[derive(Debug, Clone)]
pub struct ConfigChange {
    pub file_path: std::path::PathBuf,
    pub section: Option<String>,
    pub key: String,
    pub expected_value: String,
    pub current_value: Option<String>,
}

/// Profile repository abstraction for persistence
/// 
/// This trait defines the contract for storing and retrieving profile configurations.
/// It abstracts the underlying storage mechanism (filesystem, database, etc.).
#[async_trait]
pub trait ProfileRepo: Send + Sync {
    /// Load a profile by ID
    async fn load_profile(&self, id: &ProfileId) -> Result<Profile, ProfileRepoError>;
    
    /// Save a profile
    async fn save_profile(&self, profile: &Profile) -> Result<(), ProfileRepoError>;
    
    /// Delete a profile by ID
    async fn delete_profile(&self, id: &ProfileId) -> Result<(), ProfileRepoError>;
    
    /// List all available profiles
    async fn list_profiles(&self) -> Result<Vec<ProfileId>, ProfileRepoError>;
    
    /// Find profiles matching a scope
    async fn find_profiles_for_scope(&self, scope: &racing_wheel_schemas::ProfileScope) -> Result<Vec<Profile>, ProfileRepoError>;
    
    /// Load the global default profile
    async fn load_global_profile(&self) -> Result<Profile, ProfileRepoError>;
    
    /// Save the global default profile
    async fn save_global_profile(&self, profile: &Profile) -> Result<(), ProfileRepoError>;
    
    /// Check if a profile exists
    async fn profile_exists(&self, id: &ProfileId) -> Result<bool, ProfileRepoError>;
    
    /// Get profile metadata without loading full profile
    async fn get_profile_metadata(&self, id: &ProfileId) -> Result<racing_wheel_schemas::ProfileMetadata, ProfileRepoError>;
    
    /// Backup profiles to a specified location
    async fn backup_profiles(&self, backup_path: &std::path::Path) -> Result<(), ProfileRepoError>;
    
    /// Restore profiles from a backup
    async fn restore_profiles(&self, backup_path: &std::path::Path) -> Result<(), ProfileRepoError>;
    
    /// Validate profile repository integrity
    async fn validate_repository(&self) -> Result<RepositoryStatus, ProfileRepoError>;
}

/// Profile repository error types
#[derive(Debug, thiserror::Error)]
pub enum ProfileRepoError {
    #[error("Profile not found: {0}")]
    ProfileNotFound(ProfileId),
    
    #[error("Profile validation failed: {0}")]
    ValidationError(#[from] DomainError),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Repository corruption detected: {0}")]
    CorruptionError(String),
    
    #[error("Permission denied: {0}")]
    PermissionError(String),
    
    #[error("Repository locked by another process")]
    LockError,
    
    #[error("Backup/restore error: {0}")]
    BackupError(String),
}

/// Repository health and status information
#[derive(Debug, Clone)]
pub struct RepositoryStatus {
    pub is_healthy: bool,
    pub total_profiles: usize,
    pub corrupted_profiles: Vec<ProfileId>,
    pub missing_files: Vec<std::path::PathBuf>,
    pub permission_issues: Vec<std::path::PathBuf>,
    pub last_backup: Option<std::time::SystemTime>,
    pub disk_usage_bytes: u64,
}

/// Context information for profile resolution
#[derive(Debug, Clone)]
pub struct ProfileContext {
    pub game: Option<String>,
    pub car: Option<String>,
    pub track: Option<String>,
    pub device_id: DeviceId,
    pub session_type: Option<String>,
}

impl ProfileContext {
    /// Create a new profile context
    pub fn new(device_id: DeviceId) -> Self {
        Self {
            game: None,
            car: None,
            track: None,
            device_id,
            session_type: None,
        }
    }
    
    /// Set game context
    pub fn with_game(mut self, game: String) -> Self {
        self.game = Some(game);
        self
    }
    
    /// Set car context
    pub fn with_car(mut self, car: String) -> Self {
        self.car = Some(car);
        self
    }
    
    /// Set track context
    pub fn with_track(mut self, track: String) -> Self {
        self.track = Some(track);
        self
    }
    
    /// Set session type context
    pub fn with_session_type(mut self, session_type: String) -> Self {
        self.session_type = Some(session_type);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use racing_wheel_schemas::DeviceId;

    #[test]
    fn test_profile_context_creation() {
        let device_id = DeviceId::new("test-device".to_string()).unwrap();
        let context = ProfileContext::new(device_id.clone());
        
        assert_eq!(context.device_id, device_id);
        assert!(context.game.is_none());
        assert!(context.car.is_none());
        assert!(context.track.is_none());
    }

    #[test]
    fn test_profile_context_builder() {
        let device_id = DeviceId::new("test-device".to_string()).unwrap();
        let context = ProfileContext::new(device_id.clone())
            .with_game("iracing".to_string())
            .with_car("gt3".to_string())
            .with_track("spa".to_string())
            .with_session_type("race".to_string());
        
        assert_eq!(context.device_id, device_id);
        assert_eq!(context.game, Some("iracing".to_string()));
        assert_eq!(context.car, Some("gt3".to_string()));
        assert_eq!(context.track, Some("spa".to_string()));
        assert_eq!(context.session_type, Some("race".to_string()));
    }

    #[test]
    fn test_telemetry_flags_default() {
        let flags = TelemetryFlags::default();
        assert!(!flags.yellow_flag);
        assert!(!flags.red_flag);
        assert!(!flags.blue_flag);
        assert!(!flags.checkered_flag);
        assert!(!flags.pit_limiter);
        assert!(!flags.drs_enabled);
        assert!(!flags.ers_available);
        assert!(!flags.in_pit);
    }

    #[test]
    fn test_telemetry_statistics_default() {
        let stats = TelemetryStatistics::default();
        assert_eq!(stats.packets_received, 0);
        assert_eq!(stats.packets_dropped, 0);
        assert!(stats.last_packet_time.is_none());
        assert_eq!(stats.average_rate_hz, 0.0);
        assert_eq!(stats.connection_errors, 0);
    }
}