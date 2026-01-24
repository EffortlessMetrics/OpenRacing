//! Stream implementations for blackbox recording
//!
//! Implements three streams:
//! - Stream A: 1kHz frames + per-node outputs
//! - Stream B: 60Hz telemetry
//! - Stream C: Health/fault events

use super::HealthEvent;
#[cfg(test)]
use super::HealthEventType;
use super::bincode_compat as codec;
use crate::ports::NormalizedTelemetry;
use crate::rt::Frame;
use crate::safety::SafetyState;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Stream type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamType {
    A = 0, // 1kHz frames
    B = 1, // 60Hz telemetry
    C = 2, // Health events
}

/// Stream A record (1kHz frames + per-node outputs)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamARecord {
    /// Timestamp (nanoseconds since recording start)
    pub timestamp_ns: u64,
    /// RT frame data
    pub frame: Frame,
    /// Per-node filter outputs for debugging
    pub node_outputs: Vec<f32>,
    /// Safety state at time of frame (simplified for serialization)
    pub safety_state: SafetyStateSimple,
    /// Processing time for this frame (microseconds)
    pub processing_time_us: u64,
}

/// Simplified safety state for serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SafetyStateSimple {
    SafeTorque,
    HighTorqueChallenge,
    AwaitingPhysicalAck,
    HighTorqueActive,
    Faulted { fault_type: String },
}

/// Stream B record (60Hz telemetry)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamBRecord {
    /// Timestamp (nanoseconds since recording start)
    pub timestamp_ns: u64,
    /// Normalized telemetry data (simplified for serialization)
    pub telemetry: NormalizedTelemetrySimple,
}

/// Simplified telemetry for serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedTelemetrySimple {
    pub ffb_scalar: f32,
    pub rpm: f32,
    pub speed_ms: f32,
    pub slip_ratio: f32,
    pub gear: i8,
    pub car_id: Option<String>,
    pub track_id: Option<String>,
}

/// Stream C record (health/fault events)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamCRecord {
    /// Timestamp (nanoseconds since recording start)
    pub timestamp_ns: u64,
    /// Health event data
    pub event: HealthEvent,
}

/// Stream A implementation (1kHz frames)
pub struct StreamA {
    records: Vec<StreamARecord>,
    start_time: SystemTime,
    buffer: Vec<u8>,
}

impl SafetyStateSimple {
    fn from_safety_state(state: &SafetyState) -> Self {
        match state {
            SafetyState::SafeTorque => SafetyStateSimple::SafeTorque,
            SafetyState::HighTorqueChallenge { .. } => SafetyStateSimple::HighTorqueChallenge,
            SafetyState::AwaitingPhysicalAck { .. } => SafetyStateSimple::AwaitingPhysicalAck,
            SafetyState::HighTorqueActive { .. } => SafetyStateSimple::HighTorqueActive,
            SafetyState::Faulted { fault, .. } => SafetyStateSimple::Faulted {
                fault_type: format!("{:?}", fault),
            },
        }
    }
}

impl NormalizedTelemetrySimple {
    fn from_telemetry(telemetry: &NormalizedTelemetry) -> Self {
        Self {
            ffb_scalar: telemetry.ffb_scalar,
            rpm: telemetry.rpm,
            speed_ms: telemetry.speed_ms,
            slip_ratio: telemetry.slip_ratio,
            gear: telemetry.gear,
            car_id: telemetry.car_id.clone(),
            track_id: telemetry.track_id.clone(),
        }
    }
}

impl StreamA {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            start_time: SystemTime::now(),
            buffer: Vec::new(),
        }
    }

    pub fn record_frame(
        &mut self,
        frame: &Frame,
        node_outputs: &[f32],
        safety_state: &SafetyState,
        processing_time_us: u64,
    ) -> Result<(), String> {
        let timestamp_ns = SystemTime::now()
            .duration_since(self.start_time)
            .unwrap_or_default()
            .as_nanos() as u64;

        let record = StreamARecord {
            timestamp_ns,
            frame: *frame,
            node_outputs: node_outputs.to_vec(),
            safety_state: SafetyStateSimple::from_safety_state(safety_state),
            processing_time_us,
        };

        self.records.push(record);
        Ok(())
    }

    pub fn get_data(&mut self) -> Vec<u8> {
        self.buffer.clear();

        // Serialize all records
        for record in &self.records {
            if let Ok(serialized) = codec::encode_to_vec(record) {
                // Write record length prefix
                let len = serialized.len() as u32;
                self.buffer.extend_from_slice(&len.to_le_bytes());
                // Write record data
                self.buffer.extend_from_slice(&serialized);
            }
        }

        // Clear records after serialization to free memory
        self.records.clear();

        self.buffer.clone()
    }

    pub fn record_count(&self) -> usize {
        self.records.len()
    }
}

/// Stream B implementation (60Hz telemetry)
pub struct StreamB {
    records: Vec<StreamBRecord>,
    start_time: SystemTime,
    buffer: Vec<u8>,
    last_record_time: SystemTime,
    min_interval_ns: u64, // Minimum interval between records (for 60Hz = ~16.67ms)
}

