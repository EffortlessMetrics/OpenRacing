//! Deep tests for openracing-diagnostic.
//!
//! Covers: all diagnostic types and constructors, results formatting,
//! test suite management, health checks, device diagnostics, and error
//! conditions that existing tests do not exercise.

use openracing_diagnostic::{
    BlackboxConfig, BlackboxRecorder, DiagnosticError, DiagnosticResult, FrameData,
    HealthEventData, IndexEntry, SafetyStateSimple, StreamA, StreamC, StreamReader, StreamType,
    SupportBundle, SupportBundleConfig, TelemetryData, WbbFooter, WbbHeader,
    format::{
        DEFAULT_TIMEBASE_NS, INDEX_INTERVAL_MS, MAX_SUPPORTED_VERSION, STREAM_A_ID, STREAM_B_ID,
        STREAM_C_ID, WBB_FOOTER_MAGIC, WBB_MAGIC, WBB_VERSION,
    },
    replay::{ReplayConfig, ReplayResult, ReplayStatistics},
};
use tempfile::TempDir;

// ═══════════════════════════════════════════════════════════════════════════
// DiagnosticError – exhaustive construction and formatting
// ═══════════════════════════════════════════════════════════════════════════

mod error_exhaustive {
    use super::*;

    #[test]
    fn recording_error_contains_message() -> DiagnosticResult<()> {
        let e = DiagnosticError::Recording("buffer overflow".into());
        assert!(e.to_string().contains("buffer overflow"));
        assert!(e.to_string().starts_with("Recording error"));
        Ok(())
    }

    #[test]
    fn replay_error_contains_message() -> DiagnosticResult<()> {
        let e = DiagnosticError::Replay("frame mismatch at 42".into());
        assert!(e.to_string().contains("frame mismatch at 42"));
        Ok(())
    }

    #[test]
    fn format_error_contains_message() -> DiagnosticResult<()> {
        let e = DiagnosticError::Format("bad header alignment".into());
        assert!(e.to_string().contains("bad header alignment"));
        Ok(())
    }

    #[test]
    fn io_error_contains_message() -> DiagnosticResult<()> {
        let e = DiagnosticError::Io("permission denied".into());
        assert!(e.to_string().contains("permission denied"));
        Ok(())
    }

    #[test]
    fn serialization_error_contains_message() -> DiagnosticResult<()> {
        let e = DiagnosticError::Serialization("encode failure".into());
        assert!(e.to_string().contains("encode failure"));
        Ok(())
    }

    #[test]
    fn deserialization_error_contains_message() -> DiagnosticResult<()> {
        let e = DiagnosticError::Deserialization("unexpected EOF".into());
        assert!(e.to_string().contains("unexpected EOF"));
        Ok(())
    }

    #[test]
    fn compression_error_contains_message() -> DiagnosticResult<()> {
        let e = DiagnosticError::Compression("zlib stream corrupt".into());
        assert!(e.to_string().contains("zlib stream corrupt"));
        Ok(())
    }

    #[test]
    fn size_limit_error_contains_message() -> DiagnosticResult<()> {
        let e = DiagnosticError::SizeLimit("exceeded 25MB cap".into());
        assert!(e.to_string().contains("exceeded 25MB cap"));
        Ok(())
    }

    #[test]
    fn configuration_error_contains_message() -> DiagnosticResult<()> {
        let e = DiagnosticError::Configuration("missing output_dir".into());
        assert!(e.to_string().contains("missing output_dir"));
        Ok(())
    }

    #[test]
    fn validation_error_contains_message() -> DiagnosticResult<()> {
        let e = DiagnosticError::Validation("timestamp out of range".into());
        assert!(e.to_string().contains("timestamp out of range"));
        Ok(())
    }

    #[test]
    fn crc_mismatch_displays_both_values() -> DiagnosticResult<()> {
        let e = DiagnosticError::CrcMismatch {
            expected: 0x1234_5678,
            actual: 0xABCD_EF01,
        };
        let msg = e.to_string();
        assert!(msg.contains("CRC mismatch"));
        assert!(msg.contains("expected"));
        assert!(msg.contains("got"));
        Ok(())
    }

    #[test]
    fn invalid_magic_displays_both_arrays() -> DiagnosticResult<()> {
        let e = DiagnosticError::InvalidMagic {
            expected: *b"WBB1",
            actual: *b"NOPE",
        };
        let msg = e.to_string();
        assert!(msg.contains("Invalid magic"));
        Ok(())
    }

    #[test]
    fn unsupported_version_displays_number() -> DiagnosticResult<()> {
        let e = DiagnosticError::UnsupportedVersion(255);
        assert!(e.to_string().contains("255"));
        Ok(())
    }

    #[test]
    fn clone_preserves_all_variants() -> DiagnosticResult<()> {
        let variants: Vec<DiagnosticError> = vec![
            DiagnosticError::Recording("r".into()),
            DiagnosticError::Replay("rp".into()),
            DiagnosticError::Format("f".into()),
            DiagnosticError::Io("i".into()),
            DiagnosticError::Serialization("s".into()),
            DiagnosticError::Deserialization("d".into()),
            DiagnosticError::Compression("c".into()),
            DiagnosticError::SizeLimit("sl".into()),
            DiagnosticError::Configuration("cfg".into()),
            DiagnosticError::Validation("v".into()),
            DiagnosticError::UnsupportedVersion(7),
            DiagnosticError::CrcMismatch {
                expected: 1,
                actual: 2,
            },
            DiagnosticError::InvalidMagic {
                expected: *b"WBB1",
                actual: *b"XXXX",
            },
        ];
        for v in &variants {
            let cloned = v.clone();
            assert_eq!(v.to_string(), cloned.to_string());
        }
        Ok(())
    }

