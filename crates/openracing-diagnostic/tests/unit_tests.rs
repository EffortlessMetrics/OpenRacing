//! Unit tests for openracing-diagnostic error types, format, and streams.
//!
//! Covers: DiagnosticError variants, Display formatting, From impls,
//! WbbHeader/WbbFooter validation, StreamType, and basic stream operations.

use openracing_diagnostic::{
    DiagnosticError, DiagnosticResult, FrameData, HealthEventData, SafetyStateSimple,
    StreamA, StreamC, StreamReader, TelemetryData,
    WbbHeader, WbbFooter, IndexEntry, StreamType,
    format::{WBB_MAGIC, WBB_FOOTER_MAGIC, WBB_VERSION, STREAM_A_ID, STREAM_B_ID, STREAM_C_ID},
};

// ---------------------------------------------------------------------------
// DiagnosticError variant construction & Display
// ---------------------------------------------------------------------------

mod diagnostic_error_display {
    use super::*;

    #[test]
    fn all_string_variants_display() -> DiagnosticResult<()> {
        let variants: Vec<DiagnosticError> = vec![
            DiagnosticError::Recording("rec err".into()),
            DiagnosticError::Replay("replay err".into()),
            DiagnosticError::Format("fmt err".into()),
            DiagnosticError::Io("io err".into()),
            DiagnosticError::Serialization("ser err".into()),
            DiagnosticError::Deserialization("de err".into()),
            DiagnosticError::Compression("comp err".into()),
            DiagnosticError::SizeLimit("too big".into()),
            DiagnosticError::Configuration("bad cfg".into()),
            DiagnosticError::Validation("invalid".into()),
            DiagnosticError::UnsupportedVersion(99),
        ];
        for v in &variants {
            let msg = v.to_string();
            assert!(
                !msg.is_empty(),
                "DiagnosticError::{v:?} display must not be empty"
            );
        }
        Ok(())
    }

    #[test]
    fn crc_mismatch_display() -> DiagnosticResult<()> {
        let err = DiagnosticError::CrcMismatch {
            expected: 0xDEAD_BEEF,
            actual: 0xCAFE_BABE,
        };
        let msg = err.to_string();
        assert!(msg.contains("CRC mismatch"));
        Ok(())
    }

    #[test]
    fn invalid_magic_display() -> DiagnosticResult<()> {
        let err = DiagnosticError::InvalidMagic {
            expected: *b"WBB1",
            actual: *b"XXXX",
        };
        let msg = err.to_string();
        assert!(msg.contains("Invalid magic"));
        Ok(())
    }

    #[test]
    fn unsupported_version_display() -> DiagnosticResult<()> {
        let err = DiagnosticError::UnsupportedVersion(42);
        let msg = err.to_string();
        assert!(msg.contains("42"));
        Ok(())
    }

