//! Tauri IPC Commands for OpenRacing UI
//!
//! This module defines the Tauri commands that are exposed to the frontend
//! for communicating with the wheeld service via IPC.
//!
//! Commands follow the pattern:
//! - Frontend calls `invoke('command_name', { args })`
//! - Command executes and returns Result<T, String>
//! - Frontend receives the result or error message

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tauri::State;
use tokio::sync::RwLock;
use tonic::transport::{Channel, Endpoint};
use tracing::{debug, info, warn};

use racing_wheel_schemas::generated::wheel::v1::{
    ApplyProfileRequest, DeviceId, FeatureNegotiationRequest,
    wheel_service_client::WheelServiceClient,
};

/// Application state shared across all Tauri commands
#[derive(Debug)]
pub struct AppState {
    /// IPC client connection to wheeld service
    client: Option<WheelServiceClient<Channel>>,
    /// Connection status
    connected: bool,
    /// Negotiated features from the service
    negotiated_features: Vec<String>,
    /// Last error message
    last_error: Option<String>,
}

impl AppState {
    /// Create a new application state
    pub fn new() -> Self {
        Self {
            client: None,
            connected: false,
            negotiated_features: Vec::new(),
            last_error: None,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Device information returned to the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub device_type: String,
    pub state: String,
    pub capabilities: DeviceCapabilities,
}

/// Device capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCapabilities {
    pub supports_pid: bool,
    pub supports_raw_torque_1khz: bool,
    pub supports_health_stream: bool,
    pub supports_led_bus: bool,
    pub max_torque_cnm: u32,
    pub encoder_cpr: u32,
}

/// Device status returned to the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStatus {
    pub device: DeviceInfo,
    pub last_seen: String,
    pub active_faults: Vec<String>,
    pub telemetry: Option<TelemetryData>,
}

/// Telemetry data from a device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryData {
    pub wheel_angle_deg: f32,
    pub wheel_speed_rad_s: f32,
    pub temperature_c: u32,
    pub fault_flags: u32,
    pub hands_on: bool,
}

/// Profile information for the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileInfo {
    pub schema_version: String,
    pub game: Option<String>,
    pub car: Option<String>,
    pub track: Option<String>,
    pub ffb_gain: f32,
    pub dor_deg: u32,
    pub torque_cap_nm: f32,
}

/// Service status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub connected: bool,
    pub version: String,
    pub features: Vec<String>,
    pub error: Option<String>,
}

/// Operation result returned to the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpResult {
    pub success: bool,
    pub message: String,
}

/// Connect to the wheeld service
///
/// This command establishes a connection to the wheeld service via IPC.
/// On Windows, this uses TCP (will be Named Pipes in the future).
/// On Linux/macOS, this uses Unix Domain Sockets.
#[tauri::command]
pub async fn connect_service(
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<ServiceStatus, String> {
    info!("Connecting to wheeld service");

    let mut app_state = state.write().await;

    // Build the endpoint based on platform
    let endpoint = Endpoint::from_static("http://127.0.0.1:50051")
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(30));

    // Attempt to connect
    let channel = endpoint
        .connect()
        .await
        .map_err(|e| format!("Failed to connect to wheeld service: {}", e))?;

    let mut client = WheelServiceClient::new(channel);

    // Perform feature negotiation
    let negotiation_request = FeatureNegotiationRequest {
        client_version: env!("CARGO_PKG_VERSION").to_string(),
        supported_features: vec![
            "device_management".to_string(),
            "profile_management".to_string(),
            "safety_control".to_string(),
            "health_monitoring".to_string(),
            "telemetry".to_string(),
        ],
        namespace: "wheel.v1".to_string(),
    };

    let response = client
        .negotiate_features(tonic::Request::new(negotiation_request))
        .await
        .map_err(|e| format!("Feature negotiation failed: {}", e))?;

    let negotiation_response = response.into_inner();

    if !negotiation_response.compatible {
        let error_msg = format!(
            "Client version {} is not compatible with server. Minimum required: {}",
            env!("CARGO_PKG_VERSION"),
            negotiation_response.min_client_version
        );
        app_state.last_error = Some(error_msg.clone());
        return Err(error_msg);
    }

    // Store the client and update state
    app_state.client = Some(client);
    app_state.connected = true;
    app_state.negotiated_features = negotiation_response.enabled_features.clone();
    app_state.last_error = None;

    info!(
        "Connected to wheeld service. Features: {:?}",
        negotiation_response.enabled_features
    );

    Ok(ServiceStatus {
        connected: true,
        version: negotiation_response.server_version,
        features: negotiation_response.enabled_features,
        error: None,
    })
}

