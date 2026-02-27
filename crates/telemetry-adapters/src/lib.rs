//! Game-specific telemetry adapters.
//!
//! This crate provides the protocol implementations that were formerly embedded inside
//! `racing_wheel_service` while preserving the external adapter trait and types used by
//! higher layers.

#![deny(static_mut_refs)]

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

pub use racing_wheel_telemetry_core::{
    NormalizedTelemetry, TelemetryFlags, TelemetryFrame, TelemetryValue,
};

// Keep these protocol modules first so dependent implementations can import helpers
// via `crate::` paths unchanged from their service-side origins.
pub mod ac_rally;
pub mod acc;
pub mod ams2;
pub mod assetto_corsa;
pub mod automobilista;
pub mod beamng;
pub mod codemasters_udp;
pub mod dirt4;
pub mod dirt5;
pub mod dirt_rally_2;
pub mod eawrc;
pub mod ets2;
pub mod f1;
pub mod f1_25;
pub mod forza;
pub mod gran_turismo_7;
pub mod grid_2019;
pub mod grid_autosport;
pub mod grid_legends;
pub mod iracing;
pub mod kartkraft;
pub mod le_mans_ultimate;
pub mod lfs;
pub mod nascar;
pub mod pcars2;
pub mod raceroom;
pub mod rbr;
pub mod rennsport;
pub mod rfactor2;
pub mod trackmania;
pub mod wrc_generations;
pub mod wreckfest;
pub mod wtcr;

/// Shared type alias for outbound telemetry streams.
pub type TelemetryReceiver = mpsc::Receiver<TelemetryFrame>;

static TELEMETRY_EPOCH: OnceLock<Instant> = OnceLock::new();

/// Return a monotonic timestamp in nanoseconds using a process-wide epoch.
pub fn telemetry_now_ns() -> u64 {
    let epoch = TELEMETRY_EPOCH.get_or_init(Instant::now);
    Instant::now()
        .checked_duration_since(*epoch)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
        .min(u64::MAX as u128) as u64
}

/// Telemetry adapter trait for game-specific telemetry sources.
#[async_trait]
pub trait TelemetryAdapter: Send + Sync {
    /// Get the game identifier this adapter supports.
    fn game_id(&self) -> &str;

    /// Start monitoring telemetry from the game.
    async fn start_monitoring(&self) -> Result<TelemetryReceiver>;

    /// Stop monitoring telemetry.
    async fn stop_monitoring(&self) -> Result<()>;

    /// Normalize raw telemetry data to common format.
    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry>;

    /// Expected update rate for this adapter.
    fn expected_update_rate(&self) -> Duration;

    /// Check if the game is currently running.
    async fn is_game_running(&self) -> Result<bool>;
}

/// Factory for constructing adapter instances.
pub type AdapterFactory = fn() -> Box<dyn TelemetryAdapter>;

fn new_ac_rally_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(ACRallyAdapter::new())
}

fn new_acc_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(ACCAdapter::new())
}

fn new_ams2_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(AMS2Adapter::new())
}

fn new_assetto_corsa_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(AssettoCorsaAdapter::new())
}

fn new_beamng_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(BeamNGAdapter::new())
}

fn new_forza_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(ForzaAdapter::new())
}

fn new_gran_turismo_7_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(GranTurismo7Adapter::new())
}

fn new_iracing_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(IRacingAdapter::new())
}

fn new_kartkraft_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(KartKraftAdapter::new())
}

fn new_lfs_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(LFSAdapter::new())
}

fn new_pcars2_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(PCars2Adapter::new())
}

fn new_raceroom_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(RaceRoomAdapter::new())
}

fn new_rbr_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(RBRAdapter::new())
}

fn new_rfactor2_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(RFactor2Adapter::new())
}

fn new_eawrc_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(EAWRCAdapter::new())
}

fn new_dirt5_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(Dirt5Adapter::new())
}

fn new_dirt_rally_2_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(DirtRally2Adapter::new())
}

fn new_f1_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(F1Adapter::new())
}

fn new_f1_25_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(F1_25Adapter::new())
}

fn new_wrc_generations_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(WrcGenerationsAdapter::new())
}

fn new_dirt4_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(Dirt4Adapter::new())
}

fn new_ets2_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(Ets2Adapter::with_variant(ets2::Ets2Variant::Ets2))
}

fn new_ats_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(Ets2Adapter::with_variant(ets2::Ets2Variant::Ats))
}

fn new_wreckfest_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(WreckfestAdapter::new())
}

fn new_automobilista_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(Automobilista1Adapter::new())
}

fn new_grid_autosport_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(GridAutosportAdapter::new())
}

fn new_grid_2019_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(Grid2019Adapter::new())
}

fn new_grid_legends_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(GridLegendsAdapter::new())
}

fn new_rennsport_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(RennsportAdapter::new())
}

fn new_nascar_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(NascarAdapter::new())
}

fn new_le_mans_ultimate_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(LeMansUltimateAdapter::new())
}

fn new_wtcr_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(WtcrAdapter::new())
}

fn new_trackmania_adapter() -> Box<dyn TelemetryAdapter> {
    Box::new(TrackmaniaAdapter::new())
}

