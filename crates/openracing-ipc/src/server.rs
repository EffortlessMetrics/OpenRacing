//! IPC server implementation

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock, broadcast};
use tracing::{debug, info, warn};

use crate::error::{IpcError, IpcResult};
use crate::handlers::FeatureNegotiationResult;
use crate::transport::{TransportConfig, TransportType};
use crate::{MIN_CLIENT_VERSION, PROTOCOL_VERSION};

/// IPC server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcConfig {
    /// Transport configuration
    pub transport: TransportConfig,
    /// Server name for identification
    pub server_name: String,
    /// Health event broadcast buffer size
    pub health_buffer_size: usize,
    /// Feature negotiation timeout
    pub negotiation_timeout: Duration,
    /// Enable connection logging
    pub enable_connection_logging: bool,
}

impl Default for IpcConfig {
    fn default() -> Self {
        Self {
            transport: TransportConfig::default(),
            server_name: "openracing-ipc".to_string(),
            health_buffer_size: 1000,
            negotiation_timeout: Duration::from_secs(5),
            enable_connection_logging: true,
        }
    }
}

impl IpcConfig {
    /// Create a new configuration with transport
    pub fn with_transport(transport: TransportType) -> Self {
        let mut config = Self::default();
        config.transport.transport = transport;
        config
    }

    /// Set maximum connections
    pub fn max_connections(mut self, max: usize) -> Self {
        self.transport.max_connections = max;
        self
    }

    /// Set health buffer size
    pub fn health_buffer_size(mut self, size: usize) -> Self {
        self.health_buffer_size = size;
        self
    }
}

/// Internal health event for broadcasting
#[derive(Debug, Clone)]
pub struct HealthEvent {
    /// Event timestamp
    pub timestamp: std::time::SystemTime,
    /// Device ID
    pub device_id: String,
    /// Event type
    pub event_type: HealthEventType,
    /// Event message
    pub message: String,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Health event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum HealthEventType {
    /// Device connected
    Connected = 0,
    /// Device disconnected
    Disconnected = 1,
    /// Fault detected
    Fault = 2,
    /// Fault cleared
    FaultCleared = 3,
    /// Temperature warning
    TemperatureWarning = 4,
    /// Temperature critical
    TemperatureCritical = 5,
    /// Profile changed
    ProfileChanged = 6,
    /// High torque enabled
    HighTorqueEnabled = 7,
    /// Emergency stop
    EmergencyStop = 8,
}

/// Connected client information
#[derive(Debug, Clone)]
pub struct ClientInfo {
    /// Client unique identifier
    pub id: String,
    /// Connection timestamp
    pub connected_at: Instant,
    /// Client version
    pub version: String,
    /// Negotiated features
    pub features: Vec<String>,
    /// Peer information (platform-specific)
    pub peer_info: PeerInfo,
}

/// Peer connection information
#[derive(Debug, Clone, Default)]
pub struct PeerInfo {
    /// Process ID (Windows)
    #[cfg(windows)]
    pub process_id: Option<u32>,
    /// User ID (Unix)
    #[cfg(unix)]
    pub uid: Option<u32>,
    /// Group ID (Unix)
    #[cfg(unix)]
    pub gid: Option<u32>,
}

/// IPC server state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerState {
    /// Server is stopped
    Stopped,
    /// Server is starting
    Starting,
    /// Server is running
    Running,
    /// Server is shutting down
    ShuttingDown,
}

/// IPC server implementation
pub struct IpcServer {
    /// Server configuration
    config: IpcConfig,
    /// Health event broadcaster
    health_sender: broadcast::Sender<HealthEvent>,
    /// Connected clients
    connected_clients: Arc<RwLock<HashMap<String, ClientInfo>>>,
    /// Server state
    state: Arc<Mutex<ServerState>>,
    /// Shutdown signal
    shutdown_tx: Arc<Mutex<Option<broadcast::Sender<()>>>>,
}

impl IpcServer {
    /// Create a new IPC server
    pub fn new(config: IpcConfig) -> Self {
        let (health_sender, _) = broadcast::channel(config.health_buffer_size);

        Self {
            config,
            health_sender,
            connected_clients: Arc::new(RwLock::new(HashMap::new())),
            state: Arc::new(Mutex::new(ServerState::Stopped)),
            shutdown_tx: Arc::new(Mutex::new(None)),
        }
    }

    /// Start the IPC server
    pub async fn start(&self) -> IpcResult<()> {
        let mut state = self.state.lock().await;
        if *state != ServerState::Stopped {
            return Err(IpcError::InvalidConfig(
                "Server already running".to_string(),
            ));
        }

        *state = ServerState::Starting;
        drop(state);

        info!(
            "Starting IPC server with transport: {}",
            self.config.transport.transport.description()
        );

        // Set up shutdown channel
        let (shutdown_tx, _) = broadcast::channel(1);
        *self.shutdown_tx.lock().await = Some(shutdown_tx);

        // Update state
        *self.state.lock().await = ServerState::Running;

        info!("IPC server started successfully");
        Ok(())
    }

    /// Stop the IPC server
    pub async fn stop(&self) -> IpcResult<()> {
        let mut state = self.state.lock().await;
        if *state != ServerState::Running {
            return Ok(());
        }

        *state = ServerState::ShuttingDown;
        drop(state);

        info!("Stopping IPC server");

        // Send shutdown signal
        if let Some(tx) = self.shutdown_tx.lock().await.as_ref() {
            let _ = tx.send(());
        }

        // Clear connected clients
        self.connected_clients.write().await.clear();

        // Update state
        *self.state.lock().await = ServerState::Stopped;

        info!("IPC server stopped");
        Ok(())
    }

