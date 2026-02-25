//! Integration tests for diagnostic recording and replay
//!
//! Tests the complete lifecycle of recording, replay, and support bundle generation.

#![allow(clippy::unwrap_used)]

use openracing_diagnostic::{
    BlackboxConfig, BlackboxRecorder, BlackboxReplay, FrameData, HealthEventData, ReplayConfig,
    SafetyStateSimple, SupportBundle, SupportBundleConfig, TelemetryData,
};
use std::fs;
use tempfile::TempDir;

fn create_test_frame(i: usize) -> FrameData {
    FrameData {
        ffb_in: (i as f32 * 0.01).sin(),
        torque_out: (i as f32 * 0.005).sin(),
        wheel_speed: 50.0 + 20.0 * (i as f32 * 0.1).sin(),
        hands_off: i % 100 < 5,
        ts_mono_ns: (i * 1_000_000) as u64,
        seq: i as u16,
    }
}

fn create_test_telemetry(i: usize) -> TelemetryData {
    TelemetryData {
        ffb_scalar: 0.8,
        rpm: 3000.0 + 2000.0 * (i as f32 * 0.1).sin(),
        speed_ms: 25.0 + 10.0 * (i as f32 * 0.15).cos(),
        slip_ratio: 0.1 * (i as f32 * 0.2).sin().abs(),
        gear: ((i % 6) + 1) as i8,
        car_id: Some(format!("car_{}", i % 5)),
        track_id: Some("test_track".to_string()),
    }
}

fn create_test_health_event(i: usize) -> HealthEventData {
    let event_type = match i % 4 {
        0 => "DeviceConnected",
        1 => "SafetyFault",
        2 => "PerformanceDegradation",
        _ => "ConfigurationChange",
    };

    HealthEventData {
        timestamp_ns: 0,
        device_id: "test-device".to_string(),
        event_type: event_type.to_string(),
        context: serde_json::json!({
            "test_iteration": i,
            "test_data": format!("event_{}", i)
        }),
    }
}

#[test]
fn test_complete_recording_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let config = BlackboxConfig {
        enable_stream_a: true,
        enable_stream_b: true,
        enable_stream_c: true,
        compression_level: 6,
        ..BlackboxConfig::new("test-device", temp_dir.path())
    };

    let mut recorder = BlackboxRecorder::new(config).unwrap();

    // Record frames
    for i in 0..100 {
        let frame = create_test_frame(i);
        recorder
            .record_frame(frame, &[0.1, 0.2, 0.3], SafetyStateSimple::SafeTorque, 100)
            .unwrap();
    }

    // Record telemetry
    for i in 0..60 {
        let telemetry = create_test_telemetry(i);
        recorder.record_telemetry(telemetry).unwrap();
    }

    // Record health events
    for i in 0..10 {
        let event = create_test_health_event(i);
        recorder.record_health_event(event).unwrap();
    }

    let stats = recorder.get_stats();
    assert_eq!(stats.frames_recorded, 100);

    let output_path = recorder.finalize().unwrap();
    assert!(output_path.exists());
    assert!(output_path.extension().unwrap() == "wbb");

    let metadata = fs::metadata(&output_path).unwrap();
    assert!(metadata.len() > 0);
}

#[test]
fn test_recording_and_replay_consistency() {
    let temp_dir = TempDir::new().unwrap();
    let config = BlackboxConfig {
        enable_stream_a: true,
        enable_stream_b: false,
        enable_stream_c: false,
        compression_level: 1,
        ..BlackboxConfig::new("test-device", temp_dir.path())
    };

    let mut recorder = BlackboxRecorder::new(config).unwrap();

    let mut recorded_frames = Vec::new();
    for i in 0..50 {
        let frame = create_test_frame(i);
        recorded_frames.push(frame.clone());
        recorder
            .record_frame(frame, &[0.1, 0.2], SafetyStateSimple::SafeTorque, 100)
            .unwrap();
    }

    let output_path = recorder.finalize().unwrap();

    let replay_config = ReplayConfig {
        deterministic_seed: 12345,
        fp_tolerance: 1e-6,
        validate_outputs: true,
        ..Default::default()
    };

    let mut replay = BlackboxReplay::load_from_file(&output_path, replay_config).unwrap();
    let result = replay.execute_replay().unwrap();

    assert_eq!(result.frames_replayed, 50);
    assert!(result.success);
}

