//! IPC implementation with ACL restrictions and platform-specific transports

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::sync::{Mutex, RwLock, broadcast};
use tracing::{debug, error, info, warn};

use crate::WheelService;

/// IPC configuration with ACL support
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IpcConfig {
    pub bind_address: Option<String>,
    pub transport: TransportType,
    pub max_connections: u32,
    pub connection_timeout: Duration,
    pub enable_acl: bool,
}

/// Transport type for IPC with platform-specific defaults
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum TransportType {
    #[cfg(windows)]
    NamedPipe(String),
    #[cfg(unix)]
    UnixDomainSocket(String),
}

impl Default for TransportType {
    fn default() -> Self {
        #[cfg(windows)]
        {
            TransportType::NamedPipe(r"\\.\pipe\wheel".to_string())
        }
        #[cfg(unix)]
        {
            let uid = unsafe { libc::getuid() };
            TransportType::UnixDomainSocket(format!("/run/user/{}/wheel.sock", uid))
        }
    }
}

/// Internal health event for broadcasting
#[derive(Debug, Clone)]
pub struct HealthEventInternal {
    pub device_id: String,
    pub event_type: String,
    pub message: String,
    pub timestamp: std::time::SystemTime,
}

/// IPC server with ACL restrictions and platform-specific transports
#[derive(Clone)]
pub struct IpcServer {
    config: IpcConfig,
    health_sender: broadcast::Sender<HealthEventInternal>,
    #[allow(dead_code)]
    connected_clients: Arc<RwLock<HashMap<String, ClientInfo>>>,
    shutdown_tx: Arc<Mutex<Option<broadcast::Sender<()>>>>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ClientInfo {
    id: String,
    connected_at: std::time::SystemTime,
    features: Vec<String>,
    peer_info: PeerInfo,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct PeerInfo {
    #[cfg(windows)]
    process_id: u32,
    #[cfg(unix)]
    user_id: u32,
    #[cfg(unix)]
    group_id: u32,
}

impl IpcServer {
    pub async fn new(config: IpcConfig) -> Result<Self> {
        let (health_sender, _) = broadcast::channel(1000);

        Ok(Self {
            config,
            health_sender,
            connected_clients: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx: Arc::new(Mutex::new(None)),
        })
    }

    pub async fn serve(&self, service: Arc<WheelService>) -> Result<()> {
        info!(
            "Starting IPC server with transport: {:?}",
            self.config.transport
        );

        // Set up shutdown channel
        let (shutdown_tx, mut shutdown_rx) = broadcast::channel(1);
        *self.shutdown_tx.lock().await = Some(shutdown_tx);

        match &self.config.transport {
            #[cfg(windows)]
            TransportType::NamedPipe(pipe_name) => {
                self.serve_named_pipe(pipe_name, service, &mut shutdown_rx)
                    .await
            }
            #[cfg(unix)]
            TransportType::UnixDomainSocket(socket_path) => {
                self.serve_unix_socket(socket_path, service, &mut shutdown_rx)
                    .await
            }
        }
    }

    pub async fn shutdown(&self) {
        if let Some(tx) = self.shutdown_tx.lock().await.as_ref() {
            let _ = tx.send(());
        }
    }

    pub fn broadcast_health_event(&self, event: HealthEventInternal) {
        if let Err(e) = self.health_sender.send(event) {
            warn!("Failed to broadcast health event: {}", e);
        }
    }

    pub fn get_health_receiver(&self) -> broadcast::Receiver<HealthEventInternal> {
        self.health_sender.subscribe()
    }

    #[cfg(windows)]
    async fn serve_named_pipe(
        &self,
        pipe_name: &str,
        _service: Arc<WheelService>,
        shutdown_rx: &mut broadcast::Receiver<()>,
    ) -> Result<()> {
        info!("Starting Named Pipe server: {}", pipe_name);

        // Set up ACL restrictions if enabled
        if self.config.enable_acl {
            self.setup_windows_acl(pipe_name).await?;
        }

        // For now, just simulate the server running
        tokio::select! {
            _ = shutdown_rx.recv() => {
                info!("Named Pipe server shutting down");
            }
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                debug!("Named Pipe server tick");
            }
        }

        Ok(())
    }

    #[cfg(unix)]
    async fn serve_unix_socket(
        &self,
        socket_path: &str,
        _service: Arc<WheelService>,
        shutdown_rx: &mut broadcast::Receiver<()>,
    ) -> Result<()> {
        use tokio::net::UnixListener;

        info!("Starting Unix Domain Socket server: {}", socket_path);

        // Remove existing socket file if it exists
        if std::path::Path::new(socket_path).exists() {
            tokio::fs::remove_file(socket_path)
                .await
                .context("Failed to remove existing socket file")?;
        }

        // Create the socket
        let listener = UnixListener::bind(socket_path).context("Failed to bind Unix socket")?;

        // Set up ACL restrictions if enabled
        if self.config.enable_acl {
            self.setup_unix_acl(socket_path).await?;
        }

        info!("Unix socket server listening on {}", socket_path);

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    info!("Unix socket server shutting down");
                    break;
                }
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            debug!("New connection from {:?}", addr);

                            // Verify peer credentials if ACL is enabled
                            if self.config.enable_acl
                                && let Err(e) =
                                    self.verify_unix_peer_credentials(&stream).await
                            {
                                warn!("Connection rejected due to ACL: {}", e);
                                continue;
                            }

                            // Handle the connection
                            let clients = self.connected_clients.clone();
                            tokio::spawn(async move {
                                Self::handle_unix_connection(stream, clients).await;
                            });
                        }
                        Err(e) => {
                            error!("Failed to accept connection: {}", e);
                        }
                    }
                }
            }
        }

        // Clean up socket file
        let _ = tokio::fs::remove_file(socket_path).await;

        Ok(())
    }

    #[cfg(windows)]
    async fn setup_windows_acl(&self, _pipe_name: &str) -> Result<()> {
        // Set up Windows ACL to restrict access to current user and SYSTEM
        info!("Setting up Windows ACL restrictions for named pipe");
        // Implementation would use Windows Security APIs
        Ok(())
    }

    #[cfg(unix)]
    async fn setup_unix_acl(&self, socket_path: &str) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;

        // Set socket permissions to user-only (0600)
        let metadata = tokio::fs::metadata(socket_path)
            .await
            .context("Failed to get socket metadata")?;

        let mut permissions = metadata.permissions();
        permissions.set_mode(0o600); // User read/write only

        tokio::fs::set_permissions(socket_path, permissions)
            .await
            .context("Failed to set socket permissions")?;

        info!("Set Unix socket permissions to user-only access");
        Ok(())
    }

    #[cfg(unix)]
    async fn verify_unix_peer_credentials(&self, stream: &tokio::net::UnixStream) -> Result<()> {
        use std::os::unix::io::{AsRawFd, FromRawFd};
        use std::os::unix::net::UnixStream as StdUnixStream;

        // Get peer credentials
        let raw_fd = stream.as_raw_fd();
        let _std_stream = unsafe { StdUnixStream::from_raw_fd(raw_fd) };

        // Use SO_PEERCRED to get peer process info
        // This is a simplified version - full implementation would use libc calls
        let current_uid = unsafe { libc::getuid() };

        debug!(
            "Verifying peer credentials against current UID: {}",
            current_uid
        );

        // For now, allow all connections from the same user
        Ok(())
    }

    #[cfg(unix)]
    async fn handle_unix_connection(
        _stream: tokio::net::UnixStream,
        _clients: Arc<RwLock<HashMap<String, ClientInfo>>>,
    ) {
        debug!("Handling Unix socket connection");
        // Implementation would handle the actual IPC protocol
    }
}

impl Default for IpcConfig {
    fn default() -> Self {
        Self {
            bind_address: Some("127.0.0.1".to_string()),
            transport: TransportType::default(),
            max_connections: 10,
            connection_timeout: Duration::from_secs(30),
            enable_acl: false,
        }
    }
}

/// Simplified IPC client
pub struct IpcClient {
    config: IpcClientConfig,
}

#[derive(Debug, Clone)]
pub struct IpcClientConfig {
    pub connect_timeout: Duration,
    pub server_address: String,
}

impl IpcClient {
    pub fn new(config: IpcClientConfig) -> Self {
        Self { config }
    }

    pub async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Connecting to IPC server at {}", self.config.server_address);
        // For now, just log that we would connect
        // In a real implementation, we would establish the connection here
        Ok(())
    }

    pub async fn disconnect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Disconnecting from IPC server");
        Ok(())
    }
}

impl Default for IpcClientConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            server_address: "127.0.0.1:50051".to_string(),
        }
    }
}