/// Returns the canonical adapter factory registry for all supported native adapters.
pub fn adapter_factories() -> &'static [(&'static str, AdapterFactory)] {
    &[
        ("acc", new_acc_adapter),
        ("ac_rally", new_ac_rally_adapter),
        ("ams2", new_ams2_adapter),
        ("assetto_corsa", new_assetto_corsa_adapter),
        ("ats", new_ats_adapter),
        ("beamng_drive", new_beamng_adapter),
        ("dirt5", new_dirt5_adapter),
        ("dirt_rally_2", new_dirt_rally_2_adapter),
        ("dirt4", new_dirt4_adapter),
        ("eawrc", new_eawrc_adapter),
        ("ets2", new_ets2_adapter),
        ("f1", new_f1_adapter),
        ("f1_25", new_f1_25_adapter),
        ("forza_motorsport", new_forza_adapter),
        ("gran_turismo_7", new_gran_turismo_7_adapter),
        ("iracing", new_iracing_adapter),
        ("kartkraft", new_kartkraft_adapter),
        ("live_for_speed", new_lfs_adapter),
        ("project_cars_2", new_pcars2_adapter),
        ("raceroom", new_raceroom_adapter),
        ("rbr", new_rbr_adapter),
        ("automobilista", new_automobilista_adapter),
        ("grid_autosport", new_grid_autosport_adapter),
        ("grid_2019", new_grid_2019_adapter),
        ("grid_legends", new_grid_legends_adapter),
        ("rennsport", new_rennsport_adapter),
        ("rfactor2", new_rfactor2_adapter),
        ("wrc_generations", new_wrc_generations_adapter),
        ("wreckfest", new_wreckfest_adapter),
        ("nascar", new_nascar_adapter),
        ("le_mans_ultimate", new_le_mans_ultimate_adapter),
        ("wtcr", new_wtcr_adapter),
        ("trackmania", new_trackmania_adapter),
    ]
}

pub use ac_rally::ACRallyAdapter;
pub use acc::ACCAdapter;
pub use ams2::AMS2Adapter;
pub use assetto_corsa::AssettoCorsaAdapter;
pub use automobilista::Automobilista1Adapter;
pub use beamng::BeamNGAdapter;
pub use codemasters_udp::{CustomUdpSpec, DecodedCodemastersPacket, FieldSpec};
pub use dirt_rally_2::DirtRally2Adapter;
pub use dirt4::Dirt4Adapter;
pub use dirt5::Dirt5Adapter;
pub use eawrc::EAWRCAdapter;
pub use ets2::Ets2Adapter;
pub use f1::F1Adapter;
pub use f1_25::F1_25Adapter;
pub use forza::ForzaAdapter;
pub use gran_turismo_7::GranTurismo7Adapter;
pub use grid_2019::Grid2019Adapter;
pub use grid_autosport::GridAutosportAdapter;
pub use grid_legends::GridLegendsAdapter;
pub use iracing::IRacingAdapter;
pub use kartkraft::KartKraftAdapter;
pub use le_mans_ultimate::LeMansUltimateAdapter;
pub use lfs::LFSAdapter;
pub use nascar::NascarAdapter;
pub use pcars2::PCars2Adapter;
pub use raceroom::RaceRoomAdapter;
pub use rbr::RBRAdapter;
pub use rennsport::RennsportAdapter;
pub use rfactor2::RFactor2Adapter;
pub use trackmania::TrackmaniaAdapter;
pub use wrc_generations::WrcGenerationsAdapter;
pub use wreckfest::WreckfestAdapter;
pub use wtcr::WtcrAdapter;

/// Mock adapter for testing and deterministic fixture generation.
pub struct MockAdapter {
    game_id: String,
    update_rate: Duration,
    is_running: bool,
}

impl MockAdapter {
    pub fn new(game_id: String) -> Self {
        Self {
            game_id,
            update_rate: Duration::from_millis(16),
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

        let update_rate = self.update_rate;

        tokio::spawn(async move {
            let mut sequence = 0u64;

            loop {
                let timestamp_ns = telemetry_now_ns();
                let elapsed = std::time::Duration::from_nanos(timestamp_ns);
                let progress = (elapsed.as_secs_f32() % 10.0) / 10.0;
                let telemetry = generate_mock_telemetry(progress);

                let frame = TelemetryFrame::new(telemetry, timestamp_ns, sequence, 64);
                if tx.send(frame).await.is_err() {
                    break;
                }

                sequence += 1;
                tokio::time::sleep(update_rate).await;
            }
        });

        Ok(rx)
    }

    async fn stop_monitoring(&self) -> Result<()> {
        Ok(())
    }

    fn normalize(&self, _raw: &[u8]) -> Result<NormalizedTelemetry> {
        Ok(NormalizedTelemetry::builder().rpm(5000.0).build())
    }

    fn expected_update_rate(&self) -> Duration {
        self.update_rate
    }

    async fn is_game_running(&self) -> Result<bool> {
        Ok(self.is_running)
    }
}

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

    NormalizedTelemetry::builder()
        .ffb_scalar(ffb_scalar)
        .rpm(rpm.max(0.0))
        .speed_ms(speed)
        .slip_ratio(slip_ratio)
        .gear(gear)
        .car_id("mock_car".to_string())
        .track_id("mock_track".to_string())
        .build()
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
        let frame = tokio::time::timeout(std::time::Duration::from_millis(100), receiver.recv())
            .await?
            .ok_or("expected telemetry frame")?;

        assert!(frame.data.rpm > 0.0);
        assert!(frame.data.speed_ms > 0.0);
        assert_eq!(frame.data.car_id, Some("mock_car".to_string()));
        Ok(())
    }

    #[test]
    fn test_mock_telemetry_generation() -> TestResult {
        let telemetry = generate_mock_telemetry(0.5);

        assert!(telemetry.rpm > 0.0);
        assert!(telemetry.speed_ms > 0.0);
        assert!(telemetry.ffb_scalar > 0.0);
        assert!(telemetry.slip_ratio >= 0.0);
        Ok(())
    }
}
