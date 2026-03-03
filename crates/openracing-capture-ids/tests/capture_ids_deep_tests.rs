//! Deep tests for openracing-capture-ids.
//!
//! Covers: capture ID types, ID generation/uniqueness, serialization/deserialization,
//! string parsing, validation, and comparison/ordering.

use openracing_capture_ids::replay::{
    CapturedReport, decode_hex, parse_capture_line, parse_vid_str,
};
use openracing_capture_ids::{decode_report, hex_u16, parse_hex_id};

// ── ID generation and uniqueness ────────────────────────────────────────────

mod id_uniqueness {
    use super::*;

    #[test]
    fn hex_u16_produces_unique_strings_for_distinct_values() -> anyhow::Result<()> {
        let mut seen = std::collections::HashSet::new();
        for val in 0u16..=1024 {
            let formatted = hex_u16(val);
            assert!(
                seen.insert(formatted.clone()),
                "duplicate hex_u16 output for {val}: {formatted}"
            );
        }
        Ok(())
    }

    #[test]
    fn captured_reports_with_different_timestamps_are_distinguishable() -> anyhow::Result<()> {
        let reports: Vec<CapturedReport> = (0..10)
            .map(|i| CapturedReport {
                ts_ns: 1_000_000_000 + i * 1_000_000,
                vid: "0x346E".to_string(),
                pid: "0x0002".to_string(),
                report: "01008000000000".to_string(),
            })
            .collect();

        for (i, a) in reports.iter().enumerate() {
            for (j, b) in reports.iter().enumerate() {
                if i != j {
                    assert_ne!(a.ts_ns, b.ts_ns, "reports {i} and {j} share timestamp");
                }
            }
        }
        Ok(())
    }

    #[test]
    fn captured_reports_differ_by_report_content() -> anyhow::Result<()> {
        let hex_payloads = [
            "01008000000000",
            "01ffff00000000",
            "0100000000ffff",
            "01aaaabbbbcccc",
        ];
        let reports: Vec<CapturedReport> = hex_payloads
            .iter()
            .enumerate()
            .map(|(i, r)| CapturedReport {
                ts_ns: 1000 + i as u64,
                vid: "0x346E".to_string(),
                pid: "0x0002".to_string(),
                report: r.to_string(),
            })
            .collect();

        for (i, a) in reports.iter().enumerate() {
            for (j, b) in reports.iter().enumerate() {
                if i != j {
                    assert_ne!(a.report, b.report);
                }
            }
        }
        Ok(())
    }
}

// ── ID serialization / deserialization ──────────────────────────────────────

mod id_serde {
    use super::*;

    #[test]
    fn captured_report_json_field_names() -> anyhow::Result<()> {
        let report = CapturedReport {
            ts_ns: 42,
            vid: "0x0001".to_string(),
            pid: "0x0002".to_string(),
            report: "aa".to_string(),
        };
        let json = serde_json::to_string(&report)?;
        assert!(json.contains("\"ts_ns\""), "missing ts_ns field in {json}");
        assert!(json.contains("\"vid\""), "missing vid field in {json}");
        assert!(json.contains("\"pid\""), "missing pid field in {json}");
        assert!(json.contains("\"report\""), "missing report field in {json}");
        Ok(())
    }

    #[test]
    fn captured_report_clone_preserves_fields() -> anyhow::Result<()> {
        let original = CapturedReport {
            ts_ns: 123_456_789,
            vid: "0x346E".to_string(),
            pid: "0x0002".to_string(),
            report: "deadbeef".to_string(),
        };
        let cloned = original.clone();
        assert_eq!(cloned.ts_ns, original.ts_ns);
        assert_eq!(cloned.vid, original.vid);
        assert_eq!(cloned.pid, original.pid);
        assert_eq!(cloned.report, original.report);
        Ok(())
    }

    #[test]
    fn captured_report_debug_format_contains_fields() {
        let report = CapturedReport {
            ts_ns: 100,
            vid: "0x046D".to_string(),
            pid: "0x0001".to_string(),
            report: "ff".to_string(),
        };
        let debug = format!("{report:?}");
        assert!(debug.contains("ts_ns"), "debug missing ts_ns: {debug}");
        assert!(debug.contains("046D"), "debug missing vid: {debug}");
    }

    #[test]
    fn captured_report_roundtrip_all_boundary_timestamps() -> anyhow::Result<()> {
        for ts in [0u64, 1, u64::MAX / 2, u64::MAX] {
            let original = CapturedReport {
                ts_ns: ts,
                vid: "0x0001".to_string(),
                pid: "0x0001".to_string(),
                report: "00".to_string(),
            };
            let json = serde_json::to_string(&original)?;
            let parsed = parse_capture_line(&json)?;
            assert_eq!(parsed.ts_ns, ts, "roundtrip failed for ts={ts}");
        }
        Ok(())
    }

