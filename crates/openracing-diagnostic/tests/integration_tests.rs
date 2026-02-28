//! Integration tests for diagnostic recording and replay
//!
//! Tests the complete lifecycle of recording, replay, and support bundle generation.

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
fn test_complete_recording_workflow() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let config = BlackboxConfig {
        enable_stream_a: true,
        enable_stream_b: true,
        enable_stream_c: true,
        compression_level: 6,
        ..BlackboxConfig::new("test-device", temp_dir.path())
    };

    let mut recorder = BlackboxRecorder::new(config)?;

    // Record frames
    for i in 0..100 {
        let frame = create_test_frame(i);
        recorder
            .record_frame(frame, &[0.1, 0.2, 0.3], SafetyStateSimple::SafeTorque, 100)?;
    }

    // Record telemetry
    for i in 0..60 {
        let telemetry = create_test_telemetry(i);
        recorder.record_telemetry(telemetry)?;
    }

    // Record health events
    for i in 0..10 {
        let event = create_test_health_event(i);
        recorder.record_health_event(event)?;
    }

    let stats = recorder.get_stats();
    assert_eq!(stats.frames_recorded, 100);

    let output_path = recorder.finalize()?;
    assert!(output_path.exists());
    assert!(output_path.extension().ok_or("no extension")? == "wbb");

    let metadata = fs::metadata(&output_path)?;
    assert!(metadata.len() > 0);
    Ok(())
}

#[test]
fn test_recording_and_replay_consistency() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let config = BlackboxConfig {
        enable_stream_a: true,
        enable_stream_b: false,
        enable_stream_c: false,
        compression_level: 1,
        ..BlackboxConfig::new("test-device", temp_dir.path())
    };

    let mut recorder = BlackboxRecorder::new(config)?;

    let mut recorded_frames = Vec::new();
    for i in 0..50 {
        let frame = create_test_frame(i);
        recorded_frames.push(frame.clone());
        recorder
            .record_frame(frame, &[0.1, 0.2], SafetyStateSimple::SafeTorque, 100)?;
    }

    let output_path = recorder.finalize()?;

    let replay_config = ReplayConfig {
        deterministic_seed: 12345,
        fp_tolerance: 1e-6,
        validate_outputs: true,
        ..Default::default()
    };

    let mut replay = BlackboxReplay::load_from_file(&output_path, replay_config)?;
    let result = replay.execute_replay()?;

    assert_eq!(result.frames_replayed, 50);
    assert!(result.success);
    Ok(())
}

#[test]
fn test_deterministic_replay() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let config = BlackboxConfig::new("test-device", temp_dir.path());
    let mut recorder = BlackboxRecorder::new(config)?;

    for i in 0..30 {
        let frame = create_test_frame(i);
        recorder
            .record_frame(frame, &[0.1], SafetyStateSimple::SafeTorque, 100)?;
    }

    let output_path = recorder.finalize()?;

    let replay_config = ReplayConfig {
        deterministic_seed: 42,
        validate_outputs: true,
        ..Default::default()
    };

    // First replay
    let mut replay1 = BlackboxReplay::load_from_file(&output_path, replay_config.clone())?;
    let result1 = replay1.execute_replay()?;

    // Second replay
    let mut replay2 = BlackboxReplay::load_from_file(&output_path, replay_config)?;
    let result2 = replay2.execute_replay()?;

    // Results should be identical
    assert_eq!(result1.frames_replayed, result2.frames_replayed);
    assert_eq!(result1.frames_matched, result2.frames_matched);
    assert_eq!(result1.max_deviation, result2.max_deviation);
    Ok(())
}

