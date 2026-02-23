//! SR-P pedal parsing primitives.
//!
//! This crate is intentionally small and I/O-free so it can be reused from
//! vendor protocol crates without pulling in additional runtime concerns.

#![deny(static_mut_refs)]

/// Offset of the first SR-P throttle axis byte (after report ID).
pub const THROTTLE_START: usize = 1;
/// Offset of the first SR-P brake axis byte.
pub const BRAKE_START: usize = 3;
/// Minimum report length required for throttle + brake parsing.
pub const MIN_REPORT_LEN: usize = 5;

/// Raw SR-P pedal axis samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SrpPedalAxesRaw {
    /// Throttle axis sample (little-endian 16-bit).
    pub throttle: u16,
    /// Brake axis sample (little-endian 16-bit) when present.
    pub brake: Option<u16>,
}

/// Normalized SR-P pedal axis samples in the `[0.0, 1.0]` range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SrpPedalAxes {
    /// Throttle axis sample normalized to `[0.0, 1.0]`.
    pub throttle: f32,
    /// Brake axis sample normalized to `[0.0, 1.0]`, when present.
    pub brake: Option<f32>,
}

impl SrpPedalAxesRaw {
    /// Normalize raw 16-bit samples to `[0.0, 1.0]`.
    pub fn normalize(self) -> SrpPedalAxes {
        const MAX: f32 = u16::MAX as f32;
        SrpPedalAxes {
            throttle: self.throttle as f32 / MAX,
            brake: self.brake.map(|value| value as f32 / MAX),
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

/// Parse a standalone SR-P USB report.
///
/// Current best-effort layout:
/// - `report[0]`: report ID (ignored by this parser)
/// - `report[1..=2]`: throttle axis (u16 LE)
/// - `report[3..=4]`: brake axis (u16 LE)
///
/// Returns `None` when the report does not contain at least `MIN_REPORT_LEN`
/// bytes, or when required axis bytes are malformed.
pub fn parse_srp_usb_report_best_effort(report: &[u8]) -> Option<SrpPedalAxesRaw> {
    if report.len() < MIN_REPORT_LEN {
        return None;
    }

    let throttle = parse_axis(report, THROTTLE_START)?;
    let brake = parse_axis(report, BRAKE_START);

    Some(SrpPedalAxesRaw { throttle, brake })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_srp_report_best_effort_throttle_and_brake() -> Result<(), Box<dyn std::error::Error>> {
        let report = [0x01u8, 0xFF, 0xFF, 0x00, 0x80];
        let axes = parse_srp_usb_report_best_effort(&report)
            .ok_or("expected throttle/brake parse from best-effort SR-P report")?;

        assert_eq!(axes.throttle, 0xFFFF);
        assert_eq!(axes.brake, Some(0x8000));
        Ok(())
    }

    #[test]
    fn parse_srp_report_short_input_is_unsupported() {
        let report = [0x01u8, 0xFF];
        let axes = parse_srp_usb_report_best_effort(&report);
        assert_eq!(axes, None);
    }

    #[test]
    fn normalize_srp_axes_maps_to_unit_range() -> Result<(), Box<dyn std::error::Error>> {
        let raw = SrpPedalAxesRaw {
            throttle: 65535,
            brake: Some(32768),
        };

        let normalized = raw.normalize();
        assert!((normalized.throttle - 1.0).abs() < 0.00002);
        let brake = normalized.brake.ok_or("expected brake sample")?;
        assert!((brake - (32768.0 / 65535.0)).abs() < 0.00002);
        Ok(())
    }
}
