//! Core telemetry types, adapter traits, contracts, and orchestration.
//!
//! This crate consolidates all telemetry domain types, the adapter trait,
//! rate limiting, BDD metrics, and the telemetry service orchestration.
//!
//! ## Modules
//! - `contracts` - Normalized telemetry types (`NormalizedTelemetry`, `TelemetryFlags`, etc.)
//! - `rate_limiter` - Rate limiting utilities for RT paths
//! - `bdd_metrics` - BDD-oriented matrix parity metrics
//! - `integration` - Matrix/registry coverage validation utilities (feature: orchestrator)
//! - `orchestrator` - Telemetry service coordination (feature: orchestrator)

#![deny(static_mut_refs)]

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::mpsc;

pub mod bdd_metrics;
pub mod contracts;
#[cfg(feature = "orchestrator")]
pub mod integration;
#[cfg(feature = "orchestrator")]
pub mod orchestrator;
pub mod rate_limiter;

pub use bdd_metrics::{BddMatrixMetrics, MatrixParityPolicy, RuntimeBddMatrixMetrics};
pub use contracts::{
    FlagCoverage, NormalizedTelemetry, TelemetryFieldCoverage, TelemetryFlags, TelemetryFrame,
    TelemetryValue,
};
#[cfg(feature = "orchestrator")]
pub use integration::{
    CoverageMismatch, CoveragePolicy, RegistryCoverage, RegistryCoverageMetrics,
    RuntimeCoverageMetrics, RuntimeCoverageReport, compare_matrix_and_registry,
    compare_matrix_and_registry_with_policy, compare_runtime_registries_with_policies,
};
#[cfg(feature = "orchestrator")]
pub use orchestrator::TelemetryService;
pub use rate_limiter::{AdaptiveRateLimiter, RateLimiter, RateLimiterStats};

pub type ConnectionStateReceiver = mpsc::Receiver<ConnectionStateEvent>;
pub type ConnectionStateSender = mpsc::Sender<ConnectionStateEvent>;

pub const DEFAULT_DISCONNECTION_TIMEOUT_MS: u64 = 2000;

pub fn telemetry_now_ns() -> u64 {
    static EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let epoch = EPOCH.get_or_init(Instant::now);
    Instant::now()
        .checked_duration_since(*epoch)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
        .min(u64::MAX as u128) as u64
}

pub type TelemetryReceiver = mpsc::Receiver<TelemetryFrame>;

#[async_trait]
pub trait TelemetryAdapter: Send + Sync {
    fn game_id(&self) -> &str;
    async fn start_monitoring(&self) -> Result<TelemetryReceiver>;
    async fn stop_monitoring(&self) -> Result<()>;
    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry>;
    fn expected_update_rate(&self) -> Duration;
    async fn is_game_running(&self) -> Result<bool>;
}

pub type AdapterFactory = fn() -> Box<dyn TelemetryAdapter>;

