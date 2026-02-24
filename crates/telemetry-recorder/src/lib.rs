//! Telemetry recording, playback, and synthetic fixture generation utilities.

use racing_wheel_schemas::telemetry::{NormalizedTelemetry, TelemetryFlags, TelemetryFrame};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Telemetry recording session container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryRecording {
    pub metadata: RecordingMetadata,
    pub frames: Vec<TelemetryFrame>,
}

/// Recording metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingMetadata {
    pub game_id: String,
    pub timestamp: u64,
    pub duration_seconds: f64,
    pub frame_count: usize,
    pub average_fps: f32,
    pub car_id: Option<String>,
    pub track_id: Option<String>,
    pub description: Option<String>,
}

/// Telemetry recorder for creating and persisting fixtures.
pub struct TelemetryRecorder {
    output_path: PathBuf,
    frames: Vec<TelemetryFrame>,
    start_time: Option<SystemTime>,
    game_id: String,
}

impl TelemetryRecorder {
    pub fn new(output_path: PathBuf) -> anyhow::Result<Self> {
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

    pub fn start_recording(&mut self, game_id: String) {
        self.game_id = game_id;
        self.start_time = Some(SystemTime::now());
        self.frames.clear();
    }

    pub fn record_frame(&mut self, frame: TelemetryFrame) {
        if self.start_time.is_some() {
            self.frames.push(frame);
        }
    }

    pub fn stop_recording(
        &mut self,
        description: Option<String>,
    ) -> anyhow::Result<TelemetryRecording> {
        let start_time = self
            .start_time
            .take()
            .ok_or_else(|| anyhow::anyhow!("Recording not started"))?;

        let end_time = SystemTime::now();
        let duration = end_time.duration_since(start_time)?;

        let car_id = self.frames.iter().find_map(|f| f.data.car_id.clone());
        let track_id = self.frames.iter().find_map(|f| f.data.track_id.clone());

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

        self.save_recording(&recording)?;
        Ok(recording)
    }

    fn save_recording(&self, recording: &TelemetryRecording) -> anyhow::Result<()> {
        let file = File::create(&self.output_path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, recording)?;
        Ok(())
    }

    pub fn load_recording<P: AsRef<Path>>(path: P) -> anyhow::Result<TelemetryRecording> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        Ok(serde_json::from_reader(reader)?)
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    pub fn is_recording(&self) -> bool {
        self.start_time.is_some()
    }
}

/// Telemetry playback helper for recordings.
pub struct TelemetryPlayer {
    recording: TelemetryRecording,
    current_frame: usize,
    start_time: Option<std::time::Instant>,
    first_frame_timestamp: Option<u64>,
    playback_speed: f32,
}

impl TelemetryPlayer {
    pub fn new(recording: TelemetryRecording) -> Self {
        Self {
            recording,
            current_frame: 0,
            start_time: None,
            first_frame_timestamp: None,
            playback_speed: 1.0,
        }
    }

    pub fn start_playback(&mut self) {
        self.start_time = Some(std::time::Instant::now());
        self.current_frame = 0;
        self.first_frame_timestamp = self.recording.frames.first().map(|f| f.timestamp_ns);
    }

    pub fn get_next_frame(&mut self) -> Option<TelemetryFrame> {
        let start_time = self.start_time?;
        let first_timestamp = self.first_frame_timestamp?;

        if self.current_frame >= self.recording.frames.len() {
            return None;
        }

        let current_frame = &self.recording.frames[self.current_frame];
        let elapsed = start_time.elapsed();

        let relative_timestamp = current_frame.timestamp_ns.saturating_sub(first_timestamp);
        let frame_time = Duration::from_nanos(relative_timestamp);
        let adjusted_frame_time =
            Duration::from_nanos((frame_time.as_nanos() as f32 / self.playback_speed) as u64);

        if elapsed >= adjusted_frame_time {
            let frame = current_frame.clone();
            self.current_frame += 1;
            Some(frame)
        } else {
            None
        }
    }

    pub fn set_playback_speed(&mut self, speed: f32) {
        self.playback_speed = speed.clamp(0.1, 10.0);
    }

