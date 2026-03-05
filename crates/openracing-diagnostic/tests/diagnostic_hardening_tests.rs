//! Hardening tests for the diagnostic system.
//!
//! Tests diagnostic report generation, system health checks, RT diagnostics,
//! error code catalog, support bundle generation with redaction, and
//! diagnostic event serialization.

use openracing_diagnostic::prelude::*;
use openracing_diagnostic::{StreamA, StreamB, StreamC};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_frame(seq: u16, ffb_in: f32) -> FrameData {
    FrameData {
        ffb_in,
        torque_out: ffb_in * 0.5,
        wheel_speed: seq as f32 * 0.1,
        hands_off: false,
        ts_mono_ns: seq as u64 * 1_000_000,
        seq,
    }
}

fn make_health_event(device: &str, event_type: &str) -> HealthEventData {
    HealthEventData {
        timestamp_ns: 0,
        device_id: device.to_string(),
        event_type: event_type.to_string(),
        context: serde_json::json!({"test": true}),
    }
}

fn create_temp_config() -> DiagnosticResult<(BlackboxConfig, TempDir)> {
    let temp_dir =
        TempDir::new().map_err(|e| DiagnosticError::Io(format!("TempDir creation: {e}")))?;
    let config = BlackboxConfig::new("hardening-device", temp_dir.path());
    Ok((config, temp_dir))
}

/// Record N frames into a fresh recorder and finalize, returning the output path.
fn record_and_finalize(n: usize) -> DiagnosticResult<(std::path::PathBuf, TempDir)> {
    let (config, temp_dir) = create_temp_config()?;
    let mut recorder = BlackboxRecorder::new(config)?;
    for i in 0..n {
        recorder.record_frame(
            make_frame(i as u16, (i as f32) * 0.01),
            &[i as f32 * 0.005],
            SafetyStateSimple::SafeTorque,
            100 + i as u64,
        )?;
    }
    let path = recorder.finalize()?;
    Ok((path, temp_dir))
}

// ===========================================================================
// 1. Diagnostic report generation
// ===========================================================================

#[test]
fn test_diagnostic_report_recording_and_replay() -> DiagnosticResult<()> {
    let (path, _dir) = record_and_finalize(50)?;
    assert!(path.exists());

    let replay_config = ReplayConfig {
        validate_outputs: true,
        fp_tolerance: 1e-3,
        ..Default::default()
    };
    let mut replay = BlackboxReplay::load_from_file(&path, replay_config)?;
    let result = replay.execute_replay()?;

    assert!(
        result.frames_replayed > 0,
        "Should replay at least one frame"
    );
    assert!(result.success, "Replay should succeed");
    Ok(())
}

#[test]
fn test_diagnostic_report_statistics() -> DiagnosticResult<()> {
    let (path, _dir) = record_and_finalize(100)?;

    let mut replay = BlackboxReplay::load_from_file(&path, ReplayConfig::default())?;
    let _result = replay.execute_replay()?;
    let stats = replay.generate_statistics();

    assert!(stats.total_frames > 0);
    assert!(
        (0.0..=1.0).contains(&stats.match_rate),
        "match_rate should be between 0 and 1"
    );
    Ok(())
}

#[test]
fn test_diagnostic_report_header_footer_roundtrip() -> DiagnosticResult<()> {
    let (path, _dir) = record_and_finalize(20)?;
    let replay = BlackboxReplay::load_from_file(&path, ReplayConfig::default())?;

    let header = replay.header();
    assert_eq!(&header.magic, WBB_MAGIC);
    assert_eq!(header.version, WBB_VERSION);
    header.validate()?;

    let footer = replay.footer();
    footer.validate()?;
    Ok(())
}

// ===========================================================================
// 2. System health check
// ===========================================================================

