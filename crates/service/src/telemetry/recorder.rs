//! Telemetry recorder for CI testing and replay
//! 
//! Creates record-and-replay fixtures for CI testing without running actual games

use crate::telemetry::{NormalizedTelemetry, TelemetryFrame};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Telemetry recording session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryRecording {
    /// Recording metadata
    pub metadata: RecordingMetadata,
    
    /// Recorded telemetry frames
    pub frames: Vec<TelemetryFrame>,
}

/// Recording metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingMetadata {
    /// Game identifier
    pub game_id: String,
    
    /// Recording timestamp
    pub timestamp: u64,
    
    /// Recording duration in seconds
    pub duration_seconds: f64,
    
    /// Total frame count
    pub frame_count: usize,
    
    /// Average frame rate
    pub average_fps: f32,
    
    /// Car identifier (if available)
    pub car_id: Option<String>,
    
    /// Track identifier (if available)
    pub track_id: Option<String>,
    
    /// Recording description
    pub description: Option<String>,
}

/// Telemetry recorder for creating test fixtures
pub struct TelemetryRecorder {
    output_path: PathBuf,
    frames: Vec<TelemetryFrame>,
    start_time: Option<SystemTime>,
    game_id: String,
}

impl TelemetryRecorder {
    /// Create a new telemetry recorder
    pub fn new(output_path: PathBuf) -> Result<Self> {
        // Ensure output directory exists
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        Ok(Self {
            output_path,
            frames: Vec::new(),
            start_time: None,
            game_id: "unknown".to_string(),
        })
    }
    
    /// Start recording telemetry
    pub fn start_recording(&mut self, game_id: String) {
        self.game_id = game_id;
        self.start_time = Some(SystemTime::now());
        self.frames.clear();
    }
    
    /// Record a telemetry frame
    pub fn record_frame(&mut self, frame: TelemetryFrame) {
        if self.start_time.is_some() {
            self.frames.push(frame);
        }
    }
    
    /// Stop recording and save to file
    pub fn stop_recording(&mut self, description: Option<String>) -> Result<TelemetryRecording> {
        let start_time = self.start_time.take()
            .ok_or_else(|| anyhow::anyhow!("Recording not started"))?;
        
        let end_time = SystemTime::now();
        let duration = end_time.duration_since(start_time)?;
        
        // Extract metadata from frames
        let car_id = self.frames.iter()
            .find_map(|f| f.data.car_id.clone());
        let track_id = self.frames.iter()
            .find_map(|f| f.data.track_id.clone());
        
        let metadata = RecordingMetadata {
            game_id: self.game_id.clone(),
            timestamp: start_time.duration_since(UNIX_EPOCH)?.as_secs(),
            duration_seconds: duration.as_secs_f64(),
            frame_count: self.frames.len(),
            average_fps: if duration.as_secs_f64() > 0.0 {
                self.frames.len() as f32 / duration.as_secs_f64() as f32
            } else {
                0.0
            },
            car_id,
            track_id,
            description,
        };
        
        let recording = TelemetryRecording {
            metadata,
            frames: self.frames.clone(),
        };
        
        // Save to file
        self.save_recording(&recording)?;
        
        Ok(recording)
    }
    
    /// Save recording to file
    fn save_recording(&self, recording: &TelemetryRecording) -> Result<()> {
        let file = File::create(&self.output_path)?;
        let writer = BufWriter::new(file);
        
        // Use JSON for human readability in CI
        serde_json::to_writer_pretty(writer, recording)?;
        
        Ok(())
    }
    
    /// Load recording from file
    pub fn load_recording<P: AsRef<Path>>(path: P) -> Result<TelemetryRecording> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        
        let recording: TelemetryRecording = serde_json::from_reader(reader)?;
        Ok(recording)
    }
    
    /// Get current frame count
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }
    
    /// Check if recording is active
    pub fn is_recording(&self) -> bool {
        self.start_time.is_some()
    }
}

