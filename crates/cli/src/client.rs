//! IPC client for communicating with wheeld service
//!
//! Uses gRPC via tonic to connect to the wheeld service. Falls back to a helpful
//! error message when the service is not running.

use crate::error::CliError;
use anyhow::Result;
use racing_wheel_schemas::generated::wheel::v1 as wire;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_stream::StreamExt;

/// Default gRPC endpoint for the wheeld service
const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:50051";

/// Client for communicating with the wheel service via gRPC
pub struct WheelClient {
    /// The underlying gRPC client, wrapped in Arc<Mutex<>> because tonic's
    /// generated client methods take `&mut self`.
    inner: Arc<Mutex<wire::wheel_service_client::WheelServiceClient<tonic::transport::Channel>>>,
}

impl WheelClient {
    /// Create a new client connection to the wheeld service.
    ///
    /// If `endpoint` is `None`, connects to the default endpoint (`http://127.0.0.1:50051`).
    /// Returns `CliError::ServiceUnavailable` if the service cannot be reached.
    pub async fn connect(endpoint: Option<&str>) -> Result<Self> {
        let endpoint_str = endpoint.unwrap_or(DEFAULT_ENDPOINT);

        // Validate endpoint format
        if !endpoint_str.starts_with("http://") && !endpoint_str.starts_with("https://") {
            return Err(CliError::ServiceUnavailable("Invalid endpoint format".to_string()).into());
        }

        let channel = tonic::transport::Endpoint::from_shared(endpoint_str.to_string())
            .map_err(|e| CliError::ServiceUnavailable(format!("Invalid endpoint: {}", e)))?
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(10))
            .connect()
            .await
            .map_err(|e| {
                CliError::ServiceUnavailable(format!(
                    "Could not connect to wheeld service at {}: {}. Is wheeld running?",
                    endpoint_str, e
                ))
            })?;

        let grpc_client = wire::wheel_service_client::WheelServiceClient::new(channel);