impl StreamB {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            start_time: SystemTime::now(),
            buffer: Vec::new(),
            last_record_time: UNIX_EPOCH,
            min_interval_ns: 16_666_667, // ~60Hz
        }
    }

    pub fn record_telemetry(&mut self, telemetry: &NormalizedTelemetry) -> Result<(), String> {
        let now = SystemTime::now();

        // Rate limiting to ~60Hz
        if let Ok(duration) = now.duration_since(self.last_record_time) {
            if duration.as_nanos() < self.min_interval_ns as u128 {
                return Ok(()); // Skip this record to maintain rate limit
            }
        }

        let timestamp_ns = now
            .duration_since(self.start_time)
            .unwrap_or_default()
            .as_nanos() as u64;

        let record = StreamBRecord {
            timestamp_ns,
            telemetry: NormalizedTelemetrySimple::from_telemetry(telemetry),
        };

        self.records.push(record);
        self.last_record_time = now;
        Ok(())
    }

    pub fn get_data(&mut self) -> Vec<u8> {
        self.buffer.clear();

        // Serialize all records
        for record in &self.records {
            if let Ok(serialized) = codec::encode_to_vec(record) {
                // Write record length prefix
                let len = serialized.len() as u32;
                self.buffer.extend_from_slice(&len.to_le_bytes());
                // Write record data
                self.buffer.extend_from_slice(&serialized);
            }
        }

        // Clear records after serialization
        self.records.clear();

        self.buffer.clone()
    }

    pub fn record_count(&self) -> usize {
        self.records.len()
    }

    pub fn set_rate_limit_hz(&mut self, hz: f64) {
        self.min_interval_ns = (1_000_000_000.0 / hz) as u64;
    }
}

/// Stream C implementation (health/fault events)
pub struct StreamC {
    records: Vec<StreamCRecord>,
    start_time: SystemTime,
    buffer: Vec<u8>,
}

impl StreamC {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            start_time: SystemTime::now(),
            buffer: Vec::new(),
        }
    }

    pub fn record_health_event(&mut self, event: &HealthEvent) -> Result<(), String> {
        let timestamp_ns = SystemTime::now()
            .duration_since(self.start_time)
            .unwrap_or_default()
            .as_nanos() as u64;

        let record = StreamCRecord {
            timestamp_ns,
            event: event.clone(),
        };

        self.records.push(record);
        Ok(())
    }

    pub fn get_data(&mut self) -> Vec<u8> {
        self.buffer.clear();

        // Serialize all records
        for record in &self.records {
            if let Ok(serialized) = codec::encode_to_vec(record) {
                // Write record length prefix
                let len = serialized.len() as u32;
                self.buffer.extend_from_slice(&len.to_le_bytes());
                // Write record data
                self.buffer.extend_from_slice(&serialized);
            }
        }

        // Clear records after serialization
        self.records.clear();

        self.buffer.clone()
    }

    pub fn record_count(&self) -> usize {
        self.records.len()
    }
}

/// Stream reader for parsing recorded data
pub struct StreamReader {
    data: Vec<u8>,
    position: usize,
}

impl StreamReader {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data, position: 0 }
    }

    /// Read next Stream A record
    pub fn read_stream_a_record(&mut self) -> Result<Option<StreamARecord>, String> {
        if self.position >= self.data.len() {
            return Ok(None);
        }

        // Read length prefix
        if self.position + 4 > self.data.len() {
            return Err("Incomplete length prefix".to_string());
        }

        let len_bytes = &self.data[self.position..self.position + 4];
        let len =
            u32::from_le_bytes([len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3]]) as usize;
        self.position += 4;

        // Read record data
        if self.position + len > self.data.len() {
            return Err("Incomplete record data".to_string());
        }

        let record_data = &self.data[self.position..self.position + len];
        self.position += len;

        // Deserialize record
        let record: StreamARecord = codec::decode_from_slice(record_data)
            .map_err(|e| format!("Failed to deserialize Stream A record: {}", e))?;

        Ok(Some(record))
    }

    /// Read next Stream B record
    pub fn read_stream_b_record(&mut self) -> Result<Option<StreamBRecord>, String> {
        if self.position >= self.data.len() {
            return Ok(None);
        }

        // Read length prefix
        if self.position + 4 > self.data.len() {
            return Err("Incomplete length prefix".to_string());
        }

        let len_bytes = &self.data[self.position..self.position + 4];
        let len =
            u32::from_le_bytes([len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3]]) as usize;
        self.position += 4;

        // Read record data
        if self.position + len > self.data.len() {
            return Err("Incomplete record data".to_string());
        }

        let record_data = &self.data[self.position..self.position + len];
        self.position += len;

        // Deserialize record
        let record: StreamBRecord = codec::decode_from_slice(record_data)
            .map_err(|e| format!("Failed to deserialize Stream B record: {}", e))?;

        Ok(Some(record))
    }

    /// Read next Stream C record
    pub fn read_stream_c_record(&mut self) -> Result<Option<StreamCRecord>, String> {
        if self.position >= self.data.len() {
            return Ok(None);
        }

        // Read length prefix
        if self.position + 4 > self.data.len() {
            return Err("Incomplete length prefix".to_string());
        }

        let len_bytes = &self.data[self.position..self.position + 4];
        let len =
            u32::from_le_bytes([len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3]]) as usize;
        self.position += 4;

        // Read record data
        if self.position + len > self.data.len() {
            return Err("Incomplete record data".to_string());
        }

        let record_data = &self.data[self.position..self.position + len];
        self.position += len;

        // Deserialize record
        let record: StreamCRecord = codec::decode_from_slice(record_data)
            .map_err(|e| format!("Failed to deserialize Stream C record: {}", e))?;

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
mod tests {
    use super::*;
    use crate::safety::SafetyState;
    use racing_wheel_schemas::prelude::DeviceId;