/// Disconnect from the wheeld service
#[tauri::command]
pub async fn disconnect_service(state: State<'_, Arc<RwLock<AppState>>>) -> Result<(), String> {
    info!("Disconnecting from wheeld service");

    let mut app_state = state.write().await;
    app_state.client = None;
    app_state.connected = false;
    app_state.negotiated_features.clear();

    Ok(())
}

/// Get the current service status
#[tauri::command]
pub async fn get_service_status(
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<ServiceStatus, String> {
    let app_state = state.read().await;

    Ok(ServiceStatus {
        connected: app_state.connected,
        version: if app_state.connected {
            env!("CARGO_PKG_VERSION").to_string()
        } else {
            String::new()
        },
        features: app_state.negotiated_features.clone(),
        error: app_state.last_error.clone(),
    })
}

/// List all connected devices
///
/// Returns a list of all racing wheel devices currently connected to the system.
/// Requires: 7.1 - THE Tauri_UI SHALL display a list of connected racing wheel devices
#[tauri::command]
pub async fn list_devices(
    state: State<'_, Arc<RwLock<AppState>>>,
) -> Result<Vec<DeviceInfo>, String> {
    debug!("Listing devices");

    let mut app_state = state.write().await;

    let client = app_state
        .client
        .as_mut()
        .ok_or_else(|| "Not connected to wheeld service".to_string())?;

    let response = client
        .list_devices(tonic::Request::new(()))
        .await
        .map_err(|e| format!("Failed to list devices: {}", e))?;

    let mut stream = response.into_inner();
    let mut devices = Vec::new();

    while let Some(device_result) = tokio_stream::StreamExt::next(&mut stream).await {
        let device = device_result.map_err(|e| format!("Error receiving device: {}", e))?;

        let device_type = match device.r#type {
            0 => "Unknown",
            1 => "WheelBase",
            2 => "Pedals",
            3 => "Shifter",
            4 => "Handbrake",
            _ => "Other",
        };

        let state_str = match device.state {
            0 => "Unknown",
            1 => "Connected",
            2 => "Disconnected",
            3 => "Error",
            _ => "Unknown",
        };

        let caps = device.capabilities.unwrap_or_default();

        devices.push(DeviceInfo {
            id: device.id,
            name: device.name,
            device_type: device_type.to_string(),
            state: state_str.to_string(),
            capabilities: DeviceCapabilities {
                supports_pid: caps.supports_pid,
                supports_raw_torque_1khz: caps.supports_raw_torque_1khz,
                supports_health_stream: caps.supports_health_stream,
                supports_led_bus: caps.supports_led_bus,
                max_torque_cnm: caps.max_torque_cnm,
                encoder_cpr: caps.encoder_cpr,
            },
        });
    }

    Ok(devices)
}

