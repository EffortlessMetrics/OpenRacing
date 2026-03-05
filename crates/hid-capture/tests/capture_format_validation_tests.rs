//! Tests for HID capture format writing, reading, seeking, and validation.
//!
//! Covers: CaptureFile/CaptureReport serialization fidelity, field boundary
//! values, corrupt JSON handling, truncated capture files, timestamp monotonicity
//! enforcement, and large capture file handling.

use racing_wheel_hid_capture::{CaptureFile, CaptureReport, parse_hex_u16};

type R = Result<(), Box<dyn std::error::Error>>;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn report(ts: u64, id: u8, hex: &str) -> CaptureReport {
    CaptureReport {
        timestamp_us: ts,
        report_id: id,
        data: hex.to_string(),
    }
}

fn sample_capture_file() -> CaptureFile {
    CaptureFile {
        vendor_id: "0x346E".to_string(),
        product_id: "0x0002".to_string(),
        captures: vec![
            report(0, 0x01, "01 00 80 00 00 00 00"),
            report(1000, 0x01, "01 00 81 00 00 00 00"),
            report(2000, 0x01, "01 00 82 00 00 00 00"),
        ],
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Capture format serialization round-trip
// ═════════════════════════════════════════════════════════════════════════════

mod format_roundtrip {
    use super::*;

    #[test]
    fn capture_file_json_roundtrip() -> R {
        let original = sample_capture_file();
        let json = serde_json::to_string(&original)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        assert_eq!(restored.vendor_id, original.vendor_id);
        assert_eq!(restored.product_id, original.product_id);
        assert_eq!(restored.captures.len(), original.captures.len());
        for (a, b) in restored.captures.iter().zip(original.captures.iter()) {
            assert_eq!(a.timestamp_us, b.timestamp_us);
            assert_eq!(a.report_id, b.report_id);
            assert_eq!(a.data, b.data);
        }
        Ok(())
    }

    #[test]
    fn capture_file_pretty_json_roundtrip() -> R {
        let original = sample_capture_file();
        let pretty = serde_json::to_string_pretty(&original)?;
        let restored: CaptureFile = serde_json::from_str(&pretty)?;
        assert_eq!(restored.captures.len(), 3);
        assert_eq!(restored.vendor_id, "0x346E");
        Ok(())
    }

    #[test]
    fn single_report_roundtrip() -> R {
        let r = report(42_000, 0xFF, "ff 00 aa bb cc");
        let json = serde_json::to_string(&r)?;
        let restored: CaptureReport = serde_json::from_str(&json)?;
        assert_eq!(restored.timestamp_us, 42_000);
        assert_eq!(restored.report_id, 0xFF);
        assert_eq!(restored.data, "ff 00 aa bb cc");
        Ok(())
    }

    #[test]
    fn empty_capture_file_roundtrip() -> R {
        let empty = CaptureFile {
            vendor_id: "0x0000".to_string(),
            product_id: "0x0000".to_string(),
            captures: vec![],
        };
        let json = serde_json::to_string(&empty)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        assert!(restored.captures.is_empty());
        Ok(())
    }

    #[test]
    fn empty_data_field_roundtrip() -> R {
        let r = report(0, 0x00, "");
        let json = serde_json::to_string(&r)?;
        let restored: CaptureReport = serde_json::from_str(&json)?;
        assert!(restored.data.is_empty());
        Ok(())
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Capture format field boundary values
// ═════════════════════════════════════════════════════════════════════════════

mod field_boundaries {
    use super::*;

    #[test]
    fn timestamp_max_u64() -> R {
        let r = report(u64::MAX, 0x01, "01");
        let json = serde_json::to_string(&r)?;
        let restored: CaptureReport = serde_json::from_str(&json)?;
        assert_eq!(restored.timestamp_us, u64::MAX);
        Ok(())
    }

    #[test]
    fn timestamp_zero() -> R {
        let r = report(0, 0x00, "00");
        let json = serde_json::to_string(&r)?;
        let restored: CaptureReport = serde_json::from_str(&json)?;
        assert_eq!(restored.timestamp_us, 0);
        Ok(())
    }

    #[test]
    fn report_id_all_values() -> R {
        for id in 0..=255u8 {
            let r = report(0, id, "00");
            let json = serde_json::to_string(&r)?;
            let restored: CaptureReport = serde_json::from_str(&json)?;
            assert_eq!(restored.report_id, id);
        }
        Ok(())
    }

    #[test]
    fn large_data_payload() -> R {
        let data = (0..=255u8)
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        let r = report(1000, 0x01, &data);
        let json = serde_json::to_string(&r)?;
        let restored: CaptureReport = serde_json::from_str(&json)?;
        assert_eq!(restored.data, data);
        Ok(())
    }

    #[test]
    fn vendor_id_special_chars_preserved() -> R {
        let file = CaptureFile {
            vendor_id: "0xFFFF".to_string(),
            product_id: "0x0000".to_string(),
            captures: vec![],
        };
        let json = serde_json::to_string(&file)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        assert_eq!(restored.vendor_id, "0xFFFF");
        Ok(())
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Corrupt / malformed capture handling
// ═════════════════════════════════════════════════════════════════════════════

mod corrupt_capture_handling {
    use super::*;

    #[test]
    fn truncated_json_fails_to_parse() {
        let json = r#"{"vendor_id":"0x346E","product_id":"0x0002","captures":[{"timestam"#;
        let result: Result<CaptureFile, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn missing_captures_array_fails() {
        let json = r#"{"vendor_id":"0x346E","product_id":"0x0002"}"#;
        let result: Result<CaptureFile, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn missing_vendor_id_fails() {
        let json = r#"{"product_id":"0x0002","captures":[]}"#;
        let result: Result<CaptureFile, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn wrong_type_for_timestamp_fails() {
        let json = r#"{"timestamp_us":"not_a_number","report_id":1,"data":"00"}"#;
        let result: Result<CaptureReport, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn negative_timestamp_fails() {
        let json = r#"{"timestamp_us":-1,"report_id":1,"data":"00"}"#;
        let result: Result<CaptureReport, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn report_id_overflow_fails() {
        let json = r#"{"timestamp_us":0,"report_id":256,"data":"00"}"#;
        let result: Result<CaptureReport, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn empty_json_string_fails() {
        let result: Result<CaptureFile, _> = serde_json::from_str("");
        assert!(result.is_err());
    }

    #[test]
    fn invalid_json_syntax_fails() {
        let result: Result<CaptureFile, _> = serde_json::from_str("not json at all");
        assert!(result.is_err());
    }

    #[test]
    fn null_json_fails() {
        let result: Result<CaptureFile, _> = serde_json::from_str("null");
        assert!(result.is_err());
    }

    #[test]
    fn captures_with_null_element_fails() {
        let json = r#"{"vendor_id":"0x346E","product_id":"0x0002","captures":[null]}"#;
        let result: Result<CaptureFile, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Timestamp ordering and seeking
// ═════════════════════════════════════════════════════════════════════════════

mod timestamp_ordering {
    use super::*;

    #[test]
    fn captures_preserve_insertion_order() -> R {
        let file = sample_capture_file();
        let json = serde_json::to_string(&file)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;

        let timestamps: Vec<u64> = restored.captures.iter().map(|c| c.timestamp_us).collect();
        assert_eq!(timestamps, vec![0, 1000, 2000]);
        Ok(())
    }

    #[test]
    fn seek_by_timestamp_window() -> R {
        let file = CaptureFile {
            vendor_id: "0x346E".to_string(),
            product_id: "0x0002".to_string(),
            captures: (0..100).map(|i| report(i * 1000, 0x01, "01 00")).collect(),
        };

        // Seek to window [10ms, 20ms)
        let window: Vec<&CaptureReport> = file
            .captures
            .iter()
            .filter(|c| c.timestamp_us >= 10_000 && c.timestamp_us < 20_000)
            .collect();
        assert_eq!(window.len(), 10);
        assert_eq!(window[0].timestamp_us, 10_000);
        assert_eq!(window[9].timestamp_us, 19_000);
        Ok(())
    }

    #[test]
    fn monotonic_timestamps_validated() -> R {
        let file = sample_capture_file();
        let timestamps: Vec<u64> = file.captures.iter().map(|c| c.timestamp_us).collect();
        for pair in timestamps.windows(2) {
            assert!(
                pair[1] > pair[0],
                "timestamps must be monotonically increasing"
            );
        }
        Ok(())
    }

    #[test]
    fn duplicate_timestamps_detectable() -> R {
        let file = CaptureFile {
            vendor_id: "0x346E".to_string(),
            product_id: "0x0002".to_string(),
            captures: vec![
                report(1000, 0x01, "01"),
                report(1000, 0x01, "02"), // duplicate ts
                report(2000, 0x01, "03"),
            ],
        };

        let timestamps: Vec<u64> = file.captures.iter().map(|c| c.timestamp_us).collect();
        let has_duplicates = timestamps.windows(2).any(|w| w[0] == w[1]);
        assert!(has_duplicates, "should detect duplicate timestamps");
        Ok(())
    }

    #[test]
    fn binary_search_by_timestamp() -> R {
        let file = CaptureFile {
            vendor_id: "0x346E".to_string(),
            product_id: "0x0002".to_string(),
            captures: (0..1000).map(|i| report(i * 1000, 0x01, "01")).collect(),
        };

        // Binary search for timestamp 500000 (index 500)
        let target = 500_000u64;
        let idx = file
            .captures
            .binary_search_by_key(&target, |c| c.timestamp_us)
            .map_err(|_| "timestamp not found")?;
        assert_eq!(idx, 500);
        assert_eq!(file.captures[idx].timestamp_us, target);
        Ok(())
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Disk I/O roundtrip via tempfile
// ═════════════════════════════════════════════════════════════════════════════

mod disk_io {
    use super::*;
    use std::io::Write;

    #[test]
    fn write_and_read_capture_file() -> R {
        let file = sample_capture_file();
        let json = serde_json::to_string_pretty(&file)?;

        let tmp = std::env::temp_dir().join("hid_cap_test_rw.json");
        std::fs::write(&tmp, &json)?;
        let contents = std::fs::read_to_string(&tmp)?;
        let restored: CaptureFile = serde_json::from_str(&contents)?;

        assert_eq!(restored.captures.len(), 3);
        assert_eq!(restored.vendor_id, "0x346E");
        std::fs::remove_file(&tmp)?;
        Ok(())
    }

    #[test]
    fn truncated_file_read_fails() -> R {
        let file = sample_capture_file();
        let json = serde_json::to_string(&file)?;

        // Write only half the JSON
        let truncated = &json[..json.len() / 2];
        let tmp = std::env::temp_dir().join("hid_cap_test_trunc.json");
        std::fs::write(&tmp, truncated)?;
        let contents = std::fs::read_to_string(&tmp)?;
        let result: Result<CaptureFile, _> = serde_json::from_str(&contents);
        assert!(result.is_err());
        std::fs::remove_file(&tmp)?;
        Ok(())
    }

    #[test]
    fn appended_garbage_after_valid_json_detected() -> R {
        let file = sample_capture_file();
        let json = serde_json::to_string(&file)?;

        let tmp = std::env::temp_dir().join("hid_cap_test_garbage.json");
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(json.as_bytes())?;
        f.write_all(b"\n{garbage data here")?;
        drop(f);

        // serde_json::from_str detects trailing characters as an error
        let contents = std::fs::read_to_string(&tmp)?;
        let result: Result<CaptureFile, _> = serde_json::from_str(&contents);
        // Trailing garbage after valid JSON is properly rejected
        assert!(result.is_err());
        std::fs::remove_file(&tmp)?;
        Ok(())
    }

    #[test]
    fn large_capture_file_write_read() -> R {
        let file = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0x0001".to_string(),
            captures: (0..10_000)
                .map(|i| report(i * 1000, (i % 4) as u8, "01 02 03 04 05 06 07 08"))
                .collect(),
        };

        let json = serde_json::to_string(&file)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        assert_eq!(restored.captures.len(), 10_000);
        assert_eq!(restored.captures[9999].timestamp_us, 9_999_000);
        Ok(())
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// parse_hex_u16 edge cases
// ═════════════════════════════════════════════════════════════════════════════

mod hex_parsing {
    use super::*;

    #[test]
    fn parse_hex_u16_with_prefix() -> R {
        assert_eq!(parse_hex_u16("0x046D")?, 0x046D);
        assert_eq!(parse_hex_u16("0X346E")?, 0x346E);
        Ok(())
    }

    #[test]
    fn parse_hex_u16_without_prefix() -> R {
        assert_eq!(parse_hex_u16("FFFF")?, 0xFFFF);
        assert_eq!(parse_hex_u16("0000")?, 0x0000);
        Ok(())
    }

    #[test]
    fn parse_hex_u16_mixed_case() -> R {
        assert_eq!(parse_hex_u16("abcd")?, 0xABCD);
        assert_eq!(parse_hex_u16("AbCd")?, 0xABCD);
        Ok(())
    }

    #[test]
    fn parse_hex_u16_invalid_returns_error() {
        assert!(parse_hex_u16("ZZZZ").is_err());
        assert!(parse_hex_u16("").is_err());
        assert!(parse_hex_u16("FFFFF").is_err()); // overflow
    }

    #[test]
    fn parse_hex_u16_known_vendors() -> R {
        // MOZA Racing
        assert_eq!(parse_hex_u16("0x346E")?, 0x346E);
        // Logitech
        assert_eq!(parse_hex_u16("0x046D")?, 0x046D);
        // Fanatec
        assert_eq!(parse_hex_u16("0x0EB7")?, 0x0EB7);
        // Thrustmaster
        assert_eq!(parse_hex_u16("0x044F")?, 0x044F);
        Ok(())
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Capture metadata: device descriptors, timestamps
// ═════════════════════════════════════════════════════════════════════════════

mod capture_metadata {
    use super::*;

    #[test]
    fn capture_preserves_device_identity() -> R {
        let file = CaptureFile {
            vendor_id: "0x346E".to_string(),
            product_id: "0x0002".to_string(),
            captures: vec![report(0, 0x01, "01")],
        };
        let json = serde_json::to_string(&file)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        assert_eq!(
            parse_hex_u16(&restored.vendor_id)?,
            0x346E,
            "vendor ID mismatch"
        );
        assert_eq!(
            parse_hex_u16(&restored.product_id)?,
            0x0002,
            "product ID mismatch"
        );
        Ok(())
    }

    #[test]
    fn capture_duration_from_timestamps() -> R {
        let file = CaptureFile {
            vendor_id: "0x346E".to_string(),
            product_id: "0x0002".to_string(),
            captures: vec![
                report(1_000_000, 0x01, "01"),
                report(2_000_000, 0x01, "02"),
                report(3_500_000, 0x01, "03"),
            ],
        };

        let first_ts = file.captures.first().map(|c| c.timestamp_us);
        let last_ts = file.captures.last().map(|c| c.timestamp_us);
        let duration_us = match (first_ts, last_ts) {
            (Some(f), Some(l)) => l.saturating_sub(f),
            _ => 0,
        };
        assert_eq!(duration_us, 2_500_000); // 2.5 seconds
        Ok(())
    }

    #[test]
    fn capture_rate_estimation() -> R {
        // Simulate 1kHz capture: 1000 reports in 1 second
        let file = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0x0001".to_string(),
            captures: (0..1000).map(|i| report(i * 1000, 0x01, "01")).collect(),
        };

        let first = file.captures.first().map(|c| c.timestamp_us).unwrap_or(0);
        let last = file.captures.last().map(|c| c.timestamp_us).unwrap_or(0);
        let duration_s = (last - first) as f64 / 1_000_000.0;
        let rate_hz = if duration_s > 0.0 {
            (file.captures.len() - 1) as f64 / duration_s
        } else {
            0.0
        };
        // Should be ~1000 Hz (999 intervals in 0.999s)
        assert!(
            (rate_hz - 1000.0).abs() < 2.0,
            "expected ~1000 Hz, got {rate_hz}"
        );
        Ok(())
    }

    #[test]
    fn multi_device_captures_distinguished_by_vid_pid() -> R {
        let moza = CaptureFile {
            vendor_id: "0x346E".to_string(),
            product_id: "0x0002".to_string(),
            captures: vec![report(0, 0x01, "01")],
        };
        let logi = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0x0001".to_string(),
            captures: vec![report(0, 0x01, "01")],
        };

        assert_ne!(moza.vendor_id, logi.vendor_id);
        assert_ne!(moza.product_id, logi.product_id);
        Ok(())
    }
}