    #[test]
    fn test_stream_a_recording() {
        let mut stream = StreamA::new();

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

        let result = stream.record_frame(&frame, &node_outputs, &safety_state, processing_time_us);
        assert!(result.is_ok());
        assert_eq!(stream.record_count(), 1);

        let data = stream.get_data();
        assert!(!data.is_empty());
        assert_eq!(stream.record_count(), 0); // Should be cleared after get_data
    }

    #[test]
    fn test_stream_b_rate_limiting() {
        let mut stream = StreamB::new();
        stream.set_rate_limit_hz(10.0); // 10Hz for faster testing

        let telemetry = NormalizedTelemetry {
            ffb_scalar: 0.5,
            rpm: 3000.0,
            speed_ms: 25.0,
            slip_ratio: 0.1,
            gear: 3,
            flags: Default::default(),
            car_id: Some("test_car".to_string()),
            track_id: Some("test_track".to_string()),
            timestamp: std::time::Instant::now(),
        };

        // Record multiple times rapidly
        for _ in 0..10 {
            let _ = stream.record_telemetry(&telemetry);
        }

        // Should have rate-limited to fewer records
        assert!(stream.record_count() <= 2); // Allow some tolerance
    }

    #[test]
    fn test_stream_c_health_events() {
        let mut stream = StreamC::new();

        let device_id = DeviceId::new("test-device".to_string()).unwrap();
        let event = HealthEvent {
            timestamp: SystemTime::now(),
            device_id,
            event_type: HealthEventType::DeviceConnected,
            context: serde_json::json!({"test": true}),
        };

        let result = stream.record_health_event(&event);
        assert!(result.is_ok());
        assert_eq!(stream.record_count(), 1);

        let data = stream.get_data();
        assert!(!data.is_empty());
    }

    #[test]
    fn test_stream_reader_roundtrip() {
        let mut stream_a = StreamA::new();

        // Record some data
        for i in 0..5 {
            let frame = Frame {
                ffb_in: i as f32 * 0.1,
                torque_out: i as f32 * 0.05,
                wheel_speed: i as f32,
                hands_off: false,
                ts_mono_ns: (i * 1000000) as u64,
                seq: i as u16,
            };

            let node_outputs = vec![i as f32 * 0.01; 3];
            stream_a
                .record_frame(
                    &frame,
                    &node_outputs,
                    &SafetyState::SafeTorque,
                    100 + i as u64,
                )
                .unwrap();
        }

        // Get serialized data
        let data = stream_a.get_data();
        assert!(!data.is_empty());

        // Read back with StreamReader
        let mut reader = StreamReader::new(data);
        let mut records_read = 0;

        while let Ok(Some(record)) = reader.read_stream_a_record() {
            assert_eq!(record.node_outputs.len(), 3);
            assert!(matches!(record.safety_state, SafetyStateSimple::SafeTorque));
            records_read += 1;
        }

        assert_eq!(records_read, 5);
        assert!(reader.is_at_end());
    }

    #[test]
    fn test_stream_serialization_format() {
        let mut stream = StreamA::new();

        let frame = Frame {
            ffb_in: 0.5,
            torque_out: 0.3,
            wheel_speed: 10.0,
            hands_off: false,
            ts_mono_ns: 1000000000,
            seq: 1,
        };

        stream
            .record_frame(&frame, &[0.1, 0.2], &SafetyState::SafeTorque, 150)
            .unwrap();

        let data = stream.get_data();

        // Check that data starts with length prefix
        assert!(data.len() >= 4);
        let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        assert_eq!(data.len(), 4 + len); // Length prefix + data
    }

    #[test]
    fn test_empty_stream_data() {
        let mut stream_a = StreamA::new();
        let mut stream_b = StreamB::new();
        let mut stream_c = StreamC::new();

        // Get data from empty streams
        let data_a = stream_a.get_data();
        let data_b = stream_b.get_data();
        let data_c = stream_c.get_data();

        assert!(data_a.is_empty());
        assert!(data_b.is_empty());
        assert!(data_c.is_empty());
    }
}
