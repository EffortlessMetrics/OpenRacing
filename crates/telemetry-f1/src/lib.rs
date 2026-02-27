//! F1 2023/2024 native UDP telemetry adapter crate.
//!
//! This crate provides the [`F1NativeAdapter`] for receiving telemetry from
//! **EA F1 23** and **EA F1 24** via the Codemasters binary UDP protocol.
//!
//! # Protocol
//!
//! Both F1 23 (packet format `2023`) and F1 24 (packet format `2024`) send
//! little-endian binary UDP packets on port **20777** by default.  The packet
//! format is auto-detected from each packet header.
//!
//! ## Key packet types
//!
//! | Packet ID | Name          | Fields extracted                           |
//! |-----------|---------------|--------------------------------------------|
//! | 1         | Session        | track ID, session type, temperatures       |
//! | 6         | Car Telemetry  | speed (km/hΓåÆm/s), gear, RPM, throttle,    |
//! |           |               | brake, steer, DRS, tyre pressures/temps    |
//! | 7         | Car Status     | fuel (kg), ERS (J), pit limiter,          |
//! |           |               | tyre compound, traction control, ABS       |
//!
//! ## Protocol differences between F1 23 and F1 24
//!
//! The header and CarTelemetry layouts are **identical** in both versions.
//! CarStatusData differs:
//! - F1 23: 47 bytes per car ΓÇö no engine-power fields.
//! - F1 24: 55 bytes per car ΓÇö adds `enginePowerICE` and `enginePowerMGUK`.
//!
//! # Usage
//!
//! ```rust,no_run
//! use racing_wheel_telemetry_f1::F1NativeAdapter;
//! use racing_wheel_telemetry_adapters::TelemetryAdapter;
//!
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! let adapter = F1NativeAdapter::new();
//! assert_eq!(adapter.game_id(), "f1_native");
//! # Ok(())
//! # }
//! ```

#![deny(static_mut_refs)]

pub use racing_wheel_telemetry_adapters::TelemetryAdapter;
pub use racing_wheel_telemetry_adapters::f1_native::F1NativeAdapter;
pub use racing_wheel_telemetry_core::{NormalizedTelemetry, TelemetryFrame};
