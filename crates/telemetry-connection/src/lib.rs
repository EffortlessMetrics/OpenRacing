//! Connection state and disconnection tracking for telemetry adapters.

#![deny(static_mut_refs)]

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub type ConnectionStateReceiver = mpsc::Receiver<ConnectionStateEvent>;
pub type ConnectionStateSender = mpsc::Sender<ConnectionStateEvent>;

pub const DEFAULT_DISCONNECTION_TIMEOUT_MS: u64 = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ConnectionState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Error,
}

impl ConnectionState {
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }

    pub fn is_disconnected(&self) -> bool {
        matches!(self, Self::Disconnected | Self::Error)
    }

    pub fn is_transitioning(&self) -> bool {
        matches!(self, Self::Connecting | Self::Reconnecting)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStateEvent {
    pub game_id: String,
    pub previous_state: ConnectionState,
    pub new_state: ConnectionState,
    pub timestamp_ns: u64,
    pub reason: Option<String>,
}

impl ConnectionStateEvent {
    pub fn new(
        game_id: impl Into<String>,
        previous_state: ConnectionState,
        new_state: ConnectionState,
        reason: Option<String>,
    ) -> Self {
        let timestamp_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0);

        Self {
            game_id: game_id.into(),
            previous_state,
            new_state,
            timestamp_ns,
            reason,
        }
    }

    pub fn is_disconnection(&self) -> bool {
        self.previous_state.is_connected() && self.new_state.is_disconnected()
    }

    pub fn is_connection(&self) -> bool {
        !self.previous_state.is_connected() && self.new_state.is_connected()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisconnectionConfig {
    pub timeout_ms: u64,
    pub auto_reconnect: bool,
    pub max_reconnect_attempts: u32,
    pub reconnect_delay_ms: u64,
}

impl Default for DisconnectionConfig {
    fn default() -> Self {
        Self {
            timeout_ms: DEFAULT_DISCONNECTION_TIMEOUT_MS,
            auto_reconnect: true,
            max_reconnect_attempts: 0,
            reconnect_delay_ms: 1000,
        }
    }
}

impl DisconnectionConfig {
    pub fn with_timeout(timeout_ms: u64) -> Self {
        Self {
            timeout_ms,
            ..Self::default()
        }
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }

    pub fn reconnect_delay(&self) -> Duration {
        Duration::from_millis(self.reconnect_delay_ms)
    }
}

#[derive(Debug)]
pub struct DisconnectionTracker {
    config: DisconnectionConfig,
    last_data_time: Option<Instant>,
    state: ConnectionState,
    reconnect_attempts: u32,
    state_sender: Option<ConnectionStateSender>,
    game_id: String,
}

impl DisconnectionTracker {
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

    pub fn with_defaults(game_id: impl Into<String>) -> Self {
        Self::new(game_id, DisconnectionConfig::default())
    }

    pub fn set_state_sender(&mut self, sender: ConnectionStateSender) {
        self.state_sender = Some(sender);
    }

    pub fn subscribe(&mut self) -> ConnectionStateReceiver {
        let (tx, rx) = mpsc::channel(16);
        self.state_sender = Some(tx);
        rx
    }

    pub fn record_data_received(&mut self) {
        self.last_data_time = Some(Instant::now());

        if self.state != ConnectionState::Connected {
            self.transition_to(
                ConnectionState::Connected,
                Some("Data received".to_string()),
            );
            self.reconnect_attempts = 0;
        }
    }

    pub fn is_timed_out(&self) -> bool {
        match self.last_data_time {
            Some(last_time) => last_time.elapsed() > self.config.timeout(),
            None => false,
        }
    }

    pub fn check_disconnection(&mut self) -> ConnectionState {
        if self.state == ConnectionState::Connected && self.is_timed_out() {
            self.transition_to(
                ConnectionState::Disconnected,
                Some(format!("No data received for {}ms", self.config.timeout_ms)),
            );
        }

        self.state
    }

    pub fn state(&self) -> ConnectionState {
        self.state
    }

    pub fn set_state(&mut self, new_state: ConnectionState, reason: Option<String>) {
        self.transition_to(new_state, reason);
    }

    pub fn mark_connecting(&mut self) {
        self.transition_to(ConnectionState::Connecting, Some("Connecting".to_string()));
    }

    pub fn mark_reconnecting(&mut self) {
        self.reconnect_attempts += 1;
        self.transition_to(
            ConnectionState::Reconnecting,
            Some(format!("Reconnection attempt {}", self.reconnect_attempts)),
        );
    }

    pub fn mark_error(&mut self, reason: String) {
        self.transition_to(ConnectionState::Error, Some(reason));
    }

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

    pub fn reconnect_attempts(&self) -> u32 {
        self.reconnect_attempts
    }

    pub fn reset_reconnect_attempts(&mut self) {
        self.reconnect_attempts = 0;
    }

    pub fn time_since_last_data(&self) -> Option<Duration> {
        self.last_data_time.map(|time| time.elapsed())
    }

    fn transition_to(&mut self, new_state: ConnectionState, reason: Option<String>) {
        if self.state == new_state {
            return;
        }

        let previous_state = self.state;
        self.state = new_state;

        if let Some(sender) = &self.state_sender {
            let event =
                ConnectionStateEvent::new(self.game_id.clone(), previous_state, new_state, reason);
            let _ = sender.try_send(event);
        }
    }
}
