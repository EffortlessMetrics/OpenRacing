//! MudRunner / SnowRunner SimHub UDP bridge telemetry adapter crate.
//!
//! This crate provides the [MudRunnerAdapter] and [MudRunnerVariant] for
//! receiving telemetry from **MudRunner** and **SnowRunner** via a SimHub
//! UDP JSON bridge (port 8877).
//!
//! # Usage
//!
//! `rust,no_run
//! use racing_wheel_telemetry_mudrunner::{MudRunnerAdapter, MudRunnerVariant};
//! use racing_wheel_telemetry_adapters::TelemetryAdapter;
//!
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! let adapter = MudRunnerAdapter::with_variant(MudRunnerVariant::SnowRunner);
//! assert_eq!(adapter.game_id(), "snowrunner");
//! # Ok(())
//! # }
//! `

#![deny(static_mut_refs)]

pub use racing_wheel_telemetry_adapters::TelemetryAdapter;
pub use racing_wheel_telemetry_adapters::mudrunner::{MudRunnerAdapter, MudRunnerVariant};
pub use racing_wheel_telemetry_core::{NormalizedTelemetry, TelemetryFrame};