#[test]
fn test_health_event_recording_in_blackbox() -> DiagnosticResult<()> {
    let (config, _dir) = create_temp_config()?;
    let mut recorder = BlackboxRecorder::new(config)?;

    let event = make_health_event("device-01", "DeviceConnected");
    recorder.record_health_event(event)?;
    assert_eq!(recorder.get_stats().health_events, 1);

    let event2 = make_health_event("device-01", "FaultDetected");
    recorder.record_health_event(event2)?;
    assert_eq!(recorder.get_stats().health_events, 2);

    let _path = recorder.finalize()?;
    Ok(())
}

#[test]
fn test_health_event_stream_c_serialization() -> DiagnosticResult<()> {
    let mut stream_c = StreamC::new();

    for i in 0..5 {
        let event = HealthEventData {
            timestamp_ns: i * 1_000_000,
            device_id: format!("dev-{i}"),
            event_type: "Tick".to_string(),
            context: serde_json::json!({"seq": i}),
        };
        stream_c.record_health_event(event)?;
    }
    assert_eq!(stream_c.record_count(), 5);

    let data = stream_c.get_data()?;
    assert!(
        !data.is_empty(),
        "Serialized Stream C data should not be empty"
    );
    assert_eq!(stream_c.record_count(), 0, "get_data should drain records");
    Ok(())
}

// ===========================================================================
// 3. RT diagnostics — latency and jitter metrics
// ===========================================================================

#[test]
fn test_rt_recording_captures_processing_time() -> DiagnosticResult<()> {
    let (config, _dir) = create_temp_config()?;
    let mut recorder = BlackboxRecorder::new(config)?;

    let processing_times = [50u64, 100, 200, 150, 300];
    for (i, &pt) in processing_times.iter().enumerate() {
        recorder.record_frame(
            make_frame(i as u16, 0.5),
            &[0.1],
            SafetyStateSimple::SafeTorque,
            pt,
        )?;
    }

    let path = recorder.finalize()?;

    let replay = BlackboxReplay::load_from_file(&path, ReplayConfig::default())?;
    let stream_a = replay.stream_a_data();
    assert_eq!(stream_a.len(), processing_times.len());

    for (record, &expected_pt) in stream_a.iter().zip(processing_times.iter()) {
        assert_eq!(record.processing_time_us, expected_pt);
    }
    Ok(())
}

#[test]
fn test_rt_frame_sequence_preserved() -> DiagnosticResult<()> {
    let (config, _dir) = create_temp_config()?;
    let mut recorder = BlackboxRecorder::new(config)?;

    for seq in 0u16..20 {
        recorder.record_frame(
            make_frame(seq, seq as f32 * 0.05),
            &[],
            SafetyStateSimple::SafeTorque,
            100,
        )?;
    }

    let path = recorder.finalize()?;
    let replay = BlackboxReplay::load_from_file(&path, ReplayConfig::default())?;

    for (i, record) in replay.stream_a_data().iter().enumerate() {
        assert_eq!(record.frame.seq, i as u16);
    }
    Ok(())
}

// ===========================================================================
// 4. Error code catalog
// ===========================================================================

#[test]
fn test_error_display_recording() {
    let err = DiagnosticError::Recording("buffer full".to_string());
    assert!(format!("{err}").contains("buffer full"));
}

#[test]
fn test_error_display_replay() {
    let err = DiagnosticError::Replay("mismatch".to_string());
    assert!(format!("{err}").contains("mismatch"));
}

#[test]
fn test_error_display_format() {
    let err = DiagnosticError::Format("bad magic".to_string());
    assert!(format!("{err}").contains("bad magic"));
}

#[test]
fn test_error_display_io() {
    let err = DiagnosticError::Io("not found".to_string());
    assert!(format!("{err}").contains("not found"));
}

#[test]
fn test_error_display_serialization() {
    let err = DiagnosticError::Serialization("encode fail".to_string());
    assert!(format!("{err}").contains("encode fail"));
}

#[test]
fn test_error_display_deserialization() {
    let err = DiagnosticError::Deserialization("decode fail".to_string());
    assert!(format!("{err}").contains("decode fail"));
}

