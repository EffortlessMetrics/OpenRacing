//! Deep tests for openracing-capture-ids.
//!
//! Covers: USB device ID format validation, hex encoding/decoding,
//! VID/PID parsing, capture line serialization, cross-vendor decode
//! routing, device ID deduplication in captures, and cross-platform
//! hex formatting.

use openracing_capture_ids::replay::{
    CapturedReport, decode_hex, parse_capture_line, parse_vid_str,
};
use openracing_capture_ids::{decode_report, hex_u16, parse_hex_id};

// ── USB device ID format validation ────────────────────────────────────────

mod id_format_validation {
    use super::*;

    #[test]
    fn hex_u16_zero() -> anyhow::Result<()> {
        assert_eq!(hex_u16(0), "0x0000");
        Ok(())
    }

    #[test]
    fn hex_u16_max() -> anyhow::Result<()> {
        assert_eq!(hex_u16(0xFFFF), "0xFFFF");
        Ok(())
    }

    #[test]
    fn hex_u16_known_vendors() -> anyhow::Result<()> {
        assert_eq!(hex_u16(0x346E), "0x346E"); // MOZA
        assert_eq!(hex_u16(0x046D), "0x046D"); // Logitech
        Ok(())
    }

    #[test]
    fn hex_u16_padding_preserved() -> anyhow::Result<()> {
        assert_eq!(hex_u16(0x0001), "0x0001");
        assert_eq!(hex_u16(0x0010), "0x0010");
        assert_eq!(hex_u16(0x0100), "0x0100");
        assert_eq!(hex_u16(0x1000), "0x1000");
        Ok(())
    }

    #[test]
    fn parse_hex_id_with_0x_prefix() -> anyhow::Result<()> {
        assert_eq!(parse_hex_id("0x346E")?, 0x346E);
        assert_eq!(parse_hex_id("0x046D")?, 0x046D);
        assert_eq!(parse_hex_id("0x0000")?, 0);
        assert_eq!(parse_hex_id("0xFFFF")?, 0xFFFF);
        Ok(())
    }

    #[test]
    fn parse_hex_id_with_0x_uppercase_prefix() -> anyhow::Result<()> {
        assert_eq!(parse_hex_id("0X00FF")?, 0x00FF);
        assert_eq!(parse_hex_id("0XABCD")?, 0xABCD);
        Ok(())
    }

    #[test]
    fn parse_hex_id_without_prefix() -> anyhow::Result<()> {
        assert_eq!(parse_hex_id("346E")?, 0x346E);
        assert_eq!(parse_hex_id("FFFF")?, 0xFFFF);
        assert_eq!(parse_hex_id("0000")?, 0);
        Ok(())
    }

    #[test]
    fn parse_hex_id_trims_whitespace() -> anyhow::Result<()> {
        assert_eq!(parse_hex_id("  0x046D  ")?, 0x046D);
        assert_eq!(parse_hex_id("\t0x346E\n")?, 0x346E);
        Ok(())
    }

    #[test]
    fn parse_hex_id_invalid_returns_error() {
        assert!(parse_hex_id("ZZZZ").is_err());
        assert!(parse_hex_id("").is_err());
        assert!(parse_hex_id("0x").is_err());
        assert!(parse_hex_id("not_a_number").is_err());
    }

    #[test]
    fn hex_u16_roundtrip_through_parse() -> anyhow::Result<()> {
        for val in [0u16, 1, 255, 0x346E, 0x046D, 0xFFFF, 0x8000] {
            let formatted = hex_u16(val);
            let parsed = parse_hex_id(&formatted)?;
            assert_eq!(parsed, val, "roundtrip failed for {val}");
        }
        Ok(())
    }
}

// ── Device ID deduplication (capture line uniqueness) ──────────────────────

mod device_id_deduplication {
    use super::*;

    #[test]
    fn distinct_captures_have_different_timestamps() -> anyhow::Result<()> {
        let line1 =
            r#"{"ts_ns":1000000000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#;
        let line2 =
            r#"{"ts_ns":1001000000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#;
        let e1 = parse_capture_line(line1)?;
        let e2 = parse_capture_line(line2)?;
        assert_ne!(e1.ts_ns, e2.ts_ns);
        Ok(())
    }