/// Get the status of a specific device
///
/// Returns detailed status information for a device including health and telemetry.
/// Requires: 7.2 - WHEN a device is selected, THE Tauri_UI SHALL show device status
#[tauri::command]
pub async fn get_device_status(
    state: State<'_, Arc<RwLock<AppState>>>,
    device_id: String,
) -> Result<DeviceStatus, String> {
    debug!("Getting device status for: {}", device_id);

    let mut app_state = state.write().await;

    let client = app_state
        .client
        .as_mut()
        .ok_or_else(|| "Not connected to wheeld service".to_string())?;

    let response = client
        .get_device_status(tonic::Request::new(DeviceId {
            id: device_id.clone(),
        }))
        .await
        .map_err(|e| format!("Failed to get device status: {}", e))?;

    let status = response.into_inner();

    let device = status
        .device
        .ok_or_else(|| "No device info in response".to_string())?;
    let caps = device.capabilities.unwrap_or_default();

    let device_type = match device.r#type {
        0 => "Unknown",
        1 => "WheelBase",
        2 => "Pedals",
        3 => "Shifter",
        4 => "Handbrake",
        _ => "Other",
    };

    let state_str = match device.state {
        0 => "Unknown",
        1 => "Connected",
        2 => "Disconnected",
        3 => "Error",
        _ => "Unknown",
    };

    let last_seen = status
        .last_seen
        .map(|ts| {
            chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| "Unknown".to_string())
        })
        .unwrap_or_else(|| "Unknown".to_string());

    let telemetry = status.telemetry.map(|t| TelemetryData {
        wheel_angle_deg: t.wheel_angle_mdeg as f32 / 1000.0,
        wheel_speed_rad_s: t.wheel_speed_mrad_s as f32 / 1000.0,
        temperature_c: t.temp_c,
        fault_flags: t.faults,
        hands_on: t.hands_on,
    });

    Ok(DeviceStatus {
        device: DeviceInfo {
            id: device.id,
            name: device.name,
            device_type: device_type.to_string(),
            state: state_str.to_string(),
            capabilities: DeviceCapabilities {
                supports_pid: caps.supports_pid,
                supports_raw_torque_1khz: caps.supports_raw_torque_1khz,
                supports_health_stream: caps.supports_health_stream,
                supports_led_bus: caps.supports_led_bus,
                max_torque_cnm: caps.max_torque_cnm,
                encoder_cpr: caps.encoder_cpr,
            },
        },
        last_seen,
        active_faults: status.active_faults,
        telemetry,
    })
}

/// Get real-time telemetry data for a device
///
/// Returns the latest telemetry snapshot for a device.
/// Requires: 7.4 - THE Tauri_UI SHALL display real-time telemetry data
#[tauri::command]
pub async fn get_telemetry(
    state: State<'_, Arc<RwLock<AppState>>>,
    device_id: String,
) -> Result<TelemetryData, String> {
    debug!("Getting telemetry for device: {}", device_id);

    // Get device status which includes telemetry
    let status = get_device_status(state, device_id).await?;

    status
        .telemetry
        .ok_or_else(|| "No telemetry data available".to_string())
}

