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
///
/// NOTE: Duplicated (by design) across tiny protocol microcrates to keep them
/// dependency-minimal. Keep in sync with similar helpers (e.g. moza-wheelbase-report).
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
///
/// When layouts overlap (for example, a 4-byte packet), the report-ID-prefixed
/// interpretation takes precedence when the first byte is non-zero.
pub fn parse_hbp_usb_report_best_effort(report: &[u8]) -> Option<HbpHandbrakeSampleRaw> {
    if report.is_empty() {
        return None;
    }

    let axis = if report.len() > WITH_REPORT_ID_BUTTON && report[0] != 0x00 {
        parse_axis(report, WITH_REPORT_ID_AXIS_START)
    } else {
        None
    };
    if let Some(axis) = axis {
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

    #[test]
    fn normalize_zero_maps_to_zero() {
        let normalized = HbpHandbrakeSampleRaw {
            handbrake: 0,
            button_byte: None,
        }
        .normalize();
        assert!((normalized.handbrake).abs() < f32::EPSILON);
        assert_eq!(normalized.button_byte, None);
    }

    #[test]
    fn normalize_max_maps_to_one() {
        let normalized = HbpHandbrakeSampleRaw {
            handbrake: u16::MAX,
            button_byte: None,
        }
        .normalize();
        assert!((normalized.handbrake - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn parse_axis_returns_none_for_empty_slice() {
        assert_eq!(parse_axis(&[], 0), None);
    }

    #[test]
    fn parse_axis_returns_none_for_single_byte() {
        assert_eq!(parse_axis(&[0xFF], 0), None);
    }

    #[test]
    fn parse_axis_returns_none_for_oob_offset() {
        assert_eq!(parse_axis(&[0x00, 0x00], 1), None);
    }

    #[test]
    fn parse_axis_round_trips_le_u16() -> Result<(), Box<dyn std::error::Error>> {
        let value = 0xABCDu16;
        let bytes = value.to_le_bytes();
        let parsed = parse_axis(&bytes, 0).ok_or("expected axis parse")?;
        assert_eq!(parsed, value);
        Ok(())
    }

    #[test]
    fn parse_axis_boundary_values() -> Result<(), Box<dyn std::error::Error>> {
        let zero = parse_axis(&[0x00, 0x00], 0).ok_or("expected zero parse")?;
        assert_eq!(zero, 0u16);
        let max = parse_axis(&[0xFF, 0xFF], 0).ok_or("expected max parse")?;
        assert_eq!(max, u16::MAX);
        Ok(())
    }

    #[test]
    fn parse_hbp_zero_report_id_falls_through_to_raw() -> Result<(), Box<dyn std::error::Error>> {
        // 4-byte report with report_id=0x00 should use raw layout (3+ bytes with button)
        let report = [0x00u8, 0x34, 0x12, 0x80];
        let parsed = parse_hbp_usb_report_best_effort(&report)
            .ok_or("expected raw parse for zero report-id")?;
        assert_eq!(parsed.handbrake, u16::from_le_bytes([0x00, 0x34]));
        assert_eq!(parsed.button_byte, Some(0x12));
        Ok(())
    }

    #[test]
    fn parse_hbp_preserves_button_byte_on_all_layouts() -> Result<(), Box<dyn std::error::Error>> {
        // report-id-prefixed layout
        let r1 = parse_hbp_usb_report_best_effort(&[0x01, 0x00, 0x00, 0xFF]).ok_or("layout 1")?;
        assert_eq!(r1.button_byte, Some(0xFF));

        // raw 3-byte layout
        let r2 = parse_hbp_usb_report_best_effort(&[0x00, 0x00, 0xAA]).ok_or("layout 2")?;
        assert_eq!(r2.button_byte, Some(0xAA));

        // raw 2-byte layout has no button
        let r3 = parse_hbp_usb_report_best_effort(&[0x00, 0x00]).ok_or("layout 3")?;
        assert_eq!(r3.button_byte, None);
        Ok(())
    }

    #[test]
    fn hbp_sample_raw_debug_and_clone() {
        let sample = HbpHandbrakeSampleRaw {
            handbrake: 1234,
            button_byte: Some(0x01),
        };
        let cloned = sample;
        assert_eq!(sample, cloned);
        let _ = format!("{:?}", sample);
    }

    #[test]
    fn hbp_sample_normalized_debug_and_clone() {
        let sample = HbpHandbrakeSample {
            handbrake: 0.5,
            button_byte: None,
        };
        let cloned = sample;
        assert_eq!(cloned.button_byte, None);
        let _ = format!("{:?}", sample);
    }

    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        #[test]
        fn prop_parse_axis_round_trips(lo in 0u8..=255u8, hi in 0u8..=255u8) {
            let expected = u16::from_le_bytes([lo, hi]);
            let buf = [lo, hi];
            prop_assert_eq!(parse_axis(&buf, 0), Some(expected));
        }

        #[test]
        fn prop_normalize_within_unit_range(value: u16) {
            let sample = HbpHandbrakeSampleRaw {
                handbrake: value,
                button_byte: None,
            };
            let normalized = sample.normalize();
            prop_assert!(normalized.handbrake >= 0.0);
            prop_assert!(normalized.handbrake <= 1.0);
        }

        #[test]
        fn prop_two_byte_report_always_parses(lo in 0u8..=255u8, hi in 0u8..=255u8) {
            let report = [lo, hi];
            let parsed = parse_hbp_usb_report_best_effort(&report);
            prop_assert!(parsed.is_some());
            let sample = parsed.expect("just checked is_some");
            prop_assert_eq!(sample.handbrake, u16::from_le_bytes([lo, hi]));
            prop_assert_eq!(sample.button_byte, None);
        }

        #[test]
        fn prop_four_byte_nonzero_id_uses_prefixed_layout(
            id in 1u8..=255u8,
            lo in 0u8..=255u8,
            hi in 0u8..=255u8,
            btn in 0u8..=255u8,
        ) {
            let report = [id, lo, hi, btn];
            let parsed = parse_hbp_usb_report_best_effort(&report);
            prop_assert!(parsed.is_some());
            let sample = parsed.expect("just checked is_some");
            prop_assert_eq!(sample.handbrake, u16::from_le_bytes([lo, hi]));
            prop_assert_eq!(sample.button_byte, Some(btn));
        }
    }
}
