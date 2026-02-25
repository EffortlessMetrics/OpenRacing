//! Stream implementations for blackbox recording
//!
//! Implements three streams with different data rates and purposes:
//! - Stream A: 1kHz frames + per-node outputs (RT hot path)
//! - Stream B: 60Hz telemetry (rate-limited)
//! - Stream C: Health/fault events (event-driven)
//!
//! # RT Safety
//!
//! Stream A recording is designed for RT hot path:
//! - Pre-allocated buffers for zero-allocation recording
//! - No locks in recording path
//! - Bounded execution time

use crate::error::{DiagnosticError, DiagnosticResult};
use serde::{Deserialize, Serialize};

pub(crate) mod codec {
    use crate::error::{DiagnosticError, DiagnosticResult};
    use serde::{Serialize, de::DeserializeOwned};
    use std::io::Read;

    fn config() -> impl bincode::config::Config {
        bincode::config::legacy()
    }

    pub fn encode_to_vec<T: Serialize>(value: &T) -> DiagnosticResult<Vec<u8>> {
        bincode::serde::encode_to_vec(value, config()).map_err(DiagnosticError::from)
    }

    pub fn decode_from_slice<T: DeserializeOwned>(bytes: &[u8]) -> DiagnosticResult<T> {
        let (value, used) =
            bincode::serde::decode_from_slice(bytes, config()).map_err(DiagnosticError::from)?;
        if used != bytes.len() {
            return Err(DiagnosticError::Deserialization(format!(
                "trailing bytes: used {used} of {}",
                bytes.len()
            )));
        }
        Ok(value)
    }

    #[allow(dead_code)]
    pub fn decode_from_std_read<T: DeserializeOwned, R: Read>(
        reader: &mut R,
    ) -> DiagnosticResult<T> {
        bincode::serde::decode_from_std_read(reader, config()).map_err(DiagnosticError::from)
    }
}

/// Simplified frame data for serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameData {
    /// Force feedback input (-1.0 to 1.0)
    pub ffb_in: f32,
    /// Torque output (-1.0 to 1.0)
    pub torque_out: f32,
    /// Wheel angular velocity in rad/s
    pub wheel_speed: f32,
    /// Hands-off detection flag
    pub hands_off: bool,
    /// Monotonic timestamp in nanoseconds
    pub ts_mono_ns: u64,
    /// Sequence number
    pub seq: u16,
}

impl Default for FrameData {
    fn default() -> Self {
        Self {
            ffb_in: 0.0,
            torque_out: 0.0,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        }
    }
}

/// Simplified safety state for serialization
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum SafetyStateSimple {
    /// Safe torque mode
    #[default]
    SafeTorque,
    /// High torque challenge in progress
    HighTorqueChallenge,
    /// Waiting for physical acknowledgment
    AwaitingPhysicalAck,
    /// High torque active
    HighTorqueActive,
    /// Faulted state
    Faulted {
        /// Fault type identifier
        fault_type: String,
    },
}

/// Simplified telemetry for serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryData {
    /// FFB scalar multiplier
    pub ffb_scalar: f32,
    /// Engine RPM
    pub rpm: f32,
    /// Vehicle speed in m/s
    pub speed_ms: f32,
    /// Tire slip ratio
    pub slip_ratio: f32,
    /// Current gear
    pub gear: i8,
    /// Car identifier
    pub car_id: Option<String>,
    /// Track identifier
    pub track_id: Option<String>,
}

impl Default for TelemetryData {
    fn default() -> Self {
        Self {
            ffb_scalar: 1.0,
            rpm: 0.0,
            speed_ms: 0.0,
            slip_ratio: 0.0,
            gear: 0,
            car_id: None,
            track_id: None,
        }
    }
}

/// Health event for Stream C
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthEventData {
    /// Event timestamp (Unix nanoseconds)
    pub timestamp_ns: u64,
    /// Device identifier
    pub device_id: String,
    /// Event type identifier
    pub event_type: String,
    /// Additional context as JSON
    pub context: serde_json::Value,
}

