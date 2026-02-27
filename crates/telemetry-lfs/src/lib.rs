//! Live for Speed (LFS) telemetry adapter crate.
//!
//! This crate provides the [`LFSAdapter`] for receiving telemetry from
//! **Live for Speed** via the OutGauge UDP protocol (default port 30000).
//!
//! # Protocol
//!
//! LFS exposes a 96-byte OutGauge UDP packet containing dashboard and
//! physics data. Fields extracted:
//!
//! | Field     | Offset | Type |
//! |-----------|--------|------|
//! | Gear      | 10     | u8   |
//! | Speed     | 12     | f32 (m/s) |
//! | RPM       | 16     | f32  |
//! | Fuel      | 28     | f32 (0-1) |
//! | Throttle  | 48     | f32 (0-1) |
//! | Brake     | 52     | f32 (0-1) |
//! | Clutch    | 56     | f32 (0-1) |
//!
//! Gear encoding: `0=Reverse → -1`, `1=Neutral → 0`, `2=1st → 1`, etc.
//!
//! # Usage
//!
//! ```rust,no_run
//! use racing_wheel_telemetry_lfs::LFSAdapter;
//! use racing_wheel_telemetry_adapters::TelemetryAdapter;
//!
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! let adapter = LFSAdapter::new().with_port(30000);
//! assert_eq!(adapter.game_id(), "live_for_speed");
//! # Ok(())
//! # }
//! ```

#![deny(static_mut_refs)]

pub use racing_wheel_telemetry_adapters::lfs::LFSAdapter;
pub use racing_wheel_telemetry_adapters::TelemetryAdapter;
pub use racing_wheel_telemetry_core::{NormalizedTelemetry, TelemetryFrame};
