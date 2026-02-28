//! Prelude module for convenient diagnostic imports.
//!
//! This module re-exports the most commonly used types for
//! diagnostic recording and replay operations.
//!
//! # Example
//!
//! ```
//! use openracing_diagnostic::prelude::*;
//!
//! let config = BlackboxConfig::new("device-001", "./recordings");
//! ```

pub use crate::blackbox::{BlackboxConfig, BlackboxRecorder, RecordingStats};
pub use crate::error::{DiagnosticError, DiagnosticResult};
pub use crate::format::{IndexEntry, StreamType, WBB_MAGIC, WBB_VERSION, WbbFooter, WbbHeader};
pub use crate::replay::{
    BlackboxReplay, FrameComparison, ReplayConfig, ReplayResult, ReplayStatistics,
};
pub use crate::streams::{
    FrameData, HealthEventData, SafetyStateSimple, StreamARecord, StreamBRecord, StreamCRecord,
    StreamReader, TelemetryData,
};
pub use crate::support_bundle::{
    CpuInfo, HardwareInfo, MemoryInfo, OsInfo, ProcessInfo, SupportBundle, SupportBundleConfig,
    SystemInfo,
};
