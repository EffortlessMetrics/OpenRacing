//! Comprehensive tests for diagnostic and blackbox recording system
//!
//! Tests cover recording, compression, replay accuracy, and support bundle generation
//! as specified in task requirements.

#![allow(clippy::unwrap_used)]

use super::*;
use crate::{
    rt::Frame,
    safety::SafetyState,
    ports::NormalizedTelemetry,
};
use std::path::PathBuf;
use std::{
    fs::{create_dir_all, write},
    time::{SystemTime, Duration},
};
use tempfile::TempDir;

// Test helper functions to replace unwrap
#[track_caller]
fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
    match r {
        Ok(v) => v,
        Err(e) => panic!("unexpected Err: {e:?}"),
    }
}

#[track_caller]
fn must_parse<T: std::str::FromStr>(s: &str) -> T
where
    <T as std::str::FromStr>::Err: std::fmt::Debug,
{
    match s.parse::<T>() {
        Ok(v) => v,
        Err(e) => panic!("parse failed for {s:?}: {e:?}"),
    }
}

#[track_caller]
fn must_some<T>(o: Option<T>, msg: &str) -> T {
    match o {
        Some(v) => v,
        None => panic!("{msg}"),
    }
}

/// Create test diagnostic configuration
fn create_test_diagnostic_config(temp_dir: &TempDir) -> DiagnosticConfig {
    DiagnosticConfig {
        enable_recording: true,
        max_recording_duration_s: 30,
        recording_dir: temp_dir.path().join("recordings"),
        max_file_size_bytes: 5 * 1024 * 1024, // 5MB
        compression_level: 6,
        enable_stream_a: true,
        enable_stream_b: true,
        enable_stream_c: true,
    }
}

/// Create test blackbox configuration
fn create_test_blackbox_config(temp_dir: &TempDir) -> BlackboxConfig {
    BlackboxConfig {
        device_id: must_parse::<DeviceId>("test-device"),
        output_dir: temp_dir.path().join("recordings"),
        max_duration_s: 30,
        max_file_size_bytes: 5 * 1024 * 1024,
        compression_level: 6,
        enable_stream_a: true,
        enable_stream_b: true,
        enable_stream_c: true,
    }
}

/// Generate deterministic test frames
fn generate_test_frames(count: usize) -> Vec<(Frame, Vec<f32>, SafetyState, u64)> {
    let mut frames = Vec::new();
    
    for i in 0..count {
        let t = i as f32 * 0.001; // Time in seconds
        
        // Generate deterministic but varied data
        let frame = Frame {
            ffb_in: (t * 2.0 * std::f32::consts::PI).sin() * 0.8, // Sine wave input
            torque_out: (t * 2.0 * std::f32::consts::PI).sin() * 0.6, // Filtered output
            wheel_speed: 50.0 + 20.0 * (t * 0.5).sin(), // Varying wheel speed
            hands_off: (i % 100) < 5, // Hands off 5% of the time
            ts_mono_ns: (i as u64) * 1_000_000, // 1ms intervals
            seq: i as u16,
        };
        
        // Generate per-node outputs (simulating filter chain)
        let node_outputs = vec![
            frame.ffb_in * 0.9, // Reconstruction filter
            frame.ffb_in * 0.8, // Friction filter
            frame.torque_out,    // Final output
        ];
        
        let safety_state = if i % 50 == 0 {
            SafetyState::HighTorqueActive {
                since: std::time::Instant::now(),
                device_token: 12345,
                last_hands_on: std::time::Instant::now(),
            }
        } else {
            SafetyState::SafeTorque
        };
        
        let processing_time_us = 100 + (i % 50) as u64; // Varying processing time
        
        frames.push((frame, node_outputs, safety_state, processing_time_us));
    }
    
    frames
}

/// Generate test telemetry data
fn generate_test_telemetry(count: usize) -> Vec<NormalizedTelemetry> {
    let mut telemetry = Vec::new();
    
    for i in 0..count {
        let t = i as f32 * 0.1; // 10Hz telemetry
        
        let telem = NormalizedTelemetry {
            ffb_scalar: (t * 0.5).sin(),
            rpm: 3000.0 + 2000.0 * (t * 0.2).sin(),
            speed_ms: 25.0 + 10.0 * (t * 0.3).cos(),
            slip_ratio: 0.1 * (t * 2.0).sin().abs(),
            gear: ((t * 0.1) as i8 % 6) + 1,
            flags: Default::default(),
            car_id: Some(format!("car_{}", i % 5)),
            track_id: Some("test_track".to_string()),
            timestamp: std::time::Instant::now(),
        };
        
        telemetry.push(telem);
    }
    
    telemetry
}

