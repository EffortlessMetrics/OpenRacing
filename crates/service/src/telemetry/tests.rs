//! Comprehensive tests for telemetry adapters
//! 
//! Tests all telemetry adapters with recorded game data for validation
//! Requirements: GI-03, GI-04

use crate::telemetry::*;
use std::time::Duration;
use tempfile::tempdir;

#[tokio::test]
async fn test_telemetry_service_creation() {
    let service = TelemetryService::new();
    
    let supported_games = service.supported_games();
    assert!(supported_games.contains(&"iracing".to_string()));
    assert!(supported_games.contains(&"acc".to_string()));
}

#[tokio::test]
async fn test_telemetry_service_monitoring() {
    let mut service = TelemetryService::new();
    
    // Test starting monitoring for unsupported game
    let result = service.start_monitoring("unsupported_game").await;
    assert!(result.is_err());
    
    // Test checking if games are running
    let iracing_running = service.is_game_running("iracing").await;
    assert!(iracing_running.is_ok());
    
    let acc_running = service.is_game_running("acc").await;
    assert!(acc_running.is_ok());
}

#[tokio::test]
async fn test_iracing_adapter() {
    let adapter = IRacingAdapter::new();
    
    assert_eq!(adapter.game_id(), "iracing");
    assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    
    // Test game running check
    let is_running = adapter.is_game_running().await;
    assert!(is_running.is_ok());
}

#[tokio::test]
async fn test_acc_adapter() {
    let adapter = ACCAdapter::new();
    
    assert_eq!(adapter.game_id(), "acc");
    assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    
    // Test game running check
    let is_running = adapter.is_game_running().await;
    assert!(is_running.is_ok());
}

#[tokio::test]
async fn test_mock_adapter_telemetry_stream() {
    let mut adapter = MockAdapter::new("test_game".to_string());
    adapter.set_running(true);
    
    let mut receiver = adapter.start_monitoring().await.unwrap();
    
    // Should receive telemetry frames
    let frame = tokio::time::timeout(
        Duration::from_millis(100),
        receiver.recv()
    ).await.unwrap().unwrap();
    
    assert!(frame.data.rpm.is_some());
    assert!(frame.data.speed_ms.is_some());
    assert!(frame.data.ffb_scalar.is_some());
    assert_eq!(frame.data.car_id, Some("mock_car".to_string()));
    assert_eq!(frame.data.track_id, Some("mock_track".to_string()));
}

#[test]
fn test_rate_limiter_functionality() {
    let mut limiter = RateLimiter::new(100); // 100 Hz
    
    // First call should be allowed
    assert!(limiter.should_process());
    assert_eq!(limiter.processed_count(), 1);
    
    // Immediate second call should be dropped
    assert!(!limiter.should_process());
    assert_eq!(limiter.dropped_count(), 1);
    
    // Check statistics
    assert_eq!(limiter.drop_rate_percent(), 50.0);
}

#[tokio::test]
async fn test_rate_limiter_async() {
    let mut limiter = RateLimiter::new(1000); // 1000 Hz = 1ms interval
    
    let start = std::time::Instant::now();
    
    // First call should be immediate
    limiter.wait_for_slot().await;
    let first_elapsed = start.elapsed();
    
    // Second call should wait
    limiter.wait_for_slot().await;
    let second_elapsed = start.elapsed();
    
    // Should have waited at least close to the minimum interval
    assert!(second_elapsed >= first_elapsed + Duration::from_micros(800)); // Allow some tolerance
}

#[test]
fn test_adaptive_rate_limiter() {
    let mut adaptive = AdaptiveRateLimiter::new(1000, 50.0);
    
    // Test CPU usage adjustment
    adaptive.update_cpu_usage(80.0); // High CPU
    let stats_high = adaptive.stats();
    
    adaptive.update_cpu_usage(20.0); // Low CPU
    let stats_low = adaptive.stats();
    
    // Rate should be adjusted based on CPU usage
    assert!(stats_low.max_rate_hz >= stats_high.max_rate_hz);
}

#[test]
fn test_telemetry_recording_and_playback() {
    let temp_dir = tempdir().unwrap();
    let output_path = temp_dir.path().join("test_recording.json");
    
    // Create and record telemetry
    let mut recorder = TelemetryRecorder::new(output_path.clone()).unwrap();
    recorder.start_recording("test_game".to_string());
    
    // Record multiple frames
    for i in 0..10 {
        let telemetry = NormalizedTelemetry::new()
            .with_rpm(5000.0 + i as f32 * 100.0)
            .with_speed_ms(30.0 + i as f32 * 2.0)
            .with_gear(3);
        
        let frame = TelemetryFrame::new(telemetry, i * 16_000_000, i, 64);
        recorder.record_frame(frame);
    }
    
    let recording = recorder.stop_recording(Some("Test recording".to_string())).unwrap();
    assert_eq!(recording.frames.len(), 10);
    
    // Test playback
    let mut player = TelemetryPlayer::new(recording);
    player.start_playback();
    
    let mut frame_count = 0;
    while let Some(_frame) = player.get_next_frame() {
        frame_count += 1;
        if frame_count >= 10 {
            break; // Prevent infinite loop in test
        }
    }
    
    assert!(frame_count > 0);
}

