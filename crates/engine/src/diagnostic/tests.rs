//! Comprehensive tests for diagnostic and blackbox recording system
//!
//! Tests cover recording, compression, replay accuracy, and support bundle generation
//! as specified in the task requirements.

use super::*;
use crate::{
    rt::Frame,
    safety::SafetyState,
    ports::NormalizedTelemetry,
};
use racing_wheel_schemas::DeviceId;
use std::path::PathBuf;
use std::{
    fs::{create_dir_all, write},
    time::{SystemTime, Duration},
};
use tempfile::TempDir;

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
        device_id: DeviceId::new("test-device".to_string()).unwrap(),
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
            frame.ffb_in * 0.7, // Damper filter
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
    let device_id = DeviceId::new("test-device".to_string()).unwrap();
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
            _ => HealthEventType::ConfigurationChange { 
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
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_diagnostic_config(&temp_dir);
    
    // Create diagnostic service
    let mut service = DiagnosticService::new(config).unwrap();
    
    // Start recording
    let device_id = DeviceId::new("test-device".to_string()).unwrap();
    service.start_recording(device_id).unwrap();
    
    // Generate and record test data
    let frames = generate_test_frames(1000); // 1 second at 1kHz
    let telemetry = generate_test_telemetry(60); // 1 second at 60Hz
    let health_events = generate_test_health_events(10);
    
    // Record frames (Stream A)
    for (frame, node_outputs, safety_state, processing_time_us) in &frames {
        service.record_frame(frame, node_outputs, safety_state, *processing_time_us).unwrap();
    }
    
    // Record telemetry (Stream B)
    for telem in &telemetry {
        service.record_telemetry(telem).unwrap();
    }
    
    // Record health events (Stream C)
    for event in &health_events {
        service.record_health_event(event.clone());
    }
    
    // Check recording stats
    let stats = service.get_recording_stats().unwrap();
    assert_eq!(stats.frames_recorded, 1000);
    assert!(stats.is_active);
    
    // Stop recording
    let output_path = service.stop_recording().unwrap().unwrap();
    assert!(output_path.exists());
    assert!(output_path.extension().unwrap() == "wbb");
    
    // Verify file is not empty
    let metadata = std::fs::metadata(&output_path).unwrap();
    assert!(metadata.len() > 1000); // Should have substantial content
}

