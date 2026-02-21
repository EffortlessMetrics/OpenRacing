//! Game support matrix shim backed by shared telemetry metadata.
//!
//! This module is intentionally lightweight and delegates schema ownership to
//! `racing_wheel_telemetry_support`.

use std::collections::HashMap;

use racing_wheel_telemetry_support::load_default_matrix;
pub use racing_wheel_telemetry_support::{
    AutoDetectConfig, GameSupport, GameSupportMatrix, GameVersion, TelemetryFieldMapping,
    TelemetrySupport,
};
use tracing::warn;

/// Create the canonical default matrix from shared telemetry metadata.
pub fn create_default_matrix() -> GameSupportMatrix {
    load_default_matrix().unwrap_or_else(|err| {
        warn!(
            error = %err,
            "Failed to load default telemetry support matrix; falling back to empty matrix"
        );

        GameSupportMatrix {
            games: HashMap::new(),
        }
    })
}