#[test]
fn test_synthetic_fixture_generation() {
    // Test different scenarios
    let scenarios = [
        TestScenario::ConstantSpeed,
        TestScenario::Acceleration,
        TestScenario::Cornering,
        TestScenario::PitStop,
    ];
    
    for scenario in scenarios {
        let recording = TestFixtureGenerator::generate_test_scenario(
            scenario,
            2.0, // 2 seconds
            30.0, // 30 FPS
        );
        
        assert_eq!(recording.frames.len(), 60); // 2 * 30
        assert!(recording.metadata.description.is_some());
        
        // Verify all frames have valid data
        for frame in &recording.frames {
            assert!(frame.data.rpm.is_some());
            assert!(frame.data.speed_ms.is_some());
            assert!(frame.data.gear.is_some());
        }
    }
}

#[test]
fn test_normalized_telemetry_validation() {
    // Test value clamping and validation
    let telemetry = NormalizedTelemetry::new()
        .with_ffb_scalar(1.5) // Should be clamped to 1.0
        .with_rpm(-100.0) // Should be rejected (negative)
        .with_speed_ms(f32::NAN) // Should be rejected (NaN)
        .with_slip_ratio(2.0); // Should be clamped to 1.0
    
    assert_eq!(telemetry.ffb_scalar, Some(1.0));
    assert_eq!(telemetry.rpm, None);
    assert_eq!(telemetry.speed_ms, None);
    assert_eq!(telemetry.slip_ratio, Some(1.0));
}

#[test]
fn test_telemetry_field_coverage() {
    let telemetry = NormalizedTelemetry::new()
        .with_ffb_scalar(0.75)
        .with_rpm(6500.0)
        .with_speed_ms(45.0)
        .with_slip_ratio(0.15)
        .with_gear(4)
        .with_car_id("gt3_bmw".to_string())
        .with_track_id("spa".to_string());
    
    assert!(telemetry.has_ffb_data());
    assert!(telemetry.has_rpm_data());
    
    // Test RPM fraction calculation
    let rpm_fraction = telemetry.rpm_fraction(8000.0).unwrap();
    assert!(rpm_fraction.abs_diff(0.8125) < 0.01); // 6500/8000 = 0.8125
    
    // Test speed conversions
    assert!(telemetry.speed_kmh().unwrap().abs_diff(162.0) < 0.1); // 45 m/s = 162 km/h
    assert!(telemetry.speed_mph().unwrap().abs_diff(100.65) < 0.1); // 45 m/s ≈ 100.65 mph
}

#[test]
fn test_telemetry_flags() {
    let mut flags = TelemetryFlags::default();
    flags.yellow_flag = true;
    flags.pit_limiter = true;
    flags.drs_available = true;
    
    let telemetry = NormalizedTelemetry::new().with_flags(flags);
    
    assert!(telemetry.has_active_flags());
    assert!(telemetry.flags.yellow_flag);
    assert!(telemetry.flags.pit_limiter);
    assert!(telemetry.flags.drs_available);
    assert!(!telemetry.flags.red_flag);
}

#[test]
fn test_extended_telemetry_data() {
    let telemetry = NormalizedTelemetry::new()
        .with_extended("fuel_level".to_string(), TelemetryValue::Float(45.5))
        .with_extended("lap_count".to_string(), TelemetryValue::Integer(12))
        .with_extended("session_type".to_string(), TelemetryValue::String("Race".to_string()))
        .with_extended("drs_enabled".to_string(), TelemetryValue::Boolean(true));
    
    assert_eq!(telemetry.extended.len(), 4);
    
    // Verify each extended value type
    match telemetry.extended.get("fuel_level") {
        Some(TelemetryValue::Float(fuel)) => assert_eq!(*fuel, 45.5),
        _ => panic!("Expected fuel_level to be a float"),
    }
    
    match telemetry.extended.get("lap_count") {
        Some(TelemetryValue::Integer(laps)) => assert_eq!(*laps, 12),
        _ => panic!("Expected lap_count to be an integer"),
    }
    
    match telemetry.extended.get("session_type") {
        Some(TelemetryValue::String(session)) => assert_eq!(session, "Race"),
        _ => panic!("Expected session_type to be a string"),
    }
    
    match telemetry.extended.get("drs_enabled") {
        Some(TelemetryValue::Boolean(drs)) => assert!(*drs),
        _ => panic!("Expected drs_enabled to be a boolean"),
    }
}