#[test]
fn test_error_display_compression() {
    let err = DiagnosticError::Compression("corrupt".to_string());
    assert!(format!("{err}").contains("corrupt"));
}

#[test]
fn test_error_display_size_limit() {
    let err = DiagnosticError::SizeLimit("25MB exceeded".to_string());
    assert!(format!("{err}").contains("25MB exceeded"));
}

#[test]
fn test_error_display_configuration() {
    let err = DiagnosticError::Configuration("bad param".to_string());
    assert!(format!("{err}").contains("bad param"));
}

#[test]
fn test_error_display_validation() {
    let err = DiagnosticError::Validation("timestamp off".to_string());
    assert!(format!("{err}").contains("timestamp off"));
}

#[test]
fn test_error_display_crc_mismatch() {
    let err = DiagnosticError::CrcMismatch {
        expected: 0xAABB,
        actual: 0xCCDD,
    };
    let msg = format!("{err}");
    assert!(msg.contains("43707")); // 0xAABB
    assert!(msg.contains("52445")); // 0xCCDD
}

#[test]
fn test_error_display_invalid_magic() {
    let err = DiagnosticError::InvalidMagic {
        expected: *b"WBB1",
        actual: *b"XXXX",
    };
    assert!(format!("{err}").contains("Invalid magic"));
}

#[test]
fn test_error_display_unsupported_version() {
    let err = DiagnosticError::UnsupportedVersion(99);
    assert!(format!("{err}").contains("99"));
}

#[test]
fn test_error_from_io() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
    let diag_err: DiagnosticError = io_err.into();
    assert!(matches!(diag_err, DiagnosticError::Io(_)));
}

#[test]
fn test_error_from_serde_json() {
    let bad_json = b"{{invalid";
    let json_err: Result<serde_json::Value, _> = serde_json::from_slice(bad_json);
    if let Err(e) = json_err {
        let diag_err: DiagnosticError = e.into();
        assert!(matches!(diag_err, DiagnosticError::Serialization(_)));
    }
}

// ===========================================================================
// 5. Support bundle generation (with redaction)
// ===========================================================================

#[test]
fn test_support_bundle_empty_generation() -> DiagnosticResult<()> {
    let temp_dir =
        TempDir::new().map_err(|e| DiagnosticError::Io(format!("TempDir creation: {e}")))?;
    let config = SupportBundleConfig::default();
    let bundle = SupportBundle::new(config);

    let bundle_path = temp_dir.path().join("empty_bundle.zip");
    bundle.generate(&bundle_path)?;
    assert!(bundle_path.exists());
    Ok(())
}

#[test]
fn test_support_bundle_with_system_info() -> DiagnosticResult<()> {
    let temp_dir =
        TempDir::new().map_err(|e| DiagnosticError::Io(format!("TempDir creation: {e}")))?;
    let config = SupportBundleConfig {
        include_system_info: true,
        include_logs: false,
        include_profiles: false,
        include_recent_recordings: false,
        ..Default::default()
    };
    let mut bundle = SupportBundle::new(config);
    bundle.add_system_info()?;

    let bundle_path = temp_dir.path().join("sysinfo_bundle.zip");
    bundle.generate(&bundle_path)?;
    assert!(bundle_path.exists());

    let metadata =
        std::fs::metadata(&bundle_path).map_err(|e| DiagnosticError::Io(e.to_string()))?;
    assert!(metadata.len() > 0, "Bundle should not be zero bytes");
    Ok(())
}