/// Telemetry player for replaying recorded sessions
pub struct TelemetryPlayer {
    recording: TelemetryRecording,
    current_frame: usize,
    start_time: Option<std::time::Instant>,
    playback_speed: f32,
}

impl TelemetryPlayer {
    /// Create a new telemetry player
    pub fn new(recording: TelemetryRecording) -> Self {
        Self {
            recording,
            current_frame: 0,
            start_time: None,
            playback_speed: 1.0,
        }
    }
    
    /// Start playback
    pub fn start_playback(&mut self) {
        self.start_time = Some(std::time::Instant::now());
        self.current_frame = 0;
    }
    
    /// Get the next frame if it's time to play it
    pub fn get_next_frame(&mut self) -> Option<TelemetryFrame> {
        let start_time = self.start_time?;
        
        if self.current_frame >= self.recording.frames.len() {
            return None;
        }
        
        let current_frame = &self.recording.frames[self.current_frame];
        let elapsed = start_time.elapsed();
        
        // Calculate when this frame should be played based on its timestamp
        let frame_time = Duration::from_nanos(current_frame.timestamp_ns);
        let adjusted_frame_time = Duration::from_nanos(
            (frame_time.as_nanos() as f32 / self.playback_speed) as u64
        );
        
        if elapsed >= adjusted_frame_time {
            let frame = current_frame.clone();
            self.current_frame += 1;
            Some(frame)
        } else {
            None
        }
    }
    
    /// Set playback speed (1.0 = normal, 2.0 = 2x speed, 0.5 = half speed)
    pub fn set_playback_speed(&mut self, speed: f32) {
        self.playback_speed = speed.max(0.1).min(10.0);
    }
    
    /// Get playback progress (0.0 to 1.0)
    pub fn progress(&self) -> f32 {
        if self.recording.frames.is_empty() {
            1.0
        } else {
            self.current_frame as f32 / self.recording.frames.len() as f32
        }
    }
    
    /// Check if playback is finished
    pub fn is_finished(&self) -> bool {
        self.current_frame >= self.recording.frames.len()
    }
    
    /// Reset playback to beginning
    pub fn reset(&mut self) {
        self.current_frame = 0;
        self.start_time = None;
    }
    
    /// Get recording metadata
    pub fn metadata(&self) -> &RecordingMetadata {
        &self.recording.metadata
    }
}

/// Test fixture generator for creating synthetic telemetry data
pub struct TestFixtureGenerator;

impl TestFixtureGenerator {
    /// Generate a synthetic racing session for testing
    pub fn generate_racing_session(
        game_id: String,
        duration_seconds: f32,
        fps: f32,
    ) -> TelemetryRecording {
        let frame_count = (duration_seconds * fps) as usize;
        let mut frames = Vec::with_capacity(frame_count);
        
        let start_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time should be after UNIX epoch")
            .as_nanos() as u64;
        
        for i in 0..frame_count {
            let time_offset = (i as f32 / fps * 1_000_000_000.0) as u64;
            let timestamp_ns = start_timestamp + time_offset;
            
            // Generate synthetic telemetry data
            let progress = i as f32 / frame_count as f32;
            let telemetry = Self::generate_synthetic_telemetry(progress);
            
            let frame = TelemetryFrame::new(
                telemetry,
                timestamp_ns,
                i as u64,
                64, // Synthetic raw size
            );
            
            frames.push(frame);
        }
        
        let metadata = RecordingMetadata {
            game_id: game_id.clone(),
            timestamp: start_timestamp / 1_000_000_000,
            duration_seconds: duration_seconds as f64,
            frame_count,
            average_fps: fps,
            car_id: Some("test_car".to_string()),
            track_id: Some("test_track".to_string()),
            description: Some("Synthetic test fixture".to_string()),
        };
        
        TelemetryRecording { metadata, frames }
    }
    