        Ok(Self {
            inner: Arc::new(Mutex::new(grpc_client)),
        })
    }

    /// List all connected devices by calling the gRPC ListDevices streaming RPC.
    pub async fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        let mut client = self.inner.lock().await;
        let response = client.list_devices(()).await.map_err(|status| {
            CliError::ServiceUnavailable(format!("Failed to list devices: {}", status.message()))
        })?;

        let mut stream = response.into_inner();
        let mut devices = Vec::new();

        while let Some(item) = stream.next().await {
            match item {
                Ok(wire_device) => {
                    devices.push(DeviceInfo::from_wire(wire_device));
                }
                Err(status) => {
                    tracing::warn!("Error receiving device from stream: {}", status.message());
                    break;
                }
            }
        }

        Ok(devices)
    }

    /// Get device status by calling the gRPC GetDeviceStatus RPC.
    pub async fn get_device_status(&self, device_id: &str) -> Result<DeviceStatus> {
        let mut client = self.inner.lock().await;
        let request = wire::DeviceId {
            id: device_id.to_string(),
        };

        let response = client.get_device_status(request).await.map_err(|status| {
            if status.code() == tonic::Code::NotFound {
                CliError::DeviceNotFound(device_id.to_string())
            } else {
                CliError::ServiceUnavailable(format!(
                    "Failed to get device status: {}",
                    status.message()
                ))
            }
        })?;

        let wire_status = response.into_inner();
        Ok(DeviceStatus::from_wire(wire_status, device_id))
    }

    /// Apply profile to device by calling the gRPC ApplyProfile RPC.
    pub async fn apply_profile(
        &self,
        device_id: &str,
        _profile: &racing_wheel_schemas::config::ProfileSchema,
    ) -> Result<()> {
        let mut client = self.inner.lock().await;
        let request = wire::ApplyProfileRequest {
            device: Some(wire::DeviceId {
                id: device_id.to_string(),
            }),
            // Convert the profile schema to wire format
            // For now, send a minimal profile -- full conversion can be added later
            profile: Some(wire::Profile {
                schema_version: "wheel.profile/1".to_string(),
                scope: None,
                base: None,
                leds: None,
                haptics: None,
                signature: String::new(),
            }),
        };

        let response = client.apply_profile(request).await.map_err(|status| {
            CliError::ServiceUnavailable(format!("Failed to apply profile: {}", status.message()))
        })?;

        let result = response.into_inner();
        if result.success {
            Ok(())
        } else {
            Err(CliError::ValidationError(result.error_message).into())
        }
    }

    /// Get active profile for device
    #[allow(dead_code)]
    pub async fn get_active_profile(
        &self,
        device_id: &str,
    ) -> Result<racing_wheel_schemas::config::ProfileSchema> {
        let mut client = self.inner.lock().await;
        let request = wire::DeviceId {
            id: device_id.to_string(),
        };

        let response = client.get_active_profile(request).await.map_err(|status| {
            CliError::ServiceUnavailable(format!(
                "Failed to get active profile: {}",
                status.message()
            ))
        })?;

        let wire_profile = response.into_inner();
        Ok(ProfileSchema::from_wire(wire_profile))
    }

    /// Start high torque mode by calling the gRPC StartHighTorque RPC.
    pub async fn start_high_torque(&self, device_id: &str) -> Result<()> {
        let mut client = self.inner.lock().await;
        let request = wire::DeviceId {
            id: device_id.to_string(),
        };

        let response = client.start_high_torque(request).await.map_err(|status| {
            CliError::ServiceUnavailable(format!(
                "Failed to start high torque: {}",
                status.message()
            ))
        })?;

        let result = response.into_inner();
        if result.success {
            Ok(())
        } else {
            Err(CliError::ValidationError(result.error_message).into())
        }
    }

    /// Emergency stop by calling the gRPC EmergencyStop RPC.
    pub async fn emergency_stop(&self, device_id: Option<&str>) -> Result<()> {
        let mut client = self.inner.lock().await;
        // The gRPC API requires a device ID; use empty string for "all devices"
        let request = wire::DeviceId {
            id: device_id.unwrap_or("").to_string(),
        };

        let response = client.emergency_stop(request).await.map_err(|status| {
            CliError::ServiceUnavailable(format!(
                "Failed to send emergency stop: {}",
                status.message()
            ))
        })?;

        let result = response.into_inner();
        if result.success {
            Ok(())
        } else {
            Err(CliError::ValidationError(result.error_message).into())
        }
    }

    /// Get diagnostics by calling the gRPC GetDiagnostics RPC.
    pub async fn get_diagnostics(&self, device_id: &str) -> Result<DiagnosticInfo> {
        let mut client = self.inner.lock().await;
        let request = wire::DeviceId {
            id: device_id.to_string(),
        };

        let response = client.get_diagnostics(request).await.map_err(|status| {
            CliError::ServiceUnavailable(format!("Failed to get diagnostics: {}", status.message()))
        })?;

        let wire_diag = response.into_inner();
        Ok(DiagnosticInfo::from_wire(wire_diag))
    }

    /// Configure game telemetry by calling the gRPC ConfigureTelemetry RPC.
    pub async fn configure_telemetry(
        &self,
        game_id: &str,
        install_path: Option<&str>,
    ) -> Result<()> {
        let mut client = self.inner.lock().await;
        let request = wire::ConfigureTelemetryRequest {
            game_id: game_id.to_string(),
            install_path: install_path.unwrap_or("").to_string(),
            enable_auto_config: true,
        };

        let response = client
            .configure_telemetry(request)
            .await
            .map_err(|status| {
                CliError::ServiceUnavailable(format!(
                    "Failed to configure telemetry: {}",
                    status.message()
                ))
            })?;

        let result = response.into_inner();
        if result.success {
            Ok(())
        } else {
            Err(CliError::ValidationError(result.error_message).into())
        }
    }

    /// Get game status by calling the gRPC GetGameStatus RPC.
    pub async fn get_game_status(&self) -> Result<GameStatus> {
        let mut client = self.inner.lock().await;
        let response = client.get_game_status(()).await.map_err(|status| {
            CliError::ServiceUnavailable(format!("Failed to get game status: {}", status.message()))
        })?;

        let wire_status = response.into_inner();
        Ok(GameStatus::from_wire(wire_status))
    }

    /// Subscribe to health events via the gRPC SubscribeHealth streaming RPC.
    pub async fn subscribe_health(&self) -> Result<HealthEventStream> {
        let mut client = self.inner.lock().await;
        let response = client.subscribe_health(()).await.map_err(|status| {
            CliError::ServiceUnavailable(format!(
                "Failed to subscribe to health events: {}",
                status.message()
            ))
        })?;

        Ok(HealthEventStream {
            inner: response.into_inner(),
        })
    }
}