#[test]
fn test_support_bundle_generation() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;

    // Create test directories
    let log_dir = temp_dir.path().join("logs");
    let profile_dir = temp_dir.path().join("profiles");
    let recording_dir = temp_dir.path().join("recordings");

    fs::create_dir_all(&log_dir)?;
    fs::create_dir_all(&profile_dir)?;
    fs::create_dir_all(&recording_dir)?;

    // Create test log files
    fs::write(log_dir.join("app.log"), "Test log content\n")?;
    fs::write(log_dir.join("error.log"), "Error log content\n")?;

    // Create test profile files
    fs::write(profile_dir.join("global.json"), r#"{"gain": 0.8}"#)?;

    // Create test recording
    let blackbox_config = BlackboxConfig::new("test-device", &recording_dir);
    let mut recorder = BlackboxRecorder::new(blackbox_config)?;

    for i in 0..20 {
        let frame = create_test_frame(i);
        recorder
            .record_frame(frame, &[], SafetyStateSimple::SafeTorque, 100)?;
    }
    recorder.finalize()?;

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
    bundle.add_health_events(&events)?;

    // Add system info
    bundle.add_system_info()?;

    // Add log files
    bundle.add_log_files(&log_dir)?;

    // Add profile files
    bundle.add_profile_files(&profile_dir)?;

    // Add recordings
    bundle.add_recent_recordings(&recording_dir)?;

    // Generate bundle
    let bundle_path = temp_dir.path().join("support_bundle.zip");
    bundle.generate(&bundle_path)?;

    assert!(bundle_path.exists());

    let metadata = fs::metadata(&bundle_path)?;
    assert!(metadata.len() > 0);
    assert!(metadata.len() < 10 * 1024 * 1024);
    Ok(())
}

#[test]
fn test_compression_effectiveness() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;

    // Test with compression
    let config_compressed = BlackboxConfig {
        compression_level: 9,
        ..BlackboxConfig::new("test-device", temp_dir.path().join("compressed"))
    };

    fs::create_dir_all(temp_dir.path().join("compressed"))?;
    let mut recorder_compressed = BlackboxRecorder::new(config_compressed)?;

    // Test without compression
    let config_uncompressed = BlackboxConfig {
        compression_level: 0,
        ..BlackboxConfig::new("test-device", temp_dir.path().join("uncompressed"))
    };

    fs::create_dir_all(temp_dir.path().join("uncompressed"))?;
    let mut recorder_uncompressed = BlackboxRecorder::new(config_uncompressed)?;

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
            )?;
        recorder_uncompressed
            .record_frame(frame, &node_outputs, SafetyStateSimple::SafeTorque, 100)?;
    }

    let compressed_path = recorder_compressed.finalize()?;
    let uncompressed_path = recorder_uncompressed.finalize()?;

    let compressed_size = fs::metadata(&compressed_path)?.len();
    let uncompressed_size = fs::metadata(&uncompressed_path)?.len();

    // Compressed should be smaller
    assert!(compressed_size < uncompressed_size);
    Ok(())
}

#[test]
fn test_size_limits() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let config = BlackboxConfig {
        max_file_size_bytes: 1024, // Very small limit
        max_duration_s: 1,         // Also limit duration
        ..BlackboxConfig::new("test-device", temp_dir.path())
    };

    let mut recorder = BlackboxRecorder::new(config)?;

    // Record frames - duration limit should trigger before size in this case
    for i in 0..100 {
        let frame = create_test_frame(i);
        let _ = recorder.record_frame(frame, &[0.1; 100], SafetyStateSimple::SafeTorque, 100);
    }

    // Finalize should work even with size limits configured
    let _ = recorder.finalize();
    Ok(())
}

#[test]
fn test_format_validation() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let config = BlackboxConfig::new("test-device", temp_dir.path());
    let mut recorder = BlackboxRecorder::new(config)?;

    for i in 0..20 {
        let frame = create_test_frame(i);
        recorder
            .record_frame(frame, &[], SafetyStateSimple::SafeTorque, 100)?;
    }

    let output_path = recorder.finalize()?;

    let replay_config = ReplayConfig::default();
    let replay = BlackboxReplay::load_from_file(&output_path, replay_config)?;

    // Verify header
    assert_eq!(replay.header().magic, *b"WBB1");
    assert_eq!(replay.header().version, 1);
    assert!(!replay.header().device_id.is_empty());

    // Verify footer
    assert_eq!(replay.footer().footer_magic, *b"1BBW");
    assert!(replay.footer().total_frames > 0);
    Ok(())
}

#[test]
fn test_empty_recording() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let config = BlackboxConfig::new("test-device", temp_dir.path());

    let recorder = BlackboxRecorder::new(config)?;
    let output_path = recorder.finalize()?;

    assert!(output_path.exists());

    let replay_config = ReplayConfig::default();
    let mut replay = BlackboxReplay::load_from_file(&output_path, replay_config)?;
    let result = replay.execute_replay()?;

    assert_eq!(result.frames_replayed, 0);
    assert!(result.success);
    Ok(())
}