/// Generate test health events
fn generate_test_health_events(count: usize) -> Vec<HealthEvent> {
    let device_id = must_parse::<DeviceId>("test-device");
    let mut events = Vec::new();
    
    for i in 0..count {
        let event_type = match i % 4 {
            0 => HealthEventType::DeviceConnected,
            1 => HealthEventType::SafetyFault { 
                fault_type: crate::safety::FaultType::ThermalLimit 
            },
            2 => HealthEventType::PerformanceDegradation { 
                metric: "jitter".to_string(), 
                value: (i as f64) * 0.001 
            },
            3 => HealthEventType::ConfigurationChange { 
                change_type: "profile_update".to_string() 
            },
        };
        
        let event = HealthEvent {
            timestamp: SystemTime::now(),
            device_id: device_id.clone(),
            event_type,
            context: serde_json::json!({
                "test_iteration": i,
                "test_data": format!("event_{}", i)
            }),
        };
        
        events.push(event);
    }
    
    events
}

#[tokio::test]
async fn test_complete_recording_workflow() {
    let temp_dir = must(TempDir::new());
    let config = create_test_diagnostic_config(&temp_dir);
    
    // Create diagnostic service
    let mut service = must(DiagnosticService::new(config));
    
    // Start recording
    let device_id = must_parse::<DeviceId>("test-device");
    must(service.start_recording(device_id));
    
    // Generate and record test data
    let frames = generate_test_frames(1000); // 1 second at 1kHz
    let telemetry = generate_test_telemetry(60); // 1 second at 60Hz
    let health_events = generate_test_health_events(10);
    
    // Record frames (Stream A)
    for (frame, node_outputs, safety_state, processing_time_us) in &frames {
        must(service.record_frame(frame, node_outputs, safety_state, *processing_time_us));
    }
    
    // Record telemetry (Stream B)
    for telem in &telemetry {
        must(service.record_telemetry(telem));
    }
    
    // Record health events (Stream C)
    for event in &health_events {
        service.record_health_event(event.clone());
    }
    
    // Check recording stats
    let stats = must_some(service.get_recording_stats(), "expected stats");
    assert_eq!(stats.frames_recorded, 1000);
    assert!(stats.is_active);
    
    // Stop recording
    let output_path = must(must(service.stop_recording()));
    assert!(output_path.exists());
    assert_eq!(must_some(output_path.extension(), "expected extension"), "wbb");
    
    // Verify file is not empty
    let metadata = must(std::fs::metadata(&output_path));
    assert!(metadata.len() > 1000); // Should have substantial content
}

#[tokio::test]
async fn test_blackbox_compression_effectiveness() {
    let temp_dir = must(TempDir::new());
    
    // Test with compression
    let mut config_compressed = create_test_blackbox_config(&temp_dir);
    config_compressed.compression_level = 9; // Maximum compression
    config_compressed.output_dir = temp_dir.path().join("compressed");
    must(create_dir_all(&config_compressed.output_dir));
    
    let mut recorder_compressed = must(BlackboxRecorder::new(config_compressed));
    
    // Test without compression
    let mut config_uncompressed = create_test_blackbox_config(&temp_dir);
    config_uncompressed.compression_level = 0; // No compression
    config_uncompressed.output_dir = temp_dir.path().join("uncompressed");
    must(create_dir_all(&config_uncompressed.output_dir));
    
    let mut recorder_uncompressed = must(BlackboxRecorder::new(config_uncompressed));
    
    // Record identical data to both
    let frames = generate_test_frames(500);
    
    for (frame, node_outputs, safety_state, processing_time_us) in &frames {
        must(recorder_compressed.record_frame(frame, node_outputs, safety_state, *processing_time_us));
        must(recorder_uncompressed.record_frame(frame, node_outputs, safety_state, *processing_time_us));
    }
    
    // Finalize both recordings
    let compressed_path = must(recorder_compressed.finalize());
    let uncompressed_path = must(recorder_uncompressed.finalize());
    
    // Compare file sizes
    let compressed_size = must(std::fs::metadata(&compressed_path)).len();
    let uncompressed_size = must(std::fs::metadata(&uncompressed_path)).len();
    
    // Compressed should be smaller (exact ratio depends on data patterns)
    assert!(compressed_size < uncompressed_size);
    
    // Compression ratio should be reasonable for repetitive data
    let compression_ratio = compressed_size as f64 / uncompressed_size as f64;
    assert!(compression_ratio < 0.8); // At least 20% compression
    
    println!("Compression ratio: {:.2}% ({} -> {} bytes)", 
              compression_ratio * 100.0, uncompressed_size, compressed_size);
}

