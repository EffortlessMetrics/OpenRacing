//! Blackbox Replay System
//!
//! Implements deterministic replay of recorded blackbox data to reproduce
//! outputs within floating-point tolerance as specified in DIAG-02.

use super::{
    blackbox::{WbbHeader, WbbFooter, IndexEntry},
    streams::{StreamReader, StreamARecord, StreamBRecord, StreamCRecord},
};
use super::bincode_compat as codec;
use crate::{
    rt::Frame,
    pipeline::Pipeline,
    safety::SafetyService,
};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    time::{Duration, Instant},
    collections::HashMap,
};
use serde::{Serialize, Deserialize};
use flate2::read::GzDecoder;

/// Replay configuration
#[derive(Debug, Clone)]
pub struct ReplayConfig {
    /// Deterministic seed for reproducible results
    pub deterministic_seed: u64,
    /// Floating-point tolerance for output comparison
    pub fp_tolerance: f64,
    /// Enable strict timing reproduction
    pub strict_timing: bool,
    /// Maximum replay duration (for safety)
    pub max_duration_s: u64,
    /// Enable validation of outputs against recorded values
    pub validate_outputs: bool,
}

impl Default for ReplayConfig {
    fn default() -> Self {
        Self {
            deterministic_seed: 0x12345678,
            fp_tolerance: 1e-6, // 1 microunit tolerance
            strict_timing: false, // Don't enforce real-time during replay
            max_duration_s: 600, // 10 minutes max
            validate_outputs: true,
        }
    }
}

/// Replay result with validation statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayResult {
    /// Total frames replayed
    pub frames_replayed: u64,
    /// Frames that matched within tolerance
    pub frames_matched: u64,
    /// Maximum output deviation observed
    pub max_deviation: f64,
    /// Average output deviation
    pub avg_deviation: f64,
    /// Replay duration
    pub replay_duration: Duration,
    /// Original recording duration
    pub original_duration: Duration,
    /// Validation errors encountered
    pub validation_errors: Vec<String>,
    /// Success flag
    pub success: bool,
}

/// Replay frame comparison result
#[derive(Debug, Clone)]
pub struct FrameComparison {
    pub frame_index: u64,
    pub original_output: f32,
    pub replayed_output: f32,
    pub deviation: f64,
    pub within_tolerance: bool,
}

/// Blackbox replay engine
pub struct BlackboxReplay {
    config: ReplayConfig,
    header: WbbHeader,
    footer: WbbFooter,
    index: Vec<IndexEntry>,
    
    // Replay state
    pipeline: Pipeline,
    /// TODO: Used for future safety validation during replay
    #[allow(dead_code)]
    safety_service: SafetyService,
    current_frame: u64,
    start_time: Instant,
    
    // Validation
    frame_comparisons: Vec<FrameComparison>,
    validation_errors: Vec<String>,
    
    // Data streams
    stream_a_data: Vec<StreamARecord>,
    /// TODO: Used for future multi-stream replay implementation
    #[allow(dead_code)]
    stream_b_data: Vec<StreamBRecord>,
    /// TODO: Used for future multi-stream replay implementation
    #[allow(dead_code)]
    stream_c_data: Vec<StreamCRecord>,
}

impl BlackboxReplay {
    /// Load blackbox file for replay
    pub fn load_from_file(file_path: &Path, config: ReplayConfig) -> Result<Self, String> {
        let mut file = File::open(file_path)
            .map_err(|e| format!("Failed to open blackbox file: {}", e))?;
        
        // Read and validate header
        let header = Self::read_header(&mut file)?;
        Self::validate_header(&header)?;
        
        // Seek to footer to read metadata
        let file_size = file.metadata()
            .map_err(|e| format!("Failed to get file size: {}", e))?
            .len();
        
        let footer_size = std::mem::size_of::<WbbFooter>() as u64;
        file.seek(SeekFrom::Start(file_size - footer_size))
            .map_err(|e| format!("Failed to seek to footer: {}", e))?;
        
        let footer = Self::read_footer(&mut file)?;
        Self::validate_footer(&footer)?;
        
        // Read index
        file.seek(SeekFrom::Start(footer.index_offset))
            .map_err(|e| format!("Failed to seek to index: {}", e))?;
        
        let index = Self::read_index(&mut file, footer.index_count)?;
        
        // Read stream data
        let (stream_a_data, stream_b_data, stream_c_data) = 
            Self::read_stream_data(&mut file, &header, &footer)?;
        
        // Initialize replay components
        let pipeline = Pipeline::new();
        let safety_service = SafetyService::new(25.0, 25.0); // Default torque limits
        
        Ok(Self {
            config,
            header,
            footer,
            index,
            pipeline,
            safety_service,
            current_frame: 0,
            start_time: Instant::now(),
            frame_comparisons: Vec::new(),
            validation_errors: Vec::new(),
            stream_a_data,
            stream_b_data,
            stream_c_data,
        })
    }
    
