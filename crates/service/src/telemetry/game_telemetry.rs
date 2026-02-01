//! Game telemetry types and adapter trait
//!
//! Defines the common telemetry format and adapter interface for racing games.
//! Requirements: 12.1-12.4, 12.6 (Game Telemetry Adapters, Disconnection Handling)

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::mpsc;

/// Common telemetry data from racing games
///
/// This struct provides a normalized view of telemetry data that all game
/// adapters convert their game-specific data into. This allows the FFB engine
/// to work with a consistent data format regardless of the source game.
///
/// # Fields
/// All fields represent real-time vehicle state:
/// - Position and motion: speed, steering angle, G-forces
/// - Engine state: RPM, gear, throttle, brake
/// - Tire slip angles for each wheel (FL, FR, RL, RR)
///
/// # Example
/// ```
/// use racing_wheel_service::telemetry::GameTelemetry;
/// use std::time::Instant;
///
/// let telemetry = GameTelemetry {
///     timestamp: Instant::now(),
///     speed_mps: 45.0,
///     rpm: 6500.0,
///     gear: 4,
///     steering_angle: 0.15,
///     throttle: 0.8,
///     brake: 0.0,
///     lateral_g: 0.5,
///     longitudinal_g: 0.2,
///     slip_angle_fl: 0.02,
///     slip_angle_fr: 0.02,
///     slip_angle_rl: 0.03,
///     slip_angle_rr: 0.03,
/// };
/// ```
#[derive(Clone, Debug)]
pub struct GameTelemetry {
    /// Timestamp when this telemetry sample was captured (monotonic)
    pub timestamp: Instant,

    /// Vehicle speed in meters per second
    pub speed_mps: f32,

    /// Engine RPM (revolutions per minute)
    pub rpm: f32,

    /// Current gear (-1 = reverse, 0 = neutral, 1+ = forward gears)
    pub gear: i8,

    /// Steering wheel angle in radians (positive = right, negative = left)
    pub steering_angle: f32,

    /// Throttle position (0.0 = released, 1.0 = fully pressed)
    pub throttle: f32,

    /// Brake position (0.0 = released, 1.0 = fully pressed)
    pub brake: f32,

    /// Lateral acceleration in G-forces (positive = right)
    pub lateral_g: f32,

    /// Longitudinal acceleration in G-forces (positive = forward/acceleration)
    pub longitudinal_g: f32,

    /// Front-left tire slip angle in radians
    pub slip_angle_fl: f32,

    /// Front-right tire slip angle in radians
    pub slip_angle_fr: f32,

    /// Rear-left tire slip angle in radians
    pub slip_angle_rl: f32,

    /// Rear-right tire slip angle in radians
    pub slip_angle_rr: f32,
}

impl Default for GameTelemetry {
    fn default() -> Self {
        Self {
            timestamp: Instant::now(),
            speed_mps: 0.0,
            rpm: 0.0,
            gear: 0,
            steering_angle: 0.0,
            throttle: 0.0,
            brake: 0.0,
            lateral_g: 0.0,
            longitudinal_g: 0.0,
            slip_angle_fl: 0.0,
            slip_angle_fr: 0.0,
            slip_angle_rl: 0.0,
            slip_angle_rr: 0.0,
        }
    }
}

impl GameTelemetry {
    /// Create a new GameTelemetry with the current timestamp
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a GameTelemetry with a specific timestamp
    pub fn with_timestamp(timestamp: Instant) -> Self {
        Self {
            timestamp,
            ..Default::default()
        }
    }

    /// Get speed in km/h
    pub fn speed_kmh(&self) -> f32 {
        self.speed_mps * 3.6
    }

    /// Get speed in mph
    pub fn speed_mph(&self) -> f32 {
        self.speed_mps * 2.237
    }

    /// Get the average slip angle across all tires
    pub fn average_slip_angle(&self) -> f32 {
        (self.slip_angle_fl + self.slip_angle_fr + self.slip_angle_rl + self.slip_angle_rr) / 4.0
    }

    /// Get the front axle average slip angle
    pub fn front_slip_angle(&self) -> f32 {
        (self.slip_angle_fl + self.slip_angle_fr) / 2.0
    }

    /// Get the rear axle average slip angle
    pub fn rear_slip_angle(&self) -> f32 {
        (self.slip_angle_rl + self.slip_angle_rr) / 2.0
    }

    /// Check if the vehicle is stationary (speed below threshold)
    pub fn is_stationary(&self) -> bool {
        self.speed_mps < 0.5
    }

    /// Get total G-force magnitude
    pub fn total_g(&self) -> f32 {
        (self.lateral_g * self.lateral_g + self.longitudinal_g * self.longitudinal_g).sqrt()
    }
}