    #[test]
    fn captured_report_with_long_report_hex() -> anyhow::Result<()> {
        let long_hex: String = (0u8..=63).map(|b| format!("{b:02x}")).collect();
        let original = CapturedReport {
            ts_ns: 999,
            vid: "0x346E".to_string(),
            pid: "0x0002".to_string(),
            report: long_hex.clone(),
        };
        let json = serde_json::to_string(&original)?;
        let parsed = parse_capture_line(&json)?;
        assert_eq!(parsed.report, long_hex);
        Ok(())
    }

    #[test]
    fn captured_report_deserialization_ignores_extra_fields() -> anyhow::Result<()> {
        let line = r#"{"ts_ns":42,"vid":"0x0001","pid":"0x0002","report":"ff","extra":"ignored"}"#;
        let entry = parse_capture_line(line)?;
        assert_eq!(entry.ts_ns, 42);
        assert_eq!(entry.report, "ff");
        Ok(())
    }

    #[test]
    fn parse_capture_line_rejects_wrong_types() {
        // ts_ns as string instead of number
        let line = r#"{"ts_ns":"42","vid":"0x0001","pid":"0x0002","report":"ff"}"#;
        assert!(parse_capture_line(line).is_err());
    }
}

// ── ID parsing from strings ────────────────────────────────────────────────

mod id_parsing {
    use super::*;

    #[test]
    fn parse_hex_id_decimal_fallback() -> anyhow::Result<()> {
        // parse_hex_id tries hex first; if that fails, falls back to decimal
        assert_eq!(parse_hex_id("100")?, 0x100);
        Ok(())
    }

    #[test]
    fn parse_hex_id_all_single_digit_hex() -> anyhow::Result<()> {
        for digit in 0u16..=9 {
            let s = format!("{digit}");
            let parsed = parse_hex_id(&s)?;
            assert_eq!(parsed, digit, "failed for '{s}'");
        }
        Ok(())
    }

    #[test]
    fn parse_hex_id_mixed_case_hex_digits() -> anyhow::Result<()> {
        assert_eq!(parse_hex_id("0xAbCd")?, 0xABCD);
        assert_eq!(parse_hex_id("0xabcd")?, 0xABCD);
        assert_eq!(parse_hex_id("0xABCD")?, 0xABCD);
        Ok(())
    }

    #[test]
    fn parse_hex_id_boundary_values() -> anyhow::Result<()> {
        assert_eq!(parse_hex_id("0x0000")?, 0);
        assert_eq!(parse_hex_id("0x0001")?, 1);
        assert_eq!(parse_hex_id("0xFFFE")?, 0xFFFE);
        assert_eq!(parse_hex_id("0xFFFF")?, 0xFFFF);
        Ok(())
    }

    #[test]
    fn parse_hex_id_error_message_contains_input() {
        let result = parse_hex_id("not_valid_at_all");
        assert!(result.is_err());
        let err_msg = result.as_ref().err().map(|e| format!("{e:#}")).unwrap_or_default();
        assert!(
            err_msg.contains("not_valid_at_all"),
            "error should mention input, got: {err_msg}"
        );
    }

    #[test]
    fn parse_vid_str_mixed_case_prefix() -> anyhow::Result<()> {
        assert_eq!(parse_vid_str("0X046D")?, 0x046D);
        assert_eq!(parse_vid_str("0x046d")?, 0x046D);
        Ok(())
    }

    #[test]
    fn parse_vid_str_whitespace_handling() -> anyhow::Result<()> {
        assert_eq!(parse_vid_str("  0x046D  ")?, 0x046D);
        assert_eq!(parse_vid_str("\t046D\n")?, 0x046D);
        Ok(())
    }

    #[test]
    fn parse_vid_str_boundary_values() -> anyhow::Result<()> {
        assert_eq!(parse_vid_str("0000")?, 0);
        assert_eq!(parse_vid_str("0001")?, 1);
        assert_eq!(parse_vid_str("FFFE")?, 0xFFFE);
        assert_eq!(parse_vid_str("FFFF")?, 0xFFFF);
        Ok(())
    }
}

// ── ID validation ──────────────────────────────────────────────────────────

mod id_validation {
    use super::*;

    #[test]
    fn decode_hex_validates_even_length() {
        assert!(decode_hex("0").is_err());
        assert!(decode_hex("012").is_err());
        assert!(decode_hex("01234").is_err());
    }

    #[test]
    fn decode_hex_validates_hex_chars() {
        assert!(decode_hex("gg").is_err());
        assert!(decode_hex("zz").is_err());
        assert!(decode_hex("0xAA").is_err()); // "0x" prefix is not valid hex bytes
    }

