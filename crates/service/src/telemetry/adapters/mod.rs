//! Game-specific telemetry adapters
//!
//! Implements adapters for different racing games with their specific
//! telemetry protocols and data formats.

pub mod acc;
pub mod ams2;
pub mod eawrc;
pub mod iracing;
pub mod rfactor2;

pub use acc::ACCAdapter;
pub use ams2::AMS2Adapter;
pub use eawrc::EAWRCAdapter;
pub use iracing::IRacingAdapter;
pub use rfactor2::RFactor2Adapter;

use crate::telemetry::{NormalizedTelemetry, TelemetryAdapter, TelemetryReceiver};
use anyhow::Result;
use async_trait::async_trait;
use std::time::Duration;

/// Mock adapter for testing
pub struct MockAdapter {
    game_id: String,
    update_rate: Duration,
    is_running: bool,
}

impl MockAdapter {
    pub fn new(game_id: String) -> Self {
        Self {
            game_id,
            update_rate: Duration::from_millis(16), // ~60 FPS
            is_running: false,
        }
    }

    pub fn set_running(&mut self, running: bool) {
        self.is_running = running;
    }
}

#[async_trait]
impl TelemetryAdapter for MockAdapter {
    fn game_id(&self) -> &str {
        &self.game_id
    }

    async fn start_monitoring(&self) -> Result<TelemetryReceiver> {
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        // Spawn a task that generates mock telemetry data
        let _game_id = self.game_id.clone();
        let update_rate = self.update_rate;

        tokio::spawn(async move {
            let mut sequence = 0u64;
            let start_time = std::time::Instant::now();

            loop {
                let elapsed = start_time.elapsed();
                let timestamp_ns = elapsed.as_nanos() as u64;

                // Generate mock telemetry
                let progress = (elapsed.as_secs_f32() % 10.0) / 10.0; // 10-second cycle
                let telemetry = generate_mock_telemetry(progress);

                let frame = crate::telemetry::TelemetryFrame::new(
                    telemetry,
                    timestamp_ns,
                    sequence,
                    64, // Mock raw size
                );

                if tx.send(frame).await.is_err() {
                    break; // Receiver dropped
                }

                sequence += 1;
                tokio::time::sleep(update_rate).await;
            }
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        // Mock implementation - in real adapters this would clean up resources
        Ok(())
    }

    fn normalize(&self, _raw: &[u8]) -> Result<NormalizedTelemetry> {
        // Mock normalization
        Ok(NormalizedTelemetry::default().with_rpm(5000.0))
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(self.is_running)
    }
}

/// Generate mock telemetry data for testing
fn generate_mock_telemetry(progress: f32) -> NormalizedTelemetry {
    use std::f32::consts::PI;

    let rpm = 4000.0 + (progress * 2.0 * PI).sin() * 2000.0;
    let speed = 30.0 + progress * 40.0;
    let ffb_scalar = (progress * 4.0 * PI).sin() * 0.7;
    let slip_ratio = ((progress * 8.0 * PI).sin().abs() * 0.2).min(1.0);

    let gear = match speed {
        s if s < 20.0 => 2,
        s if s < 35.0 => 3,
        s if s < 50.0 => 4,
        s if s < 65.0 => 5,
        _ => 6,
    };

    NormalizedTelemetry::default()
        .with_ffb_scalar(ffb_scalar)
        .with_rpm(rpm.max(0.0))
        .with_speed_ms(speed)
        .with_slip_ratio(slip_ratio)
        .with_gear(gear)
        .with_car_id("mock_car".to_string())
        .with_track_id("mock_track".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[tokio::test]
    async fn test_mock_adapter() -> TestResult {
        let adapter = MockAdapter::new("test_game".to_string());

        assert_eq!(adapter.game_id(), "test_game");
        let is_running = adapter.is_game_running().await?;
        assert!(!is_running);

        let mut receiver = adapter.start_monitoring().await?;

        // Should receive telemetry frames
        let frame = tokio::time::timeout(Duration::from_millis(100), receiver.recv())
            .await?
            .ok_or("expected telemetry frame but got None")?;

        assert!(frame.data.rpm.is_some());
        assert!(frame.data.speed_ms.is_some());
        assert_eq!(frame.data.car_id, Some("mock_car".to_string()));
        Ok(())
    }

    #[test]
    fn test_mock_telemetry_generation() -> TestResult {
        let telemetry = generate_mock_telemetry(0.5);

        assert!(telemetry.rpm.is_some());
        assert!(telemetry.speed_ms.is_some());
        assert!(telemetry.ffb_scalar.is_some());
        assert!(telemetry.slip_ratio.is_some());
        assert!(telemetry.gear.is_some());
        Ok(())
    }
}
