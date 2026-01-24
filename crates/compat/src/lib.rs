//! Compatibility layer for schema migrations
//!
//! This crate provides compatibility traits to ease migration from old field names
//! to new field names in telemetry and configuration structs.

pub mod telemetry_compat;

pub use telemetry_compat::TelemetryCompat;
