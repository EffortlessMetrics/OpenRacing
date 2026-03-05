//! Tests for HID capture replay timing accuracy and protocol extraction.
//!
//! Covers: replay timing fidelity, inter-report timing relationships,
//! protocol extraction from captures, multi-vendor interleaved replay,
//! corrupt capture line handling, and capture file I/O edge cases.

use openracing_capture_ids::replay::{
    CapturedReport, decode_hex, parse_capture_line, parse_vid_str,
};
use openracing_capture_ids::{decode_report, hex_u16, parse_hex_id};

// ═════════════════════════════════════════════════════════════════════════════
// Replay timing accuracy
// ═════════════════════════════════════════════════════════════════════════════

mod replay_timing {
    use super::*;

    fn make_capture_lines(count: usize, interval_ns: u64) -> Vec<String> {
        (0..count)
            .map(|i| {
                let ts = 1_000_000_000u64 + (i as u64) * interval_ns;
                format!(
                    r#"{{"ts_ns":{ts},"vid":"0x346E","pid":"0x0002","report":"01008000000000"}}"#
                )
            })
            .collect()
    }

    #[test]
    fn timing_deltas_preserved_through_parse() -> anyhow::Result<()> {
        let lines = make_capture_lines(100, 1_000_000); // 1ms intervals
        let reports: Vec<CapturedReport> = lines
            .iter()
            .map(|l| parse_capture_line(l))
            .collect::<anyhow::Result<Vec<_>>>()?;

        for pair in reports.windows(2) {
            let delta = pair[1].ts_ns - pair[0].ts_ns;
            assert_eq!(delta, 1_000_000, "expected 1ms interval, got {delta}ns");
        }
        Ok(())
    }

    #[test]
    fn timing_deltas_from_first_report() -> anyhow::Result<()> {
        let lines = make_capture_lines(10, 2_000_000); // 2ms intervals
        let reports: Vec<CapturedReport> = lines
            .iter()
            .map(|l| parse_capture_line(l))
            .collect::<anyhow::Result<Vec<_>>>()?;

        let first_ts = reports[0].ts_ns;
        for (i, r) in reports.iter().enumerate() {
            let expected_delta = (i as u64) * 2_000_000;
            let actual_delta = r.ts_ns.saturating_sub(first_ts);
            assert_eq!(
                actual_delta, expected_delta,
                "frame {i}: expected delta {expected_delta}, got {actual_delta}"
            );
        }
        Ok(())
    }

    #[test]
    fn replay_at_double_speed_halves_intervals() -> anyhow::Result<()> {
        let lines = make_capture_lines(10, 10_000_000); // 10ms intervals
        let reports: Vec<CapturedReport> = lines
            .iter()
            .map(|l| parse_capture_line(l))
            .collect::<anyhow::Result<Vec<_>>>()?;

        let speed = 2.0f64;
        let first_ts = reports[0].ts_ns;

        for (i, r) in reports.iter().enumerate().skip(1) {
            let original_delta = r.ts_ns.saturating_sub(first_ts);
            let scaled_delta_ns = (original_delta as f64 / speed) as u64;
            let expected_ns = (i as u64) * 5_000_000; // 5ms at 2x
            assert_eq!(
                scaled_delta_ns, expected_ns,
                "frame {i}: scaled delta {scaled_delta_ns} != expected {expected_ns}"
            );
        }
        Ok(())
    }

    #[test]
    fn replay_at_half_speed_doubles_intervals() -> anyhow::Result<()> {
        let lines = make_capture_lines(5, 4_000_000); // 4ms intervals
        let reports: Vec<CapturedReport> = lines
            .iter()
            .map(|l| parse_capture_line(l))
            .collect::<anyhow::Result<Vec<_>>>()?;

        let speed = 0.5f64;
        let first_ts = reports[0].ts_ns;

        for (i, r) in reports.iter().enumerate().skip(1) {
            let original_delta = r.ts_ns.saturating_sub(first_ts);
            let scaled_delta_ns = (original_delta as f64 / speed) as u64;
            let expected_ns = (i as u64) * 8_000_000; // 8ms at 0.5x
            assert_eq!(
                scaled_delta_ns, expected_ns,
                "frame {i}: scaled delta {scaled_delta_ns} != expected {expected_ns}"
            );
        }
        Ok(())
    }