/// Serializable version of GameTelemetry for recording/replay
///
/// Since `Instant` cannot be serialized, this struct uses a relative
/// timestamp in nanoseconds from an epoch for serialization purposes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameTelemetrySnapshot {
    /// Relative timestamp in nanoseconds
    pub timestamp_ns: u64,

    /// Vehicle speed in meters per second
    pub speed_mps: f32,

    /// Engine RPM
    pub rpm: f32,

    /// Current gear
    pub gear: i8,

    /// Steering wheel angle in radians
    pub steering_angle: f32,

    /// Throttle position (0.0-1.0)
    pub throttle: f32,

    /// Brake position (0.0-1.0)
    pub brake: f32,

    /// Lateral G-force
    pub lateral_g: f32,

    /// Longitudinal G-force
    pub longitudinal_g: f32,

    /// Front-left slip angle
    pub slip_angle_fl: f32,

    /// Front-right slip angle
    pub slip_angle_fr: f32,

    /// Rear-left slip angle
    pub slip_angle_rl: f32,

    /// Rear-right slip angle
    pub slip_angle_rr: f32,
}

impl GameTelemetrySnapshot {
    /// Convert from GameTelemetry using a reference epoch
    pub fn from_telemetry(telemetry: &GameTelemetry, epoch: Instant) -> Self {
        let timestamp_ns = telemetry
            .timestamp
            .saturating_duration_since(epoch)
            .as_nanos() as u64;

        Self {
            timestamp_ns,
            speed_mps: telemetry.speed_mps,
            rpm: telemetry.rpm,
            gear: telemetry.gear,
            steering_angle: telemetry.steering_angle,
            throttle: telemetry.throttle,
            brake: telemetry.brake,
            lateral_g: telemetry.lateral_g,
            longitudinal_g: telemetry.longitudinal_g,
            slip_angle_fl: telemetry.slip_angle_fl,
            slip_angle_fr: telemetry.slip_angle_fr,
            slip_angle_rl: telemetry.slip_angle_rl,
            slip_angle_rr: telemetry.slip_angle_rr,
        }
    }
}

/// Errors that can occur during telemetry operations
#[derive(Error, Debug)]
pub enum TelemetryError {
    /// Failed to connect to the game's telemetry source
    #[error("Failed to connect to telemetry source: {0}")]
    ConnectionFailed(String),

    /// The game is not currently running
    #[error("Game is not running: {game_id}")]
    GameNotRunning { game_id: String },

    /// Telemetry data parsing failed
    #[error("Failed to parse telemetry data: {0}")]
    ParseError(String),

    /// Shared memory access failed (for games using shared memory)
    #[error("Shared memory error: {0}")]
    SharedMemoryError(String),

    /// Network/UDP error (for games using network telemetry)
    #[error("Network error: {0}")]
    NetworkError(String),

    /// The adapter is already connected
    #[error("Adapter already connected")]
    AlreadyConnected,

    /// The adapter is not connected
    #[error("Adapter not connected")]
    NotConnected,

    /// Timeout waiting for telemetry data
    #[error("Telemetry timeout after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },

    /// Invalid telemetry data received
    #[error("Invalid telemetry data: {reason}")]
    InvalidData { reason: String },

    /// Generic I/O error
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Connection state for telemetry adapters
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ConnectionState {
    /// Not connected to the game
    #[default]
    Disconnected,
    /// Currently attempting to connect
    Connecting,
    /// Successfully connected and receiving telemetry
    Connected,
    /// Connection lost, may attempt reconnection
    Reconnecting,
    /// Adapter has encountered an error
    Error,
}

impl ConnectionState {
    /// Check if the adapter is in a connected state (receiving data)
    pub fn is_connected(&self) -> bool {
        matches!(self, ConnectionState::Connected)
    }

    /// Check if the adapter is in a disconnected or error state
    pub fn is_disconnected(&self) -> bool {
        matches!(self, ConnectionState::Disconnected | ConnectionState::Error)
    }

    /// Check if the adapter is attempting to connect or reconnect
    pub fn is_transitioning(&self) -> bool {
        matches!(
            self,
            ConnectionState::Connecting | ConnectionState::Reconnecting
        )
    }
}

/// Event emitted when connection state changes
///
/// This event is used to notify the FFB engine and other components
/// when the telemetry connection state changes, particularly for
/// disconnection events that require entering a safe state.
///
/// Requirements: 12.6 (Graceful disconnection handling)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStateEvent {
    /// The game adapter that generated this event
    pub game_id: String,
    /// Previous connection state
    pub previous_state: ConnectionState,
    /// New connection state
    pub new_state: ConnectionState,
    /// Timestamp when the state change occurred (as duration since UNIX epoch)
    pub timestamp_ns: u64,
    /// Optional reason for the state change
    pub reason: Option<String>,
}

impl ConnectionStateEvent {
    /// Create a new connection state event
    pub fn new(
        game_id: impl Into<String>,
        previous_state: ConnectionState,
        new_state: ConnectionState,
        reason: Option<String>,
    ) -> Self {
        let timestamp_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);