    #[test]
    fn error_is_std_error_and_debug() -> DiagnosticResult<()> {
        let e = DiagnosticError::Recording("test".into());
        // Verify it implements std::error::Error
        let _: &dyn std::error::Error = &e;
        // Verify Debug
        let dbg = format!("{e:?}");
        assert!(!dbg.is_empty());
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// DiagnosticError From impls
// ═══════════════════════════════════════════════════════════════════════════

mod error_from_impls {
    use super::*;

    #[test]
    fn from_io_error_preserves_message() -> DiagnosticResult<()> {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let diag: DiagnosticError = io_err.into();
        assert!(matches!(diag, DiagnosticError::Io(_)));
        assert!(diag.to_string().contains("access denied"));
        Ok(())
    }

    #[test]
    fn from_io_error_not_found() -> DiagnosticResult<()> {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing file");
        let diag: DiagnosticError = io_err.into();
        assert!(matches!(diag, DiagnosticError::Io(_)));
        Ok(())
    }

    #[test]
    fn from_serde_json_error() -> DiagnosticResult<()> {
        let result: std::result::Result<serde_json::Value, _> =
            serde_json::from_str("not valid json");
        if let Err(json_err) = result {
            let diag: DiagnosticError = json_err.into();
            assert!(matches!(diag, DiagnosticError::Serialization(_)));
        }
        Ok(())
    }

    #[test]
    fn diagnostic_result_type_alias_works() -> DiagnosticResult<()> {
        fn returns_ok() -> DiagnosticResult<u32> {
            Ok(42)
        }
        fn returns_err() -> DiagnosticResult<u32> {
            Err(DiagnosticError::Validation("bad".into()))
        }
        assert_eq!(returns_ok()?, 42);
        assert!(returns_err().is_err());
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Format constants
// ═══════════════════════════════════════════════════════════════════════════

mod format_constants {
    use super::*;

    #[test]
    fn magic_bytes_are_correct() -> DiagnosticResult<()> {
        assert_eq!(WBB_MAGIC, b"WBB1");
        assert_eq!(WBB_FOOTER_MAGIC, b"1BBW");
        Ok(())
    }

    #[test]
    fn version_matches_max_supported() -> DiagnosticResult<()> {
        assert_eq!(WBB_VERSION, 1);
        assert_eq!(MAX_SUPPORTED_VERSION, 1);
        Ok(())
    }

    #[test]
    fn timebase_is_one_millisecond() -> DiagnosticResult<()> {
        assert_eq!(DEFAULT_TIMEBASE_NS, 1_000_000);
        Ok(())
    }

    #[test]
    fn index_interval_is_100ms() -> DiagnosticResult<()> {
        assert_eq!(INDEX_INTERVAL_MS, 100);
        Ok(())
    }

    #[test]
    fn stream_ids_are_power_of_two() -> DiagnosticResult<()> {
        assert_eq!(STREAM_A_ID, 0x01);
        assert_eq!(STREAM_B_ID, 0x02);
        assert_eq!(STREAM_C_ID, 0x04);
        // Non-overlapping
        assert_eq!(STREAM_A_ID & STREAM_B_ID, 0);
        assert_eq!(STREAM_A_ID & STREAM_C_ID, 0);
        assert_eq!(STREAM_B_ID & STREAM_C_ID, 0);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// WbbHeader – deep validation
// ═══════════════════════════════════════════════════════════════════════════

mod header_deep {
    use super::*;

    #[test]
    fn new_header_sets_timebase_and_reserved() -> DiagnosticResult<()> {
        let h = WbbHeader::new("dev-1", 2, 0x07, 3);
        assert_eq!(h.timebase_ns, DEFAULT_TIMEBASE_NS);
        assert_eq!(h.reserved, [0; 15]);
        assert!(!h.engine_version.is_empty());
        Ok(())
    }

    #[test]
    fn start_time_is_recent() -> DiagnosticResult<()> {
        let h = WbbHeader::new("dev", 0, 0, 0);
        // Should be a reasonable Unix timestamp (after year 2020)
        assert!(h.start_time_unix > 1_577_836_800); // 2020-01-01
        Ok(())
    }

    #[test]
    fn validate_compression_boundary_9_ok() -> DiagnosticResult<()> {
        let h = WbbHeader::new("d", 0, 0, 9);
        h.validate()?;
        Ok(())
    }

    #[test]
    fn validate_compression_boundary_10_err() -> DiagnosticResult<()> {
        let mut h = WbbHeader::new("d", 0, 0, 0);
        h.compression_level = 10;
        assert!(h.validate().is_err());
        Ok(())
    }

    #[test]
    fn validate_compression_0_ok() -> DiagnosticResult<()> {
        let h = WbbHeader::new("d", 0, 0, 0);
        h.validate()?;
        Ok(())
    }

    #[test]
    fn validate_version_1_ok() -> DiagnosticResult<()> {
        let h = WbbHeader::new("d", 0, 0, 0);
        assert_eq!(h.version, 1);
        h.validate()?;
        Ok(())
    }

    #[test]
    fn validate_version_0_ok() -> DiagnosticResult<()> {
        let mut h = WbbHeader::new("d", 0, 0, 0);
        h.version = 0;
        h.validate()?;
        Ok(())
    }

    #[test]
    fn validate_version_2_err() -> DiagnosticResult<()> {
        let mut h = WbbHeader::new("d", 0, 0, 0);
        h.version = 2;
        let result = h.validate();
        assert!(matches!(
            result,
            Err(DiagnosticError::UnsupportedVersion(2))
        ));
        Ok(())
    }

    #[test]
    fn stream_flags_all_combinations() -> DiagnosticResult<()> {
        // No streams
        let h = WbbHeader::new("d", 0, 0, 0);
        assert!(!h.has_stream_a());
        assert!(!h.has_stream_b());
        assert!(!h.has_stream_c());

        // Only A
        let h = WbbHeader::new("d", 0, STREAM_A_ID, 0);
        assert!(h.has_stream_a());
        assert!(!h.has_stream_b());
        assert!(!h.has_stream_c());

        // Only B
        let h = WbbHeader::new("d", 0, STREAM_B_ID, 0);
        assert!(!h.has_stream_a());
        assert!(h.has_stream_b());
        assert!(!h.has_stream_c());

        // Only C
        let h = WbbHeader::new("d", 0, STREAM_C_ID, 0);
        assert!(!h.has_stream_a());
        assert!(!h.has_stream_b());
        assert!(h.has_stream_c());

        // A+B
        let h = WbbHeader::new("d", 0, STREAM_A_ID | STREAM_B_ID, 0);
        assert!(h.has_stream_a());
        assert!(h.has_stream_b());
        assert!(!h.has_stream_c());

        // All
        let h = WbbHeader::new("d", 0, STREAM_A_ID | STREAM_B_ID | STREAM_C_ID, 0);
        assert!(h.has_stream_a());
        assert!(h.has_stream_b());
        assert!(h.has_stream_c());

        Ok(())
    }

    #[test]
    fn header_is_serializable_with_serde() -> DiagnosticResult<()> {
        let h = WbbHeader::new("device-x", 1, 0x07, 6);
        let json =
            serde_json::to_string(&h).map_err(|e| DiagnosticError::Serialization(e.to_string()))?;
        let restored: WbbHeader = serde_json::from_str(&json)
            .map_err(|e| DiagnosticError::Deserialization(e.to_string()))?;
        assert_eq!(restored.device_id, "device-x");
        assert_eq!(restored.magic, *WBB_MAGIC);
        assert_eq!(restored.version, WBB_VERSION);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// WbbFooter – deep validation
// ═══════════════════════════════════════════════════════════════════════════

mod footer_deep {
    use super::*;

    #[test]
    fn new_footer_defaults() -> DiagnosticResult<()> {
        let f = WbbFooter::new(5000, 500);
        assert_eq!(f.duration_ms, 5000);
        assert_eq!(f.total_frames, 500);
        assert_eq!(f.index_offset, 0);
        assert_eq!(f.index_count, 0);
        assert_eq!(f.file_crc32c, 0);
        assert_eq!(f.footer_magic, *WBB_FOOTER_MAGIC);
        Ok(())
    }

    #[test]
    fn validate_correct_magic() -> DiagnosticResult<()> {
        let f = WbbFooter::new(0, 0);
        f.validate()?;
        Ok(())
    }

    #[test]
    fn validate_bad_magic_returns_invalid_magic_error() -> DiagnosticResult<()> {
        let mut f = WbbFooter::new(0, 0);
        f.footer_magic = *b"ZZZZ";
        let result = f.validate();
        assert!(matches!(result, Err(DiagnosticError::InvalidMagic { .. })));
        Ok(())
    }

    #[test]
    fn footer_with_large_values() -> DiagnosticResult<()> {
        let f = WbbFooter::new(u32::MAX, u64::MAX);
        assert_eq!(f.duration_ms, u32::MAX);
        assert_eq!(f.total_frames, u64::MAX);
        f.validate()?;
        Ok(())
    }

    #[test]
    fn footer_serde_roundtrip() -> DiagnosticResult<()> {
        let mut f = WbbFooter::new(3000, 300);
        f.index_offset = 1024;
        f.index_count = 30;
        f.file_crc32c = 0xDEADBEEF;
        let json =
            serde_json::to_string(&f).map_err(|e| DiagnosticError::Serialization(e.to_string()))?;
        let restored: WbbFooter = serde_json::from_str(&json)
            .map_err(|e| DiagnosticError::Deserialization(e.to_string()))?;
        assert_eq!(restored.duration_ms, 3000);
        assert_eq!(restored.total_frames, 300);
        assert_eq!(restored.index_offset, 1024);
        assert_eq!(restored.index_count, 30);
        assert_eq!(restored.file_crc32c, 0xDEADBEEF);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// IndexEntry
// ═══════════════════════════════════════════════════════════════════════════

mod index_entry_deep {
    use super::*;

    #[test]
    fn new_sets_defaults() -> DiagnosticResult<()> {
        let e = IndexEntry::new(500, 100);
        assert_eq!(e.timestamp_ms, 500);
        assert_eq!(e.frame_count, 100);
        assert_eq!(e.stream_a_offset, 0);
        assert_eq!(e.stream_b_offset, 0);
        assert_eq!(e.stream_c_offset, 0);
        Ok(())
    }

    #[test]
    fn index_entry_serde_roundtrip() -> DiagnosticResult<()> {
        let mut e = IndexEntry::new(200, 50);
        e.stream_a_offset = 4096;
        e.stream_b_offset = 8192;
        e.stream_c_offset = 16384;
        let json = serde_json::to_string(&e)
            .map_err(|err| DiagnosticError::Serialization(err.to_string()))?;
        let restored: IndexEntry = serde_json::from_str(&json)
            .map_err(|err| DiagnosticError::Deserialization(err.to_string()))?;
        assert_eq!(restored.timestamp_ms, 200);
        assert_eq!(restored.frame_count, 50);
        assert_eq!(restored.stream_a_offset, 4096);
        assert_eq!(restored.stream_b_offset, 8192);
        assert_eq!(restored.stream_c_offset, 16384);
        Ok(())
    }

    #[test]
    fn index_entry_clone() -> DiagnosticResult<()> {
        let e = IndexEntry::new(100, 10);
        let c = e.clone();
        assert_eq!(c.timestamp_ms, 100);
        assert_eq!(c.frame_count, 10);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// StreamType
// ═══════════════════════════════════════════════════════════════════════════

mod stream_type_deep {
    use super::*;

    #[test]
    fn flag_values_match_constants() -> DiagnosticResult<()> {
        assert_eq!(StreamType::A.flag(), STREAM_A_ID);
        assert_eq!(StreamType::B.flag(), STREAM_B_ID);
        assert_eq!(StreamType::C.flag(), STREAM_C_ID);
        Ok(())
    }

    #[test]
    fn all_flags_are_unique() -> DiagnosticResult<()> {
        let flags = [
            StreamType::A.flag(),
            StreamType::B.flag(),
            StreamType::C.flag(),
        ];
        for i in 0..flags.len() {
            for j in (i + 1)..flags.len() {
                assert_ne!(flags[i], flags[j], "stream flags must be unique");
                assert_eq!(flags[i] & flags[j], 0, "stream flags must not overlap");
            }
        }
        Ok(())
    }

    #[test]
    fn stream_type_is_copy() -> DiagnosticResult<()> {
        let a = StreamType::A;
        let b = a;
        assert_eq!(a, b);
        Ok(())
    }

    #[test]
    fn stream_type_debug() -> DiagnosticResult<()> {
        let dbg = format!("{:?}", StreamType::B);
        assert!(dbg.contains("B"));
        Ok(())
    }

    #[test]
    fn stream_type_hash() -> DiagnosticResult<()> {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(StreamType::A);
        set.insert(StreamType::B);
        set.insert(StreamType::C);
        assert_eq!(set.len(), 3);
        assert!(set.contains(&StreamType::A));
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// FrameData – deep tests
// ═══════════════════════════════════════════════════════════════════════════

mod frame_data_deep {
    use super::*;

    #[test]
    fn default_all_zeros() -> DiagnosticResult<()> {
        let f = FrameData::default();
        assert!((f.ffb_in).abs() < f32::EPSILON);
        assert!((f.torque_out).abs() < f32::EPSILON);
        assert!((f.wheel_speed).abs() < f32::EPSILON);
        assert!(!f.hands_off);
        assert_eq!(f.ts_mono_ns, 0);
        assert_eq!(f.seq, 0);
        Ok(())
    }

    #[test]
    fn extreme_float_values() -> DiagnosticResult<()> {
        let f = FrameData {
            ffb_in: -1.0,
            torque_out: 1.0,
            wheel_speed: f32::MAX,
            hands_off: true,
            ts_mono_ns: u64::MAX,
            seq: u16::MAX,
        };
        assert!((f.ffb_in - (-1.0)).abs() < f32::EPSILON);
        assert!((f.torque_out - 1.0).abs() < f32::EPSILON);
        assert!(f.hands_off);
        assert_eq!(f.seq, u16::MAX);
        Ok(())
    }

    #[test]
    fn serde_roundtrip() -> DiagnosticResult<()> {
        let f = FrameData {
            ffb_in: 0.42,
            torque_out: -0.99,
            wheel_speed: 123.456,
            hands_off: true,
            ts_mono_ns: 999_999_999,
            seq: 1234,
        };
        let json =
            serde_json::to_string(&f).map_err(|e| DiagnosticError::Serialization(e.to_string()))?;
        let restored: FrameData = serde_json::from_str(&json)
            .map_err(|e| DiagnosticError::Deserialization(e.to_string()))?;
        assert!((restored.ffb_in - 0.42).abs() < 0.001);
        assert_eq!(restored.seq, 1234);
        assert!(restored.hands_off);
        Ok(())
    }

    #[test]
    fn clone_preserves_values() -> DiagnosticResult<()> {
        let f = FrameData {
            ffb_in: 0.5,
            torque_out: 0.3,
            wheel_speed: 10.0,
            hands_off: false,
            ts_mono_ns: 42,
            seq: 7,
        };
        let c = f.clone();
        assert!((c.ffb_in - f.ffb_in).abs() < f32::EPSILON);
        assert_eq!(c.seq, f.seq);
        assert_eq!(c.ts_mono_ns, f.ts_mono_ns);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SafetyStateSimple – all variants
// ═══════════════════════════════════════════════════════════════════════════

mod safety_state_deep {
    use super::*;

    #[test]
    fn default_is_safe_torque() -> DiagnosticResult<()> {
        assert!(matches!(
            SafetyStateSimple::default(),
            SafetyStateSimple::SafeTorque
        ));
        Ok(())
    }

    #[test]
    fn high_torque_challenge_variant() -> DiagnosticResult<()> {
        let s = SafetyStateSimple::HighTorqueChallenge;
        assert!(matches!(s, SafetyStateSimple::HighTorqueChallenge));
        Ok(())
    }

    #[test]
    fn awaiting_physical_ack_variant() -> DiagnosticResult<()> {
        let s = SafetyStateSimple::AwaitingPhysicalAck;
        assert!(matches!(s, SafetyStateSimple::AwaitingPhysicalAck));
        Ok(())
    }

    #[test]
    fn high_torque_active_variant() -> DiagnosticResult<()> {
        let s = SafetyStateSimple::HighTorqueActive;
        assert!(matches!(s, SafetyStateSimple::HighTorqueActive));
        Ok(())
    }

    #[test]
    fn faulted_variant_with_fault_type() -> DiagnosticResult<()> {
        let s = SafetyStateSimple::Faulted {
            fault_type: "OverCurrent".to_string(),
        };
        if let SafetyStateSimple::Faulted { fault_type } = &s {
            assert_eq!(fault_type, "OverCurrent");
        } else {
            return Err(DiagnosticError::Validation("expected Faulted".into()));
        }
        Ok(())
    }

    #[test]
    fn faulted_clone_preserves_fault_type() -> DiagnosticResult<()> {
        let s = SafetyStateSimple::Faulted {
            fault_type: "OverTemp".to_string(),
        };
        let c = s.clone();
        if let SafetyStateSimple::Faulted { fault_type } = &c {
            assert_eq!(fault_type, "OverTemp");
        } else {
            return Err(DiagnosticError::Validation(
                "clone should be Faulted".into(),
            ));
        }
        Ok(())
    }

    #[test]
    fn all_variants_are_serializable() -> DiagnosticResult<()> {
        let variants = [
            SafetyStateSimple::SafeTorque,
            SafetyStateSimple::HighTorqueChallenge,
            SafetyStateSimple::AwaitingPhysicalAck,
            SafetyStateSimple::HighTorqueActive,
            SafetyStateSimple::Faulted {
                fault_type: "test".to_string(),
            },
        ];
        for v in &variants {
            let json = serde_json::to_string(v)
                .map_err(|e| DiagnosticError::Serialization(e.to_string()))?;
            assert!(!json.is_empty());
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TelemetryData
// ═══════════════════════════════════════════════════════════════════════════

mod telemetry_deep {
    use super::*;

    #[test]
    fn default_values() -> DiagnosticResult<()> {
        let t = TelemetryData::default();
        assert!((t.ffb_scalar - 1.0).abs() < f32::EPSILON);
        assert!((t.rpm).abs() < f32::EPSILON);
        assert!((t.speed_ms).abs() < f32::EPSILON);
        assert!((t.slip_ratio).abs() < f32::EPSILON);
        assert_eq!(t.gear, 0);
        assert!(t.car_id.is_none());
        assert!(t.track_id.is_none());
        Ok(())
    }

    #[test]
    fn with_optional_fields() -> DiagnosticResult<()> {
        let t = TelemetryData {
            ffb_scalar: 0.75,
            rpm: 8000.0,
            speed_ms: 55.0,
            slip_ratio: 0.15,
            gear: 5,
            car_id: Some("ferrari_488".to_string()),
            track_id: Some("monza".to_string()),
        };
        assert_eq!(t.car_id.as_deref(), Some("ferrari_488"));
        assert_eq!(t.track_id.as_deref(), Some("monza"));
        assert_eq!(t.gear, 5);
        Ok(())
    }

    #[test]
    fn serde_roundtrip_with_none_optionals() -> DiagnosticResult<()> {
        let t = TelemetryData::default();
        let json =
            serde_json::to_string(&t).map_err(|e| DiagnosticError::Serialization(e.to_string()))?;
        let restored: TelemetryData = serde_json::from_str(&json)
            .map_err(|e| DiagnosticError::Deserialization(e.to_string()))?;
        assert!(restored.car_id.is_none());
        assert!(restored.track_id.is_none());
        Ok(())
    }

    #[test]
    fn negative_gear_for_reverse() -> DiagnosticResult<()> {
        let t = TelemetryData {
            gear: -1,
            ..Default::default()
        };
        assert_eq!(t.gear, -1);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// HealthEventData
// ═══════════════════════════════════════════════════════════════════════════

mod health_event_deep {
    use super::*;

    #[test]
    fn construction_and_fields() -> DiagnosticResult<()> {
        let e = HealthEventData {
            timestamp_ns: 123_456_789,
            device_id: "wheel-pro".to_string(),
            event_type: "FirmwareUpdate".to_string(),
            context: serde_json::json!({"version": "2.0.1"}),
        };
        assert_eq!(e.timestamp_ns, 123_456_789);
        assert_eq!(e.device_id, "wheel-pro");
        assert_eq!(e.event_type, "FirmwareUpdate");
        assert!(e.context.is_object());
        Ok(())
    }

    #[test]
    fn null_context() -> DiagnosticResult<()> {
        let e = HealthEventData {
            timestamp_ns: 0,
            device_id: "d".to_string(),
            event_type: "Ping".to_string(),
            context: serde_json::Value::Null,
        };
        assert!(e.context.is_null());
        Ok(())
    }

    #[test]
    fn complex_context() -> DiagnosticResult<()> {
        let ctx = serde_json::json!({
            "temps": [65.2, 72.1, 58.0],
            "faults": {"count": 3, "codes": [101, 202, 303]},
            "active": true,
        });
        let e = HealthEventData {
            timestamp_ns: 1000,
            device_id: "d".to_string(),
            event_type: "DiagDump".to_string(),
            context: ctx,
        };
        assert!(e.context.is_object());
        let json = serde_json::to_string(&e)
            .map_err(|err| DiagnosticError::Serialization(err.to_string()))?;
        assert!(json.contains("temps"));
        Ok(())
    }

    #[test]
    fn clone_preserves_context() -> DiagnosticResult<()> {
        let e = HealthEventData {
            timestamp_ns: 42,
            device_id: "w".to_string(),
            event_type: "E".to_string(),
            context: serde_json::json!({"key": "value"}),
        };
        let c = e.clone();
        assert_eq!(c.context, e.context);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// BlackboxConfig
// ═══════════════════════════════════════════════════════════════════════════

mod config_deep {
    use super::*;

    #[test]
    fn defaults() -> DiagnosticResult<()> {
        let cfg = BlackboxConfig::new("dev-1", "/tmp/rec");
        assert_eq!(cfg.device_id, "dev-1");
        assert_eq!(cfg.max_duration_s, 300);
        assert_eq!(cfg.max_file_size_bytes, 25 * 1024 * 1024);
        assert_eq!(cfg.compression_level, 6);
        assert!(cfg.enable_stream_a);
        assert!(cfg.enable_stream_b);
        assert!(cfg.enable_stream_c);
        Ok(())
    }

    #[test]
    fn stream_flags_all_enabled() -> DiagnosticResult<()> {
        let cfg = BlackboxConfig::new("d", "/tmp");
        assert_eq!(cfg.stream_flags(), STREAM_A_ID | STREAM_B_ID | STREAM_C_ID);
        Ok(())
    }

    #[test]
    fn stream_flags_none_enabled() -> DiagnosticResult<()> {
        let cfg = BlackboxConfig {
            enable_stream_a: false,
            enable_stream_b: false,
            enable_stream_c: false,
            ..BlackboxConfig::new("d", "/tmp")
        };
        assert_eq!(cfg.stream_flags(), 0);
        Ok(())
    }

    #[test]
    fn stream_flags_only_a() -> DiagnosticResult<()> {
        let cfg = BlackboxConfig {
            enable_stream_a: true,
            enable_stream_b: false,
            enable_stream_c: false,
            ..BlackboxConfig::new("d", "/tmp")
        };
        assert_eq!(cfg.stream_flags(), STREAM_A_ID);
        Ok(())
    }

    #[test]
    fn stream_flags_only_b_and_c() -> DiagnosticResult<()> {
        let cfg = BlackboxConfig {
            enable_stream_a: false,
            enable_stream_b: true,
            enable_stream_c: true,
            ..BlackboxConfig::new("d", "/tmp")
        };
        assert_eq!(cfg.stream_flags(), STREAM_B_ID | STREAM_C_ID);
        Ok(())
    }

    #[test]
    fn clone_preserves_all_fields() -> DiagnosticResult<()> {
        let cfg = BlackboxConfig {
            max_duration_s: 60,
            max_file_size_bytes: 1024,
            compression_level: 3,
            enable_stream_a: false,
            ..BlackboxConfig::new("cloned", "/cloned")
        };
        let c = cfg.clone();
        assert_eq!(c.device_id, "cloned");
        assert_eq!(c.max_duration_s, 60);
        assert_eq!(c.max_file_size_bytes, 1024);
        assert_eq!(c.compression_level, 3);
        assert!(!c.enable_stream_a);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// RecordingStats
// ═══════════════════════════════════════════════════════════════════════════

mod recording_stats_deep {
    use super::*;
    use openracing_diagnostic::RecordingStats;

    #[test]
    fn default_values() -> DiagnosticResult<()> {
        let s = RecordingStats::default();
        assert_eq!(s.frames_recorded, 0);
        assert_eq!(s.telemetry_records, 0);
        assert_eq!(s.health_events, 0);
        assert_eq!(s.file_size_bytes, 0);
        assert!((s.compression_ratio - 1.0).abs() < f64::EPSILON);
        assert!(!s.is_active);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// BlackboxRecorder – recording lifecycle
// ═══════════════════════════════════════════════════════════════════════════

mod recorder_deep {
    use super::*;

    fn temp_config() -> DiagnosticResult<(BlackboxConfig, TempDir)> {
        let d = tempfile::tempdir().map_err(|e| DiagnosticError::Io(e.to_string()))?;
        let cfg = BlackboxConfig::new("test-dev", d.path());
        Ok((cfg, d))
    }

    #[test]
    fn new_recorder_is_active_with_zero_frames() -> DiagnosticResult<()> {
        let (cfg, _td) = temp_config()?;
        let recorder = BlackboxRecorder::new(cfg)?;
        let stats = recorder.get_stats();
        assert!(stats.is_active);
        assert_eq!(stats.frames_recorded, 0);
        assert_eq!(stats.telemetry_records, 0);
        assert_eq!(stats.health_events, 0);
        Ok(())
    }

    #[test]
    fn record_frame_increments_count() -> DiagnosticResult<()> {
        let (cfg, _td) = temp_config()?;
        let mut recorder = BlackboxRecorder::new(cfg)?;
        let frame = FrameData::default();
        recorder.record_frame(frame, &[], SafetyStateSimple::SafeTorque, 100)?;
        assert_eq!(recorder.get_stats().frames_recorded, 1);
        Ok(())
    }

    #[test]
    fn record_multiple_frames() -> DiagnosticResult<()> {
        let (cfg, _td) = temp_config()?;
        let mut recorder = BlackboxRecorder::new(cfg)?;
        for i in 0..50 {
            let frame = FrameData {
                seq: i,
                ..Default::default()
            };
            recorder.record_frame(frame, &[0.1], SafetyStateSimple::SafeTorque, 50)?;
        }
        assert_eq!(recorder.get_stats().frames_recorded, 50);
        Ok(())
    }

    #[test]
    fn record_with_disabled_stream_a_skips() -> DiagnosticResult<()> {
        let (mut cfg, _td) = temp_config()?;
        cfg.enable_stream_a = false;
        let mut recorder = BlackboxRecorder::new(cfg)?;
        recorder.record_frame(FrameData::default(), &[], SafetyStateSimple::SafeTorque, 0)?;
        assert_eq!(recorder.get_stats().frames_recorded, 0);
        Ok(())
    }

    #[test]
    fn record_telemetry_with_disabled_stream_b_skips() -> DiagnosticResult<()> {
        let (mut cfg, _td) = temp_config()?;
        cfg.enable_stream_b = false;
        let mut recorder = BlackboxRecorder::new(cfg)?;
        recorder.record_telemetry(TelemetryData::default())?;
        assert_eq!(recorder.get_stats().telemetry_records, 0);
        Ok(())
    }

    #[test]
    fn record_health_event_increments_count() -> DiagnosticResult<()> {
        let (cfg, _td) = temp_config()?;
        let mut recorder = BlackboxRecorder::new(cfg)?;
        let event = HealthEventData {
            timestamp_ns: 0,
            device_id: "d".to_string(),
            event_type: "E".to_string(),
            context: serde_json::json!(null),
        };
        recorder.record_health_event(event)?;
        assert_eq!(recorder.get_stats().health_events, 1);
        Ok(())
    }

    #[test]
    fn record_health_event_disabled_stream_c_skips() -> DiagnosticResult<()> {
        let (mut cfg, _td) = temp_config()?;
        cfg.enable_stream_c = false;
        let mut recorder = BlackboxRecorder::new(cfg)?;
        let event = HealthEventData {
            timestamp_ns: 0,
            device_id: "d".to_string(),
            event_type: "E".to_string(),
            context: serde_json::json!(null),
        };
        recorder.record_health_event(event)?;
        assert_eq!(recorder.get_stats().health_events, 0);
        Ok(())
    }

    #[test]
    fn finalize_produces_wbb_file() -> DiagnosticResult<()> {
        let (cfg, _td) = temp_config()?;
        let mut recorder = BlackboxRecorder::new(cfg)?;
        for i in 0..5 {
            let frame = FrameData {
                seq: i,
                ..Default::default()
            };
            recorder.record_frame(frame, &[], SafetyStateSimple::SafeTorque, 0)?;
        }
        let path = recorder.finalize()?;
        assert!(path.exists());
        let ext = path
            .extension()
            .ok_or(DiagnosticError::Validation("no extension".into()))?;
        assert_eq!(ext, "wbb");
        let meta = std::fs::metadata(&path).map_err(|e| DiagnosticError::Io(e.to_string()))?;
        assert!(meta.len() > 0);
        Ok(())
    }

    #[test]
    fn output_path_method() -> DiagnosticResult<()> {
        let (cfg, _td) = temp_config()?;
        let recorder = BlackboxRecorder::new(cfg)?;
        let path = recorder.output_path();
        assert!(path.to_string_lossy().contains("blackbox_"));
        assert!(path.to_string_lossy().contains("test-dev"));
        Ok(())
    }

    #[test]
    fn finalize_with_no_compression() -> DiagnosticResult<()> {
        let (mut cfg, _td) = temp_config()?;
        cfg.compression_level = 0;
        let mut recorder = BlackboxRecorder::new(cfg)?;
        recorder.record_frame(
            FrameData::default(),
            &[0.5],
            SafetyStateSimple::SafeTorque,
            100,
        )?;
        let path = recorder.finalize()?;
        assert!(path.exists());
        Ok(())
    }

    #[test]
    fn finalize_with_all_streams_recorded() -> DiagnosticResult<()> {
        let (cfg, _td) = temp_config()?;
        let mut recorder = BlackboxRecorder::new(cfg)?;

        // Stream A
        recorder.record_frame(
            FrameData::default(),
            &[0.1],
            SafetyStateSimple::SafeTorque,
            50,
        )?;

        // Stream B
        recorder.record_telemetry(TelemetryData {
            rpm: 5000.0,
            gear: 3,
            ..Default::default()
        })?;

        // Stream C
        recorder.record_health_event(HealthEventData {
            timestamp_ns: 0,
            device_id: "d".to_string(),
            event_type: "Start".to_string(),
            context: serde_json::json!({}),
        })?;

        let path = recorder.finalize()?;
        assert!(path.exists());
        Ok(())
    }

    #[test]
    fn finalize_empty_recording() -> DiagnosticResult<()> {
        let (cfg, _td) = temp_config()?;
        let recorder = BlackboxRecorder::new(cfg)?;
        let path = recorder.finalize()?;
        assert!(path.exists());
        Ok(())
    }

    #[test]
    fn device_id_with_special_chars_in_filename() -> DiagnosticResult<()> {
        let td = tempfile::tempdir().map_err(|e| DiagnosticError::Io(e.to_string()))?;
        let cfg = BlackboxConfig::new("dev/sub:path", td.path());
        let recorder = BlackboxRecorder::new(cfg)?;
        let path = recorder.output_path();
        let filename = path
            .file_name()
            .ok_or(DiagnosticError::Validation("no filename".into()))?
            .to_string_lossy();
        // Slashes and colons should be replaced with underscores
        assert!(!filename.contains('/'));
        assert!(!filename.contains(':'));
        assert!(filename.contains("dev_sub_path"));
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Stream A – deep tests
// ═══════════════════════════════════════════════════════════════════════════

mod stream_a_deep {
    use super::*;

    #[test]
    fn new_has_zero_records() -> DiagnosticResult<()> {
        let s = StreamA::new();
        assert_eq!(s.record_count(), 0);
        Ok(())
    }

    #[test]
    fn with_capacity_has_zero_records() -> DiagnosticResult<()> {
        let s = StreamA::with_capacity(500);
        assert_eq!(s.record_count(), 0);
        Ok(())
    }

    #[test]
    fn record_all_safety_states() -> DiagnosticResult<()> {
        let mut s = StreamA::with_capacity(10);
        let states = [
            SafetyStateSimple::SafeTorque,
            SafetyStateSimple::HighTorqueChallenge,
            SafetyStateSimple::AwaitingPhysicalAck,
            SafetyStateSimple::HighTorqueActive,
            SafetyStateSimple::Faulted {
                fault_type: "test".to_string(),
            },
        ];
        for (i, state) in states.into_iter().enumerate() {
            s.record_frame(
                FrameData {
                    seq: i as u16,
                    ..Default::default()
                },
                &[],
                state,
                0,
            )?;
        }
        assert_eq!(s.record_count(), 5);
        Ok(())
    }

    #[test]
    fn get_data_clears_records() -> DiagnosticResult<()> {
        let mut s = StreamA::new();
        s.record_frame(
            FrameData::default(),
            &[1.0, 2.0],
            SafetyStateSimple::SafeTorque,
            50,
        )?;
        assert_eq!(s.record_count(), 1);
        let data = s.get_data()?;
        assert!(!data.is_empty());
        assert_eq!(s.record_count(), 0);
        Ok(())
    }

    #[test]
    fn get_data_empty_stream_returns_empty() -> DiagnosticResult<()> {
        let mut s = StreamA::new();
        let data = s.get_data()?;
        assert!(data.is_empty());
        Ok(())
    }

    #[test]
    fn reset_clears_everything() -> DiagnosticResult<()> {
        let mut s = StreamA::new();
        s.record_frame(FrameData::default(), &[], SafetyStateSimple::SafeTorque, 0)?;
        s.record_frame(FrameData::default(), &[], SafetyStateSimple::SafeTorque, 0)?;
        assert_eq!(s.record_count(), 2);
        s.reset();
        assert_eq!(s.record_count(), 0);
        Ok(())
    }

    #[test]
    fn record_with_many_node_outputs() -> DiagnosticResult<()> {
        let mut s = StreamA::new();
        let outputs: Vec<f32> = (0..100).map(|i| i as f32 * 0.01).collect();
        s.record_frame(
            FrameData::default(),
            &outputs,
            SafetyStateSimple::SafeTorque,
            200,
        )?;

        let data = s.get_data()?;
        let mut reader = StreamReader::new(data);
        let record = reader
            .read_stream_a_record()?
            .ok_or(DiagnosticError::Validation("expected record".into()))?;
        assert_eq!(record.node_outputs.len(), 100);
        assert!((record.node_outputs[50] - 0.5).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn roundtrip_preserves_processing_time() -> DiagnosticResult<()> {
        let mut s = StreamA::with_capacity(1);
        s.record_frame(
            FrameData::default(),
            &[],
            SafetyStateSimple::SafeTorque,
            12345,
        )?;
        let data = s.get_data()?;
        let mut reader = StreamReader::new(data);
        let record = reader
            .read_stream_a_record()?
            .ok_or(DiagnosticError::Validation("missing".into()))?;
        assert_eq!(record.processing_time_us, 12345);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Stream C – deep tests
// ═══════════════════════════════════════════════════════════════════════════

mod stream_c_deep {
    use super::*;

    #[test]
    fn new_is_empty() -> DiagnosticResult<()> {
        let s = StreamC::new();
        assert_eq!(s.record_count(), 0);
        Ok(())
    }

    #[test]
    fn record_multiple_events() -> DiagnosticResult<()> {
        let mut s = StreamC::new();
        for i in 0..10 {
            s.record_health_event(HealthEventData {
                timestamp_ns: i * 1000,
                device_id: format!("dev-{i}"),
                event_type: "TestEvent".to_string(),
                context: serde_json::json!({"index": i}),
            })?;
        }
        assert_eq!(s.record_count(), 10);
        let data = s.get_data()?;
        assert!(!data.is_empty());
        assert_eq!(s.record_count(), 0);
        Ok(())
    }

    #[test]
    fn empty_get_data() -> DiagnosticResult<()> {
        let mut s = StreamC::new();
        let data = s.get_data()?;
        assert!(data.is_empty());
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// StreamReader – edge cases
// ═══════════════════════════════════════════════════════════════════════════

mod stream_reader_deep {
    use super::*;

    #[test]
    fn empty_reader_position_and_end() -> DiagnosticResult<()> {
        let reader = StreamReader::new(vec![]);
        assert_eq!(reader.position(), 0);
        assert!(reader.is_at_end());
        Ok(())
    }

    #[test]
    fn empty_reader_returns_none_for_all_stream_types() -> DiagnosticResult<()> {
        let mut reader = StreamReader::new(vec![]);
        assert!(reader.read_stream_a_record()?.is_none());
        reader.reset();
        assert!(reader.read_stream_b_record()?.is_none());
        reader.reset();
        assert!(reader.read_stream_c_record()?.is_none());
        Ok(())
    }

    #[test]
    fn truncated_length_prefix_errors() {
        // Only 2 bytes when 4 are needed for the length prefix
        let mut reader = StreamReader::new(vec![0xFF, 0x00]);
        let result = reader.read_stream_a_record();
        assert!(result.is_err());
    }

    #[test]
    fn truncated_record_body_errors() {
        // Length prefix says 100 bytes, but only 2 follow
        let mut data = 100u32.to_le_bytes().to_vec();
        data.extend_from_slice(&[0u8; 2]);
        let mut reader = StreamReader::new(data);
        let result = reader.read_stream_a_record();
        assert!(result.is_err());
    }

    #[test]
    fn reset_allows_re_reading() -> DiagnosticResult<()> {
        let mut stream = StreamA::with_capacity(5);
        stream.record_frame(
            FrameData {
                seq: 99,
                ..Default::default()
            },
            &[],
            SafetyStateSimple::SafeTorque,
            0,
        )?;
        let data = stream.get_data()?;

        let mut reader = StreamReader::new(data);
        let first = reader
            .read_stream_a_record()?
            .ok_or(DiagnosticError::Validation("first read".into()))?;
        assert_eq!(first.frame.seq, 99);
        assert!(reader.is_at_end());

        reader.reset();
        assert_eq!(reader.position(), 0);
        assert!(!reader.is_at_end());

        let second = reader
            .read_stream_a_record()?
            .ok_or(DiagnosticError::Validation("second read".into()))?;
        assert_eq!(second.frame.seq, 99);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ReplayConfig
// ═══════════════════════════════════════════════════════════════════════════

mod replay_config_deep {
    use super::*;

    #[test]
    fn default_values() -> DiagnosticResult<()> {
        let cfg = ReplayConfig::default();
        assert_eq!(cfg.deterministic_seed, 0x12345678);
        assert!((cfg.fp_tolerance - 1e-6).abs() < f64::EPSILON);
        assert!(!cfg.strict_timing);
        assert_eq!(cfg.max_duration_s, 600);
        assert!(cfg.validate_outputs);
        Ok(())
    }

    #[test]
    fn custom_config() -> DiagnosticResult<()> {
        let cfg = ReplayConfig {
            deterministic_seed: 42,
            fp_tolerance: 1e-3,
            strict_timing: true,
            max_duration_s: 120,
            validate_outputs: false,
        };
        assert_eq!(cfg.deterministic_seed, 42);
        assert!(!cfg.validate_outputs);
        assert!(cfg.strict_timing);
        Ok(())
    }

    #[test]
    fn clone() -> DiagnosticResult<()> {
        let cfg = ReplayConfig {
            deterministic_seed: 99,
            ..Default::default()
        };
        let c = cfg.clone();
        assert_eq!(c.deterministic_seed, 99);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ReplayResult serialization
// ═══════════════════════════════════════════════════════════════════════════

mod replay_result_deep {
    use super::*;
    use std::time::Duration;

    #[test]
    fn serde_roundtrip() -> DiagnosticResult<()> {
        let result = ReplayResult {
            frames_replayed: 1000,
            frames_matched: 998,
            max_deviation: 1e-5,
            avg_deviation: 1e-7,
            replay_duration: Duration::from_millis(500),
            original_duration: Duration::from_secs(10),
            validation_errors: vec!["frame 42 mismatch".to_string()],
            success: false,
        };
        let json = serde_json::to_string(&result)
            .map_err(|e| DiagnosticError::Serialization(e.to_string()))?;
        let restored: ReplayResult = serde_json::from_str(&json)
            .map_err(|e| DiagnosticError::Deserialization(e.to_string()))?;
        assert_eq!(restored.frames_replayed, 1000);
        assert_eq!(restored.frames_matched, 998);
        assert!(!restored.success);
        assert_eq!(restored.validation_errors.len(), 1);
        Ok(())
    }

    #[test]
    fn successful_result() -> DiagnosticResult<()> {
        let result = ReplayResult {
            frames_replayed: 500,
            frames_matched: 500,
            max_deviation: 0.0,
            avg_deviation: 0.0,
            replay_duration: Duration::from_millis(100),
            original_duration: Duration::from_secs(5),
            validation_errors: vec![],
            success: true,
        };
        assert!(result.success);
        assert!(result.validation_errors.is_empty());
        assert_eq!(result.frames_replayed, result.frames_matched);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ReplayStatistics
// ═══════════════════════════════════════════════════════════════════════════

mod replay_statistics_deep {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn serde_roundtrip() -> DiagnosticResult<()> {
        let mut histogram = HashMap::new();
        histogram.insert("< 1e-9".to_string(), 450);
        histogram.insert("1e-9 to 1e-6".to_string(), 50);

        let stats = ReplayStatistics {
            total_frames: 500,
            match_rate: 0.99,
            deviation_histogram: histogram,
            timing_accuracy: 0.998,
            memory_usage_mb: 12.5,
        };
        let json = serde_json::to_string(&stats)
            .map_err(|e| DiagnosticError::Serialization(e.to_string()))?;
        let restored: ReplayStatistics = serde_json::from_str(&json)
            .map_err(|e| DiagnosticError::Deserialization(e.to_string()))?;
        assert_eq!(restored.total_frames, 500);
        assert!((restored.match_rate - 0.99).abs() < f64::EPSILON);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SupportBundleConfig
// ═══════════════════════════════════════════════════════════════════════════

mod support_bundle_config_deep {
    use super::*;

    #[test]
    fn default_values() -> DiagnosticResult<()> {
        let cfg = SupportBundleConfig::default();
        assert!(cfg.include_logs);
        assert!(cfg.include_profiles);
        assert!(cfg.include_system_info);
        assert!(cfg.include_recent_recordings);
        assert_eq!(cfg.max_bundle_size_mb, 25);
        Ok(())
    }

    #[test]
    fn custom_config() -> DiagnosticResult<()> {
        let cfg = SupportBundleConfig {
            include_logs: false,
            include_profiles: false,
            include_system_info: true,
            include_recent_recordings: false,
            max_bundle_size_mb: 5,
        };
        assert!(!cfg.include_logs);
        assert!(!cfg.include_profiles);
        assert!(cfg.include_system_info);
        assert_eq!(cfg.max_bundle_size_mb, 5);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SupportBundle – management and health checks
// ═══════════════════════════════════════════════════════════════════════════

mod support_bundle_deep {
    use super::*;

    #[test]
    fn new_bundle_is_empty() -> DiagnosticResult<()> {
        let bundle = SupportBundle::new(SupportBundleConfig::default());
        assert!((bundle.estimated_size_mb()).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    fn add_system_info_increases_size() -> DiagnosticResult<()> {
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        let before = bundle.estimated_size_mb();
        bundle.add_system_info()?;
        let after = bundle.estimated_size_mb();
        assert!(after > before);
        Ok(())
    }

    #[test]
    fn add_system_info_disabled_is_noop() -> DiagnosticResult<()> {
        let cfg = SupportBundleConfig {
            include_system_info: false,
            ..Default::default()
        };
        let mut bundle = SupportBundle::new(cfg);
        let before = bundle.estimated_size_mb();
        bundle.add_system_info()?;
        assert!((bundle.estimated_size_mb() - before).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    fn add_health_events_increases_size() -> DiagnosticResult<()> {
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        let events = vec![HealthEventData {
            timestamp_ns: 0,
            device_id: "d".to_string(),
            event_type: "E".to_string(),
            context: serde_json::json!({"data": "test"}),
        }];
        let before = bundle.estimated_size_mb();
        bundle.add_health_events(&events)?;
        assert!(bundle.estimated_size_mb() > before);
        Ok(())
    }

    #[test]
    fn add_events_exceeding_size_limit_returns_error() -> DiagnosticResult<()> {
        let cfg = SupportBundleConfig {
            max_bundle_size_mb: 1,
            ..Default::default()
        };
        let mut bundle = SupportBundle::new(cfg);
        let large_ctx = serde_json::json!({"big": "x".repeat(2 * 1024 * 1024)});
        let events = vec![HealthEventData {
            timestamp_ns: 0,
            device_id: "d".to_string(),
            event_type: "E".to_string(),
            context: large_ctx,
        }];
        let result = bundle.add_health_events(&events);
        assert!(result.is_err());
        assert!(matches!(result, Err(DiagnosticError::SizeLimit(_))));
        Ok(())
    }

    #[test]
    fn add_log_files_with_disabled_logs_is_noop() -> DiagnosticResult<()> {
        let cfg = SupportBundleConfig {
            include_logs: false,
            ..Default::default()
        };
        let mut bundle = SupportBundle::new(cfg);
        let td = tempfile::tempdir().map_err(|e| DiagnosticError::Io(e.to_string()))?;
        std::fs::write(td.path().join("test.log"), "content")
            .map_err(|e| DiagnosticError::Io(e.to_string()))?;
        bundle.add_log_files(td.path())?;
        assert!((bundle.estimated_size_mb()).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    fn add_log_files_from_nonexistent_dir() -> DiagnosticResult<()> {
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        let result = bundle.add_log_files(std::path::Path::new("/nonexistent/path/logs"));
        assert!(result.is_ok()); // Should succeed with empty list
        Ok(())
    }

    #[test]
    fn add_profile_files_with_disabled_profiles_is_noop() -> DiagnosticResult<()> {
        let cfg = SupportBundleConfig {
            include_profiles: false,
            ..Default::default()
        };
        let mut bundle = SupportBundle::new(cfg);
        let td = tempfile::tempdir().map_err(|e| DiagnosticError::Io(e.to_string()))?;
        std::fs::write(td.path().join("prof.json"), "{}")
            .map_err(|e| DiagnosticError::Io(e.to_string()))?;
        bundle.add_profile_files(td.path())?;
        assert!((bundle.estimated_size_mb()).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    fn add_recent_recordings_with_disabled_is_noop() -> DiagnosticResult<()> {
        let cfg = SupportBundleConfig {
            include_recent_recordings: false,
            ..Default::default()
        };
        let mut bundle = SupportBundle::new(cfg);
        let td = tempfile::tempdir().map_err(|e| DiagnosticError::Io(e.to_string()))?;
        std::fs::write(td.path().join("rec.wbb"), b"data")
            .map_err(|e| DiagnosticError::Io(e.to_string()))?;
        bundle.add_recent_recordings(td.path())?;
        assert!((bundle.estimated_size_mb()).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    fn generate_produces_zip_with_manifest() -> DiagnosticResult<()> {
        let td = tempfile::tempdir().map_err(|e| DiagnosticError::Io(e.to_string()))?;
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        bundle.add_system_info()?;
        let zip_path = td.path().join("bundle.zip");
        bundle.generate(&zip_path)?;
        assert!(zip_path.exists());
        let meta = std::fs::metadata(&zip_path).map_err(|e| DiagnosticError::Io(e.to_string()))?;
        assert!(meta.len() > 0);
        Ok(())
    }

    #[test]
    fn generate_with_health_events() -> DiagnosticResult<()> {
        let td = tempfile::tempdir().map_err(|e| DiagnosticError::Io(e.to_string()))?;
        let mut bundle = SupportBundle::new(SupportBundleConfig::default());
        let events = vec![
            HealthEventData {
                timestamp_ns: 0,
                device_id: "dev1".to_string(),
                event_type: "Connected".to_string(),
                context: serde_json::json!({}),
            },
            HealthEventData {
                timestamp_ns: 1000,
                device_id: "dev1".to_string(),
                event_type: "Disconnected".to_string(),
                context: serde_json::json!({"reason": "timeout"}),
            },
        ];
        bundle.add_health_events(&events)?;
        let zip_path = td.path().join("health_bundle.zip");
        bundle.generate(&zip_path)?;
        assert!(zip_path.exists());
        Ok(())
    }

    #[test]
    fn generate_minimal_bundle_no_system_info() -> DiagnosticResult<()> {
        let cfg = SupportBundleConfig {
            include_system_info: false,
            include_logs: false,
            include_profiles: false,
            include_recent_recordings: false,
            max_bundle_size_mb: 1,
        };
        let td = tempfile::tempdir().map_err(|e| DiagnosticError::Io(e.to_string()))?;
        let bundle = SupportBundle::new(cfg);
        let zip_path = td.path().join("minimal.zip");
        bundle.generate(&zip_path)?;
        assert!(zip_path.exists());
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Full recording-to-replay workflow
// ═══════════════════════════════════════════════════════════════════════════

mod full_workflow {
    use super::*;
    use openracing_diagnostic::BlackboxReplay;

    fn create_recording(frame_count: usize) -> DiagnosticResult<(std::path::PathBuf, TempDir)> {
        let td = tempfile::tempdir().map_err(|e| DiagnosticError::Io(e.to_string()))?;
        let cfg = BlackboxConfig {
            enable_stream_a: true,
            enable_stream_b: false,
            enable_stream_c: false,
            ..BlackboxConfig::new("test-dev", td.path())
        };
        let mut recorder = BlackboxRecorder::new(cfg)?;
        for i in 0..frame_count {
            let frame = FrameData {
                ffb_in: (i as f32 * 0.01).sin(),
                torque_out: (i as f32 * 0.01).cos() * 0.5,
                wheel_speed: 10.0 + i as f32 * 0.1,
                hands_off: i % 100 == 0,
                ts_mono_ns: i as u64 * 1_000_000,
                seq: (i % 65536) as u16,
            };
            recorder.record_frame(frame, &[0.1, 0.2], SafetyStateSimple::SafeTorque, 80)?;
        }
        let path = recorder.finalize()?;
        Ok((path, td))
    }

    #[test]
    fn record_and_replay_small() -> DiagnosticResult<()> {
        let (path, _td) = create_recording(10)?;
        let cfg = ReplayConfig::default();
        let mut replay = BlackboxReplay::load_from_file(&path, cfg)?;
        let result = replay.execute_replay()?;
        assert!(result.frames_replayed > 0);
        assert!(result.success);
        Ok(())
    }

    #[test]
    fn record_and_replay_medium() -> DiagnosticResult<()> {
        let (path, _td) = create_recording(100)?;
        let cfg = ReplayConfig {
            validate_outputs: true,
            fp_tolerance: 1e-3,
            ..Default::default()
        };
        let mut replay = BlackboxReplay::load_from_file(&path, cfg)?;
        let result = replay.execute_replay()?;
        assert_eq!(result.frames_replayed, 100);
        Ok(())
    }

    #[test]
    fn deterministic_replay_consistency() -> DiagnosticResult<()> {
        let (path, _td) = create_recording(50)?;
        let cfg = ReplayConfig {
            deterministic_seed: 42,
            ..Default::default()
        };

        let mut r1 = BlackboxReplay::load_from_file(&path, cfg.clone())?;
        let res1 = r1.execute_replay()?;

        let mut r2 = BlackboxReplay::load_from_file(&path, cfg)?;
        let res2 = r2.execute_replay()?;

        assert_eq!(res1.frames_replayed, res2.frames_replayed);
        assert_eq!(res1.frames_matched, res2.frames_matched);
        assert!((res1.max_deviation - res2.max_deviation).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    fn replay_generates_statistics() -> DiagnosticResult<()> {
        let (path, _td) = create_recording(20)?;
        let mut replay = BlackboxReplay::load_from_file(&path, ReplayConfig::default())?;
        let _result = replay.execute_replay()?;
        let stats = replay.generate_statistics();
        assert!(stats.total_frames > 0);
        assert!(stats.match_rate >= 0.0);
        assert!(stats.match_rate <= 1.0);
        Ok(())
    }

    #[test]
    fn replay_header_and_footer_accessible() -> DiagnosticResult<()> {
        let (path, _td) = create_recording(5)?;
        let replay = BlackboxReplay::load_from_file(&path, ReplayConfig::default())?;
        let header = replay.header();
        assert_eq!(&header.magic, WBB_MAGIC);
        assert_eq!(header.device_id, "test-dev");
        let footer = replay.footer();
        assert_eq!(&footer.footer_magic, WBB_FOOTER_MAGIC);
        assert_eq!(footer.total_frames, 5);
        Ok(())
    }

    #[test]
    fn replay_stream_a_data_accessible() -> DiagnosticResult<()> {
        let (path, _td) = create_recording(3)?;
        let replay = BlackboxReplay::load_from_file(&path, ReplayConfig::default())?;
        let data = replay.stream_a_data();
        assert_eq!(data.len(), 3);
        Ok(())
    }

    #[test]
    fn replay_frame_comparisons() -> DiagnosticResult<()> {
        let (path, _td) = create_recording(10)?;
        let cfg = ReplayConfig {
            validate_outputs: true,
            ..Default::default()
        };
        let mut replay = BlackboxReplay::load_from_file(&path, cfg)?;
        let _result = replay.execute_replay()?;
        let comparisons = replay.get_frame_comparisons();
        assert_eq!(comparisons.len(), 10);
        for c in comparisons {
            assert!(c.within_tolerance);
        }
        Ok(())
    }

    #[test]
    fn replay_validation_errors_empty_on_success() -> DiagnosticResult<()> {
        let (path, _td) = create_recording(5)?;
        let mut replay = BlackboxReplay::load_from_file(&path, ReplayConfig::default())?;
        let _result = replay.execute_replay()?;
        assert!(replay.get_validation_errors().is_empty());
        Ok(())
    }

    #[test]
    fn replay_with_output_validation_disabled() -> DiagnosticResult<()> {
        let (path, _td) = create_recording(10)?;
        let cfg = ReplayConfig {
            validate_outputs: false,
            ..Default::default()
        };
        let mut replay = BlackboxReplay::load_from_file(&path, cfg)?;
        let result = replay.execute_replay()?;
        assert_eq!(result.frames_replayed, 10);
        // No comparisons recorded when validation disabled
        assert!(replay.get_frame_comparisons().is_empty());
        Ok(())
    }

    #[test]
    fn load_nonexistent_file_returns_error() {
        let result = BlackboxReplay::load_from_file(
            std::path::Path::new("/no/such/file.wbb"),
            ReplayConfig::default(),
        );
        assert!(result.is_err());
    }
}
