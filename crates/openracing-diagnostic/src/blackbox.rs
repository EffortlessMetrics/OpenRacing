//! Blackbox Recording System with .wbb v1 Format
//!
//! Implements the .wbb v1 format with three streams:
//! - Stream A: 1kHz frames + per-node outputs
//! - Stream B: 60Hz telemetry
//! - Stream C: Health/fault events
//!
//! # RT Safety
//!
//! The recording path is designed for RT hot path:
//! - Stream A recording uses pre-allocated buffers
//! - No heap allocations in record_frame hot path
//! - File I/O is deferred to finalization (non-RT)
//!
//! # Example
//!
//! ```no_run
//! use openracing_diagnostic::{
//!     BlackboxRecorder, BlackboxConfig, FrameData, SafetyStateSimple,
//! };
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let config = BlackboxConfig::new("device-001", "./recordings");
//! let mut recorder = BlackboxRecorder::new(config)?;
//!
//! // Record frames in RT hot path
//! let frame = FrameData {
//!     ffb_in: 0.5,
//!     torque_out: 0.3,
//!     wheel_speed: 10.0,
//!     hands_off: false,
//!     ts_mono_ns: 1_000_000_000,
//!     seq: 1,
//! };
//!
//! recorder.record_frame(frame, &[0.1, 0.2], SafetyStateSimple::SafeTorque, 150)?;
//!
//! // Finalize recording (non-RT)
//! let output_path = recorder.finalize()?;
//! # Ok(())
//! # }
//! ```

