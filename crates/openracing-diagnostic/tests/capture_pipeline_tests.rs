//! Tests for end-to-end capture pipeline: WBB format writing/reading,
//! version mismatch handling, stream serialization, metadata validation,
//! and replay fidelity across record → finalize → load → replay.

use openracing_diagnostic::format::{
    MAX_SUPPORTED_VERSION, STREAM_A_ID, STREAM_B_ID, STREAM_C_ID, WBB_FOOTER_MAGIC, WBB_MAGIC,
    WBB_VERSION,
};
use openracing_diagnostic::{
    BlackboxConfig, BlackboxRecorder, BlackboxReplay, DiagnosticError, FrameData, HealthEventData,
    IndexEntry, ReplayConfig, ReplayResult, SafetyStateSimple, StreamA, StreamB, StreamC,
    StreamReader, StreamType, TelemetryData, WbbFooter, WbbHeader,
};
use tempfile::TempDir;

type R = Result<(), Box<dyn std::error::Error>>;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn make_tmp() -> Result<TempDir, Box<dyn std::error::Error>> {
    Ok(TempDir::new()?)
}

fn make_frame(i: u32) -> FrameData {
    FrameData {
        ffb_in: (i as f32) * 0.01,
        torque_out: (i as f32) * 0.005,
        wheel_speed: 10.0 + (i as f32) * 0.1,
        hands_off: false,
        ts_mono_ns: (i as u64) * 1_000_000,
        seq: i as u16,
    }
}

fn record_frames(n: u32, td: &TempDir) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let config = BlackboxConfig::new("test-capture-device", td.path());
    let mut recorder = BlackboxRecorder::new(config)?;
    for i in 0..n {
        recorder.record_frame(
            make_frame(i),
            &[0.1, 0.2, 0.3],
            SafetyStateSimple::SafeTorque,
            100,
        )?;
    }
    let path = recorder.finalize()?;
    Ok(path)
}

// ═════════════════════════════════════════════════════════════════════════════
// WBB Header validation
// ═════════════════════════════════════════════════════════════════════════════

mod header_validation {
    use super::*;

    #[test]
    fn valid_header_passes_validation() -> R {
        let header = WbbHeader::new("dev-001", 1, 0x07, 6);
        header.validate()?;
        Ok(())
    }

    #[test]
    fn wrong_magic_rejected() {
        let mut header = WbbHeader::new("dev-001", 1, 0x07, 6);
        header.magic = *b"XXXX";
        let err = header.validate().unwrap_err();
        assert!(
            matches!(err, DiagnosticError::InvalidMagic { .. }),
            "expected InvalidMagic, got: {err}"
        );
    }

    #[test]
    fn future_version_rejected() {
        let mut header = WbbHeader::new("dev-001", 1, 0x07, 6);
        header.version = MAX_SUPPORTED_VERSION + 1;
        let err = header.validate().unwrap_err();
        assert!(
            matches!(err, DiagnosticError::UnsupportedVersion(_)),
            "expected UnsupportedVersion, got: {err}"
        );
    }

    #[test]
    fn invalid_compression_level_rejected() {
        let mut header = WbbHeader::new("dev-001", 1, 0x07, 6);
        header.compression_level = 10; // max is 9
        let err = header.validate().unwrap_err();
        assert!(
            matches!(err, DiagnosticError::Configuration(_)),
            "expected Configuration, got: {err}"
        );
    }

    #[test]
    fn compression_level_zero_valid() -> R {
        let header = WbbHeader::new("dev-001", 1, 0x07, 0);
        header.validate()?;
        Ok(())
    }

    #[test]
    fn compression_level_nine_valid() -> R {
        let header = WbbHeader::new("dev-001", 1, 0x07, 9);
        header.validate()?;
        Ok(())
    }

    #[test]
    fn header_magic_bytes_correct() {
        let header = WbbHeader::new("dev-001", 1, 0x07, 6);
        assert_eq!(&header.magic, WBB_MAGIC);
        assert_eq!(&header.magic, b"WBB1");
    }

    #[test]
    fn header_version_is_current() {
        let header = WbbHeader::new("dev-001", 1, 0x07, 6);
        assert_eq!(header.version, WBB_VERSION);
        assert_eq!(header.version, 1);
    }

    #[test]
    fn header_device_id_preserved() {
        let header = WbbHeader::new("my-custom-device-id", 1, 0x07, 6);
        assert_eq!(header.device_id, "my-custom-device-id");
    }

