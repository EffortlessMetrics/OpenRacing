//! Shared HID axis parsing/normalization primitives.
//!
//! Tiny helper crate for protocol microcrates that need deterministic parsing of
//! little-endian axis data in I/O-free code paths.

#![deny(static_mut_refs)]

/// Parse a little-endian `u16` axis from `report` at `start`.
#[must_use]
pub fn parse_u16_axis_le(report: &[u8], start: usize) -> Option<u16> {
    if report.len() < start.saturating_add(2) {
        return None;
    }
    Some(u16::from_le_bytes([report[start], report[start + 1]]))
}

/// Normalize a 16-bit axis sample into the `[0.0, 1.0]` range.
#[must_use]
pub fn normalize_u16_axis(sample: u16) -> f32 {
    sample as f32 / u16::MAX as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_u16_axis_le_reads_expected_value() {
        let parsed = parse_u16_axis_le(&[0x34, 0x12], 0);
        assert_eq!(parsed, Some(0x1234));
    }

    #[test]
    fn parse_u16_axis_le_rejects_short_buffers() {
        assert_eq!(parse_u16_axis_le(&[], 0), None);
        assert_eq!(parse_u16_axis_le(&[0x01], 0), None);
        assert_eq!(parse_u16_axis_le(&[0x00, 0x00], 1), None);
    }

    #[test]
    fn normalize_u16_axis_maps_bounds() {
        assert!((normalize_u16_axis(0) - 0.0).abs() < f32::EPSILON);
        assert!((normalize_u16_axis(u16::MAX) - 1.0).abs() < f32::EPSILON);
    }
}