#[tokio::test]
async fn test_replay_accuracy_and_determinism() {
    let temp_dir = must(TempDir::new());
    let config = create_test_blackbox_config(&temp_dir);
    
    // Create recording with deterministic data
    let mut recorder = must(BlackboxRecorder::new(config));
    
    let frames = generate_test_frames(200); // Smaller set for faster test
    
    for (frame, node_outputs, safety_state, processing_time_us) in &frames {
        must(recorder.record_frame(frame, node_outputs, safety_state, *processing_time_us));
    }
    
    let recording_path = must(recorder.finalize());
    
    // Test replay with strict tolerance
    let replay_config = ReplayConfig {
        deterministic_seed: 12345,
        fp_tolerance: 1e-6, // Very strict tolerance
        validate_outputs: true,
        strict_timing: false, // Don't enforce real-time in tests
        max_duration_s: 60,
    };
    
    // First replay
    let mut replay1 = must(BlackboxReplay::load_from_file(&recording_path, replay_config.clone()));
    let result1 = must(replay1.execute_replay());
    
    // Second replay with same seed
    let mut replay2 = must(BlackboxReplay::load_from_file(&recording_path, replay_config));
    let result2 = must(replay2.execute_replay());
    
    // Results should be identical
    assert_eq!(result1.frames_replayed, result2.frames_replayed);
    assert_eq!(result1.frames_matched, result2.frames_matched);
    assert_eq!(result1.max_deviation, result2.max_deviation);
    
    // Should have high match rate (allowing for some floating-point variance)
    assert!(result1.frames_matched as f64 / result1.frames_replayed as f64 > 0.95);
    
    // Verify frame-by-frame determinism
    let comparisons1 = replay1.get_frame_comparisons();
    let comparisons2 = replay2.get_frame_comparisons();
    
    assert_eq!(comparisons1.len(), comparisons2.len());
    
    for (c1, c2) in comparisons1.iter().zip(comparisons2.iter()) {
        assert_eq!(c1.replayed_output, c2.replayed_output);
        assert_eq!(c1.deviation, c2.deviation);
        assert_eq!(c1.within_tolerance, c2.within_tolerance);
    }
    
    println!("Replay accuracy: {}/{} frames matched ({:.2}%)", 
              result1.frames_matched, result1.frames_replayed,
              (result1.frames_matched as f64 / result1.frames_replayed as f64) * 100.0);
    println!("Max deviation: {:.2e}, Avg deviation: {:.2e}", 
              result1.max_deviation, result1.avg_deviation);
}

