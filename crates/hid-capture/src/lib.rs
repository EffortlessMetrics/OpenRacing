//! Shared library for hid-capture.
//!
//! Exposes data types, parsing helpers, timing analysis, protocol detection,
//! and capture format utilities for integration testing and community sharing.

#![deny(static_mut_refs)]

use serde::{Deserialize, Serialize};

pub fn parse_hex_u16(s: &str) -> Result<u16, String> {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    u16::from_str_radix(s, 16).map_err(|e| format!("invalid hex value '{s}': {e}"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureReport {
    pub timestamp_us: u64,
    pub report_id: u8,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureFile {
    pub vendor_id: String,
    pub product_id: String,
    pub captures: Vec<CaptureReport>,
}

// ── Capture metadata for community sharing format ────────────────────────────

/// Metadata for the community sharing capture format (versioned).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CaptureMetadata {
    /// Format version string, e.g. "1.0".
    pub format_version: String,
    /// ISO-8601 timestamp when the capture was recorded.
    pub captured_at: String,
    /// Platform the capture was taken on (e.g. "windows", "linux", "macos").
    pub platform: String,
    /// Freeform tool name / version that produced the capture.
    pub tool_version: String,
    /// Optional description provided by the user.
    pub description: String,
}

/// Community-sharing capture file with versioned metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedCaptureFile {
    pub metadata: CaptureMetadata,
    pub vendor_id: String,
    pub product_id: String,
    pub captures: Vec<CaptureReport>,
}

// ── Timing analysis ──────────────────────────────────────────────────────────

/// Statistical summary of inter-report timing from a capture session.
#[derive(Debug, Clone, PartialEq)]
pub struct TimingStats {
    /// Number of inter-report intervals analysed.
    pub count: usize,
    /// Mean interval in microseconds.
    pub mean_us: f64,
    /// Median interval in microseconds.
    pub median_us: f64,
    /// Minimum interval in microseconds.
    pub min_us: f64,
    /// Maximum interval in microseconds.
    pub max_us: f64,
    /// Standard deviation in microseconds.
    pub std_dev_us: f64,
    /// P99 interval in microseconds.
    pub p99_us: f64,
    /// Jitter: max - min interval in microseconds.
    pub jitter_us: f64,
    /// Estimated capture rate in Hz.
    pub estimated_rate_hz: f64,
}

/// Compute timing statistics from a slice of [`CaptureReport`]s.
///
/// Returns `None` if fewer than 2 reports are provided.
pub fn compute_timing_stats(captures: &[CaptureReport]) -> Option<TimingStats> {
    if captures.len() < 2 {
        return None;
    }

    let mut intervals: Vec<f64> = captures
        .windows(2)
        .map(|w| (w[1].timestamp_us as f64) - (w[0].timestamp_us as f64))
        .collect();

    intervals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let count = intervals.len();
    let sum: f64 = intervals.iter().sum();
    let mean = sum / count as f64;
    let min = intervals[0];
    let max = intervals[count - 1];
    let median = if count.is_multiple_of(2) {
        (intervals[count / 2 - 1] + intervals[count / 2]) / 2.0
    } else {
        intervals[count / 2]
    };

    let p99_idx = ((count as f64) * 0.99).ceil() as usize;
    let p99 = intervals[p99_idx.min(count - 1)];

    let variance: f64 = intervals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / count as f64;
    let std_dev = variance.sqrt();

    let estimated_rate_hz = if mean > 0.0 { 1_000_000.0 / mean } else { 0.0 };

    Some(TimingStats {
        count,
        mean_us: mean,
        median_us: median,
        min_us: min,
        max_us: max,
        std_dev_us: std_dev,
        p99_us: p99,
        jitter_us: max - min,
        estimated_rate_hz,
    })
}

/// Check that all timestamps in a capture are monotonically increasing.
///
/// Returns the index of the first violation, or `None` if all timestamps are
/// monotonic.
pub fn validate_monotonic_timestamps(captures: &[CaptureReport]) -> Option<usize> {
    captures
        .windows(2)
        .position(|w| w[1].timestamp_us <= w[0].timestamp_us)
        .map(|i| i + 1)
}

/// Filter captures to only those matching a specific `report_id`.
pub fn filter_by_report_id(captures: &[CaptureReport], report_id: u8) -> Vec<&CaptureReport> {
    captures
        .iter()
        .filter(|c| c.report_id == report_id)
        .collect()
}

/// Compute the total capture duration in microseconds. Returns 0 for empty/single-element captures.
pub fn capture_duration_us(captures: &[CaptureReport]) -> u64 {
    match (captures.first(), captures.last()) {
        (Some(first), Some(last)) => last.timestamp_us.saturating_sub(first.timestamp_us),
        _ => 0,
    }
}

// ── Vendor / protocol detection ──────────────────────────────────────────────

/// Known HID racing wheel vendor IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnownVendor {
    Moza,
    Logitech,
    Fanatec,
    Thrustmaster,
    Simagic,
    Simucube,
    CammusDirect,
    AccuForce,
    VRS,
    Heusinkveld,
}

/// Attempt to identify a known racing wheel vendor from a VID string.
///
/// Returns `None` for unrecognised vendor IDs.
pub fn detect_vendor(vid_str: &str) -> Option<KnownVendor> {
    let vid = parse_hex_u16(vid_str).ok()?;
    detect_vendor_by_id(vid)
}

/// Identify a known racing wheel vendor from a numeric VID.
pub fn detect_vendor_by_id(vid: u16) -> Option<KnownVendor> {
    match vid {
        0x346E => Some(KnownVendor::Moza),
        0x046D => Some(KnownVendor::Logitech),
        0x0EB7 => Some(KnownVendor::Fanatec),
        0x044F => Some(KnownVendor::Thrustmaster),
        0x0483 => Some(KnownVendor::Simagic),
        0x16D0 => Some(KnownVendor::Simucube),
        0x3416 => Some(KnownVendor::CammusDirect),
        0x1FC9 => Some(KnownVendor::AccuForce),
        0x35F0 => Some(KnownVendor::VRS),
        0x04D8 => Some(KnownVendor::Heusinkveld),
        _ => None,
    }
}

/// Return a human-readable vendor name for a known vendor.
pub fn vendor_name(vendor: KnownVendor) -> &'static str {
    match vendor {
        KnownVendor::Moza => "MOZA Racing",
        KnownVendor::Logitech => "Logitech",
        KnownVendor::Fanatec => "Fanatec",
        KnownVendor::Thrustmaster => "Thrustmaster",
        KnownVendor::Simagic => "Simagic",
        KnownVendor::Simucube => "Simucube",
        KnownVendor::CammusDirect => "Cammus",
        KnownVendor::AccuForce => "AccuForce",
        KnownVendor::VRS => "VRS DirectForce",
        KnownVendor::Heusinkveld => "Heusinkveld",
    }
}