    /// Generate synthetic telemetry data for a given progress (0.0 to 1.0)
    fn generate_synthetic_telemetry(progress: f32) -> NormalizedTelemetry {
        use std::f32::consts::PI;
        
        // Simulate a racing scenario with varying RPM, speed, etc.
        let rpm_base = 4000.0 + (progress * 4000.0);
        let rpm_variation = (progress * 20.0 * PI).sin() * 500.0;
        let rpm = rpm_base + rpm_variation;
        
        let speed = 30.0 + progress * 50.0 + (progress * 10.0 * PI).sin() * 10.0;
        let ffb_scalar = (progress * 8.0 * PI).sin() * 0.8;
        let slip_ratio = ((progress * 15.0 * PI).sin().abs() * 0.3).min(1.0);
        
        // Simulate gear changes
        let gear = match speed {
            s if s < 15.0 => 1,
            s if s < 25.0 => 2,
            s if s < 40.0 => 3,
            s if s < 60.0 => 4,
            s if s < 80.0 => 5,
            _ => 6,
        };
        
        NormalizedTelemetry::default()
            .with_ffb_scalar(ffb_scalar)
            .with_rpm(rpm)
            .with_speed_ms(speed)
            .with_slip_ratio(slip_ratio)
            .with_gear(gear)
            .with_car_id("test_car".to_string())
            .with_track_id("test_track".to_string())
    }
    
    /// Generate a session with specific characteristics for testing
    pub fn generate_test_scenario(
        scenario: TestScenario,
        duration_seconds: f32,
        fps: f32,
    ) -> TelemetryRecording {
        match scenario {
            TestScenario::ConstantSpeed => {
                Self::generate_constant_speed_session(duration_seconds, fps)
            }
            TestScenario::Acceleration => {
                Self::generate_acceleration_session(duration_seconds, fps)
            }
            TestScenario::Cornering => {
                Self::generate_cornering_session(duration_seconds, fps)
            }
            TestScenario::PitStop => {
                Self::generate_pitstop_session(duration_seconds, fps)
            }
        }
    }
    
    fn generate_constant_speed_session(duration_seconds: f32, fps: f32) -> TelemetryRecording {
        let mut recording = Self::generate_racing_session("test".to_string(), duration_seconds, fps);
        
        // Override with constant values
        for frame in &mut recording.frames {
            frame.data = NormalizedTelemetry::default()
                .with_ffb_scalar(0.5)
                .with_rpm(6000.0)
                .with_speed_ms(50.0)
                .with_slip_ratio(0.1)
                .with_gear(4);
        }
        
        recording.metadata.description = Some("Constant speed test scenario".to_string());
        recording
    }
    
    fn generate_acceleration_session(duration_seconds: f32, fps: f32) -> TelemetryRecording {
        let mut recording = Self::generate_racing_session("test".to_string(), duration_seconds, fps);
        
        // Override with acceleration pattern
        let frame_count = recording.frames.len();
        for (i, frame) in recording.frames.iter_mut().enumerate() {
            let progress = i as f32 / frame_count as f32;
            let speed = progress * 80.0; // 0 to 80 m/s
            let rpm = 2000.0 + progress * 6000.0; // 2000 to 8000 RPM
            
            frame.data = NormalizedTelemetry::default()
                .with_ffb_scalar(0.3)
                .with_rpm(rpm)
                .with_speed_ms(speed)
                .with_slip_ratio(0.05)
                .with_gear(((speed / 15.0) as i8 + 1).min(6));
        }
        
        recording.metadata.description = Some("Acceleration test scenario".to_string());
        recording
    }
    
    fn generate_cornering_session(duration_seconds: f32, fps: f32) -> TelemetryRecording {
        let mut recording = Self::generate_racing_session("test".to_string(), duration_seconds, fps);
        
        // Override with cornering pattern (high FFB, slip)
        for frame in &mut recording.frames {
            frame.data = NormalizedTelemetry::new()
                .with_ffb_scalar(0.9) // High steering force
                .with_rpm(5500.0)
                .with_speed_ms(35.0) // Slower corner speed
                .with_slip_ratio(0.4) // Higher slip in corners
                .with_gear(3);
        }
        
        recording.metadata.description = Some("Cornering test scenario".to_string());
        recording
    }
    