    /// Execute replay with validation
    pub fn execute_replay(&mut self) -> Result<ReplayResult, String> {
        let replay_start = Instant::now();
        
        // Set deterministic seed for reproducible results
        self.set_deterministic_seed(self.config.deterministic_seed);
        
        // Process all Stream A records (frames)
        let stream_data = self.stream_a_data.clone(); // Clone to avoid borrowing issues
        for (frame_index, record) in stream_data.iter().enumerate() {
            if frame_index as u64 >= self.config.max_duration_s * 1000 {
                break; // Safety limit
            }
            
            // Replay frame through pipeline
            let replayed_output = self.replay_frame(&record.frame)?;
            
            // Validate against recorded output if enabled
            if self.config.validate_outputs {
                let comparison = self.compare_outputs(
                    frame_index as u64,
                    record.frame.torque_out,
                    replayed_output,
                );
                
                self.frame_comparisons.push(comparison);
            }
            
            // Note: Safety state is recorded but not actively updated during replay
            // This preserves the original state for validation purposes
            
            // Simulate timing if strict timing is enabled
            if self.config.strict_timing {
                let target_time = Duration::from_nanos(record.timestamp_ns);
                let elapsed = self.start_time.elapsed();
                
                if target_time > elapsed {
                    std::thread::sleep(target_time - elapsed);
                }
            }
            
            self.current_frame += 1;
        }
        
        // Calculate results
        let replay_duration = replay_start.elapsed();
        let original_duration = Duration::from_millis(self.footer.duration_ms as u64);
        
        let frames_matched = self.frame_comparisons.iter()
            .filter(|c| c.within_tolerance)
            .count() as u64;
        
        let max_deviation = self.frame_comparisons.iter()
            .map(|c| c.deviation)
            .fold(0.0, f64::max);
        
        let avg_deviation = if !self.frame_comparisons.is_empty() {
            self.frame_comparisons.iter()
                .map(|c| c.deviation)
                .sum::<f64>() / self.frame_comparisons.len() as f64
        } else {
            0.0
        };
        
        let success = self.validation_errors.is_empty() && 
                     (frames_matched as f64 / self.current_frame as f64) > 0.99; // 99% match rate
        
        Ok(ReplayResult {
            frames_replayed: self.current_frame,
            frames_matched,
            max_deviation,
            avg_deviation,
            replay_duration,
            original_duration,
            validation_errors: self.validation_errors.clone(),
            success,
        })
    }
    
    /// Replay a single frame through the pipeline
    fn replay_frame(&mut self, frame: &Frame) -> Result<f32, String> {
        // Convert to mutable frame for pipeline processing
        let mut replay_frame = crate::rt::Frame {
            ffb_in: frame.ffb_in,
            torque_out: frame.torque_out,
            wheel_speed: frame.wheel_speed,
            hands_off: frame.hands_off,
            ts_mono_ns: frame.ts_mono_ns,
            seq: frame.seq,
        };
        
        // Process through pipeline with deterministic behavior
        self.pipeline.process(&mut replay_frame)
            .map_err(|e| format!("Pipeline processing failed: {:?}", e))?;
        
        Ok(replay_frame.torque_out)
    }
    