/// Apply a profile to a device
///
/// Loads and applies an FFB profile to the specified device.
/// Requires: 7.3 - THE Tauri_UI SHALL allow loading and applying FFB profiles
#[tauri::command]
pub async fn apply_profile(
    state: State<'_, Arc<RwLock<AppState>>>,
    device_id: String,
    profile_path: String,
) -> Result<OpResult, String> {
    info!("Applying profile {} to device {}", profile_path, device_id);

    // Read and parse the profile file
    let profile_content = tokio::fs::read_to_string(&profile_path)
        .await
        .map_err(|e| format!("Failed to read profile file: {}", e))?;

    let profile_json: serde_json::Value = serde_json::from_str(&profile_content)
        .map_err(|e| format!("Failed to parse profile JSON: {}", e))?;

    // Convert to protobuf profile
    let profile = racing_wheel_schemas::generated::wheel::v1::Profile {
        schema_version: profile_json
            .get("schema_version")
            .and_then(|v| v.as_str())
            .unwrap_or("wheel.profile/1")
            .to_string(),
        scope: Some(racing_wheel_schemas::generated::wheel::v1::ProfileScope {
            game: profile_json
                .get("scope")
                .and_then(|s| s.get("game"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            car: profile_json
                .get("scope")
                .and_then(|s| s.get("car"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            track: profile_json
                .get("scope")
                .and_then(|s| s.get("track"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        }),
        base: Some(racing_wheel_schemas::generated::wheel::v1::BaseSettings {
            ffb_gain: profile_json
                .get("base")
                .and_then(|b| b.get("ffb_gain"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.5) as f32,
            dor_deg: profile_json
                .get("base")
                .and_then(|b| b.get("dor_deg"))
                .and_then(|v| v.as_u64())
                .unwrap_or(900) as u32,
            torque_cap_nm: profile_json
                .get("base")
                .and_then(|b| b.get("torque_cap_nm"))
                .and_then(|v| v.as_f64())
                .unwrap_or(10.0) as f32,
            filters: None, // Simplified for now
        }),
        leds: None,
        haptics: None,
        signature: String::new(),
    };

    let mut app_state = state.write().await;

    let client = app_state
        .client
        .as_mut()
        .ok_or_else(|| "Not connected to wheeld service".to_string())?;

    let response = client
        .apply_profile(tonic::Request::new(ApplyProfileRequest {
            device: Some(DeviceId { id: device_id }),
            profile: Some(profile),
        }))
        .await
        .map_err(|e| format!("Failed to apply profile: {}", e))?;

    let result = response.into_inner();

    if result.success {
        Ok(OpResult {
            success: true,
            message: "Profile applied successfully".to_string(),
        })
    } else {
        Ok(OpResult {
            success: false,
            message: result.error_message,
        })
    }
}

/// Trigger emergency stop for a device
///
/// Immediately stops all force feedback output for safety.
/// Requires: 7.5 - WHEN an error occurs, THE Tauri_UI SHALL display a user-friendly error message
#[tauri::command]
pub async fn emergency_stop(
    state: State<'_, Arc<RwLock<AppState>>>,
    device_id: String,
) -> Result<OpResult, String> {
    warn!("Emergency stop triggered for device: {}", device_id);

    let mut app_state = state.write().await;

    let client = app_state
        .client
        .as_mut()
        .ok_or_else(|| "Not connected to wheeld service".to_string())?;

    let response = client
        .emergency_stop(tonic::Request::new(DeviceId { id: device_id }))
        .await
        .map_err(|e| format!("Emergency stop failed: {}", e))?;

    let result = response.into_inner();

    if result.success {
        Ok(OpResult {
            success: true,
            message: "Emergency stop executed".to_string(),
        })
    } else {
        Ok(OpResult {
            success: false,
            message: result.error_message,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_new() {
        let state = AppState::new();
        assert!(!state.connected);
        assert!(state.client.is_none());
        assert!(state.negotiated_features.is_empty());
    }

    #[test]
    fn test_device_info_serialization() -> Result<(), serde_json::Error> {
        let device = DeviceInfo {
            id: "test-device".to_string(),
            name: "Test Wheel".to_string(),
            device_type: "WheelBase".to_string(),
            state: "Connected".to_string(),
            capabilities: DeviceCapabilities {
                supports_pid: true,
                supports_raw_torque_1khz: true,
                supports_health_stream: true,
                supports_led_bus: false,
                max_torque_cnm: 2500,
                encoder_cpr: 65536,
            },
        };

        let json = serde_json::to_string(&device)?;
        let parsed: DeviceInfo = serde_json::from_str(&json)?;

        assert_eq!(parsed.id, device.id);
        assert_eq!(parsed.name, device.name);
        Ok(())
    }

    #[test]
    fn test_service_status_serialization() -> Result<(), serde_json::Error> {
        let status = ServiceStatus {
            connected: true,
            version: "0.1.0".to_string(),
            features: vec!["device_management".to_string()],
            error: None,
        };

        let json = serde_json::to_string(&status)?;
        let parsed: ServiceStatus = serde_json::from_str(&json)?;

        assert_eq!(parsed.connected, status.connected);
        assert_eq!(parsed.version, status.version);
        Ok(())
    }
}
