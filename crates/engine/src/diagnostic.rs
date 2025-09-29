//! Diagnostic and Blackbox Recording System
//!
//! This module implements the blackbox recording system with .wbb v1 format,
//! replay capabilities, and support bundle generation as specified in DIAG-01, DIAG-02, DIAG-03.

pub mod blackbox;
pub mod replay;
pub mod support_bundle;
pub mod streams;



pub use blackbox::{BlackboxRecorder, BlackboxConfig, RecordingStats};
pub use replay::{BlackboxReplay, ReplayConfig, ReplayResult};
pub use support_bundle::{SupportBundle, SupportBundleConfig};
pub use streams::{StreamA, StreamB, StreamC, StreamType};

use crate::rt::Frame;
use crate::safety::{SafetyState, FaultType};
use crate::ports::NormalizedTelemetry;
use racing_wheel_schemas::DeviceId;
use std::time::{Instant, SystemTime};
use serde::{Deserialize, Serialize};

/// Health event for stream C
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthEvent {
    /// Timestamp when event occurred
    pub timestamp: SystemTime,
    /// Device ID that generated the event
    pub device_id: DeviceId,
    /// Event type
    pub event_type: HealthEventType,
    /// Additional context data
    pub context: serde_json::Value,
}

/// Types of health events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthEventType {
    /// Device connected
    DeviceConnected,
    /// Device disconnected
    DeviceDisconnected,
    /// Safety fault occurred
    SafetyFault { fault_type: FaultType },
    /// Performance degradation detected
    PerformanceDegradation { metric: String, value: f64 },
    /// Configuration change applied
    ConfigurationChange { change_type: String },
    /// System resource warning
    ResourceWarning { resource: String, usage: f64 },
    /// Plugin event
    PluginEvent { plugin_id: String, event: String },
}

/// Diagnostic service configuration
#[derive(Debug, Clone)]
pub struct DiagnosticConfig {
    /// Enable blackbox recording
    pub enable_recording: bool,
    /// Maximum recording duration (seconds)
    pub max_recording_duration_s: u64,
    /// Recording directory path
    pub recording_dir: std::path::PathBuf,
    /// Maximum file size per recording (bytes)
    pub max_file_size_bytes: u64,
    /// Compression level (0-9)
    pub compression_level: u8,
    /// Enable stream A (1kHz frames)
    pub enable_stream_a: bool,
    /// Enable stream B (60Hz telemetry)
    pub enable_stream_b: bool,
    /// Enable stream C (health events)
    pub enable_stream_c: bool,
}

impl Default for DiagnosticConfig {
    fn default() -> Self {
        Self {
            enable_recording: true,
            max_recording_duration_s: 300, // 5 minutes
            recording_dir: std::path::PathBuf::from("./diag/blackbox"),
            max_file_size_bytes: 25 * 1024 * 1024, // 25MB for 2-minute capture
            compression_level: 6,
            enable_stream_a: true,
            enable_stream_b: true,
            enable_stream_c: true,
        }
    }
}

/// Main diagnostic service
pub struct DiagnosticService {
    config: DiagnosticConfig,
    recorder: Option<BlackboxRecorder>,
    health_events: Vec<HealthEvent>,
    start_time: Instant,
}

impl DiagnosticService {
    /// Create new diagnostic service
    pub fn new(config: DiagnosticConfig) -> Result<Self, String> {
        // Ensure recording directory exists
        if config.enable_recording {
            std::fs::create_dir_all(&config.recording_dir)
                .map_err(|e| format!("Failed to create recording directory: {}", e))?;
        }

        Ok(Self {
            config,
            recorder: None,
            health_events: Vec::new(),
            start_time: Instant::now(),
        })
    }

    /// Start recording session
    pub fn start_recording(&mut self, device_id: DeviceId) -> Result<(), String> {
        if !self.config.enable_recording {
            return Err("Recording disabled in configuration".to_string());
        }

        if self.recorder.is_some() {
            return Err("Recording already in progress".to_string());
        }

        let blackbox_config = BlackboxConfig {
            device_id: device_id.clone(),
            output_dir: self.config.recording_dir.clone(),
            max_duration_s: self.config.max_recording_duration_s,
            max_file_size_bytes: self.config.max_file_size_bytes,
            compression_level: self.config.compression_level,
            enable_stream_a: self.config.enable_stream_a,
            enable_stream_b: self.config.enable_stream_b,
            enable_stream_c: self.config.enable_stream_c,
        };

        let recorder = BlackboxRecorder::new(blackbox_config)?;
        self.recorder = Some(recorder);

        // Record session start event
        self.record_health_event(HealthEvent {
            timestamp: SystemTime::now(),
            device_id,
            event_type: HealthEventType::ConfigurationChange {
                change_type: "recording_started".to_string(),
            },
            context: serde_json::json!({}),
        });

        Ok(())
    }

    /// Stop recording session
    pub fn stop_recording(&mut self) -> Result<Option<std::path::PathBuf>, String> {
        if let Some(recorder) = self.recorder.take() {
            let output_path = recorder.finalize()?;
            Ok(Some(output_path))
        } else {
            Ok(None)
        }
    }

    /// Record a frame (Stream A - 1kHz)
    pub fn record_frame(&mut self, frame: &Frame, node_outputs: &[f32], safety_state: &SafetyState, processing_time_us: u64) -> Result<(), String> {
        if let Some(ref mut recorder) = self.recorder {
            recorder.record_frame(frame, node_outputs, safety_state, processing_time_us)?;
        }
        Ok(())
    }

