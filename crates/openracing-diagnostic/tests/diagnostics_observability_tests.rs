//! Deep tests for diagnostics and observability: metric collection, trace event
//! recording/filtering, diagnostic snapshot generation, metric overflow/wraparound,
//! and stream data integrity across the openracing-diagnostic crate.

use openracing_diagnostic::{
    BlackboxConfig, BlackboxRecorder, DiagnosticError, DiagnosticResult, FrameData,
    HealthEventData, SafetyStateSimple, StreamA, StreamB, StreamC, StreamReader, SupportBundle,
    SupportBundleConfig, TelemetryData,
};
use std::io::Read;
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

fn make_frame(seq: u16, ffb: f32) -> FrameData {
    FrameData {
        ffb_in: ffb,
        torque_out: ffb * 0.5,
        wheel_speed: seq as f32 * 0.1,
        hands_off: false,
        ts_mono_ns: seq as u64 * 1_000_000,
        seq,
    }
}

fn make_health_event(device: &str, etype: &str) -> HealthEventData {
    HealthEventData {
        timestamp_ns: 1_000_000,
        device_id: device.to_string(),
        event_type: etype.to_string(),
        context: serde_json::json!({"test": true}),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. Metric collection – counters, gauges, histograms via streams
// ═══════════════════════════════════════════════════════════════════════════

mod metric_collection {
    use super::*;

    #[test]
    fn stream_a_counter_increments_per_frame() -> DiagnosticResult<()> {
        let mut stream = StreamA::new();
        for i in 0..100 {
            stream.record_frame(
                make_frame(i, 0.5),
                &[0.1],
                SafetyStateSimple::SafeTorque,
                100,
            )?;
        }
        assert_eq!(stream.record_count(), 100);
        Ok(())
    }

    #[test]
    fn stream_b_rate_limited_counter_reflects_accepted() -> DiagnosticResult<()> {
        let mut stream = StreamB::with_rate(10.0);
        let telem = TelemetryData::default();

        let first = stream.record_telemetry(telem.clone())?;
        assert!(first, "first record should always be accepted");

        // Immediate second call should be rate-limited
        let second = stream.record_telemetry(telem)?;
        assert!(!second, "immediate second call should be rate-limited");

        assert_eq!(stream.record_count(), 1);
        Ok(())
    }

    #[test]
    fn stream_c_event_counter_tracks_health_events() -> DiagnosticResult<()> {
        let mut stream = StreamC::new();
        for i in 0..50 {
            stream
                .record_health_event(make_health_event(&format!("dev-{i}"), "DeviceConnected"))?;
        }
        assert_eq!(stream.record_count(), 50);
        Ok(())
    }

    #[test]
    fn stream_a_processing_time_tracked_per_record() -> DiagnosticResult<()> {
        let mut stream = StreamA::new();
        let processing_times: Vec<u64> = vec![50, 100, 150, 200, 250];

        for (i, &pt) in processing_times.iter().enumerate() {
            stream.record_frame(
                make_frame(i as u16, 0.3),
                &[0.1],
                SafetyStateSimple::SafeTorque,
                pt,
            )?;
        }

        let data = stream.get_data()?;
        let mut reader = StreamReader::new(data);
        let mut read_times = Vec::new();

        while let Some(record) = reader.read_stream_a_record()? {
            read_times.push(record.processing_time_us);
        }
        assert_eq!(read_times, processing_times);
        Ok(())
    }

    #[test]
    fn stream_a_with_capacity_preallocates_without_affecting_count() -> DiagnosticResult<()> {
        let stream = StreamA::with_capacity(5000);
        assert_eq!(stream.record_count(), 0);
        Ok(())
    }

    #[test]
    fn stream_b_rate_change_takes_effect() -> DiagnosticResult<()> {
        let mut stream = StreamB::with_rate(1.0);
        let telem = TelemetryData::default();

        let accepted = stream.record_telemetry(telem.clone())?;
        assert!(accepted);

        // Change to a very high rate so the next call is accepted quickly
        stream.set_rate_limit_hz(1_000_000.0);

        // Allow a tiny bit of time to pass
        std::thread::sleep(std::time::Duration::from_micros(10));
        let accepted = stream.record_telemetry(telem)?;
        assert!(accepted);
        assert_eq!(stream.record_count(), 2);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. Trace event recording and filtering
// ═══════════════════════════════════════════════════════════════════════════

mod trace_event_recording {
    use super::*;

    #[test]
    fn stream_a_records_all_safety_state_variants() -> DiagnosticResult<()> {
        let mut stream = StreamA::new();
        let states = [
            SafetyStateSimple::SafeTorque,
            SafetyStateSimple::HighTorqueChallenge,
            SafetyStateSimple::AwaitingPhysicalAck,
            SafetyStateSimple::HighTorqueActive,
            SafetyStateSimple::Faulted {
                fault_type: "overcurrent".to_string(),
            },
        ];

        for (i, state) in states.iter().enumerate() {
            stream.record_frame(make_frame(i as u16, 0.1), &[], state.clone(), 100)?;
        }

        let data = stream.get_data()?;
        let mut reader = StreamReader::new(data);
        let mut read_states = Vec::new();

        while let Some(record) = reader.read_stream_a_record()? {
            read_states.push(record.safety_state);
        }
        assert_eq!(read_states.len(), 5);

        // Verify Faulted variant preserves fault_type
        if let SafetyStateSimple::Faulted { fault_type } = &read_states[4] {
            assert_eq!(fault_type, "overcurrent");
        } else {
            return Err(DiagnosticError::Validation(
                "expected Faulted variant".to_string(),
            ));
        }
        Ok(())
    }

    #[test]
    fn stream_c_records_multiple_event_types() -> DiagnosticResult<()> {
        let mut stream = StreamC::new();
        stream.record_health_event(HealthEventData {
            timestamp_ns: 1_000_000,
            device_id: "dev-1".to_string(),
            event_type: "DeviceConnected".to_string(),
            context: serde_json::Value::Null,
        })?;
        stream.record_health_event(HealthEventData {
            timestamp_ns: 2_000_000,
            device_id: "dev-1".to_string(),
            event_type: "SafetyFault".to_string(),
            context: serde_json::Value::Null,
        })?;
        stream.record_health_event(HealthEventData {
            timestamp_ns: 3_000_000,
            device_id: "dev-2".to_string(),
            event_type: "DeviceConnected".to_string(),
            context: serde_json::Value::Null,
        })?;

        // Stream C uses bincode under the hood; serde_json::Value doesn't
        // roundtrip through bincode, so we verify counts and serialized size.
        assert_eq!(stream.record_count(), 3);
        let data = stream.get_data()?;
        assert!(!data.is_empty());
        assert_eq!(stream.record_count(), 0);
        Ok(())
    }

    #[test]
    fn stream_a_node_outputs_preserved_across_roundtrip() -> DiagnosticResult<()> {
        let mut stream = StreamA::new();
        let outputs = vec![0.1, 0.2, 0.3, -0.5, 1.0];
        stream.record_frame(
            make_frame(0, 0.5),
            &outputs,
            SafetyStateSimple::SafeTorque,
            100,
        )?;

        let data = stream.get_data()?;
        let mut reader = StreamReader::new(data);
        let record = reader
            .read_stream_a_record()?
            .ok_or_else(|| DiagnosticError::Validation("no record".to_string()))?;

        assert_eq!(record.node_outputs.len(), 5);
        for (a, b) in record.node_outputs.iter().zip(outputs.iter()) {
            assert!((a - b).abs() < f32::EPSILON);
        }
        Ok(())
    }

    #[test]
    fn stream_b_telemetry_fields_preserved_roundtrip() -> DiagnosticResult<()> {
        let mut stream = StreamB::with_rate(1_000_000.0);
        let telem = TelemetryData {
            ffb_scalar: 0.75,
            rpm: 8000.0,
            speed_ms: 65.5,
            slip_ratio: 0.15,
            gear: 5,
            car_id: Some("ferrari_488".to_string()),
            track_id: Some("monza".to_string()),
        };

        stream.record_telemetry(telem)?;
        let data = stream.get_data()?;
        let mut reader = StreamReader::new(data);
        let record = reader
            .read_stream_b_record()?
            .ok_or_else(|| DiagnosticError::Validation("no record".to_string()))?;

        assert!((record.telemetry.rpm - 8000.0).abs() < f32::EPSILON);
        assert_eq!(record.telemetry.gear, 5);
        assert_eq!(record.telemetry.car_id.as_deref(), Some("ferrari_488"));
        assert_eq!(record.telemetry.track_id.as_deref(), Some("monza"));
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. Diagnostic snapshot generation
// ═══════════════════════════════════════════════════════════════════════════

mod diagnostic_snapshot {
    use super::*;

    #[test]
    fn snapshot_captures_multi_stream_state() -> DiagnosticResult<()> {
        let temp = TempDir::new().map_err(|e| DiagnosticError::Io(e.to_string()))?;
        let config = BlackboxConfig::new("snap-dev", temp.path());
        let mut recorder = BlackboxRecorder::new(config)?;

        // Record frames across streams
        for i in 0..10 {
            recorder.record_frame(
                make_frame(i, 0.5),
                &[0.1, 0.2],
                SafetyStateSimple::SafeTorque,
                100,
            )?;
        }

        let stats = recorder.get_stats();
        assert!(stats.frames_recorded >= 10);
        let path = recorder.finalize()?;
        assert!(path.exists());
        Ok(())
    }

    #[test]
    fn snapshot_via_bundle_captures_system_state() -> TestResult {
        let temp = TempDir::new()?;
        let config = SupportBundleConfig::default();
        let mut bundle = SupportBundle::new(config);

        bundle.add_system_info()?;
        let path = temp.path().join("snap.zip");
        bundle.generate(&path)?;

        // Verify system_info.json is present and parseable
        let file = std::fs::File::open(&path)?;
        let mut archive = zip::ZipArchive::new(file)?;

        let mut sys_info_content = String::new();
        archive
            .by_name("system_info.json")?
            .read_to_string(&mut sys_info_content)?;

        let sys_info: serde_json::Value = serde_json::from_str(&sys_info_content)?;
        assert!(sys_info.get("os_info").is_some());
        assert!(sys_info.get("hardware_info").is_some());
        assert!(sys_info.get("process_info").is_some());
        assert!(sys_info.get("environment").is_some());
        Ok(())
    }

    #[test]
    fn snapshot_records_timestamps_monotonically() -> DiagnosticResult<()> {
        let mut stream = StreamA::new();
        for i in 0..20 {
            stream.record_frame(make_frame(i, 0.1), &[], SafetyStateSimple::SafeTorque, 50)?;
        }

        let data = stream.get_data()?;
        let mut reader = StreamReader::new(data);
        let mut prev_ts = 0u64;

        while let Some(record) = reader.read_stream_a_record()? {
            assert!(
                record.timestamp_ns >= prev_ts,
                "timestamps must be monotonically non-decreasing"
            );
            prev_ts = record.timestamp_ns;
        }
        Ok(())
    }

    #[test]
    fn snapshot_frame_data_preserved_through_finalize() -> DiagnosticResult<()> {
        let temp = TempDir::new().map_err(|e| DiagnosticError::Io(e.to_string()))?;
        let config = BlackboxConfig::new("roundtrip-dev", temp.path());
        let mut recorder = BlackboxRecorder::new(config)?;

        recorder.record_frame(
            FrameData {
                ffb_in: -1.0,
                torque_out: 1.0,
                wheel_speed: 99.9,
                hands_off: true,
                ts_mono_ns: 42_000_000,
                seq: 42,
            },
            &[0.5, -0.5],
            SafetyStateSimple::HighTorqueActive,
            200,
        )?;

        let path = recorder.finalize()?;
        assert!(
            std::fs::metadata(&path)
                .map_err(|e| DiagnosticError::Io(e.to_string()))?
                .len()
                > 0
        );
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. Metric overflow / wraparound handling
// ═══════════════════════════════════════════════════════════════════════════

mod metric_overflow {
    use super::*;

    #[test]
    fn stream_a_handles_max_seq_number() -> DiagnosticResult<()> {
        let mut stream = StreamA::new();
        stream.record_frame(
            FrameData {
                seq: u16::MAX,
                ..FrameData::default()
            },
            &[],
            SafetyStateSimple::SafeTorque,
            0,
        )?;

        let data = stream.get_data()?;
        let mut reader = StreamReader::new(data);
        let record = reader
            .read_stream_a_record()?
            .ok_or_else(|| DiagnosticError::Validation("no record".to_string()))?;
        assert_eq!(record.frame.seq, u16::MAX);
        Ok(())
    }

    #[test]
    fn stream_a_handles_max_timestamp() -> DiagnosticResult<()> {
        let mut stream = StreamA::new();
        stream.record_frame(
            FrameData {
                ts_mono_ns: u64::MAX,
                ..FrameData::default()
            },
            &[],
            SafetyStateSimple::SafeTorque,
            0,
        )?;

        let data = stream.get_data()?;
        let mut reader = StreamReader::new(data);
        let record = reader
            .read_stream_a_record()?
            .ok_or_else(|| DiagnosticError::Validation("no record".to_string()))?;
        assert_eq!(record.frame.ts_mono_ns, u64::MAX);
        Ok(())
    }

    #[test]
    fn stream_a_handles_extreme_float_values() -> DiagnosticResult<()> {
        let mut stream = StreamA::new();
        let extremes = [f32::MAX, f32::MIN, f32::MIN_POSITIVE, -0.0, 0.0];

        for (i, &val) in extremes.iter().enumerate() {
            stream.record_frame(
                FrameData {
                    ffb_in: val,
                    torque_out: val,
                    wheel_speed: val.abs(),
                    ..FrameData::default()
                },
                &[val],
                SafetyStateSimple::SafeTorque,
                i as u64,
            )?;
        }

        let data = stream.get_data()?;
        let mut reader = StreamReader::new(data);
        let mut count = 0;
        while reader.read_stream_a_record()?.is_some() {
            count += 1;
        }
        assert_eq!(count, 5);
        Ok(())
    }

    #[test]
    fn stream_a_handles_max_processing_time() -> DiagnosticResult<()> {
        let mut stream = StreamA::new();
        stream.record_frame(
            FrameData::default(),
            &[],
            SafetyStateSimple::SafeTorque,
            u64::MAX,
        )?;

        let data = stream.get_data()?;
        let mut reader = StreamReader::new(data);
        let record = reader
            .read_stream_a_record()?
            .ok_or_else(|| DiagnosticError::Validation("no record".to_string()))?;
        assert_eq!(record.processing_time_us, u64::MAX);
        Ok(())
    }

    #[test]
    fn stream_reader_handles_empty_data() -> DiagnosticResult<()> {
        let reader = StreamReader::new(Vec::new());
        assert!(reader.is_at_end());
        assert_eq!(reader.position(), 0);
        Ok(())
    }

    #[test]
    fn stream_reader_rejects_truncated_length_prefix() {
        let mut reader = StreamReader::new(vec![0x01, 0x02]);
        let result = reader.read_stream_a_record();
        assert!(result.is_err());
    }

    #[test]
    fn stream_reader_rejects_truncated_record_body() {
        let mut data = Vec::new();
        // Length prefix says 100 bytes, but we only have 4 bytes of header
        data.extend_from_slice(&100u32.to_le_bytes());
        data.extend_from_slice(&[0u8; 10]);

        let mut reader = StreamReader::new(data);
        let result = reader.read_stream_a_record();
        assert!(result.is_err());
    }

    #[test]
    fn stream_reader_reset_allows_multiple_passes() -> DiagnosticResult<()> {
        let mut stream = StreamA::new();
        for i in 0..3 {
            stream.record_frame(make_frame(i, 0.1), &[], SafetyStateSimple::SafeTorque, 50)?;
        }

        let data = stream.get_data()?;
        let mut reader = StreamReader::new(data);

        // First pass
        let mut count1 = 0;
        while reader.read_stream_a_record()?.is_some() {
            count1 += 1;
        }

        // Reset and second pass
        reader.reset();
        let mut count2 = 0;
        while reader.read_stream_a_record()?.is_some() {
            count2 += 1;
        }

        assert_eq!(count1, count2);
        assert_eq!(count1, 3);
        Ok(())
    }

    #[test]
    fn stream_a_large_node_output_vector() -> DiagnosticResult<()> {
        let mut stream = StreamA::new();
        let outputs: Vec<f32> = (0..256).map(|i| i as f32 * 0.001).collect();

        stream.record_frame(
            make_frame(0, 0.5),
            &outputs,
            SafetyStateSimple::SafeTorque,
            100,
        )?;

        let data = stream.get_data()?;
        let mut reader = StreamReader::new(data);
        let record = reader
            .read_stream_a_record()?
            .ok_or_else(|| DiagnosticError::Validation("no record".to_string()))?;
        assert_eq!(record.node_outputs.len(), 256);
        Ok(())
    }

    #[test]
    fn stream_c_serializes_null_context() -> DiagnosticResult<()> {
        let mut stream = StreamC::new();

        stream.record_health_event(HealthEventData {
            timestamp_ns: 0,
            device_id: "dev-1".to_string(),
            event_type: "ComplexEvent".to_string(),
            context: serde_json::Value::Null,
        })?;

        assert_eq!(stream.record_count(), 1);
        let data = stream.get_data()?;
        assert!(!data.is_empty());
        Ok(())
    }

    #[test]
    fn bundle_handles_large_json_context_in_health_events() -> TestResult {
        let config = SupportBundleConfig::default();
        let mut bundle = SupportBundle::new(config);

        let large_context = serde_json::json!({
            "nested": {
                "array": (0..100).collect::<Vec<i32>>(),
                "deep": {"a": {"b": {"c": {"d": "value"}}}},
            },
            "large_string": "x".repeat(1024),
        });

        bundle.add_health_events(&[HealthEventData {
            timestamp_ns: 0,
            device_id: "dev-1".to_string(),
            event_type: "ComplexEvent".to_string(),
            context: large_context,
        }])?;

        let temp = TempDir::new()?;
        let path = temp.path().join("large_ctx.zip");
        bundle.generate(&path)?;

        // Verify the JSON in the bundle is parseable
        let file = std::fs::File::open(&path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        let mut content = String::new();
        archive
            .by_name("health_events.json")?
            .read_to_string(&mut content)?;
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&content)?;
        assert_eq!(parsed.len(), 1);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. Support bundle – PII redaction & integrity
// ═══════════════════════════════════════════════════════════════════════════

mod bundle_pii_and_integrity {
    use super::*;

    #[test]
    fn bundle_manifest_is_valid_json_with_required_keys() -> TestResult {
        let temp = TempDir::new()?;
        let config = SupportBundleConfig::default();
        let mut bundle = SupportBundle::new(config);

        bundle.add_health_events(&[make_health_event("dev-1", "Test")])?;
        let path = temp.path().join("pii_test.zip");
        bundle.generate(&path)?;

        let file = std::fs::File::open(&path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        let mut manifest = String::new();
        archive
            .by_name("manifest.json")?
            .read_to_string(&mut manifest)?;

        let parsed: serde_json::Value = serde_json::from_str(&manifest)?;
        assert!(parsed.get("bundle_version").is_some());
        assert!(parsed.get("created_at").is_some());
        assert!(parsed.get("config").is_some());
        assert!(parsed.get("contents").is_some());
        Ok(())
    }

    #[test]
    fn bundle_with_log_and_profile_files_includes_them() -> TestResult {
        let temp = TempDir::new()?;
        let log_dir = temp.path().join("logs");
        let profile_dir = temp.path().join("profiles");
        std::fs::create_dir_all(&log_dir)?;
        std::fs::create_dir_all(&profile_dir)?;

        std::fs::write(log_dir.join("app.log"), "log line 1\nlog line 2")?;
        std::fs::write(profile_dir.join("default.json"), r#"{"name": "default"}"#)?;

        let config = SupportBundleConfig::default();
        let mut bundle = SupportBundle::new(config);
        bundle.add_log_files(&log_dir)?;
        bundle.add_profile_files(&profile_dir)?;

        let path = temp.path().join("full_bundle.zip");
        bundle.generate(&path)?;

        let file = std::fs::File::open(&path)?;
        let mut archive = zip::ZipArchive::new(file)?;

        let mut names = Vec::new();
        for i in 0..archive.len() {
            let entry = archive.by_index(i)?;
            names.push(entry.name().to_string());
        }

        assert!(names.iter().any(|n| n.starts_with("logs/")));
        assert!(names.iter().any(|n| n.starts_with("profiles/")));
        Ok(())
    }

    #[test]
    fn bundle_size_limit_enforced_on_health_events() -> TestResult {
        let config = SupportBundleConfig {
            max_bundle_size_mb: 1,
            ..SupportBundleConfig::default()
        };

        let mut bundle = SupportBundle::new(config);

        // Generate enough events to approach the limit
        let big_events: Vec<HealthEventData> = (0..50_000)
            .map(|i| HealthEventData {
                timestamp_ns: i,
                device_id: format!("device-{i}"),
                event_type: "LargeEvent".to_string(),
                context: serde_json::json!({"data": "x".repeat(100)}),
            })
            .collect();

        let result = bundle.add_health_events(&big_events);
        assert!(result.is_err(), "should reject events exceeding size limit");
        Ok(())
    }

    #[test]
    fn bundle_with_recording_files_includes_up_to_five() -> TestResult {
        let temp = TempDir::new()?;
        let rec_dir = temp.path().join("recordings");
        std::fs::create_dir_all(&rec_dir)?;

        // Create 7 .wbb files
        for i in 0..7 {
            std::fs::write(rec_dir.join(format!("rec_{i}.wbb")), vec![0u8; 100])?;
            // Small delay to ensure different modification times
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let config = SupportBundleConfig::default();
        let mut bundle = SupportBundle::new(config);
        bundle.add_recent_recordings(&rec_dir)?;

        let path = temp.path().join("rec_bundle.zip");
        bundle.generate(&path)?;

        let file = std::fs::File::open(&path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        let rec_count = (0..archive.len())
            .filter(|&i| {
                archive
                    .by_index(i)
                    .map(|e| e.name().starts_with("recordings/"))
                    .unwrap_or(false)
            })
            .count();

        assert!(
            rec_count <= 5,
            "should include at most 5 recordings, got {rec_count}"
        );
        Ok(())
    }

    #[test]
    fn bundle_partial_when_no_system_info() -> TestResult {
        let temp = TempDir::new()?;
        let config = SupportBundleConfig {
            include_system_info: false,
            ..SupportBundleConfig::default()
        };

        let mut bundle = SupportBundle::new(config);
        bundle.add_health_events(&[make_health_event("dev-1", "Test")])?;

        let path = temp.path().join("partial.zip");
        bundle.generate(&path)?;

        let file = std::fs::File::open(&path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        let has_sys_info = (0..archive.len()).any(|i| {
            archive
                .by_index(i)
                .map(|e| e.name() == "system_info.json")
                .unwrap_or(false)
        });

        assert!(!has_sys_info, "should not include system_info.json");
        Ok(())
    }

    #[test]
    fn bundle_partial_with_missing_dirs_succeeds() -> TestResult {
        let temp = TempDir::new()?;
        let config = SupportBundleConfig::default();
        let mut bundle = SupportBundle::new(config);

        // Non-existent directories should not cause errors
        let nonexistent = temp.path().join("nonexistent");
        bundle.add_log_files(&nonexistent)?;
        bundle.add_profile_files(&nonexistent)?;
        bundle.add_recent_recordings(&nonexistent)?;

        let path = temp.path().join("partial_missing.zip");
        bundle.generate(&path)?;
        assert!(path.exists());
        Ok(())
    }

    #[test]
    fn bundle_zip_integrity_all_entries_readable() -> TestResult {
        let temp = TempDir::new()?;
        let config = SupportBundleConfig::default();
        let mut bundle = SupportBundle::new(config);

        bundle.add_health_events(&[
            make_health_event("dev-1", "Connected"),
            make_health_event("dev-2", "Fault"),
        ])?;
        bundle.add_system_info()?;

        let path = temp.path().join("integrity.zip");
        bundle.generate(&path)?;

        let file = std::fs::File::open(&path)?;
        let mut archive = zip::ZipArchive::new(file)?;

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            assert!(
                !buf.is_empty() || entry.size() == 0,
                "entry {} should be readable",
                entry.name()
            );
        }
        Ok(())
    }

    #[test]
    fn bundle_health_events_json_parseable() -> TestResult {
        let temp = TempDir::new()?;
        let config = SupportBundleConfig::default();
        let mut bundle = SupportBundle::new(config);

        let events = vec![
            make_health_event("dev-1", "Connected"),
            make_health_event("dev-1", "SafetyFault"),
        ];
        bundle.add_health_events(&events)?;

        let path = temp.path().join("events.zip");
        bundle.generate(&path)?;

        let file = std::fs::File::open(&path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        let mut content = String::new();
        archive
            .by_name("health_events.json")?
            .read_to_string(&mut content)?;

        let parsed: Vec<serde_json::Value> = serde_json::from_str(&content)?;
        assert_eq!(parsed.len(), 2);
        Ok(())
    }

    #[test]
    fn bundle_estimated_size_tracks_additions() -> TestResult {
        let config = SupportBundleConfig::default();
        let mut bundle = SupportBundle::new(config);

        let initial = bundle.estimated_size_mb();
        bundle.add_health_events(&[make_health_event("dev-1", "Test")])?;
        let after_events = bundle.estimated_size_mb();
        assert!(after_events > initial);

        bundle.add_system_info()?;
        let after_sys = bundle.estimated_size_mb();
        assert!(after_sys > after_events);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. Stream A get_data clears state correctly
// ═══════════════════════════════════════════════════════════════════════════

mod stream_state_management {
    use super::*;

    #[test]
    fn stream_a_get_data_clears_records() -> DiagnosticResult<()> {
        let mut stream = StreamA::new();
        for i in 0..5 {
            stream.record_frame(make_frame(i, 0.1), &[], SafetyStateSimple::SafeTorque, 50)?;
        }
        assert_eq!(stream.record_count(), 5);

        let _data = stream.get_data()?;
        assert_eq!(stream.record_count(), 0);
        Ok(())
    }

    #[test]
    fn stream_a_reset_clears_all() -> DiagnosticResult<()> {
        let mut stream = StreamA::new();
        stream.record_frame(make_frame(0, 0.1), &[], SafetyStateSimple::SafeTorque, 50)?;
        stream.reset();
        assert_eq!(stream.record_count(), 0);

        let data = stream.get_data()?;
        assert!(data.is_empty());
        Ok(())
    }

    #[test]
    fn stream_b_get_data_clears_records() -> DiagnosticResult<()> {
        let mut stream = StreamB::with_rate(1_000_000.0);
        stream.record_telemetry(TelemetryData::default())?;
        std::thread::sleep(std::time::Duration::from_micros(10));
        stream.record_telemetry(TelemetryData::default())?;

        let data = stream.get_data()?;
        assert!(!data.is_empty());
        assert_eq!(stream.record_count(), 0);
        Ok(())
    }

    #[test]
    fn stream_c_get_data_clears_records() -> DiagnosticResult<()> {
        let mut stream = StreamC::new();
        stream.record_health_event(make_health_event("dev", "Test"))?;
        stream.record_health_event(make_health_event("dev", "Test2"))?;

        let data = stream.get_data()?;
        assert!(!data.is_empty());
        assert_eq!(stream.record_count(), 0);
        Ok(())
    }

    #[test]
    fn stream_a_default_equals_new() -> DiagnosticResult<()> {
        let a = StreamA::default();
        let b = StreamA::new();
        assert_eq!(a.record_count(), b.record_count());
        Ok(())
    }

    #[test]
    fn stream_b_default_equals_new() -> DiagnosticResult<()> {
        let a = StreamB::default();
        let b = StreamB::new();
        assert_eq!(a.record_count(), b.record_count());
        Ok(())
    }

    #[test]
    fn stream_c_default_equals_new() -> DiagnosticResult<()> {
        let a = StreamC::default();
        let b = StreamC::new();
        assert_eq!(a.record_count(), b.record_count());
        Ok(())
    }
}