    #[test]
    fn timestamps_monotonically_increase_in_parsed_capture() -> anyhow::Result<()> {
        let lines = make_capture_lines(500, 1_000_000);
        let reports: Vec<CapturedReport> = lines
            .iter()
            .map(|l| parse_capture_line(l))
            .collect::<anyhow::Result<Vec<_>>>()?;

        for pair in reports.windows(2) {
            assert!(
                pair[1].ts_ns > pair[0].ts_ns,
                "timestamp regression: {} <= {}",
                pair[1].ts_ns,
                pair[0].ts_ns
            );
        }
        Ok(())
    }

    #[test]
    fn zero_speed_replay_skips_all_timing() -> anyhow::Result<()> {
        // speed=0.0 means print everything immediately — just verify
        // the timing calculation doesn't divide by zero
        let lines = make_capture_lines(5, 10_000_000);
        let reports: Vec<CapturedReport> = lines
            .iter()
            .map(|l| parse_capture_line(l))
            .collect::<anyhow::Result<Vec<_>>>()?;

        let speed = 0.0f64;
        let first_ts = reports[0].ts_ns;

        for r in &reports[1..] {
            let original_delta = r.ts_ns.saturating_sub(first_ts);
            // At speed 0, no sleeping — just verify delta is valid
            assert!(original_delta > 0);
            if speed > 0.0 {
                let _scaled = (original_delta as f64 / speed) as u64;
            }
            // No sleep needed when speed is 0
        }
        Ok(())
    }