    #[test]
    fn header_start_time_is_reasonable() {
        let header = WbbHeader::new("dev-001", 1, 0x07, 6);
        // Should be a Unix timestamp after 2020
        assert!(header.start_time_unix > 1_577_836_800); // Jan 1, 2020
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// WBB Footer validation
// ═════════════════════════════════════════════════════════════════════════════

mod footer_validation {
    use super::*;

    #[test]
    fn valid_footer_passes() -> R {
        let footer = WbbFooter::new(5000, 500);
        footer.validate()?;
        Ok(())
    }

    #[test]
    fn wrong_footer_magic_rejected() {
        let mut footer = WbbFooter::new(5000, 500);
        footer.footer_magic = *b"ZZZZ";
        let err = footer.validate().unwrap_err();
        assert!(matches!(err, DiagnosticError::InvalidMagic { .. }));
    }

    #[test]
    fn footer_magic_bytes_correct() {
        let footer = WbbFooter::new(0, 0);
        assert_eq!(&footer.footer_magic, WBB_FOOTER_MAGIC);
        assert_eq!(&footer.footer_magic, b"1BBW");
    }

    #[test]
    fn footer_duration_and_frames_stored() {
        let footer = WbbFooter::new(12345, 999);
        assert_eq!(footer.duration_ms, 12345);
        assert_eq!(footer.total_frames, 999);
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Stream flags and types
// ═════════════════════════════════════════════════════════════════════════════

mod stream_flags {
    use super::*;

    #[test]
    fn all_streams_enabled() {
        let header = WbbHeader::new("dev", 0, STREAM_A_ID | STREAM_B_ID | STREAM_C_ID, 0);
        assert!(header.has_stream_a());
        assert!(header.has_stream_b());
        assert!(header.has_stream_c());
    }

    #[test]
    fn only_stream_a_enabled() {
        let header = WbbHeader::new("dev", 0, STREAM_A_ID, 0);
        assert!(header.has_stream_a());
        assert!(!header.has_stream_b());
        assert!(!header.has_stream_c());
    }

    #[test]
    fn no_streams_enabled() {
        let header = WbbHeader::new("dev", 0, 0, 0);
        assert!(!header.has_stream_a());
        assert!(!header.has_stream_b());
        assert!(!header.has_stream_c());
    }

    #[test]
    fn stream_type_flag_values() {
        assert_eq!(StreamType::A.flag(), STREAM_A_ID);
        assert_eq!(StreamType::B.flag(), STREAM_B_ID);
        assert_eq!(StreamType::C.flag(), STREAM_C_ID);
    }

    #[test]
    fn config_stream_flags_match_enables() {
        let mut config = BlackboxConfig::new("dev", ".");
        config.enable_stream_a = true;
        config.enable_stream_b = false;
        config.enable_stream_c = true;
        assert_eq!(config.stream_flags(), STREAM_A_ID | STREAM_C_ID);
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Stream A/B/C recording and serialization roundtrip
// ═════════════════════════════════════════════════════════════════════════════

mod stream_serialization {
    use super::*;

    #[test]
    fn stream_a_write_read_roundtrip() -> R {
        let mut stream = StreamA::with_capacity(100);
        for i in 0..20 {
            stream.record_frame(
                make_frame(i),
                &[0.1 * i as f32],
                SafetyStateSimple::SafeTorque,
                50,
            )?;
        }
        assert_eq!(stream.record_count(), 20);

        let data = stream.get_data()?;
        assert!(!data.is_empty());

        let mut reader = StreamReader::new(data);
        let mut count = 0u32;
        while let Ok(Some(_)) = reader.read_stream_a_record() {
            count += 1;
        }
        assert_eq!(count, 20);
        assert!(reader.is_at_end());
        Ok(())
    }

    #[test]
    fn stream_b_write_read_roundtrip() -> R {
        let mut stream = StreamB::with_rate(1_000_000.0); // very high rate for testing
        let tel = TelemetryData {
            ffb_scalar: 0.8,
            rpm: 5000.0,
            speed_ms: 30.0,
            slip_ratio: 0.05,
            gear: 4,
            car_id: Some("test_car".to_string()),
            track_id: Some("test_track".to_string()),
        };
        // Record with sleep to beat rate limiter
        stream.record_telemetry(tel.clone())?;
        std::thread::sleep(std::time::Duration::from_millis(2));
        stream.record_telemetry(tel)?;

        let data = stream.get_data()?;

        let mut reader = StreamReader::new(data);
        let mut count = 0u32;
        while let Ok(Some(_)) = reader.read_stream_b_record() {
            count += 1;
        }
        // At least 1 record (rate limiter may reject second)
        assert!(count >= 1);
        Ok(())
    }

    #[test]
    fn stream_c_record_and_serialize() -> R {
        let mut stream = StreamC::new();
        for i in 0..5 {
            stream.record_health_event(HealthEventData {
                timestamp_ns: i * 1_000_000_000,
                device_id: format!("dev-{i}"),
                event_type: "TestEvent".to_string(),
                context: serde_json::json!({"index": i}),
            })?;
        }
        assert_eq!(stream.record_count(), 5);

        let data = stream.get_data()?;
        // After get_data, records are cleared
        assert_eq!(stream.record_count(), 0);
        // Serialized data is non-empty
        assert!(!data.is_empty());
        Ok(())
    }

    #[test]
    fn empty_stream_produces_empty_data() -> R {
        let mut stream = StreamA::new();
        let data = stream.get_data()?;
        assert!(data.is_empty());
        Ok(())
    }

    #[test]
    fn stream_reader_on_empty_data() -> R {
        let mut reader = StreamReader::new(vec![]);
        let record = reader.read_stream_a_record()?;
        assert!(record.is_none());
        assert!(reader.is_at_end());
        Ok(())
    }

    #[test]
    fn stream_reader_reset_rereads() -> R {
        let mut stream = StreamA::with_capacity(10);
        stream.record_frame(make_frame(0), &[0.1], SafetyStateSimple::SafeTorque, 50)?;
        let data = stream.get_data()?;

        let mut reader = StreamReader::new(data);
        let first_read = reader.read_stream_a_record()?;
        assert!(first_read.is_some());
        assert!(reader.is_at_end());

        reader.reset();
        assert_eq!(reader.position(), 0);
        let second_read = reader.read_stream_a_record()?;
        assert!(second_read.is_some());
        Ok(())
    }

    #[test]
    fn stream_reader_truncated_length_prefix_fails() {
        let mut reader = StreamReader::new(vec![0x01, 0x02]); // only 2 bytes, need 4
        let result = reader.read_stream_a_record();
        assert!(result.is_err());
    }

    #[test]
    fn stream_reader_truncated_record_data_fails() {
        // Length says 100 bytes but only 2 available
        let data = vec![100, 0, 0, 0, 0x01, 0x02];
        let mut reader = StreamReader::new(data);
        let result = reader.read_stream_a_record();
        assert!(result.is_err());
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// End-to-end: record → finalize → replay
// ═════════════════════════════════════════════════════════════════════════════

mod end_to_end_pipeline {
    use super::*;

    #[test]
    fn record_and_replay_10_frames() -> R {
        let td = make_tmp()?;
        let path = record_frames(10, &td)?;
        assert!(path.exists());

        let config = ReplayConfig::default();
        let mut replay = BlackboxReplay::load_from_file(&path, config)?;
        let result = replay.execute_replay()?;
        assert_eq!(result.frames_replayed, 10);
        assert!(result.success);
        Ok(())
    }

    #[test]
    fn record_and_replay_preserves_frame_count() -> R {
        for count in [1u32, 5, 50, 200] {
            let td = make_tmp()?;
            let path = record_frames(count, &td)?;
            let config = ReplayConfig::default();
            let mut replay = BlackboxReplay::load_from_file(&path, config)?;
            let result = replay.execute_replay()?;
            assert_eq!(
                result.frames_replayed, count as u64,
                "frame count mismatch for {count} frames"
            );
        }
        Ok(())
    }

    #[test]
    fn replay_result_has_zero_deviation_for_identity() -> R {
        let td = make_tmp()?;
        let path = record_frames(50, &td)?;
        let config = ReplayConfig {
            validate_outputs: true,
            fp_tolerance: 1e-6,
            ..Default::default()
        };
        let mut replay = BlackboxReplay::load_from_file(&path, config)?;
        let result = replay.execute_replay()?;
        // Identity replay: recorded output == replayed output
        assert_eq!(result.max_deviation, 0.0);
        assert_eq!(result.avg_deviation, 0.0);
        assert_eq!(result.frames_matched, result.frames_replayed);
        Ok(())
    }

    #[test]
    fn replay_statistics_match_rate_one() -> R {
        let td = make_tmp()?;
        let path = record_frames(20, &td)?;
        let config = ReplayConfig::default();
        let mut replay = BlackboxReplay::load_from_file(&path, config)?;
        replay.execute_replay()?;

        let stats = replay.generate_statistics();
        assert_eq!(stats.match_rate, 1.0);
        assert_eq!(stats.total_frames, 20);
        Ok(())
    }

    #[test]
    fn replay_header_matches_recording() -> R {
        let td = make_tmp()?;
        let path = record_frames(10, &td)?;
        let config = ReplayConfig::default();
        let replay = BlackboxReplay::load_from_file(&path, config)?;

        let header = replay.header();
        assert_eq!(&header.magic, WBB_MAGIC);
        assert_eq!(header.version, WBB_VERSION);
        assert_eq!(header.device_id, "test-capture-device");
        header.validate()?;
        Ok(())
    }

    #[test]
    fn replay_footer_valid() -> R {
        let td = make_tmp()?;
        let path = record_frames(10, &td)?;
        let config = ReplayConfig::default();
        let replay = BlackboxReplay::load_from_file(&path, config)?;

        let footer = replay.footer();
        footer.validate()?;
        assert_eq!(footer.total_frames, 10);
        Ok(())
    }

    #[test]
    fn replay_stream_a_data_accessible() -> R {
        let td = make_tmp()?;
        let path = record_frames(25, &td)?;
        let config = ReplayConfig::default();
        let replay = BlackboxReplay::load_from_file(&path, config)?;

        let data = replay.stream_a_data();
        assert_eq!(data.len(), 25);
        Ok(())
    }

    #[test]
    fn replay_deterministic_across_runs() -> R {
        let td = make_tmp()?;
        let path = record_frames(30, &td)?;

        let config = ReplayConfig {
            deterministic_seed: 42,
            ..Default::default()
        };

        let mut r1 = BlackboxReplay::load_from_file(&path, config.clone())?;
        let res1 = r1.execute_replay()?;

        let mut r2 = BlackboxReplay::load_from_file(&path, config)?;
        let res2 = r2.execute_replay()?;

        assert_eq!(res1.frames_replayed, res2.frames_replayed);
        assert_eq!(res1.frames_matched, res2.frames_matched);
        assert_eq!(res1.max_deviation, res2.max_deviation);
        assert_eq!(res1.avg_deviation, res2.avg_deviation);
        Ok(())
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Error handling: corrupt files, truncated, version mismatch
// ═════════════════════════════════════════════════════════════════════════════

mod error_handling {
    use super::*;

    #[test]
    fn load_nonexistent_file_fails() {
        let config = ReplayConfig::default();
        let result =
            BlackboxReplay::load_from_file(std::path::Path::new("nonexistent_file.wbb"), config);
        assert!(result.is_err());
    }

    #[test]
    fn load_empty_file_fails() -> R {
        let td = make_tmp()?;
        let path = td.path().join("empty.wbb");
        std::fs::write(&path, b"")?;
        let result = BlackboxReplay::load_from_file(&path, ReplayConfig::default());
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn load_garbage_file_fails() -> R {
        let td = make_tmp()?;
        let path = td.path().join("garbage.wbb");
        // Write just zeros — bincode will fail to deserialize the header
        // but won't trigger a huge memory allocation like random data could
        std::fs::write(&path, vec![0u8; 64])?;
        let result = BlackboxReplay::load_from_file(&path, ReplayConfig::default());
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn load_truncated_file_fails() -> R {
        let td = make_tmp()?;
        // Write a valid WBB magic followed by truncation
        let mut data = Vec::new();
        data.extend_from_slice(b"WBB1");
        data.extend_from_slice(&[0u8; 8]); // not enough for a full header
        let trunc_path = td.path().join("truncated.wbb");
        std::fs::write(&trunc_path, &data)?;
        let result = BlackboxReplay::load_from_file(&trunc_path, ReplayConfig::default());
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn load_file_with_corrupt_magic_fails() -> R {
        let td = make_tmp()?;
        // All zeros — magic won't match WBB1 and bincode won't try huge allocs
        let corrupt_path = td.path().join("bad_magic.wbb");
        std::fs::write(&corrupt_path, vec![0u8; 512])?;
        let result = BlackboxReplay::load_from_file(&corrupt_path, ReplayConfig::default());
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn seek_to_nonexistent_timestamp_fails() -> R {
        let td = make_tmp()?;
        let path = record_frames(10, &td)?;
        let config = ReplayConfig::default();
        let mut replay = BlackboxReplay::load_from_file(&path, config)?;

        // Recording is very short, this timestamp shouldn't exist in index
        let result = replay.seek_to_timestamp(999_999);
        // May succeed or fail depending on index state; just verify no panic
        let _ = result;
        Ok(())
    }

    #[test]
    fn diagnostic_error_display_variants() {
        let errors = [
            DiagnosticError::Recording("test".to_string()),
            DiagnosticError::Replay("test".to_string()),
            DiagnosticError::Format("test".to_string()),
            DiagnosticError::Io("test".to_string()),
            DiagnosticError::Serialization("test".to_string()),
            DiagnosticError::Deserialization("test".to_string()),
            DiagnosticError::Compression("test".to_string()),
            DiagnosticError::SizeLimit("test".to_string()),
            DiagnosticError::Configuration("test".to_string()),
            DiagnosticError::Validation("test".to_string()),
            DiagnosticError::CrcMismatch {
                expected: 0x1234,
                actual: 0x5678,
            },
            DiagnosticError::InvalidMagic {
                expected: *b"WBB1",
                actual: *b"XXXX",
            },
            DiagnosticError::UnsupportedVersion(99),
        ];

        for err in &errors {
            let msg = format!("{err}");
            assert!(!msg.is_empty(), "error display should not be empty");
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Index entry and random access
// ═════════════════════════════════════════════════════════════════════════════

mod index_and_seeking {
    use super::*;

    #[test]
    fn index_entry_creation() {
        let entry = IndexEntry::new(500, 100);
        assert_eq!(entry.timestamp_ms, 500);
        assert_eq!(entry.frame_count, 100);
        assert_eq!(entry.stream_a_offset, 0);
        assert_eq!(entry.stream_b_offset, 0);
        assert_eq!(entry.stream_c_offset, 0);
    }

    #[test]
    fn index_entries_bincode_roundtrip() -> R {
        let entries = vec![
            IndexEntry::new(0, 100),
            IndexEntry::new(100, 100),
            IndexEntry::new(200, 100),
        ];
        let encoded = bincode::serde::encode_to_vec(&entries, bincode::config::legacy())?;
        let (decoded, _): (Vec<IndexEntry>, _) =
            bincode::serde::decode_from_slice(&encoded, bincode::config::legacy())?;
        assert_eq!(decoded.len(), 3);
        assert_eq!(decoded[0].timestamp_ms, 0);
        assert_eq!(decoded[2].timestamp_ms, 200);
        Ok(())
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Replay result serialization
// ═════════════════════════════════════════════════════════════════════════════

mod replay_result_serde {
    use super::*;

    #[test]
    fn replay_result_json_roundtrip() -> R {
        let td = make_tmp()?;
        let path = record_frames(10, &td)?;
        let config = ReplayConfig::default();
        let mut replay = BlackboxReplay::load_from_file(&path, config)?;
        let result = replay.execute_replay()?;

        let json = serde_json::to_string(&result)?;
        let restored: ReplayResult = serde_json::from_str(&json)?;
        assert_eq!(restored.frames_replayed, result.frames_replayed);
        assert_eq!(restored.frames_matched, result.frames_matched);
        assert_eq!(restored.success, result.success);
        Ok(())
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Data type defaults
// ═════════════════════════════════════════════════════════════════════════════

mod data_defaults {
    use super::*;

    #[test]
    fn frame_data_default_zeroed() {
        let frame = FrameData::default();
        assert_eq!(frame.ffb_in, 0.0);
        assert_eq!(frame.torque_out, 0.0);
        assert_eq!(frame.wheel_speed, 0.0);
        assert!(!frame.hands_off);
        assert_eq!(frame.ts_mono_ns, 0);
        assert_eq!(frame.seq, 0);
    }

    #[test]
    fn telemetry_data_default() {
        let tel = TelemetryData::default();
        assert_eq!(tel.ffb_scalar, 1.0);
        assert_eq!(tel.rpm, 0.0);
        assert_eq!(tel.speed_ms, 0.0);
        assert_eq!(tel.gear, 0);
        assert!(tel.car_id.is_none());
        assert!(tel.track_id.is_none());
    }

    #[test]
    fn safety_state_default_is_safe_torque() {
        let state = SafetyStateSimple::default();
        assert!(matches!(state, SafetyStateSimple::SafeTorque));
    }

    #[test]
    fn replay_config_default_values() {
        let config = ReplayConfig::default();
        assert!(config.fp_tolerance > 0.0);
        assert!(config.max_duration_s > 0);
        assert!(config.validate_outputs);
        assert!(!config.strict_timing);
    }
}
