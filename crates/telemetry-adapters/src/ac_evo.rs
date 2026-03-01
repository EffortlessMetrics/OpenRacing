//! Assetto Corsa EVO telemetry adapter — stub.
//!
//! AC EVO (Kunos Simulazioni, Early Access since 2024) is built on a new
//! engine. As of version 0.5.2 (Feb 2026) no public telemetry API, shared
//! memory interface, or UDP protocol documentation has been published.
//!
//! The original Assetto Corsa (AC1) used a Windows shared-memory interface
//! (`acpmf_physics`, `acpmf_graphics`, `acpmf_static`) and ACC uses a UDP
//! broadcasting protocol v4. AC EVO may adopt one of these approaches or
//! introduce a new API once it exits Early Access.
//!
//! SimHub does not list AC EVO support as of early 2026, and no community
//! reverse-engineering repositories exist on GitHub.
//!
//! This stub adapter registers the game in the support matrix so users can
//! see it as a known (pending) title. `normalize` always returns
//! [`NormalizedTelemetry::default()`] and `start_monitoring` emits no frames.
//!
//! See friction log entry F-022.

use crate::{NormalizedTelemetry, TelemetryAdapter, TelemetryReceiver};
use anyhow::Result;
use async_trait::async_trait;
use std::time::Duration;
use tokio::sync::mpsc;

/// Stub adapter for Assetto Corsa EVO — no public telemetry protocol documented.
pub struct ACEvoAdapter;

impl Default for ACEvoAdapter {
    fn default() -> Self {
        Self
    }
}

impl ACEvoAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TelemetryAdapter for ACEvoAdapter {
    fn game_id(&self) -> &str {
        "ac_evo"
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
        assert_eq!(ACEvoAdapter::new().game_id(), "ac_evo");
    }

    #[test]
    fn normalize_returns_default() -> Result<(), Box<dyn std::error::Error>> {
        let adapter = ACEvoAdapter::new();
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
            let adapter = ACEvoAdapter::new();
            let _ = adapter.normalize(&data);
        }
    }
}