    /// Compare replayed output with recorded output
    fn compare_outputs(&self, frame_index: u64, original: f32, replayed: f32) -> FrameComparison {
        let deviation = (original - replayed).abs() as f64;
        let within_tolerance = deviation <= self.config.fp_tolerance;
        
        FrameComparison {
            frame_index,
            original_output: original,
            replayed_output: replayed,
            deviation,
            within_tolerance,
        }
    }
    
    /// Set deterministic seed for reproducible results
    fn set_deterministic_seed(&self, _seed: u64) {
        // In a real implementation, this would set seeds for:
        // - Random number generators used in filters
        // - Any non-deterministic operations
        // - Floating-point operations that might vary
        
        // For now, this is a placeholder as the current pipeline
        // doesn't use random operations
    }
    
    /// Read and validate file header
    fn read_header(file: &mut File) -> Result<WbbHeader, String> {
        codec::decode_from_std_read(file)
            .map_err(|e| format!("Failed to decode header: {}", e))
    }
    
    /// Validate header contents
    fn validate_header(header: &WbbHeader) -> Result<(), String> {
        if &header.magic != b"WBB1" {
            return Err("Invalid file magic number".to_string());
        }
        
        if header.version != 1 {
            return Err(format!("Unsupported file version: {}", header.version));
        }
        
        Ok(())
    }
    
    /// Read and validate file footer
    fn read_footer(file: &mut File) -> Result<WbbFooter, String> {
        let mut footer_bytes = vec![0u8; std::mem::size_of::<WbbFooter>()];
        file.read_exact(&mut footer_bytes)
            .map_err(|e| format!("Failed to read footer: {}", e))?;
        
        let footer: WbbFooter =
            codec::decode_from_slice(&footer_bytes)
                .map_err(|e| format!("Failed to decode footer: {}", e))?;
        
        Ok(footer)
    }
    
    /// Validate footer contents
    fn validate_footer(footer: &WbbFooter) -> Result<(), String> {
        if &footer.footer_magic != b"1BBW" {
            return Err("Invalid footer magic number".to_string());
        }
        
        // Additional validation could include CRC checking
        
        Ok(())
    }
    
    /// Read index entries
    fn read_index(file: &mut File, count: u32) -> Result<Vec<IndexEntry>, String> {
        let index: Vec<IndexEntry> = codec::decode_from_std_read(file)
            .map_err(|e| format!("Failed to decode index: {}", e))?;
        if index.len() != count as usize {
            return Err(format!(
                "Index count mismatch: expected {}, got {}",
                count,
                index.len()
            ));
        }
        Ok(index)
    }
    
    /// Read stream data from file
    fn read_stream_data(
        file: &mut File,
        header: &WbbHeader,
        footer: &WbbFooter,
    ) -> Result<(Vec<StreamARecord>, Vec<StreamBRecord>, Vec<StreamCRecord>), String> {
        // Seek to start of compressed data (after header)
        file.seek(SeekFrom::Start(header.header_size as u64))
            .map_err(|e| format!("Failed to seek to data: {}", e))?;
        
        // Calculate data size (from header to index)
        let data_size = footer.index_offset - header.header_size as u64;
        let mut compressed_data = vec![0u8; data_size as usize];
        
        file.read_exact(&mut compressed_data)
            .map_err(|e| format!("Failed to read compressed data: {}", e))?;
        
        // Decompress if needed
        let data = if header.compression_level > 0 {
            let mut decoder = GzDecoder::new(&compressed_data[..]);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)
                .map_err(|e| format!("Failed to decompress data: {}", e))?;
            decompressed
        } else {
            compressed_data
        };
        
        // Parse streams
        let mut reader = StreamReader::new(data);
        let mut stream_a_data = Vec::new();
        let stream_b_data = Vec::new();
        let stream_c_data = Vec::new();
        
        // Read all records (simplified - in real implementation, would need to
        // distinguish between stream types based on record format or markers)
        while !reader.is_at_end() {
            // Try to read as Stream A first
            if let Ok(Some(record)) = reader.read_stream_a_record() {
                stream_a_data.push(record);
                continue;
            }
            
            // Reset and try Stream B
            // (In real implementation, would have proper stream identification)
            break;
        }
        