#[tokio::test]
async fn test_blackbox_compression_effectiveness() {
    let temp_dir = TempDir::new().unwrap();
    
    // Test with compression
    let mut config_compressed = create_test_blackbox_config(&temp_dir);
    config_compressed.compression_level = 9; // Maximum compression
    config_compressed.output_dir = temp_dir.path().join("compressed");
    create_dir_all(&config_compressed.output_dir).unwrap();
    
    let mut recorder_compressed = BlackboxRecorder::new(config_compressed).unwrap();
    
    // Test without compression
    let mut config_uncompressed = create_test_blackbox_config(&temp_dir);
    config_uncompressed.compression_level = 0; // No compression
    config_uncompressed.output_dir = temp_dir.path().join("uncompressed");
    create_dir_all(&config_uncompressed.output_dir).unwrap();
    
    let mut recorder_uncompressed = BlackboxRecorder::new(config_uncompressed).unwrap();
    
    // Record identical data to both
    let frames = generate_test_frames(500);
    
    for (frame, node_outputs, safety_state, processing_time_us) in &frames {
        recorder_compressed.record_frame(frame, node_outputs, safety_state, *processing_time_us).unwrap();
        recorder_uncompressed.record_frame(frame, node_outputs, safety_state, *processing_time_us).unwrap();
    }
    
    // Finalize both recordings
    let compressed_path = recorder_compressed.finalize().unwrap();
    let uncompressed_path = recorder_uncompressed.finalize().unwrap();
    
    // Compare file sizes
    let compressed_size = std::fs::metadata(&compressed_path).unwrap().len();
    let uncompressed_size = std::fs::metadata(&uncompressed_path).unwrap().len();
    
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
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_blackbox_config(&temp_dir);
    
    // Create recording with deterministic data
    let mut recorder = BlackboxRecorder::new(config).unwrap();
    
    let frames = generate_test_frames(200); // Smaller set for faster test
    
    for (frame, node_outputs, safety_state, processing_time_us) in &frames {
        recorder.record_frame(frame, node_outputs, safety_state, *processing_time_us).unwrap();
    }
    
    let recording_path = recorder.finalize().unwrap();
    
    // Test replay with strict tolerance
    let replay_config = ReplayConfig {
        deterministic_seed: 12345,
        fp_tolerance: 1e-6, // Very strict tolerance
        validate_outputs: true,
        strict_timing: false, // Don't enforce real-time in tests
        max_duration_s: 60,
    };
    
    // First replay
    let mut replay1 = BlackboxReplay::load_from_file(&recording_path, replay_config.clone()).unwrap();
    let result1 = replay1.execute_replay().unwrap();
    
    // Second replay with same seed
    let mut replay2 = BlackboxReplay::load_from_file(&recording_path, replay_config).unwrap();
    let result2 = replay2.execute_replay().unwrap();
    
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
    let temp_dir = TempDir::new().unwrap();
    
    // Create test environment with logs, profiles, and recordings
    let log_dir = temp_dir.path().join("logs");
    let profile_dir = temp_dir.path().join("profiles");
    let recording_dir = temp_dir.path().join("recordings");
    
    create_dir_all(&log_dir).unwrap();
    create_dir_all(&profile_dir).unwrap();
    create_dir_all(&recording_dir).unwrap();
    
    // Create test log files
    write(log_dir.join("app.log"), "Application log content\nINFO: System started\nWARN: Minor issue detected").unwrap();
    write(log_dir.join("error.log"), "ERROR: Test error message\nERROR: Another error").unwrap();
    
    // Create test profile files
    write(profile_dir.join("global.profile.json"), r#"{"ffb_gain": 0.8, "dor_deg": 900}"#).unwrap();
    write(profile_dir.join("iracing.json"), r#"{"game": "iracing", "settings": {}}"#).unwrap();
    
    // Create test recording file
    let blackbox_config = BlackboxConfig {
        device_id: DeviceId::new("test-device".to_string()).unwrap(),
        output_dir: recording_dir.clone(),
        max_duration_s: 10,
        max_file_size_bytes: 1024 * 1024,
        compression_level: 1,
        enable_stream_a: true,
        enable_stream_b: false,
        enable_stream_c: false,
    };
    
    let mut recorder = BlackboxRecorder::new(blackbox_config).unwrap();
    let frames = generate_test_frames(50);
    for (frame, node_outputs, safety_state, processing_time_us) in &frames {
        recorder.record_frame(frame, node_outputs, safety_state, *processing_time_us).unwrap();
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
    let health_events = generate_test_health_events(5);
    bundle.add_health_events(&health_events).unwrap();
    
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
    
    // Verify bundle was created
    assert!(bundle_path.exists());
    
    // Check bundle size is reasonable
    let bundle_size = std::fs::metadata(&bundle_path).unwrap().len();
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
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_blackbox_config(&temp_dir);
    
    let mut recorder = BlackboxRecorder::new(config).unwrap();
    
    // Test Stream B rate limiting (should limit to ~60Hz)
    let telemetry_data = generate_test_telemetry(1000); // Way more than 60Hz worth
    
    let start_time = std::time::Instant::now();
    
    for telem in &telemetry_data {
        recorder.record_telemetry(telem).unwrap();
        // Simulate rapid telemetry updates
        std::thread::sleep(Duration::from_micros(100)); // 10kHz rate
    }
    
    let elapsed = start_time.elapsed();
    
    // Should have rate-limited the telemetry
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
    let temp_dir = TempDir::new().unwrap();
    
    // Test with invalid directory
    let mut invalid_config = create_test_diagnostic_config(&temp_dir);
    invalid_config.recording_dir = PathBuf::from("/invalid/path/that/does/not/exist");
    
    let result = DiagnosticService::new(invalid_config);
    assert!(result.is_err());
    
    // Test size limit enforcement
    let mut size_limited_config = create_test_diagnostic_config(&temp_dir);
    size_limited_config.max_file_size_bytes = 1024; // Very small limit
    
    let mut service = DiagnosticService::new(size_limited_config).unwrap();
    let device_id = DeviceId::new("test-device".to_string()).unwrap();
    service.start_recording(device_id).unwrap();
    
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
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_blackbox_config(&temp_dir);
    
    let mut recorder = BlackboxRecorder::new(config).unwrap();
    
    // Record some data
    let frames = generate_test_frames(100);
    for (frame, node_outputs, safety_state, processing_time_us) in &frames {
        recorder.record_frame(frame, node_outputs, safety_state, *processing_time_us).unwrap();
    }
    
    let recording_path = recorder.finalize().unwrap();
    
    // Validate file format by attempting to load it
    let replay_config = ReplayConfig::default();
    let replay_result = BlackboxReplay::load_from_file(&recording_path, replay_config);
    
    assert!(replay_result.is_ok());
    
    let replay = replay_result.unwrap();
    
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
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_diagnostic_config(&temp_dir);
    
    let mut service = DiagnosticService::new(config).unwrap();
    let device_id = DeviceId::new("test-device".to_string()).unwrap();
    service.start_recording(device_id).unwrap();
    
    // Simulate high-frequency recording (1kHz for 1 second)
    let frames = generate_test_frames(1000);
    
    let start_time = std::time::Instant::now();
    
    for (frame, node_outputs, safety_state, processing_time_us) in &frames {
        service.record_frame(frame, node_outputs, safety_state, *processing_time_us).unwrap();
    }
    
    let recording_time = start_time.elapsed();
    
    // Should be able to record 1000 frames quickly (well under 1 second)
    assert!(recording_time < Duration::from_millis(500));
    
    // Verify all frames were recorded
    let stats = service.get_recording_stats().unwrap();
    assert_eq!(stats.frames_recorded, 1000);
    
    println!("Performance: Recorded {} frames in {:.2}ms ({:.0} fps)", 
             stats.frames_recorded, recording_time.as_millis(),
             1000.0 / recording_time.as_secs_f64());
}

/// Integration test that exercises the complete diagnostic workflow
#[tokio::test]
async fn test_end_to_end_diagnostic_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_diagnostic_config(&temp_dir);
    
    // Phase 1: Recording
    let mut service = DiagnosticService::new(config).unwrap();
    let device_id = DeviceId::new("test-device".to_string()).unwrap();
    
    service.start_recording(device_id).unwrap();
    
    // Record comprehensive test data
    let frames = generate_test_frames(500);
    let telemetry = generate_test_telemetry(30);
    let health_events = generate_test_health_events(8);
    
    for (frame, node_outputs, safety_state, processing_time_us) in &frames {
        service.record_frame(frame, node_outputs, safety_state, *processing_time_us).unwrap();
    }
    
    for telem in &telemetry {
        service.record_telemetry(telem).unwrap();
    }
    
    for event in &health_events {
        service.record_health_event(event.clone());
    }
    
    let recording_path = service.stop_recording().unwrap().unwrap();
    
    // Phase 2: Replay and Validation
    let replay_config = ReplayConfig {
        deterministic_seed: 42,
        fp_tolerance: 1e-5,
        validate_outputs: true,
        strict_timing: false,
        max_duration_s: 60,
    };
    
    let mut replay = BlackboxReplay::load_from_file(&recording_path, replay_config).unwrap();
    let replay_result = replay.execute_replay().unwrap();
    
    assert!(replay_result.success);
    assert_eq!(replay_result.frames_replayed, 500);
    assert!(replay_result.frames_matched > 450); // Allow some tolerance
    
    // Phase 3: Support Bundle Generation
    let bundle_config = SupportBundleConfig::default();
    let mut bundle = SupportBundle::new(bundle_config);
    
    bundle.add_health_events(&health_events).unwrap();
    bundle.add_system_info().unwrap();
    
    let bundle_path = temp_dir.path().join("complete_support_bundle.zip");
    bundle.generate(&bundle_path).unwrap();
    
    // Verify complete workflow
    assert!(recording_path.exists());
    assert!(bundle_path.exists());
    
    let recording_size = std::fs::metadata(&recording_path).unwrap().len();
    let bundle_size = std::fs::metadata(&bundle_path).unwrap().len();
    
    assert!(recording_size > 1000);
    assert!(bundle_size > 1000);
    
    println!("End-to-end test completed:");
    println!("  Recording: {} bytes", recording_size);
    println!("  Bundle: {} bytes", bundle_size);
    println!("  Replay accuracy: {}/{} frames ({:.1}%)", 
             replay_result.frames_matched, replay_result.frames_replayed,
             (replay_result.frames_matched as f64 / replay_result.frames_replayed as f64) * 100.0);
}