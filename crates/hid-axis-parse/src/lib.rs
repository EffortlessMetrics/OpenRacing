//! HID axis parsing primitives shared by protocol microcrates.
//!
//! This crate intentionally stays tiny and allocation-free.

#![deny(static_mut_refs)]

/// Parse a little-endian `u16` from `report` at byte offset `start`.
///
/// Returns `None` if the report does not include both bytes.
pub fn parse_u16_le_at(report: &[u8], start: usize) -> Option<u16> {
    if report.len() < start.saturating_add(2) {
        return None;
    }
    Some(u16::from_le_bytes([report[start], report[start + 1]]))
}

#[cfg(test)]
mod tests {
    use super::parse_u16_le_at;

    #[test]
    fn parse_u16_le_at_rejects_short_report() {
        assert_eq!(parse_u16_le_at(&[0xAA], 0), None);
        assert_eq!(parse_u16_le_at(&[0xAA, 0xBB], 1), None);
    }

    #[test]
    fn parse_u16_le_at_parses_expected_value() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = parse_u16_le_at(&[0x34, 0x12], 0).ok_or("expected u16 parse")?;
        assert_eq!(parsed, 0x1234);
        Ok(())
    }
}
