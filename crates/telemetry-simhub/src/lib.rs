//! SimHub UDP JSON bridge telemetry adapter crate.
//!
//! This crate provides the [SimHubAdapter] for receiving generic JSON
//! telemetry from **SimHub** (SHWotever) over UDP (default port 5555).
//!
//! # Protocol
//!
//! SimHub broadcasts normalised telemetry as UTF-8 JSON packets at ~60 Hz.
//! The adapter parses `SpeedMs`, `Rpm`, `Gear`, `Throttle`, `Brake`,
//! `Steer`, `LatAcc`, `LonAcc`, `FFBValue`, and `FuelPercent` fields.
//!
//! # Usage
//!
//! `rust,no_run
//! use racing_wheel_telemetry_simhub::SimHubAdapter;
//! use racing_wheel_telemetry_adapters::TelemetryAdapter;
//!
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! let adapter = SimHubAdapter::new();
//! assert_eq!(adapter.game_id(), "simhub");
//! # Ok(())
//! # }
//! `

#![deny(static_mut_refs)]

pub use racing_wheel_telemetry_adapters::TelemetryAdapter;
pub use racing_wheel_telemetry_adapters::simhub::SimHubAdapter;
pub use racing_wheel_telemetry_core::{NormalizedTelemetry, TelemetryFrame};