#[test]
fn test_support_bundle_with_health_events() -> DiagnosticResult<()> {
    let temp_dir =
        TempDir::new().map_err(|e| DiagnosticError::Io(format!("TempDir creation: {e}")))?;
    let config = SupportBundleConfig::default();
    let mut bundle = SupportBundle::new(config);

    let events: Vec<HealthEventData> = (0..10)
        .map(|i| HealthEventData {
            timestamp_ns: i * 1_000_000,
            device_id: format!("dev-{i}"),
            event_type: "StatusUpdate".to_string(),
            context: serde_json::json!({"seq": i}),
        })
        .collect();

    bundle.add_health_events(&events)?;

    let bundle_path = temp_dir.path().join("health_bundle.zip");
    bundle.generate(&bundle_path)?;
    assert!(bundle_path.exists());
    Ok(())
}

#[test]
fn test_support_bundle_size_limit_enforcement() -> DiagnosticResult<()> {
    let config = SupportBundleConfig {
        max_bundle_size_mb: 0, // impossible limit
        ..Default::default()
    };
    let mut bundle = SupportBundle::new(config);

    let event = make_health_event("dev", "Tick");
    let result = bundle.add_health_events(&[event]);
    assert!(result.is_err(), "Should reject when size limit is 0");
    Ok(())
}

#[test]
fn test_support_bundle_estimated_size_grows() -> DiagnosticResult<()> {
    let config = SupportBundleConfig::default();
    let mut bundle = SupportBundle::new(config);

    let before = bundle.estimated_size_mb();
    bundle.add_system_info()?;
    let after = bundle.estimated_size_mb();

    assert!(
        after >= before,
        "Size estimate should grow after adding system info"
    );
    Ok(())
}

#[test]
fn test_support_bundle_disabled_system_info() -> DiagnosticResult<()> {
    let config = SupportBundleConfig {
        include_system_info: false,
        ..Default::default()
    };
    let mut bundle = SupportBundle::new(config);
    // Should be a no-op when disabled
    bundle.add_system_info()?;
    assert!(
        bundle.estimated_size_mb() < 0.001,
        "No data should be added when system info is disabled"
    );
    Ok(())
}

// ===========================================================================
// 6. Diagnostic event serialization
// ===========================================================================

#[test]
fn test_frame_data_serialization_roundtrip() -> DiagnosticResult<()> {
    let frame = FrameData {
        ffb_in: 0.75,
        torque_out: -0.33,
        wheel_speed: 42.0,
        hands_off: true,
        ts_mono_ns: 123_456_789,
        seq: 999,
    };
    let json = serde_json::to_string(&frame).map_err(DiagnosticError::from)?;
    let restored: FrameData = serde_json::from_str(&json)
        .map_err(|e| DiagnosticError::Deserialization(format!("FrameData JSON roundtrip: {e}")))?;

    assert!((restored.ffb_in - frame.ffb_in).abs() < f32::EPSILON);
    assert!((restored.torque_out - frame.torque_out).abs() < f32::EPSILON);
    assert_eq!(restored.seq, frame.seq);
    assert_eq!(restored.hands_off, frame.hands_off);
    Ok(())
}

#[test]
fn test_telemetry_data_serialization_roundtrip() -> DiagnosticResult<()> {
    let telemetry = TelemetryData {
        ffb_scalar: 0.85,
        rpm: 7500.0,
        speed_ms: 55.0,
        slip_ratio: 0.02,
        gear: 4,
        car_id: Some("porsche_911".to_string()),
        track_id: Some("spa".to_string()),
    };
    let json = serde_json::to_string(&telemetry).map_err(DiagnosticError::from)?;
    let restored: TelemetryData = serde_json::from_str(&json).map_err(|e| {
        DiagnosticError::Deserialization(format!("TelemetryData JSON roundtrip: {e}"))
    })?;

    assert!((restored.rpm - telemetry.rpm).abs() < f32::EPSILON);
    assert_eq!(restored.gear, telemetry.gear);
    assert_eq!(restored.car_id.as_deref(), Some("porsche_911"));
    Ok(())
}