    /// Get current server state
    pub async fn state(&self) -> ServerState {
        *self.state.lock().await
    }

    /// Check if server is running
    pub async fn is_running(&self) -> bool {
        *self.state.lock().await == ServerState::Running
    }

    /// Get connected client count
    pub async fn client_count(&self) -> usize {
        self.connected_clients.read().await.len()
    }

    /// Get connected clients
    pub async fn connected_clients(&self) -> Vec<ClientInfo> {
        self.connected_clients
            .read()
            .await
            .values()
            .cloned()
            .collect()
    }

    /// Broadcast a health event
    pub fn broadcast_health_event(&self, event: HealthEvent) {
        if let Err(e) = self.health_sender.send(event) {
            warn!("Failed to broadcast health event: {}", e);
        }
    }

    /// Subscribe to health events
    pub fn subscribe_health(&self) -> broadcast::Receiver<HealthEvent> {
        self.health_sender.subscribe()
    }

    /// Handle feature negotiation
    pub async fn negotiate_features(
        &self,
        client_version: &str,
        supported_features: &[String],
    ) -> IpcResult<FeatureNegotiationResult> {
        debug!(
            "Feature negotiation from client version: {}",
            client_version
        );

        let compatible = is_version_compatible(client_version, MIN_CLIENT_VERSION);

        let server_features = vec![
            "device_management".to_string(),
            "profile_management".to_string(),
            "safety_control".to_string(),
            "health_monitoring".to_string(),
            "game_integration".to_string(),
            "streaming_health".to_string(),
            "streaming_devices".to_string(),
        ];

        let enabled_features: Vec<String> = supported_features
            .iter()
            .filter(|f| server_features.contains(f))
            .cloned()
            .collect();

        // Register client if compatible
        if compatible {
            let client_id = format!("client_{}", uuid::Uuid::new_v4());
            let client_info = ClientInfo {
                id: client_id.clone(),
                connected_at: Instant::now(),
                version: client_version.to_string(),
                features: enabled_features.clone(),
                peer_info: PeerInfo::default(),
            };

            self.connected_clients
                .write()
                .await
                .insert(client_info.id.clone(), client_info);

            if self.config.enable_connection_logging {
                info!("Client connected: {}", client_id);
            }
        }

        Ok(FeatureNegotiationResult {
            server_version: PROTOCOL_VERSION.to_string(),
            supported_features: server_features,
            enabled_features,
            compatible,
            min_client_version: MIN_CLIENT_VERSION.to_string(),
        })
    }

    /// Get server configuration
    pub fn config(&self) -> &IpcConfig {
        &self.config
    }

    /// Register a client manually
    pub async fn register_client(&self, client_info: ClientInfo) {
        if self.config.enable_connection_logging {
            info!("Client registered: {}", client_info.id);
        }
        self.connected_clients
            .write()
            .await
            .insert(client_info.id.clone(), client_info);
    }

    /// Unregister a client
    pub async fn unregister_client(&self, client_id: &str) {
        if self.config.enable_connection_logging {
            info!("Client unregistered: {}", client_id);
        }
        self.connected_clients.write().await.remove(client_id);
    }
}

/// Check if client version is compatible with minimum required version
pub fn is_version_compatible(client_version: &str, min_version: &str) -> bool {
    let parse_version = |v: &str| -> Vec<u32> {
        v.split('.')
            .take(3)
            .filter_map(|s| s.parse().ok())
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
        assert!(!is_version_compatible("1.0.0", "1.1.0"));
    }

    #[tokio::test]
    async fn test_server_creation() {
        let config = IpcConfig::default();
        let server = IpcServer::new(config);

        assert_eq!(server.state().await, ServerState::Stopped);
    }

    #[tokio::test]
    async fn test_server_start_stop() -> IpcResult<()> {
        let config = IpcConfig::default();
        let server = IpcServer::new(config);

        server.start().await?;
        assert_eq!(server.state().await, ServerState::Running);

        server.stop().await?;
        assert_eq!(server.state().await, ServerState::Stopped);

        Ok(())
    }

    #[tokio::test]
    async fn test_feature_negotiation() -> IpcResult<()> {
        let config = IpcConfig::default();
        let server = IpcServer::new(config);
        server.start().await?;

        let result = server
            .negotiate_features("1.0.0", &["device_management".to_string()])
            .await?;

        assert!(result.compatible);
        assert!(
            result
                .enabled_features
                .contains(&"device_management".to_string())
        );

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_health_event_broadcast() {
        let config = IpcConfig::default();
        let server = IpcServer::new(config);

        let mut receiver = server.subscribe_health();

        let event = HealthEvent {
            timestamp: std::time::SystemTime::now(),
            device_id: "test-device".to_string(),
            event_type: HealthEventType::Connected,
            message: "Device connected".to_string(),
            metadata: HashMap::new(),
        };

        server.broadcast_health_event(event);

        let received = receiver.try_recv();
        assert!(received.is_ok());
    }

    #[test]
    fn test_ipc_config_builder() {
        let config = IpcConfig::default()
            .max_connections(50)
            .health_buffer_size(500);

        assert_eq!(config.transport.max_connections, 50);
        assert_eq!(config.health_buffer_size, 500);
    }
}