// ── Capture format validation ────────────────────────────────────────────────

/// Errors that can occur when validating a capture file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureValidationError {
    /// VID string could not be parsed.
    InvalidVendorId(String),
    /// PID string could not be parsed.
    InvalidProductId(String),
    /// Timestamp at the given index is not monotonically increasing.
    NonMonotonicTimestamp { index: usize },
    /// Metadata format version is unsupported.
    UnsupportedFormatVersion(String),
}

/// Validate a [`CaptureFile`] for structural correctness.
pub fn validate_capture_file(file: &CaptureFile) -> Vec<CaptureValidationError> {
    let mut errors = Vec::new();

    if parse_hex_u16(&file.vendor_id).is_err() {
        errors.push(CaptureValidationError::InvalidVendorId(
            file.vendor_id.clone(),
        ));
    }
    if parse_hex_u16(&file.product_id).is_err() {
        errors.push(CaptureValidationError::InvalidProductId(
            file.product_id.clone(),
        ));
    }
    if let Some(idx) = validate_monotonic_timestamps(&file.captures) {
        errors.push(CaptureValidationError::NonMonotonicTimestamp { index: idx });
    }

    errors
}

/// Validate a [`SharedCaptureFile`] including metadata checks.
pub fn validate_shared_capture_file(file: &SharedCaptureFile) -> Vec<CaptureValidationError> {
    let mut errors = Vec::new();

    if file.metadata.format_version != "1.0" {
        errors.push(CaptureValidationError::UnsupportedFormatVersion(
            file.metadata.format_version.clone(),
        ));
    }
    if parse_hex_u16(&file.vendor_id).is_err() {
        errors.push(CaptureValidationError::InvalidVendorId(
            file.vendor_id.clone(),
        ));
    }
    if parse_hex_u16(&file.product_id).is_err() {
        errors.push(CaptureValidationError::InvalidProductId(
            file.product_id.clone(),
        ));
    }
    if let Some(idx) = validate_monotonic_timestamps(&file.captures) {
        errors.push(CaptureValidationError::NonMonotonicTimestamp { index: idx });
    }

    errors
}

/// Convert a basic [`CaptureFile`] to the community [`SharedCaptureFile`] format.
pub fn to_shared_format(
    file: &CaptureFile,
    platform: &str,
    tool_version: &str,
    captured_at: &str,
    description: &str,
) -> SharedCaptureFile {
    SharedCaptureFile {
        metadata: CaptureMetadata {
            format_version: "1.0".to_string(),
            captured_at: captured_at.to_string(),
            platform: platform.to_string(),
            tool_version: tool_version.to_string(),
            description: description.to_string(),
        },
        vendor_id: file.vendor_id.clone(),
        product_id: file.product_id.clone(),
        captures: file
            .captures
            .iter()
            .map(|c| CaptureReport {
                timestamp_us: c.timestamp_us,
                report_id: c.report_id,
                data: c.data.clone(),
            })
            .collect(),
    }
}
