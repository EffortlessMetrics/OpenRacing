//! Deep tests for HID capture session lifecycle, file format, filtering,
//! replay, statistics, and multi-device capture scenarios.
//!
//! Covers: start→record→stop→save lifecycle, capture file format validation,
//! VID/PID filtering, replay from saved files, capture statistics computation,
//! and simultaneous multi-device capture sessions.

use racing_wheel_hid_capture::{CaptureFile, CaptureReport, parse_hex_u16};

type R = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

fn make_report(timestamp_us: u64, report_id: u8, bytes: &[u8]) -> CaptureReport {
    let data = bytes
        .iter()
        .map(|b| format!("0x{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ");
    CaptureReport {
        timestamp_us,
        report_id,
        data,
    }
}

fn make_capture_file(vid: &str, pid: &str, count: u64) -> CaptureFile {
    let captures = (0..count)
        .map(|i| make_report(i * 1000, (i % 4) as u8, &[i as u8, (i >> 8) as u8]))
        .collect();
    CaptureFile {
        vendor_id: vid.to_string(),
        product_id: pid.to_string(),
        captures,
    }
}

fn session_duration_us(file: &CaptureFile) -> Option<u64> {
    let first = file.captures.first().map(|r| r.timestamp_us);
    let last = file.captures.last().map(|r| r.timestamp_us);
    first.zip(last).map(|(f, l)| l.saturating_sub(f))
}

fn total_data_bytes(file: &CaptureFile) -> usize {
    file.captures.iter().map(|r| r.data.len()).sum()
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. Capture session lifecycle (start→record→stop→save)
// ═══════════════════════════════════════════════════════════════════════════

mod session_lifecycle {
    use super::*;

    #[test]
    fn lifecycle_01_empty_session_roundtrips() -> R {
        let file = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0xC266".to_string(),
            captures: vec![],
        };
        let json = serde_json::to_string(&file)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        assert!(restored.captures.is_empty());
        assert_eq!(restored.vendor_id, "0x046D");
        Ok(())
    }

    #[test]
    fn lifecycle_02_start_record_stop_produces_ordered_captures() -> R {
        let mut file = CaptureFile {
            vendor_id: "0x0EB7".to_string(),
            product_id: "0x0006".to_string(),
            captures: Vec::new(),
        };
        // Simulate start→record phase
        for i in 0..50u64 {
            file.captures.push(make_report(i * 1000, 0x01, &[i as u8]));
        }
        // Stop: verify captures are complete and ordered
        assert_eq!(file.captures.len(), 50);
        for window in file.captures.windows(2) {
            assert!(
                window[0].timestamp_us < window[1].timestamp_us,
                "timestamps must be strictly increasing"
            );
        }
        Ok(())
    }

    #[test]
    fn lifecycle_03_save_and_reload_preserves_all_data() -> R {
        let file = make_capture_file("0x346E", "0x0010", 100);
        let json = serde_json::to_string_pretty(&file)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        assert_eq!(restored.vendor_id, "0x346E");
        assert_eq!(restored.product_id, "0x0010");
        assert_eq!(restored.captures.len(), 100);
        for (orig, rest) in file.captures.iter().zip(restored.captures.iter()) {
            assert_eq!(orig.timestamp_us, rest.timestamp_us);
            assert_eq!(orig.report_id, rest.report_id);
            assert_eq!(orig.data, rest.data);
        }
        Ok(())
    }

    #[test]
    fn lifecycle_04_session_with_single_report() -> R {
        let file = CaptureFile {
            vendor_id: "0x044F".to_string(),
            product_id: "0xB677".to_string(),
            captures: vec![make_report(5000, 0x02, &[0xAA, 0xBB])],
        };
        let json = serde_json::to_string(&file)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        assert_eq!(restored.captures.len(), 1);
        assert_eq!(restored.captures[0].timestamp_us, 5000);
        assert_eq!(restored.captures[0].report_id, 0x02);
        Ok(())
    }

    #[test]
    fn lifecycle_05_session_duration_computation() -> R {
        let file = make_capture_file("0x0EB7", "0x0007", 200);
        let dur = session_duration_us(&file);
        assert_eq!(dur, Some(199_000));
        Ok(())
    }

    #[test]
    fn lifecycle_06_empty_session_has_zero_duration() {
        let file = make_capture_file("0x046D", "0xC24F", 0);
        assert_eq!(session_duration_us(&file), None);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. Capture file format validation
// ═══════════════════════════════════════════════════════════════════════════

mod file_format {
    use super::*;

    #[test]
    fn format_01_json_keys_present() -> R {
        let file = make_capture_file("0x046D", "0xC266", 1);
        let json = serde_json::to_string(&file)?;
        let value: serde_json::Value = serde_json::from_str(&json)?;
        assert!(value.get("vendor_id").is_some());
        assert!(value.get("product_id").is_some());
        assert!(value.get("captures").is_some());
        Ok(())
    }

    #[test]
    fn format_02_report_json_keys_present() -> R {
        let file = make_capture_file("0x0EB7", "0x0001", 1);
        let json = serde_json::to_string(&file)?;
        let value: serde_json::Value = serde_json::from_str(&json)?;
        let captures = value
            .get("captures")
            .and_then(|v| v.as_array())
            .ok_or("missing captures array")?;
        let first = captures.first().ok_or("no captures")?;
        assert!(first.get("timestamp_us").is_some());
        assert!(first.get("report_id").is_some());
        assert!(first.get("data").is_some());
        Ok(())
    }

    #[test]
    fn format_03_compact_and_pretty_are_equivalent() -> R {
        let file = make_capture_file("0x346E", "0x0000", 5);
        let compact = serde_json::to_string(&file)?;
        let pretty = serde_json::to_string_pretty(&file)?;
        assert_ne!(compact, pretty);
        let from_compact: CaptureFile = serde_json::from_str(&compact)?;
        let from_pretty: CaptureFile = serde_json::from_str(&pretty)?;
        assert_eq!(from_compact.captures.len(), from_pretty.captures.len());
        for (a, b) in from_compact
            .captures
            .iter()
            .zip(from_pretty.captures.iter())
        {
            assert_eq!(a.timestamp_us, b.timestamp_us);
            assert_eq!(a.report_id, b.report_id);
            assert_eq!(a.data, b.data);
        }
        Ok(())
    }

    #[test]
    fn format_04_missing_required_field_fails() {
        let bad_json = r#"{"vendor_id": "0x046D", "product_id": "0xC266"}"#;
        assert!(serde_json::from_str::<CaptureFile>(bad_json).is_err());
    }

    #[test]
    fn format_05_wrong_type_for_timestamp_fails() {
        let bad = r#"{"timestamp_us": "not_a_number", "report_id": 1, "data": "0x01"}"#;
        assert!(serde_json::from_str::<CaptureReport>(bad).is_err());
    }

    #[test]
    fn format_06_report_id_overflow_fails() {
        let bad = r#"{"timestamp_us": 100, "report_id": 256, "data": "0x01"}"#;
        assert!(serde_json::from_str::<CaptureReport>(bad).is_err());
    }

    #[test]
    fn format_07_extra_fields_are_ignored() -> R {
        let json = r#"{
            "vendor_id": "0x046D",
            "product_id": "0xC266",
            "captures": [],
            "firmware_version": "1.2.3"
        }"#;
        let file: CaptureFile = serde_json::from_str(json)?;
        assert!(file.captures.is_empty());
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. Capture filtering by VID/PID
// ═══════════════════════════════════════════════════════════════════════════

mod vid_pid_filtering {
    use super::*;

    #[test]
    fn filter_01_select_files_by_vid() -> R {
        let files = [
            make_capture_file("0x046D", "0xC266", 5),
            make_capture_file("0x0EB7", "0x0006", 3),
            make_capture_file("0x046D", "0xC24F", 7),
        ];
        let logitech_files: Vec<_> = files.iter().filter(|f| f.vendor_id == "0x046D").collect();
        assert_eq!(logitech_files.len(), 2);
        Ok(())
    }

    #[test]
    fn filter_02_select_files_by_vid_and_pid() -> R {
        let files = [
            make_capture_file("0x046D", "0xC266", 5),
            make_capture_file("0x046D", "0xC24F", 3),
            make_capture_file("0x046D", "0xC266", 10),
        ];
        let g923_files: Vec<_> = files
            .iter()
            .filter(|f| f.vendor_id == "0x046D" && f.product_id == "0xC266")
            .collect();
        assert_eq!(g923_files.len(), 2);
        assert_eq!(g923_files[0].captures.len(), 5);
        assert_eq!(g923_files[1].captures.len(), 10);
        Ok(())
    }

    #[test]
    fn filter_03_no_match_returns_empty() {
        let files = [
            make_capture_file("0x046D", "0xC266", 5),
            make_capture_file("0x0EB7", "0x0006", 3),
        ];
        let no_match: Vec<_> = files.iter().filter(|f| f.vendor_id == "0x9999").collect();
        assert!(no_match.is_empty());
    }

    #[test]
    fn filter_04_parse_hex_vid_pid_for_matching() -> R {
        let target_vid = parse_hex_u16("0x046D")?;
        let target_pid = parse_hex_u16("0xC266")?;
        assert_eq!(target_vid, 0x046D);
        assert_eq!(target_pid, 0xC266);
        Ok(())
    }

    #[test]
    fn filter_05_case_insensitive_hex_matching() -> R {
        let lower = parse_hex_u16("0x046d")?;
        let upper = parse_hex_u16("0x046D")?;
        let mixed = parse_hex_u16("0x046D")?;
        assert_eq!(lower, upper);
        assert_eq!(upper, mixed);
        Ok(())
    }

    #[test]
    fn filter_06_all_known_vendors_parseable() -> R {
        let vids = [
            "0x046D", "0x0EB7", "0x346E", "0x044F", "0x0483", "0x16D0", "0x3670", "0x2433",
            "0x1D50", "0x1209", "0x045B", "0x3416", "0x1FC9", "0x1DD2", "0x11FF", "0x04D8",
            "0x30B7", "0x10C4", "0xA020",
        ];
        for vid_str in &vids {
            let val = parse_hex_u16(vid_str)?;
            assert!(val > 0, "VID {vid_str} parsed to zero");
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. Capture replay from saved files
// ═══════════════════════════════════════════════════════════════════════════

mod capture_replay {
    use super::*;

    #[test]
    fn replay_01_disk_roundtrip() -> R {
        let file = make_capture_file("0x046D", "0xC266", 20);
        let dir = std::env::temp_dir().join("capture_deep_replay_01");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("replay_test.json");
        let json = serde_json::to_string_pretty(&file)?;
        std::fs::write(&path, &json)?;
        let read_back = std::fs::read_to_string(&path)?;
        let restored: CaptureFile = serde_json::from_str(&read_back)?;
        assert_eq!(restored.captures.len(), 20);
        for (orig, rest) in file.captures.iter().zip(restored.captures.iter()) {
            assert_eq!(orig.timestamp_us, rest.timestamp_us);
            assert_eq!(orig.data, rest.data);
        }
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
        Ok(())
    }

    #[test]
    fn replay_02_preserves_timing_deltas() -> R {
        let file = CaptureFile {
            vendor_id: "0x0EB7".to_string(),
            product_id: "0x0006".to_string(),
            captures: vec![
                make_report(1000, 0x01, &[0x10]),
                make_report(2500, 0x01, &[0x20]),
                make_report(5000, 0x01, &[0x30]),
            ],
        };
        let json = serde_json::to_string(&file)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        let deltas: Vec<u64> = restored
            .captures
            .windows(2)
            .map(|w| w[1].timestamp_us - w[0].timestamp_us)
            .collect();
        assert_eq!(deltas, vec![1500, 2500]);
        Ok(())
    }

    #[test]
    fn replay_03_filter_by_report_id_on_loaded_data() -> R {
        let file = CaptureFile {
            vendor_id: "0x346E".to_string(),
            product_id: "0x0010".to_string(),
            captures: (0..30)
                .map(|i| make_report(i * 1000, (i % 3) as u8, &[i as u8]))
                .collect(),
        };
        let json = serde_json::to_string(&file)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        let report_0: Vec<_> = restored
            .captures
            .iter()
            .filter(|r| r.report_id == 0)
            .collect();
        assert_eq!(report_0.len(), 10);
        Ok(())
    }

    #[test]
    fn replay_04_filter_by_time_window() -> R {
        let file = make_capture_file("0x044F", "0xB677", 100);
        let json = serde_json::to_string(&file)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        let windowed: Vec<_> = restored
            .captures
            .iter()
            .filter(|r| r.timestamp_us >= 10_000 && r.timestamp_us < 20_000)
            .collect();
        assert_eq!(windowed.len(), 10);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. Capture statistics (packet count, bytes, timing)
// ═══════════════════════════════════════════════════════════════════════════

mod capture_statistics {
    use super::*;

    #[test]
    fn stats_01_packet_count() {
        let file = make_capture_file("0x046D", "0xC266", 250);
        assert_eq!(file.captures.len(), 250);
    }

    #[test]
    fn stats_02_total_data_bytes() {
        let file = CaptureFile {
            vendor_id: "0x0EB7".to_string(),
            product_id: "0x0006".to_string(),
            captures: vec![
                make_report(0, 0x01, &[0x01, 0x02, 0x03]),
                make_report(1000, 0x01, &[0x04, 0x05]),
            ],
        };
        let total = total_data_bytes(&file);
        // "0x01 0x02 0x03" = 14 chars, "0x04 0x05" = 9 chars
        assert_eq!(total, 14 + 9);
    }

    #[test]
    fn stats_03_report_id_histogram() {
        let file = CaptureFile {
            vendor_id: "0x346E".to_string(),
            product_id: "0x0000".to_string(),
            captures: (0..60)
                .map(|i| make_report(i * 1000, (i % 3) as u8, &[i as u8]))
                .collect(),
        };
        let mut counts = [0usize; 256];
        for r in &file.captures {
            counts[r.report_id as usize] += 1;
        }
        assert_eq!(counts[0], 20);
        assert_eq!(counts[1], 20);
        assert_eq!(counts[2], 20);
    }

    #[test]
    fn stats_04_inter_packet_intervals() {
        let file = make_capture_file("0x044F", "0xB66E", 10);
        let intervals: Vec<u64> = file
            .captures
            .windows(2)
            .map(|w| w[1].timestamp_us - w[0].timestamp_us)
            .collect();
        assert_eq!(intervals.len(), 9);
        assert!(intervals.iter().all(|&d| d == 1000));
    }

    #[test]
    fn stats_05_min_max_timestamps() {
        let file = make_capture_file("0x2433", "0xF300", 500);
        let min_ts = file.captures.iter().map(|r| r.timestamp_us).min();
        let max_ts = file.captures.iter().map(|r| r.timestamp_us).max();
        assert_eq!(min_ts, Some(0));
        assert_eq!(max_ts, Some(499_000));
    }

    #[test]
    fn stats_06_average_interval() {
        let file = make_capture_file("0x1DD2", "0x000E", 101);
        let dur = session_duration_us(&file);
        let avg = dur.map(|d| d / (file.captures.len() as u64 - 1));
        assert_eq!(avg, Some(1000));
    }

    #[test]
    fn stats_07_empty_session_statistics() {
        let file = make_capture_file("0x046D", "0xC24F", 0);
        assert_eq!(file.captures.len(), 0);
        assert_eq!(total_data_bytes(&file), 0);
        assert_eq!(session_duration_us(&file), None);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. Multi-device simultaneous capture
// ═══════════════════════════════════════════════════════════════════════════

mod multi_device_capture {
    use super::*;

    #[test]
    fn multi_01_two_devices_independent_captures() -> R {
        let wheel = make_capture_file("0x046D", "0xC266", 50);
        let pedals = make_capture_file("0x30B7", "0x0001", 30);
        let wheel_json = serde_json::to_string(&wheel)?;
        let pedals_json = serde_json::to_string(&pedals)?;
        let w: CaptureFile = serde_json::from_str(&wheel_json)?;
        let p: CaptureFile = serde_json::from_str(&pedals_json)?;
        assert_eq!(w.captures.len(), 50);
        assert_eq!(p.captures.len(), 30);
        assert_ne!(w.vendor_id, p.vendor_id);
        Ok(())
    }

    #[test]
    fn multi_02_three_devices_different_vendors() -> R {
        let devices = [
            make_capture_file("0x046D", "0xC266", 20),
            make_capture_file("0x0EB7", "0x0006", 15),
            make_capture_file("0x346E", "0x0010", 25),
        ];
        let jsons: Vec<String> = devices
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<_, _>>()?;
        let restored: Vec<CaptureFile> = jsons
            .iter()
            .map(|j| serde_json::from_str(j))
            .collect::<Result<_, _>>()?;
        assert_eq!(restored.len(), 3);
        assert_eq!(restored[0].captures.len(), 20);
        assert_eq!(restored[1].captures.len(), 15);
        assert_eq!(restored[2].captures.len(), 25);
        Ok(())
    }

    #[test]
    fn multi_03_same_vid_different_pids() -> R {
        let g29 = make_capture_file("0x046D", "0xC24F", 10);
        let g923 = make_capture_file("0x046D", "0xC266", 10);
        assert_eq!(g29.vendor_id, g923.vendor_id);
        assert_ne!(g29.product_id, g923.product_id);
        let all = [g29, g923];
        let g923_only: Vec<_> = all.iter().filter(|f| f.product_id == "0xC266").collect();
        assert_eq!(g923_only.len(), 1);
        Ok(())
    }

    #[test]
    fn multi_04_merge_captures_by_timestamp() {
        let mut a = make_capture_file("0x046D", "0xC266", 5);
        let b = make_capture_file("0x0EB7", "0x0006", 5);
        // Offset b's timestamps to interleave
        let mut b_adjusted: Vec<CaptureReport> = b
            .captures
            .into_iter()
            .enumerate()
            .map(|(i, mut r)| {
                r.timestamp_us = (i as u64) * 1000 + 500;
                r
            })
            .collect();
        a.captures.append(&mut b_adjusted);
        a.captures.sort_by_key(|r| r.timestamp_us);
        assert_eq!(a.captures.len(), 10);
        for window in a.captures.windows(2) {
            assert!(window[0].timestamp_us <= window[1].timestamp_us);
        }
    }

    #[test]
    fn multi_05_large_multi_device_set() -> R {
        let device_configs = [
            ("0x046D", "0xC266"),
            ("0x0EB7", "0x0006"),
            ("0x346E", "0x0010"),
            ("0x044F", "0xB66E"),
            ("0x2433", "0xF300"),
        ];
        let files: Vec<CaptureFile> = device_configs
            .iter()
            .map(|(vid, pid)| make_capture_file(vid, pid, 100))
            .collect();
        let total_captures: usize = files.iter().map(|f| f.captures.len()).sum();
        assert_eq!(total_captures, 500);
        // Each can be independently serialized
        for f in &files {
            let json = serde_json::to_string(f)?;
            let restored: CaptureFile = serde_json::from_str(&json)?;
            assert_eq!(restored.captures.len(), 100);
        }
        Ok(())
    }
}
