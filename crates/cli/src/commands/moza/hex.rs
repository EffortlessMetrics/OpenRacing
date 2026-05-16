//! Shared hexadecimal parsing helpers for Moza receipts, captures, and descriptors.

pub(super) fn parse_hex_bytes(value: &str) -> std::result::Result<Vec<u8>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("data_hex is empty".to_string());
    }

    if trimmed.contains(char::is_whitespace) {
        return trimmed.split_whitespace().map(parse_hex_u8_token).collect();
    }

    if !trimmed.len().is_multiple_of(2) {
        return Err("compact data_hex must contain an even number of hex digits".to_string());
    }

    (0..trimmed.len())
        .step_by(2)
        .map(|start| parse_hex_u8_token(&trimmed[start..start + 2]))
        .collect()
}

pub(super) fn parse_hex_u8_token(token: &str) -> std::result::Result<u8, String> {
    let value = token
        .trim()
        .strip_prefix("0x")
        .or_else(|| token.trim().strip_prefix("0X"))
        .unwrap_or_else(|| token.trim());
    u8::from_str_radix(value, 16).map_err(|_| format!("invalid byte token '{token}'"))
}