    pub fn progress(&self) -> f32 {
        if self.recording.frames.is_empty() {
            1.0
        } else {
            self.current_frame as f32 / self.recording.frames.len() as f32
        }
    }

    pub fn is_finished(&self) -> bool {
        self.current_frame >= self.recording.frames.len()
    }

    pub fn reset(&mut self) {
        self.current_frame = 0;
        self.start_time = None;
    }

    pub fn metadata(&self) -> &RecordingMetadata {
        &self.recording.metadata
    }
}

/// Fixture generation for synthetic and scenario-based recordings.
pub struct TestFixtureGenerator;

impl TestFixtureGenerator {
    pub fn generate_racing_session(
        game_id: String,
        duration_seconds: f32,
        fps: f32,
    ) -> TelemetryRecording {
        let frame_count = (duration_seconds * fps) as usize;
        let mut frames = Vec::with_capacity(frame_count);
        let start_timestamp = 0u64;

        for i in 0..frame_count {
            let time_offset = (i as f32 / fps * 1_000_000_000.0) as u64;
            let timestamp_ns = start_timestamp + time_offset;
            let progress = i as f32 / frame_count as f32;
            let telemetry = Self::generate_synthetic_telemetry(progress);

            frames.push(TelemetryFrame::new(telemetry, timestamp_ns, i as u64, 64));
        }

        let metadata = RecordingMetadata {
            game_id,
            timestamp: 0,
            duration_seconds: duration_seconds as f64,
            frame_count,
            average_fps: fps,
            car_id: Some("test_car".to_string()),
            track_id: Some("test_track".to_string()),
            description: Some("Synthetic test fixture".to_string()),
        };

        TelemetryRecording { metadata, frames }
    }

    fn generate_synthetic_telemetry(progress: f32) -> NormalizedTelemetry {
        use std::f32::consts::PI;

        let rpm_base = 4000.0 + (progress * 4000.0);
        let rpm_variation = (progress * 20.0 * PI).sin() * 500.0;
        let rpm = rpm_base + rpm_variation;

        let speed = 30.0 + progress * 50.0 + (progress * 10.0 * PI).sin() * 10.0;
        let ffb_scalar = (progress * 8.0 * PI).sin() * 0.8;
        let slip_ratio = ((progress * 15.0 * PI).sin().abs() * 0.3).min(1.0);

        let gear = match speed {
            s if s < 15.0 => 1,
            s if s < 25.0 => 2,
            s if s < 40.0 => 3,
            s if s < 60.0 => 4,
            s if s < 80.0 => 5,
            _ => 6,
        };

        NormalizedTelemetry::builder()
            .ffb_scalar(ffb_scalar)
            .rpm(rpm)
            .speed_mps(speed)
            .slip_ratio(slip_ratio)
            .gear(gear)
            .car_id("test_car")
            .track_id("test_track")
            .build()
    }

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
            TestScenario::Cornering => Self::generate_cornering_session(duration_seconds, fps),
            TestScenario::PitStop => Self::generate_pitstop_session(duration_seconds, fps),
        }
    }

    fn generate_constant_speed_session(duration_seconds: f32, fps: f32) -> TelemetryRecording {
        let mut recording =
            Self::generate_racing_session("test".to_string(), duration_seconds, fps);
        for frame in &mut recording.frames {
            frame.data = NormalizedTelemetry::builder()
                .ffb_scalar(0.5)
                .rpm(6000.0)
                .speed_mps(50.0)
                .slip_ratio(0.1)
                .gear(4)
                .build();
        }
        recording.metadata.description = Some("Constant speed test scenario".to_string());
        recording
    }

    fn generate_acceleration_session(duration_seconds: f32, fps: f32) -> TelemetryRecording {
        let mut recording =
            Self::generate_racing_session("test".to_string(), duration_seconds, fps);
        let frame_count = recording.frames.len();
        for (i, frame) in recording.frames.iter_mut().enumerate() {
            let progress = if frame_count > 0 {
                i as f32 / frame_count as f32
            } else {
                0.0
            };
            let speed = progress * 80.0;
            let rpm = 2000.0 + progress * 6000.0;
            frame.data = NormalizedTelemetry::builder()
                .ffb_scalar(0.3)
                .rpm(rpm)
                .speed_mps(speed)
                .slip_ratio(0.05)
                .gear(((speed / 15.0) as i8 + 1).min(6))
                .build();
        }
        recording.metadata.description = Some("Acceleration test scenario".to_string());
        recording
    }

    fn generate_cornering_session(duration_seconds: f32, fps: f32) -> TelemetryRecording {
        let mut recording =
            Self::generate_racing_session("test".to_string(), duration_seconds, fps);
        for frame in &mut recording.frames {
            frame.data = NormalizedTelemetry::builder()
                .ffb_scalar(0.9)
                .rpm(5500.0)
                .speed_mps(35.0)
                .slip_ratio(0.4)
                .gear(3)
                .build();
        }
        recording.metadata.description = Some("Cornering test scenario".to_string());
        recording
    }

    fn generate_pitstop_session(duration_seconds: f32, fps: f32) -> TelemetryRecording {
        let mut recording =
            Self::generate_racing_session("test".to_string(), duration_seconds, fps);
        let frame_count = recording.frames.len();
        for (i, frame) in recording.frames.iter_mut().enumerate() {
            let progress = if frame_count > 0 {
                i as f32 / frame_count as f32
            } else {
                0.0
            };
            let in_pits = progress > 0.3 && progress < 0.7;

            let mut flags = TelemetryFlags::default();
            flags.in_pits = in_pits;
            flags.pit_limiter = in_pits;

            let speed = if in_pits { 15.0 } else { 45.0 };
            let rpm = if in_pits { 2000.0 } else { 6000.0 };

            frame.data = NormalizedTelemetry::builder()
                .ffb_scalar(0.2)
                .rpm(rpm)
                .speed_mps(speed)
                .slip_ratio(0.05)
                .gear(if in_pits { 1 } else { 4 })
                .flags(flags)
                .build();
        }
        recording.metadata.description = Some("Pit stop test scenario".to_string());
        recording
    }
}

