//! Tests for vendor detection, timing analysis, and replay validation
//! pipeline in openracing-capture-ids.
//!
//! Covers: vendor auto-detection from VID, timing statistics from captured
//! report streams, replay validation pipeline (decode success/failure stats),
//! multi-vendor interleaved captures, and edge cases.

use openracing_capture_ids::replay::CapturedReport;
use openracing_capture_ids::{
    DetectedVendor, analyze_capture_timing, detect_vendor_from_vid,
    detect_vendor_from_vid_str, validate_replay_pipeline, vendor_label,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

fn moza_report(ts_ns: u64) -> CapturedReport {
    CapturedReport {
        ts_ns,
        vid: "0x346E".to_string(),
        pid: "0x0002".to_string(),
        report: "01008000000000".to_string(),
    }
}

fn logitech_report(ts_ns: u64) -> CapturedReport {
    CapturedReport {
        ts_ns,
        vid: "0x046D".to_string(),
        pid: "0x0001".to_string(),
        report: "01008000000000000800".to_string(),
    }
}

fn unknown_report(ts_ns: u64) -> CapturedReport {
    CapturedReport {
        ts_ns,
        vid: "0x9999".to_string(),
        pid: "0x0001".to_string(),
        report: "0102030405060708".to_string(),
    }
}

fn make_timed_reports(count: usize, interval_ns: u64) -> Vec<CapturedReport> {
    (0..count)
        .map(|i| moza_report(1_000_000_000 + (i as u64) * interval_ns))
        .collect()
}

// ═════════════════════════════════════════════════════════════════════════════
// Vendor detection
// ═════════════════════════════════════════════════════════════════════════════

mod vendor_detection {
    use super::*;

    #[test]
    fn detect_moza() {
        assert_eq!(detect_vendor_from_vid(0x346E), DetectedVendor::Moza);
    }

    #[test]
    fn detect_logitech() {
        assert_eq!(detect_vendor_from_vid(0x046D), DetectedVendor::Logitech);
    }

    #[test]
    fn detect_fanatec() {
        assert_eq!(detect_vendor_from_vid(0x0EB7), DetectedVendor::Fanatec);
    }

    #[test]
    fn detect_thrustmaster() {
        assert_eq!(detect_vendor_from_vid(0x044F), DetectedVendor::Thrustmaster);
    }

    #[test]
    fn detect_unknown_vid() {
        assert_eq!(detect_vendor_from_vid(0x0000), DetectedVendor::Unknown);
        assert_eq!(detect_vendor_from_vid(0xFFFF), DetectedVendor::Unknown);
        assert_eq!(detect_vendor_from_vid(0x1234), DetectedVendor::Unknown);
    }

    #[test]
    fn detect_from_vid_str_hex_prefix() {
        assert_eq!(detect_vendor_from_vid_str("0x346E"), DetectedVendor::Moza);
        assert_eq!(
            detect_vendor_from_vid_str("0x046D"),
            DetectedVendor::Logitech
        );
    }

    #[test]
    fn detect_from_vid_str_no_prefix() {
        assert_eq!(detect_vendor_from_vid_str("346E"), DetectedVendor::Moza);
    }

    #[test]
    fn detect_from_vid_str_invalid() {
        assert_eq!(detect_vendor_from_vid_str("ZZZZ"), DetectedVendor::Unknown);
        assert_eq!(detect_vendor_from_vid_str(""), DetectedVendor::Unknown);
    }

    #[test]
    fn vendor_labels_correct() {
        assert_eq!(vendor_label(DetectedVendor::Moza), "MOZA Racing");
        assert_eq!(vendor_label(DetectedVendor::Logitech), "Logitech");
        assert_eq!(vendor_label(DetectedVendor::Fanatec), "Fanatec");
        assert_eq!(vendor_label(DetectedVendor::Thrustmaster), "Thrustmaster");
        assert_eq!(vendor_label(DetectedVendor::Unknown), "Unknown");
    }

    #[test]
    fn vendor_label_unknown_is_not_empty() {
        assert!(!vendor_label(DetectedVendor::Unknown).is_empty());
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Timing analysis for captured reports
// ═════════════════════════════════════════════════════════════════════════════

mod timing_analysis {
    use super::*;

    #[test]
    fn timing_1khz_stream() -> anyhow::Result<()> {
        let reports = make_timed_reports(1001, 1_000_000); // 1ms intervals
        let stats =
            analyze_capture_timing(&reports).ok_or_else(|| anyhow::anyhow!("expected stats"))?;
        assert_eq!(stats.interval_count, 1000);
        assert!(
            (stats.mean_ns - 1_000_000.0).abs() < 1.0,
            "mean should be 1ms"
        );
        assert!(stats.jitter_ns.abs() < 1.0, "jitter should be ~0");
        assert!(
            (stats.estimated_rate_hz - 1000.0).abs() < 1.0,
            "rate should be ~1000 Hz"
        );
        Ok(())
    }

    #[test]
    fn timing_returns_none_for_empty() {
        assert!(analyze_capture_timing(&[]).is_none());
    }

    #[test]
    fn timing_returns_none_for_single() {
        assert!(analyze_capture_timing(&[moza_report(1000)]).is_none());
    }

    #[test]
    fn timing_two_reports() -> anyhow::Result<()> {
        let reports = vec![moza_report(1_000_000_000), moza_report(1_002_000_000)];
        let stats =
            analyze_capture_timing(&reports).ok_or_else(|| anyhow::anyhow!("expected stats"))?;
        assert_eq!(stats.interval_count, 1);
        assert!((stats.mean_ns - 2_000_000.0).abs() < 1.0);
        assert!(
            (stats.estimated_rate_hz - 500.0).abs() < 1.0,
            "expected 500 Hz"
        );
        Ok(())
    }

    #[test]
    fn timing_variable_intervals() -> anyhow::Result<()> {
        let reports = vec![
            moza_report(1_000_000_000),
            moza_report(1_001_000_000), // 1ms
            moza_report(1_003_000_000), // 2ms
            moza_report(1_006_000_000), // 3ms
            moza_report(1_010_000_000), // 4ms
        ];
        let stats =
            analyze_capture_timing(&reports).ok_or_else(|| anyhow::anyhow!("expected stats"))?;
        assert_eq!(stats.interval_count, 4);
        assert!((stats.mean_ns - 2_500_000.0).abs() < 1.0);
        assert!((stats.min_ns - 1_000_000.0).abs() < 1.0);
        assert!((stats.max_ns - 4_000_000.0).abs() < 1.0);
        assert!((stats.jitter_ns - 3_000_000.0).abs() < 1.0);
        Ok(())
    }

    #[test]
    fn timing_std_dev_nonzero_for_mixed() -> anyhow::Result<()> {
        let reports = vec![moza_report(0), moza_report(500_000), moza_report(2_500_000)];
        let stats =
            analyze_capture_timing(&reports).ok_or_else(|| anyhow::anyhow!("expected stats"))?;
        assert!(stats.std_dev_ns > 0.0);
        Ok(())
    }

    #[test]
    fn timing_p99_for_large_uniform_set() -> anyhow::Result<()> {
        let reports = make_timed_reports(501, 1_000_000);
        let stats =
            analyze_capture_timing(&reports).ok_or_else(|| anyhow::anyhow!("expected stats"))?;
        assert!((stats.p99_ns - 1_000_000.0).abs() < 1.0);
        Ok(())
    }

    #[test]
    fn timing_250hz_stream() -> anyhow::Result<()> {
        let reports = make_timed_reports(101, 4_000_000); // 4ms = 250 Hz
        let stats =
            analyze_capture_timing(&reports).ok_or_else(|| anyhow::anyhow!("expected stats"))?;
        assert!(
            (stats.estimated_rate_hz - 250.0).abs() < 1.0,
            "expected ~250 Hz, got {}",
            stats.estimated_rate_hz
        );
        Ok(())
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Replay validation pipeline
// ═════════════════════════════════════════════════════════════════════════════

mod replay_validation {
    use super::*;

    #[test]
    fn validate_moza_stream() -> anyhow::Result<()> {
        let reports: Vec<CapturedReport> = (0..10).map(|i| moza_report(i * 1_000_000)).collect();
        let result = validate_replay_pipeline(&reports)?;
        assert_eq!(result.total_reports, 10);
        assert_eq!(result.decoded_count, 10);
        assert_eq!(result.failed_count, 0);
        assert_eq!(result.distinct_vids, vec![0x346E]);
        Ok(())
    }

    #[test]
    fn validate_logitech_stream() -> anyhow::Result<()> {
        let reports: Vec<CapturedReport> = (0..5).map(|i| logitech_report(i * 1_000_000)).collect();
        let result = validate_replay_pipeline(&reports)?;
        assert_eq!(result.total_reports, 5);
        assert_eq!(result.decoded_count, 5);
        assert_eq!(result.failed_count, 0);
        assert_eq!(result.distinct_vids, vec![0x046D]);
        Ok(())
    }

    #[test]
    fn validate_unknown_vendor_all_fail() -> anyhow::Result<()> {
        let reports: Vec<CapturedReport> = (0..5).map(|i| unknown_report(i * 1_000_000)).collect();
        let result = validate_replay_pipeline(&reports)?;
        assert_eq!(result.total_reports, 5);
        assert_eq!(result.decoded_count, 0);
        assert_eq!(result.failed_count, 5);
        Ok(())
    }

    #[test]
    fn validate_mixed_vendor_stream() -> anyhow::Result<()> {
        let reports = vec![
            moza_report(1_000_000),
            logitech_report(2_000_000),
            unknown_report(3_000_000),
            moza_report(4_000_000),
        ];
        let result = validate_replay_pipeline(&reports)?;
        assert_eq!(result.total_reports, 4);
        assert_eq!(result.decoded_count, 3); // 2 moza + 1 logitech
        assert_eq!(result.failed_count, 1); // 1 unknown
        assert!(result.distinct_vids.contains(&0x346E));
        assert!(result.distinct_vids.contains(&0x046D));
        assert!(result.distinct_vids.contains(&0x9999));
        Ok(())
    }

    #[test]
    fn validate_empty_stream() -> anyhow::Result<()> {
        let result = validate_replay_pipeline(&[])?;
        assert_eq!(result.total_reports, 0);
        assert_eq!(result.decoded_count, 0);
        assert_eq!(result.failed_count, 0);
        assert!(result.distinct_vids.is_empty());
        Ok(())
    }

    #[test]
    fn validate_bad_hex_report_counted_as_failure() -> anyhow::Result<()> {
        let bad = CapturedReport {
            ts_ns: 1_000_000,
            vid: "0x346E".to_string(),
            pid: "0x0002".to_string(),
            report: "ZZZZ".to_string(), // invalid hex
        };
        let result = validate_replay_pipeline(&[bad])?;
        assert_eq!(result.failed_count, 1);
        assert_eq!(result.decoded_count, 0);
        Ok(())
    }

    #[test]
    fn validate_short_report_for_known_vendor_fails() -> anyhow::Result<()> {
        let short = CapturedReport {
            ts_ns: 1_000_000,
            vid: "0x346E".to_string(),
            pid: "0x0002".to_string(),
            report: "01".to_string(), // too short for MOZA
        };
        let result = validate_replay_pipeline(&[short])?;
        assert_eq!(result.failed_count, 1);
        assert_eq!(result.decoded_count, 0);
        Ok(())
    }

    #[test]
    fn validate_distinct_vids_sorted() -> anyhow::Result<()> {
        let reports = vec![
            unknown_report(1_000_000),
            moza_report(2_000_000),
            logitech_report(3_000_000),
        ];
        let result = validate_replay_pipeline(&reports)?;
        // Should be sorted: 0x046D, 0x346E, 0x9999
        assert_eq!(result.distinct_vids, vec![0x046D, 0x346E, 0x9999]);
        Ok(())
    }

    #[test]
    fn validate_large_moza_stream() -> anyhow::Result<()> {
        let reports: Vec<CapturedReport> = (0..1000).map(|i| moza_report(i * 1_000_000)).collect();
        let result = validate_replay_pipeline(&reports)?;
        assert_eq!(result.total_reports, 1000);
        assert_eq!(result.decoded_count, 1000);
        assert_eq!(result.failed_count, 0);
        Ok(())
    }
}