    #[test]
    fn decode_hex_all_256_byte_values() -> anyhow::Result<()> {
        let all_bytes: Vec<u8> = (0u8..=255).collect();
        let hex: String = all_bytes.iter().map(|b| format!("{b:02x}")).collect();
        let decoded = decode_hex(&hex)?;
        assert_eq!(decoded.len(), 256);
        for (i, byte) in decoded.iter().enumerate() {
            assert_eq!(*byte, i as u8, "mismatch at index {i}");
        }
        Ok(())
    }

    #[test]
    fn decode_hex_uppercase_valid() -> anyhow::Result<()> {
        assert_eq!(decode_hex("AABBCCDD")?, vec![0xAA, 0xBB, 0xCC, 0xDD]);
        Ok(())
    }

    #[test]
    fn decode_hex_mixed_case_valid() -> anyhow::Result<()> {
        assert_eq!(decode_hex("aAbBcCdD")?, vec![0xAA, 0xBB, 0xCC, 0xDD]);
        Ok(())
    }

    #[test]
    fn parse_hex_id_rejects_empty_string() {
        assert!(parse_hex_id("").is_err());
    }

    #[test]
    fn parse_hex_id_rejects_bare_prefix() {
        assert!(parse_hex_id("0x").is_err());
    }

    #[test]
    fn parse_vid_str_rejects_empty_string() {
        assert!(parse_vid_str("").is_err());
    }

    #[test]
    fn parse_vid_str_rejects_non_hex_chars() {
        assert!(parse_vid_str("WXYZ").is_err());
        assert!(parse_vid_str("0xGGGG").is_err());
    }

    #[test]
    fn parse_capture_line_rejects_empty_json() {
        assert!(parse_capture_line("{}").is_err());
    }

    #[test]
    fn parse_capture_line_rejects_plain_text() {
        assert!(parse_capture_line("hello world").is_err());
    }

    #[test]
    fn parse_capture_line_rejects_partial_fields() {
        // Missing pid and report
        let line = r#"{"ts_ns":1000,"vid":"0x046D"}"#;
        assert!(parse_capture_line(line).is_err());
    }
}

// ── ID comparison and ordering ─────────────────────────────────────────────

mod id_comparison {
    use super::*;

    #[test]
    fn hex_u16_lexicographic_order_matches_numeric_for_same_length() {
        // All hex_u16 outputs are exactly 6 chars ("0xNNNN"), so lexicographic
        // order should match numeric order
        let values = [0u16, 1, 0xFF, 0x100, 0x346E, 0x046D, 0xFFFE, 0xFFFF];
        let mut sorted_by_string: Vec<u16> = values.to_vec();
        sorted_by_string.sort_by_key(|v| hex_u16(*v));

        let mut sorted_numerically = values.to_vec();
        sorted_numerically.sort();

        assert_eq!(sorted_by_string, sorted_numerically);
    }

    #[test]
    fn parse_hex_id_symmetry_with_hex_u16() -> anyhow::Result<()> {
        // For every value, format → parse should yield the original
        let test_values: Vec<u16> = vec![
            0, 1, 2, 0xF, 0xFF, 0xFFF, 0x346E, 0x046D, 0x8000, 0xFFFF,
        ];
        for val in test_values {
            let formatted = hex_u16(val);
            let parsed = parse_hex_id(&formatted)?;
            assert_eq!(parsed, val, "symmetry failed for {val} (formatted: {formatted})");
        }
        Ok(())
    }

    #[test]
    fn captured_report_timestamp_ordering() -> anyhow::Result<()> {
        let lines = [
            r#"{"ts_ns":100,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#,
            r#"{"ts_ns":200,"vid":"0x046D","pid":"0x0001","report":"01008000000000000800"}"#,
            r#"{"ts_ns":300,"vid":"0x346E","pid":"0x0002","report":"01ffff00000000"}"#,
            r#"{"ts_ns":400,"vid":"0x046D","pid":"0x0001","report":"0100800000ff00000800"}"#,
        ];
        let mut timestamps = Vec::new();
        for line in &lines {
            let entry = parse_capture_line(line)?;
            timestamps.push(entry.ts_ns);
        }
        for window in timestamps.windows(2) {
            assert!(
                window[0] < window[1],
                "timestamps not strictly ordered: {} >= {}",
                window[0],
                window[1]
            );
        }
        Ok(())
    }

    #[test]
    fn vid_numeric_comparison_after_parsing() -> anyhow::Result<()> {
        let moza_vid = parse_vid_str("0x346E")?;
        let logi_vid = parse_vid_str("0x046D")?;
        assert!(moza_vid > logi_vid, "MOZA VID should be numerically larger");
        Ok(())
    }
}

