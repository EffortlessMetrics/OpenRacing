//! Moza wheelbase aggregated input report parsing primitives.
//!
//! This crate is intentionally small and I/O-free so protocol crates can
//! consume capture-validated parsing logic without pulling runtime concerns.

#![deny(static_mut_refs)]

/// Report ID and byte offsets for wheelbase-aggregated input reports.
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

/// Minimum bytes required for a valid wheelbase report containing steering,
/// throttle, and brake axes.
pub const MIN_REPORT_LEN: usize = input_report::BRAKE_START + 2;

/// Lightweight parsed view over a wheelbase-style input report.
#[derive(Debug, Clone, Copy)]
pub struct RawWheelbaseReport<'a> {
    report: &'a [u8],
}

impl<'a> RawWheelbaseReport<'a> {
    /// Construct a borrowed report view without validation.
    ///
    /// Prefer [`parse_wheelbase_report`] when report ID/length validation is required.
    pub fn new(report: &'a [u8]) -> Self {
        Self { report }
    }

    pub fn report_id(&self) -> u8 {
        self.report.first().copied().unwrap_or(0)
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

/// Raw wheelbase pedal samples from an aggregated report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WheelbasePedalAxesRaw {
    pub throttle: u16,
    pub brake: u16,
    pub clutch: Option<u16>,
    pub handbrake: Option<u16>,
}

/// Raw wheelbase input sample extracted from a single report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WheelbaseInputRaw {
    pub steering: u16,
    pub pedals: WheelbasePedalAxesRaw,
    pub buttons: [u8; input_report::BUTTONS_LEN],
    pub hat: u8,
    /// Vendor-specific byte immediately after `hat`.
    ///
    /// OpenRacing currently treats this as an opaque discriminator; some firmwares
    /// appear to use it as a rim identifier and it is used to gate rim-specific parsing.
    pub funky: u8,
    pub rotary: [u8; input_report::ROTARY_LEN],
}

/// Parse a little-endian `u16` axis from `report` at `start`.
///
/// NOTE: This helper is intentionally duplicated in other tiny protocol microcrates
/// (e.g. `racing-wheel-hbp`) to keep them dependency-minimal. Keep implementations in sync.
pub fn parse_axis(report: &[u8], start: usize) -> Option<u16> {
    if report.len() < start.saturating_add(2) {
        return None;
    }
    Some(u16::from_le_bytes([report[start], report[start + 1]]))
}

fn parse_wheelbase_pedal_axes_from_report(
    report: &RawWheelbaseReport<'_>,
) -> Option<WheelbasePedalAxesRaw> {
    let throttle = report.axis_u16_le(input_report::THROTTLE_START)?;
    let brake = report.axis_u16_le(input_report::BRAKE_START)?;
    let clutch = report.axis_u16_le(input_report::CLUTCH_START);
    let handbrake = report.axis_u16_le(input_report::HANDBRAKE_START);

    Some(WheelbasePedalAxesRaw {
        throttle,
        brake,
        clutch,
        handbrake,
    })
}

/// Parse a wheelbase input report into a lightweight borrowed view.
///
/// Returns `None` unless:
/// - report ID is `input_report::REPORT_ID`
/// - report length is at least `MIN_REPORT_LEN`
pub fn parse_wheelbase_report(report: &[u8]) -> Option<RawWheelbaseReport<'_>> {
    if report.first().copied() != Some(input_report::REPORT_ID) {
        return None;
    }
    if report.len() < MIN_REPORT_LEN {
        return None;
    }
    Some(RawWheelbaseReport::new(report))
}

/// Parse wheelbase-aggregated pedal axes.
pub fn parse_wheelbase_pedal_axes(report: &[u8]) -> Option<WheelbasePedalAxesRaw> {
    let report = parse_wheelbase_report(report)?;
    parse_wheelbase_pedal_axes_from_report(&report)
}

