use racing_wheel_hid_capture::{CaptureFile, CaptureReport, parse_hex_u16};

// ── Capture Session Management ──────────────────────────────────────────────

#[test]
fn capture_file_empty_session() -> Result<(), Box<dyn std::error::Error>> {
    let file = CaptureFile {
        vendor_id: "0x046D".to_string(),
        product_id: "0xC266".to_string(),
        captures: vec![],
    };
    let json = serde_json::to_string(&file)?;
    let restored: CaptureFile = serde_json::from_str(&json)?;
    assert_eq!(restored.vendor_id, "0x046D");
    assert_eq!(restored.product_id, "0xC266");
    assert!(restored.captures.is_empty());
    Ok(())
}

#[test]
fn capture_file_preserves_insertion_order() -> Result<(), Box<dyn std::error::Error>> {
    let mut file = CaptureFile {
        vendor_id: "0x046D".to_string(),
        product_id: "0xC266".to_string(),
        captures: Vec::new(),
    };
    for i in 0..10u64 {
        file.captures.push(CaptureReport {
            timestamp_us: i * 1000,
            report_id: (i as u8) % 4,
            data: format!("0x{i:02X}"),
        });
    }
    let json = serde_json::to_string(&file)?;
    let restored: CaptureFile = serde_json::from_str(&json)?;
    assert_eq!(restored.captures.len(), 10);
    for (i, report) in restored.captures.iter().enumerate() {
        assert_eq!(report.timestamp_us, (i as u64) * 1000);
    }
    Ok(())
}

#[test]
fn capture_file_session_duration() {
    let file = CaptureFile {
        vendor_id: "0x046D".to_string(),
        product_id: "0xC266".to_string(),
        captures: vec![
            CaptureReport {
                timestamp_us: 1_000_000,
                report_id: 1,
                data: "0x01".into(),
            },
            CaptureReport {
                timestamp_us: 6_000_000,
                report_id: 1,
                data: "0x02".into(),
            },
        ],
    };
    let duration = file
        .captures
        .last()
        .map(|l| l.timestamp_us)
        .zip(file.captures.first().map(|f| f.timestamp_us))
        .map(|(last, first)| last - first);
    assert_eq!(duration, Some(5_000_000));
}

#[test]
fn capture_file_monotonic_timestamps() -> Result<(), Box<dyn std::error::Error>> {
    let file = CaptureFile {
        vendor_id: "0x0EB7".to_string(),
        product_id: "0x0001".to_string(),
        captures: vec![
            CaptureReport {
                timestamp_us: 100,
                report_id: 1,
                data: "0x01".into(),
            },
            CaptureReport {
                timestamp_us: 200,
                report_id: 1,
                data: "0x02".into(),
            },
            CaptureReport {
                timestamp_us: 300,
                report_id: 1,
                data: "0x03".into(),
            },
        ],
    };
    let json = serde_json::to_string(&file)?;
    let restored: CaptureFile = serde_json::from_str(&json)?;
    for window in restored.captures.windows(2) {
        assert!(
            window[0].timestamp_us < window[1].timestamp_us,
            "timestamps must be monotonically increasing"
        );
    }
    Ok(())
}

// ── Report Recording and Replay ─────────────────────────────────────────────

#[test]
fn capture_report_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let report = CaptureReport {
        timestamp_us: 1_000_000,
        report_id: 0x01,
        data: "0x01 0x02 0x03".to_string(),
    };
    let json = serde_json::to_string(&report)?;
    let restored: CaptureReport = serde_json::from_str(&json)?;
    assert_eq!(restored.timestamp_us, 1_000_000);
    assert_eq!(restored.report_id, 0x01);
    assert_eq!(restored.data, "0x01 0x02 0x03");
    Ok(())
}

#[test]
fn capture_report_zero_values() -> Result<(), Box<dyn std::error::Error>> {
    let report = CaptureReport {
        timestamp_us: 0,
        report_id: 0x00,
        data: String::new(),
    };
    let json = serde_json::to_string(&report)?;
    let restored: CaptureReport = serde_json::from_str(&json)?;
    assert_eq!(restored.timestamp_us, 0);
    assert_eq!(restored.report_id, 0x00);
    assert!(restored.data.is_empty());
    Ok(())
}