#[test]
fn test_health_event_serialization_roundtrip() -> DiagnosticResult<()> {
    let event = HealthEventData {
        timestamp_ns: 99_999,
        device_id: "wheel-007".to_string(),
        event_type: "FaultCleared".to_string(),
        context: serde_json::json!({"fault": "overtemp", "resolved": true}),
    };
    let json = serde_json::to_string(&event).map_err(DiagnosticError::from)?;
    let restored: HealthEventData = serde_json::from_str(&json).map_err(|e| {
        DiagnosticError::Deserialization(format!("HealthEventData JSON roundtrip: {e}"))
    })?;

    assert_eq!(restored.device_id, event.device_id);
    assert_eq!(restored.event_type, event.event_type);
    assert_eq!(restored.context["fault"], "overtemp");
    Ok(())
}

#[test]
fn test_safety_state_variants_serialize() -> DiagnosticResult<()> {
    let states = vec![
        SafetyStateSimple::SafeTorque,
        SafetyStateSimple::HighTorqueChallenge,
        SafetyStateSimple::AwaitingPhysicalAck,
        SafetyStateSimple::HighTorqueActive,
        SafetyStateSimple::Faulted {
            fault_type: "Overtemp".to_string(),
        },
    ];

    for state in states {
        let json = serde_json::to_string(&state).map_err(DiagnosticError::from)?;
        let restored: SafetyStateSimple = serde_json::from_str(&json)
            .map_err(|e| DiagnosticError::Deserialization(format!("SafetyState roundtrip: {e}")))?;
        // Ensure the round-trip re-serializes identically
        let json2 = serde_json::to_string(&restored).map_err(DiagnosticError::from)?;
        assert_eq!(json, json2);
    }
    Ok(())
}

#[test]
fn test_stream_a_binary_roundtrip() -> DiagnosticResult<()> {
    let mut stream = StreamA::with_capacity(10);

    for i in 0..5 {
        stream.record_frame(
            make_frame(i, i as f32 * 0.2),
            &[0.1, 0.2],
            SafetyStateSimple::SafeTorque,
            100,
        )?;
    }
    assert_eq!(stream.record_count(), 5);

    let data = stream.get_data()?;
    assert!(!data.is_empty());
    assert_eq!(stream.record_count(), 0, "get_data should drain records");

    let mut reader = StreamReader::new(data);
    let mut count = 0;
    while let Some(record) = reader.read_stream_a_record()? {
        assert_eq!(record.frame.seq, count as u16);
        count += 1;
    }
    assert_eq!(count, 5);
    Ok(())
}

