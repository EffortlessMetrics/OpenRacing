//! Bridge between game process detection and telemetry adapter activation.
//!
//! When a [`ProcessEvent::GameStarted`] event arrives, this bridge starts the
//! appropriate telemetry adapter.  When a [`ProcessEvent::GameStopped`] event
//! arrives, the adapter is stopped.
//!
//! The [`TelemetryAdapterControl`] trait is the seam that allows unit-testing
//! without a real [`TelemetryService`].

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::telemetry::{TelemetryReceiver, TelemetryService};

// ─── Trait ──────────────────────────────────────────────────────────────────

/// Controls the lifecycle of a telemetry adapter for a specific game.
///
/// The default implementation ([`GameTelemetryBridge`]) delegates to a real
/// [`TelemetryService`].  Tests supply a mock that records calls.
#[async_trait]
pub trait TelemetryAdapterControl: Send + Sync {
    /// Start the telemetry adapter for `game_id`.
    async fn start_for_game(&self, game_id: &str) -> Result<()>;

    /// Stop the telemetry adapter for `game_id`.
    async fn stop_for_game(&self, game_id: &str) -> Result<()>;
}

// ─── Production implementation ───────────────────────────────────────────────

/// Bridges game-detection events to the [`TelemetryService`].
///
/// Active receiver handles are kept alive in an internal map so that the
/// adapter's background task continues running for the duration of the game
/// session.  Dropping the receiver closes the channel and signals the task to
/// stop.
pub struct GameTelemetryBridge {
    telemetry_service: Arc<Mutex<TelemetryService>>,
    active_receivers: Arc<Mutex<HashMap<String, TelemetryReceiver>>>,
}

impl GameTelemetryBridge {
    /// Create a bridge backed by `telemetry_service`.
    pub fn new(telemetry_service: Arc<Mutex<TelemetryService>>) -> Self {
        Self {
            telemetry_service,
            active_receivers: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl TelemetryAdapterControl for GameTelemetryBridge {
    async fn start_for_game(&self, game_id: &str) -> Result<()> {
        info!(game_id = %game_id, "Starting telemetry adapter for game");

        let mut service = self.telemetry_service.lock().await;
        match service.start_monitoring(game_id).await {
            Ok(receiver) => {
                // Release the service lock before acquiring the receivers lock.
                drop(service);
                let mut receivers = self.active_receivers.lock().await;
                receivers.insert(game_id.to_string(), receiver);
                info!(game_id = %game_id, "Telemetry adapter started");
                Ok(())
            }
            Err(e) => {
                // Not every game has a registered adapter; this is non-fatal.
                warn!(
                    game_id = %game_id,
                    error = %e,
                    "No telemetry adapter registered for game — skipping"
                );
                Ok(())
            }
        }
    }

    async fn stop_for_game(&self, game_id: &str) -> Result<()> {
        info!(game_id = %game_id, "Stopping telemetry adapter for game");

        // Drop the receiver first; this closes the channel and signals the
        // producer task to exit before we call stop_monitoring.
        {
            let mut receivers = self.active_receivers.lock().await;
            receivers.remove(game_id);
        }

        let service = self.telemetry_service.lock().await;
        if let Err(e) = service.stop_monitoring(game_id).await {
            warn!(
                game_id = %game_id,
                error = %e,
                "stop_monitoring returned an error (adapter may not have been active)"
            );
        }

        info!(game_id = %game_id, "Telemetry adapter stopped");
        Ok(())
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    struct MockControl {
        starts: Arc<Mutex<Vec<String>>>,
        stops: Arc<Mutex<Vec<String>>>,
    }

    impl MockControl {
        fn new() -> Self {
            Self {
                starts: Arc::new(Mutex::new(Vec::new())),
                stops: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait]
    impl TelemetryAdapterControl for MockControl {
        async fn start_for_game(&self, game_id: &str) -> Result<()> {
            self.starts.lock().await.push(game_id.to_string());
            Ok(())
        }

        async fn stop_for_game(&self, game_id: &str) -> Result<()> {
            self.stops.lock().await.push(game_id.to_string());
            Ok(())
        }
    }

    #[tokio::test]
    async fn mock_start_records_game_id() -> anyhow::Result<()> {
        let mock = MockControl::new();
        mock.start_for_game("iracing").await?;
        let calls = mock.starts.lock().await;
        assert_eq!(calls.as_slice(), ["iracing"]);
        Ok(())
    }

    #[tokio::test]
    async fn mock_stop_records_game_id() -> anyhow::Result<()> {
        let mock = MockControl::new();
        mock.stop_for_game("acc").await?;
        let calls = mock.stops.lock().await;
        assert_eq!(calls.as_slice(), ["acc"]);
        Ok(())
    }

    #[tokio::test]
    async fn mock_start_stop_sequence() -> anyhow::Result<()> {
        let mock = MockControl::new();
        mock.start_for_game("iracing").await?;
        mock.stop_for_game("iracing").await?;

        let starts = mock.starts.lock().await;
        let stops = mock.stops.lock().await;
        assert_eq!(starts.as_slice(), ["iracing"]);
        assert_eq!(stops.as_slice(), ["iracing"]);
        Ok(())
    }
}
