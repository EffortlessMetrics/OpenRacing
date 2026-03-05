//! Shared library for openracing-capture-ids.
//!
//! Exposes the replay module, vendor decode functions, protocol analysis,
//! timing analysis, and replay validation helpers for integration testing.

#![deny(static_mut_refs)]

pub mod replay;

/// Decode a raw HID report for a known vendor.
///
/// Returns a human-readable description when the VID and report format are
/// recognised; `None` for unknown vendors or unrecognised report layouts.
pub fn decode_report(vid: u16, data: &[u8]) -> Option<String> {
    match vid {
        0x346E => decode_moza_report(data),
        0x046D => decode_logitech_report(data),
        _ => None,
    }
}

fn decode_moza_report(data: &[u8]) -> Option<String> {
    let input = racing_wheel_moza_wheelbase_report::parse_wheelbase_input_report(data)?;
    Some(format!(
        "MOZA: steering={:.3} throttle={:.3} brake={:.3}",
        input.steering as f32 / 65535.0,
        input.pedals.throttle as f32 / 65535.0,
        input.pedals.brake as f32 / 65535.0,
    ))
}

fn decode_logitech_report(data: &[u8]) -> Option<String> {
    let state = racing_wheel_hid_logitech_protocol::parse_input_report(data)?;
    Some(format!(
        "Logitech: steering={:.3} throttle={:.3} brake={:.3} buttons={:04X}",
        state.steering, state.throttle, state.brake, state.buttons,
    ))
}

pub fn hex_u16(v: u16) -> String {
    format!("0x{v:04X}")
}

/// Parse a VID/PID string in hex (`0x1234`) or decimal (`4660`) form.
pub fn parse_hex_id(raw: &str) -> anyhow::Result<u16> {
    use anyhow::Context;
    let raw = raw.trim();
    let digits = if raw.starts_with("0x") || raw.starts_with("0X") {
        &raw[2..]
    } else {
        raw
    };
    u16::from_str_radix(digits, 16)
        .or_else(|_| raw.parse::<u16>())
        .with_context(|| format!("invalid ID value '{raw}', expected hex (0x1234) or decimal"))
}

// ── Vendor detection ─────────────────────────────────────────────────────────

/// Known vendor classification from VID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectedVendor {
    Moza,
    Logitech,
    Fanatec,
    Thrustmaster,
    Unknown,
}

/// Detect the vendor from a numeric VID.
pub fn detect_vendor_from_vid(vid: u16) -> DetectedVendor {
    match vid {
        0x346E => DetectedVendor::Moza,
        0x046D => DetectedVendor::Logitech,
        0x0EB7 => DetectedVendor::Fanatec,
        0x044F => DetectedVendor::Thrustmaster,
        _ => DetectedVendor::Unknown,
    }
}

/// Detect the vendor from a VID hex string.
pub fn detect_vendor_from_vid_str(vid_str: &str) -> DetectedVendor {
    match replay::parse_vid_str(vid_str) {
        Ok(vid) => detect_vendor_from_vid(vid),
        Err(_) => DetectedVendor::Unknown,
    }
}

/// Return a human-readable label for a [`DetectedVendor`].
pub fn vendor_label(vendor: DetectedVendor) -> &'static str {
    match vendor {
        DetectedVendor::Moza => "MOZA Racing",
        DetectedVendor::Logitech => "Logitech",
        DetectedVendor::Fanatec => "Fanatec",
        DetectedVendor::Thrustmaster => "Thrustmaster",
        DetectedVendor::Unknown => "Unknown",
    }
}

// ── Timing analysis for captured reports ─────────────────────────────────────

/// Statistical summary of inter-report timing from a capture stream.
#[derive(Debug, Clone)]
pub struct CaptureTimingStats {
    /// Number of inter-report intervals.
    pub interval_count: usize,
    /// Mean interval in nanoseconds.
    pub mean_ns: f64,
    /// Median interval in nanoseconds.
    pub median_ns: f64,
    /// Minimum interval in nanoseconds.
    pub min_ns: f64,
    /// Maximum interval in nanoseconds.
    pub max_ns: f64,
    /// Jitter (max − min) in nanoseconds.
    pub jitter_ns: f64,
    /// P99 interval in nanoseconds.
    pub p99_ns: f64,
    /// Standard deviation in nanoseconds.
    pub std_dev_ns: f64,
    /// Estimated report rate in Hz.
    pub estimated_rate_hz: f64,
}

/// Compute timing statistics from a slice of [`replay::CapturedReport`]s.
///
/// Returns `None` when fewer than 2 reports are provided.
pub fn analyze_capture_timing(reports: &[replay::CapturedReport]) -> Option<CaptureTimingStats> {
    if reports.len() < 2 {
        return None;
    }

    let mut intervals: Vec<f64> = reports
        .windows(2)
        .map(|w| (w[1].ts_ns as f64) - (w[0].ts_ns as f64))
        .collect();

    intervals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let count = intervals.len();
    let sum: f64 = intervals.iter().sum();
    let mean = sum / count as f64;
    let min = intervals[0];
    let max = intervals[count - 1];
    let median = if count % 2 == 0 {
        (intervals[count / 2 - 1] + intervals[count / 2]) / 2.0
    } else {
        intervals[count / 2]
    };

    let p99_idx = ((count as f64) * 0.99).ceil() as usize;
    let p99 = intervals[p99_idx.min(count - 1)];

    let variance: f64 = intervals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / count as f64;
    let std_dev = variance.sqrt();

    let estimated_rate_hz = if mean > 0.0 {
        1_000_000_000.0 / mean
    } else {
        0.0
    };

    Some(CaptureTimingStats {
        interval_count: count,
        mean_ns: mean,
        median_ns: median,
        min_ns: min,
        max_ns: max,
        jitter_ns: max - min,
        p99_ns: p99,
        std_dev_ns: std_dev,
        estimated_rate_hz,
    })
}

// ── Replay validation pipeline ───────────────────────────────────────────────

/// Result of running captured reports through the protocol decode pipeline.
#[derive(Debug, Clone)]
pub struct ReplayValidationResult {
    /// Total reports processed.
    pub total_reports: usize,
    /// Reports that decoded successfully.
    pub decoded_count: usize,
    /// Reports that could not be decoded (unknown vendor or bad data).
    pub failed_count: usize,
    /// Distinct VIDs seen in the capture.
    pub distinct_vids: Vec<u16>,
}

/// Run a stream of captured reports through the protocol decoders and report
/// decode success/failure statistics.
pub fn validate_replay_pipeline(
    reports: &[replay::CapturedReport],
) -> anyhow::Result<ReplayValidationResult> {
    let mut decoded_count = 0usize;
    let mut failed_count = 0usize;
    let mut vids = std::collections::HashSet::new();

    for entry in reports {
        let vid = replay::parse_vid_str(&entry.vid).unwrap_or(0);
        vids.insert(vid);

        let bytes = match replay::decode_hex(&entry.report) {
            Ok(b) => b,
            Err(_) => {
                failed_count += 1;
                continue;
            }
        };

        if decode_report(vid, &bytes).is_some() {
            decoded_count += 1;
        } else {
            failed_count += 1;
        }
    }

    let mut distinct_vids: Vec<u16> = vids.into_iter().collect();
    distinct_vids.sort();

    Ok(ReplayValidationResult {
        total_reports: reports.len(),
        decoded_count,
        failed_count,
        distinct_vids,
    })
}
