//! Sébastien Loeb Rally EVO (Milestone, 2016) telemetry adapter — stub.
//!
//! Sébastien Loeb Rally EVO has limited / undocumented telemetry support.
//! This stub adapter returns a default `NormalizedTelemetry` frame until a
//! concrete protocol implementation is contributed.

use crate::{NormalizedTelemetry, TelemetryAdapter, TelemetryReceiver};
use anyhow::Result;
use async_trait::async_trait;
use std::time::Duration;
use tokio::sync::mpsc;

/// Sébastien Loeb Rally EVO adapter (stub — no native telemetry protocol documented).
pub struct SebLoebRallyAdapter;

impl SebLoebRallyAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SebLoebRallyAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TelemetryAdapter for SebLoebRallyAdapter {
    fn game_id(&self) -> &str {
        "seb_loeb_rally"
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (_tx, rx) = mpsc::channel(1);
        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, _raw: &[u8]) -> Result<NormalizedTelemetry> {
        Ok(NormalizedTelemetry::builder().build())
    }

    fn expected_update_rate(&self) -> Duration {
        Duration::from_millis(16)
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_id_is_correct() {
        assert_eq!(SebLoebRallyAdapter::new().game_id(), "seb_loeb_rally");
    }

    #[test]
    fn normalize_returns_default() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = SebLoebRallyAdapter::new();
        let t = adapter.normalize(&[])?;
        assert_eq!(t.speed_ms, 0.0);
        assert_eq!(t.rpm, 0.0);
        Ok(())
    }
}