#[tokio::test]
async fn test_support_bundle_generation_complete() {
    let temp_dir = must(TempDir::new());
    
    // Create test environment with logs, profiles, and recordings
    let log_dir = temp_dir.path().join("logs");
    let profile_dir = temp_dir.path().join("profiles");
    let recording_dir = temp_dir.path().join("recordings");
    
    must(create_dir_all(&log_dir));
    must(create_dir_all(&profile_dir));
    must(create_dir_all(&recording_dir));
    
    // Create test log files
    must(write(log_dir.join("app.log"), r#"Application log content\nINFO: System started\nWARN: Minor issue detected"#);
    must(write(log_dir.join("error.log"), r#"ERROR: Test error message\nERROR: Another error"#);
    
    // Create test profile files
    must(write(profile_dir.join("global.profile.json"), r#"{"ffb_gain": 0.8, "dor_deg": 900}"#));
    must(write(profile_dir.join("iracing.json"), r#"{"game": "iracing", "settings": {}}"#));
    
    // Create test recording file
    let blackbox_config = BlackboxConfig {
        device_id: must_parse::<DeviceId>("test-device"),
        output_dir: recording_dir.clone(),
        max_duration_s: 10,
        max_file_size_bytes: 1024 * 1024,
        compression_level: 1,
        enable_stream_a: true,
        enable_stream_b: false,
        enable_stream_c: false,
    };
    
    let mut recorder = must(BlackboxRecorder::new(blackbox_config));
    let frames = generate_test_frames(50);
    for (frame, node_outputs, safety_state, processing_time_us) in &frames {
        must(recorder.record_frame(frame, node_outputs, safety_state, *processing_time_us));
    }
    must(recorder.finalize());
    
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
    let health_events = generate_test_health_events(5);
    must(bundle.add_health_events(&health_events));
    
    // Add system info
    must(bundle.add_system_info());
    
    // Add log files
    must(bundle.add_log_files(&log_dir));
    
    // Add profile files
    must(bundle.add_profile_files(&profile_dir));
    
    // Add recordings
    must(bundle.add_recent_recordings(&recording_dir));
    
    // Generate bundle
    let bundle_path = temp_dir.path().join("support_bundle.zip");
    must(bundle.generate(&bundle_path));
    
    // Verify bundle was created
    assert!(bundle_path.exists());
    
    // Check bundle size is reasonable
    let bundle_size = must(std::fs::metadata(&bundle_path)).len();
    assert!(bundle_size > 1000); // Should have substantial content
    assert!(bundle_size < 10 * 1024 * 1024); // Should be under 10MB limit
    
    // Verify bundle size estimation
    let estimated_size_mb = bundle.estimated_size_mb();
    assert!(estimated_size_mb > 0.0);
    assert!(estimated_size_mb < 10.0);
    
    println!("Support bundle size: {:.2} MB (estimated: {:.2} MB)", 
              bundle_size as f64 / 1024.0 / 1024.0, estimated_size_mb);
}

#[tokio::test]
async fn test_stream_rate_limiting_and_accuracy() {
    let temp_dir = must(TempDir::new());
    let config = create_test_blackbox_config(&temp_dir);
    
    let mut recorder = must(BlackboxRecorder::new(config));
    
    // Test Stream B rate limiting (should limit to ~60Hz)
    let telemetry_data = generate_test_telemetry(1000); // Way more than 60Hz worth
    
    let start_time = std::time::Instant::now();
    
    for telem in &telemetry_data {
        must(recorder.record_telemetry(telem));
        // Simulate rapid telemetry updates
        std::thread::sleep(Duration::from_micros(100)); // 10kHz rate
    }
    
    let elapsed = start_time.elapsed();
    
    // Should have rate-limited telemetry
    let stats = recorder.get_stats();
    
    // With 60Hz rate limiting, should have much fewer records than input
    assert!(stats.telemetry_records < telemetry_data.len() as u64);
    
    // Should be approximately 60Hz * elapsed_seconds
    let expected_records = (60.0 * elapsed.as_secs_f64()) as u64;
    let tolerance = expected_records / 4; // 25% tolerance
    
    assert!(stats.telemetry_records >= expected_records.saturating_sub(tolerance));
    assert!(stats.telemetry_records <= expected_records + tolerance);
    
    println!("Telemetry rate limiting: {} records in {:.2}s (expected ~{})", 
              stats.telemetry_records, elapsed.as_secs_f64(), expected_records);
}

#[tokio::test]
async fn test_error_handling_and_recovery() {
    let temp_dir = must(TempDir::new());
    
    // Test with invalid directory
    let mut invalid_config = create_test_diagnostic_config(&temp_dir);
    invalid_config.recording_dir = PathBuf::from("/invalid/path/that/does/not/exist");
    
    let result = DiagnosticService::new(invalid_config);
    assert!(result.is_err());
    
    // Test size limit enforcement
    let mut size_limited_config = create_test_diagnostic_config(&temp_dir);
    size_limited_config.max_file_size_bytes = 1024; // Very small limit
    
    let mut service = must(DiagnosticService::new(size_limited_config));
    let device_id = must_parse::<DeviceId>("test-device");
    must(service.start_recording(device_id));
    
    // Try to record more data than the limit allows
    let large_frames = generate_test_frames(10000); // Should exceed 1KB limit
    
    let mut error_encountered = false;
    for (frame, node_outputs, safety_state, processing_time_us) in &large_frames {
        if let Err(_) = service.record_frame(frame, node_outputs, safety_state, *processing_time_us) {
            error_encountered = true;
            break;
        }
    }
    
    // Should have hit size limit
    assert!(error_encountered);
}

#[tokio::test]
async fn test_wbb_format_validation() {
    let temp_dir = must(TempDir::new());
    let config = create_test_blackbox_config(&temp_dir);
    
    let mut recorder = must(BlackboxRecorder::new(config));
    
    // Record some data
    let frames = generate_test_frames(100);
    for (frame, node_outputs, safety_state, processing_time_us) in &frames {
        must(recorder.record_frame(frame, node_outputs, safety_state, *processing_time_us));
    }
    
    let recording_path = must(recorder.finalize());
    
    // Validate file format by attempting to load it
    let replay_config = ReplayConfig::default();
    let replay_result = BlackboxReplay::load_from_file(&recording_path, replay_config);
    
    assert!(replay_result.is_ok());
    
    let replay = must(replay_result);
    
    // Verify header information
    assert_eq!(replay.header().magic, *b"WBB1");
    assert_eq!(replay.header().version, 1);
    assert!(!replay.header().device_id.is_empty());
    assert!(!replay.header().engine_version.is_empty());
    
    // Verify footer information
    assert_eq!(replay.footer().footer_magic, *b"1BBW");
    assert!(replay.footer().total_frames > 0);
    assert!(replay.footer().duration_ms > 0);
    
    // Verify we have stream data
    assert!(!replay.stream_a_data().is_empty());
}

#[tokio::test]
async fn test_performance_under_load() {
    let temp_dir = must(TempDir::new());
    let config = create_test_blackbox_config(&temp_dir);
    
    let mut service = must(DiagnosticService::new(config));
    let device_id = must_parse::<DeviceId>("test-device");
    must(service.start_recording(device_id));
    
    // Simulate high-frequency recording (1kHz for 1 second)
    let frames = generate_test_frames(1000);
    
    let start_time = std::time::Instant::now();
    
    for (frame, node_outputs, safety_state, processing_time_us) in &frames {
        must(service.record_frame(frame, node_outputs, safety_state, *processing_time_us));
    }
    
    let recording_time = start_time.elapsed();
    
    // Should be able to record 1000 frames quickly (well under 1 second)
    assert!(recording_time < Duration::from_millis(500));
    
    // Verify all frames were recorded
    let stats = must_some(service.get_recording_stats(), "expected stats");
    assert_eq!(stats.frames_recorded, 1000);
    
    println!("Performance: Recorded {} frames in {:.2}ms ({:.0} fps)", 
              stats.frames_recorded, recording_time.as_millis(),
              1000.0 / recording_time.as_secs_f64());
}

/// Integration test that exercises the complete diagnostic workflow
#[tokio::test]
async fn test_end_to_end_diagnostic_workflow() {
    let temp_dir = must(TempDir::new());
    let config = create_test_diagnostic_config(&temp_dir);
    
    // Phase 1: Recording
    let mut service = must(DiagnosticService::new(config));
    let device_id = must_parse::<DeviceId>("test-device");
    
    must(service.start_recording(device_id));
    
    // Record comprehensive test data
    let frames = generate_test_frames(500);
    let telemetry = generate_test_telemetry(30);
    let health_events = generate_test_health_events(8);
    
    for (frame, node_outputs, safety_state, processing_time_us) in &frames {
        must(service.record_frame(frame, node_outputs, safety_state, *processing_time_us));
    }
    
    for telem in &telemetry {
        must(service.record_telemetry(telem));
    }
    
    for event in &health_events {
        service.record_health_event(event.clone());
    }
    
    let recording_path = must(must(service.stop_recording()));
    
    // Phase 2: Replay and Validation
    let replay_config = ReplayConfig {
        deterministic_seed: 42,
        fp_tolerance: 1e-5,
        validate_outputs: true,
        strict_timing: false,
        max_duration_s: 60,
    };
    
    let mut replay = must(BlackboxReplay::load_from_file(&recording_path, replay_config));
    let replay_result = must(replay.execute_replay());
    
    assert!(replay_result.success);
    assert_eq!(replay_result.frames_replayed, 500);
    assert!(replay_result.frames_matched > 450); // Allow some tolerance
    
    // Phase 3: Support Bundle Generation
    let bundle_config = SupportBundleConfig::default();
    let mut bundle = SupportBundle::new(bundle_config);
    
    must(bundle.add_health_events(&health_events));
    must(bundle.add_system_info());
    
    let bundle_path = temp_dir.path().join("complete_support_bundle.zip");
    must(bundle.generate(&bundle_path));
    
    // Verify complete workflow
    assert!(recording_path.exists());
    assert!(bundle_path.exists());
    
    let recording_size = must(std::fs::metadata(&recording_path)).len();
    let bundle_size = must(std::fs::metadata(&bundle_path)).len();
    
    assert!(recording_size > 1000);
    assert!(bundle_size > 1000);
    assert!(bundle_size < 10 * 1024 * 1024); // Should be under 10MB limit
    
    let estimated_size_mb = bundle.estimated_size_mb();
    assert!(estimated_size_mb > 0.0);
    assert!(estimated_size_mb < 10.0);
    
    println!("End-to-end test completed:");
    println!("  Recording: {} bytes", recording_size);
    println!("  Bundle: {} bytes", bundle_size);
    println!("Replay accuracy: {}/{} frames matched ({:.1}%)", 
              replay_result.frames_matched, replay_result.frames_replayed,
              (replay_result.frames_matched as f64 / replay_result.frames_replayed as f64) * 100.0);
}
