//! Shared library for openracing-capture-ids.
//!
//! Exposes the replay module and vendor decode functions for integration testing.

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