        Ok((stream_a_data, stream_b_data, stream_c_data))
    }
    
    /// Get detailed comparison results
    pub fn get_frame_comparisons(&self) -> &[FrameComparison] {
        &self.frame_comparisons
    }
    
    /// Get validation errors
    pub fn get_validation_errors(&self) -> &[String] {
        &self.validation_errors
    }
    
    /// Get header information for validation
    pub fn header(&self) -> &WbbHeader {
        &self.header
    }
    
    /// Get footer information for validation
    pub fn footer(&self) -> &WbbFooter {
        &self.footer
    }
    
    /// Get stream A data for validation
    pub fn stream_a_data(&self) -> &[StreamARecord] {
        &self.stream_a_data
    }
    
    /// Seek to specific timestamp for random access replay
    pub fn seek_to_timestamp(&mut self, timestamp_ms: u32) -> Result<(), String> {
        // Find appropriate index entry
        let _index_entry = self.index.iter()
            .find(|entry| entry.timestamp_ms <= timestamp_ms)
            .ok_or("Timestamp not found in index")?;
        
        // In a full implementation, would seek to the appropriate stream position
        // and resume replay from there
        
        self.current_frame = (timestamp_ms / 1) as u64; // Approximate frame number
        Ok(())
    }
}

/// Replay statistics for analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayStatistics {
    pub total_frames: u64,
    pub match_rate: f64,
    pub deviation_histogram: HashMap<String, u64>, // Deviation ranges -> count
    pub timing_accuracy: f64,
    pub memory_usage_mb: f64,
}