pub fn adapter_factories() -> &'static [(&'static str, AdapterFactory)] {
    static FACTORIES: std::sync::OnceLock<Vec<(&'static str, AdapterFactory)>> =
        std::sync::OnceLock::new();
    FACTORIES.get_or_init(Vec::new)
}

#[derive(Clone, Debug)]
pub struct GameTelemetry {
    pub timestamp: Instant,
    pub speed_mps: f32,
    pub rpm: f32,
    pub gear: i8,
    pub steering_angle: f32,
    pub throttle: f32,
    pub brake: f32,
    pub lateral_g: f32,
    pub longitudinal_g: f32,
    pub slip_angle_fl: f32,
    pub slip_angle_fr: f32,
    pub slip_angle_rl: f32,
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
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_timestamp(timestamp: Instant) -> Self {
        Self {
            timestamp,
            ..Default::default()
        }
    }

    pub fn speed_kmh(&self) -> f32 {
        self.speed_mps * 3.6
    }

    pub fn speed_mph(&self) -> f32 {
        self.speed_mps * 2.237
    }

    pub fn average_slip_angle(&self) -> f32 {
        (self.slip_angle_fl + self.slip_angle_fr + self.slip_angle_rl + self.slip_angle_rr) / 4.0
    }

    pub fn front_slip_angle(&self) -> f32 {
        (self.slip_angle_fl + self.slip_angle_fr) / 2.0
    }

    pub fn rear_slip_angle(&self) -> f32 {
        (self.slip_angle_rl + self.slip_angle_rr) / 2.0
    }

    pub fn is_stationary(&self) -> bool {
        self.speed_mps < 0.5
    }

    pub fn total_g(&self) -> f32 {
        (self.lateral_g * self.lateral_g + self.longitudinal_g * self.longitudinal_g).sqrt()
    }

    pub fn to_normalized(&self) -> NormalizedTelemetry {
        NormalizedTelemetry::builder()
            .rpm(self.rpm)
            .speed_ms(self.speed_mps)
            .gear(self.gear)
            .steering_angle(self.steering_angle)
            .throttle(self.throttle)
            .brake(self.brake)
            .lateral_g(self.lateral_g)
            .longitudinal_g(self.longitudinal_g)
            .slip_angle_fl(self.slip_angle_fl)
            .slip_angle_fr(self.slip_angle_fr)
            .slip_angle_rl(self.slip_angle_rl)
            .slip_angle_rr(self.slip_angle_rr)
            .slip_ratio(self.average_slip_angle().abs().min(1.0))
            .timestamp(self.timestamp)
            .build()
    }
}

impl From<GameTelemetry> for NormalizedTelemetry {
    fn from(telemetry: GameTelemetry) -> Self {
        telemetry.to_normalized()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameTelemetrySnapshot {
    pub timestamp_ns: u64,
    pub speed_mps: f32,
    pub rpm: f32,
    pub gear: i8,
    pub steering_angle: f32,
    pub throttle: f32,
    pub brake: f32,
    pub lateral_g: f32,
    pub longitudinal_g: f32,
    pub slip_angle_fl: f32,
    pub slip_angle_fr: f32,
    pub slip_angle_rl: f32,
    pub slip_angle_rr: f32,
}

impl GameTelemetrySnapshot {
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

#[derive(Error, Debug)]
pub enum TelemetryError {
    #[error("Failed to connect to telemetry source: {0}")]
    ConnectionFailed(String),

    #[error("Game is not running: {game_id}")]
    GameNotRunning { game_id: String },

    #[error("Failed to parse telemetry data: {0}")]
    ParseError(String),

    #[error("Shared memory error: {0}")]
    SharedMemoryError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Adapter already connected")]
    AlreadyConnected,

    #[error("Adapter not connected")]
    NotConnected,

    #[error("Telemetry timeout after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },

    #[error("Invalid telemetry data: {reason}")]
    InvalidData { reason: String },

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

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
        matches!(self, ConnectionState::Connected)
    }

    pub fn is_disconnected(&self) -> bool {
        matches!(self, ConnectionState::Disconnected | ConnectionState::Error)
    }

    pub fn is_transitioning(&self) -> bool {
        matches!(
            self,
            ConnectionState::Connecting | ConnectionState::Reconnecting
        )
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
            ..Default::default()
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
        self.last_data_time.map(|t| t.elapsed())
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

#[async_trait]
pub trait GameTelemetryAdapter: Send + Sync {
    async fn connect(&mut self) -> Result<(), TelemetryError>;
    async fn disconnect(&mut self) -> Result<(), TelemetryError>;
    fn poll(&mut self) -> Option<GameTelemetry>;
    fn game_id(&self) -> &str;
    fn connection_state(&self) -> ConnectionState {
        ConnectionState::Disconnected
    }
    async fn is_game_running(&self) -> bool {
        false
    }
    fn subscribe_state_changes(&mut self) -> Option<ConnectionStateReceiver> {
        None
    }
    fn disconnection_config(&self) -> DisconnectionConfig {
        DisconnectionConfig::default()
    }
    fn set_disconnection_config(&mut self, _config: DisconnectionConfig) {}
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
        assert_eq!(telemetry.throttle, 0.0);
        assert_eq!(telemetry.brake, 0.0);
        assert_eq!(telemetry.steering_angle, 0.0);
        assert_eq!(telemetry.lateral_g, 0.0);
        assert_eq!(telemetry.longitudinal_g, 0.0);
        Ok(())
    }

    #[test]
    fn test_game_telemetry_new_equals_default() -> TestResult {
        let a = GameTelemetry::new();
        let b = GameTelemetry::default();
        assert_eq!(a.speed_mps, b.speed_mps);
        assert_eq!(a.rpm, b.rpm);
        assert_eq!(a.gear, b.gear);
        Ok(())
    }

    #[test]
    fn test_game_telemetry_with_timestamp() -> TestResult {
        let ts = Instant::now();
        let telemetry = GameTelemetry::with_timestamp(ts);
        assert_eq!(telemetry.speed_mps, 0.0);
        assert_eq!(telemetry.rpm, 0.0);
        Ok(())
    }

    #[test]
    fn test_game_telemetry_conversions() -> TestResult {
        let telemetry = GameTelemetry {
            speed_mps: 27.78,
            ..Default::default()
        };

        assert!((telemetry.speed_kmh() - 100.0).abs() < 0.1);
        assert!((telemetry.speed_mph() - 62.14).abs() < 0.1);
        Ok(())
    }

    #[test]
    fn test_game_telemetry_zero_speed_conversions() -> TestResult {
        let telemetry = GameTelemetry::default();
        assert_eq!(telemetry.speed_kmh(), 0.0);
        assert_eq!(telemetry.speed_mph(), 0.0);
        Ok(())
    }

    #[test]
    fn test_game_telemetry_to_normalized() -> TestResult {
        let telemetry = GameTelemetry {
            speed_mps: 50.0,
            rpm: 6000.0,
            gear: 4,
            throttle: 0.8,
            brake: 0.1,
            steering_angle: 0.15,
            lateral_g: 1.5,
            longitudinal_g: 0.5,
            ..Default::default()
        };

        let normalized = telemetry.to_normalized();

        assert_eq!(normalized.rpm, 6000.0);
        assert_eq!(normalized.speed_ms, 50.0);
        assert_eq!(normalized.gear, 4);
        assert_eq!(normalized.throttle, 0.8);
        assert_eq!(normalized.brake, 0.1);
        assert_eq!(normalized.steering_angle, 0.15);
        assert_eq!(normalized.lateral_g, 1.5);
        assert_eq!(normalized.longitudinal_g, 0.5);
        Ok(())
    }

    #[test]
    fn test_game_telemetry_to_normalized_slip_ratio() -> TestResult {
        let telemetry = GameTelemetry {
            slip_angle_fl: 0.1,
            slip_angle_fr: 0.2,
            slip_angle_rl: 0.3,
            slip_angle_rr: 0.4,
            ..Default::default()
        };

        let normalized = telemetry.to_normalized();
        let expected_avg: f32 = (0.1 + 0.2 + 0.3 + 0.4) / 4.0;
        assert!((normalized.slip_ratio - expected_avg.abs().min(1.0)).abs() < 0.001);
        assert_eq!(normalized.slip_angle_fl, 0.1);
        assert_eq!(normalized.slip_angle_fr, 0.2);
        assert_eq!(normalized.slip_angle_rl, 0.3);
        assert_eq!(normalized.slip_angle_rr, 0.4);
        Ok(())
    }

    #[test]
    fn test_game_telemetry_from_into_normalized() -> TestResult {
        let telemetry = GameTelemetry {
            speed_mps: 30.0,
            rpm: 4000.0,
            gear: 3,
            ..Default::default()
        };

        let normalized: NormalizedTelemetry = telemetry.into();
        assert_eq!(normalized.speed_ms, 30.0);
        assert_eq!(normalized.rpm, 4000.0);
        assert_eq!(normalized.gear, 3);
        Ok(())
    }

    #[test]
    fn test_game_telemetry_slip_angles() -> TestResult {
        let telemetry = GameTelemetry {
            slip_angle_fl: 0.02,
            slip_angle_fr: 0.04,
            slip_angle_rl: 0.06,
            slip_angle_rr: 0.08,
            ..Default::default()
        };

        assert!((telemetry.average_slip_angle() - 0.05).abs() < 0.001);
        assert!((telemetry.front_slip_angle() - 0.03).abs() < 0.001);
        assert!((telemetry.rear_slip_angle() - 0.07).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_game_telemetry_is_stationary() -> TestResult {
        let stationary = GameTelemetry {
            speed_mps: 0.3,
            ..Default::default()
        };
        assert!(stationary.is_stationary());

        let moving = GameTelemetry {
            speed_mps: 10.0,
            ..Default::default()
        };
        assert!(!moving.is_stationary());

        let threshold = GameTelemetry {
            speed_mps: 0.5,
            ..Default::default()
        };
        assert!(!threshold.is_stationary());
        Ok(())
    }

    #[test]
    fn test_game_telemetry_total_g() -> TestResult {
        let telemetry = GameTelemetry {
            lateral_g: 3.0,
            longitudinal_g: 4.0,
            ..Default::default()
        };
        assert!((telemetry.total_g() - 5.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_game_telemetry_total_g_zero() -> TestResult {
        let telemetry = GameTelemetry::default();
        assert_eq!(telemetry.total_g(), 0.0);
        Ok(())
    }

    // ── GameTelemetrySnapshot tests ───────────────────────────────────────

    #[test]
    fn test_game_telemetry_snapshot_from_telemetry() -> TestResult {
        let epoch = Instant::now();
        let telemetry = GameTelemetry {
            speed_mps: 50.0,
            rpm: 6000.0,
            gear: 4,
            throttle: 0.8,
            brake: 0.1,
            lateral_g: 1.5,
            longitudinal_g: 0.5,
            slip_angle_fl: 0.01,
            slip_angle_fr: 0.02,
            slip_angle_rl: 0.03,
            slip_angle_rr: 0.04,
            ..Default::default()
        };

        let snapshot = GameTelemetrySnapshot::from_telemetry(&telemetry, epoch);
        assert_eq!(snapshot.speed_mps, 50.0);
        assert_eq!(snapshot.rpm, 6000.0);
        assert_eq!(snapshot.gear, 4);
        assert_eq!(snapshot.throttle, 0.8);
        assert_eq!(snapshot.brake, 0.1);
        assert_eq!(snapshot.lateral_g, 1.5);
        assert_eq!(snapshot.longitudinal_g, 0.5);
        assert_eq!(snapshot.slip_angle_fl, 0.01);
        Ok(())
    }

    #[test]
    fn test_game_telemetry_snapshot_serializable() -> TestResult {
        let epoch = Instant::now();
        let telemetry = GameTelemetry {
            speed_mps: 30.0,
            rpm: 4000.0,
            gear: 3,
            ..Default::default()
        };

        let snapshot = GameTelemetrySnapshot::from_telemetry(&telemetry, epoch);
        let json = serde_json::to_string(&snapshot)?;
        let deserialized: GameTelemetrySnapshot = serde_json::from_str(&json)?;
        assert_eq!(deserialized.speed_mps, 30.0);
        assert_eq!(deserialized.rpm, 4000.0);
        assert_eq!(deserialized.gear, 3);
        Ok(())
    }

    // ── ConnectionState tests ─────────────────────────────────────────────

    #[test]
    fn test_connection_state() -> TestResult {
        assert!(ConnectionState::Connected.is_connected());
        assert!(!ConnectionState::Disconnected.is_connected());
        assert!(ConnectionState::Disconnected.is_disconnected());
        Ok(())
    }

    #[test]
    fn test_connection_state_error_is_disconnected() -> TestResult {
        assert!(ConnectionState::Error.is_disconnected());
        assert!(!ConnectionState::Error.is_connected());
        Ok(())
    }

    #[test]
    fn test_connection_state_transitioning() -> TestResult {
        assert!(ConnectionState::Connecting.is_transitioning());
        assert!(ConnectionState::Reconnecting.is_transitioning());
        assert!(!ConnectionState::Connected.is_transitioning());
        assert!(!ConnectionState::Disconnected.is_transitioning());
        assert!(!ConnectionState::Error.is_transitioning());
        Ok(())
    }

    #[test]
    fn test_connection_state_default() -> TestResult {
        let state = ConnectionState::default();
        assert_eq!(state, ConnectionState::Disconnected);
        Ok(())
    }

    // ── ConnectionStateEvent tests ────────────────────────────────────────

    #[test]
    fn test_connection_state_event_new() -> TestResult {
        let event = ConnectionStateEvent::new(
            "forza",
            ConnectionState::Disconnected,
            ConnectionState::Connected,
            Some("Data received".to_string()),
        );
        assert_eq!(event.game_id, "forza");
        assert_eq!(event.previous_state, ConnectionState::Disconnected);
        assert_eq!(event.new_state, ConnectionState::Connected);
        assert!(event.timestamp_ns > 0);
        assert_eq!(event.reason, Some("Data received".to_string()));
        Ok(())
    }

    #[test]
    fn test_connection_state_event_is_disconnection() -> TestResult {
        let event = ConnectionStateEvent::new(
            "test",
            ConnectionState::Connected,
            ConnectionState::Disconnected,
            None,
        );
        assert!(event.is_disconnection());
        assert!(!event.is_connection());
        Ok(())
    }

    #[test]
    fn test_connection_state_event_is_connection() -> TestResult {
        let event = ConnectionStateEvent::new(
            "test",
            ConnectionState::Disconnected,
            ConnectionState::Connected,
            None,
        );
        assert!(event.is_connection());
        assert!(!event.is_disconnection());
        Ok(())
    }

    #[test]
    fn test_connection_state_event_error_to_connected() -> TestResult {
        let event = ConnectionStateEvent::new(
            "test",
            ConnectionState::Error,
            ConnectionState::Connected,
            None,
        );
        assert!(event.is_connection());
        assert!(!event.is_disconnection());
        Ok(())
    }

    #[test]
    fn test_connection_state_event_connected_to_error() -> TestResult {
        let event = ConnectionStateEvent::new(
            "test",
            ConnectionState::Connected,
            ConnectionState::Error,
            None,
        );
        assert!(event.is_disconnection());
        Ok(())
    }

    // ── DisconnectionConfig tests ─────────────────────────────────────────

    #[test]
    fn test_disconnection_config() -> TestResult {
        let config = DisconnectionConfig::with_timeout(5000);
        assert_eq!(config.timeout_ms, 5000);
        assert_eq!(config.timeout(), Duration::from_millis(5000));
        assert!(config.auto_reconnect);
        assert_eq!(config.max_reconnect_attempts, 0);
        Ok(())
    }

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
    fn test_disconnection_config_reconnect_delay() -> TestResult {
        let config = DisconnectionConfig {
            reconnect_delay_ms: 2500,
            ..Default::default()
        };
        assert_eq!(config.reconnect_delay(), Duration::from_millis(2500));
        Ok(())
    }

    // ── DisconnectionTracker tests ────────────────────────────────────────

    #[test]
    fn test_disconnection_tracker() -> TestResult {
        let mut tracker = DisconnectionTracker::with_defaults("test_game");
        assert_eq!(tracker.state(), ConnectionState::Disconnected);

        tracker.record_data_received();
        assert_eq!(tracker.state(), ConnectionState::Connected);
        Ok(())
    }

    #[test]
    fn test_disconnection_tracker_mark_connecting() -> TestResult {
        let mut tracker = DisconnectionTracker::with_defaults("test_game");
        tracker.mark_connecting();
        assert_eq!(tracker.state(), ConnectionState::Connecting);
        Ok(())
    }

    #[test]
    fn test_disconnection_tracker_mark_reconnecting() -> TestResult {
        let mut tracker = DisconnectionTracker::with_defaults("test_game");
        tracker.mark_reconnecting();
        assert_eq!(tracker.state(), ConnectionState::Reconnecting);
        assert_eq!(tracker.reconnect_attempts(), 1);

        tracker.mark_reconnecting();
        assert_eq!(tracker.reconnect_attempts(), 2);
        Ok(())
    }

    #[test]
    fn test_disconnection_tracker_mark_error() -> TestResult {
        let mut tracker = DisconnectionTracker::with_defaults("test_game");
        tracker.mark_error("socket failed".to_string());
        assert_eq!(tracker.state(), ConnectionState::Error);
        Ok(())
    }

    #[test]
    fn test_disconnection_tracker_reset_reconnect_attempts() -> TestResult {
        let mut tracker = DisconnectionTracker::with_defaults("test_game");
        tracker.mark_reconnecting();
        tracker.mark_reconnecting();
        assert_eq!(tracker.reconnect_attempts(), 2);

        tracker.reset_reconnect_attempts();
        assert_eq!(tracker.reconnect_attempts(), 0);
        Ok(())
    }

    #[test]
    fn test_disconnection_tracker_should_reconnect_default() -> TestResult {
        let tracker = DisconnectionTracker::with_defaults("test_game");
        // Disconnected + auto_reconnect=true + no max attempts → should reconnect
        assert!(tracker.should_reconnect());
        Ok(())
    }

    #[test]
    fn test_disconnection_tracker_should_not_reconnect_when_connected() -> TestResult {
        let mut tracker = DisconnectionTracker::with_defaults("test_game");
        tracker.record_data_received();
        assert!(!tracker.should_reconnect());
        Ok(())
    }

    #[test]
    fn test_disconnection_tracker_should_not_reconnect_max_attempts() -> TestResult {
        let config = DisconnectionConfig {
            max_reconnect_attempts: 2,
            ..Default::default()
        };
        let mut tracker = DisconnectionTracker::new("test_game", config);
        tracker.mark_reconnecting(); // attempt 1
        tracker.mark_reconnecting(); // attempt 2
        // Now at max, should not reconnect even though state is Reconnecting
        // Need to transition to disconnected first
        tracker.set_state(ConnectionState::Disconnected, None);
        assert!(!tracker.should_reconnect());
        Ok(())
    }

    #[test]
    fn test_disconnection_tracker_should_not_reconnect_disabled() -> TestResult {
        let config = DisconnectionConfig {
            auto_reconnect: false,
            ..Default::default()
        };
        let tracker = DisconnectionTracker::new("test_game", config);
        assert!(!tracker.should_reconnect());
        Ok(())
    }

    #[test]
    fn test_disconnection_tracker_data_received_resets_reconnects() -> TestResult {
        let mut tracker = DisconnectionTracker::with_defaults("test_game");
        tracker.mark_reconnecting();
        tracker.mark_reconnecting();
        assert_eq!(tracker.reconnect_attempts(), 2);

        tracker.record_data_received();
        assert_eq!(tracker.state(), ConnectionState::Connected);
        assert_eq!(tracker.reconnect_attempts(), 0);
        Ok(())
    }

    #[test]
    fn test_disconnection_tracker_repeated_data_received_stays_connected() -> TestResult {
        let mut tracker = DisconnectionTracker::with_defaults("test_game");
        tracker.record_data_received();
        assert_eq!(tracker.state(), ConnectionState::Connected);

        tracker.record_data_received();
        assert_eq!(tracker.state(), ConnectionState::Connected);
        Ok(())
    }

    #[test]
    fn test_disconnection_tracker_is_timed_out_no_data() -> TestResult {
        let tracker = DisconnectionTracker::with_defaults("test_game");
        // No data ever received → not timed out
        assert!(!tracker.is_timed_out());
        Ok(())
    }

    #[test]
    fn test_disconnection_tracker_time_since_last_data_none() -> TestResult {
        let tracker = DisconnectionTracker::with_defaults("test_game");
        assert!(tracker.time_since_last_data().is_none());
        Ok(())
    }

    #[test]
    fn test_disconnection_tracker_time_since_last_data_some() -> TestResult {
        let mut tracker = DisconnectionTracker::with_defaults("test_game");
        tracker.record_data_received();
        let elapsed = tracker.time_since_last_data();
        assert!(elapsed.is_some());
        Ok(())
    }

    #[test]
    fn test_disconnection_tracker_subscribe() -> TestResult {
        let mut tracker = DisconnectionTracker::with_defaults("test_game");
        let mut rx = tracker.subscribe();

        tracker.mark_connecting();
        let event = rx.try_recv();
        assert!(event.is_ok());
        let event = event.map_err(|e| format!("{e}"))?;
        assert_eq!(event.game_id, "test_game");
        assert_eq!(event.new_state, ConnectionState::Connecting);
        Ok(())
    }

    #[test]
    fn test_disconnection_tracker_no_event_on_same_state() -> TestResult {
        let mut tracker = DisconnectionTracker::with_defaults("test_game");
        let mut rx = tracker.subscribe();

        // First transition emits event
        tracker.mark_connecting();
        assert!(rx.try_recv().is_ok());

        // Same state transition does not emit
        tracker.set_state(ConnectionState::Connecting, None);
        assert!(rx.try_recv().is_err());
        Ok(())
    }

    #[test]
    fn test_disconnection_tracker_check_disconnection_not_connected() -> TestResult {
        let mut tracker = DisconnectionTracker::with_defaults("test_game");
        // Not connected, should stay disconnected
        let state = tracker.check_disconnection();
        assert_eq!(state, ConnectionState::Disconnected);
        Ok(())
    }

    // ── TelemetryError tests ──────────────────────────────────────────────

    #[test]
    fn test_telemetry_error_display() -> TestResult {
        let err = TelemetryError::ConnectionFailed("port in use".to_string());
        assert!(format!("{err}").contains("port in use"));

        let err = TelemetryError::GameNotRunning {
            game_id: "forza".to_string(),
        };
        assert!(format!("{err}").contains("forza"));

        let err = TelemetryError::ParseError("bad packet".to_string());
        assert!(format!("{err}").contains("bad packet"));

        let err = TelemetryError::Timeout { timeout_ms: 5000 };
        assert!(format!("{err}").contains("5000"));

        let err = TelemetryError::AlreadyConnected;
        assert!(format!("{err}").contains("already"));

        let err = TelemetryError::NotConnected;
        assert!(format!("{err}").contains("not connected"));

        let err = TelemetryError::InvalidData {
            reason: "truncated".to_string(),
        };
        assert!(format!("{err}").contains("truncated"));
        Ok(())
    }

    // ── telemetry_now_ns tests ────────────────────────────────────────────

    #[test]
    fn test_telemetry_now_ns_monotonic() -> TestResult {
        let a = telemetry_now_ns();
        std::thread::sleep(Duration::from_millis(1));
        let b = telemetry_now_ns();
        assert!(b >= a);
        Ok(())
    }
}
