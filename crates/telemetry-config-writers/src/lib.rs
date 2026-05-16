//! Configuration writers for game-specific telemetry setup

#![deny(static_mut_refs)]

mod path;
mod registry;
mod types;
mod writers;

pub use registry::{ConfigWriterFactory, config_writer_factories};
pub use types::{ConfigDiff, ConfigWriter, DiffOperation, TelemetryConfig};
pub use writers::*;
