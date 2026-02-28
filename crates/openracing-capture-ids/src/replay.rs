//! Replay a captured HID JSON Lines file produced by `--record` mode.

#![deny(static_mut_refs)]

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::Duration;

// ── Types ────────────────────────────────────────────────────────────────────

/// A single captured HID input report as stored in the JSON Lines capture file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedReport {
    /// Timestamp in nanoseconds (Unix epoch).
    pub ts_ns: u64,
    /// Vendor ID as hex string (e.g. `"0x046D"`).
    pub vid: String,
    /// Product ID as hex string (e.g. `"0x0002"`).
    pub pid: String,
    /// Report bytes as lowercase hex string (e.g. `"0102030405"`).
    pub report: String,
}

// ── Parsing helpers ──────────────────────────────────────────────────────────

/// Parse a single JSON Line from a capture file into a [`CapturedReport`].
pub fn parse_capture_line(line: &str) -> Result<CapturedReport> {
    serde_json::from_str(line).with_context(|| format!("failed to parse capture line: {line}"))
}

/// Decode a lowercase hex string into raw bytes.
pub fn decode_hex(s: &str) -> Result<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return Err(anyhow!(
            "hex string has odd length ({} chars): '{s}'",
            s.len()
        ));
    }
    (0..s.len() / 2)
        .map(|i| {
            u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
                .with_context(|| format!("invalid hex byte at index {i} in '{s}'"))
        })
        .collect()
}

/// Parse a VID/PID hex string (e.g. `"0x046D"` or `"046D"`) to `u16`.
pub fn parse_vid_str(s: &str) -> Result<u16> {
    let s = s.trim();
    let digits = if s.starts_with("0x") || s.starts_with("0X") {
        &s[2..]
    } else {
        s
    };
    u16::from_str_radix(digits, 16)
        .with_context(|| format!("invalid VID/PID value '{s}', expected hex like 0x046D"))
}

// ── Display ──────────────────────────────────────────────────────────────────

/// Print a single captured report in human-readable form.
///
/// `delta_ns` is the nanosecond offset from the first report in the file.
pub fn print_capture_entry(entry: &CapturedReport, delta_ns: u64) -> Result<()> {
    let bytes = decode_hex(&entry.report)?;
    let report_id = bytes.first().copied().unwrap_or(0);

    let hex: String = bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ");
    let ascii: String = bytes
        .iter()
        .map(|&b| {
            if b.is_ascii_graphic() || b == b' ' {
                b as char
            } else {
                '.'
            }
        })
        .collect();

    let delta_ms = delta_ns / 1_000_000;

    println!(
        "[+{delta_ms:>8}ms] vid={} pid={} id=0x{report_id:02X} hex=[{hex}] ascii=[{ascii}]",
        entry.vid, entry.pid
    );

    let vid = parse_vid_str(&entry.vid).unwrap_or(0);
    if let Some(decoded) = crate::decode_report(vid, &bytes) {
        println!("  {decoded}");
    }

    Ok(())
}

// ── Replay ───────────────────────────────────────────────────────────────────

/// Replay a captured JSON Lines file, sleeping between reports to honour
/// original timestamps scaled by `speed`.
///
/// `speed = 1.0` plays back at real-time; `speed = 2.0` plays back at double
/// speed; `speed = 0.0` prints all reports without any delay.
pub fn replay_file(path: &Path, speed: f64) -> Result<()> {
    let file = File::open(path).with_context(|| format!("failed to open '{}'", path.display()))?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    let first_line = match lines.next() {
        Some(l) => l.with_context(|| "failed to read first line of capture file")?,
        None => return Ok(()), // empty file
    };

    let first = parse_capture_line(&first_line)?;
    let first_ts = first.ts_ns;
    let wall_start = std::time::Instant::now();

    print_capture_entry(&first, 0)?;

    for line_result in lines {
        let line = line_result.with_context(|| "failed to read line from capture file")?;
        if line.trim().is_empty() {
            continue;
        }
        let entry = parse_capture_line(&line)?;

        // How far into the original capture is this report?
        let original_delta_ns = entry.ts_ns.saturating_sub(first_ts);

        // Scale by speed factor; skip sleep when speed is 0.
        if speed > 0.0 {
            let target_wall_ns = (original_delta_ns as f64 / speed) as u64;
            let wall_elapsed_ns = wall_start.elapsed().as_nanos() as u64;
            if target_wall_ns > wall_elapsed_ns {
                std::thread::sleep(Duration::from_nanos(
                    target_wall_ns.saturating_sub(wall_elapsed_ns),
                ));
            }
        }

        print_capture_entry(&entry, original_delta_ns)?;
    }

    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_capture_line_all_fields() -> Result<()> {
        let line = r#"{"ts_ns":1234567890,"vid":"0x046D","pid":"0x0002","report":"0102030405"}"#;
        let entry = parse_capture_line(line)?;
        assert_eq!(entry.ts_ns, 1_234_567_890);
        assert_eq!(entry.vid, "0x046D");
        assert_eq!(entry.pid, "0x0002");
        assert_eq!(entry.report, "0102030405");
        Ok(())
    }

    #[test]
    fn test_parse_capture_line_missing_field() {
        // "report" field is missing
        let line = r#"{"ts_ns":1234,"vid":"0x046D","pid":"0x0002"}"#;
        assert!(parse_capture_line(line).is_err());
    }

    #[test]
    fn test_decode_hex_basic() -> Result<()> {
        let bytes = decode_hex("0102030405")?;
        assert_eq!(bytes, vec![0x01, 0x02, 0x03, 0x04, 0x05]);
        Ok(())
    }

    #[test]
    fn test_decode_hex_empty() -> Result<()> {
        let bytes = decode_hex("")?;
        assert!(bytes.is_empty());
        Ok(())
    }

    #[test]
    fn test_decode_hex_odd_length_error() {
        assert!(decode_hex("012").is_err());
    }

    #[test]
    fn test_decode_hex_invalid_chars_error() {
        assert!(decode_hex("0xZZ").is_err());
    }

    #[test]
    fn test_parse_vid_str_with_prefix() -> Result<()> {
        assert_eq!(parse_vid_str("0x046D")?, 0x046D);
        assert_eq!(parse_vid_str("0x346E")?, 0x346E);
        Ok(())
    }

    #[test]
    fn test_parse_vid_str_without_prefix() -> Result<()> {
        assert_eq!(parse_vid_str("046D")?, 0x046D);
        Ok(())
    }

    #[test]
    fn test_parse_vid_str_invalid() {
        assert!(parse_vid_str("ZZZZ").is_err());
    }

    #[test]
    fn test_roundtrip_capture_line() -> Result<()> {
        let original = CapturedReport {
            ts_ns: 9_999_999_000,
            vid: "0x346E".to_string(),
            pid: "0x0000".to_string(),
            report: "deadbeef".to_string(),
        };
        let serialized = serde_json::to_string(&original).map_err(|e| anyhow!("serialize: {e}"))?;
        let parsed = parse_capture_line(&serialized)?;
        assert_eq!(parsed.ts_ns, original.ts_ns);
        assert_eq!(parsed.vid, original.vid);
        assert_eq!(parsed.pid, original.pid);
        assert_eq!(parsed.report, original.report);
        Ok(())
    }

    #[test]
    fn test_roundtrip_hex_encode_decode() -> Result<()> {
        let original: Vec<u8> = (0u8..=15).collect();
        let hex: String = original.iter().map(|b| format!("{b:02x}")).collect();
        let decoded = decode_hex(&hex)?;
        assert_eq!(decoded, original);
        Ok(())
    }
}
