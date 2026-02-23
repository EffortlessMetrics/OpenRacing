//! Standalone (direct USB) Moza peripheral device parsing.
//!
//! Devices like the HBP handbrake and SR-P pedals can appear as direct USB HID
//! devices rather than being aggregated through the wheelbase. When a validated
//! `DeviceInputMap` is present, bindings are map-driven; otherwise a best-effort
//! fallback is used.

#![deny(static_mut_refs)]

use crate::ids::product_ids;
use crate::report::{hbp_report, parse_axis};
use racing_wheel_srp::parse_srp_usb_report_best_effort;

/// Axis data from a parsed standalone Moza peripheral report.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StandaloneAxes {
    /// Primary handbrake axis (HBP) or throttle axis (SR-P), normalized [0.0, 1.0].
    pub primary: f32,
    /// Secondary pedal axis (SR-P brake), normalized [0.0, 1.0]. `None` for single-axis devices.
    pub secondary: Option<f32>,
    /// Raw button byte, when present in the report.
    pub button_byte: Option<u8>,
}

impl StandaloneAxes {
    fn from_raw(primary_raw: u16, secondary_raw: Option<u16>, button_byte: Option<u8>) -> Self {
        Self {
            primary: primary_raw as f32 / 65535.0,
            secondary: secondary_raw.map(|v| v as f32 / 65535.0),
            button_byte,
        }
    }
}

/// Result of parsing a standalone peripheral report.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StandaloneParseResult {
    /// Report parsed using a validated capture-derived map.
    ParsedViaMap(StandaloneAxes),
    /// Report parsed using best-effort layout inference.
    ParsedBestEffort(StandaloneAxes),
    /// No parse path available for this device/report combination.
    Unsupported,
}

/// Parse a standalone HBP handbrake report into axis data.
///
/// Supports three layouts:
/// 1. With report ID prefix: `[report_id, axis_lo, axis_hi, button]`
/// 2. Raw two-byte: `[axis_lo, axis_hi]`
/// 3. Raw with button: `[axis_lo, axis_hi, button]`
pub fn parse_hbp_report(product_id: u16, report: &[u8]) -> StandaloneParseResult {
    if product_id != product_ids::HBP_HANDBRAKE {
        return StandaloneParseResult::Unsupported;
    }

    if report.is_empty() {
        return StandaloneParseResult::Unsupported;
    }

    // Layout 1: with report ID prefix (report[0] is non-zero report ID)
    if report.len() > hbp_report::WITH_REPORT_ID_BUTTON
        && report[0] != 0x00
        && report.len() >= 4
        && let Some(axis) = parse_axis(report, hbp_report::WITH_REPORT_ID_AXIS_START)
    {
        let button = Some(report[hbp_report::WITH_REPORT_ID_BUTTON]);
        return StandaloneParseResult::ParsedBestEffort(StandaloneAxes::from_raw(
            axis, None, button,
        ));
    }

    // Layout 2: raw two-byte (no report ID)
    if report.len() == 2 {
        let axis = u16::from_le_bytes([report[0], report[1]]);
        return StandaloneParseResult::ParsedBestEffort(StandaloneAxes::from_raw(axis, None, None));
    }

    // Layout 3: raw with button byte
    if report.len() >= 3 {
        let axis = u16::from_le_bytes([report[0], report[1]]);
        let button = if report.len() > hbp_report::RAW_BUTTON {
            Some(report[hbp_report::RAW_BUTTON])
        } else {
            None
        };
        return StandaloneParseResult::ParsedBestEffort(StandaloneAxes::from_raw(
            axis, None, button,
        ));
    }

    StandaloneParseResult::Unsupported
}