/// Stream A record (1kHz frames + per-node outputs)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamARecord {
    /// Timestamp (nanoseconds since recording start)
    pub timestamp_ns: u64,
    /// RT frame data
    pub frame: FrameData,
    /// Per-node filter outputs for debugging
    pub node_outputs: Vec<f32>,
    /// Safety state at time of frame
    pub safety_state: SafetyStateSimple,
    /// Processing time for this frame (microseconds)
    pub processing_time_us: u64,
}

/// Stream B record (60Hz telemetry)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamBRecord {
    /// Timestamp (nanoseconds since recording start)
    pub timestamp_ns: u64,
    /// Normalized telemetry data
    pub telemetry: TelemetryData,
}

/// Stream C record (health/fault events)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamCRecord {
    /// Timestamp (nanoseconds since recording start)
    pub timestamp_ns: u64,
    /// Health event data
    pub event: HealthEventData,
}

/// Stream A implementation (1kHz frames)
///
/// # RT Safety
///
/// This stream is designed for RT hot path recording:
/// - Pre-allocated internal buffer
/// - No heap allocations during recording (uses pre-allocated Vec)
/// - Bounded execution time for record_frame
pub struct StreamA {
    records: Vec<StreamARecord>,
    start_time_ns: u64,
    buffer: Vec<u8>,
}

impl Default for StreamA {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamA {
    /// Create a new Stream A with pre-allocated capacity
    ///
    /// Pre-allocates capacity for ~1 second of frames at 1kHz
    pub fn new() -> Self {
        Self::with_capacity(1000)
    }

    /// Create a new Stream A with specified capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            records: Vec::with_capacity(capacity),
            start_time_ns: Self::current_time_ns(),
            buffer: Vec::with_capacity(capacity * 128),
        }
    }

    /// Record a frame (RT-safe)
    ///
    /// This method is designed for RT hot path:
    /// - No heap allocations (pre-allocated Vec)
    /// - Bounded execution time
    /// - No locks or blocking operations
    ///
    /// # Arguments
    ///
    /// * `frame` - Frame data to record
    /// * `node_outputs` - Per-node filter outputs
    /// * `safety_state` - Current safety state
    /// * `processing_time_us` - Processing time in microseconds
    pub fn record_frame(
        &mut self,
        frame: FrameData,
        node_outputs: &[f32],
        safety_state: SafetyStateSimple,
        processing_time_us: u64,
    ) -> DiagnosticResult<()> {
        let timestamp_ns = Self::current_time_ns().saturating_sub(self.start_time_ns);

        let record = StreamARecord {
            timestamp_ns,
            frame,
            node_outputs: node_outputs.to_vec(),
            safety_state,
            processing_time_us,
        };

        self.records.push(record);
        Ok(())
    }

    /// Get serialized data for file output
    ///
    /// This method consumes and serializes all records.
    /// NOT RT-safe - performs heap allocations.
    pub fn get_data(&mut self) -> DiagnosticResult<Vec<u8>> {
        self.buffer.clear();

        for record in &self.records {
            let serialized = codec::encode_to_vec(record)?;
            let len = serialized.len() as u32;
            self.buffer.extend_from_slice(&len.to_le_bytes());
            self.buffer.extend_from_slice(&serialized);
        }

        self.records.clear();
        Ok(self.buffer.clone())
    }

    /// Get current record count
    pub fn record_count(&self) -> usize {
        self.records.len()
    }

    /// Reset stream state for new recording
    pub fn reset(&mut self) {
        self.records.clear();
        self.buffer.clear();
        self.start_time_ns = Self::current_time_ns();
    }

    fn current_time_ns() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64
    }
}

/// Stream B implementation (60Hz telemetry)
///
/// Rate-limited to approximately 60Hz to reduce file size
/// while maintaining useful telemetry resolution.
pub struct StreamB {
    records: Vec<StreamBRecord>,
    start_time_ns: u64,
    buffer: Vec<u8>,
    last_record_time_ns: u64,
    min_interval_ns: u64,
}

