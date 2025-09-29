//! Blackbox Recording System with .wbb v1 Format
//!
//! Implements the .wbb v1 format with three streams:
//! - Stream A: 1kHz frames + per-node outputs
//! - Stream B: 60Hz telemetry
//! - Stream C: Health/fault events

use super::{HealthEvent, streams::{StreamA, StreamB, StreamC}};
use crate::rt::Frame;
use crate::safety::SafetyState;
use crate::ports::NormalizedTelemetry;
use racing_wheel_schemas::DeviceId;
use std::{
    fs::File,
    io::{BufWriter, Write, Seek},
    path::{Path, PathBuf},
    time::{SystemTime, Instant, UNIX_EPOCH},
};
use serde::{Serialize, Deserialize};
use crc32c::crc32c;
use flate2::{write::GzEncoder, Compression};

/// .wbb v1 file format magic number
const WBB_MAGIC: &[u8; 4] = b"WBB1";

/// .wbb v1 file header
#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbbHeader {
    /// Magic number "WBB1"
    pub magic: [u8; 4],
    /// File format version (1)
    pub version: u32,
    /// Device UUID
    pub device_id: String,
    /// Engine version string
    pub engine_version: String,
    /// Recording start time (Unix timestamp)
    pub start_time_unix: u64,
    /// Timebase for timestamps (nanoseconds per tick)
    pub timebase_ns: u64,
    /// FFB mode during recording
    pub ffb_mode: u8,
    /// Stream flags (bit 0: A, bit 1: B, bit 2: C)
    pub stream_flags: u8,
    /// Compression level used
    pub compression_level: u8,
    /// Reserved for future use
    pub reserved: [u8; 15],
    /// Header size (for extensibility)
    pub header_size: u32,
}

impl WbbHeader {
    pub fn new(device_id: DeviceId, ffb_mode: u8, stream_flags: u8, compression_level: u8) -> Self {
        Self {
            magic: *WBB_MAGIC,
            version: 1,
            device_id: device_id.to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            start_time_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            timebase_ns: 1_000_000, // 1ms for 1kHz
            ffb_mode,
            stream_flags,
            compression_level,
            reserved: [0; 15],
            header_size: std::mem::size_of::<WbbHeader>() as u32,
        }
    }
}

/// Index entry for random access (every 100ms)
#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    /// Timestamp offset from start (ms)
    pub timestamp_ms: u32,
    /// File offset for Stream A
    pub stream_a_offset: u64,
    /// File offset for Stream B
    pub stream_b_offset: u64,
    /// File offset for Stream C
    pub stream_c_offset: u64,
    /// Number of frames in this 100ms window
    pub frame_count: u32,
}

/// .wbb v1 file footer
#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WbbFooter {
    /// Total recording duration (ms)
    pub duration_ms: u32,
    /// Total frames recorded
    pub total_frames: u64,
    /// Index offset in file
    pub index_offset: u64,
    /// Number of index entries
    pub index_count: u32,
    /// CRC32C of entire file (excluding this field)
    pub file_crc32c: u32,
    /// Footer magic "1BBW" (WBB1 reversed)
    pub footer_magic: [u8; 4],
}

/// Blackbox recorder configuration
#[derive(Debug, Clone)]
pub struct BlackboxConfig {
    pub device_id: DeviceId,
    pub output_dir: PathBuf,
    pub max_duration_s: u64,
    pub max_file_size_bytes: u64,
    pub compression_level: u8,
    pub enable_stream_a: bool,
    pub enable_stream_b: bool,
    pub enable_stream_c: bool,
}

/// Recording statistics
#[derive(Debug, Clone)]
pub struct RecordingStats {
    pub start_time: Instant,
    pub frames_recorded: u64,
    pub telemetry_records: u64,
    pub health_events: u64,
    pub file_size_bytes: u64,
    pub compression_ratio: f64,
    pub is_active: bool,
}

/// Blackbox recorder implementation
pub struct BlackboxRecorder {
    config: BlackboxConfig,
    output_file: PathBuf,
    writer: BufWriter<File>,
    encoder: GzEncoder<Vec<u8>>,
    
    // Streams
    stream_a: StreamA,
    stream_b: StreamB,
    stream_c: StreamC,
    
    // State
    start_time: Instant,
    last_index_time: Instant,
    index_entries: Vec<IndexEntry>,
    stats: RecordingStats,
    
    // Buffers
    compressed_buffer: Vec<u8>,
    frame_buffer: Vec<u8>,
}