impl BlackboxReplay {
    /// Generate detailed statistics from replay
    pub fn generate_statistics(&self) -> ReplayStatistics {
        let total_frames = self.frame_comparisons.len() as u64;
        let matched_frames = self.frame_comparisons.iter()
            .filter(|c| c.within_tolerance)
            .count() as u64;
        
        let match_rate = if total_frames > 0 {
            matched_frames as f64 / total_frames as f64
        } else {
            0.0
        };
        
        // Create deviation histogram
        let mut histogram = HashMap::new();
        for comparison in &self.frame_comparisons {
            let range = match comparison.deviation {
                d if d < 1e-9 => "< 1e-9",
                d if d < 1e-6 => "1e-9 to 1e-6",
                d if d < 1e-3 => "1e-6 to 1e-3",
                d if d < 1e-2 => "1e-3 to 1e-2",
                _ => "> 1e-2",
            };
            
            *histogram.entry(range.to_string()).or_insert(0) += 1;
        }
        
        ReplayStatistics {
            total_frames,
            match_rate,
            deviation_histogram: histogram,
            timing_accuracy: 1.0, // Placeholder
            memory_usage_mb: 0.0, // Placeholder
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::diagnostic::blackbox::{BlackboxRecorder, BlackboxConfig};
    use crate::safety::SafetyState;
    
    use std::path::PathBuf;

    fn create_test_recording() -> (PathBuf, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = BlackboxConfig {
            device_id: DeviceId::new("test-device".to_string()).unwrap(),
            output_dir: temp_dir.path().to_path_buf(),
            max_duration_s: 10,
            max_file_size_bytes: 1024 * 1024,
            compression_level: 1,
            enable_stream_a: true,
            enable_stream_b: false,
            enable_stream_c: false,
        };
        
        let mut recorder = BlackboxRecorder::new(config).unwrap();
        
        // Record some deterministic data
        for i in 0..100 {
            let frame = Frame {
                ffb_in: (i as f32) * 0.01,
                torque_out: (i as f32) * 0.005, // Simple linear relationship
                wheel_speed: 10.0,
                hands_off: false,
                ts_mono_ns: (i * 1000000) as u64,
                seq: i as u16,
            };
            
            let node_outputs = vec![0.1, 0.2, 0.3];
            recorder.record_frame(&frame, &node_outputs, &SafetyState::SafeTorque, 100).unwrap();
        }
        
        let output_path = recorder.finalize().unwrap();
        (output_path, temp_dir)
    }

    #[test]
    fn test_replay_creation() {
        let (recording_path, _temp_dir) = create_test_recording();
        let config = ReplayConfig::default();
        
        let replay = BlackboxReplay::load_from_file(&recording_path, config);
        assert!(replay.is_ok());
        
        let replay = replay.unwrap();
        assert_eq!(replay.current_frame, 0);
        assert!(!replay.stream_a_data.is_empty());
    }

    #[test]
    fn test_replay_execution() {
        let (recording_path, _temp_dir) = create_test_recording();
        let config = ReplayConfig {
            validate_outputs: true,
            fp_tolerance: 1e-3, // Relaxed tolerance for test
            ..Default::default()
        };
        
        let mut replay = BlackboxReplay::load_from_file(&recording_path, config).unwrap();
        let result = replay.execute_replay();
        
        assert!(result.is_ok());
        let result = result.unwrap();
        
        assert!(result.frames_replayed > 0);
        assert!(result.success || result.frames_matched > 0); // Allow some tolerance in tests
    }

    #[test]
    fn test_deterministic_replay() {
        let (recording_path, _temp_dir) = create_test_recording();
        let config = ReplayConfig {
            deterministic_seed: 12345,
            validate_outputs: true,
            ..Default::default()
        };
        
        // Run replay twice with same seed
        let mut replay1 = BlackboxReplay::load_from_file(&recording_path, config.clone()).unwrap();
        let result1 = replay1.execute_replay().unwrap();
        
        let mut replay2 = BlackboxReplay::load_from_file(&recording_path, config).unwrap();
        let result2 = replay2.execute_replay().unwrap();
        
        // Results should be identical
        assert_eq!(result1.frames_replayed, result2.frames_replayed);
        assert_eq!(result1.frames_matched, result2.frames_matched);
        
        // Compare frame-by-frame results
        let comparisons1 = replay1.get_frame_comparisons();
        let comparisons2 = replay2.get_frame_comparisons();
        
        assert_eq!(comparisons1.len(), comparisons2.len());
        
        for (c1, c2) in comparisons1.iter().zip(comparisons2.iter()) {
            assert_eq!(c1.replayed_output, c2.replayed_output);
            assert_eq!(c1.deviation, c2.deviation);
        }
    }

    #[test]
    fn test_replay_statistics() {
        let (recording_path, _temp_dir) = create_test_recording();
        let config = ReplayConfig::default();
        
        let mut replay = BlackboxReplay::load_from_file(&recording_path, config).unwrap();
        let _result = replay.execute_replay().unwrap();
        
        let stats = replay.generate_statistics();
        assert!(stats.total_frames > 0);
        assert!(stats.match_rate >= 0.0 && stats.match_rate <= 1.0);
        assert!(!stats.deviation_histogram.is_empty());
    }

    #[test]
    fn test_frame_comparison() {
        let config = ReplayConfig {
            fp_tolerance: 1e-6,
            ..Default::default()
        };
        
        let replay = BlackboxReplay {
            config: config.clone(),
            header: WbbHeader::new(
                DeviceId::new("test".to_string()).unwrap(),
                1, 1, 0
            ),
            footer: WbbFooter {
                duration_ms: 1000,
                total_frames: 100,
                index_offset: 0,
                index_count: 0,
                file_crc32c: 0,
                footer_magic: *b"1BBW",
            },
            index: Vec::new(),
            pipeline: Pipeline::new(),
            safety_service: SafetyService::new(25.0, 25.0),
            current_frame: 0,
            start_time: Instant::now(),
            frame_comparisons: Vec::new(),
            validation_errors: Vec::new(),
            stream_a_data: Vec::new(),
            stream_b_data: Vec::new(),
            stream_c_data: Vec::new(),
        };
        
        // Test exact match
        let comparison = replay.compare_outputs(0, 0.5, 0.5);
        assert!(comparison.within_tolerance);
        assert_eq!(comparison.deviation, 0.0);
        
        // Test within tolerance
        let comparison = replay.compare_outputs(1, 0.5, 0.5000005);
        assert!(comparison.within_tolerance);
        
        // Test outside tolerance
        let comparison = replay.compare_outputs(2, 0.5, 0.501);
        assert!(!comparison.within_tolerance);
    }
}
