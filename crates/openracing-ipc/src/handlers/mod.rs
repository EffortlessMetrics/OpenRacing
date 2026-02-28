//! gRPC service handler traits

use async_trait::async_trait;

use crate::error::IpcResult;

/// Handler trait for device management operations
#[async_trait]
pub trait DeviceHandler: Send + Sync {
    /// List all connected devices
    async fn list_devices(&self) -> IpcResult<Vec<DeviceInfo>>;

    /// Get device status
    async fn get_device_status(&self, device_id: &str) -> IpcResult<DeviceStatus>;

    /// Subscribe to device change events
    async fn subscribe_devices(&self) -> IpcResult<()>;
}

/// Handler trait for profile management operations
#[async_trait]
pub trait ProfileHandler: Send + Sync {
    /// List all available profiles
    async fn list_profiles(&self) -> IpcResult<Vec<ProfileInfo>>;

    /// Get active profile for a device
    async fn get_active_profile(&self, device_id: &str) -> IpcResult<ProfileInfo>;

    /// Apply a profile to a device
    async fn apply_profile(&self, device_id: &str, profile_id: &str) -> IpcResult<()>;
}

/// Handler trait for safety operations
#[async_trait]
pub trait SafetyHandler: Send + Sync {
    /// Start high torque mode
    async fn start_high_torque(&self, device_id: &str) -> IpcResult<()>;

    /// Emergency stop
    async fn emergency_stop(&self, device_id: &str, reason: &str) -> IpcResult<()>;
}

/// Handler trait for health monitoring
#[async_trait]
pub trait HealthHandler: Send + Sync {
    /// Subscribe to health events
    async fn subscribe_health(&self) -> IpcResult<()>;

    /// Get diagnostic info
    async fn get_diagnostics(&self, device_id: &str) -> IpcResult<DiagnosticInfo>;
}

/// Handler trait for feature negotiation
#[async_trait]
pub trait FeatureNegotiator: Send + Sync {
    /// Negotiate features with client
    async fn negotiate_features(
        &self,
        client_version: &str,
        supported_features: &[String],
    ) -> IpcResult<FeatureNegotiationResult>;
}

/// Device information
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// Device unique identifier
    pub id: String,
    /// Device name
    pub name: String,
    /// Device type (1 = Wheel Base, 2 = Pedals, etc.)
    pub device_type: i32,
    /// Connection state (0 = Disconnected, 1 = Connected)
    pub state: i32,
    /// Device capabilities
    pub capabilities: Option<DeviceCapabilities>,
}

/// Device capabilities
#[derive(Debug, Clone)]
pub struct DeviceCapabilities {
    /// Supports PID FFB
    pub supports_pid: bool,
    /// Supports raw torque at 1kHz
    pub supports_raw_torque_1khz: bool,
    /// Supports health streaming
    pub supports_health_stream: bool,
    /// Supports LED bus
    pub supports_led_bus: bool,
    /// Maximum torque in centi-Newton-meters
    pub max_torque_cnm: u32,
    /// Encoder counts per revolution
    pub encoder_cpr: u32,
    /// Minimum report period in microseconds
    pub min_report_period_us: u32,
}

/// Device status
#[derive(Debug, Clone)]
pub struct DeviceStatus {
    /// Device info
    pub device: DeviceInfo,
    /// Last seen timestamp (Unix epoch)
    pub last_seen: i64,
    /// Active faults
    pub active_faults: Vec<String>,
    /// Telemetry data
    pub telemetry: Option<TelemetryData>,
}

/// Telemetry data
#[derive(Debug, Clone)]
pub struct TelemetryData {
    /// Wheel angle in degrees
    pub wheel_angle_deg: f32,
    /// Wheel speed in radians per second
    pub wheel_speed_rad_s: f32,
    /// Temperature in Celsius
    pub temperature_c: f32,
    /// Fault flags
    pub fault_flags: u32,
    /// Hands on detection
    pub hands_on: bool,
}

/// Profile information
#[derive(Debug, Clone)]
pub struct ProfileInfo {
    /// Profile unique identifier
    pub id: String,
    /// Schema version
    pub schema_version: String,
    /// Profile name
    pub name: String,
    /// Profile scope
    pub scope: ProfileScope,
}

/// Profile scope
#[derive(Debug, Clone)]
pub struct ProfileScope {
    /// Game identifier
    pub game: Option<String>,
    /// Car identifier
    pub car: Option<String>,
    /// Track identifier
    pub track: Option<String>,
}

/// Diagnostic information
#[derive(Debug, Clone)]
pub struct DiagnosticInfo {
    /// Device ID
    pub device_id: String,
    /// System info key-value pairs
    pub system_info: std::collections::BTreeMap<String, String>,
    /// Recent faults
    pub recent_faults: Vec<FaultRecord>,
    /// Performance metrics
    pub performance: Option<PerformanceMetrics>,
}

/// Fault record
#[derive(Debug, Clone)]
pub struct FaultRecord {
    /// Timestamp
    pub timestamp: i64,
    /// Fault code
    pub code: String,
    /// Fault message
    pub message: String,
    /// Whether the fault is still active
    pub active: bool,
}

/// Performance metrics
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    /// P99 jitter in microseconds
    pub p99_jitter_us: f64,
    /// Missed tick rate
    pub missed_tick_rate: f64,
    /// Total tick count
    pub total_ticks: u64,
    /// Missed tick count
    pub missed_ticks: u64,
}

/// Feature negotiation result
#[derive(Debug, Clone)]
pub struct FeatureNegotiationResult {
    /// Server version
    pub server_version: String,
    /// All supported features
    pub supported_features: Vec<String>,
    /// Enabled features (intersection)
    pub enabled_features: Vec<String>,
    /// Whether client is compatible
    pub compatible: bool,
    /// Minimum client version required
    pub min_client_version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_info_creation() {
        let info = DeviceInfo {
            id: "test-device".to_string(),
            name: "Test Wheel".to_string(),
            device_type: 1,
            state: 1,
            capabilities: None,
        };

        assert_eq!(info.id, "test-device");
        assert_eq!(info.device_type, 1);
    }

    #[test]
    fn test_feature_negotiation_result() {
        let result = FeatureNegotiationResult {
            server_version: "1.0.0".to_string(),
            supported_features: vec!["device_management".to_string()],
            enabled_features: vec!["device_management".to_string()],
            compatible: true,
            min_client_version: "1.0.0".to_string(),
        };

        assert!(result.compatible);
        assert_eq!(result.server_version, "1.0.0");
    }
}