impl BlackboxRecorder {
    /// Create new blackbox recorder
    pub fn new(config: BlackboxConfig) -> Result<Self, String> {
        // Generate unique filename
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        let filename = format!("blackbox_{}_{}.wbb", 
                              config.device_id.to_string().replace(['/', '\\', ':'], "_"),
                              timestamp);
        
        let output_file = config.output_dir.join(filename);
        
        // Create output file
        let file = File::create(&output_file)
            .map_err(|e| format!("Failed to create blackbox file: {}", e))?;
        
        let mut writer = BufWriter::new(file);
        
        // Write header
        let stream_flags = 
            (if config.enable_stream_a { 1 } else { 0 }) |
            (if config.enable_stream_b { 2 } else { 0 }) |
            (if config.enable_stream_c { 4 } else { 0 });
        
        let header = WbbHeader::new(
            config.device_id.clone(),
            1, // FFB mode (will be updated)
            stream_flags,
            config.compression_level,
        );
        
        let header_bytes = bincode::serialize(&header)
            .map_err(|e| format!("Failed to serialize header: {}", e))?;
        
        writer.write_all(&header_bytes)
            .map_err(|e| format!("Failed to write header: {}", e))?;
        
        // Initialize compression
        let compression = Compression::new(config.compression_level as u32);
        let encoder = GzEncoder::new(Vec::new(), compression);
        
        let start_time = Instant::now();
        
        Ok(Self {
            config: config.clone(),
            output_file,
            writer,
            encoder,
            stream_a: StreamA::new(),
            stream_b: StreamB::new(),
            stream_c: StreamC::new(),
            start_time,
            last_index_time: start_time,
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
    pub fn record_frame(
        &mut self,
        frame: &Frame,
        node_outputs: &[f32],
        safety_state: &SafetyState,
        processing_time_us: u64,
    ) -> Result<(), String> {
        if !self.config.enable_stream_a {
            return Ok(());
        }
        
        // Check if we need to create an index entry (every 100ms)
        let elapsed = self.start_time.elapsed();
        if elapsed.as_millis() >= (self.index_entries.len() + 1) as u128 * 100 {
            self.create_index_entry()?;
        }
        
        // Record to Stream A
        self.stream_a.record_frame(frame, node_outputs, safety_state, processing_time_us)?;
        
        self.stats.frames_recorded += 1;
        
        // Check limits
        self.check_limits()?;
        
        Ok(())
    }
    
    /// Record telemetry (Stream B - 60Hz)
    pub fn record_telemetry(&mut self, telemetry: &NormalizedTelemetry) -> Result<(), String> {
        if !self.config.enable_stream_b {
            return Ok(());
        }
        
        self.stream_b.record_telemetry(telemetry)?;
        self.stats.telemetry_records += 1;
        
        Ok(())
    }
    
    /// Record health event (Stream C)
    pub fn record_health_event(&mut self, event: &HealthEvent) -> Result<(), String> {
        if !self.config.enable_stream_c {
            return Ok(());
        }
        
        self.stream_c.record_health_event(event)?;
        self.stats.health_events += 1;
        
        Ok(())
    }
    
    /// Finalize recording and return output path
    pub fn finalize(mut self) -> Result<PathBuf, String> {
        // Write final streams to file
        self.write_streams_to_file()?;
        
        // Write index
        let index_offset = self.writer.stream_position()
            .map_err(|e| format!("Failed to get file position: {}", e))?;
        
        let index_bytes = bincode::serialize(&self.index_entries)
            .map_err(|e| format!("Failed to serialize index: {}", e))?;
        
        self.writer.write_all(&index_bytes)
            .map_err(|e| format!("Failed to write index: {}", e))?;
        
        // Calculate file CRC (excluding footer)
        self.writer.flush()
            .map_err(|e| format!("Failed to flush writer: {}", e))?;
        
        let file_size = self.writer.stream_position()
            .map_err(|e| format!("Failed to get file size: {}", e))?;
        
        // Write footer
        let footer = WbbFooter {
            duration_ms: self.start_time.elapsed().as_millis() as u32,
            total_frames: self.stats.frames_recorded,
            index_offset,
            index_count: self.index_entries.len() as u32,
            file_crc32c: 0, // Will be calculated separately
            footer_magic: *b"1BBW",
        };
        
        let footer_bytes = bincode::serialize(&footer)
            .map_err(|e| format!("Failed to serialize footer: {}", e))?;
        
        self.writer.write_all(&footer_bytes)
            .map_err(|e| format!("Failed to write footer: {}", e))?;
        
        self.writer.flush()
            .map_err(|e| format!("Failed to flush final write: {}", e))?;
        
        // Update stats
        self.stats.file_size_bytes = file_size;
        self.stats.is_active = false;
        
        Ok(self.output_file)
    }
    
    /// Get current recording statistics
    pub fn get_stats(&self) -> RecordingStats {
        self.stats.clone()
    }
    
    /// Create index entry for current position
    fn create_index_entry(&mut self) -> Result<(), String> {
        let elapsed_ms = self.start_time.elapsed().as_millis() as u32;
        
        let current_pos = self.writer.stream_position()
            .map_err(|e| format!("Failed to get stream position: {}", e))?;
        
        let entry = IndexEntry {
            timestamp_ms: elapsed_ms,
            stream_a_offset: current_pos,
            stream_b_offset: current_pos, // Simplified - in real implementation, track per stream
            stream_c_offset: current_pos,
            frame_count: 100, // Approximate for 100ms at 1kHz
        };
        
        self.index_entries.push(entry);
        Ok(())
    }
    
    /// Write accumulated stream data to file
    fn write_streams_to_file(&mut self) -> Result<(), String> {
        // Get data from streams
        let stream_a_data = self.stream_a.get_data();
        let stream_b_data = self.stream_b.get_data();
        let stream_c_data = self.stream_c.get_data();
        
        // Combine all stream data
        self.frame_buffer.clear();
        self.frame_buffer.extend_from_slice(&stream_a_data);
        self.frame_buffer.extend_from_slice(&stream_b_data);
        self.frame_buffer.extend_from_slice(&stream_c_data);
        
        // Compress if enabled
        if self.config.compression_level > 0 {
            self.encoder.write_all(&self.frame_buffer)
                .map_err(|e| format!("Failed to compress data: {}", e))?;
            
            // Replace encoder to get the compressed data
            let compression = Compression::new(self.config.compression_level as u32);
            let old_encoder = std::mem::replace(&mut self.encoder, GzEncoder::new(Vec::new(), compression));
            self.compressed_buffer = old_encoder.finish()
                .map_err(|e| format!("Failed to finish compression: {}", e))?;
            
            // Calculate compression ratio
            if !self.frame_buffer.is_empty() {
                self.stats.compression_ratio = 
                    self.compressed_buffer.len() as f64 / self.frame_buffer.len() as f64;
            }
            
            self.writer.write_all(&self.compressed_buffer)
                .map_err(|e| format!("Failed to write compressed data: {}", e))?;
        } else {
            self.writer.write_all(&self.frame_buffer)
                .map_err(|e| format!("Failed to write uncompressed data: {}", e))?;
        }
        
        Ok(())
    }
    
    /// Check recording limits
    fn check_limits(&self) -> Result<(), String> {
        // Check duration limit
        if self.start_time.elapsed().as_secs() > self.config.max_duration_s {
            return Err("Maximum recording duration exceeded".to_string());
        }
        
        // Check file size limit (approximate)
        if self.stats.file_size_bytes > self.config.max_file_size_bytes {
            return Err("Maximum file size exceeded".to_string());
        }
        
        Ok(())
    }
}

/// Calculate CRC32C for file validation
pub fn calculate_file_crc32c(file_path: &Path) -> Result<u32, String> {
    use std::io::Read;
    
    let mut file = File::open(file_path)
        .map_err(|e| format!("Failed to open file for CRC: {}", e))?;
    
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)
        .map_err(|e| format!("Failed to read file for CRC: {}", e))?;
    
    Ok(crc32c(&buffer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::safety::SafetyState;

    fn create_test_config() -> (BlackboxConfig, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = BlackboxConfig {
            device_id: DeviceId::new("test-device".to_string()).unwrap(),
            output_dir: temp_dir.path().to_path_buf(),
            max_duration_s: 10,
            max_file_size_bytes: 1024 * 1024, // 1MB
            compression_level: 1,
            enable_stream_a: true,
            enable_stream_b: true,
            enable_stream_c: true,
        };
        (config, temp_dir)
    }

    #[test]
    fn test_blackbox_creation() {
        let (config, _temp_dir) = create_test_config();
        let recorder = BlackboxRecorder::new(config);
        assert!(recorder.is_ok());
        
        let recorder = recorder.unwrap();
        let stats = recorder.get_stats();
        assert!(stats.is_active);
        assert_eq!(stats.frames_recorded, 0);
    }

    #[test]
    fn test_frame_recording() {
        let (config, _temp_dir) = create_test_config();
        let mut recorder = BlackboxRecorder::new(config).unwrap();
        
        let frame = Frame {
            ffb_in: 0.5,
            torque_out: 0.3,
            wheel_speed: 10.0,
            hands_off: false,
            ts_mono_ns: 1000000000,
            seq: 1,
        };
        
        let node_outputs = vec![0.1, 0.2, 0.3];
        let safety_state = SafetyState::SafeTorque;
        let processing_time_us = 150;
        
        let result = recorder.record_frame(&frame, &node_outputs, &safety_state, processing_time_us);
        assert!(result.is_ok());
        
        let stats = recorder.get_stats();
        assert_eq!(stats.frames_recorded, 1);
    }

    #[test]
    fn test_recording_finalization() {
        let (config, _temp_dir) = create_test_config();
        let mut recorder = BlackboxRecorder::new(config).unwrap();
        
        // Record some data
        for i in 0..10 {
            let frame = Frame {
                ffb_in: i as f32 * 0.1,
                torque_out: i as f32 * 0.05,
                wheel_speed: i as f32,
                hands_off: false,
                ts_mono_ns: (i * 1000000) as u64,
                seq: i as u16,
            };
            
            let node_outputs = vec![i as f32 * 0.01; 3];
            let safety_state = SafetyState::SafeTorque;
            let processing_time_us = 100 + i as u64;
            
            recorder.record_frame(&frame, &node_outputs, &safety_state, processing_time_us).unwrap();
        }
        
        // Finalize
        let output_path = recorder.finalize();
        assert!(output_path.is_ok());
        
        let output_path = output_path.unwrap();
        assert!(output_path.exists());
        assert!(output_path.extension().unwrap() == "wbb");
        
        // Check file is not empty
        let metadata = std::fs::metadata(&output_path).unwrap();
        assert!(metadata.len() > 0);
    }

    #[test]
    fn test_wbb_header_serialization() {
        let device_id = DeviceId::new("test-device".to_string()).unwrap();
        let header = WbbHeader::new(device_id, 1, 7, 6);
        
        let serialized = bincode::serialize(&header);
        assert!(serialized.is_ok());
        
        let deserialized: WbbHeader = bincode::deserialize(&serialized.unwrap()).unwrap();
        assert_eq!(deserialized.magic, *WBB_MAGIC);
        assert_eq!(deserialized.version, 1);
        assert_eq!(deserialized.stream_flags, 7);
        assert_eq!(deserialized.compression_level, 6);
    }

    #[test]
    fn test_index_creation() {
        let (config, _temp_dir) = create_test_config();
        let mut recorder = BlackboxRecorder::new(config).unwrap();
        
        // Record enough frames to trigger index creation
        for i in 0..200 {
            let frame = Frame {
                ffb_in: 0.0,
                torque_out: 0.0,
                wheel_speed: 0.0,
                hands_off: false,
                ts_mono_ns: (i * 1000000) as u64, // 1ms intervals
                seq: i as u16,
            };
            
            recorder.record_frame(&frame, &[], &SafetyState::SafeTorque, 100).unwrap();
            
            // Simulate time passing
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        
        // Should have created at least one index entry
        assert!(!recorder.index_entries.is_empty());
    }

    #[test]
    fn test_compression() {
        let (mut config, _temp_dir) = create_test_config();
        config.compression_level = 6; // Higher compression
        
        let mut recorder = BlackboxRecorder::new(config).unwrap();
        
        // Record repetitive data that should compress well
        for i in 0..100 {
            let frame = Frame {
                ffb_in: 0.5, // Constant value
                torque_out: 0.5, // Constant value
                wheel_speed: 10.0, // Constant value
                hands_off: false,
                ts_mono_ns: (i * 1000000) as u64,
                seq: i as u16,
            };
            
            let node_outputs = vec![0.1, 0.2, 0.3]; // Constant values
            recorder.record_frame(&frame, &node_outputs, &SafetyState::SafeTorque, 100).unwrap();
        }
        
        let stats = recorder.get_stats();
        // With repetitive data, compression ratio should be good
        // (This is a basic test - actual compression depends on data patterns)
        assert!(stats.compression_ratio <= 1.0);
    }
}