// ---------------------------------------------------------------------------
// Local CLI types -- the output module relies on these structures.
// Wire types from the gRPC schema are converted into these types.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub device_type: DeviceType,
    pub state: DeviceState,
    pub capabilities: DeviceCapabilities,
}

impl DeviceInfo {
    fn from_wire(w: wire::DeviceInfo) -> Self {
        Self {
            id: w.id,
            name: w.name,
            device_type: DeviceType::from_wire(w.r#type),
            state: DeviceState::from_wire(w.state),
            capabilities: w
                .capabilities
                .map(DeviceCapabilities::from_wire)
                .unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeviceType {
    WheelBase,
    Pedals,
    Shifter,
    Handbrake,
}

impl DeviceType {
    fn from_wire(v: i32) -> Self {
        match v {
            1 => DeviceType::WheelBase,
            2 => DeviceType::Pedals,
            3 => DeviceType::Shifter,
            4 => DeviceType::Handbrake,
            _ => DeviceType::WheelBase, // default for unknown
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeviceState {
    Connected,
    Disconnected,
    Faulted,
    Calibrating,
}

impl DeviceState {
    fn from_wire(v: i32) -> Self {
        match v {
            1 => DeviceState::Connected,
            2 => DeviceState::Disconnected,
            3 => DeviceState::Faulted,
            4 => DeviceState::Calibrating,
            _ => DeviceState::Disconnected, // default for unknown
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCapabilities {
    pub supports_pid: bool,
    pub supports_raw_torque_1khz: bool,
    pub supports_health_stream: bool,
    pub supports_led_bus: bool,
    pub max_torque_nm: f32,
    pub encoder_cpr: u32,
    pub min_report_period_us: u32,
}

impl DeviceCapabilities {
    fn from_wire(w: wire::DeviceCapabilities) -> Self {
        Self {
            supports_pid: w.supports_pid,
            supports_raw_torque_1khz: w.supports_raw_torque_1khz,
            supports_health_stream: w.supports_health_stream,
            supports_led_bus: w.supports_led_bus,
            // Wire uses centi-Nm (max_torque_cnm); convert to Nm
            max_torque_nm: w.max_torque_cnm as f32 / 100.0,
            encoder_cpr: w.encoder_cpr,
            min_report_period_us: w.min_report_period_us,
        }
    }
}

impl Default for DeviceCapabilities {
    fn default() -> Self {
        Self {
            supports_pid: false,
            supports_raw_torque_1khz: false,
            supports_health_stream: false,
            supports_led_bus: false,
            max_torque_nm: 0.0,
            encoder_cpr: 1024,
            min_report_period_us: 1000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStatus {
    pub device: DeviceInfo,
    pub last_seen: chrono::DateTime<chrono::Utc>,
    pub active_faults: Vec<String>,
    pub telemetry: TelemetryData,
}

impl DeviceStatus {
    fn from_wire(w: wire::DeviceStatus, fallback_id: &str) -> Self {
        let device = w
            .device
            .map(DeviceInfo::from_wire)
            .unwrap_or_else(|| DeviceInfo {
                id: fallback_id.to_string(),
                name: "Unknown".to_string(),
                device_type: DeviceType::WheelBase,
                state: DeviceState::Connected,
                capabilities: DeviceCapabilities::default(),
            });

        let last_seen = w
            .last_seen
            .and_then(|ts| chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32))
            .unwrap_or_else(chrono::Utc::now);

        let telemetry = w
            .telemetry
            .map(TelemetryData::from_wire)
            .unwrap_or_default();

        Self {
            device,
            last_seen,
            active_faults: w.active_faults,
            telemetry,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryData {
    pub wheel_angle_deg: f32,
    pub wheel_speed_rad_s: f32,
    pub temperature_c: u8,
    pub fault_flags: u8,
    pub hands_on: bool,
}

impl TelemetryData {
    fn from_wire(w: wire::TelemetryData) -> Self {
        Self {
            // Wire uses milli-degrees; convert to degrees
            wheel_angle_deg: w.wheel_angle_mdeg as f32 / 1000.0,
            // Wire uses milli-rad/s; convert to rad/s
            wheel_speed_rad_s: w.wheel_speed_mrad_s as f32 / 1000.0,
            temperature_c: w.temp_c as u8,
            fault_flags: w.faults as u8,
            hands_on: w.hands_on,
        }
    }
}

impl Default for TelemetryData {
    fn default() -> Self {
        Self {
            wheel_angle_deg: 0.0,
            wheel_speed_rad_s: 0.0,
            temperature_c: 0,
            fault_flags: 0,
            hands_on: false,
        }
    }
}

/// Helper struct to convert wire Profile to the CLI's ProfileSchema
struct ProfileSchema;

impl ProfileSchema {
    fn from_wire(w: wire::Profile) -> racing_wheel_schemas::config::ProfileSchema {
        let scope = w.scope.map(|s| racing_wheel_schemas::config::ProfileScope {
            game: if s.game.is_empty() {
                None
            } else {
                Some(s.game)
            },
            car: if s.car.is_empty() { None } else { Some(s.car) },
            track: if s.track.is_empty() {
                None
            } else {
                Some(s.track)
            },
        });

        let base = w.base.map(|b| racing_wheel_schemas::config::BaseConfig {
            ffb_gain: b.ffb_gain,
            dor_deg: b.dor_deg as u16,
            torque_cap_nm: b.torque_cap_nm,
            filters: b
                .filters
                .map(|f| racing_wheel_schemas::config::FilterConfig {
                    reconstruction: f.reconstruction as u8,
                    friction: f.friction,
                    damper: f.damper,
                    inertia: f.inertia,
                    bumpstop: Default::default(),
                    hands_off: Default::default(),
                    torque_cap: None,
                    notch_filters: f
                        .notch_filters
                        .into_iter()
                        .map(|n| racing_wheel_schemas::config::NotchFilter {
                            hz: n.hz,
                            q: n.q,
                            gain_db: n.gain_db,
                        })
                        .collect(),
                    slew_rate: f.slew_rate,
                    curve_points: f
                        .curve_points
                        .into_iter()
                        .map(|p| racing_wheel_schemas::config::CurvePoint {
                            input: p.input,
                            output: p.output,
                        })
                        .collect(),
                })
                .unwrap_or_default(),
        });

        racing_wheel_schemas::config::ProfileSchema {
            schema: w.schema_version,
            scope: scope.unwrap_or(racing_wheel_schemas::config::ProfileScope {
                game: None,
                car: None,
                track: None,
            }),
            base: base.unwrap_or(racing_wheel_schemas::config::BaseConfig {
                ffb_gain: 0.75,
                dor_deg: 540,
                torque_cap_nm: 8.0,
                filters: racing_wheel_schemas::config::FilterConfig::default(),
            }),
            leds: None,
            haptics: None,
            signature: if w.signature.is_empty() {
                None
            } else {
                Some(w.signature)
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticInfo {
    pub device_id: String,
    pub system_info: std::collections::HashMap<String, String>,
    pub recent_faults: Vec<String>,
    pub performance: PerformanceMetrics,
}

impl DiagnosticInfo {
    fn from_wire(w: wire::DiagnosticInfo) -> Self {
        Self {
            device_id: w.device_id,
            system_info: w.system_info.into_iter().collect(),
            recent_faults: w.recent_faults,
            performance: w
                .performance
                .map(PerformanceMetrics::from_wire)
                .unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub p99_jitter_us: f32,
    pub missed_tick_rate: f32,
    pub total_ticks: u64,
    pub missed_ticks: u64,
}

impl PerformanceMetrics {
    fn from_wire(w: wire::PerformanceMetrics) -> Self {
        Self {
            p99_jitter_us: w.p99_jitter_us,
            missed_tick_rate: w.missed_tick_rate,
            total_ticks: w.total_ticks,
            missed_ticks: w.missed_ticks,
        }
    }
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            p99_jitter_us: 0.0,
            missed_tick_rate: 0.0,
            total_ticks: 0,
            missed_ticks: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameStatus {
    pub active_game: Option<String>,
    pub telemetry_active: bool,
    pub car_id: Option<String>,
    pub track_id: Option<String>,
}

impl GameStatus {
    fn from_wire(w: wire::GameStatus) -> Self {
        Self {
            active_game: if w.active_game.is_empty() {
                None
            } else {
                Some(w.active_game)
            },
            telemetry_active: w.telemetry_active,
            car_id: if w.car_id.is_empty() {
                None
            } else {
                Some(w.car_id)
            },
            track_id: if w.track_id.is_empty() {
                None
            } else {
                Some(w.track_id)
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthEvent {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub device_id: String,
    pub event_type: HealthEventType,
    pub message: String,
    pub metadata: std::collections::HashMap<String, String>,
}

impl HealthEvent {
    fn from_wire(w: wire::HealthEvent) -> Self {
        let timestamp = w
            .timestamp
            .and_then(|ts| chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32))
            .unwrap_or_else(chrono::Utc::now);

        Self {
            timestamp,
            device_id: w.device_id,
            event_type: HealthEventType::from_wire(w.r#type),
            message: w.message,
            metadata: w.metadata.into_iter().collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthEventType {
    DeviceConnected,
    DeviceDisconnected,
    FaultDetected,
    FaultCleared,
    PerformanceWarning,
}

impl HealthEventType {
    fn from_wire(v: i32) -> Self {
        match v {
            1 => HealthEventType::DeviceConnected,
            2 => HealthEventType::DeviceDisconnected,
            3 => HealthEventType::FaultDetected,
            4 => HealthEventType::FaultCleared,
            5 => HealthEventType::PerformanceWarning,
            _ => HealthEventType::PerformanceWarning, // default for unknown
        }
    }
}

/// Health event stream wrapping a gRPC server-streaming response
pub struct HealthEventStream {
    inner: tonic::codec::Streaming<wire::HealthEvent>,
}

impl HealthEventStream {
    pub async fn next(&mut self) -> Option<HealthEvent> {
        match self.inner.next().await {
            Some(Ok(wire_event)) => Some(HealthEvent::from_wire(wire_event)),
            Some(Err(status)) => {
                tracing::warn!("Health stream error: {}", status.message());
                None
            }
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn connect_rejects_invalid_scheme() {
        let result = WheelClient::connect(Some("ftp://localhost")).await;
        assert!(result.is_err());
        let err_msg = result
            .as_ref()
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(err_msg.contains("Invalid endpoint"));
    }

    #[tokio::test]
    async fn connect_rejects_plain_string() {
        let result = WheelClient::connect(Some("not-a-url")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn connect_fails_when_service_not_running() {
        // Try connecting to a port where no service is running
        let result = WheelClient::connect(Some("http://127.0.0.1:19999")).await;
        assert!(result.is_err());
        let err_msg = result
            .as_ref()
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(
            err_msg.contains("Could not connect") || err_msg.contains("Service unavailable"),
            "Expected connection error, got: {}",
            err_msg
        );
    }

    #[test]
    fn device_capabilities_default() {
        let caps = DeviceCapabilities::default();
        assert!(!caps.supports_pid);
        assert!(!caps.supports_raw_torque_1khz);
        assert!(!caps.supports_health_stream);
        assert!(!caps.supports_led_bus);
        assert!((caps.max_torque_nm - 0.0).abs() < f32::EPSILON);
        assert_eq!(caps.encoder_cpr, 1024);
        assert_eq!(caps.min_report_period_us, 1000);
    }

    #[test]
    fn device_info_from_wire() {
        let wire_device = wire::DeviceInfo {
            id: "wheel-001".to_string(),
            name: "Test Wheel".to_string(),
            r#type: 1, // WheelBase
            state: 1,  // Connected
            capabilities: Some(wire::DeviceCapabilities {
                supports_pid: true,
                supports_raw_torque_1khz: true,
                supports_health_stream: true,
                supports_led_bus: false,
                max_torque_cnm: 800, // 8.0 Nm
                encoder_cpr: 2048,
                min_report_period_us: 1000,
            }),
        };

        let device = DeviceInfo::from_wire(wire_device);
        assert_eq!(device.id, "wheel-001");
        assert_eq!(device.name, "Test Wheel");
        assert!(matches!(device.device_type, DeviceType::WheelBase));
        assert!(matches!(device.state, DeviceState::Connected));
        assert!(device.capabilities.supports_pid);
        assert!((device.capabilities.max_torque_nm - 8.0).abs() < f32::EPSILON);
    }

    #[test]
    fn device_state_from_wire_covers_all_variants() {
        assert!(matches!(DeviceState::from_wire(1), DeviceState::Connected));
        assert!(matches!(
            DeviceState::from_wire(2),
            DeviceState::Disconnected
        ));
        assert!(matches!(DeviceState::from_wire(3), DeviceState::Faulted));
        assert!(matches!(
            DeviceState::from_wire(4),
            DeviceState::Calibrating
        ));
        // Unknown defaults to Disconnected
        assert!(matches!(
            DeviceState::from_wire(99),
            DeviceState::Disconnected
        ));
    }

    #[test]
    fn device_type_from_wire_covers_all_variants() {
        assert!(matches!(DeviceType::from_wire(1), DeviceType::WheelBase));
        assert!(matches!(DeviceType::from_wire(2), DeviceType::Pedals));
        assert!(matches!(DeviceType::from_wire(3), DeviceType::Shifter));
        assert!(matches!(DeviceType::from_wire(4), DeviceType::Handbrake));
        // Unknown defaults to WheelBase
        assert!(matches!(DeviceType::from_wire(99), DeviceType::WheelBase));
    }

    #[test]
    fn telemetry_data_from_wire() {
        let wire_telemetry = wire::TelemetryData {
            wheel_angle_mdeg: 45_000,  // 45.0 degrees
            wheel_speed_mrad_s: 1_500, // 1.5 rad/s
            temp_c: 42,
            faults: 0,
            hands_on: true,
            sequence: 100,
        };

        let telemetry = TelemetryData::from_wire(wire_telemetry);
        assert!((telemetry.wheel_angle_deg - 45.0).abs() < 0.01);
        assert!((telemetry.wheel_speed_rad_s - 1.5).abs() < 0.01);
        assert_eq!(telemetry.temperature_c, 42);
        assert_eq!(telemetry.fault_flags, 0);
        assert!(telemetry.hands_on);
    }

    #[test]
    fn diagnostic_info_from_wire() {
        let wire_diag = wire::DiagnosticInfo {
            device_id: "dev-1".to_string(),
            system_info: std::collections::BTreeMap::from([(
                "os".to_string(),
                "linux".to_string(),
            )]),
            recent_faults: vec!["fault-1".to_string()],
            performance: Some(wire::PerformanceMetrics {
                p99_jitter_us: 0.15,
                missed_tick_rate: 0.0001,
                total_ticks: 1_000_000,
                missed_ticks: 1,
            }),
        };

        let diag = DiagnosticInfo::from_wire(wire_diag);
        assert_eq!(diag.device_id, "dev-1");
        assert_eq!(
            diag.system_info.get("os").map(|s| s.as_str()),
            Some("linux")
        );
        assert_eq!(diag.recent_faults.len(), 1);
        assert!((diag.performance.p99_jitter_us - 0.15).abs() < f32::EPSILON);
    }

    #[test]
    fn game_status_from_wire() {
        let wire_status = wire::GameStatus {
            active_game: "iracing".to_string(),
            telemetry_active: true,
            car_id: "gt3".to_string(),
            track_id: "spa".to_string(),
        };

        let status = GameStatus::from_wire(wire_status);
        assert_eq!(status.active_game.as_deref(), Some("iracing"));
        assert!(status.telemetry_active);
        assert_eq!(status.car_id.as_deref(), Some("gt3"));
        assert_eq!(status.track_id.as_deref(), Some("spa"));
    }

    #[test]
    fn game_status_from_wire_empty_strings_become_none() {
        let wire_status = wire::GameStatus {
            active_game: String::new(),
            telemetry_active: false,
            car_id: String::new(),
            track_id: String::new(),
        };

        let status = GameStatus::from_wire(wire_status);
        assert!(status.active_game.is_none());
        assert!(status.car_id.is_none());
        assert!(status.track_id.is_none());
    }

    #[test]
    fn health_event_type_from_wire_covers_all_variants() {
        assert!(matches!(
            HealthEventType::from_wire(1),
            HealthEventType::DeviceConnected
        ));
        assert!(matches!(
            HealthEventType::from_wire(2),
            HealthEventType::DeviceDisconnected
        ));
        assert!(matches!(
            HealthEventType::from_wire(3),
            HealthEventType::FaultDetected
        ));
        assert!(matches!(
            HealthEventType::from_wire(4),
            HealthEventType::FaultCleared
        ));
        assert!(matches!(
            HealthEventType::from_wire(5),
            HealthEventType::PerformanceWarning
        ));
    }

    #[test]
    fn performance_metrics_default() {
        let perf = PerformanceMetrics::default();
        assert!((perf.p99_jitter_us - 0.0).abs() < f32::EPSILON);
        assert!((perf.missed_tick_rate - 0.0).abs() < f32::EPSILON);
        assert_eq!(perf.total_ticks, 0);
        assert_eq!(perf.missed_ticks, 0);
    }
}