/// Parse a standalone SR-P pedal USB report into axis data.
///
/// Best-effort only until a capture-derived `device_map.json` is validated.
/// Returns throttle as primary and brake as secondary when the report is long enough.
pub fn parse_srp_report(product_id: u16, report: &[u8]) -> StandaloneParseResult {
    if product_id != product_ids::SR_P_PEDALS {
        return StandaloneParseResult::Unsupported;
    }

    let Some(axes) = parse_srp_usb_report_best_effort(report) else {
        return StandaloneParseResult::Unsupported;
    };

    StandaloneParseResult::ParsedBestEffort(StandaloneAxes::from_raw(
        axes.throttle,
        axes.brake,
        None,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hbp_parse_with_report_id_prefix() {
        let report = [0x01u8, 0xFF, 0xFF, 0x01, 0x00];
        let result = parse_hbp_report(product_ids::HBP_HANDBRAKE, &report);

        let axes = match result {
            StandaloneParseResult::ParsedBestEffort(a) => a,
            other => panic!("expected ParsedBestEffort, got {:?}", other),
        };

        assert!(
            (axes.primary - 1.0).abs() < 0.00002,
            "primary should be 1.0"
        );
        assert_eq!(axes.button_byte, Some(0x01));
        assert_eq!(axes.secondary, None);
    }

    #[test]
    fn hbp_parse_raw_two_byte() {
        let report = [0x00u8, 0x80]; // 0x8000 = 32768 → ~0.5
        let result = parse_hbp_report(product_ids::HBP_HANDBRAKE, &report);

        let axes = match result {
            StandaloneParseResult::ParsedBestEffort(a) => a,
            other => panic!("expected ParsedBestEffort, got {:?}", other),
        };

        assert!((axes.primary - (32768.0 / 65535.0)).abs() < 0.00002);
        assert_eq!(axes.button_byte, None);
    }

    #[test]
    fn hbp_parse_raw_with_button() {
        let report = [0xFF, 0xFF, 0x01u8]; // full scale + button=1
        let result = parse_hbp_report(product_ids::HBP_HANDBRAKE, &report);

        let axes = match result {
            StandaloneParseResult::ParsedBestEffort(a) => a,
            other => panic!("expected ParsedBestEffort, got {:?}", other),
        };

        assert!((axes.primary - 1.0).abs() < 0.00002);
        assert_eq!(axes.button_byte, Some(0x01));
    }

    #[test]
    fn hbp_wrong_product_id_returns_unsupported() {
        let report = [0xFF, 0xFF];
        assert_eq!(
            parse_hbp_report(0x9999, &report),
            StandaloneParseResult::Unsupported
        );
    }

    #[test]
    fn hbp_empty_report_returns_unsupported() {
        assert_eq!(
            parse_hbp_report(product_ids::HBP_HANDBRAKE, &[]),
            StandaloneParseResult::Unsupported
        );
    }

    #[test]
    fn srp_parse_best_effort_throttle_and_brake() {
        // [report_id, t_lo, t_hi, b_lo, b_hi]
        let report = [0x01u8, 0xFF, 0xFF, 0x00, 0x80];
        let result = parse_srp_report(product_ids::SR_P_PEDALS, &report);

        let axes = match result {
            StandaloneParseResult::ParsedBestEffort(a) => a,
            other => panic!("expected ParsedBestEffort, got {:?}", other),
        };

        assert!(
            (axes.primary - 1.0).abs() < 0.00002,
            "throttle should be 1.0"
        );
        let brake = axes.secondary.expect("brake should be present");
        assert!(
            (brake - (32768.0 / 65535.0)).abs() < 0.00002,
            "brake should be ~0.5"
        );
    }

    #[test]
    fn srp_short_report_returns_unsupported() {
        let report = [0x01u8, 0xFF]; // too short
        assert_eq!(
            parse_srp_report(product_ids::SR_P_PEDALS, &report),
            StandaloneParseResult::Unsupported
        );
    }

    #[test]
    fn srp_wrong_product_id_returns_unsupported() {
        let report = [0x01u8, 0xFF, 0xFF, 0x00, 0x80];
        assert_eq!(
            parse_srp_report(0x9999, &report),
            StandaloneParseResult::Unsupported
        );
    }
}
