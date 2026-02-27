//! Automobilista 2 (AMS2) telemetry adapter crate.
//!
//! This crate provides the [`AMS2Adapter`] for receiving telemetry from
//! **Automobilista 2** via the Project CARS 2 shared memory interface.
//!
//! # Protocol
//!
//! AMS2 uses the same shared memory layout as Project CARS 2 (`$pcars2$`),
//! mapped at `Local\$pcars2$` on Windows. The adapter reads timestamped
//! participant data at ~60 Hz and normalizes it to [`NormalizedTelemetry`].
//!
//! Fields extracted: speed, RPM, gear, throttle, brake, steering, g-forces,
//! and tyre slip data.
//!
//! # Usage
//!
//! ```rust,no_run
//! use racing_wheel_telemetry_ams2::AMS2Adapter;
//! use racing_wheel_telemetry_adapters::TelemetryAdapter;
//!
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! let adapter = AMS2Adapter::new();
//! assert_eq!(adapter.game_id(), "ams2");
//! # Ok(())
//! # }
//! ```

#![deny(static_mut_refs)]

pub use racing_wheel_telemetry_adapters::ams2::AMS2Adapter;
pub use racing_wheel_telemetry_adapters::TelemetryAdapter;
pub use racing_wheel_telemetry_core::{NormalizedTelemetry, TelemetryFrame};