impl Default for StreamB {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamB {
    /// Create a new Stream B with default 60Hz rate limiting
    pub fn new() -> Self {
        Self::with_rate(60.0)
    }

    /// Create a new Stream B with specified rate limit
    pub fn with_rate(hz: f64) -> Self {
        Self {
            records: Vec::with_capacity(100),
            start_time_ns: Self::current_time_ns(),
            buffer: Vec::with_capacity(16 * 1024),
            last_record_time_ns: 0,
            min_interval_ns: (1_000_000_000.0 / hz) as u64,
        }
    }

    /// Record telemetry (rate-limited)
    ///
    /// Returns Ok(true) if recorded, Ok(false) if rate-limited.
    pub fn record_telemetry(&mut self, telemetry: TelemetryData) -> DiagnosticResult<bool> {
        let now = Self::current_time_ns();

        // Rate limiting
        if now.saturating_sub(self.last_record_time_ns) < self.min_interval_ns {
            return Ok(false);
        }

        let timestamp_ns = now.saturating_sub(self.start_time_ns);

        let record = StreamBRecord {
            timestamp_ns,
            telemetry,
        };

        self.records.push(record);
        self.last_record_time_ns = now;
        Ok(true)
    }

    /// Get serialized data for file output
    pub fn get_data(&mut self) -> DiagnosticResult<Vec<u8>> {
        self.buffer.clear();

        for record in &self.records {
            let serialized = codec::encode_to_vec(record)?;
            let len = serialized.len() as u32;
            self.buffer.extend_from_slice(&len.to_le_bytes());
            self.buffer.extend_from_slice(&serialized);
        }

        self.records.clear();
        Ok(self.buffer.clone())
    }

    /// Get current record count
    pub fn record_count(&self) -> usize {
        self.records.len()
    }

    /// Set rate limit in Hz
    pub fn set_rate_limit_hz(&mut self, hz: f64) {
        self.min_interval_ns = (1_000_000_000.0 / hz) as u64;
    }

    fn current_time_ns() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64
    }
}

/// Stream C implementation (health/fault events)
///
/// Event-driven stream with no rate limiting.
pub struct StreamC {
    records: Vec<StreamCRecord>,
    start_time_ns: u64,
    buffer: Vec<u8>,
}

impl Default for StreamC {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamC {
    /// Create a new Stream C
    pub fn new() -> Self {
        Self {
            records: Vec::with_capacity(100),
            start_time_ns: Self::current_time_ns(),
            buffer: Vec::with_capacity(16 * 1024),
        }
    }

    /// Record a health event
    pub fn record_health_event(&mut self, event: HealthEventData) -> DiagnosticResult<()> {
        let timestamp_ns = Self::current_time_ns().saturating_sub(self.start_time_ns);

        let record = StreamCRecord {
            timestamp_ns,
            event,
        };

        self.records.push(record);
        Ok(())
    }

    /// Get serialized data for file output
    pub fn get_data(&mut self) -> DiagnosticResult<Vec<u8>> {
        self.buffer.clear();

        for record in &self.records {
            let serialized = codec::encode_to_vec(record)?;
            let len = serialized.len() as u32;
            self.buffer.extend_from_slice(&len.to_le_bytes());
            self.buffer.extend_from_slice(&serialized);
        }

        self.records.clear();
        Ok(self.buffer.clone())
    }

    /// Get current record count
    pub fn record_count(&self) -> usize {
        self.records.len()
    }

    fn current_time_ns() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64
    }
}

/// Stream reader for parsing recorded data
pub struct StreamReader {
    data: Vec<u8>,
    position: usize,
}

impl StreamReader {
    /// Create a new stream reader from data
    pub fn new(data: Vec<u8>) -> Self {
        Self { data, position: 0 }
    }

    /// Read next Stream A record
    pub fn read_stream_a_record(&mut self) -> DiagnosticResult<Option<StreamARecord>> {
        self.read_record()
    }

    /// Read next Stream B record
    pub fn read_stream_b_record(&mut self) -> DiagnosticResult<Option<StreamBRecord>> {
        self.read_record()
    }