    #[test]
    fn large_timestamp_gap_handled() -> anyhow::Result<()> {
        let lines = [
            r#"{"ts_ns":1000000000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#
                .to_string(),
            // 60 second gap
            r#"{"ts_ns":61000000000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#
                .to_string(),
        ];
        let reports: Vec<CapturedReport> = lines
            .iter()
            .map(|l| parse_capture_line(l))
            .collect::<anyhow::Result<Vec<_>>>()?;

        let gap = reports[1].ts_ns - reports[0].ts_ns;
        assert_eq!(gap, 60_000_000_000); // 60 seconds in nanoseconds
        Ok(())
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Protocol extraction from captures
// ═════════════════════════════════════════════════════════════════════════════

mod protocol_extraction {
    use super::*;

    #[test]
    fn extract_moza_steering_from_capture() -> anyhow::Result<()> {
        // MOZA report: steering at center (0x8000 = 32768)
        let line =
            r#"{"ts_ns":1000000000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#;
        let entry = parse_capture_line(line)?;
        let bytes = decode_hex(&entry.report)?;
        let vid = parse_vid_str(&entry.vid)?;
        let decoded = decode_report(vid, &bytes)
            .ok_or_else(|| anyhow::anyhow!("MOZA decode returned None"))?;
        assert!(decoded.contains("steering="), "missing steering: {decoded}");
        assert!(decoded.contains("MOZA:"), "missing MOZA prefix: {decoded}");
        Ok(())
    }

    #[test]
    fn extract_logitech_buttons_from_capture() -> anyhow::Result<()> {
        // Logitech report with button flags
        let line =
            r#"{"ts_ns":2000000000,"vid":"0x046D","pid":"0x0001","report":"01008000000000000800"}"#;
        let entry = parse_capture_line(line)?;
        let bytes = decode_hex(&entry.report)?;
        let vid = parse_vid_str(&entry.vid)?;
        let decoded = decode_report(vid, &bytes)
            .ok_or_else(|| anyhow::anyhow!("Logitech decode returned None"))?;
        assert!(decoded.contains("buttons="), "missing buttons: {decoded}");
        Ok(())
    }

    #[test]
    fn extract_protocol_from_stream_of_captures() -> anyhow::Result<()> {
        let lines = [
            r#"{"ts_ns":1000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#,
            r#"{"ts_ns":2000,"vid":"0x346E","pid":"0x0002","report":"01ff8000000000"}"#,
            r#"{"ts_ns":3000,"vid":"0x346E","pid":"0x0002","report":"0100ff00000000"}"#,
        ];

        let mut decoded_values = Vec::new();
        for line in &lines {
            let entry = parse_capture_line(line)?;
            let bytes = decode_hex(&entry.report)?;
            let vid = parse_vid_str(&entry.vid)?;
            if let Some(decoded) = decode_report(vid, &bytes) {
                decoded_values.push(decoded);
            }
        }

        assert_eq!(decoded_values.len(), 3, "all MOZA reports should decode");
        for d in &decoded_values {
            assert!(d.starts_with("MOZA:"));
        }
        Ok(())
    }

    #[test]
    fn unknown_vendor_report_extraction_returns_none() -> anyhow::Result<()> {
        let line =
            r#"{"ts_ns":1000,"vid":"0x1234","pid":"0x5678","report":"0102030405060708090a"}"#;
        let entry = parse_capture_line(line)?;
        let bytes = decode_hex(&entry.report)?;
        let vid = parse_vid_str(&entry.vid)?;
        assert!(decode_report(vid, &bytes).is_none());
        Ok(())
    }

    #[test]
    fn extract_from_interleaved_multi_vendor_stream() -> anyhow::Result<()> {
        let lines = [
            r#"{"ts_ns":1000,"vid":"0x346E","pid":"0x0002","report":"01008000000000"}"#,
            r#"{"ts_ns":1500,"vid":"0x046D","pid":"0x0001","report":"01008000000000000800"}"#,
            r#"{"ts_ns":2000,"vid":"0x346E","pid":"0x0002","report":"01ffff00000000"}"#,
            r#"{"ts_ns":2500,"vid":"0x046D","pid":"0x0001","report":"0100ff00000000000800"}"#,
        ];

        let mut moza = Vec::new();
        let mut logi = Vec::new();

        for line in &lines {
            let entry = parse_capture_line(line)?;
            let bytes = decode_hex(&entry.report)?;
            let vid = parse_vid_str(&entry.vid)?;
            if let Some(decoded) = decode_report(vid, &bytes) {
                if decoded.starts_with("MOZA:") {
                    moza.push(decoded);
                } else if decoded.starts_with("Logitech:") {
                    logi.push(decoded);
                }
            }
        }

        assert_eq!(moza.len(), 2);
        assert_eq!(logi.len(), 2);
        Ok(())
    }

    #[test]
    fn short_report_for_known_vendor_returns_none() -> anyhow::Result<()> {
        // Too short for MOZA (needs 7 bytes for wheelbase input)
        let short_bytes = decode_hex("0100")?;
        assert!(decode_report(0x346E, &short_bytes).is_none());

        // Too short for Logitech (needs 10 bytes)
        assert!(decode_report(0x046D, &short_bytes).is_none());
        Ok(())
    }

    #[test]
    fn empty_report_for_known_vendor_returns_none() {
        assert!(decode_report(0x346E, &[]).is_none());
        assert!(decode_report(0x046D, &[]).is_none());
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Capture line error handling
// ═════════════════════════════════════════════════════════════════════════════

mod capture_error_handling {
    use super::*;

    #[test]
    fn parse_empty_line_fails() {
        assert!(parse_capture_line("").is_err());
    }

    #[test]
    fn parse_whitespace_only_fails() {
        assert!(parse_capture_line("   ").is_err());
    }

    #[test]
    fn parse_incomplete_json_object_fails() {
        assert!(parse_capture_line(r#"{"ts_ns":1000"#).is_err());
    }

    #[test]
    fn parse_line_with_extra_fields_succeeds() -> anyhow::Result<()> {
        let line =
            r#"{"ts_ns":1000,"vid":"0x346E","pid":"0x0002","report":"01","extra":"ignored"}"#;
        let entry = parse_capture_line(line)?;
        assert_eq!(entry.ts_ns, 1000);
        Ok(())
    }

    #[test]
    fn parse_line_wrong_field_types() {
        // ts_ns as string instead of number
        let line = r#"{"ts_ns":"1000","vid":"0x346E","pid":"0x0002","report":"01"}"#;
        assert!(parse_capture_line(line).is_err());
    }

    #[test]
    fn decode_hex_with_spaces_fails() {
        // decode_hex expects contiguous hex, not space-separated
        assert!(decode_hex("01 02 03").is_err());
    }

    #[test]
    fn decode_hex_single_char_fails() {
        assert!(decode_hex("f").is_err());
    }

    #[test]
    fn parse_vid_str_empty_fails() {
        assert!(parse_vid_str("").is_err());
    }

    #[test]
    fn parse_vid_str_pure_whitespace_fails() {
        assert!(parse_vid_str("   ").is_err());
    }

    #[test]
    fn parse_hex_id_garbage_fails() {
        assert!(parse_hex_id("not_hex").is_err());
        assert!(parse_hex_id("0xGGGG").is_err());
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Capture file I/O with tempfile
// ═════════════════════════════════════════════════════════════════════════════

mod capture_file_io {
    use super::*;
    use std::io::{BufRead, BufReader, Write};

    fn write_jsonl_capture(reports: &[CapturedReport]) -> anyhow::Result<std::path::PathBuf> {
        let path = std::env::temp_dir().join(format!(
            "cap_test_{}.jsonl",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let mut f = std::fs::File::create(&path)?;
        for r in reports {
            let line = serde_json::to_string(r)?;
            writeln!(f, "{line}")?;
        }
        Ok(path)
    }

    #[test]
    fn jsonl_write_and_read_roundtrip() -> anyhow::Result<()> {
        let reports: Vec<CapturedReport> = (0..10)
            .map(|i| CapturedReport {
                ts_ns: 1_000_000_000 + i * 1_000_000,
                vid: "0x346E".to_string(),
                pid: "0x0002".to_string(),
                report: "01008000000000".to_string(),
            })
            .collect();

        let path = write_jsonl_capture(&reports)?;
        let file = std::fs::File::open(&path)?;
        let reader = BufReader::new(file);
        let mut parsed = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            parsed.push(parse_capture_line(&line)?);
        }

        assert_eq!(parsed.len(), 10);
        for (orig, read) in reports.iter().zip(parsed.iter()) {
            assert_eq!(orig.ts_ns, read.ts_ns);
            assert_eq!(orig.vid, read.vid);
            assert_eq!(orig.report, read.report);
        }
        std::fs::remove_file(&path)?;
        Ok(())
    }

    #[test]
    fn jsonl_empty_file_produces_no_reports() -> anyhow::Result<()> {
        let path = write_jsonl_capture(&[])?;
        let file = std::fs::File::open(&path)?;
        let reader = BufReader::new(file);
        let count = reader.lines().count();
        assert_eq!(count, 0);
        std::fs::remove_file(&path)?;
        Ok(())
    }

    #[test]
    fn jsonl_with_blank_lines_skipped() -> anyhow::Result<()> {
        let path = std::env::temp_dir().join("cap_test_blanks.jsonl");
        let mut f = std::fs::File::create(&path)?;
        writeln!(
            f,
            r#"{{"ts_ns":1000,"vid":"0x346E","pid":"0x0002","report":"01"}}"#
        )?;
        writeln!(f)?; // blank line
        writeln!(
            f,
            r#"{{"ts_ns":2000,"vid":"0x346E","pid":"0x0002","report":"02"}}"#
        )?;
        drop(f);

        let file = std::fs::File::open(&path)?;
        let reader = BufReader::new(file);
        let mut parsed = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            parsed.push(parse_capture_line(&line)?);
        }
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].ts_ns, 1000);
        assert_eq!(parsed[1].ts_ns, 2000);
        std::fs::remove_file(&path)?;
        Ok(())
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// hex_u16 and parse_hex_id extended coverage
// ═════════════════════════════════════════════════════════════════════════════

mod hex_utility_extended {
    use super::*;

    #[test]
    fn hex_u16_roundtrip_all_boundary_values() -> anyhow::Result<()> {
        for val in [0u16, 1, 0xFF, 0x100, 0x7FFF, 0x8000, 0xFFFE, 0xFFFF] {
            let formatted = hex_u16(val);
            let parsed = parse_hex_id(&formatted)?;
            assert_eq!(parsed, val, "roundtrip failed for {val:#06X}");
        }
        Ok(())
    }

    #[test]
    fn parse_hex_id_decimal_fallback() -> anyhow::Result<()> {
        // If hex fails, parse_hex_id tries decimal
        // "100" as hex = 0x100 = 256, as decimal = 100
        // It tries hex first with from_str_radix, which succeeds for "100" as hex
        let val = parse_hex_id("100")?;
        assert_eq!(val, 0x100); // parsed as hex
        Ok(())
    }

    #[test]
    fn decode_hex_full_byte_range() -> anyhow::Result<()> {
        let hex: String = (0u8..=255).map(|b| format!("{b:02x}")).collect();
        let bytes = decode_hex(&hex)?;
        assert_eq!(bytes.len(), 256);
        for (i, &b) in bytes.iter().enumerate() {
            assert_eq!(b, i as u8);
        }
        Ok(())
    }
}