#[test]
fn test_stream_b_binary_roundtrip() -> DiagnosticResult<()> {
    let mut stream = StreamB::with_rate(1_000_000.0); // very high rate to avoid rate-limiting

    let recorded = stream.record_telemetry(TelemetryData {
        ffb_scalar: 0.9,
        rpm: 5000.0,
        speed_ms: 30.0,
        slip_ratio: 0.05,
        gear: 3,
        car_id: None,
        track_id: None,
    })?;
    assert!(recorded, "First record should not be rate-limited");

    let data = stream.get_data()?;
    let mut reader = StreamReader::new(data);
    let record = reader.read_stream_b_record()?;
    assert!(record.is_some());
    let record = record.ok_or(DiagnosticError::Replay("missing record".into()))?;
    assert!((record.telemetry.rpm - 5000.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_stream_reader_empty_data() -> DiagnosticResult<()> {
    let reader = StreamReader::new(Vec::new());
    assert!(reader.is_at_end());
    assert_eq!(reader.position(), 0);
    Ok(())
}

#[test]
fn test_stream_reader_reset() -> DiagnosticResult<()> {
    let mut stream = StreamA::new();
    stream.record_frame(make_frame(0, 0.5), &[], SafetyStateSimple::SafeTorque, 100)?;
    let data = stream.get_data()?;

    let mut reader = StreamReader::new(data);
    let _ = reader.read_stream_a_record()?;
    assert!(reader.is_at_end());

    reader.reset();
    assert!(!reader.is_at_end());
    assert_eq!(reader.position(), 0);
    Ok(())
}

#[test]
fn test_stream_a_reset() -> DiagnosticResult<()> {
    let mut stream = StreamA::new();
    stream.record_frame(make_frame(0, 0.1), &[], SafetyStateSimple::SafeTorque, 50)?;
    assert_eq!(stream.record_count(), 1);

    stream.reset();
    assert_eq!(stream.record_count(), 0);
    Ok(())
}

#[test]
fn test_frame_data_default_values() {
    let frame = FrameData::default();
    assert_eq!(frame.ffb_in, 0.0);
    assert_eq!(frame.torque_out, 0.0);
    assert_eq!(frame.wheel_speed, 0.0);
    assert!(!frame.hands_off);
    assert_eq!(frame.ts_mono_ns, 0);
    assert_eq!(frame.seq, 0);
}

#[test]
fn test_telemetry_data_default_values() {
    let t = TelemetryData::default();
    assert!((t.ffb_scalar - 1.0).abs() < f32::EPSILON);
    assert_eq!(t.rpm, 0.0);
    assert!(t.car_id.is_none());
    assert!(t.track_id.is_none());
}

// ===========================================================================
// Header / footer validation edge cases
// ===========================================================================

#[test]
fn test_header_invalid_magic_detected() {
    let mut header = WbbHeader::new("test", 1, 0x07, 6);
    header.magic = *b"ZZZZ";
    let result = header.validate();
    assert!(result.is_err());
}

#[test]
fn test_header_unsupported_version_detected() {
    let mut header = WbbHeader::new("test", 1, 0x07, 6);
    header.version = 999;
    let result = header.validate();
    assert!(result.is_err());
}

#[test]
fn test_header_invalid_compression_detected() {
    let header = WbbHeader::new("test", 1, 0x07, 10);
    let result = header.validate();
    assert!(result.is_err());
}

#[test]
fn test_footer_invalid_magic_detected() {
    let mut footer = WbbFooter::new(1000, 100);
    footer.footer_magic = *b"XXXX";
    let result = footer.validate();
    assert!(result.is_err());
}

#[test]
fn test_stream_type_flags() {
    assert_eq!(StreamType::A.flag(), 0x01);
    assert_eq!(StreamType::B.flag(), 0x02);
    assert_eq!(StreamType::C.flag(), 0x04);
}

#[test]
fn test_config_selective_streams() -> DiagnosticResult<()> {
    let (mut config, _dir) = create_temp_config()?;
    config.enable_stream_a = true;
    config.enable_stream_b = false;
    config.enable_stream_c = false;

    let mut recorder = BlackboxRecorder::new(config)?;
    recorder.record_frame(
        make_frame(0, 0.5),
        &[0.1],
        SafetyStateSimple::SafeTorque,
        100,
    )?;
    // Telemetry should be silently ignored when stream B is off
    recorder.record_telemetry(TelemetryData::default())?;
    assert_eq!(recorder.get_stats().telemetry_records, 0);

    let path = recorder.finalize()?;
    assert!(path.exists());
    Ok(())
}

#[test]
fn test_recording_stats_initial_state() -> DiagnosticResult<()> {
    let (config, _dir) = create_temp_config()?;
    let recorder = BlackboxRecorder::new(config)?;
    let stats = recorder.get_stats();

    assert!(stats.is_active);
    assert_eq!(stats.frames_recorded, 0);
    assert_eq!(stats.telemetry_records, 0);
    assert_eq!(stats.health_events, 0);
    Ok(())
}

#[test]
fn test_index_entry_creation() {
    let entry = IndexEntry::new(200, 50);
    assert_eq!(entry.timestamp_ms, 200);
    assert_eq!(entry.frame_count, 50);
    assert_eq!(entry.stream_a_offset, 0);
    assert_eq!(entry.stream_b_offset, 0);
    assert_eq!(entry.stream_c_offset, 0);
}