#[test]
fn capture_report_max_values() -> Result<(), Box<dyn std::error::Error>> {
    let report = CaptureReport {
        timestamp_us: u64::MAX,
        report_id: 0xFF,
        data: "0xFF".repeat(64),
    };
    let json = serde_json::to_string(&report)?;
    let restored: CaptureReport = serde_json::from_str(&json)?;
    assert_eq!(restored.timestamp_us, u64::MAX);
    assert_eq!(restored.report_id, 0xFF);
    assert_eq!(restored.data.len(), report.data.len());
    Ok(())
}

#[test]
fn capture_file_disk_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let file = CaptureFile {
        vendor_id: "0x046D".to_string(),
        product_id: "0xC266".to_string(),
        captures: vec![
            CaptureReport {
                timestamp_us: 1000,
                report_id: 0x01,
                data: "0x01 0x80 0x7F".to_string(),
            },
            CaptureReport {
                timestamp_us: 2000,
                report_id: 0x01,
                data: "0x01 0x81 0x80".to_string(),
            },
        ],
    };
    let dir = std::env::temp_dir().join("hid_capture_comprehensive_test");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("test_capture.json");
    let json = serde_json::to_string_pretty(&file)?;
    std::fs::write(&path, &json)?;
    let read_back = std::fs::read_to_string(&path)?;
    let restored: CaptureFile = serde_json::from_str(&read_back)?;
    assert_eq!(restored.vendor_id, "0x046D");
    assert_eq!(restored.captures.len(), 2);
    assert_eq!(restored.captures[0].data, "0x01 0x80 0x7F");
    assert_eq!(restored.captures[1].data, "0x01 0x81 0x80");
    // cleanup
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&dir);
    Ok(())
}

#[test]
fn capture_file_large_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let captures: Vec<CaptureReport> = (0..1000)
        .map(|i| CaptureReport {
            timestamp_us: i * 1000,
            report_id: (i % 256) as u8,
            data: format!("0x{:02X} 0x{:02X}", i % 256, (i / 256) % 256),
        })
        .collect();
    let file = CaptureFile {
        vendor_id: "0x046D".to_string(),
        product_id: "0xC24F".to_string(),
        captures,
    };
    let json = serde_json::to_string(&file)?;
    let restored: CaptureFile = serde_json::from_str(&json)?;
    assert_eq!(restored.captures.len(), 1000);
    assert_eq!(restored.captures[0].timestamp_us, 0);
    assert_eq!(restored.captures[999].timestamp_us, 999_000);
    Ok(())
}

// ── Packet Filtering ────────────────────────────────────────────────────────

#[test]
fn filter_captures_by_report_id() {
    let captures = [
        CaptureReport {
            timestamp_us: 100,
            report_id: 0x01,
            data: "a".into(),
        },
        CaptureReport {
            timestamp_us: 200,
            report_id: 0x02,
            data: "b".into(),
        },
        CaptureReport {
            timestamp_us: 300,
            report_id: 0x01,
            data: "c".into(),
        },
        CaptureReport {
            timestamp_us: 400,
            report_id: 0x03,
            data: "d".into(),
        },
        CaptureReport {
            timestamp_us: 500,
            report_id: 0x01,
            data: "e".into(),
        },
    ];
    let filtered: Vec<&CaptureReport> =
        captures.iter().filter(|r| r.report_id == 0x01).collect();
    assert_eq!(filtered.len(), 3);
    assert_eq!(filtered[0].data, "a");
    assert_eq!(filtered[1].data, "c");
    assert_eq!(filtered[2].data, "e");
}

#[test]
fn compute_inter_report_intervals() {
    let captures = [
        CaptureReport {
            timestamp_us: 1000,
            report_id: 1,
            data: "0x01".into(),
        },
        CaptureReport {
            timestamp_us: 2000,
            report_id: 1,
            data: "0x02".into(),
        },
        CaptureReport {
            timestamp_us: 3500,
            report_id: 1,
            data: "0x03".into(),
        },
        CaptureReport {
            timestamp_us: 4000,
            report_id: 1,
            data: "0x04".into(),
        },
    ];
    let intervals: Vec<u64> = captures
        .windows(2)
        .map(|w| w[1].timestamp_us - w[0].timestamp_us)
        .collect();
    assert_eq!(intervals, vec![1000, 1500, 500]);
}

// ── Hex Parsing ─────────────────────────────────────────────────────────────

#[test]
fn parse_hex_u16_with_prefix() {
    assert_eq!(parse_hex_u16("0x0EB7"), Ok(0x0EB7));
    assert_eq!(parse_hex_u16("0x0001"), Ok(0x0001));
    assert_eq!(parse_hex_u16("0X046D"), Ok(0x046D));
}