/// Parse a full wheelbase-aggregated input report.
///
/// Optional controls (clutch, handbrake, buttons, hat, funky, rotary) are
/// zero-filled when their bytes are absent.
pub fn parse_wheelbase_input_report(report: &[u8]) -> Option<WheelbaseInputRaw> {
    let report = parse_wheelbase_report(report)?;
    let steering = report.axis_u16_le(input_report::STEERING_START)?;
    let pedals = parse_wheelbase_pedal_axes_from_report(&report)?;

    let mut buttons = [0u8; input_report::BUTTONS_LEN];
    let bytes = report.report_bytes();
    if bytes.len() > input_report::BUTTONS_START {
        let end = bytes
            .len()
            .min(input_report::BUTTONS_START + input_report::BUTTONS_LEN);
        let count = end - input_report::BUTTONS_START;
        buttons[..count].copy_from_slice(&bytes[input_report::BUTTONS_START..end]);
    }

    let hat = report.byte(input_report::HAT_START).unwrap_or(0);
    let funky = report.byte(input_report::FUNKY_START).unwrap_or(0);

    let mut rotary = [0u8; input_report::ROTARY_LEN];
    if bytes.len() > input_report::ROTARY_START {
        let end = bytes
            .len()
            .min(input_report::ROTARY_START + input_report::ROTARY_LEN);
        let count = end - input_report::ROTARY_START;
        rotary[..count].copy_from_slice(&bytes[input_report::ROTARY_START..end]);
    }

    Some(WheelbaseInputRaw {
        steering,
        pedals,
        buttons,
        hat,
        funky,
        rotary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_wheelbase_report_rejects_non_input_id() {
        let report = [0x02u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(parse_wheelbase_report(&report).map(|r| r.report_id()), None);
    }

    #[test]
    fn parse_wheelbase_report_rejects_short_input() {
        let report = [input_report::REPORT_ID, 0x00, 0x80, 0x01, 0x00, 0x02];
        assert_eq!(parse_wheelbase_report(&report).map(|r| r.report_id()), None);
    }

    #[test]
    fn parse_wheelbase_pedal_axes_reads_optional_axes() -> Result<(), Box<dyn std::error::Error>> {
        let report = [
            input_report::REPORT_ID,
            0x00,
            0x80,
            0x34,
            0x12,
            0x78,
            0x56,
            0xBC,
            0x9A,
            0xEF,
            0xCD,
        ];

        let parsed =
            parse_wheelbase_pedal_axes(&report).ok_or("expected wheelbase pedal axis parse")?;

        assert_eq!(parsed.throttle, 0x1234);
        assert_eq!(parsed.brake, 0x5678);
        assert_eq!(parsed.clutch, Some(0x9ABC));
        assert_eq!(parsed.handbrake, Some(0xCDEF));
        Ok(())
    }

    #[test]
    fn parse_wheelbase_input_zero_fills_missing_controls() -> Result<(), Box<dyn std::error::Error>>
    {
        let report = [input_report::REPORT_ID, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66];

        let parsed = parse_wheelbase_input_report(&report)
            .ok_or("expected wheelbase input parse for required fields")?;

        assert_eq!(parsed.steering, 0x2211);
        assert_eq!(parsed.pedals.throttle, 0x4433);
        assert_eq!(parsed.pedals.brake, 0x6655);
        assert_eq!(parsed.pedals.clutch, None);
        assert_eq!(parsed.pedals.handbrake, None);
        assert_eq!(parsed.buttons, [0u8; input_report::BUTTONS_LEN]);
        assert_eq!(parsed.hat, 0);
        assert_eq!(parsed.funky, 0);
        assert_eq!(parsed.rotary, [0u8; input_report::ROTARY_LEN]);
        Ok(())
    }

    #[test]
    fn parse_wheelbase_input_preserves_partial_buttons() -> Result<(), Box<dyn std::error::Error>> {
        let mut report = [0u8; input_report::BUTTONS_START + 3];
        report[0] = input_report::REPORT_ID;
        report[input_report::STEERING_START..input_report::STEERING_START + 2]
            .copy_from_slice(&0x2211u16.to_le_bytes());
        report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
            .copy_from_slice(&0x4433u16.to_le_bytes());
        report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
            .copy_from_slice(&0x6655u16.to_le_bytes());
        report[input_report::BUTTONS_START] = 0xA1;
        report[input_report::BUTTONS_START + 1] = 0xB2;
        report[input_report::BUTTONS_START + 2] = 0xC3;

        let parsed =
            parse_wheelbase_input_report(&report).ok_or("expected partial wheelbase parse")?;

        assert_eq!(parsed.buttons[0], 0xA1);
        assert_eq!(parsed.buttons[1], 0xB2);
        assert_eq!(parsed.buttons[2], 0xC3);
        assert_eq!(parsed.buttons[3..], [0u8; input_report::BUTTONS_LEN - 3]);
        Ok(())
    }

    #[test]
    fn parse_wheelbase_input_reads_full_length_controls() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut report = [0u8; input_report::ROTARY_START + input_report::ROTARY_LEN];
        report[0] = input_report::REPORT_ID;
        report[input_report::STEERING_START..input_report::STEERING_START + 2]
            .copy_from_slice(&0x2211u16.to_le_bytes());
        report[input_report::THROTTLE_START..input_report::THROTTLE_START + 2]
            .copy_from_slice(&0x4433u16.to_le_bytes());
        report[input_report::BRAKE_START..input_report::BRAKE_START + 2]
            .copy_from_slice(&0x6655u16.to_le_bytes());
        report[input_report::CLUTCH_START..input_report::CLUTCH_START + 2]
            .copy_from_slice(&0x8877u16.to_le_bytes());
        report[input_report::HANDBRAKE_START..input_report::HANDBRAKE_START + 2]
            .copy_from_slice(&0xAA99u16.to_le_bytes());

        let mut expected_buttons = [0u8; input_report::BUTTONS_LEN];
        for (i, button) in expected_buttons.iter_mut().enumerate() {
            *button = i as u8;
            report[input_report::BUTTONS_START + i] = *button;
        }

        report[input_report::HAT_START] = 0x04;
        report[input_report::FUNKY_START] = 0x05;
        report[input_report::ROTARY_START] = 0x19;
        report[input_report::ROTARY_START + 1] = 0x64;

        let parsed =
            parse_wheelbase_input_report(&report).ok_or("expected full-length wheelbase parse")?;

        assert_eq!(parsed.steering, 0x2211);
        assert_eq!(parsed.pedals.throttle, 0x4433);
        assert_eq!(parsed.pedals.brake, 0x6655);
        assert_eq!(parsed.pedals.clutch, Some(0x8877));
        assert_eq!(parsed.pedals.handbrake, Some(0xAA99));
        assert_eq!(parsed.buttons, expected_buttons);
        assert_eq!(parsed.hat, 0x04);
        assert_eq!(parsed.funky, 0x05);
        assert_eq!(parsed.rotary, [0x19, 0x64]);
        Ok(())
    }
}
