//! Deep tests for racing-wheel-hid-capture.
//!
//! Covers: HID device capture data structures, report recording and playback,
//! capture session lifecycle, file format serialization, and error conditions.

use racing_wheel_hid_capture::{CaptureFile, CaptureReport, parse_hex_u16};

type R = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════
// CaptureReport construction and field access
// ═══════════════════════════════════════════════════════════════════════════

mod capture_report_construction {
    use super::*;

    #[test]
    fn all_fields_accessible() -> R {
        let r = CaptureReport {
            timestamp_us: 42,
            report_id: 0x07,
            data: "0x07 0xFF".to_string(),
        };
        assert_eq!(r.timestamp_us, 42);
        assert_eq!(r.report_id, 0x07);
        assert_eq!(r.data, "0x07 0xFF");
        Ok(())
    }

    #[test]
    fn zero_valued_fields() -> R {
        let r = CaptureReport {
            timestamp_us: 0,
            report_id: 0x00,
            data: String::new(),
        };
        assert_eq!(r.timestamp_us, 0);
        assert_eq!(r.report_id, 0);
        assert!(r.data.is_empty());
        Ok(())
    }

    #[test]
    fn max_valued_fields() -> R {
        let r = CaptureReport {
            timestamp_us: u64::MAX,
            report_id: 0xFF,
            data: "x".repeat(10000),
        };
        assert_eq!(r.timestamp_us, u64::MAX);
        assert_eq!(r.report_id, 0xFF);
        assert_eq!(r.data.len(), 10000);
        Ok(())
    }

