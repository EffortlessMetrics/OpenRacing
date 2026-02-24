//! Game support matrix, utilities, and config writers for OpenRacing telemetry.
//!
//! This crate combines:
//! - Game support matrix metadata and utilities
//! - Game-specific configuration writers

pub mod support;
pub mod writers;

pub use support::{
    load_default_matrix, matrix_game_id_set, matrix_game_ids, normalize_game_id,
    AutoDetectConfig, GameSupport, GameSupportMatrix, GameSupportStatus, TelemetryFieldMapping,
    TelemetrySupport, GameVersion, TELEMETRY_SUPPORT_MATRIX_YAML,
};
pub use writers::{
    config_writer_factories, ConfigDiff, ConfigWriter, ConfigWriterFactory, TelemetryConfig,
    DiffOperation, ACCConfigWriter, ACRallyConfigWriter, AMS2ConfigWriter, Dirt5ConfigWriter,
    EAWRCConfigWriter, F1ConfigWriter, F1_25ConfigWriter, IRacingConfigWriter, RFactor2ConfigWriter,
};