        Self {
            game_id: game_id.into(),
            previous_state,
            new_state,
            timestamp_ns,
            reason,
        }
    }

    /// Check if this event represents a disconnection
    pub fn is_disconnection(&self) -> bool {
        self.previous_state.is_connected() && self.new_state.is_disconnected()
    }

    /// Check if this event represents a successful connection
    pub fn is_connection(&self) -> bool {
        !self.previous_state.is_connected() && self.new_state.is_connected()
    }
}

/// Receiver for connection state change events
pub type ConnectionStateReceiver = mpsc::Receiver<ConnectionStateEvent>;

/// Sender for connection state change events
pub type ConnectionStateSender = mpsc::Sender<ConnectionStateEvent>;

/// Default timeout for detecting disconnection (in milliseconds)
///
/// If no telemetry data is received within this timeout, the adapter
/// should transition to the Disconnected state.
pub const DEFAULT_DISCONNECTION_TIMEOUT_MS: u64 = 2000;

/// Configuration for disconnection detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisconnectionConfig {
    /// Timeout in milliseconds before considering the connection lost
    pub timeout_ms: u64,
    /// Whether to attempt automatic reconnection
    pub auto_reconnect: bool,
    /// Maximum number of reconnection attempts (0 = unlimited)
    pub max_reconnect_attempts: u32,
    /// Delay between reconnection attempts in milliseconds
    pub reconnect_delay_ms: u64,
}

impl Default for DisconnectionConfig {
    fn default() -> Self {
        Self {
            timeout_ms: DEFAULT_DISCONNECTION_TIMEOUT_MS,
            auto_reconnect: true,
            max_reconnect_attempts: 0, // Unlimited
            reconnect_delay_ms: 1000,
        }
    }
}

impl DisconnectionConfig {
    /// Create a new disconnection config with custom timeout
    pub fn with_timeout(timeout_ms: u64) -> Self {
        Self {
            timeout_ms,
            ..Default::default()
        }
    }

    /// Get the timeout as a Duration
    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }

    /// Get the reconnect delay as a Duration
    pub fn reconnect_delay(&self) -> Duration {
        Duration::from_millis(self.reconnect_delay_ms)
    }
}

/// Tracks the last telemetry update time for disconnection detection
#[derive(Debug)]
pub struct DisconnectionTracker {
    /// Configuration for disconnection detection
    config: DisconnectionConfig,
    /// Last time telemetry data was received
    last_data_time: Option<Instant>,
    /// Current connection state
    state: ConnectionState,
    /// Number of reconnection attempts made
    reconnect_attempts: u32,
    /// Sender for state change notifications
    state_sender: Option<ConnectionStateSender>,
    /// Game ID for event generation
    game_id: String,
}

impl DisconnectionTracker {
    /// Create a new disconnection tracker
    pub fn new(game_id: impl Into<String>, config: DisconnectionConfig) -> Self {
        Self {
            config,
            last_data_time: None,
            state: ConnectionState::Disconnected,
            reconnect_attempts: 0,
            state_sender: None,
            game_id: game_id.into(),
        }
    }

    /// Create a tracker with default configuration
    pub fn with_defaults(game_id: impl Into<String>) -> Self {
        Self::new(game_id, DisconnectionConfig::default())
    }

    /// Set the state change notification sender
    pub fn set_state_sender(&mut self, sender: ConnectionStateSender) {
        self.state_sender = Some(sender);
    }

    /// Subscribe to state change events
    ///
    /// Returns a receiver that will receive connection state change events.
    /// The channel has a buffer of 16 events.
    pub fn subscribe(&mut self) -> ConnectionStateReceiver {
        let (tx, rx) = mpsc::channel(16);
        self.state_sender = Some(tx);
        rx
    }

    /// Record that telemetry data was received
    ///
    /// This should be called whenever valid telemetry data is received
    /// to reset the disconnection timeout.
    pub fn record_data_received(&mut self) {
        self.last_data_time = Some(Instant::now());

        // If we were disconnected or reconnecting, transition to connected
        if self.state != ConnectionState::Connected {
            self.transition_to(
                ConnectionState::Connected,
                Some("Data received".to_string()),
            );
            self.reconnect_attempts = 0;
        }
    }

    /// Check if the connection has timed out
    ///
    /// Returns true if no data has been received within the configured timeout.
    pub fn is_timed_out(&self) -> bool {
        match self.last_data_time {
            Some(last_time) => last_time.elapsed() > self.config.timeout(),
            None => false, // Never received data, not a timeout
        }
    }

    /// Check for disconnection and update state if necessary
    ///
    /// This should be called periodically (e.g., in the polling loop)
    /// to detect disconnection based on timeout.
    ///
    /// Returns the current connection state after the check.
    pub fn check_disconnection(&mut self) -> ConnectionState {
        if self.state == ConnectionState::Connected && self.is_timed_out() {
            self.transition_to(
                ConnectionState::Disconnected,
                Some(format!("No data received for {}ms", self.config.timeout_ms)),
            );
        }
        self.state
    }

    /// Get the current connection state
    pub fn state(&self) -> ConnectionState {
        self.state
    }