#[test]
fn parse_hex_u16_without_prefix() {
    assert_eq!(parse_hex_u16("346E"), Ok(0x346E));
    assert_eq!(parse_hex_u16("FFFF"), Ok(0xFFFF));
    assert_eq!(parse_hex_u16("0000"), Ok(0x0000));
}

#[test]
fn parse_hex_u16_invalid_input() {
    assert!(parse_hex_u16("ZZZZ").is_err());
    assert!(parse_hex_u16("xyz").is_err());
}

#[test]
fn parse_hex_u16_overflow() {
    assert!(parse_hex_u16("0x10000").is_err());
}

#[test]
fn parse_hex_u16_empty() {
    assert!(parse_hex_u16("").is_err());
}

#[test]
fn parse_hex_u16_bare_prefix() {
    assert!(parse_hex_u16("0x").is_err());
    assert!(parse_hex_u16("0X").is_err());
}

#[test]
fn parse_hex_u16_mixed_case() {
    assert_eq!(parse_hex_u16("0xAbCd"), Ok(0xABCD));
    assert_eq!(parse_hex_u16("abcd"), Ok(0xABCD));
}

#[test]
fn vid_pid_format_roundtrip() {
    let vid: u16 = 0x046D;
    let pid: u16 = 0xC266;
    let vid_str = format!("0x{vid:04X}");
    let pid_str = format!("0x{pid:04X}");
    assert_eq!(parse_hex_u16(&vid_str), Ok(vid));
    assert_eq!(parse_hex_u16(&pid_str), Ok(pid));
}

// ── JSON Structure Validation ───────────────────────────────────────────────

#[test]
fn capture_file_json_has_expected_keys() -> Result<(), Box<dyn std::error::Error>> {
    let file = CaptureFile {
        vendor_id: "0x046D".to_string(),
        product_id: "0x0002".to_string(),
        captures: vec![],
    };
    let json = serde_json::to_string(&file)?;
    let value: serde_json::Value = serde_json::from_str(&json)?;
    assert!(value.get("vendor_id").is_some());
    assert!(value.get("product_id").is_some());
    assert!(value.get("captures").is_some());
    Ok(())
}

#[test]
fn malformed_json_fails_deserialization() {
    assert!(serde_json::from_str::<CaptureFile>(r#"{"vendor_id": "0x046D"}"#).is_err());
    assert!(serde_json::from_str::<CaptureFile>("not json").is_err());
    assert!(serde_json::from_str::<CaptureFile>("{}").is_err());
}

#[test]
fn wrong_field_types_fail_deserialization() {
    let bad = r#"{"timestamp_us": "not_a_number", "report_id": 1, "data": "0x01"}"#;
    assert!(serde_json::from_str::<CaptureReport>(bad).is_err());
}

#[test]
fn report_id_overflow_fails_deserialization() {
    let bad = r#"{"timestamp_us": 100, "report_id": 256, "data": "0x01"}"#;
    assert!(serde_json::from_str::<CaptureReport>(bad).is_err());
}

#[test]
fn extra_fields_ignored_on_deserialization() -> Result<(), Box<dyn std::error::Error>> {
    let json = r#"{
        "vendor_id": "0x046D",
        "product_id": "0x0002",
        "captures": [],
        "extra_field": "should be ignored"
    }"#;
    let file: CaptureFile = serde_json::from_str(json)?;
    assert_eq!(file.vendor_id, "0x046D");
    assert!(file.captures.is_empty());
    Ok(())
}

#[test]
fn pretty_and_compact_json_equivalent() -> Result<(), Box<dyn std::error::Error>> {
    let file = CaptureFile {
        vendor_id: "0x0EB7".to_string(),
        product_id: "0x0001".to_string(),
        captures: vec![CaptureReport {
            timestamp_us: 500,
            report_id: 0x03,
            data: "0x03 0x10".to_string(),
        }],
    };
    let compact = serde_json::to_string(&file)?;
    let pretty = serde_json::to_string_pretty(&file)?;
    assert_ne!(compact, pretty);
    let from_compact: CaptureFile = serde_json::from_str(&compact)?;
    let from_pretty: CaptureFile = serde_json::from_str(&pretty)?;
    assert_eq!(from_compact.vendor_id, from_pretty.vendor_id);
    assert_eq!(from_compact.captures.len(), from_pretty.captures.len());
    Ok(())
}
