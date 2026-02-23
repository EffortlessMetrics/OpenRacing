//! Moza HID report layout constants and zero-copy report views.

#![deny(static_mut_refs)]

/// Report ID and axis offsets for aggregated wheelbase input reports.
///
/// These offsets are based on the current Moza protocol document in this
/// repository and should be validated against per-firmware capture traces.
pub mod input_report {
    pub const REPORT_ID: u8 = 0x01;
    pub const STEERING_START: usize = 1;
    pub const THROTTLE_START: usize = 3;
    pub const BRAKE_START: usize = 5;
    pub const CLUTCH_START: usize = 7;
    pub const HANDBRAKE_START: usize = 9;
    pub const BUTTONS_START: usize = 11;
    pub const BUTTONS_LEN: usize = 16;
    pub const HAT_START: usize = BUTTONS_START + BUTTONS_LEN;
    pub const FUNKY_START: usize = HAT_START + 1;
    pub const ROTARY_START: usize = FUNKY_START + 1;
    pub const ROTARY_LEN: usize = 2;
}

/// Best-effort layouts for direct USB HBP handbrake reports.
pub mod hbp_report {
    /// Handbrake axis with report-id prefix.
    pub const WITH_REPORT_ID_AXIS_START: usize = 1;
    /// Optional button-style byte with report-id prefix.
    pub const WITH_REPORT_ID_BUTTON: usize = 3;
    /// Handbrake axis with no report-id prefix.
    pub const RAW_AXIS_START: usize = 0;
    /// Optional button-style byte with no report-id prefix.
    pub const RAW_BUTTON: usize = 2;
}

/// Moza HID Report IDs.
pub mod report_ids {
    /// Device info query
    pub const DEVICE_INFO: u8 = 0x01;
    /// High torque enable
    pub const HIGH_TORQUE: u8 = 0x02;
    /// Start input reports
    pub const START_REPORTS: u8 = 0x03;
    /// Set rotation range
    pub const ROTATION_RANGE: u8 = 0x10;
    /// Set FFB mode
    pub const FFB_MODE: u8 = 0x11;
    /// Direct torque output
    pub const DIRECT_TORQUE: u8 = 0x20;
    /// Device gain
    pub const DEVICE_GAIN: u8 = 0x21;
}

/// Parse a little-endian u16 axis from `report` at `start`.
pub fn parse_axis(report: &[u8], start: usize) -> Option<u16> {
    if report.len() < start.saturating_add(2) {
        return None;
    }
    Some(u16::from_le_bytes([report[start], report[start + 1]]))
}

/// Lightweight parsed view over a wheelbase-style input report.
#[derive(Debug, Clone, Copy)]
pub struct RawWheelbaseReport<'a> {
    report: &'a [u8],
}

impl<'a> RawWheelbaseReport<'a> {
    pub fn new(report: &'a [u8]) -> Self {
        Self { report }
    }

    pub fn report_id(&self) -> u8 {
        self.report[0]
    }

    pub fn report_bytes(&self) -> &'a [u8] {
        self.report
    }

    pub fn byte(&self, offset: usize) -> Option<u8> {
        self.report.get(offset).copied()
    }

    pub fn axis_u16_le(&self, start: usize) -> Option<u16> {
        parse_axis(self.report, start)
    }

    pub fn axis_u16_or_zero(&self, start: usize) -> u16 {
        self.axis_u16_le(start).unwrap_or(0)
    }
}