    /// Manually set the connection state
    ///
    /// This can be used when the adapter detects disconnection through
    /// other means (e.g., shared memory becoming unavailable).
    pub fn set_state(&mut self, new_state: ConnectionState, reason: Option<String>) {
        self.transition_to(new_state, reason);
    }

    /// Mark the adapter as connecting
    pub fn mark_connecting(&mut self) {
        self.transition_to(ConnectionState::Connecting, Some("Connecting".to_string()));
    }

    /// Mark the adapter as reconnecting
    pub fn mark_reconnecting(&mut self) {
        self.reconnect_attempts += 1;
        self.transition_to(
            ConnectionState::Reconnecting,
            Some(format!("Reconnection attempt {}", self.reconnect_attempts)),
        );
    }

    /// Mark the adapter as having an error
    pub fn mark_error(&mut self, reason: String) {
        self.transition_to(ConnectionState::Error, Some(reason));
    }

    /// Check if reconnection should be attempted
    pub fn should_reconnect(&self) -> bool {
        if !self.config.auto_reconnect {
            return false;
        }

        if self.config.max_reconnect_attempts > 0
            && self.reconnect_attempts >= self.config.max_reconnect_attempts
        {
            return false;
        }

        self.state.is_disconnected()
    }

    /// Get the number of reconnection attempts made
    pub fn reconnect_attempts(&self) -> u32 {
        self.reconnect_attempts
    }

    /// Reset the reconnection attempt counter
    pub fn reset_reconnect_attempts(&mut self) {
        self.reconnect_attempts = 0;
    }

    /// Get the time since last data was received
    pub fn time_since_last_data(&self) -> Option<Duration> {
        self.last_data_time.map(|t| t.elapsed())
    }

    /// Transition to a new state and send notification
    fn transition_to(&mut self, new_state: ConnectionState, reason: Option<String>) {
        if self.state == new_state {
            return;
        }

        let previous_state = self.state;
        self.state = new_state;

        // Send state change notification if sender is configured
        if let Some(sender) = &self.state_sender {
            let event =
                ConnectionStateEvent::new(self.game_id.clone(), previous_state, new_state, reason);

            // Use try_send to avoid blocking - if the channel is full, drop the event
            // This is acceptable because state changes are relatively infrequent
            let _ = sender.try_send(event);
        }
    }
}

/// Telemetry adapter trait for game-specific telemetry sources
///
/// This trait defines the interface that all game telemetry adapters must implement.
/// Each adapter handles the game-specific protocol (shared memory, UDP, plugin API)
/// and converts the data to the common `GameTelemetry` format.
///
/// # Implementation Notes
/// - `connect()` should be idempotent - calling it when already connected should succeed
/// - `disconnect()` should be safe to call even when not connected
/// - `poll()` must be non-blocking and suitable for use in the RT path
/// - Adapters should handle reconnection gracefully when games restart
/// - Adapters should detect disconnection and notify via state change events
///
/// # Disconnection Handling (Requirements: 12.6)
/// Adapters must detect when the game disconnects and:
/// 1. Transition to `ConnectionState::Disconnected`
/// 2. Notify subscribers via the state change channel
/// 3. Allow the FFB engine to enter a safe state
///
/// # Example Implementation
/// ```ignore
/// struct MyGameAdapter {
///     state: ConnectionState,
///     last_telemetry: Option<GameTelemetry>,
///     disconnection_tracker: DisconnectionTracker,
/// }
///
/// #[async_trait]
/// impl GameTelemetryAdapter for MyGameAdapter {
///     async fn connect(&mut self) -> Result<(), TelemetryError> {
///         // Connect to game's telemetry source
///         self.disconnection_tracker.mark_connecting();
///         // ... connection logic ...
///         self.disconnection_tracker.record_data_received();
///         Ok(())
///     }
///
///     async fn disconnect(&mut self) -> Result<(), TelemetryError> {
///         self.disconnection_tracker.set_state(
///             ConnectionState::Disconnected,
///             Some("Manual disconnect".to_string())
///         );
///         Ok(())
///     }
///
///     fn poll(&mut self) -> Option<GameTelemetry> {
///         // Check for disconnection timeout
///         self.disconnection_tracker.check_disconnection();
///         
///         // Return cached telemetry if available
///         if let Some(data) = self.read_data() {
///             self.disconnection_tracker.record_data_received();
///             return Some(data);
///         }
///         None
///     }
///
///     fn game_id(&self) -> &str {
///         "my_game"
///     }
///
///     fn subscribe_state_changes(&mut self) -> Option<ConnectionStateReceiver> {
///         Some(self.disconnection_tracker.subscribe())
///     }
/// }
/// ```
#[async_trait]
pub trait GameTelemetryAdapter: Send + Sync {
    /// Start receiving telemetry from the game
    ///
    /// This method establishes a connection to the game's telemetry source.
    /// For shared memory games (iRacing, AMS2), this opens the memory-mapped file.
    /// For UDP games (ACC), this binds to the appropriate port.
    ///
    /// # Errors
    /// - `TelemetryError::GameNotRunning` if the game is not running
    /// - `TelemetryError::ConnectionFailed` if connection cannot be established
    /// - `TelemetryError::AlreadyConnected` if already connected (optional)
    async fn connect(&mut self) -> Result<(), TelemetryError>;