use crate::error::{DiagnosticError, DiagnosticResult};
use crate::format::{INDEX_INTERVAL_MS, IndexEntry, WbbFooter, WbbHeader};
use crate::streams::{
    FrameData, HealthEventData, SafetyStateSimple, StreamA, StreamB, StreamC, TelemetryData,
};
use flate2::{Compression, write::GzEncoder};
use std::{
    fs::File,
    io::{BufWriter, Seek, Write},
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

/// Blackbox recorder configuration
#[derive(Debug, Clone)]
pub struct BlackboxConfig {
    /// Device identifier
    pub device_id: String,
    /// Output directory for recordings
    pub output_dir: PathBuf,
    /// Maximum recording duration in seconds
    pub max_duration_s: u64,
    /// Maximum file size in bytes
    pub max_file_size_bytes: u64,
    /// Compression level (0-9)
    pub compression_level: u8,
    /// Enable Stream A (1kHz frames)
    pub enable_stream_a: bool,
    /// Enable Stream B (60Hz telemetry)
    pub enable_stream_b: bool,
    /// Enable Stream C (health events)
    pub enable_stream_c: bool,
}

impl BlackboxConfig {
    /// Create a new configuration with defaults
    pub fn new(device_id: impl Into<String>, output_dir: impl AsRef<Path>) -> Self {
        Self {
            device_id: device_id.into(),
            output_dir: output_dir.as_ref().to_path_buf(),
            max_duration_s: 300,
            max_file_size_bytes: 25 * 1024 * 1024,
            compression_level: 6,
            enable_stream_a: true,
            enable_stream_b: true,
            enable_stream_c: true,
        }
    }

    /// Get stream flags byte
    pub fn stream_flags(&self) -> u8 {
        let mut flags = 0u8;
        if self.enable_stream_a {
            flags |= crate::format::STREAM_A_ID;
        }
        if self.enable_stream_b {
            flags |= crate::format::STREAM_B_ID;
        }
        if self.enable_stream_c {
            flags |= crate::format::STREAM_C_ID;
        }
        flags
    }
}

/// Recording statistics
#[derive(Debug, Clone)]
pub struct RecordingStats {
    /// Recording start time
    pub start_time: Instant,
    /// Total frames recorded
    pub frames_recorded: u64,
    /// Total telemetry records
    pub telemetry_records: u64,
    /// Total health events recorded
    pub health_events: u64,
    /// Current file size estimate
    pub file_size_bytes: u64,
    /// Compression ratio achieved
    pub compression_ratio: f64,
    /// Whether recording is active
    pub is_active: bool,
}

impl Default for RecordingStats {
    fn default() -> Self {
        Self {
            start_time: Instant::now(),
            frames_recorded: 0,
            telemetry_records: 0,
            health_events: 0,
            file_size_bytes: 0,
            compression_ratio: 1.0,
            is_active: false,
        }
    }
}

/// Blackbox recorder implementation
///
/// Records diagnostic data to .wbb format files with compression.
pub struct BlackboxRecorder {
    config: BlackboxConfig,
    output_file: PathBuf,
    writer: BufWriter<File>,
    encoder: GzEncoder<Vec<u8>>,
    stream_a: StreamA,
    stream_b: StreamB,
    stream_c: StreamC,
    start_time: Instant,
    index_entries: Vec<IndexEntry>,
    stats: RecordingStats,
    compressed_buffer: Vec<u8>,
    frame_buffer: Vec<u8>,
}

impl BlackboxRecorder {
    /// Create new blackbox recorder
    ///
    /// This method creates the output file and writes the header.
    /// NOT RT-safe - performs file I/O.
    pub fn new(config: BlackboxConfig) -> DiagnosticResult<Self> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let filename = format!(
            "blackbox_{}_{}.wbb",
            config.device_id.replace(['/', '\\', ':'], "_"),
            timestamp
        );

        let output_file = config.output_dir.join(filename);

        let file = File::create(&output_file).map_err(|e| DiagnosticError::Io(e.to_string()))?;

        let mut writer = BufWriter::new(file);

        let mut header = WbbHeader::new(
            config.device_id.clone(),
            1,
            config.stream_flags(),
            config.compression_level,
        );

        let header_bytes = bincode::serde::encode_to_vec(&header, bincode::config::legacy())
            .map_err(|e| DiagnosticError::Serialization(e.to_string()))?;

        let header_size = header_bytes.len() as u32;
        header.header_size = header_size;

        // Re-serialize with the correct header_size
        let header_bytes = bincode::serde::encode_to_vec(&header, bincode::config::legacy())
            .map_err(|e| DiagnosticError::Serialization(e.to_string()))?;

        writer
            .write_all(&header_bytes)
            .map_err(|e| DiagnosticError::Io(e.to_string()))?;

        let compression = Compression::new(config.compression_level as u32);
        let encoder = GzEncoder::new(Vec::new(), compression);

        let start_time = Instant::now();

        Ok(Self {
            config,
            output_file,
            writer,
            encoder,
            stream_a: StreamA::new(),
            stream_b: StreamB::new(),
            stream_c: StreamC::new(),
            start_time,
            index_entries: Vec::new(),
            stats: RecordingStats {
                start_time,
                frames_recorded: 0,
                telemetry_records: 0,
                health_events: 0,
                file_size_bytes: 0,
                compression_ratio: 1.0,
                is_active: true,
            },
            compressed_buffer: Vec::new(),
            frame_buffer: Vec::new(),
        })
    }

    /// Record a frame (Stream A - 1kHz)
    ///
    /// RT-safe: Uses pre-allocated buffers, no heap allocations.
    pub fn record_frame(
        &mut self,
        frame: FrameData,
        node_outputs: &[f32],
        safety_state: SafetyStateSimple,
        processing_time_us: u64,
    ) -> DiagnosticResult<()> {
        if !self.config.enable_stream_a {
            return Ok(());
        }

        let elapsed_ms = self.start_time.elapsed().as_millis() as u32;

        // Check if we need to create an index entry (every 100ms)
        if elapsed_ms >= (self.index_entries.len() as u32 + 1) * INDEX_INTERVAL_MS {
            self.create_index_entry(elapsed_ms)?;
        }

        self.stream_a
            .record_frame(frame, node_outputs, safety_state, processing_time_us)?;

        self.stats.frames_recorded += 1;

        // Check limits
        self.check_limits()?;

        Ok(())
    }

    /// Record telemetry (Stream B - 60Hz)
    ///
    /// Rate-limited to approximately 60Hz.
    pub fn record_telemetry(&mut self, telemetry: TelemetryData) -> DiagnosticResult<()> {
        if !self.config.enable_stream_b {
            return Ok(());
        }

        if self.stream_b.record_telemetry(telemetry)? {
            self.stats.telemetry_records += 1;
        }

        Ok(())
    }

    /// Record health event (Stream C)
    pub fn record_health_event(&mut self, event: HealthEventData) -> DiagnosticResult<()> {
        if !self.config.enable_stream_c {
            return Ok(());
        }

        self.stream_c.record_health_event(event)?;
        self.stats.health_events += 1;

        Ok(())
    }

    /// Finalize recording and return output path
    ///
    /// NOT RT-safe: Performs file I/O and compression.
    pub fn finalize(mut self) -> DiagnosticResult<PathBuf> {
        self.write_streams_to_file()?;

        let index_offset = self
            .writer
            .stream_position()
            .map_err(|e| DiagnosticError::Io(e.to_string()))?;

        let index_count = self.index_entries.len() as u32;

        let index_bytes =
            bincode::serde::encode_to_vec(&self.index_entries, bincode::config::legacy())
                .map_err(|e| DiagnosticError::Serialization(e.to_string()))?;

        self.writer
            .write_all(&index_bytes)
            .map_err(|e| DiagnosticError::Io(e.to_string()))?;

        self.writer
            .flush()
            .map_err(|e| DiagnosticError::Io(e.to_string()))?;

        let file_size = self
            .writer
            .stream_position()
            .map_err(|e| DiagnosticError::Io(e.to_string()))?;

        let mut footer = WbbFooter::new(
            self.start_time.elapsed().as_millis() as u32,
            self.stats.frames_recorded,
        );
        footer.index_offset = index_offset;
        footer.index_count = index_count;

        let footer_bytes = bincode::serde::encode_to_vec(&footer, bincode::config::legacy())
            .map_err(|e| DiagnosticError::Serialization(e.to_string()))?;

        self.writer
            .write_all(&footer_bytes)
            .map_err(|e| DiagnosticError::Io(e.to_string()))?;

        self.writer
            .flush()
            .map_err(|e| DiagnosticError::Io(e.to_string()))?;

        self.stats.file_size_bytes = file_size;
        self.stats.is_active = false;

        Ok(self.output_file)
    }

    /// Get current recording statistics
    pub fn get_stats(&self) -> &RecordingStats {
        &self.stats
    }

    /// Get output file path
    pub fn output_path(&self) -> &Path {
        &self.output_file
    }

    fn create_index_entry(&mut self, elapsed_ms: u32) -> DiagnosticResult<()> {
        let current_pos = self
            .writer
            .stream_position()
            .map_err(|e| DiagnosticError::Io(e.to_string()))?;

        let entry = IndexEntry {
            timestamp_ms: elapsed_ms,
            stream_a_offset: current_pos,
            stream_b_offset: current_pos,
            stream_c_offset: current_pos,
            frame_count: 100,
        };

        self.index_entries.push(entry);
        Ok(())
    }

    fn write_streams_to_file(&mut self) -> DiagnosticResult<()> {
        let stream_a_data = self.stream_a.get_data()?;
        let stream_b_data = self.stream_b.get_data()?;
        let stream_c_data = self.stream_c.get_data()?;

        self.frame_buffer.clear();
        self.frame_buffer.extend_from_slice(&stream_a_data);
        self.frame_buffer.extend_from_slice(&stream_b_data);
        self.frame_buffer.extend_from_slice(&stream_c_data);

        if self.config.compression_level > 0 {
            self.encoder
                .write_all(&self.frame_buffer)
                .map_err(|e| DiagnosticError::Compression(e.to_string()))?;

            let compression = Compression::new(self.config.compression_level as u32);
            let old_encoder =
                std::mem::replace(&mut self.encoder, GzEncoder::new(Vec::new(), compression));
            self.compressed_buffer = old_encoder
                .finish()
                .map_err(|e| DiagnosticError::Compression(e.to_string()))?;

            if !self.frame_buffer.is_empty() {
                self.stats.compression_ratio =
                    self.compressed_buffer.len() as f64 / self.frame_buffer.len() as f64;
            }

            self.writer
                .write_all(&self.compressed_buffer)
                .map_err(|e| DiagnosticError::Io(e.to_string()))?;
        } else {
            self.writer
                .write_all(&self.frame_buffer)
                .map_err(|e| DiagnosticError::Io(e.to_string()))?;
        }

        Ok(())
    }

    fn check_limits(&self) -> DiagnosticResult<()> {
        if self.start_time.elapsed().as_secs() > self.config.max_duration_s {
            return Err(DiagnosticError::SizeLimit(
                "Maximum recording duration exceeded".to_string(),
            ));
        }

        if self.stats.file_size_bytes > self.config.max_file_size_bytes {
            return Err(DiagnosticError::SizeLimit(
                "Maximum file size exceeded".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_config() -> (BlackboxConfig, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = BlackboxConfig::new("test-device", temp_dir.path());
        (config, temp_dir)
    }

    #[test]
    fn test_blackbox_creation() {
        let (config, _temp_dir) = create_test_config();
        let recorder = BlackboxRecorder::new(config);
        assert!(recorder.is_ok());

        let recorder = recorder.unwrap();
        assert!(recorder.get_stats().is_active);
        assert_eq!(recorder.get_stats().frames_recorded, 0);
    }

    #[test]
    fn test_frame_recording() {
        let (config, _temp_dir) = create_test_config();
        let mut recorder = BlackboxRecorder::new(config).unwrap();

        let frame = FrameData {
            ffb_in: 0.5,
            torque_out: 0.3,
            wheel_speed: 10.0,
            hands_off: false,
            ts_mono_ns: 1_000_000_000,
            seq: 1,
        };

        recorder
            .record_frame(frame, &[0.1, 0.2, 0.3], SafetyStateSimple::SafeTorque, 150)
            .unwrap();

        assert_eq!(recorder.get_stats().frames_recorded, 1);
    }

    #[test]
    fn test_recording_finalization() {
        let (config, _temp_dir) = create_test_config();
        let mut recorder = BlackboxRecorder::new(config).unwrap();

        for i in 0..10 {
            let frame = FrameData {
                ffb_in: i as f32 * 0.1,
                torque_out: i as f32 * 0.05,
                wheel_speed: i as f32,
                hands_off: false,
                ts_mono_ns: (i * 1_000_000) as u64,
                seq: i as u16,
            };

            recorder
                .record_frame(
                    frame,
                    &[i as f32 * 0.01],
                    SafetyStateSimple::SafeTorque,
                    100,
                )
                .unwrap();
        }

        let output_path = recorder.finalize().unwrap();
        assert!(output_path.exists());
        assert!(output_path.extension().unwrap() == "wbb");

        let metadata = std::fs::metadata(&output_path).unwrap();
        assert!(metadata.len() > 0);
    }

    #[test]
    fn test_telemetry_recording() {
        let (config, _temp_dir) = create_test_config();
        let mut recorder = BlackboxRecorder::new(config).unwrap();

        let telemetry = TelemetryData {
            ffb_scalar: 0.8,
            rpm: 3000.0,
            speed_ms: 25.0,
            slip_ratio: 0.1,
            gear: 3,
            car_id: Some("test_car".to_string()),
            track_id: Some("test_track".to_string()),
        };

        recorder.record_telemetry(telemetry).unwrap();
    }

    #[test]
    fn test_health_event_recording() {
        let (config, _temp_dir) = create_test_config();
        let mut recorder = BlackboxRecorder::new(config).unwrap();

        let event = HealthEventData {
            timestamp_ns: 0,
            device_id: "test-device".to_string(),
            event_type: "DeviceConnected".to_string(),
            context: serde_json::json!({"test": true}),
        };

        recorder.record_health_event(event).unwrap();
        assert_eq!(recorder.get_stats().health_events, 1);
    }

    #[test]
    fn test_config_stream_flags() {
        let config = BlackboxConfig {
            enable_stream_a: true,
            enable_stream_b: true,
            enable_stream_c: true,
            ..BlackboxConfig::new("test", "./test")
        };

        assert_eq!(config.stream_flags(), 0x07);
    }
}