    #[test]
    fn same_vid_pid_different_reports_are_distinct() -> anyhow::Result<()> {
        let line1 = r#"{"ts_ns":1000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#;
        let line2 = r#"{"ts_ns":1001,"vid":"0x346E","pid":"0x0002","report":"01ffff00000000"}"#;
        let e1 = parse_capture_line(line1)?;
        let e2 = parse_capture_line(line2)?;
        assert_eq!(e1.vid, e2.vid);
        assert_eq!(e1.pid, e2.pid);
        assert_ne!(e1.report, e2.report);
        Ok(())
    }

    #[test]
    fn different_vendors_in_same_capture() -> anyhow::Result<()> {
        let lines = [
            r#"{"ts_ns":1000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#,
            r#"{"ts_ns":1001,"vid":"0x046D","pid":"0x0001","report":"01008000000000000800"}"#,
        ];
        let entries: Vec<CapturedReport> = lines
            .iter()
            .map(|l| parse_capture_line(l))
            .collect::<anyhow::Result<Vec<_>>>()?;
        assert_ne!(entries[0].vid, entries[1].vid);
        Ok(())
    }

    #[test]
    fn timestamps_monotonically_increase() -> anyhow::Result<()> {
        let lines = [
            r#"{"ts_ns":1000000000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#,
            r#"{"ts_ns":1001000000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#,
            r#"{"ts_ns":1002000000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#,
            r#"{"ts_ns":1003000000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#,
        ];
        let mut prev = 0u64;
        for line in &lines {
            let entry = parse_capture_line(line)?;
            assert!(entry.ts_ns > prev, "{} should be > {prev}", entry.ts_ns);
            prev = entry.ts_ns;
        }
        Ok(())
    }
}

// ── Cross-platform device naming (hex decode/encode) ───────────────────────

mod cross_platform_naming {
    use super::*;

    #[test]
    fn decode_hex_basic() -> anyhow::Result<()> {
        assert_eq!(decode_hex("0102030405")?, vec![1, 2, 3, 4, 5]);
        Ok(())
    }

    #[test]
    fn decode_hex_empty() -> anyhow::Result<()> {
        assert!(decode_hex("")?.is_empty());
        Ok(())
    }

    #[test]
    fn decode_hex_all_zeros() -> anyhow::Result<()> {
        assert_eq!(decode_hex("0000000000")?, vec![0, 0, 0, 0, 0]);
        Ok(())
    }

    #[test]
    fn decode_hex_all_ff() -> anyhow::Result<()> {
        assert_eq!(decode_hex("ffff")?, vec![0xFF, 0xFF]);
        Ok(())
    }

    #[test]
    fn decode_hex_uppercase_chars() -> anyhow::Result<()> {
        assert_eq!(decode_hex("AABB")?, vec![0xAA, 0xBB]);
        Ok(())
    }

    #[test]
    fn decode_hex_odd_length_fails() {
        assert!(decode_hex("012").is_err());
        assert!(decode_hex("1").is_err());
        assert!(decode_hex("abc").is_err());
    }

    #[test]
    fn decode_hex_invalid_chars_fails() {
        assert!(decode_hex("0xZZ").is_err());
        assert!(decode_hex("gg").is_err());
    }

    #[test]
    fn hex_encode_decode_roundtrip() -> anyhow::Result<()> {
        let original: Vec<u8> = (0u8..=255).collect();
        let hex: String = original.iter().map(|b| format!("{b:02x}")).collect();
        let decoded = decode_hex(&hex)?;
        assert_eq!(decoded, original);
        Ok(())
    }

    #[test]
    fn parse_vid_str_with_prefix() -> anyhow::Result<()> {
        assert_eq!(parse_vid_str("0x046D")?, 0x046D);
        assert_eq!(parse_vid_str("0x346E")?, 0x346E);
        assert_eq!(parse_vid_str("0x0000")?, 0);
        assert_eq!(parse_vid_str("0xFFFF")?, 0xFFFF);
        Ok(())
    }

    #[test]
    fn parse_vid_str_without_prefix() -> anyhow::Result<()> {
        assert_eq!(parse_vid_str("046D")?, 0x046D);
        assert_eq!(parse_vid_str("FFFF")?, 0xFFFF);
        Ok(())
    }

    #[test]
    fn parse_vid_str_invalid_fails() {
        assert!(parse_vid_str("ZZZZ").is_err());
        assert!(parse_vid_str("").is_err());
    }
}

