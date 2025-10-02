//! Game Telemetry Adapters Module
//! 
//! Implements task 8: Game telemetry adapters with rate limiting
//! Requirements: GI-03, GI-04

pub mod adapters;
pub mod normalized;
pub mod rate_limiter;
pub mod recorder;

pub use adapters::*;
pub use normalized::*;
pub use rate_limiter::*;
pub use recorder::*;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::mpsc;

/// Telemetry adapter trait for game-specific telemetry sources
#[async_trait]
pub trait TelemetryAdapter: Send + Sync {
    /// Get the game identifier this adapter supports
    fn game_id(&self) -> &str;
    
    /// Start monitoring telemetry from the game
    async fn start_monitoring(&self) -> Result<TelemetryReceiver>;
    
    /// Stop monitoring telemetry
    async fn stop_monitoring(&self) -> Result<()>;
    
    /// Normalize raw telemetry data to common format
    fn normalize(&self, raw: &[u8]) -> Result<NormalizedTelemetry>;
    
    /// Get the expected update rate for this adapter
    fn expected_update_rate(&self) -> Duration;
    
    /// Check if the game is currently running
    async fn is_game_running(&self) -> Result<bool>;
}

/// Telemetry receiver for streaming telemetry data
pub type TelemetryReceiver = mpsc::Receiver<TelemetryFrame>;

/// Telemetry frame with timing information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryFrame {
    /// Normalized telemetry data
    pub data: NormalizedTelemetry,
    
    /// Timestamp when frame was received (monotonic)
    pub timestamp_ns: u64,
    
    /// Sequence number for ordering
    pub sequence: u64,
    
    /// Raw data size for diagnostics
    pub raw_size: usize,
}

impl TelemetryFrame {
    /// Create a new telemetry frame
    pub fn new(data: NormalizedTelemetry, timestamp_ns: u64, sequence: u64, raw_size: usize) -> Self {
        Self {
            data,
            timestamp_ns,
            sequence,
            raw_size,
        }
    }
}

/// Telemetry service that manages multiple adapters
pub struct TelemetryService {
    adapters: std::collections::HashMap<String, Box<dyn TelemetryAdapter>>,
    #[allow(dead_code)]
    rate_limiter: RateLimiter,
    recorder: Option<TelemetryRecorder>,
}

impl TelemetryService {
    /// Create a new telemetry service
    pub fn new() -> Self {
        let mut adapters: std::collections::HashMap<String, Box<dyn TelemetryAdapter>> = std::collections::HashMap::new();
        
        // Register adapters
        adapters.insert("iracing".to_string(), Box::new(IRacingAdapter::new()));
        adapters.insert("acc".to_string(), Box::new(ACCAdapter::new()));
        
        Self {
            adapters,
            rate_limiter: RateLimiter::new(1000), // 1kHz max rate to protect RT thread
            recorder: None,
        }
    }
    
    /// Start telemetry monitoring for a specific game
    pub async fn start_monitoring(&mut self, game_id: &str) -> Result<TelemetryReceiver> {
        let adapter = self.adapters.get(game_id)
            .ok_or_else(|| anyhow::anyhow!("No adapter for game: {}", game_id))?;
        
        let receiver = adapter.start_monitoring().await?;
        Ok(receiver)
    }
    
    /// Stop telemetry monitoring for a specific game
    pub async fn stop_monitoring(&self, game_id: &str) -> Result<()> {
        let adapter = self.adapters.get(game_id)
            .ok_or_else(|| anyhow::anyhow!("No adapter for game: {}", game_id))?;
        
        adapter.stop_monitoring().await
    }
    
    /// Enable telemetry recording for CI testing
    pub fn enable_recording(&mut self, output_path: std::path::PathBuf) -> Result<()> {
        self.recorder = Some(TelemetryRecorder::new(output_path)?);
        Ok(())
    }
    
    /// Disable telemetry recording
    pub fn disable_recording(&mut self) {
        self.recorder = None;
    }
    
    /// Get list of supported games
    pub fn supported_games(&self) -> Vec<String> {
        self.adapters.keys().cloned().collect()
    }
    
    /// Check if a game is currently running
    pub async fn is_game_running(&self, game_id: &str) -> Result<bool> {
        let adapter = self.adapters.get(game_id)
            .ok_or_else(|| anyhow::anyhow!("No adapter for game: {}", game_id))?;
        
        adapter.is_game_running().await
    }
}