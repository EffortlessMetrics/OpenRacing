//! Diagnostic recording and replay system for OpenRacing
//!
//! This crate provides comprehensive diagnostic capabilities including:
//!
//! - **Blackbox Recording**: High-performance recording of RT frames at 1kHz
//! - **Replay System**: Deterministic replay with floating-point tolerance validation
//! - **Support Bundle**: Comprehensive diagnostic packages for troubleshooting
//!
//! # Architecture
//!
//! The crate is organized into several modules:
//!
//! - [`blackbox`]: .wbb format recording with Stream A/B/C support
//! - [`replay`]: Deterministic replay with validation
//! - [`support_bundle`]: ZIP-based diagnostic packages
//! - [`streams`]: Stream implementations for different data rates
//! - [`format`]: File format definitions and constants
//! - [`error`]: Error types for diagnostic operations
//!
//! # RT Safety
//!
//! The recording path is designed for RT hot path:
//! - Stream A recording uses pre-allocated buffers
//! - No heap allocations in `record_frame` hot path
//! - File I/O is deferred to finalization (non-RT)
//!
//! # File Format Stability
//!
//! The .wbb v1 format is designed for long-term stability:
//! - Magic number validation
//! - Version field for future extensions
//! - Backward-compatible header structure
//!
//! # Example
//!
//! ```no_run
//! use openracing_diagnostic::prelude::*;
//!
//! # fn main() -> DiagnosticResult<()> {
//! // Create blackbox recorder
//! let config = BlackboxConfig::new("device-001", "./recordings");
//! let mut recorder = BlackboxRecorder::new(config)?;
//!
//! // Record frames in RT hot path
//! let frame = FrameData {
//!     ffb_in: 0.5,
//!     torque_out: 0.3,
//!     wheel_speed: 10.0,
//!     hands_off: false,
//!     ts_mono_ns: 1_000_000_000,
//!     seq: 1,
//! };
//!
//! recorder.record_frame(frame, &[0.1, 0.2], SafetyStateSimple::SafeTorque, 150)?;
//!
//! // Finalize recording
//! let output_path = recorder.finalize()?;
//!
//! // Replay for validation
//! let replay_config = ReplayConfig::default();
//! let mut replay = BlackboxReplay::load_from_file(&output_path, replay_config)?;
//! let result = replay.execute_replay()?;
//!
//! println!("Replay accuracy: {:.2}%",
//!     (result.frames_matched as f64 / result.frames_replayed as f64) * 100.0);
//!
//! # Ok(())
//! # }
//! ```

#![deny(unsafe_op_in_unsafe_fn, clippy::unwrap_used)]
#![warn(missing_docs, rust_2018_idioms)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod blackbox;
pub mod error;
pub mod format;
pub mod prelude;
pub mod replay;
pub mod streams;
pub mod support_bundle;

pub use blackbox::{BlackboxConfig, BlackboxRecorder, RecordingStats};
pub use error::{DiagnosticError, DiagnosticResult};
pub use format::{IndexEntry, StreamType, WbbFooter, WbbHeader};
pub use replay::{BlackboxReplay, ReplayConfig, ReplayResult};
pub use streams::{
    FrameData, HealthEventData, SafetyStateSimple, StreamA, StreamB, StreamC, StreamReader,
    TelemetryData,
};
pub use support_bundle::{SupportBundle, SupportBundleConfig};

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_config() -> (BlackboxConfig, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = BlackboxConfig::new("test-device", temp_dir.path());
        (config, temp_dir)
    }

    #[test]
    fn test_complete_workflow() {
        let (config, _temp_dir) = create_test_config();
        let mut recorder = BlackboxRecorder::new(config).unwrap();

        for i in 0..10 {
            let frame = FrameData {
                ffb_in: i as f32 * 0.1,
                torque_out: i as f32 * 0.05,
                wheel_speed: i as f32,
                hands_off: false,
                ts_mono_ns: (i * 1_000_000) as u64,
                seq: i as u16,
            };

            recorder
                .record_frame(
                    frame,
                    &[i as f32 * 0.01],
                    SafetyStateSimple::SafeTorque,
                    100,
                )
                .unwrap();
        }

        let output_path = recorder.finalize().unwrap();
        assert!(output_path.exists());

        let replay_config = ReplayConfig::default();
        let mut replay = BlackboxReplay::load_from_file(&output_path, replay_config).unwrap();
        let result = replay.execute_replay().unwrap();

        assert!(result.frames_replayed > 0);
    }

    #[test]
    fn test_support_bundle_workflow() {
        let temp_dir = TempDir::new().unwrap();

        let config = SupportBundleConfig::default();
        let mut bundle = SupportBundle::new(config);

        let event = HealthEventData {
            timestamp_ns: 0,
            device_id: "test-device".to_string(),
            event_type: "DeviceConnected".to_string(),
            context: serde_json::json!({"test": true}),
        };

        bundle.add_health_events(&[event]).unwrap();
        bundle.add_system_info().unwrap();

        let bundle_path = temp_dir.path().join("test_bundle.zip");
        bundle.generate(&bundle_path).unwrap();

        assert!(bundle_path.exists());
    }
}