    #[test]
    fn debug_trait() {
        let r = CaptureReport {
            timestamp_us: 100,
            report_id: 0x01,
            data: "0x01".to_string(),
        };
        let dbg = format!("{r:?}");
        assert!(dbg.contains("CaptureReport"));
        assert!(dbg.contains("100"));
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CaptureFile construction and field access
// ═══════════════════════════════════════════════════════════════════════════

mod capture_file_construction {
    use super::*;

    #[test]
    fn empty_captures() -> R {
        let f = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0xC266".to_string(),
            captures: vec![],
        };
        assert_eq!(f.vendor_id, "0x046D");
        assert_eq!(f.product_id, "0xC266");
        assert!(f.captures.is_empty());
        Ok(())
    }

    #[test]
    fn with_captures() -> R {
        let f = CaptureFile {
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
                    report_id: 2,
                    data: "0x02".into(),
                },
            ],
        };
        assert_eq!(f.captures.len(), 2);
        assert_eq!(f.captures[0].report_id, 1);
        assert_eq!(f.captures[1].report_id, 2);
        Ok(())
    }

    #[test]
    fn debug_trait() {
        let f = CaptureFile {
            vendor_id: "0x0000".to_string(),
            product_id: "0x0000".to_string(),
            captures: vec![],
        };
        let dbg = format!("{f:?}");
        assert!(dbg.contains("CaptureFile"));
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Report recording – serde roundtrip
// ═══════════════════════════════════════════════════════════════════════════

mod report_serde {
    use super::*;

    #[test]
    fn single_report_roundtrip() -> R {
        let r = CaptureReport {
            timestamp_us: 1_500_000,
            report_id: 0x03,
            data: "0x03 0x10 0x20".to_string(),
        };
        let json = serde_json::to_string(&r)?;
        let restored: CaptureReport = serde_json::from_str(&json)?;
        assert_eq!(restored.timestamp_us, r.timestamp_us);
        assert_eq!(restored.report_id, r.report_id);
        assert_eq!(restored.data, r.data);
        Ok(())
    }

    #[test]
    fn report_compact_and_pretty_json_equivalent() -> R {
        let r = CaptureReport {
            timestamp_us: 42,
            report_id: 0x01,
            data: "0x01 0xFF".to_string(),
        };
        let compact = serde_json::to_string(&r)?;
        let pretty = serde_json::to_string_pretty(&r)?;
        assert_ne!(compact, pretty, "formats should differ");
        let from_compact: CaptureReport = serde_json::from_str(&compact)?;
        let from_pretty: CaptureReport = serde_json::from_str(&pretty)?;
        assert_eq!(from_compact.timestamp_us, from_pretty.timestamp_us);
        assert_eq!(from_compact.report_id, from_pretty.report_id);
        assert_eq!(from_compact.data, from_pretty.data);
        Ok(())
    }

    #[test]
    fn report_with_empty_data() -> R {
        let r = CaptureReport {
            timestamp_us: 0,
            report_id: 0,
            data: String::new(),
        };
        let json = serde_json::to_string(&r)?;
        let restored: CaptureReport = serde_json::from_str(&json)?;
        assert!(restored.data.is_empty());
        Ok(())
    }

    #[test]
    fn report_with_long_data_string() -> R {
        let data = (0..256)
            .map(|b| format!("0x{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ");
        let r = CaptureReport {
            timestamp_us: 999,
            report_id: 0xFF,
            data,
        };
        let json = serde_json::to_string(&r)?;
        let restored: CaptureReport = serde_json::from_str(&json)?;
        assert_eq!(restored.data, r.data);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CaptureFile serde roundtrip
// ═══════════════════════════════════════════════════════════════════════════

mod file_serde {
    use super::*;

    #[test]
    fn empty_file_roundtrip() -> R {
        let f = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0xC266".to_string(),
            captures: vec![],
        };
        let json = serde_json::to_string(&f)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        assert_eq!(restored.vendor_id, "0x046D");
        assert!(restored.captures.is_empty());
        Ok(())
    }

    #[test]
    fn file_with_many_reports() -> R {
        let captures: Vec<CaptureReport> = (0..500)
            .map(|i| CaptureReport {
                timestamp_us: i * 2000,
                report_id: (i % 256) as u8,
                data: format!("0x{:02X}", i % 256),
            })
            .collect();
        let f = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0xC24F".to_string(),
            captures,
        };
        let json = serde_json::to_string(&f)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        assert_eq!(restored.captures.len(), 500);
        assert_eq!(restored.captures[0].timestamp_us, 0);
        assert_eq!(restored.captures[499].timestamp_us, 499 * 2000);
        Ok(())
    }

    #[test]
    fn json_has_expected_top_level_keys() -> R {
        let f = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0x0002".to_string(),
            captures: vec![],
        };
        let json = serde_json::to_string(&f)?;
        let value: serde_json::Value = serde_json::from_str(&json)?;
        assert!(value.get("vendor_id").is_some());
        assert!(value.get("product_id").is_some());
        assert!(value.get("captures").is_some());
        let arr = value
            .get("captures")
            .and_then(|v| v.as_array())
            .ok_or("captures should be array")?;
        assert!(arr.is_empty());
        Ok(())
    }

    #[test]
    fn json_report_has_expected_fields() -> R {
        let f = CaptureFile {
            vendor_id: "0x0EB7".to_string(),
            product_id: "0x0001".to_string(),
            captures: vec![CaptureReport {
                timestamp_us: 42,
                report_id: 0x07,
                data: "0x07 0xFF".to_string(),
            }],
        };
        let json = serde_json::to_string(&f)?;
        let value: serde_json::Value = serde_json::from_str(&json)?;
        let report = &value["captures"][0];
        assert_eq!(report["timestamp_us"], 42);
        assert_eq!(report["report_id"], 7);
        assert_eq!(report["data"], "0x07 0xFF");
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Capture session lifecycle
// ═══════════════════════════════════════════════════════════════════════════

mod session_lifecycle {
    use super::*;

    #[test]
    fn build_session_incrementally() -> R {
        let mut file = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0xC266".to_string(),
            captures: Vec::new(),
        };
        assert!(file.captures.is_empty());

        // Simulate adding reports over time
        for i in 0..20u64 {
            file.captures.push(CaptureReport {
                timestamp_us: i * 1000,
                report_id: 0x01,
                data: format!("frame-{i}"),
            });
        }
        assert_eq!(file.captures.len(), 20);

        // Verify order preserved through serialization
        let json = serde_json::to_string(&file)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        for (i, report) in restored.captures.iter().enumerate() {
            assert_eq!(report.timestamp_us, i as u64 * 1000);
            assert_eq!(report.data, format!("frame-{i}"));
        }
        Ok(())
    }

    #[test]
    fn session_duration_computed_from_timestamps() {
        let file = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0xC266".to_string(),
            captures: vec![
                CaptureReport {
                    timestamp_us: 1_000_000,
                    report_id: 1,
                    data: "start".into(),
                },
                CaptureReport {
                    timestamp_us: 3_500_000,
                    report_id: 1,
                    data: "mid".into(),
                },
                CaptureReport {
                    timestamp_us: 6_000_000,
                    report_id: 1,
                    data: "end".into(),
                },
            ],
        };
        let first_ts = file.captures.first().map(|r| r.timestamp_us);
        let last_ts = file.captures.last().map(|r| r.timestamp_us);
        let duration = first_ts.zip(last_ts).map(|(first, last)| last - first);
        assert_eq!(duration, Some(5_000_000));
    }

    #[test]
    fn empty_session_duration_is_none() {
        let file = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0xC266".to_string(),
            captures: vec![],
        };
        let first_ts = file.captures.first().map(|r| r.timestamp_us);
        let last_ts = file.captures.last().map(|r| r.timestamp_us);
        let duration = first_ts.zip(last_ts);
        assert!(duration.is_none());
    }

    #[test]
    fn single_report_session_duration_is_zero() {
        let file = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0xC266".to_string(),
            captures: vec![CaptureReport {
                timestamp_us: 1_000_000,
                report_id: 1,
                data: "only".into(),
            }],
        };
        let first_ts = file.captures.first().map(|r| r.timestamp_us);
        let last_ts = file.captures.last().map(|r| r.timestamp_us);
        let duration = first_ts.zip(last_ts).map(|(first, last)| last - first);
        assert_eq!(duration, Some(0));
    }

    #[test]
    fn monotonic_timestamps_verified() -> R {
        let file = CaptureFile {
            vendor_id: "0x0EB7".to_string(),
            product_id: "0x0001".to_string(),
            captures: vec![
                CaptureReport {
                    timestamp_us: 100,
                    report_id: 1,
                    data: "a".into(),
                },
                CaptureReport {
                    timestamp_us: 200,
                    report_id: 1,
                    data: "b".into(),
                },
                CaptureReport {
                    timestamp_us: 300,
                    report_id: 1,
                    data: "c".into(),
                },
                CaptureReport {
                    timestamp_us: 400,
                    report_id: 1,
                    data: "d".into(),
                },
            ],
        };
        for window in file.captures.windows(2) {
            assert!(
                window[0].timestamp_us < window[1].timestamp_us,
                "timestamps should be strictly increasing"
            );
        }
        Ok(())
    }

    #[test]
    fn inter_report_intervals() {
        let captures = [
            CaptureReport {
                timestamp_us: 1000,
                report_id: 1,
                data: "a".into(),
            },
            CaptureReport {
                timestamp_us: 2500,
                report_id: 1,
                data: "b".into(),
            },
            CaptureReport {
                timestamp_us: 4000,
                report_id: 1,
                data: "c".into(),
            },
            CaptureReport {
                timestamp_us: 4500,
                report_id: 1,
                data: "d".into(),
            },
        ];
        let intervals: Vec<u64> = captures
            .windows(2)
            .map(|w| w[1].timestamp_us - w[0].timestamp_us)
            .collect();
        assert_eq!(intervals, vec![1500, 1500, 500]);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Report playback – filtering and querying
// ═══════════════════════════════════════════════════════════════════════════

mod playback {
    use super::*;

    #[test]
    fn filter_by_report_id() {
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
        ];
        let id_1: Vec<_> = captures.iter().filter(|r| r.report_id == 0x01).collect();
        assert_eq!(id_1.len(), 2);
        assert_eq!(id_1[0].data, "a");
        assert_eq!(id_1[1].data, "c");

        let id_3: Vec<_> = captures.iter().filter(|r| r.report_id == 0x03).collect();
        assert_eq!(id_3.len(), 1);
        assert_eq!(id_3[0].data, "d");
    }

    #[test]
    fn filter_by_time_range() {
        let captures = [
            CaptureReport {
                timestamp_us: 100,
                report_id: 1,
                data: "a".into(),
            },
            CaptureReport {
                timestamp_us: 500,
                report_id: 1,
                data: "b".into(),
            },
            CaptureReport {
                timestamp_us: 1000,
                report_id: 1,
                data: "c".into(),
            },
            CaptureReport {
                timestamp_us: 1500,
                report_id: 1,
                data: "d".into(),
            },
        ];
        let in_range: Vec<_> = captures
            .iter()
            .filter(|r| r.timestamp_us >= 400 && r.timestamp_us <= 1200)
            .collect();
        assert_eq!(in_range.len(), 2);
        assert_eq!(in_range[0].data, "b");
        assert_eq!(in_range[1].data, "c");
    }

    #[test]
    fn count_reports_by_id() {
        let captures = [
            CaptureReport {
                timestamp_us: 100,
                report_id: 0x01,
                data: "".into(),
            },
            CaptureReport {
                timestamp_us: 200,
                report_id: 0x02,
                data: "".into(),
            },
            CaptureReport {
                timestamp_us: 300,
                report_id: 0x01,
                data: "".into(),
            },
            CaptureReport {
                timestamp_us: 400,
                report_id: 0x01,
                data: "".into(),
            },
            CaptureReport {
                timestamp_us: 500,
                report_id: 0x02,
                data: "".into(),
            },
        ];
        let count_1 = captures.iter().filter(|r| r.report_id == 0x01).count();
        let count_2 = captures.iter().filter(|r| r.report_id == 0x02).count();
        assert_eq!(count_1, 3);
        assert_eq!(count_2, 2);
    }

    #[test]
    fn filter_nonexistent_id_returns_empty() {
        let captures = [CaptureReport {
            timestamp_us: 100,
            report_id: 0x01,
            data: "a".into(),
        }];
        let filtered: Vec<_> = captures.iter().filter(|r| r.report_id == 0xFF).collect();
        assert!(filtered.is_empty());
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// File format – disk I/O roundtrip
// ═══════════════════════════════════════════════════════════════════════════

mod file_io {
    use super::*;

    #[test]
    fn write_and_read_back() -> R {
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
                    report_id: 0x02,
                    data: "0x02 0x90 0xFF".to_string(),
                },
            ],
        };
        let dir = std::env::temp_dir().join("hid_capture_deep_test");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("roundtrip.json");
        let json = serde_json::to_string_pretty(&file)?;
        std::fs::write(&path, &json)?;

        let read_back = std::fs::read_to_string(&path)?;
        let restored: CaptureFile = serde_json::from_str(&read_back)?;
        assert_eq!(restored.vendor_id, "0x046D");
        assert_eq!(restored.product_id, "0xC266");
        assert_eq!(restored.captures.len(), 2);
        assert_eq!(restored.captures[0].data, "0x01 0x80 0x7F");

        // cleanup
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
        Ok(())
    }

    #[test]
    fn write_empty_file() -> R {
        let file = CaptureFile {
            vendor_id: "0x0000".to_string(),
            product_id: "0x0000".to_string(),
            captures: vec![],
        };
        let dir = std::env::temp_dir().join("hid_capture_deep_test_empty");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("empty.json");
        let json = serde_json::to_string(&file)?;
        std::fs::write(&path, &json)?;

        let read_back = std::fs::read_to_string(&path)?;
        let restored: CaptureFile = serde_json::from_str(&read_back)?;
        assert!(restored.captures.is_empty());

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Hex parsing – deep edge cases
// ═══════════════════════════════════════════════════════════════════════════

mod hex_parsing_deep {
    use super::*;

    #[test]
    fn prefix_0x_lowercase() {
        assert_eq!(parse_hex_u16("0xff"), Ok(0xFF));
    }

    #[test]
    fn prefix_0x_uppercase() {
        assert_eq!(parse_hex_u16("0XFF"), Ok(0xFF));
    }

    #[test]
    fn no_prefix() {
        assert_eq!(parse_hex_u16("FF"), Ok(0xFF));
    }

    #[test]
    fn zero() {
        assert_eq!(parse_hex_u16("0"), Ok(0));
        assert_eq!(parse_hex_u16("0x0"), Ok(0));
        assert_eq!(parse_hex_u16("0x0000"), Ok(0));
    }

    #[test]
    fn max_u16() {
        assert_eq!(parse_hex_u16("0xFFFF"), Ok(0xFFFF));
        assert_eq!(parse_hex_u16("FFFF"), Ok(0xFFFF));
        assert_eq!(parse_hex_u16("ffff"), Ok(0xFFFF));
    }

    #[test]
    fn overflow_u16() {
        assert!(parse_hex_u16("0x10000").is_err());
        assert!(parse_hex_u16("0xFFFFF").is_err());
        assert!(parse_hex_u16("100000").is_err());
    }

    #[test]
    fn empty_string_errors() {
        assert!(parse_hex_u16("").is_err());
    }

    #[test]
    fn bare_prefix_errors() {
        assert!(parse_hex_u16("0x").is_err());
        assert!(parse_hex_u16("0X").is_err());
    }

    #[test]
    fn invalid_chars() {
        assert!(parse_hex_u16("GHIJ").is_err());
        assert!(parse_hex_u16("0xZZ").is_err());
        assert!(parse_hex_u16("xyz").is_err());
    }

    #[test]
    fn whitespace_in_input_errors() {
        assert!(parse_hex_u16(" 0x01").is_err());
        assert!(parse_hex_u16("0x01 ").is_err());
        assert!(parse_hex_u16(" FF ").is_err());
    }

    #[test]
    fn mixed_case() {
        assert_eq!(parse_hex_u16("0xAbCd"), Ok(0xABCD));
        assert_eq!(parse_hex_u16("aBcD"), Ok(0xABCD));
    }

    #[test]
    fn leading_zeros() {
        assert_eq!(parse_hex_u16("0x0001"), Ok(1));
        assert_eq!(parse_hex_u16("0x00FF"), Ok(255));
        assert_eq!(parse_hex_u16("0001"), Ok(1));
    }

    #[test]
    fn single_digit() {
        assert_eq!(parse_hex_u16("0"), Ok(0));
        assert_eq!(parse_hex_u16("F"), Ok(15));
        assert_eq!(parse_hex_u16("0xA"), Ok(10));
    }

    #[test]
    fn common_vid_pid_values() {
        // Logitech
        assert_eq!(parse_hex_u16("0x046D"), Ok(0x046D));
        // Fanatec
        assert_eq!(parse_hex_u16("0x0EB7"), Ok(0x0EB7));
        // Thrustmaster
        assert_eq!(parse_hex_u16("0x044F"), Ok(0x044F));
    }

    #[test]
    fn vid_pid_format_roundtrip() {
        for original in [0u16, 1, 0x046D, 0x0EB7, 0xFFFF] {
            let formatted = format!("0x{original:04X}");
            assert_eq!(
                parse_hex_u16(&formatted),
                Ok(original),
                "roundtrip failed for {original}"
            );
        }
    }

    #[test]
    fn error_message_is_descriptive() {
        let result = parse_hex_u16("ZZZZ");
        if let Err(msg) = result {
            assert!(msg.contains("invalid hex value"));
            assert!(msg.contains("ZZZZ"));
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Error conditions – deserialization failures
// ═══════════════════════════════════════════════════════════════════════════

mod error_conditions {
    use super::*;

    #[test]
    fn malformed_json_fails() {
        assert!(serde_json::from_str::<CaptureFile>("not json").is_err());
    }

    #[test]
    fn empty_json_object_fails() {
        assert!(serde_json::from_str::<CaptureFile>("{}").is_err());
    }

    #[test]
    fn missing_product_id_fails() {
        let json = r#"{"vendor_id": "0x046D", "captures": []}"#;
        assert!(serde_json::from_str::<CaptureFile>(json).is_err());
    }

    #[test]
    fn missing_captures_field_fails() {
        let json = r#"{"vendor_id": "0x046D", "product_id": "0x0002"}"#;
        assert!(serde_json::from_str::<CaptureFile>(json).is_err());
    }

    #[test]
    fn wrong_type_timestamp_fails() {
        let json = r#"{"timestamp_us": "not_a_number", "report_id": 1, "data": "0x01"}"#;
        assert!(serde_json::from_str::<CaptureReport>(json).is_err());
    }

    #[test]
    fn wrong_type_report_id_fails() {
        let json = r#"{"timestamp_us": 100, "report_id": "one", "data": "0x01"}"#;
        assert!(serde_json::from_str::<CaptureReport>(json).is_err());
    }

    #[test]
    fn report_id_overflow_u8_fails() {
        let json = r#"{"timestamp_us": 100, "report_id": 256, "data": "0x01"}"#;
        assert!(serde_json::from_str::<CaptureReport>(json).is_err());
    }

    #[test]
    fn negative_report_id_fails() {
        let json = r#"{"timestamp_us": 100, "report_id": -1, "data": "0x01"}"#;
        assert!(serde_json::from_str::<CaptureReport>(json).is_err());
    }

    #[test]
    fn extra_fields_ignored_on_deserialization() -> R {
        let json = r#"{
            "vendor_id": "0x046D",
            "product_id": "0x0002",
            "captures": [],
            "extra_field": "should be ignored",
            "another": 42
        }"#;
        let file: CaptureFile = serde_json::from_str(json)?;
        assert_eq!(file.vendor_id, "0x046D");
        assert!(file.captures.is_empty());
        Ok(())
    }

    #[test]
    fn null_captures_array_fails() {
        let json = r#"{"vendor_id": "0x046D", "product_id": "0x0002", "captures": null}"#;
        assert!(serde_json::from_str::<CaptureFile>(json).is_err());
    }

    #[test]
    fn captures_as_string_fails() {
        let json = r#"{"vendor_id": "0x046D", "product_id": "0x0002", "captures": "not array"}"#;
        assert!(serde_json::from_str::<CaptureFile>(json).is_err());
    }

    #[test]
    fn negative_timestamp_fails() {
        let json = r#"{"timestamp_us": -100, "report_id": 1, "data": "0x01"}"#;
        assert!(serde_json::from_str::<CaptureReport>(json).is_err());
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Data integrity – various capture scenarios
// ═══════════════════════════════════════════════════════════════════════════

mod data_integrity {
    use super::*;

    #[test]
    fn duplicate_timestamps_preserved() -> R {
        let file = CaptureFile {
            vendor_id: "0x046D".to_string(),
            product_id: "0xC266".to_string(),
            captures: vec![
                CaptureReport {
                    timestamp_us: 1000,
                    report_id: 0x01,
                    data: "first".into(),
                },
                CaptureReport {
                    timestamp_us: 1000,
                    report_id: 0x02,
                    data: "second".into(),
                },
            ],
        };
        let json = serde_json::to_string(&file)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        assert_eq!(restored.captures.len(), 2);
        assert_eq!(
            restored.captures[0].timestamp_us,
            restored.captures[1].timestamp_us
        );
        assert_eq!(restored.captures[0].data, "first");
        assert_eq!(restored.captures[1].data, "second");
        Ok(())
    }

    #[test]
    fn unicode_in_vendor_product_id() -> R {
        // While unusual, serde should handle any valid string
        let file = CaptureFile {
            vendor_id: "日本語".to_string(),
            product_id: "émoji🎮".to_string(),
            captures: vec![],
        };
        let json = serde_json::to_string(&file)?;
        let restored: CaptureFile = serde_json::from_str(&json)?;
        assert_eq!(restored.vendor_id, "日本語");
        assert_eq!(restored.product_id, "émoji🎮");
        Ok(())
    }

    #[test]
    fn special_chars_in_data_field() -> R {
        let report = CaptureReport {
            timestamp_us: 0,
            report_id: 0,
            data: r#"contains "quotes" and \backslash and 🎮"#.to_string(),
        };
        let json = serde_json::to_string(&report)?;
        let restored: CaptureReport = serde_json::from_str(&json)?;
        assert_eq!(restored.data, report.data);
        Ok(())
    }

    #[test]
    fn all_report_id_values_valid() -> R {
        // Every u8 value should be a valid report_id
        for id in 0..=255u8 {
            let report = CaptureReport {
                timestamp_us: 0,
                report_id: id,
                data: String::new(),
            };
            let json = serde_json::to_string(&report)?;
            let restored: CaptureReport = serde_json::from_str(&json)?;
            assert_eq!(restored.report_id, id);
        }
        Ok(())
    }
}