#[tokio::test]
async fn test_telemetry_service_recording() {
    let temp_dir = tempdir().unwrap();
    let output_path = temp_dir.path().join("service_recording.json");
    
    let mut service = TelemetryService::new();
    
    // Enable recording
    service.enable_recording(output_path.clone()).unwrap();
    
    // Disable recording
    service.disable_recording();
    
    // Test that we can enable/disable without errors
    assert!(true); // If we get here, no panics occurred
}

/// Integration test that validates the complete telemetry pipeline
#[tokio::test]
async fn test_complete_telemetry_pipeline() {
    let temp_dir = tempdir().unwrap();
    let recording_path = temp_dir.path().join("pipeline_test.json");
    
    // Create a synthetic recording
    let recording = TestFixtureGenerator::generate_racing_session(
        "test_game".to_string(),
        1.0, // 1 second
        60.0, // 60 FPS
    );
    
    // Save the recording
    let mut recorder = TelemetryRecorder::new(recording_path.clone()).unwrap();
    recorder.start_recording("test_game".to_string());
    
    for frame in &recording.frames {
        recorder.record_frame(frame.clone());
    }
    
    recorder.stop_recording(Some("Pipeline test".to_string())).unwrap();
    
    // Load and replay the recording
    let loaded_recording = TelemetryRecorder::load_recording(&recording_path).unwrap();
    let mut player = TelemetryPlayer::new(loaded_recording);
    
    player.start_playback();
    
    let mut replayed_frames = Vec::new();
    while let Some(frame) = player.get_next_frame() {
        replayed_frames.push(frame);
        if replayed_frames.len() >= recording.frames.len() {
            break;
        }
    }
    
    // Verify that we replayed the expected number of frames
    assert_eq!(replayed_frames.len(), recording.frames.len());
    
    // Verify that the data is consistent
    for (original, replayed) in recording.frames.iter().zip(replayed_frames.iter()) {
        assert_eq!(original.data.rpm, replayed.data.rpm);
        assert_eq!(original.data.speed_ms, replayed.data.speed_ms);
        assert_eq!(original.data.gear, replayed.data.gear);
    }
}

/// Test rate limiting protection for RT thread
#[tokio::test]
async fn test_rate_limiting_protection() {
    let mut rate_limiter = RateLimiter::new(100); // 100 Hz max
    
    // Simulate high-frequency telemetry data
    let mut processed = 0;
    let mut dropped = 0;
    
    for _ in 0..1000 {
        if rate_limiter.should_process() {
            processed += 1;
        } else {
            dropped += 1;
        }
    }
    
    // Should have dropped most frames to protect RT thread
    assert!(dropped > processed);
    assert_eq!(rate_limiter.processed_count(), processed);
    assert_eq!(rate_limiter.dropped_count(), dropped);
}

/// Test telemetry adapter error handling
#[test]
fn test_adapter_error_handling() {
    let iracing_adapter = IRacingAdapter::new();
    let acc_adapter = ACCAdapter::new();
    
    // Test invalid data handling
    let invalid_data = vec![0u8; 10];
    
    let iracing_result = iracing_adapter.normalize(&invalid_data);
    assert!(iracing_result.is_err());
    
    let acc_result = acc_adapter.normalize(&invalid_data);
    assert!(acc_result.is_err());
}

/// Test telemetry data normalization consistency
#[test]
fn test_normalization_consistency() {
    // Create test data that should normalize consistently
    let test_cases = vec![
        (5000.0, 50.0, 4, 0.5), // RPM, speed_ms, gear, ffb_scalar
        (7500.0, 75.0, 6, -0.3),
        (3000.0, 25.0, 2, 0.8),
    ];
    
    for (rpm, speed, gear, ffb) in test_cases {
        let telemetry = NormalizedTelemetry::new()
            .with_rpm(rpm)
            .with_speed_ms(speed)
            .with_gear(gear)
            .with_ffb_scalar(ffb);
        
        // Verify normalization is consistent
        assert_eq!(telemetry.rpm, Some(rpm));
        assert_eq!(telemetry.speed_ms, Some(speed));
        assert_eq!(telemetry.gear, Some(gear));
        assert_eq!(telemetry.ffb_scalar, Some(ffb.clamp(-1.0, 1.0)));
    }
}