    fn generate_pitstop_session(duration_seconds: f32, fps: f32) -> TelemetryRecording {
        let mut recording = Self::generate_racing_session("test".to_string(), duration_seconds, fps);
        
        // Override with pit stop pattern
        let frame_count = recording.frames.len();
        for (i, frame) in recording.frames.iter_mut().enumerate() {
            let progress = i as f32 / frame_count as f32;
            let in_pits = progress > 0.3 && progress < 0.7;
            
            let mut flags = crate::telemetry::TelemetryFlags::default();
            flags.in_pits = in_pits;
            flags.pit_limiter = in_pits;
            
            let speed = if in_pits { 15.0 } else { 45.0 };
            let rpm = if in_pits { 2000.0 } else { 6000.0 };
            
            frame.data = NormalizedTelemetry::new()
                .with_ffb_scalar(0.2)
                .with_rpm(rpm)
                .with_speed_ms(speed)
                .with_slip_ratio(0.05)
                .with_gear(if in_pits { 1 } else { 4 })
                .with_flags(flags);
        }
        
        recording.metadata.description = Some("Pit stop test scenario".to_string());
        recording
    }
}

/// Test scenarios for fixture generation
#[derive(Debug, Clone, Copy)]
pub enum TestScenario {
    ConstantSpeed,
    Acceleration,
    Cornering,
    PitStop,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_recorder_creation() {
        let temp_dir = tempdir().unwrap();
        let output_path = temp_dir.path().join("test_recording.json");
        
        let recorder = TelemetryRecorder::new(output_path);
        assert!(recorder.is_ok());
    }

    #[test]
    fn test_recording_lifecycle() {
        let temp_dir = tempdir().unwrap();
        let output_path = temp_dir.path().join("test_recording.json");
        
        let mut recorder = TelemetryRecorder::new(output_path.clone()).unwrap();
        
        // Start recording
        recorder.start_recording("test_game".to_string());
        assert!(recorder.is_recording());
        
        // Record some frames
        let telemetry = NormalizedTelemetry::new().with_rpm(5000.0);
        let frame = TelemetryFrame::new(telemetry, 1000000, 0, 64);
        recorder.record_frame(frame);
        
        assert_eq!(recorder.frame_count(), 1);
        
        // Stop recording
        let recording = recorder.stop_recording(Some("Test recording".to_string())).unwrap();
        assert!(!recorder.is_recording());
        assert_eq!(recording.frames.len(), 1);
        assert_eq!(recording.metadata.game_id, "test_game");
        
        // Verify file was created
        assert!(output_path.exists());
    }

    #[test]
    fn test_load_recording() {
        let temp_dir = tempdir().unwrap();
        let output_path = temp_dir.path().join("test_recording.json");
        
        // Create and save a recording
        let mut recorder = TelemetryRecorder::new(output_path.clone()).unwrap();
        recorder.start_recording("test_game".to_string());
        
        let telemetry = NormalizedTelemetry::new().with_rpm(5000.0);
        let frame = TelemetryFrame::new(telemetry, 1000000, 0, 64);
        recorder.record_frame(frame);
        
        recorder.stop_recording(Some("Test recording".to_string())).unwrap();
        
        // Load the recording
        let loaded = TelemetryRecorder::load_recording(&output_path).unwrap();
        assert_eq!(loaded.metadata.game_id, "test_game");
        assert_eq!(loaded.frames.len(), 1);
    }

    #[test]
    fn test_telemetry_player() {
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
    fn test_test_scenarios() {
        let scenarios = [
            TestScenario::ConstantSpeed,
            TestScenario::Acceleration,
            TestScenario::Cornering,
            TestScenario::PitStop,
        ];
        
        for scenario in scenarios {
            let recording = TestFixtureGenerator::generate_test_scenario(
                scenario,
                1.0,
                30.0,
            );
            
            assert_eq!(recording.frames.len(), 30);
            assert!(recording.metadata.description.is_some());
        }
    }
}