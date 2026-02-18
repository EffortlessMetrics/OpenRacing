//! Standalone tests for telemetry module
//! 
//! These tests can run independently of the rest of the service

#[cfg(test)]
mod tests {
    use super::super::*;
    use std::time::Duration;
    use tempfile::tempdir;
    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn test_normalized_telemetry_creation() {
        let telemetry = NormalizedTelemetry::new()
            .with_ffb_scalar(0.75)
            .with_rpm(6500.0)
            .with_speed_ms(45.0)
            .with_slip_ratio(0.15)
            .with_gear(4)
            .with_car_id("gt3_bmw".to_string())
            .with_track_id("spa".to_string());
        
        assert_eq!(telemetry.ffb_scalar, Some(0.75));
        assert_eq!(telemetry.rpm, Some(6500.0));
        assert_eq!(telemetry.speed_ms, Some(45.0));
        assert_eq!(telemetry.slip_ratio, Some(0.15));
        assert_eq!(telemetry.gear, Some(4));
        assert_eq!(telemetry.car_id, Some("gt3_bmw".to_string()));
        assert_eq!(telemetry.track_id, Some("spa".to_string()));
    }

    #[test]
    fn test_ffb_scalar_clamping() {
        let telemetry1 = NormalizedTelemetry::new().with_ffb_scalar(1.5);
        assert_eq!(telemetry1.ffb_scalar, Some(1.0));
        
        let telemetry2 = NormalizedTelemetry::new().with_ffb_scalar(-1.5);
        assert_eq!(telemetry2.ffb_scalar, Some(-1.0));
    }

    #[test]
    fn test_rate_limiter_basic() {
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

    #[test]
    fn test_telemetry_recording() -> TestResult {
        let temp_dir = tempdir()?;
        let output_path = temp_dir.path().join("test_recording.json");

        let mut recorder = TelemetryRecorder::new(output_path.clone())?;
        
        // Start recording
        recorder.start_recording("test_game".to_string());
        assert!(recorder.is_recording());
        
        // Record some frames
        let telemetry = NormalizedTelemetry::new().with_rpm(5000.0);
        let frame = TelemetryFrame::new(telemetry, 1000000, 0, 64);
        recorder.record_frame(frame);
        
        assert_eq!(recorder.frame_count(), 1);
        
        // Stop recording
        let recording = recorder.stop_recording(Some("Test recording".to_string()))?;
        assert!(!recorder.is_recording());
        assert_eq!(recording.frames.len(), 1);
        assert_eq!(recording.metadata.game_id, "test_game");

        // Verify file was created
        assert!(output_path.exists());
        Ok(())
    }

    #[test]
    fn test_synthetic_fixture_generation() {
        let recording = TestFixtureGenerator::generate_racing_session(
            "test_game".to_string(),
            2.0, // 2 seconds
            60.0, // 60 FPS
        );
        
        assert_eq!(recording.metadata.game_id, "test_game");
        assert_eq!(recording.metadata.frame_count, 120); // 2 * 60
        assert_eq!(recording.frames.len(), 120);
        
        // Check that frames have reasonable data
        for frame in &recording.frames {
            assert!(frame.data.rpm.is_some());
            assert!(frame.data.speed_ms.is_some());
            assert!(frame.data.ffb_scalar.is_some());
        }
    }

    #[test]
    fn test_telemetry_playback() {
        let recording = TestFixtureGenerator::generate_racing_session(
            "test_game".to_string(),
            1.0, // 1 second
            10.0, // 10 FPS
        );
        
        let mut player = TelemetryPlayer::new(recording);
        
        // Start playback
        player.start_playback();
        assert_eq!(player.progress(), 0.0);
        assert!(!player.is_finished());
        
        // Should have frames to play
        assert!(player.get_next_frame().is_some());
        
        // Progress should increase
        assert!(player.progress() > 0.0);
    }

    #[tokio::test]
    async fn test_mock_adapter() -> TestResult {
        let mut adapter = MockAdapter::new("test_game".to_string());
        adapter.set_running(true);

        assert_eq!(adapter.game_id(), "test_game");
        assert!(adapter.is_game_running().await?);

        let mut receiver = adapter.start_monitoring().await?;

        // Should receive telemetry frames
        let frame = tokio::time::timeout(Duration::from_millis(100), receiver.recv())
            .await?
            .ok_or("expected telemetry frame")?;

        assert!(frame.data.rpm.is_some());
        assert!(frame.data.speed_ms.is_some());
        assert_eq!(frame.data.car_id, Some("mock_car".to_string()));
        Ok(())
    }

    #[test]
    fn test_iracing_adapter_creation() {
        let adapter = IRacingAdapter::new();
        assert_eq!(adapter.game_id(), "iracing");
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    }

    #[test]
    fn test_acc_adapter_creation() {
        let adapter = ACCAdapter::new();
        assert_eq!(adapter.game_id(), "acc");
        assert_eq!(adapter.expected_update_rate(), Duration::from_millis(16));
    }

    #[test]
    fn test_telemetry_service_creation() -> TestResult {
        let service = TelemetryService::new()?;

        let supported_games = service.supported_games();
        assert!(supported_games.contains(&"iracing".to_string()));
        assert!(supported_games.contains(&"acc".to_string()));
        assert!(supported_games.contains(&"ams2".to_string()));
        assert!(supported_games.contains(&"rfactor2".to_string()));
        assert!(supported_games.contains(&"eawrc".to_string()));
        assert!(supported_games.contains(&"dirt5".to_string()));
        Ok(())
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
    fn test_extended_telemetry_data() -> TestResult {
        let telemetry = NormalizedTelemetry::new()
            .with_extended("fuel_level".to_string(), TelemetryValue::Float(45.5))
            .with_extended("lap_count".to_string(), TelemetryValue::Integer(12))
            .with_extended("session_type".to_string(), TelemetryValue::String("Race".to_string()))
            .with_extended("drs_enabled".to_string(), TelemetryValue::Boolean(true));

        assert_eq!(telemetry.extended.len(), 4);

        let fuel = telemetry
            .extended
            .get("fuel_level")
            .ok_or("Expected fuel_level to exist")?;
        assert_eq!(fuel, &TelemetryValue::Float(45.5));

        let laps = telemetry
            .extended
            .get("lap_count")
            .ok_or("Expected lap_count to exist")?;
        assert_eq!(laps, &TelemetryValue::Integer(12));

        let session = telemetry
            .extended
            .get("session_type")
            .ok_or("Expected session_type to exist")?;
        assert_eq!(session, &TelemetryValue::String("Race".to_string()));

        let drs = telemetry
            .extended
            .get("drs_enabled")
            .ok_or("Expected drs_enabled to exist")?;
        assert_eq!(drs, &TelemetryValue::Boolean(true));
        Ok(())
    }

    #[test]
    fn test_complete_telemetry_pipeline() -> TestResult {
        let temp_dir = tempdir()?;
        let recording_path = temp_dir.path().join("pipeline_test.json");

        // Create a synthetic recording
        let recording = TestFixtureGenerator::generate_racing_session(
            "test_game".to_string(),
            1.0, // 1 second
            60.0, // 60 FPS
        );

        // Save the recording
        let mut recorder = TelemetryRecorder::new(recording_path.clone())?;
        recorder.start_recording("test_game".to_string());

        for frame in &recording.frames {
            recorder.record_frame(frame.clone());
        }

        recorder.stop_recording(Some("Pipeline test".to_string()))?;

        // Load and replay the recording
        let loaded_recording = TelemetryRecorder::load_recording(&recording_path)?;
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
        Ok(())
    }
}