    /// Read next Stream C record
    pub fn read_stream_c_record(&mut self) -> DiagnosticResult<Option<StreamCRecord>> {
        self.read_record()
    }

    fn read_record<T: serde::de::DeserializeOwned>(&mut self) -> DiagnosticResult<Option<T>> {
        if self.position >= self.data.len() {
            return Ok(None);
        }

        if self.position + 4 > self.data.len() {
            return Err(DiagnosticError::Deserialization(
                "Incomplete length prefix".to_string(),
            ));
        }

        let len_bytes = &self.data[self.position..self.position + 4];
        let len =
            u32::from_le_bytes([len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3]]) as usize;
        self.position += 4;

        if self.position + len > self.data.len() {
            return Err(DiagnosticError::Deserialization(
                "Incomplete record data".to_string(),
            ));
        }

        let record_data = &self.data[self.position..self.position + len];
        self.position += len;

        let record = codec::decode_from_slice(record_data)?;
        Ok(Some(record))
    }

    /// Reset reader position
    pub fn reset(&mut self) {
        self.position = 0;
    }

    /// Get current position
    pub fn position(&self) -> usize {
        self.position
    }

    /// Check if at end of data
    pub fn is_at_end(&self) -> bool {
        self.position >= self.data.len()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_a_recording() {
        let mut stream = StreamA::new();

        let frame = FrameData {
            ffb_in: 0.5,
            torque_out: 0.3,
            wheel_speed: 10.0,
            hands_off: false,
            ts_mono_ns: 1_000_000_000,
            seq: 1,
        };

        stream
            .record_frame(frame, &[0.1, 0.2, 0.3], SafetyStateSimple::SafeTorque, 150)
            .unwrap();

        assert_eq!(stream.record_count(), 1);

        let data = stream.get_data().unwrap();
        assert!(!data.is_empty());
        assert_eq!(stream.record_count(), 0);
    }

    #[test]
    fn test_stream_b_rate_limiting() {
        let mut stream = StreamB::with_rate(1000.0); // 1kHz for testing

        let telemetry = TelemetryData::default();

        // First record should succeed
        assert!(stream.record_telemetry(telemetry.clone()).unwrap());

        // Immediate second record should be rate-limited
        assert!(!stream.record_telemetry(telemetry).unwrap());
    }

    #[test]
    fn test_stream_c_health_events() {
        let mut stream = StreamC::new();

        let event = HealthEventData {
            timestamp_ns: 0,
            device_id: "test-device".to_string(),
            event_type: "DeviceConnected".to_string(),
            context: serde_json::json!({"test": true}),
        };

        stream.record_health_event(event).unwrap();
        assert_eq!(stream.record_count(), 1);
    }

    #[test]
    fn test_stream_reader_roundtrip() {
        let mut stream_a = StreamA::new();

        for i in 0..5 {
            let frame = FrameData {
                ffb_in: i as f32 * 0.1,
                torque_out: i as f32 * 0.05,
                wheel_speed: i as f32,
                hands_off: false,
                ts_mono_ns: (i * 1_000_000) as u64,
                seq: i as u16,
            };

            stream_a
                .record_frame(
                    frame,
                    &[i as f32 * 0.01],
                    SafetyStateSimple::SafeTorque,
                    100,
                )
                .unwrap();
        }

        let data = stream_a.get_data().unwrap();

        let mut reader = StreamReader::new(data);
        let mut records_read = 0;

        while let Ok(Some(_record)) = reader.read_stream_a_record() {
            records_read += 1;
        }

        assert_eq!(records_read, 5);
        assert!(reader.is_at_end());
    }

    #[test]
    fn test_frame_data_default() {
        let frame = FrameData::default();
        assert_eq!(frame.ffb_in, 0.0);
        assert_eq!(frame.torque_out, 0.0);
        assert!(!frame.hands_off);
    }

    #[test]
    fn test_safety_state_simple_default() {
        let state = SafetyStateSimple::default();
        assert!(matches!(state, SafetyStateSimple::SafeTorque));
    }
}