/// Test scenarios for fixture generation.
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
    fn test_recorder_creation() -> anyhow::Result<()> {
        let temp_dir = tempdir()?;
        let output_path = temp_dir.path().join("test_recording.json");

        assert!(TelemetryRecorder::new(output_path).is_ok());
        Ok(())
    }

    #[test]
    fn test_recording_lifecycle() -> anyhow::Result<()> {
        let temp_dir = tempdir()?;
        let output_path = temp_dir.path().join("test_recording.json");
        let mut recorder = TelemetryRecorder::new(output_path)?;

        recorder.start_recording("test_game".to_string());
        assert!(recorder.is_recording());

        let telemetry = NormalizedTelemetry::builder().rpm(5000.0).build();
        let frame = TelemetryFrame::new(telemetry, 1_000_000, 0, 64);
        recorder.record_frame(frame);

        assert_eq!(recorder.frame_count(), 1);
        let recording = recorder.stop_recording(Some("Test recording".to_string()))?;
        assert!(!recorder.is_recording());
        assert_eq!(recording.frames.len(), 1);
        assert_eq!(recording.metadata.game_id, "test_game");

        Ok(())
    }

    #[test]
    fn test_telemetry_player() {
        let recording =
            TestFixtureGenerator::generate_racing_session("test_game".to_string(), 1.0, 10.0);

        let mut player = TelemetryPlayer::new(recording);
        assert_eq!(player.progress(), 0.0);
        assert!(!player.is_finished());

        player.start_playback();
        player.set_playback_speed(2.0);
        player.reset();
        assert_eq!(player.progress(), 0.0);
    }

    #[test]
    fn test_synthetic_fixture_generation() {
        let recording =
            TestFixtureGenerator::generate_racing_session("test_game".to_string(), 2.0, 60.0);
        assert_eq!(recording.metadata.frame_count, 120);
        assert_eq!(recording.frames.len(), 120);
        for frame in &recording.frames {
            assert!(frame.data.rpm > 0.0);
            assert!(frame.data.speed_mps > 0.0);
        }
    }
}
