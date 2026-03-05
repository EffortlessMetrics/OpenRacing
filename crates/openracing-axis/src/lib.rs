//! Shared axis parsing and normalization helpers for protocol SRP microcrates.
//!
//! This crate is intentionally tiny and I/O-free so protocol crates can reuse
//! deterministic, allocation-free axis primitives.

#![deny(static_mut_refs)]

/// Parse a little-endian `u16` axis from `report` at byte offset `start`.
#[must_use]
pub fn parse_u16_axis_le(report: &[u8], start: usize) -> Option<u16> {
    if report.len() < start.saturating_add(2) {
        return None;
    }
    Some(u16::from_le_bytes([report[start], report[start + 1]]))
}

/// Normalize a raw `u16` axis value to `[0.0, 1.0]`.
#[must_use]
pub fn normalize_u16_to_unit(value: u16) -> f32 {
    value as f32 / u16::MAX as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_axis_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let value = 0xABCDu16;
        let parsed = parse_u16_axis_le(&value.to_le_bytes(), 0)
            .ok_or("expected parse for two-byte little-endian axis")?;
        assert_eq!(parsed, value);
        Ok(())
    }

    #[test]
    fn parse_axis_returns_none_when_too_short() {
        assert_eq!(parse_u16_axis_le(&[0xFF], 0), None);
        assert_eq!(parse_u16_axis_le(&[0x00, 0x01], 1), None);
    }

    #[test]
    fn normalize_axis_boundary_values() {
        assert!((normalize_u16_to_unit(0)).abs() < f32::EPSILON);
        assert!((normalize_u16_to_unit(u16::MAX) - 1.0).abs() < f32::EPSILON);
    }
}