// ── Vendor decode dispatch ─────────────────────────────────────────────────

mod decode_dispatch {
    use super::*;

    #[test]
    fn all_known_vids_decode_with_valid_reports() -> anyhow::Result<()> {
        let moza_report: [u8; 7] = [0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00];
        let logi_report: [u8; 10] =
            [0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00];

        let moza = decode_report(0x346E, &moza_report)
            .ok_or_else(|| anyhow::anyhow!("MOZA should decode"))?;
        let logi = decode_report(0x046D, &logi_report)
            .ok_or_else(|| anyhow::anyhow!("Logitech should decode"))?;

        assert!(moza.starts_with("MOZA:"));
        assert!(logi.starts_with("Logitech:"));

        // Cross-contamination check
        assert!(!moza.contains("Logitech"));
        assert!(!logi.contains("MOZA"));
        Ok(())
    }

    #[test]
    fn decode_report_returns_none_for_many_unknown_vids() {
        let report: [u8; 10] = [0x01; 10];
        let unknown_vids: [u16; 8] = [0x0000, 0x0001, 0x1234, 0x5678, 0x9ABC, 0xBEEF, 0xDEAD, 0xFFFF];
        for vid in unknown_vids {
            assert!(
                decode_report(vid, &report).is_none(),
                "VID {vid:#06X} should return None"
            );
        }
    }

    #[test]
    fn moza_zero_position_report() -> anyhow::Result<()> {
        let report: [u8; 7] = [0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let text = decode_report(0x346E, &report)
            .ok_or_else(|| anyhow::anyhow!("zero position should decode"))?;
        assert!(text.contains("steering=0.000"));
        assert!(text.contains("throttle=0.000"));
        assert!(text.contains("brake=0.000"));
        Ok(())
    }

    #[test]
    fn moza_max_brake_report() -> anyhow::Result<()> {
        let report: [u8; 7] = [0x01, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF];
        let text = decode_report(0x346E, &report)
            .ok_or_else(|| anyhow::anyhow!("max brake should decode"))?;
        assert!(text.contains("brake=1.000"));
        Ok(())
    }

    #[test]
    fn logitech_full_right_steering() -> anyhow::Result<()> {
        // steering = 0xFFFF → full right → +1.0
        let report: [u8; 10] = [0x01, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00];
        let text = decode_report(0x046D, &report)
            .ok_or_else(|| anyhow::anyhow!("full right steering should decode"))?;
        assert!(text.contains("steering=1.000"), "got: {text}");
        Ok(())
    }

    #[test]
    fn logitech_button_state_encoded() -> anyhow::Result<()> {
        // buttons in bytes 7-8 (little-endian): 0x00FF
        let report: [u8; 10] = [0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x08, 0x00];
        let text = decode_report(0x046D, &report)
            .ok_or_else(|| anyhow::anyhow!("button state should decode"))?;
        assert!(text.contains("buttons="), "got: {text}");
        Ok(())
    }

    #[test]
    fn decode_single_byte_report_returns_none_for_known_vids() {
        assert!(decode_report(0x346E, &[0x01]).is_none());
        assert!(decode_report(0x046D, &[0x01]).is_none());
    }
}

// ── Hex format consistency ─────────────────────────────────────────────────

mod hex_format {
    use super::*;

    #[test]
    fn hex_u16_always_six_chars() {
        for val in [0u16, 1, 0xFF, 0x100, 0x1000, 0xFFFF] {
            let formatted = hex_u16(val);
            assert_eq!(
                formatted.len(),
                6,
                "hex_u16({val}) = '{formatted}' should be 6 chars"
            );
        }
    }

    #[test]
    fn hex_u16_always_starts_with_0x() {
        for val in [0u16, 0x7FFF, 0xFFFF] {
            let formatted = hex_u16(val);
            assert!(
                formatted.starts_with("0x"),
                "hex_u16({val}) = '{formatted}' should start with 0x"
            );
        }
    }

    #[test]
    fn hex_u16_uses_uppercase_hex_digits() {
        let formatted = hex_u16(0xABCD);
        assert_eq!(formatted, "0xABCD");
        // Ensure no lowercase a-f
        let digits = &formatted[2..];
        assert_eq!(digits, digits.to_uppercase());
    }

    #[test]
    fn hex_encode_decode_roundtrip_all_single_bytes() -> anyhow::Result<()> {
        for byte_val in 0u8..=255 {
            let hex = format!("{byte_val:02x}");
            let decoded = decode_hex(&hex)?;
            assert_eq!(decoded.len(), 1);
            assert_eq!(decoded[0], byte_val, "roundtrip failed for byte {byte_val}");
        }
        Ok(())
    }
}