    /// Disconnect from the game's telemetry source
    ///
    /// This method cleanly disconnects from the telemetry source and releases
    /// any resources (file handles, sockets, etc.).
    ///
    /// # Errors
    /// - `TelemetryError::NotConnected` if not currently connected (optional)
    async fn disconnect(&mut self) -> Result<(), TelemetryError>;

    /// Poll for the latest telemetry data (non-blocking)
    ///
    /// This method returns the most recent telemetry data if available.
    /// It must be non-blocking and suitable for use in the RT path.
    ///
    /// Implementations should also check for disconnection timeout and
    /// update the connection state accordingly.
    ///
    /// # Returns
    /// - `Some(GameTelemetry)` if new telemetry data is available
    /// - `None` if no new data is available or not connected
    fn poll(&mut self) -> Option<GameTelemetry>;

    /// Get the game identifier for this adapter
    ///
    /// Returns a unique string identifier for the game this adapter supports.
    /// Examples: "iracing", "acc", "ams2", "rfactor2"
    fn game_id(&self) -> &str;

    /// Get the current connection state
    ///
    /// Returns the current state of the adapter's connection to the game.
    fn connection_state(&self) -> ConnectionState {
        ConnectionState::Disconnected
    }

    /// Check if the game is currently running
    ///
    /// This is a quick check to determine if the game process is running,
    /// without necessarily establishing a full telemetry connection.
    async fn is_game_running(&self) -> bool {
        false
    }

    /// Subscribe to connection state change events
    ///
    /// Returns a receiver that will receive `ConnectionStateEvent` notifications
    /// when the connection state changes. This is particularly important for
    /// detecting disconnection events so the FFB engine can enter a safe state.
    ///
    /// Requirements: 12.6 (Graceful disconnection handling and FFB engine notification)
    ///
    /// # Returns
    /// - `Some(ConnectionStateReceiver)` if the adapter supports state notifications
    /// - `None` if state notifications are not supported
    fn subscribe_state_changes(&mut self) -> Option<ConnectionStateReceiver> {
        None
    }

    /// Get the disconnection configuration
    ///
    /// Returns the configuration used for disconnection detection,
    /// including timeout and reconnection settings.
    fn disconnection_config(&self) -> DisconnectionConfig {
        DisconnectionConfig::default()
    }

    /// Set the disconnection configuration
    ///
    /// Allows customizing the disconnection detection behavior.
    fn set_disconnection_config(&mut self, _config: DisconnectionConfig) {
        // Default implementation does nothing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn test_game_telemetry_default() -> TestResult {
        let telemetry = GameTelemetry::default();

        assert_eq!(telemetry.speed_mps, 0.0);
        assert_eq!(telemetry.rpm, 0.0);
        assert_eq!(telemetry.gear, 0);
        assert_eq!(telemetry.steering_angle, 0.0);
        assert_eq!(telemetry.throttle, 0.0);
        assert_eq!(telemetry.brake, 0.0);
        assert_eq!(telemetry.lateral_g, 0.0);
        assert_eq!(telemetry.longitudinal_g, 0.0);
        assert_eq!(telemetry.slip_angle_fl, 0.0);
        assert_eq!(telemetry.slip_angle_fr, 0.0);
        assert_eq!(telemetry.slip_angle_rl, 0.0);
        assert_eq!(telemetry.slip_angle_rr, 0.0);
        Ok(())
    }

    #[test]
    fn test_game_telemetry_new() -> TestResult {
        let before = Instant::now();
        let telemetry = GameTelemetry::new();
        let after = Instant::now();

        // Timestamp should be between before and after
        assert!(telemetry.timestamp >= before);
        assert!(telemetry.timestamp <= after);
        Ok(())
    }

    #[test]
    fn test_game_telemetry_with_timestamp() -> TestResult {
        let timestamp = Instant::now();
        let telemetry = GameTelemetry::with_timestamp(timestamp);

        assert_eq!(telemetry.timestamp, timestamp);
        assert_eq!(telemetry.speed_mps, 0.0);
        Ok(())
    }

    #[test]
    fn test_speed_conversions() -> TestResult {
        let mut telemetry = GameTelemetry::default();
        telemetry.speed_mps = 27.78; // ~100 km/h

        let speed_kmh = telemetry.speed_kmh();
        let speed_mph = telemetry.speed_mph();

        assert!((speed_kmh - 100.0).abs() < 0.1);
        assert!((speed_mph - 62.14).abs() < 0.1);
        Ok(())
    }

