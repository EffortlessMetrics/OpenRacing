//! Tests for protocol analysis, timing statistics, vendor detection, capture
//! format validation, community sharing format, and replay pipeline utilities.
//!
//! Covers: timing analysis (jitter, rate, latency distribution), protocol
//! detection from VID, capture format validation, shared capture roundtrip,
//! filter by report ID, monotonic timestamp validation, and edge cases.

use racing_wheel_hid_capture::{
    CaptureFile, CaptureMetadata, CaptureReport, CaptureValidationError, KnownVendor,
    SharedCaptureFile, capture_duration_us, compute_timing_stats, detect_vendor,
    detect_vendor_by_id, filter_by_report_id, to_shared_format, validate_capture_file,
    validate_monotonic_timestamps, validate_shared_capture_file, vendor_name,
};

type R = Result<(), Box<dyn std::error::Error>>;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn report(ts: u64, id: u8, hex: &str) -> CaptureReport {
    CaptureReport {
        timestamp_us: ts,
        report_id: id,
        data: hex.to_string(),
    }
}

fn make_1khz_captures(count: u64) -> Vec<CaptureReport> {
    (0..count)
        .map(|i| report(i * 1000, 0x01, "01 00 80 00 00 00 00"))
        .collect()
}

fn sample_metadata() -> CaptureMetadata {
    CaptureMetadata {
        format_version: "1.0".to_string(),
        captured_at: "2025-01-15T12:00:00Z".to_string(),
        platform: "windows".to_string(),
        tool_version: "hid-capture 0.1.0".to_string(),
        description: "Test capture".to_string(),
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Timing analysis
// ═════════════════════════════════════════════════════════════════════════════

mod timing_analysis {
    use super::*;

    #[test]
    fn stats_for_uniform_1khz_stream() -> R {
        let captures = make_1khz_captures(1001);
        let stats = compute_timing_stats(&captures).ok_or("expected stats for 1001 captures")?;
        assert_eq!(stats.count, 1000);
        assert!(
            (stats.mean_us - 1000.0).abs() < 0.01,
            "mean should be 1000us"
        );
        assert!(
            (stats.median_us - 1000.0).abs() < 0.01,
            "median should be 1000us"
        );
        assert!((stats.min_us - 1000.0).abs() < 0.01);
        assert!((stats.max_us - 1000.0).abs() < 0.01);
        assert!(
            stats.jitter_us.abs() < 0.01,
            "jitter should be ~0 for uniform stream"
        );
        assert!(
            stats.std_dev_us < 0.01,
            "std_dev should be ~0 for uniform stream"
        );
        assert!(
            (stats.estimated_rate_hz - 1000.0).abs() < 1.0,
            "rate should be ~1000 Hz, got {}",
            stats.estimated_rate_hz
        );
        Ok(())
    }

    #[test]
    fn stats_for_varying_intervals() -> R {
        let captures = vec![
            report(0, 0x01, "01"),
            report(1000, 0x01, "01"),  // 1ms
            report(3000, 0x01, "01"),  // 2ms
            report(6000, 0x01, "01"),  // 3ms
            report(10000, 0x01, "01"), // 4ms
        ];
        let stats = compute_timing_stats(&captures).ok_or("expected stats")?;
        assert_eq!(stats.count, 4);
        // intervals: 1000, 2000, 3000, 4000 → mean = 2500
        assert!((stats.mean_us - 2500.0).abs() < 0.01);
        // median of [1000, 2000, 3000, 4000] = (2000 + 3000) / 2 = 2500
        assert!((stats.median_us - 2500.0).abs() < 0.01);
        assert!((stats.min_us - 1000.0).abs() < 0.01);
        assert!((stats.max_us - 4000.0).abs() < 0.01);
        assert!((stats.jitter_us - 3000.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn stats_returns_none_for_empty() {
        assert!(compute_timing_stats(&[]).is_none());
    }

    #[test]
    fn stats_returns_none_for_single_report() {
        let captures = vec![report(0, 0x01, "01")];
        assert!(compute_timing_stats(&captures).is_none());
    }

    #[test]
    fn stats_two_reports() -> R {
        let captures = vec![report(0, 0x01, "01"), report(500, 0x01, "01")];
        let stats = compute_timing_stats(&captures).ok_or("expected stats")?;
        assert_eq!(stats.count, 1);
        assert!((stats.mean_us - 500.0).abs() < 0.01);
        assert!((stats.estimated_rate_hz - 2000.0).abs() < 1.0);
        Ok(())
    }

    #[test]
    fn stats_p99_for_large_set() -> R {
        // 100 intervals of 1000us each, plus 1 outlier of 5000us
        let mut captures: Vec<CaptureReport> = Vec::new();
        let mut ts = 0u64;
        captures.push(report(ts, 0x01, "01"));
        for i in 0..100 {
            ts += if i == 50 { 5000 } else { 1000 };
            captures.push(report(ts, 0x01, "01"));
        }
        let stats = compute_timing_stats(&captures).ok_or("expected stats")?;
        assert_eq!(stats.count, 100);
        // p99 should be the outlier or near it
        assert!(stats.p99_us >= 1000.0);
        assert!(stats.max_us >= 5000.0 - 0.01);
        Ok(())
    }

    #[test]
    fn jitter_for_perfectly_uniform_is_zero() -> R {
        let captures = make_1khz_captures(100);
        let stats = compute_timing_stats(&captures).ok_or("expected stats")?;
        assert!(stats.jitter_us.abs() < 0.01);
        Ok(())
    }

    #[test]
    fn estimated_rate_500hz() -> R {
        let captures: Vec<CaptureReport> = (0..501).map(|i| report(i * 2000, 0x01, "01")).collect();
        let stats = compute_timing_stats(&captures).ok_or("expected stats")?;
        assert!(
            (stats.estimated_rate_hz - 500.0).abs() < 1.0,
            "expected ~500Hz, got {}",
            stats.estimated_rate_hz
        );
        Ok(())
    }

    #[test]
    fn std_dev_nonzero_for_variable_intervals() -> R {
        let captures = vec![
            report(0, 0x01, "01"),
            report(500, 0x01, "01"),
            report(2500, 0x01, "01"),
        ];
        let stats = compute_timing_stats(&captures).ok_or("expected stats")?;
        assert!(stats.std_dev_us > 0.0);
        Ok(())
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Vendor / protocol detection
// ═════════════════════════════════════════════════════════════════════════════

mod vendor_detection {
    use super::*;

    #[test]
    fn detect_moza_by_vid_string() {
        assert_eq!(detect_vendor("0x346E"), Some(KnownVendor::Moza));
    }

    #[test]
    fn detect_logitech_by_vid_string() {
        assert_eq!(detect_vendor("0x046D"), Some(KnownVendor::Logitech));
    }

    #[test]
    fn detect_fanatec_by_vid_string() {
        assert_eq!(detect_vendor("0x0EB7"), Some(KnownVendor::Fanatec));
    }

    #[test]
    fn detect_thrustmaster_by_vid_string() {
        assert_eq!(detect_vendor("0x044F"), Some(KnownVendor::Thrustmaster));
    }

    #[test]
    fn detect_unknown_vendor() {
        assert_eq!(detect_vendor("0x9999"), None);
    }

    #[test]
    fn detect_vendor_by_numeric_id() {
        assert_eq!(detect_vendor_by_id(0x346E), Some(KnownVendor::Moza));
        assert_eq!(detect_vendor_by_id(0x046D), Some(KnownVendor::Logitech));
        assert_eq!(detect_vendor_by_id(0x0000), None);
        assert_eq!(detect_vendor_by_id(0xFFFF), None);
    }

    #[test]
    fn detect_vendor_invalid_string() {
        assert_eq!(detect_vendor("ZZZZ"), None);
        assert_eq!(detect_vendor(""), None);
    }

    #[test]
    fn vendor_name_all_known() {
        assert_eq!(vendor_name(KnownVendor::Moza), "MOZA Racing");
        assert_eq!(vendor_name(KnownVendor::Logitech), "Logitech");
        assert_eq!(vendor_name(KnownVendor::Fanatec), "Fanatec");
        assert_eq!(vendor_name(KnownVendor::Thrustmaster), "Thrustmaster");
        assert_eq!(vendor_name(KnownVendor::Simucube), "Simucube");
        assert_eq!(vendor_name(KnownVendor::CammusDirect), "Cammus");
        assert_eq!(vendor_name(KnownVendor::AccuForce), "AccuForce");
        assert_eq!(vendor_name(KnownVendor::VRS), "VRS DirectForce");
        assert_eq!(vendor_name(KnownVendor::Heusinkveld), "Heusinkveld");
        assert_eq!(vendor_name(KnownVendor::Simagic), "Simagic");
    }

    #[test]
    fn detect_vendor_case_insensitive_hex() {
        assert_eq!(detect_vendor("0x346e"), Some(KnownVendor::Moza));
        assert_eq!(detect_vendor("346E"), Some(KnownVendor::Moza));
        assert_eq!(detect_vendor("346e"), Some(KnownVendor::Moza));
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Capture format validation
// ═════════════════════════════════════════════════════════════════════════════

mod format_validation {
    use super::*;

    #[test]
    fn valid_capture_file_no_errors() {
        let file = CaptureFile {
            vendor_id: "0x346E".to_string(),
            product_id: "0x0002".to_string(),
            captures: vec![report(0, 0x01, "01"), report(1000, 0x01, "02")],
        };
        let errors = validate_capture_file(&file);
        assert!(errors.is_empty(), "expected no errors, got {errors:?}");
    }

    #[test]
    fn invalid_vendor_id_detected() {
        let file = CaptureFile {
            vendor_id: "not-hex".to_string(),
            product_id: "0x0002".to_string(),
            captures: vec![],
        };
        let errors = validate_capture_file(&file);
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, CaptureValidationError::InvalidVendorId(_)))
        );
    }

    #[test]
    fn invalid_product_id_detected() {
        let file = CaptureFile {
            vendor_id: "0x346E".to_string(),
            product_id: "xyz".to_string(),
            captures: vec![],
        };
        let errors = validate_capture_file(&file);
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, CaptureValidationError::InvalidProductId(_)))
        );
    }

    #[test]
    fn non_monotonic_timestamp_detected() {
        let file = CaptureFile {
            vendor_id: "0x346E".to_string(),
            product_id: "0x0002".to_string(),
            captures: vec![report(2000, 0x01, "01"), report(1000, 0x01, "02")],
        };
        let errors = validate_capture_file(&file);
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, CaptureValidationError::NonMonotonicTimestamp { .. }))
        );
    }

    #[test]
    fn duplicate_timestamps_flagged_as_non_monotonic() {
        let file = CaptureFile {
            vendor_id: "0x346E".to_string(),
            product_id: "0x0002".to_string(),
            captures: vec![report(1000, 0x01, "01"), report(1000, 0x01, "02")],
        };
        let errors = validate_capture_file(&file);
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, CaptureValidationError::NonMonotonicTimestamp { .. }))
        );
    }

    #[test]
    fn empty_captures_valid() {
        let file = CaptureFile {
            vendor_id: "0x0000".to_string(),
            product_id: "0xFFFF".to_string(),
            captures: vec![],
        };
        let errors = validate_capture_file(&file);
        assert!(errors.is_empty());
    }

    #[test]
    fn multiple_validation_errors_reported() {
        let file = CaptureFile {
            vendor_id: "bad".to_string(),
            product_id: "worse".to_string(),
            captures: vec![report(5000, 0x01, "01"), report(1000, 0x01, "02")],
        };
        let errors = validate_capture_file(&file);
        // invalid VID + invalid PID + non-monotonic timestamp = at least 2+
        assert!(
            errors.len() >= 2,
            "expected ≥2 errors, got {}",
            errors.len()
        );
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Shared capture format (community sharing)
// ═════════════════════════════════════════════════════════════════════════════

mod shared_capture_format {
    use super::*;

    #[test]
    fn shared_capture_json_roundtrip() -> R {
        let shared = SharedCaptureFile {
            metadata: sample_metadata(),
            vendor_id: "0x346E".to_string(),
            product_id: "0x0002".to_string(),
            captures: vec![report(0, 0x01, "01"), report(1000, 0x01, "02")],
        };
        let json = serde_json::to_string(&shared)?;
        let restored: SharedCaptureFile = serde_json::from_str(&json)?;
        assert_eq!(restored.metadata.format_version, "1.0");
        assert_eq!(restored.metadata.platform, "windows");
        assert_eq!(restored.vendor_id, "0x346E");
        assert_eq!(restored.captures.len(), 2);
        Ok(())
    }

    #[test]
    fn shared_capture_pretty_json_roundtrip() -> R {
        let shared = SharedCaptureFile {
            metadata: sample_metadata(),
            vendor_id: "0x046D".to_string(),
            product_id: "0x0001".to_string(),
            captures: make_1khz_captures(10),
        };
        let pretty = serde_json::to_string_pretty(&shared)?;
        let restored: SharedCaptureFile = serde_json::from_str(&pretty)?;
        assert_eq!(restored.captures.len(), 10);
        assert_eq!(restored.metadata.tool_version, "hid-capture 0.1.0");
        Ok(())
    }

    #[test]
    fn shared_capture_metadata_equality() {
        let a = sample_metadata();
        let b = sample_metadata();
        assert_eq!(a, b);
    }

    #[test]
    fn to_shared_format_conversion() -> R {
        let file = CaptureFile {
            vendor_id: "0x346E".to_string(),
            product_id: "0x0002".to_string(),
            captures: vec![report(0, 0x01, "01"), report(1000, 0x01, "02")],
        };
        let shared = to_shared_format(
            &file,
            "linux",
            "hid-capture 0.2.0",
            "2025-06-01T00:00:00Z",
            "A test",
        );
        assert_eq!(shared.metadata.format_version, "1.0");
        assert_eq!(shared.metadata.platform, "linux");
        assert_eq!(shared.metadata.tool_version, "hid-capture 0.2.0");
        assert_eq!(shared.metadata.description, "A test");
        assert_eq!(shared.vendor_id, "0x346E");
        assert_eq!(shared.captures.len(), 2);
        Ok(())
    }

    #[test]
    fn shared_capture_validation_valid() {
        let shared = SharedCaptureFile {
            metadata: sample_metadata(),
            vendor_id: "0x346E".to_string(),
            product_id: "0x0002".to_string(),
            captures: vec![report(0, 0x01, "01"), report(1000, 0x01, "02")],
        };
        let errors = validate_shared_capture_file(&shared);
        assert!(errors.is_empty(), "expected no errors, got {errors:?}");
    }

    #[test]
    fn shared_capture_validation_bad_version() {
        let shared = SharedCaptureFile {
            metadata: CaptureMetadata {
                format_version: "99.0".to_string(),
                ..sample_metadata()
            },
            vendor_id: "0x346E".to_string(),
            product_id: "0x0002".to_string(),
            captures: vec![],
        };
        let errors = validate_shared_capture_file(&shared);
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, CaptureValidationError::UnsupportedFormatVersion(_)))
        );
    }

    #[test]
    fn shared_capture_disk_roundtrip() -> R {
        let shared = SharedCaptureFile {
            metadata: sample_metadata(),
            vendor_id: "0x046D".to_string(),
            product_id: "0x0001".to_string(),
            captures: make_1khz_captures(50),
        };
        let json = serde_json::to_string_pretty(&shared)?;
        let tmp = std::env::temp_dir().join("shared_cap_roundtrip.json");
        std::fs::write(&tmp, &json)?;
        let read_back = std::fs::read_to_string(&tmp)?;
        let restored: SharedCaptureFile = serde_json::from_str(&read_back)?;
        assert_eq!(restored.captures.len(), 50);
        assert_eq!(restored.metadata.format_version, "1.0");
        std::fs::remove_file(&tmp)?;
        Ok(())
    }

    #[test]
    fn shared_capture_empty_description() -> R {
        let file = CaptureFile {
            vendor_id: "0x346E".to_string(),
            product_id: "0x0002".to_string(),
            captures: vec![],
        };
        let shared = to_shared_format(&file, "macos", "v0.1", "2025-01-01T00:00:00Z", "");
        assert!(shared.metadata.description.is_empty());
        let json = serde_json::to_string(&shared)?;
        let restored: SharedCaptureFile = serde_json::from_str(&json)?;
        assert!(restored.metadata.description.is_empty());
        Ok(())
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Monotonic timestamp validation
// ═════════════════════════════════════════════════════════════════════════════

mod monotonic_validation {
    use super::*;

    #[test]
    fn monotonic_returns_none_for_sorted() {
        let captures = vec![
            report(100, 0x01, "01"),
            report(200, 0x01, "02"),
            report(300, 0x01, "03"),
        ];
        assert!(validate_monotonic_timestamps(&captures).is_none());
    }

    #[test]
    fn monotonic_returns_index_for_regression() {
        let captures = vec![
            report(100, 0x01, "01"),
            report(300, 0x01, "02"),
            report(200, 0x01, "03"),
        ];
        assert_eq!(validate_monotonic_timestamps(&captures), Some(2));
    }

    #[test]
    fn monotonic_returns_index_for_duplicate() {
        let captures = vec![report(100, 0x01, "01"), report(100, 0x01, "02")];
        assert_eq!(validate_monotonic_timestamps(&captures), Some(1));
    }

    #[test]
    fn monotonic_empty_returns_none() {
        assert!(validate_monotonic_timestamps(&[]).is_none());
    }

    #[test]
    fn monotonic_single_returns_none() {
        let captures = vec![report(42, 0x01, "01")];
        assert!(validate_monotonic_timestamps(&captures).is_none());
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Filter by report ID
// ═════════════════════════════════════════════════════════════════════════════

mod filter_tests {
    use super::*;

    #[test]
    fn filter_selects_matching_report_id() {
        let captures = vec![
            report(0, 0x01, "a"),
            report(1, 0x02, "b"),
            report(2, 0x01, "c"),
            report(3, 0x03, "d"),
        ];
        let filtered = filter_by_report_id(&captures, 0x01);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].data, "a");
        assert_eq!(filtered[1].data, "c");
    }

    #[test]
    fn filter_returns_empty_for_no_match() {
        let captures = vec![report(0, 0x01, "a"), report(1, 0x02, "b")];
        let filtered = filter_by_report_id(&captures, 0xFF);
        assert!(filtered.is_empty());
    }

    #[test]
    fn filter_on_empty_returns_empty() {
        let filtered = filter_by_report_id(&[], 0x01);
        assert!(filtered.is_empty());
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Capture duration
// ═════════════════════════════════════════════════════════════════════════════

mod duration_tests {
    use super::*;

    #[test]
    fn duration_of_empty_is_zero() {
        assert_eq!(capture_duration_us(&[]), 0);
    }

    #[test]
    fn duration_of_single_is_zero() {
        assert_eq!(capture_duration_us(&[report(5000, 0x01, "01")]), 0);
    }

    #[test]
    fn duration_computed_correctly() {
        let captures = vec![report(1000, 0x01, "01"), report(6000, 0x01, "02")];
        assert_eq!(capture_duration_us(&captures), 5000);
    }

    #[test]
    fn duration_1khz_1sec() {
        let captures = make_1khz_captures(1001);
        assert_eq!(capture_duration_us(&captures), 1_000_000);
    }
}