#[test]
fn test_deterministic_replay() {
    let temp_dir = TempDir::new().unwrap();
    let config = BlackboxConfig::new("test-device", temp_dir.path());
    let mut recorder = BlackboxRecorder::new(config).unwrap();

    for i in 0..30 {
        let frame = create_test_frame(i);
        recorder
            .record_frame(frame, &[0.1], SafetyStateSimple::SafeTorque, 100)
            .unwrap();
    }

    let output_path = recorder.finalize().unwrap();

    let replay_config = ReplayConfig {
        deterministic_seed: 42,
        validate_outputs: true,
        ..Default::default()
    };

    // First replay
    let mut replay1 = BlackboxReplay::load_from_file(&output_path, replay_config.clone()).unwrap();
    let result1 = replay1.execute_replay().unwrap();

    // Second replay
    let mut replay2 = BlackboxReplay::load_from_file(&output_path, replay_config).unwrap();
    let result2 = replay2.execute_replay().unwrap();

    // Results should be identical
    assert_eq!(result1.frames_replayed, result2.frames_replayed);
    assert_eq!(result1.frames_matched, result2.frames_matched);
    assert_eq!(result1.max_deviation, result2.max_deviation);
}

#[test]
fn test_support_bundle_generation() {
    let temp_dir = TempDir::new().unwrap();

    // Create test directories
    let log_dir = temp_dir.path().join("logs");
    let profile_dir = temp_dir.path().join("profiles");
    let recording_dir = temp_dir.path().join("recordings");

    fs::create_dir_all(&log_dir).unwrap();
    fs::create_dir_all(&profile_dir).unwrap();
    fs::create_dir_all(&recording_dir).unwrap();

    // Create test log files
    fs::write(log_dir.join("app.log"), "Test log content\n").unwrap();
    fs::write(log_dir.join("error.log"), "Error log content\n").unwrap();

    // Create test profile files
    fs::write(profile_dir.join("global.json"), r#"{"gain": 0.8}"#).unwrap();

    // Create test recording
    let blackbox_config = BlackboxConfig::new("test-device", &recording_dir);
    let mut recorder = BlackboxRecorder::new(blackbox_config).unwrap();

    for i in 0..20 {
        let frame = create_test_frame(i);
        recorder
            .record_frame(frame, &[], SafetyStateSimple::SafeTorque, 100)
            .unwrap();
    }
    recorder.finalize().unwrap();

    // Create support bundle
    let bundle_config = SupportBundleConfig {
        include_logs: true,
        include_profiles: true,
        include_system_info: true,
        include_recent_recordings: true,
        max_bundle_size_mb: 10,
    };

    let mut bundle = SupportBundle::new(bundle_config);

    // Add health events
    let events: Vec<_> = (0..5).map(create_test_health_event).collect();
    bundle.add_health_events(&events).unwrap();

    // Add system info
    bundle.add_system_info().unwrap();

    // Add log files
    bundle.add_log_files(&log_dir).unwrap();

    // Add profile files
    bundle.add_profile_files(&profile_dir).unwrap();

    // Add recordings
    bundle.add_recent_recordings(&recording_dir).unwrap();

    // Generate bundle
    let bundle_path = temp_dir.path().join("support_bundle.zip");
    bundle.generate(&bundle_path).unwrap();

    assert!(bundle_path.exists());

    let metadata = fs::metadata(&bundle_path).unwrap();
    assert!(metadata.len() > 0);
    assert!(metadata.len() < 10 * 1024 * 1024);
}

#[test]
fn test_compression_effectiveness() {
    let temp_dir = TempDir::new().unwrap();

    // Test with compression
    let config_compressed = BlackboxConfig {
        compression_level: 9,
        ..BlackboxConfig::new("test-device", temp_dir.path().join("compressed"))
    };

    fs::create_dir_all(temp_dir.path().join("compressed")).unwrap();
    let mut recorder_compressed = BlackboxRecorder::new(config_compressed).unwrap();

    // Test without compression
    let config_uncompressed = BlackboxConfig {
        compression_level: 0,
        ..BlackboxConfig::new("test-device", temp_dir.path().join("uncompressed"))
    };

    fs::create_dir_all(temp_dir.path().join("uncompressed")).unwrap();
    let mut recorder_uncompressed = BlackboxRecorder::new(config_uncompressed).unwrap();

    // Record identical data
    for i in 0..200 {
        let frame = create_test_frame(i);
        let node_outputs = vec![0.1, 0.2, 0.3];

        recorder_compressed
            .record_frame(
                frame.clone(),
                &node_outputs,
                SafetyStateSimple::SafeTorque,
                100,
            )
            .unwrap();
        recorder_uncompressed
            .record_frame(frame, &node_outputs, SafetyStateSimple::SafeTorque, 100)
            .unwrap();
    }

    let compressed_path = recorder_compressed.finalize().unwrap();
    let uncompressed_path = recorder_uncompressed.finalize().unwrap();

    let compressed_size = fs::metadata(&compressed_path).unwrap().len();
    let uncompressed_size = fs::metadata(&uncompressed_path).unwrap().len();

    // Compressed should be smaller
    assert!(compressed_size < uncompressed_size);
}

#[test]
fn test_size_limits() {
    let temp_dir = TempDir::new().unwrap();
    let config = BlackboxConfig {
        max_file_size_bytes: 1024, // Very small limit
        max_duration_s: 1,         // Also limit duration
        ..BlackboxConfig::new("test-device", temp_dir.path())
    };

    let mut recorder = BlackboxRecorder::new(config).unwrap();

    // Record frames - duration limit should trigger before size in this case
    for i in 0..100 {
        let frame = create_test_frame(i);
        let _ = recorder.record_frame(frame, &[0.1; 100], SafetyStateSimple::SafeTorque, 100);
    }

    // Finalize should work even with size limits configured
    let _ = recorder.finalize();
}

#[test]
fn test_format_validation() {
    let temp_dir = TempDir::new().unwrap();
    let config = BlackboxConfig::new("test-device", temp_dir.path());
    let mut recorder = BlackboxRecorder::new(config).unwrap();

    for i in 0..20 {
        let frame = create_test_frame(i);
        recorder
            .record_frame(frame, &[], SafetyStateSimple::SafeTorque, 100)
            .unwrap();
    }

    let output_path = recorder.finalize().unwrap();

    let replay_config = ReplayConfig::default();
    let replay = BlackboxReplay::load_from_file(&output_path, replay_config).unwrap();

    // Verify header
    assert_eq!(replay.header().magic, *b"WBB1");
    assert_eq!(replay.header().version, 1);
    assert!(!replay.header().device_id.is_empty());

    // Verify footer
    assert_eq!(replay.footer().footer_magic, *b"1BBW");
    assert!(replay.footer().total_frames > 0);
}

#[test]
fn test_empty_recording() {
    let temp_dir = TempDir::new().unwrap();
    let config = BlackboxConfig::new("test-device", temp_dir.path());

    let recorder = BlackboxRecorder::new(config).unwrap();
    let output_path = recorder.finalize().unwrap();

    assert!(output_path.exists());

    let replay_config = ReplayConfig::default();
    let mut replay = BlackboxReplay::load_from_file(&output_path, replay_config).unwrap();
    let result = replay.execute_replay().unwrap();

    assert_eq!(result.frames_replayed, 0);
    assert!(result.success);
}