    #[test]
    fn test_slip_angle_calculations() -> TestResult {
        let mut telemetry = GameTelemetry::default();
        telemetry.slip_angle_fl = 0.02;
        telemetry.slip_angle_fr = 0.04;
        telemetry.slip_angle_rl = 0.06;
        telemetry.slip_angle_rr = 0.08;

        let avg = telemetry.average_slip_angle();
        let front = telemetry.front_slip_angle();
        let rear = telemetry.rear_slip_angle();

        assert!((avg - 0.05).abs() < 0.001);
        assert!((front - 0.03).abs() < 0.001);
        assert!((rear - 0.07).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_is_stationary() -> TestResult {
        let mut telemetry = GameTelemetry::default();

        // Zero speed should be stationary
        assert!(telemetry.is_stationary());

        // Below threshold should be stationary
        telemetry.speed_mps = 0.4;
        assert!(telemetry.is_stationary());

        // At threshold should not be stationary
        telemetry.speed_mps = 0.5;
        assert!(!telemetry.is_stationary());

        // Above threshold should not be stationary
        telemetry.speed_mps = 10.0;
        assert!(!telemetry.is_stationary());
        Ok(())
    }

    #[test]
    fn test_total_g() -> TestResult {
        let mut telemetry = GameTelemetry::default();
        telemetry.lateral_g = 3.0;
        telemetry.longitudinal_g = 4.0;

        let total = telemetry.total_g();
        assert!((total - 5.0).abs() < 0.001); // 3-4-5 triangle
        Ok(())
    }

    #[test]
    fn test_game_telemetry_snapshot_conversion() -> TestResult {
        let epoch = Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(10));

        let mut telemetry = GameTelemetry::new();
        telemetry.speed_mps = 50.0;
        telemetry.rpm = 6000.0;
        telemetry.gear = 4;
        telemetry.steering_angle = 0.1;
        telemetry.throttle = 0.8;
        telemetry.brake = 0.0;
        telemetry.lateral_g = 0.5;
        telemetry.longitudinal_g = 0.2;
        telemetry.slip_angle_fl = 0.01;
        telemetry.slip_angle_fr = 0.02;
        telemetry.slip_angle_rl = 0.03;
        telemetry.slip_angle_rr = 0.04;

        let snapshot = GameTelemetrySnapshot::from_telemetry(&telemetry, epoch);

        // Timestamp should be positive (after epoch)
        assert!(snapshot.timestamp_ns > 0);

        // All other fields should match
        assert_eq!(snapshot.speed_mps, 50.0);
        assert_eq!(snapshot.rpm, 6000.0);
        assert_eq!(snapshot.gear, 4);
        assert_eq!(snapshot.steering_angle, 0.1);
        assert_eq!(snapshot.throttle, 0.8);
        assert_eq!(snapshot.brake, 0.0);
        assert_eq!(snapshot.lateral_g, 0.5);
        assert_eq!(snapshot.longitudinal_g, 0.2);
        assert_eq!(snapshot.slip_angle_fl, 0.01);
        assert_eq!(snapshot.slip_angle_fr, 0.02);
        assert_eq!(snapshot.slip_angle_rl, 0.03);
        assert_eq!(snapshot.slip_angle_rr, 0.04);
        Ok(())
    }

    #[test]
    fn test_telemetry_error_display() -> TestResult {
        let err = TelemetryError::ConnectionFailed("test error".to_string());
        assert!(err.to_string().contains("test error"));

        let err = TelemetryError::GameNotRunning {
            game_id: "iracing".to_string(),
        };
        assert!(err.to_string().contains("iracing"));

        let err = TelemetryError::Timeout { timeout_ms: 1000 };
        assert!(err.to_string().contains("1000"));
        Ok(())
    }

    #[test]
    fn test_connection_state_default() -> TestResult {
        let state = ConnectionState::default();
        assert_eq!(state, ConnectionState::Disconnected);
        Ok(())
    }

    #[test]
    fn test_game_telemetry_snapshot_serialization() -> TestResult {
        let snapshot = GameTelemetrySnapshot {
            timestamp_ns: 1000000,
            speed_mps: 50.0,
            rpm: 6000.0,
            gear: 4,
            steering_angle: 0.1,
            throttle: 0.8,
            brake: 0.0,
            lateral_g: 0.5,
            longitudinal_g: 0.2,
            slip_angle_fl: 0.01,
            slip_angle_fr: 0.02,
            slip_angle_rl: 0.03,
            slip_angle_rr: 0.04,
        };

        // Test JSON serialization round-trip
        let json = serde_json::to_string(&snapshot)?;
        let deserialized: GameTelemetrySnapshot = serde_json::from_str(&json)?;

        assert_eq!(deserialized.timestamp_ns, snapshot.timestamp_ns);
        assert_eq!(deserialized.speed_mps, snapshot.speed_mps);
        assert_eq!(deserialized.rpm, snapshot.rpm);
        assert_eq!(deserialized.gear, snapshot.gear);
        Ok(())
    }

    // Tests for ConnectionState
    #[test]
    fn test_connection_state_is_connected() -> TestResult {
        assert!(ConnectionState::Connected.is_connected());
        assert!(!ConnectionState::Disconnected.is_connected());
        assert!(!ConnectionState::Connecting.is_connected());
        assert!(!ConnectionState::Reconnecting.is_connected());
        assert!(!ConnectionState::Error.is_connected());
        Ok(())
    }

    #[test]
    fn test_connection_state_is_disconnected() -> TestResult {
        assert!(ConnectionState::Disconnected.is_disconnected());
        assert!(ConnectionState::Error.is_disconnected());
        assert!(!ConnectionState::Connected.is_disconnected());
        assert!(!ConnectionState::Connecting.is_disconnected());
        assert!(!ConnectionState::Reconnecting.is_disconnected());
        Ok(())
    }

    #[test]
    fn test_connection_state_is_transitioning() -> TestResult {
        assert!(ConnectionState::Connecting.is_transitioning());
        assert!(ConnectionState::Reconnecting.is_transitioning());
        assert!(!ConnectionState::Connected.is_transitioning());
        assert!(!ConnectionState::Disconnected.is_transitioning());
        assert!(!ConnectionState::Error.is_transitioning());
        Ok(())
    }

    // Tests for ConnectionStateEvent
    #[test]
    fn test_connection_state_event_creation() -> TestResult {
        let event = ConnectionStateEvent::new(
            "test_game",
            ConnectionState::Connected,
            ConnectionState::Disconnected,
            Some("Test reason".to_string()),
        );

        assert_eq!(event.game_id, "test_game");
        assert_eq!(event.previous_state, ConnectionState::Connected);
        assert_eq!(event.new_state, ConnectionState::Disconnected);
        assert_eq!(event.reason, Some("Test reason".to_string()));
        assert!(event.timestamp_ns > 0);
        Ok(())
    }

    #[test]
    fn test_connection_state_event_is_disconnection() -> TestResult {
        let disconnection_event = ConnectionStateEvent::new(
            "test_game",
            ConnectionState::Connected,
            ConnectionState::Disconnected,
            None,
        );
        assert!(disconnection_event.is_disconnection());

        let connection_event = ConnectionStateEvent::new(
            "test_game",
            ConnectionState::Disconnected,
            ConnectionState::Connected,
            None,
        );
        assert!(!connection_event.is_disconnection());

        let error_event = ConnectionStateEvent::new(
            "test_game",
            ConnectionState::Connected,
            ConnectionState::Error,
            None,
        );
        assert!(error_event.is_disconnection());
        Ok(())
    }

    #[test]
    fn test_connection_state_event_is_connection() -> TestResult {
        let connection_event = ConnectionStateEvent::new(
            "test_game",
            ConnectionState::Disconnected,
            ConnectionState::Connected,
            None,
        );
        assert!(connection_event.is_connection());

        let reconnection_event = ConnectionStateEvent::new(
            "test_game",
            ConnectionState::Reconnecting,
            ConnectionState::Connected,
            None,
        );
        assert!(reconnection_event.is_connection());

        let disconnection_event = ConnectionStateEvent::new(
            "test_game",
            ConnectionState::Connected,
            ConnectionState::Disconnected,
            None,
        );
        assert!(!disconnection_event.is_connection());
        Ok(())
    }

    // Tests for DisconnectionConfig
    #[test]
    fn test_disconnection_config_default() -> TestResult {
        let config = DisconnectionConfig::default();

        assert_eq!(config.timeout_ms, DEFAULT_DISCONNECTION_TIMEOUT_MS);
        assert!(config.auto_reconnect);
        assert_eq!(config.max_reconnect_attempts, 0);
        assert_eq!(config.reconnect_delay_ms, 1000);
        Ok(())
    }

    #[test]
    fn test_disconnection_config_with_timeout() -> TestResult {
        let config = DisconnectionConfig::with_timeout(5000);

        assert_eq!(config.timeout_ms, 5000);
        assert_eq!(config.timeout(), Duration::from_millis(5000));
        Ok(())
    }

    #[test]
    fn test_disconnection_config_durations() -> TestResult {
        let config = DisconnectionConfig {
            timeout_ms: 3000,
            reconnect_delay_ms: 500,
            ..Default::default()
        };

        assert_eq!(config.timeout(), Duration::from_millis(3000));
        assert_eq!(config.reconnect_delay(), Duration::from_millis(500));
        Ok(())
    }

    // Tests for DisconnectionTracker
    #[test]
    fn test_disconnection_tracker_creation() -> TestResult {
        let tracker = DisconnectionTracker::with_defaults("test_game");

        assert_eq!(tracker.state(), ConnectionState::Disconnected);
        assert_eq!(tracker.reconnect_attempts(), 0);
        assert!(tracker.time_since_last_data().is_none());
        Ok(())
    }

    #[test]
    fn test_disconnection_tracker_record_data() -> TestResult {
        let mut tracker = DisconnectionTracker::with_defaults("test_game");

        // Initially disconnected
        assert_eq!(tracker.state(), ConnectionState::Disconnected);

        // Record data received - should transition to connected
        tracker.record_data_received();
        assert_eq!(tracker.state(), ConnectionState::Connected);
        assert!(tracker.time_since_last_data().is_some());
        Ok(())
    }

    #[test]
    fn test_disconnection_tracker_timeout_detection() -> TestResult {
        let config = DisconnectionConfig::with_timeout(10); // 10ms timeout for testing
        let mut tracker = DisconnectionTracker::new("test_game", config);

        // Record data and become connected
        tracker.record_data_received();
        assert_eq!(tracker.state(), ConnectionState::Connected);
        assert!(!tracker.is_timed_out());

        // Wait for timeout
        std::thread::sleep(Duration::from_millis(20));

        // Should now be timed out
        assert!(tracker.is_timed_out());

        // Check disconnection should transition state
        let state = tracker.check_disconnection();
        assert_eq!(state, ConnectionState::Disconnected);
        assert_eq!(tracker.state(), ConnectionState::Disconnected);
        Ok(())
    }

    #[test]
    fn test_disconnection_tracker_state_transitions() -> TestResult {
        let mut tracker = DisconnectionTracker::with_defaults("test_game");

        // Mark connecting
        tracker.mark_connecting();
        assert_eq!(tracker.state(), ConnectionState::Connecting);

        // Record data - should become connected
        tracker.record_data_received();
        assert_eq!(tracker.state(), ConnectionState::Connected);

        // Mark error
        tracker.mark_error("Test error".to_string());
        assert_eq!(tracker.state(), ConnectionState::Error);

        // Mark reconnecting
        tracker.mark_reconnecting();
        assert_eq!(tracker.state(), ConnectionState::Reconnecting);
        assert_eq!(tracker.reconnect_attempts(), 1);

        // Mark reconnecting again
        tracker.mark_reconnecting();
        assert_eq!(tracker.reconnect_attempts(), 2);
        Ok(())
    }

    #[test]
    fn test_disconnection_tracker_should_reconnect() -> TestResult {
        let config = DisconnectionConfig {
            auto_reconnect: true,
            max_reconnect_attempts: 3,
            ..Default::default()
        };
        let mut tracker = DisconnectionTracker::new("test_game", config);

        // Initially disconnected - should reconnect
        assert!(tracker.should_reconnect());

        // After max attempts - should not reconnect
        tracker.mark_reconnecting();
        tracker.mark_reconnecting();
        tracker.mark_reconnecting();
        tracker.set_state(ConnectionState::Disconnected, None);
        assert!(!tracker.should_reconnect());

        // Reset attempts
        tracker.reset_reconnect_attempts();
        assert!(tracker.should_reconnect());
        Ok(())
    }

    #[test]
    fn test_disconnection_tracker_auto_reconnect_disabled() -> TestResult {
        let config = DisconnectionConfig {
            auto_reconnect: false,
            ..Default::default()
        };
        let tracker = DisconnectionTracker::new("test_game", config);

        // Should not reconnect when disabled
        assert!(!tracker.should_reconnect());
        Ok(())
    }

    #[tokio::test]
    async fn test_disconnection_tracker_subscribe() -> TestResult {
        let mut tracker = DisconnectionTracker::with_defaults("test_game");
        let mut receiver = tracker.subscribe();

        // Trigger a state change
        tracker.record_data_received();

        // Should receive the state change event
        let event = tokio::time::timeout(Duration::from_millis(100), receiver.recv())
            .await?
            .ok_or("Expected event but got None")?;

        assert_eq!(event.game_id, "test_game");
        assert_eq!(event.previous_state, ConnectionState::Disconnected);
        assert_eq!(event.new_state, ConnectionState::Connected);
        Ok(())
    }

    #[tokio::test]
    async fn test_disconnection_tracker_multiple_events() -> TestResult {
        let config = DisconnectionConfig::with_timeout(10);
        let mut tracker = DisconnectionTracker::new("test_game", config);
        let mut receiver = tracker.subscribe();

        // Connect
        tracker.record_data_received();

        // Wait for timeout and check disconnection
        std::thread::sleep(Duration::from_millis(20));
        tracker.check_disconnection();

        // Should receive both events
        let connect_event = tokio::time::timeout(Duration::from_millis(100), receiver.recv())
            .await?
            .ok_or("Expected connect event")?;
        assert!(connect_event.is_connection());

        let disconnect_event = tokio::time::timeout(Duration::from_millis(100), receiver.recv())
            .await?
            .ok_or("Expected disconnect event")?;
        assert!(disconnect_event.is_disconnection());
        Ok(())
    }

    #[test]
    fn test_disconnection_tracker_no_duplicate_events() -> TestResult {
        let mut tracker = DisconnectionTracker::with_defaults("test_game");

        // Record data multiple times - should only transition once
        tracker.record_data_received();
        assert_eq!(tracker.state(), ConnectionState::Connected);

        tracker.record_data_received();
        assert_eq!(tracker.state(), ConnectionState::Connected);

        // State should still be connected, no additional transitions
        Ok(())
    }
}
