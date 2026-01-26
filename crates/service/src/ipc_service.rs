//! IPC service implementation with domain/wire type conversion
//!
//! This module provides the gRPC service implementation that uses the conversion
//! layer to separate domain logic from wire protocol concerns.

use std::collections::{BTreeMap, HashMap};
use std::pin::Pin;
use std::sync::Arc;
use std::time::SystemTime;

use async_trait::async_trait;
use tokio::sync::{RwLock, broadcast};
use tokio_stream::Stream;
use tonic::{Request, Response, Status};
use tracing::debug;

use racing_wheel_schemas::generated::wheel::v1::{
    ApplyProfileRequest, ConfigureTelemetryRequest, DeviceId as WireDeviceId, DeviceStatus,
    DiagnosticInfo, FeatureNegotiationRequest, FeatureNegotiationResponse, GameStatus, HealthEvent,
    OpResult, Profile as WireProfile, ProfileList, wheel_service_server::WheelService,
};
use racing_wheel_schemas::ipc_conversion::ConversionError;

// Import domain services (these will be the real implementations)
use crate::ApplicationProfileService;
use crate::device_service::ApplicationDeviceService;
use crate::game_service::GameService;
use crate::safety_service::ApplicationSafetyService;

/// Health event for internal broadcasting
#[derive(Debug, Clone)]
pub struct HealthEventInternal {
    pub timestamp: SystemTime,
    pub device_id: String,
    pub event_type: i32, // Maps to HealthEventType enum
    pub message: String,
    pub metadata: HashMap<String, String>,
}

