//! Game support matrix, utilities, and config writers for OpenRacing telemetry.
//!
//! This crate combines:
//! - Game support matrix metadata and utilities
//! - Game-specific configuration writers

#![deny(static_mut_refs)]

pub mod support;
pub mod writers;

pub use support::{
    AutoDetectConfig, GameSupport, GameSupportMatrix, GameSupportStatus, GameVersion,
    TELEMETRY_SUPPORT_MATRIX_YAML, TelemetryFieldMapping, TelemetrySupport, load_default_matrix,
    matrix_game_id_set, matrix_game_ids, normalize_game_id,
};
pub use writers::{
    ACCConfigWriter, ACRallyConfigWriter, AMS2ConfigWriter, AssettoCorsaConfigWriter,
    BeamNGDriveConfigWriter, ConfigDiff, ConfigWriter, ConfigWriterFactory, DiffOperation,
    Dirt5ConfigWriter, DirtRally2ConfigWriter, EAWRCConfigWriter, F1_25ConfigWriter,
    F1ConfigWriter, ForzaMotorsportConfigWriter, GranTurismo7ConfigWriter, IRacingConfigWriter,
    RBRConfigWriter, RFactor2ConfigWriter, TelemetryConfig, config_writer_factories,
};