// ── Capture line serialization / deserialization ───────────────────────────

mod capture_serde {
    use super::*;

    #[test]
    fn parse_capture_line_all_fields() -> anyhow::Result<()> {
        let line = r#"{"ts_ns":1234567890,"vid":"0x046D","pid":"0x0002","report":"0102030405"}"#;
        let entry = parse_capture_line(line)?;
        assert_eq!(entry.ts_ns, 1_234_567_890);
        assert_eq!(entry.vid, "0x046D");
        assert_eq!(entry.pid, "0x0002");
        assert_eq!(entry.report, "0102030405");
        Ok(())
    }

    #[test]
    fn parse_capture_line_missing_field_fails() {
        let line = r#"{"ts_ns":1234,"vid":"0x046D","pid":"0x0002"}"#;
        assert!(parse_capture_line(line).is_err());
    }

    #[test]
    fn parse_capture_line_empty_report() -> anyhow::Result<()> {
        let line = r#"{"ts_ns":100,"vid":"0x046D","pid":"0x0002","report":""}"#;
        let entry = parse_capture_line(line)?;
        assert!(entry.report.is_empty());
        Ok(())
    }

    #[test]
    fn parse_capture_line_invalid_json() {
        assert!(parse_capture_line("not json").is_err());
        assert!(parse_capture_line("{incomplete").is_err());
        assert!(parse_capture_line("").is_err());
    }

    #[test]
    fn captured_report_roundtrip() -> anyhow::Result<()> {
        let original = CapturedReport {
            ts_ns: 9_999_999_000,
            vid: "0x346E".to_string(),
            pid: "0x0002".to_string(),
            report: "01ff80aabbccdd".to_string(),
        };
        let json = serde_json::to_string(&original)?;
        let parsed = parse_capture_line(&json)?;
        assert_eq!(parsed.ts_ns, original.ts_ns);
        assert_eq!(parsed.vid, original.vid);
        assert_eq!(parsed.pid, original.pid);
        assert_eq!(parsed.report, original.report);
        Ok(())
    }

    #[test]
    fn captured_report_large_timestamp() -> anyhow::Result<()> {
        let line = r#"{"ts_ns":18446744073709551615,"vid":"0x046D","pid":"0x0001","report":"00"}"#;
        let entry = parse_capture_line(line)?;
        assert_eq!(entry.ts_ns, u64::MAX);
        Ok(())
    }

    #[test]
    fn captured_report_zero_timestamp() -> anyhow::Result<()> {
        let line = r#"{"ts_ns":0,"vid":"0x046D","pid":"0x0001","report":"00"}"#;
        let entry = parse_capture_line(line)?;
        assert_eq!(entry.ts_ns, 0);
        Ok(())
    }
}

// ── Vendor decode routing ──────────────────────────────────────────────────

mod vendor_decode {
    use super::*;

    #[test]
    fn moza_vid_produces_moza_output() -> anyhow::Result<()> {
        let report: [u8; 7] = [0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00];
        let text =
            decode_report(0x346E, &report).ok_or_else(|| anyhow::anyhow!("MOZA decode failed"))?;
        assert!(text.starts_with("MOZA:"), "got: {text}");
        assert!(text.contains("steering="));
        assert!(text.contains("throttle="));
        assert!(text.contains("brake="));
        Ok(())
    }

    #[test]
    fn logitech_vid_produces_logitech_output() -> anyhow::Result<()> {
        let report: [u8; 10] = [0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00];
        let text = decode_report(0x046D, &report)
            .ok_or_else(|| anyhow::anyhow!("Logitech decode failed"))?;
        assert!(text.starts_with("Logitech:"), "got: {text}");
        assert!(text.contains("steering="));
        assert!(text.contains("buttons="));
        Ok(())
    }

    #[test]
    fn unknown_vid_returns_none() {
        let report: [u8; 10] = [0x01; 10];
        assert!(decode_report(0xFFFF, &report).is_none());
        assert!(decode_report(0x0000, &report).is_none());
        assert!(decode_report(0x1234, &report).is_none());
    }

    #[test]
    fn known_vid_short_report_returns_none() {
        let short: [u8; 2] = [0x01, 0x00];
        assert!(decode_report(0x346E, &short).is_none());
        assert!(decode_report(0x046D, &short).is_none());
    }