    #[test]
    fn display_contains_inner_message() -> DiagnosticResult<()> {
        let cases = [
            (DiagnosticError::Recording("buffer full".into()), "buffer full"),
            (DiagnosticError::Replay("frame mismatch".into()), "frame mismatch"),
            (DiagnosticError::Format("bad header".into()), "bad header"),
            (DiagnosticError::Io("disk full".into()), "disk full"),
            (DiagnosticError::Serialization("encode fail".into()), "encode fail"),
            (DiagnosticError::Deserialization("decode fail".into()), "decode fail"),
            (DiagnosticError::Compression("zlib error".into()), "zlib error"),
            (DiagnosticError::SizeLimit("exceeded 10MB".into()), "exceeded 10MB"),
            (DiagnosticError::Configuration("missing key".into()), "missing key"),
            (DiagnosticError::Validation("out of range".into()), "out of range"),
        ];
        for (err, expected_substr) in &cases {
            assert!(
                err.to_string().contains(expected_substr),
                "Expected '{expected_substr}' in '{}'",
                err
            );
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// DiagnosticError From impls
// ---------------------------------------------------------------------------

mod diagnostic_error_from {
    use super::*;

    #[test]
    fn from_io_error() -> DiagnosticResult<()> {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "no such file");
        let diag: DiagnosticError = io_err.into();
        assert!(matches!(diag, DiagnosticError::Io(_)));
        assert!(diag.to_string().contains("no such file"));
        Ok(())
    }

    #[test]
    fn from_serde_json_error() -> DiagnosticResult<()> {
        // Produce a real serde_json error by parsing invalid JSON
        let result: std::result::Result<serde_json::Value, _> = serde_json::from_str("{invalid}");
        if let Err(json_err) = result {
            let diag: DiagnosticError = json_err.into();
            assert!(matches!(diag, DiagnosticError::Serialization(_)));
        }
        Ok(())
    }

    #[test]
    fn diagnostic_error_is_std_error() -> DiagnosticResult<()> {
        let err = DiagnosticError::Recording("test".into());
        let _: &dyn std::error::Error = &err;
        Ok(())
    }

    #[test]
    fn diagnostic_error_is_clone() -> DiagnosticResult<()> {
        let err = DiagnosticError::CrcMismatch {
            expected: 1,
            actual: 2,
        };
        let cloned = err.clone();
        assert_eq!(err.to_string(), cloned.to_string());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// WbbHeader tests
// ---------------------------------------------------------------------------

mod wbb_header_tests {
    use super::*;

    #[test]
    fn new_header_has_correct_magic_and_version() -> DiagnosticResult<()> {
        let header = WbbHeader::new("test-dev", 1, 0x07, 6);
        assert_eq!(&header.magic, WBB_MAGIC);
        assert_eq!(header.version, WBB_VERSION);
        assert_eq!(header.device_id, "test-dev");
        assert_eq!(header.ffb_mode, 1);
        assert_eq!(header.stream_flags, 0x07);
        assert_eq!(header.compression_level, 6);
        Ok(())
    }

    #[test]
    fn stream_flag_checks() -> DiagnosticResult<()> {
        let header = WbbHeader::new("d", 0, STREAM_A_ID | STREAM_C_ID, 0);
        assert!(header.has_stream_a());
        assert!(!header.has_stream_b());
        assert!(header.has_stream_c());

        let header = WbbHeader::new("d", 0, STREAM_B_ID, 0);
        assert!(!header.has_stream_a());
        assert!(header.has_stream_b());
        assert!(!header.has_stream_c());

        let header = WbbHeader::new("d", 0, 0, 0);
        assert!(!header.has_stream_a());
        assert!(!header.has_stream_b());
        assert!(!header.has_stream_c());

        Ok(())
    }

    #[test]
    fn validate_valid_header() -> DiagnosticResult<()> {
        let header = WbbHeader::new("d", 0, 0x07, 6);
        header.validate()?;
        Ok(())
    }

    #[test]
    fn validate_bad_magic() -> DiagnosticResult<()> {
        let mut header = WbbHeader::new("d", 0, 0, 0);
        header.magic = *b"XXXX";
        let result = header.validate();
        assert!(result.is_err());
        if let Err(DiagnosticError::InvalidMagic { expected, actual }) = result {
            assert_eq!(&expected, WBB_MAGIC);
            assert_eq!(&actual, b"XXXX");
        }
        Ok(())
    }

    #[test]
    fn validate_unsupported_version() -> DiagnosticResult<()> {
        let mut header = WbbHeader::new("d", 0, 0, 0);
        header.version = 99;
        let result = header.validate();
        assert!(result.is_err());
        assert!(matches!(result, Err(DiagnosticError::UnsupportedVersion(99))));
        Ok(())
    }

    #[test]
    fn validate_bad_compression() -> DiagnosticResult<()> {
        let mut header = WbbHeader::new("d", 0, 0, 10);
        header.compression_level = 10;
        let result = header.validate();
        assert!(result.is_err());
        assert!(matches!(result, Err(DiagnosticError::Configuration(_))));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// WbbFooter tests
// ---------------------------------------------------------------------------

mod wbb_footer_tests {
    use super::*;

    #[test]
    fn new_footer_has_correct_magic() -> DiagnosticResult<()> {
        let footer = WbbFooter::new(1000, 100);
        assert_eq!(&footer.footer_magic, WBB_FOOTER_MAGIC);
        assert_eq!(footer.duration_ms, 1000);
        assert_eq!(footer.total_frames, 100);
        assert_eq!(footer.index_offset, 0);
        assert_eq!(footer.index_count, 0);
        assert_eq!(footer.file_crc32c, 0);
        Ok(())
    }

    #[test]
    fn validate_valid_footer() -> DiagnosticResult<()> {
        let footer = WbbFooter::new(500, 50);
        footer.validate()?;
        Ok(())
    }

    #[test]
    fn validate_bad_footer_magic() -> DiagnosticResult<()> {
        let mut footer = WbbFooter::new(0, 0);
        footer.footer_magic = *b"XXXX";
        let result = footer.validate();
        assert!(result.is_err());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// IndexEntry tests
// ---------------------------------------------------------------------------

mod index_entry_tests {
    use super::*;

    #[test]
    fn new_index_entry() -> DiagnosticResult<()> {
        let entry = IndexEntry::new(200, 50);
        assert_eq!(entry.timestamp_ms, 200);
        assert_eq!(entry.frame_count, 50);
        assert_eq!(entry.stream_a_offset, 0);
        assert_eq!(entry.stream_b_offset, 0);
        assert_eq!(entry.stream_c_offset, 0);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// StreamType tests
// ---------------------------------------------------------------------------

mod stream_type_tests {
    use super::*;

    #[test]
    fn flag_values() -> DiagnosticResult<()> {
        assert_eq!(StreamType::A.flag(), STREAM_A_ID);
        assert_eq!(StreamType::B.flag(), STREAM_B_ID);
        assert_eq!(StreamType::C.flag(), STREAM_C_ID);
        Ok(())
    }

    #[test]
    fn stream_type_equality() -> DiagnosticResult<()> {
        assert_eq!(StreamType::A, StreamType::A);
        assert_ne!(StreamType::A, StreamType::B);
        assert_ne!(StreamType::B, StreamType::C);
        Ok(())
    }

    #[test]
    fn stream_type_copy() -> DiagnosticResult<()> {
        let st = StreamType::A;
        let copy = st;
        assert_eq!(st, copy);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// FrameData default
// ---------------------------------------------------------------------------

mod frame_data_tests {
    use super::*;

    #[test]
    fn default_is_zeroed() -> DiagnosticResult<()> {
        let fd = FrameData::default();
        assert!((fd.ffb_in - 0.0).abs() < f32::EPSILON);
        assert!((fd.torque_out - 0.0).abs() < f32::EPSILON);
        assert!((fd.wheel_speed - 0.0).abs() < f32::EPSILON);
        assert!(!fd.hands_off);
        assert_eq!(fd.ts_mono_ns, 0);
        assert_eq!(fd.seq, 0);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// SafetyStateSimple
// ---------------------------------------------------------------------------

mod safety_state_tests {
    use super::*;

    #[test]
    fn default_is_safe_torque() -> DiagnosticResult<()> {
        let state = SafetyStateSimple::default();
        assert!(matches!(state, SafetyStateSimple::SafeTorque));
        Ok(())
    }

    #[test]
    fn faulted_variant() -> DiagnosticResult<()> {
        let state = SafetyStateSimple::Faulted {
            fault_type: "OverTemp".into(),
        };
        if let SafetyStateSimple::Faulted { fault_type } = &state {
            assert_eq!(fault_type, "OverTemp");
        } else {
            return Err(DiagnosticError::Validation("Expected Faulted".into()));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// TelemetryData default
// ---------------------------------------------------------------------------

mod telemetry_data_tests {
    use super::*;

    #[test]
    fn default_values() -> DiagnosticResult<()> {
        let td = TelemetryData::default();
        assert!((td.ffb_scalar - 1.0).abs() < f32::EPSILON);
        assert!((td.rpm - 0.0).abs() < f32::EPSILON);
        assert!((td.speed_ms - 0.0).abs() < f32::EPSILON);
        assert!((td.slip_ratio - 0.0).abs() < f32::EPSILON);
        assert_eq!(td.gear, 0);
        assert!(td.car_id.is_none());
        assert!(td.track_id.is_none());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Stream A write-read round-trip
// ---------------------------------------------------------------------------

mod stream_a_tests {
    use super::*;

    #[test]
    fn record_and_read_roundtrip() -> DiagnosticResult<()> {
        let mut stream = StreamA::with_capacity(10);
        assert_eq!(stream.record_count(), 0);

        let frame = FrameData {
            ffb_in: 0.75,
            torque_out: 0.5,
            wheel_speed: 15.0,
            hands_off: false,
            ts_mono_ns: 1_000_000,
            seq: 42,
        };

        stream.record_frame(
            frame,
            &[0.1, 0.2],
            SafetyStateSimple::SafeTorque,
            150,
        )?;
        assert_eq!(stream.record_count(), 1);

        let data = stream.get_data()?;
        assert!(!data.is_empty());
        assert_eq!(stream.record_count(), 0);

        // Read back
        let mut reader = StreamReader::new(data);
        let record = reader.read_stream_a_record()?;
        assert!(record.is_some());
        let record = record.ok_or(DiagnosticError::Validation("expected record".into()))?;
        assert!((record.frame.ffb_in - 0.75).abs() < f32::EPSILON);
        assert_eq!(record.frame.seq, 42);
        assert_eq!(record.node_outputs.len(), 2);
        assert_eq!(record.processing_time_us, 150);
        assert!(reader.is_at_end());

        Ok(())
    }

    #[test]
    fn multiple_records_roundtrip() -> DiagnosticResult<()> {
        let mut stream = StreamA::with_capacity(20);

        for i in 0..10 {
            let frame = FrameData {
                ffb_in: i as f32 * 0.1,
                seq: i as u16,
                ..Default::default()
            };
            stream.record_frame(frame, &[], SafetyStateSimple::SafeTorque, 100)?;
        }
        assert_eq!(stream.record_count(), 10);

        let data = stream.get_data()?;
        let mut reader = StreamReader::new(data);
        let mut count = 0u32;

        while reader.read_stream_a_record()?.is_some() {
            count += 1;
        }
        assert_eq!(count, 10);
        assert!(reader.is_at_end());
        Ok(())
    }

    #[test]
    fn reset_clears_state() -> DiagnosticResult<()> {
        let mut stream = StreamA::new();
        stream.record_frame(
            FrameData::default(),
            &[],
            SafetyStateSimple::SafeTorque,
            0,
        )?;
        assert_eq!(stream.record_count(), 1);
        stream.reset();
        assert_eq!(stream.record_count(), 0);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Stream C health events
// ---------------------------------------------------------------------------

mod stream_c_tests {
    use super::*;

    #[test]
    fn record_health_event_and_serialize() -> DiagnosticResult<()> {
        let mut stream = StreamC::new();

        let event = HealthEventData {
            timestamp_ns: 12345,
            device_id: "wheel-1".into(),
            event_type: "SafetyFault".into(),
            context: serde_json::json!({"code": 42}),
        };

        stream.record_health_event(event)?;
        assert_eq!(stream.record_count(), 1);

        // Verify serialization produces data (bincode + serde_json::Value
        // does not round-trip through decode_from_slice, so we only verify
        // the serialized output is non-empty).
        let data = stream.get_data()?;
        assert!(!data.is_empty());
        assert_eq!(stream.record_count(), 0);
        Ok(())
    }

    #[test]
    fn multiple_health_events() -> DiagnosticResult<()> {
        let mut stream = StreamC::new();

        for i in 0..5 {
            let event = HealthEventData {
                timestamp_ns: i * 1000,
                device_id: format!("device-{i}"),
                event_type: "TestEvent".into(),
                context: serde_json::json!(null),
            };
            stream.record_health_event(event)?;
        }
        assert_eq!(stream.record_count(), 5);

        let data = stream.get_data()?;
        assert!(!data.is_empty());
        assert_eq!(stream.record_count(), 0);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// StreamReader edge cases
// ---------------------------------------------------------------------------

mod stream_reader_tests {
    use super::*;

    #[test]
    fn empty_data_returns_none() -> DiagnosticResult<()> {
        let mut reader = StreamReader::new(Vec::new());
        assert!(reader.is_at_end());
        let record = reader.read_stream_a_record()?;
        assert!(record.is_none());
        Ok(())
    }

    #[test]
    fn truncated_length_returns_error() {
        let mut reader = StreamReader::new(vec![0xFF, 0xFF]); // only 2 bytes, need 4
        let result = reader.read_stream_a_record();
        assert!(result.is_err());
    }

    #[test]
    fn truncated_record_returns_error() {
        // Length prefix says 100 bytes, but only 4 bytes of data follow
        let mut data = Vec::new();
        data.extend_from_slice(&100u32.to_le_bytes());
        data.extend_from_slice(&[0u8; 4]);
        let mut reader = StreamReader::new(data);
        let result = reader.read_stream_a_record();
        assert!(result.is_err());
    }

    #[test]
    fn position_and_reset() -> DiagnosticResult<()> {
        let mut stream = StreamA::with_capacity(5);
        for i in 0..3 {
            stream.record_frame(
                FrameData {
                    seq: i,
                    ..Default::default()
                },
                &[],
                SafetyStateSimple::SafeTorque,
                0,
            )?;
        }
        let data = stream.get_data()?;

        let mut reader = StreamReader::new(data);
        assert_eq!(reader.position(), 0);

        let _ = reader.read_stream_a_record()?;
        assert!(reader.position() > 0);

        reader.reset();
        assert_eq!(reader.position(), 0);
        assert!(!reader.is_at_end());
        Ok(())
    }
}
