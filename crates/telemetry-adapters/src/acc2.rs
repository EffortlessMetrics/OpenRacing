//! ACC2 (Assetto Corsa Competizione 2) telemetry adapter — stub.
//!
//! As of 2025, Kunos Simulazioni has not announced or released ACC2. The
//! existing Assetto Corsa Competizione (ACC 1.x) uses the UDP broadcasting
//! protocol v4, and Assetto Corsa EVO is a separate title on a new engine.
//! No public SDK, shared-memory struct layout, or UDP protocol documentation
//! exists for a hypothetical ACC2.
//!
//! This stub adapter registers the game family in the support matrix so users
//! can see it as a known (pending) title. `normalize` always returns
//! [`NormalizedTelemetry::default()`] and `start_monitoring` emits no frames.
//!
//! See friction log entry F-022.

use crate::{NormalizedTelemetry, TelemetryAdapter, TelemetryReceiver};
use anyhow::Result;
use async_trait::async_trait;
use std::time::Duration;
use tokio::sync::mpsc;

/// Stub adapter for ACC2 — no public telemetry protocol documented.
pub struct ACC2Adapter;

impl Default for ACC2Adapter {
    fn default() -> Self {
        Self
    }
}

impl ACC2Adapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TelemetryAdapter for ACC2Adapter {
    fn game_id(&self) -> &str {
        "acc2"
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (_tx, rx) = mpsc::channel(1);
        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, _raw: &[u8]) -> Result<NormalizedTelemetry> {
        Ok(NormalizedTelemetry::default())
    }

    fn expected_update_rate(&self) -> Duration {
        Duration::from_secs(1)
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
        assert_eq!(ACC2Adapter::new().game_id(), "acc2");
    }

    #[test]
    fn normalize_returns_default() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = ACC2Adapter::new();
        let t = adapter.normalize(&[])?;
        assert_eq!(t.speed_ms, 0.0);
        assert_eq!(t.rpm, 0.0);
        Ok(())
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        #[test]
        fn parse_no_panic_on_arbitrary(
            data in proptest::collection::vec(any::<u8>(), 0..1024)
        ) {
            let adapter = ACC2Adapter::new();
            let _ = adapter.normalize(&data);
        }
    }
}
