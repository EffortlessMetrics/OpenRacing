//! HBP handbrake parsing primitives.
//!
//! This crate is intentionally small and I/O-free so it can be reused from
//! vendor protocol crates without pulling in additional runtime concerns.

#![deny(static_mut_refs)]

/// Handbrake axis with report-id prefix.
pub const WITH_REPORT_ID_AXIS_START: usize = 1;
/// Optional button-style byte with report-id prefix.
pub const WITH_REPORT_ID_BUTTON: usize = 3;
/// Handbrake axis with no report-id prefix.
pub const RAW_AXIS_START: usize = 0;
/// Optional button-style byte with no report-id prefix.
pub const RAW_BUTTON: usize = 2;

/// Raw HBP handbrake sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HbpHandbrakeSampleRaw {
    /// Handbrake axis sample (little-endian 16-bit).
    pub handbrake: u16,
    /// Optional byte that may encode button-mode state.
    pub button_byte: Option<u8>,
}

/// Normalized HBP handbrake sample in the `[0.0, 1.0]` range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HbpHandbrakeSample {
    /// Handbrake axis sample normalized to `[0.0, 1.0]`.
    pub handbrake: f32,
    /// Optional byte that may encode button-mode state.
    pub button_byte: Option<u8>,
}

impl HbpHandbrakeSampleRaw {
    /// Normalize raw 16-bit sample to `[0.0, 1.0]`.
    pub fn normalize(self) -> HbpHandbrakeSample {
        const MAX: f32 = u16::MAX as f32;
        HbpHandbrakeSample {
            handbrake: self.handbrake as f32 / MAX,
            button_byte: self.button_byte,
        }
    }
}

/// Parse a little-endian `u16` axis from `report` at `start`.
pub fn parse_axis(report: &[u8], start: usize) -> Option<u16> {
    if report.len() < start.saturating_add(2) {
        return None;
    }
    Some(u16::from_le_bytes([report[start], report[start + 1]]))
}

/// Parse a standalone HBP USB report using best-effort layout inference.
///
/// Supported layouts:
/// 1. With report ID prefix: `[report_id, axis_lo, axis_hi, button]`
/// 2. Raw two-byte: `[axis_lo, axis_hi]`
/// 3. Raw with button: `[axis_lo, axis_hi, button]`
pub fn parse_hbp_usb_report_best_effort(report: &[u8]) -> Option<HbpHandbrakeSampleRaw> {
    if report.is_empty() {
        return None;
    }

    if report.len() > WITH_REPORT_ID_BUTTON
        && report[0] != 0x00
        && let Some(axis) = parse_axis(report, WITH_REPORT_ID_AXIS_START)
    {
        return Some(HbpHandbrakeSampleRaw {
            handbrake: axis,
            button_byte: Some(report[WITH_REPORT_ID_BUTTON]),
        });
    }

    if report.len() == 2 {
        return Some(HbpHandbrakeSampleRaw {
            handbrake: u16::from_le_bytes([report[RAW_AXIS_START], report[RAW_AXIS_START + 1]]),
            button_byte: None,
        });
    }

    if report.len() > RAW_BUTTON {
        return Some(HbpHandbrakeSampleRaw {
            handbrake: u16::from_le_bytes([report[RAW_AXIS_START], report[RAW_AXIS_START + 1]]),
            button_byte: Some(report[RAW_BUTTON]),
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hbp_with_report_id_prefix() -> Result<(), Box<dyn std::error::Error>> {
        let report = [0x11u8, 0x34, 0x12, 0x80];
        let parsed = parse_hbp_usb_report_best_effort(&report)
            .ok_or("expected HBP parse with report-id-prefixed layout")?;

        assert_eq!(parsed.handbrake, 0x1234);
        assert_eq!(parsed.button_byte, Some(0x80));
        Ok(())
    }

    #[test]
    fn parse_hbp_raw_two_byte() -> Result<(), Box<dyn std::error::Error>> {
        let report = [0xAAu8, 0x55];
        let parsed = parse_hbp_usb_report_best_effort(&report)
            .ok_or("expected HBP parse for raw two-byte layout")?;

        assert_eq!(parsed.handbrake, 0x55AA);
        assert_eq!(parsed.button_byte, None);
        Ok(())
    }

    #[test]
    fn parse_hbp_raw_with_button() -> Result<(), Box<dyn std::error::Error>> {
        let report = [0xAAu8, 0x55, 0x01];
        let parsed = parse_hbp_usb_report_best_effort(&report)
            .ok_or("expected HBP parse for raw layout with button byte")?;

        assert_eq!(parsed.handbrake, 0x55AA);
        assert_eq!(parsed.button_byte, Some(0x01));
        Ok(())
    }

    #[test]
    fn parse_hbp_empty_report_is_unsupported() {
        assert_eq!(parse_hbp_usb_report_best_effort(&[]), None);
    }

    #[test]
    fn parse_hbp_single_byte_report_is_unsupported() {
        assert_eq!(parse_hbp_usb_report_best_effort(&[0x01]), None);
    }

    #[test]
    fn normalize_hbp_axis_maps_to_unit_range() {
        let normalized = HbpHandbrakeSampleRaw {
            handbrake: 32768,
            button_byte: Some(0x01),
        }
        .normalize();

        assert!((normalized.handbrake - (32768.0 / 65535.0)).abs() < 0.00002);
        assert_eq!(normalized.button_byte, Some(0x01));
    }
}
