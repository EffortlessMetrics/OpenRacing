//! Hex parsing helpers for operator-supplied HID report descriptors.

use anyhow::{Result, anyhow};

pub(super) fn parse_hex_bytes(value: &str) -> std::result::Result<Vec<u8>, String> {
    let compact: String = value
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',' && *c != '_')
        .collect();
    if !compact.len().is_multiple_of(2) {
        return Err("hex string has odd length".to_string());
    }
    (0..compact.len())
        .step_by(2)
        .map(|index| parse_hex_u8_token(&compact[index..index + 2]))
        .collect()
}

pub(super) fn parse_hex_u8_token(token: &str) -> std::result::Result<u8, String> {
    let value = token
        .trim()
        .strip_prefix("0x")
        .or_else(|| token.trim().strip_prefix("0X"))
        .unwrap_or(token.trim());
    u8::from_str_radix(value, 16).map_err(|e| format!("invalid hex byte '{token}': {e}"))
}

pub(super) fn reject_unsupported_report_descriptor_bytes(bytes: &[u8]) -> Result<()> {
    if bytes.starts_with(b"HidP KDR") {
        return Err(anyhow!(
            "Windows HID collection/preparsed descriptor bytes are not raw HID report descriptor bytes; export the actual Report Descriptor block instead, for example Linux sysfs report_descriptor bytes or a USB descriptor tool's Report Descriptor hexdump."
        ));
    }
    Ok(())
}

pub(super) fn bytes_hex_compact(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}
