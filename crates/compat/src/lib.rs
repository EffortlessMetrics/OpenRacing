//! Compatibility layer for schema migrations
//! 
//! This crate provides compatibility traits to ease migration from old field names
//! to new field names in telemetry and configuration structs. It is gated with
//! #[cfg(test)] to ensure it never ships in release builds.

pub mod telemetry_compat;

pub use telemetry_compat::TelemetryCompat;