    #[test]
    fn empty_report_returns_none() {
        assert!(decode_report(0x346E, &[]).is_none());
        assert!(decode_report(0x046D, &[]).is_none());
    }

    #[test]
    fn wrong_report_id_returns_none() {
        let moza_wrong: [u8; 7] = [0x02, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00];
        assert!(decode_report(0x346E, &moza_wrong).is_none());

        let logi_wrong: [u8; 10] = [0x02, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00];
        assert!(decode_report(0x046D, &logi_wrong).is_none());
    }

    #[test]
    fn moza_full_deflection_values() -> anyhow::Result<()> {
        let report: [u8; 7] = [0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00];
        let text =
            decode_report(0x346E, &report).ok_or_else(|| anyhow::anyhow!("decode failed"))?;
        assert!(text.contains("steering=1.000"));
        assert!(text.contains("throttle=1.000"));
        Ok(())
    }
}

// ── Full pipeline: capture → parse → decode ────────────────────────────────

mod full_pipeline {
    use super::*;

    #[test]
    fn moza_capture_pipeline() -> anyhow::Result<()> {
        let line =
            r#"{"ts_ns":1000000000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#;
        let entry = parse_capture_line(line)?;
        let bytes = decode_hex(&entry.report)?;
        let vid = parse_vid_str(&entry.vid)?;
        let decoded =
            decode_report(vid, &bytes).ok_or_else(|| anyhow::anyhow!("pipeline decode failed"))?;
        assert!(decoded.starts_with("MOZA:"));
        Ok(())
    }

    #[test]
    fn logitech_capture_pipeline() -> anyhow::Result<()> {
        let line =
            r#"{"ts_ns":2000000000,"vid":"0x046D","pid":"0x0001","report":"01008000000000000800"}"#;
        let entry = parse_capture_line(line)?;
        let bytes = decode_hex(&entry.report)?;
        let vid = parse_vid_str(&entry.vid)?;
        let decoded =
            decode_report(vid, &bytes).ok_or_else(|| anyhow::anyhow!("pipeline decode failed"))?;
        assert!(decoded.starts_with("Logitech:"));
        Ok(())
    }

    #[test]
    fn interleaved_multi_vendor_captures() -> anyhow::Result<()> {
        let lines = [
            r#"{"ts_ns":1000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#,
            r#"{"ts_ns":1001,"vid":"0x046D","pid":"0x0001","report":"01008000000000000800"}"#,
            r#"{"ts_ns":1002,"vid":"0x346E","pid":"0x0002","report":"01ffff00000000"}"#,
        ];
        let mut moza_count = 0u32;
        let mut logi_count = 0u32;
        for line in &lines {
            let entry = parse_capture_line(line)?;
            let bytes = decode_hex(&entry.report)?;
            let vid = parse_vid_str(&entry.vid)?;
            if let Some(text) = decode_report(vid, &bytes) {
                if text.starts_with("MOZA:") {
                    moza_count += 1;
                } else if text.starts_with("Logitech:") {
                    logi_count += 1;
                }
            }
        }
        assert_eq!(moza_count, 2);
        assert_eq!(logi_count, 1);
        Ok(())
    }

    #[test]
    fn unknown_vendor_in_pipeline_returns_none() -> anyhow::Result<()> {
        let line =
            r#"{"ts_ns":3000,"vid":"0xFFFF","pid":"0x9999","report":"0102030405060708090a"}"#;
        let entry = parse_capture_line(line)?;
        let bytes = decode_hex(&entry.report)?;
        let vid = parse_vid_str(&entry.vid)?;
        assert!(decode_report(vid, &bytes).is_none());
        Ok(())
    }

    #[test]
    fn captured_report_serde_roundtrip_pipeline() -> anyhow::Result<()> {
        let original = CapturedReport {
            ts_ns: 42_000_000,
            vid: "0x346E".to_string(),
            pid: "0x0002".to_string(),
            report: "01008000000000".to_string(),
        };
        let json = serde_json::to_string(&original)?;
        let parsed = parse_capture_line(&json)?;
        let bytes = decode_hex(&parsed.report)?;
        let vid = parse_vid_str(&parsed.vid)?;
        let decoded = decode_report(vid, &bytes)
            .ok_or_else(|| anyhow::anyhow!("roundtrip pipeline failed"))?;
        assert!(decoded.starts_with("MOZA:"));
        Ok(())
    }
}