/// IPC service implementation that uses domain services with conversion layer
#[derive(Clone)]
pub struct WheelServiceImpl {
    device_service: Arc<ApplicationDeviceService>,
    profile_service: Arc<ApplicationProfileService>,
    safety_service: Arc<ApplicationSafetyService>,
    game_service: Arc<GameService>,
    health_broadcaster: broadcast::Sender<HealthEventInternal>,
    #[allow(dead_code)]
    connected_clients: Arc<RwLock<HashMap<String, ClientInfo>>>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ClientInfo {
    id: String,
    connected_at: std::time::Instant,
    features: Vec<String>,
    version: String,
}

impl WheelServiceImpl {
    pub fn new(
        device_service: Arc<ApplicationDeviceService>,
        profile_service: Arc<ApplicationProfileService>,
        safety_service: Arc<ApplicationSafetyService>,
        game_service: Arc<GameService>,
        health_broadcaster: broadcast::Sender<HealthEventInternal>,
    ) -> Self {
        Self {
            device_service,
            profile_service,
            safety_service,
            game_service,
            health_broadcaster,
            connected_clients: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl WheelService for WheelServiceImpl {
    type ListDevicesStream = Pin<
        Box<
            dyn Stream<
                    Item = Result<racing_wheel_schemas::generated::wheel::v1::DeviceInfo, Status>,
                > + Send,
        >,
    >;
    type SubscribeHealthStream = Pin<Box<dyn Stream<Item = Result<HealthEvent, Status>> + Send>>;

    /// Feature negotiation for backward compatibility
    async fn negotiate_features(
        &self,
        request: Request<FeatureNegotiationRequest>,
    ) -> Result<Response<FeatureNegotiationResponse>, Status> {
        let req = request.into_inner();
        debug!(
            "Feature negotiation from client version: {}",
            req.client_version
        );

        // For now, accept all clients with basic compatibility check
        let compatible = is_version_compatible(&req.client_version, "1.0.0");

        let response = FeatureNegotiationResponse {
            server_version: "1.0.0".to_string(),
            supported_features: vec![
                "device_management".to_string(),
                "profile_management".to_string(),
                "safety_control".to_string(),
                "health_monitoring".to_string(),
            ],
            enabled_features: vec![
                "device_management".to_string(),
                "profile_management".to_string(),
                "safety_control".to_string(),
                "health_monitoring".to_string(),
            ],
            compatible,
            min_client_version: "1.0.0".to_string(),
        };

        Ok(Response::new(response))
    }

    /// List all connected devices (streaming)
    async fn list_devices(
        &self,
        _request: Request<()>,
    ) -> Result<Response<Self::ListDevicesStream>, Status> {
        debug!("ListDevices called");

        let device_service = self.device_service.clone();

        let stream = async_stream::stream! {
            match device_service.list_devices().await {
                Ok(devices) => {
                    for device in devices {
                        // Convert engine DeviceInfo to wire DeviceInfo
                        let device_info = racing_wheel_schemas::generated::wheel::v1::DeviceInfo {
                            id: device.id.to_string(),
                            name: device.name,
                            r#type: 1, // Default to WheelBase type
                            state: if device.is_connected { 1 } else { 0 },
                            capabilities: None, // TODO: Convert capabilities if needed
                        };
                        yield Ok(device_info);
                    }
                }
                Err(e) => {
                    yield Err(Status::internal(format!("Failed to list devices: {}", e)));
                }
            }
        };

        Ok(Response::new(Box::pin(stream)))
    }

    /// Get device status
    async fn get_device_status(
        &self,
        request: Request<WireDeviceId>,
    ) -> Result<Response<DeviceStatus>, Status> {
        let device_id_wire = request.into_inner();
        debug!("GetDeviceStatus called for device: {}", device_id_wire.id);

        // Convert wire DeviceId to domain DeviceId
        let device_id: racing_wheel_schemas::domain::DeviceId =
            device_id_wire
                .id
                .parse()
                .map_err(|e: racing_wheel_schemas::domain::DomainError| {
                    Status::invalid_argument(format!("Invalid device ID: {}", e))
                })?;

        match self.device_service.get_device_status(&device_id).await {
            Ok((device, telemetry)) => {
                // Convert domain types to wire types
                let device_info = racing_wheel_schemas::generated::wheel::v1::DeviceInfo {
                    id: device.id.to_string(),
                    name: device.name,
                    r#type: 1, // Default to WheelBase type
                    state: if device.is_connected { 1 } else { 0 },
                    capabilities: None, // TODO: Convert capabilities if needed
                };
                let telemetry_data: Option<
                    racing_wheel_schemas::generated::wheel::v1::TelemetryData,
                > = telemetry.map(|t| {
                    racing_wheel_schemas::generated::wheel::v1::TelemetryData {
                        wheel_angle_mdeg: (t.wheel_angle_deg * 1000.0) as i32,
                        wheel_speed_mrad_s: (t.wheel_speed_rad_s * 1000.0) as i32,
                        temp_c: t.temperature_c as u32,
                        faults: t.fault_flags as u32,
                        hands_on: t.hands_on,
                        sequence: 0, // Default sequence number
                    }
                });

                let device_status = DeviceStatus {
                    device: Some(device_info),
                    last_seen: Some(prost_types::Timestamp {
                        seconds: SystemTime::now()
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64,
                        nanos: 0,
                    }),
                    active_faults: vec![], // Will be populated by device service
                    telemetry: telemetry_data,
                };

                Ok(Response::new(device_status))
            }
            Err(e) => Err(Status::not_found(format!("Device not found: {}", e))),
        }
    }

    /// Get active profile for a device
    async fn get_active_profile(
        &self,
        request: Request<WireDeviceId>,
    ) -> Result<Response<WireProfile>, Status> {
        let device_id_wire = request.into_inner();
        debug!("GetActiveProfile called for device: {}", device_id_wire.id);

        // Convert wire DeviceId to domain DeviceId
        let device_id: racing_wheel_schemas::domain::DeviceId =
            device_id_wire
                .id
                .parse()
                .map_err(|e: racing_wheel_schemas::domain::DomainError| {
                    Status::invalid_argument(format!("Invalid device ID: {}", e))
                })?;

        match self.profile_service.get_active_profile(&device_id).await {
            Ok(Some(profile_id)) => {
                // Load the full profile and convert to wire format
                match self.profile_service.load_profile(profile_id.as_ref()).await {
                    Ok(profile) => {
                        let wire_profile: WireProfile = profile.into();
                        Ok(Response::new(wire_profile))
                    }
                    Err(e) => Err(Status::not_found(format!("Profile not found: {}", e))),
                }
            }
            Ok(None) => Err(Status::not_found("No active profile for device")),
            Err(e) => Err(Status::internal(format!(
                "Failed to get active profile: {}",
                e
            ))),
        }
    }

    /// Apply a profile to a device
    async fn apply_profile(
        &self,
        request: Request<ApplyProfileRequest>,
    ) -> Result<Response<OpResult>, Status> {
        let req = request.into_inner();
        debug!("ApplyProfile called");

        let device_id_wire = req
            .device
            .ok_or_else(|| Status::invalid_argument("Device ID is required"))?;

        let profile_wire = req
            .profile
            .ok_or_else(|| Status::invalid_argument("Profile is required"))?;

        // Convert wire types to domain types
        let device_id: racing_wheel_schemas::domain::DeviceId =
            device_id_wire
                .id
                .parse()
                .map_err(|e: racing_wheel_schemas::domain::DomainError| {
                    Status::invalid_argument(format!("Invalid device ID: {}", e))
                })?;

        let _profile: racing_wheel_schemas::entities::Profile =
            profile_wire.try_into().map_err(|e: ConversionError| {
                Status::invalid_argument(format!("Invalid profile: {}", e))
            })?;

        // Get device capabilities (simplified for now)
        let max_torque = racing_wheel_schemas::domain::TorqueNm::new(10.0)
            .map_err(|e| Status::internal(format!("invalid max torque: {}", e)))?;
        let device_capabilities = racing_wheel_schemas::entities::DeviceCapabilities::new(
            true, true, true, true, max_torque, 1024, 1000,
        );

        match self
            .profile_service
            .apply_profile_to_device(&device_id, None, None, None, &device_capabilities)
            .await
        {
            Ok(_profile) => Ok(Response::new(OpResult {
                success: true,
                error_message: String::new(),
                metadata: BTreeMap::new(),
            })),
            Err(e) => Ok(Response::new(OpResult {
                success: false,
                error_message: format!("Failed to apply profile: {}", e),
                metadata: BTreeMap::new(),
            })),
        }
    }

    /// List all available profiles
    async fn list_profiles(&self, _request: Request<()>) -> Result<Response<ProfileList>, Status> {
        debug!("ListProfiles called");

        match self.profile_service.list_profiles().await {
            Ok(profiles) => {
                // Convert domain Profiles to wire Profiles
                let wire_profiles: Vec<WireProfile> =
                    profiles.into_iter().map(Into::into).collect();

                Ok(Response::new(ProfileList {
                    profiles: wire_profiles,
                }))
            }
            Err(e) => Err(Status::internal(format!("Failed to list profiles: {}", e))),
        }
    }

    /// Start high torque mode
    async fn start_high_torque(
        &self,
        request: Request<WireDeviceId>,
    ) -> Result<Response<OpResult>, Status> {
        let device_id_wire = request.into_inner();
        debug!("StartHighTorque called for device: {}", device_id_wire.id);

        // Convert wire DeviceId to domain DeviceId
        let device_id: racing_wheel_schemas::domain::DeviceId =
            device_id_wire
                .id
                .parse()
                .map_err(|e: racing_wheel_schemas::domain::DomainError| {
                    Status::invalid_argument(format!("Invalid device ID: {}", e))
                })?;

        match self.safety_service.start_high_torque(&device_id).await {
            Ok(()) => Ok(Response::new(OpResult {
                success: true,
                error_message: String::new(),
                metadata: BTreeMap::new(),
            })),
            Err(e) => Ok(Response::new(OpResult {
                success: false,
                error_message: format!("Failed to start high torque: {}", e),
                metadata: BTreeMap::new(),
            })),
        }
    }

    /// Emergency stop
    async fn emergency_stop(
        &self,
        request: Request<WireDeviceId>,
    ) -> Result<Response<OpResult>, Status> {
        let device_id_wire = request.into_inner();
        debug!("EmergencyStop called for device: {}", device_id_wire.id);

        // Convert wire DeviceId to domain DeviceId
        let device_id: racing_wheel_schemas::domain::DeviceId =
            device_id_wire
                .id
                .parse()
                .map_err(|e: racing_wheel_schemas::domain::DomainError| {
                    Status::invalid_argument(format!("Invalid device ID: {}", e))
                })?;

        match self
            .safety_service
            .emergency_stop(&device_id, "IPC request".to_string())
            .await
        {
            Ok(()) => Ok(Response::new(OpResult {
                success: true,
                error_message: String::new(),
                metadata: BTreeMap::new(),
            })),
            Err(e) => Ok(Response::new(OpResult {
                success: false,
                error_message: format!("Failed to emergency stop: {}", e),
                metadata: BTreeMap::new(),
            })),
        }
    }

    /// Subscribe to health events
    async fn subscribe_health(
        &self,
        _request: Request<()>,
    ) -> Result<Response<Self::SubscribeHealthStream>, Status> {
        debug!("SubscribeHealth called");

        let mut health_receiver = self.health_broadcaster.subscribe();

        let stream = async_stream::stream! {
            while let Ok(event) = health_receiver.recv().await {
                let health_event = HealthEvent {
                    timestamp: Some(prost_types::Timestamp {
                        seconds: event.timestamp
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64,
                        nanos: 0,
                    }),
                    device_id: event.device_id,
                    r#type: event.event_type,
                    message: event.message,
                    metadata: event.metadata.into_iter().collect(),
                };
                yield Ok(health_event);
            }
        };

        Ok(Response::new(Box::pin(stream)))
    }

    /// Get diagnostics
    async fn get_diagnostics(
        &self,
        request: Request<WireDeviceId>,
    ) -> Result<Response<DiagnosticInfo>, Status> {
        let device_id_wire = request.into_inner();
        debug!("GetDiagnostics called for device: {}", device_id_wire.id);

        // Convert wire DeviceId to domain DeviceId
        let device_id: racing_wheel_schemas::domain::DeviceId =
            device_id_wire
                .id
                .parse()
                .map_err(|e: racing_wheel_schemas::domain::DomainError| {
                    Status::invalid_argument(format!("Invalid device ID: {}", e))
                })?;

        // For now, return basic diagnostic info
        // This will be enhanced when the diagnostic service is implemented
        let diagnostic_info = DiagnosticInfo {
            device_id: device_id.to_string(),
            system_info: BTreeMap::new(),
            recent_faults: vec![],
            performance: Some(
                racing_wheel_schemas::generated::wheel::v1::PerformanceMetrics {
                    p99_jitter_us: 0.0,
                    missed_tick_rate: 0.0,
                    total_ticks: 0,
                    missed_ticks: 0,
                },
            ),
        };

        Ok(Response::new(diagnostic_info))
    }

    /// Configure telemetry
    async fn configure_telemetry(
        &self,
        request: Request<ConfigureTelemetryRequest>,
    ) -> Result<Response<OpResult>, Status> {
        let req = request.into_inner();
        debug!("ConfigureTelemetry called for game: {}", req.game_id);

        use std::path::Path;
        match self
            .game_service
            .configure_telemetry(&req.game_id, Path::new(&req.install_path))
            .await
        {
            Ok(_config_diffs) => Ok(Response::new(OpResult {
                success: true,
                error_message: String::new(),
                metadata: BTreeMap::new(),
            })),
            Err(e) => Ok(Response::new(OpResult {
                success: false,
                error_message: format!("Failed to configure telemetry: {}", e),
                metadata: BTreeMap::new(),
            })),
        }
    }

    /// Get game status
    async fn get_game_status(&self, _request: Request<()>) -> Result<Response<GameStatus>, Status> {
        debug!("GetGameStatus called");

        match self.game_service.get_game_status().await {
            Ok(status) => {
                let game_status = GameStatus {
                    active_game: status.active_game.unwrap_or_default(),
                    telemetry_active: status.telemetry_active,
                    car_id: status.car_id.unwrap_or_default(),
                    track_id: status.track_id.unwrap_or_default(),
                };
                Ok(Response::new(game_status))
            }
            Err(e) => Err(Status::internal(format!(
                "Failed to get game status: {}",
                e
            ))),
        }
    }
}

/// Check if client version is compatible with minimum required version
fn is_version_compatible(client_version: &str, min_version: &str) -> bool {
    // Simplified semantic version comparison
    let parse_version = |v: &str| -> Vec<u32> {
        v.split('.')
            .take(3)
            .map(|s| s.parse().unwrap_or(0))
            .collect()
    };

    let client_parts = parse_version(client_version);
    let min_parts = parse_version(min_version);

    if client_parts.len() < 3 || min_parts.len() < 3 {
        return false;
    }

    // Major version must match
    if client_parts[0] != min_parts[0] {
        return false;
    }

    // Minor version must be >= minimum
    if client_parts[1] < min_parts[1] {
        return false;
    }

    // If minor versions match, patch must be >= minimum
    if client_parts[1] == min_parts[1] && client_parts[2] < min_parts[2] {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_compatibility() {
        assert!(is_version_compatible("1.0.0", "1.0.0"));
        assert!(is_version_compatible("1.1.0", "1.0.0"));
        assert!(is_version_compatible("1.0.1", "1.0.0"));
        assert!(!is_version_compatible("0.9.0", "1.0.0"));
        assert!(!is_version_compatible("2.0.0", "1.0.0"));
    }

    #[tokio::test]
    async fn test_device_id_conversion() {
        // Test that invalid device IDs are properly rejected
        let invalid_device_id = WireDeviceId {
            id: "".to_string(), // Empty string should be invalid
        };

        let result: Result<racing_wheel_schemas::domain::DeviceId, _> =
            invalid_device_id.id.parse();
        assert!(result.is_err());
    }
}