    /// Record telemetry (Stream B - 60Hz)
    pub fn record_telemetry(&mut self, telemetry: &NormalizedTelemetry) -> Result<(), String> {
        if let Some(ref mut recorder) = self.recorder {
            recorder.record_telemetry(telemetry)?;
        }
        Ok(())
    }

    /// Record health event (Stream C)
    pub fn record_health_event(&mut self, event: HealthEvent) {
        // Store in memory for support bundle
        self.health_events.push(event.clone());

        // Keep only recent events to prevent unbounded growth
        if self.health_events.len() > 1000 {
            self.health_events.drain(0..500); // Remove oldest half
        }

        // Record to blackbox if active
        if let Some(ref mut recorder) = self.recorder {
            let _ = recorder.record_health_event(&event);
        }
    }

    /// Generate support bundle
    pub fn generate_support_bundle(&self, output_path: &std::path::Path) -> Result<(), String> {
        let bundle_config = SupportBundleConfig {
            include_logs: true,
            include_profiles: true,
            include_system_info: true,
            include_recent_recordings: true,
            max_bundle_size_mb: 25,
        };

        let mut bundle = SupportBundle::new(bundle_config);

        // Add health events
        bundle.add_health_events(&self.health_events)?;

        // Add system information
        bundle.add_system_info()?;

        // Add recent blackbox recordings
        if self.config.enable_recording {
            bundle.add_recent_recordings(&self.config.recording_dir)?;
        }

        // Generate the bundle
        bundle.generate(output_path)?;

        Ok(())
    }

    /// Get recording statistics
    pub fn get_recording_stats(&self) -> Option<RecordingStats> {
        self.recorder.as_ref().map(|r| r.get_stats())
    }

    /// Check if recording is active
    pub fn is_recording(&self) -> bool {
        self.recorder.is_some()
    }

    /// Get uptime
    pub fn uptime(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }

    /// Get recent health events
    pub fn get_recent_health_events(&self, limit: usize) -> &[HealthEvent] {
        let start = if self.health_events.len() > limit {
            self.health_events.len() - limit
        } else {
            0
        };
        &self.health_events[start..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_config() -> (DiagnosticConfig, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = DiagnosticConfig {
            enable_recording: true,
            max_recording_duration_s: 10,
            recording_dir: temp_dir.path().to_path_buf(),
            max_file_size_bytes: 1024 * 1024, // 1MB
            compression_level: 1, // Fast compression for tests
            enable_stream_a: true,
            enable_stream_b: true,
            enable_stream_c: true,
        };
        (config, temp_dir)
    }

    #[test]
    fn test_diagnostic_service_creation() {
        let (config, _temp_dir) = create_test_config();
        let service = DiagnosticService::new(config);
        assert!(service.is_ok());

        let service = service.unwrap();
        assert!(!service.is_recording());
        assert!(service.get_recording_stats().is_none());
    }

    #[test]
    fn test_recording_lifecycle() {
        let (config, _temp_dir) = create_test_config();
        let mut service = DiagnosticService::new(config).unwrap();

        let device_id = DeviceId::new("test-device".to_string()).unwrap();

        // Start recording
        let result = service.start_recording(device_id.clone());
        assert!(result.is_ok());
        assert!(service.is_recording());

        // Try to start again (should fail)
        let result = service.start_recording(device_id);
        assert!(result.is_err());

        // Stop recording
        let result = service.stop_recording();
        assert!(result.is_ok());
        assert!(!service.is_recording());
    }

    #[test]
    fn test_health_event_recording() {
        let (config, _temp_dir) = create_test_config();
        let mut service = DiagnosticService::new(config).unwrap();

        let device_id = DeviceId::new("test-device".to_string()).unwrap();
        let event = HealthEvent {
            timestamp: SystemTime::now(),
            device_id,
            event_type: HealthEventType::DeviceConnected,
            context: serde_json::json!({"test": true}),
        };

        service.record_health_event(event);

        let recent_events = service.get_recent_health_events(10);
        assert_eq!(recent_events.len(), 1);
    }

    #[test]
    fn test_frame_recording() {
        let (config, _temp_dir) = create_test_config();
        let mut service = DiagnosticService::new(config).unwrap();

        let device_id = DeviceId::new("test-device".to_string()).unwrap();
        service.start_recording(device_id).unwrap();

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

        let result = service.record_frame(&frame, &node_outputs, &safety_state, processing_time_us);
        assert!(result.is_ok());

        let stats = service.get_recording_stats();
        assert!(stats.is_some());
    }

    #[test]
    fn test_support_bundle_generation() {
        let (config, temp_dir) = create_test_config();
        let mut service = DiagnosticService::new(config).unwrap();

        // Add some health events
        let device_id = DeviceId::new("test-device".to_string()).unwrap();
        for i in 0..5 {
            let event = HealthEvent {
                timestamp: SystemTime::now(),
                device_id: device_id.clone(),
                event_type: HealthEventType::PerformanceDegradation {
                    metric: "jitter".to_string(),
                    value: i as f64 * 0.1,
                },
                context: serde_json::json!({"iteration": i}),
            };
            service.record_health_event(event);
        }

        let bundle_path = temp_dir.path().join("support_bundle.zip");
        let result = service.generate_support_bundle(&bundle_path);
        assert!(result.is_ok());
        assert!(bundle_path.exists());
    }

    #[test]
    fn test_disabled_recording() {
        let (mut config, _temp_dir) = create_test_config();
        config.enable_recording = false;

        let mut service = DiagnosticService::new(config).unwrap();
        let device_id = DeviceId::new("test-device".to_string()).unwrap();

        let result = service.start_recording(device_id);
        assert!(result.is_err());
        assert!(!service.is_recording());
    }
}