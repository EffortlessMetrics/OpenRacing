//! Mock implementations for testing.
//!
//! This module provides mock implementations of common traits and types
//! used throughout the OpenRacing codebase.

use std::cell::RefCell;
use std::fmt::Debug;

pub trait MockDeviceWriter: Send {
    fn write_feature_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>>;
    fn write_output_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>>;
}

pub struct MockHidDevice {
    pub feature_reports: RefCell<Vec<Vec<u8>>>,
    pub output_reports: RefCell<Vec<Vec<u8>>>,
    pub fail_on_write: bool,
    pub write_delay_ms: u64,
}

impl MockHidDevice {
    pub fn new() -> Self {
        Self {
            feature_reports: RefCell::new(Vec::new()),
            output_reports: RefCell::new(Vec::new()),
            fail_on_write: false,
            write_delay_ms: 0,
        }
    }

    pub fn with_failure() -> Self {
        Self {
            fail_on_write: true,
            ..Self::new()
        }
    }

    pub fn with_delay(delay_ms: u64) -> Self {
        Self {
            write_delay_ms: delay_ms,
            ..Self::new()
        }
    }

    pub fn feature_reports(&self) -> Vec<Vec<u8>> {
        self.feature_reports.borrow().clone()
    }

    pub fn output_reports(&self) -> Vec<Vec<u8>> {
        self.output_reports.borrow().clone()
    }

    pub fn last_feature_report(&self) -> Option<Vec<u8>> {
        self.feature_reports.borrow().last().cloned()
    }

    pub fn last_output_report(&self) -> Option<Vec<u8>> {
        self.output_reports.borrow().last().cloned()
    }

    pub fn clear(&self) {
        self.feature_reports.borrow_mut().clear();
        self.output_reports.borrow_mut().clear();
    }

    pub fn total_writes(&self) -> usize {
        self.feature_reports.borrow().len() + self.output_reports.borrow().len()
    }
}

impl Default for MockHidDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl MockDeviceWriter for MockHidDevice {
    fn write_feature_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        if self.fail_on_write {
            return Err("Mock write failure".into());
        }
        if self.write_delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(self.write_delay_ms));
        }
        let len = data.len();
        self.feature_reports.borrow_mut().push(data.to_vec());
        Ok(len)
    }

    fn write_output_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        if self.fail_on_write {
            return Err("Mock write failure".into());
        }
        if self.write_delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(self.write_delay_ms));
        }
        let len = data.len();
        self.output_reports.borrow_mut().push(data.to_vec());
        Ok(len)
    }
}

#[derive(Debug, Clone, Default)]
pub struct MockTelemetryData {
    pub rpm: f32,
    pub speed_ms: f32,
    pub ffb_scalar: f32,
    pub slip_ratio: f32,
    pub gear: i8,
    pub timestamp_ms: u64,
}

impl MockTelemetryData {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_rpm(mut self, rpm: f32) -> Self {
        self.rpm = rpm;
        self
    }

    pub fn with_speed(mut self, speed_ms: f32) -> Self {
        self.speed_ms = speed_ms;
        self
    }

    pub fn with_ffb(mut self, ffb_scalar: f32) -> Self {
        self.ffb_scalar = ffb_scalar;
        self
    }

    pub fn with_gear(mut self, gear: i8) -> Self {
        self.gear = gear;
        self
    }

    pub fn with_timestamp(mut self, timestamp_ms: u64) -> Self {
        self.timestamp_ms = timestamp_ms;
        self
    }

    pub fn racing_sample(progress: f32) -> Self {
        use std::f32::consts::PI;
        Self {
            rpm: 4000.0 + (progress * 2.0 * PI).sin() * 2000.0,
            speed_ms: 30.0 + progress * 40.0,
            ffb_scalar: (progress * 4.0 * PI).sin() * 0.7,
            slip_ratio: ((progress * 8.0 * PI).sin().abs() * 0.2).min(1.0),
            gear: ((progress * 5.0) as i8 % 6) + 1,
            timestamp_ms: (progress * 10000.0) as u64,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MockProfileId(pub String);

impl MockProfileId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl Default for MockProfileId {
    fn default() -> Self {
        Self::new("default")
    }
}

#[derive(Debug, Clone)]
pub struct MockProfile {
    pub id: MockProfileId,
    pub name: String,
    pub game: String,
    pub car: Option<String>,
    pub ffb_gain: f32,
    pub dor_deg: u16,
    pub torque_cap_nm: f32,
}

impl MockProfile {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: MockProfileId::new(id),
            name: "Default Profile".to_string(),
            game: "default".to_string(),
            car: None,
            ffb_gain: 1.0,
            dor_deg: 900,
            torque_cap_nm: 10.0,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn with_game(mut self, game: impl Into<String>) -> Self {
        self.game = game.into();
        self
    }

    pub fn with_car(mut self, car: impl Into<String>) -> Self {
        self.car = Some(car.into());
        self
    }

    pub fn with_ffb_gain(mut self, gain: f32) -> Self {
        self.ffb_gain = gain;
        self
    }

    pub fn with_dor(mut self, dor_deg: u16) -> Self {
        self.dor_deg = dor_deg;
        self
    }

    pub fn with_torque_cap(mut self, cap: f32) -> Self {
        self.torque_cap_nm = cap;
        self
    }
}

#[derive(Debug, Clone)]
pub struct MockTelemetryPort {
    pub data: Vec<MockTelemetryData>,
    pub current_index: RefCell<usize>,
}

impl MockTelemetryPort {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            current_index: RefCell::new(0),
        }
    }

    pub fn with_data(data: Vec<MockTelemetryData>) -> Self {
        Self {
            data,
            current_index: RefCell::new(0),
        }
    }

    pub fn add(&mut self, data: MockTelemetryData) {
        self.data.push(data);
    }

    pub fn next(&self) -> Option<MockTelemetryData> {
        let mut index = self.current_index.borrow_mut();
        if *index < self.data.len() {
            let data = self.data[*index].clone();
            *index += 1;
            Some(data)
        } else {
            None
        }
    }

    pub fn reset(&self) {
        *self.current_index.borrow_mut() = 0;
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn generate_racing_sequence(duration_s: f32, rate_hz: u32) -> Self {
        let samples = (duration_s * rate_hz as f32) as usize;
        let data: Vec<MockTelemetryData> = (0..samples)
            .map(|i| {
                let progress = i as f32 / samples as f32;
                MockTelemetryData::racing_sample(progress)
            })
            .collect();
        Self::with_data(data)
    }
}

impl Default for MockTelemetryPort {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_hid_device() -> Result<(), Box<dyn std::error::Error>> {
        let mut device = MockHidDevice::new();
        device.write_feature_report(&[1, 2, 3])?;
        device.write_output_report(&[4, 5, 6])?;

        assert_eq!(device.feature_reports().len(), 1);
        assert_eq!(device.output_reports().len(), 1);
        assert_eq!(device.total_writes(), 2);

        Ok(())
    }

    #[test]
    fn test_mock_hid_device_failure() {
        let mut device = MockHidDevice::with_failure();
        let result = device.write_feature_report(&[1, 2, 3]);
        assert!(result.is_err());
    }

    #[test]
    fn test_mock_hid_device_clear() -> Result<(), Box<dyn std::error::Error>> {
        let mut device = MockHidDevice::new();
        device.write_feature_report(&[1, 2, 3])?;
        assert!(!device.feature_reports().is_empty());

        device.clear();
        assert!(device.feature_reports().is_empty());

        Ok(())
    }

    #[test]
    fn test_mock_telemetry_data() {
        let data = MockTelemetryData::new()
            .with_rpm(5000.0)
            .with_speed(100.0)
            .with_ffb(0.5)
            .with_gear(3);

        assert_eq!(data.rpm, 5000.0);
        assert_eq!(data.speed_ms, 100.0);
        assert_eq!(data.ffb_scalar, 0.5);
        assert_eq!(data.gear, 3);
    }

    #[test]
    fn test_mock_telemetry_data_racing_sample() {
        let sample1 = MockTelemetryData::racing_sample(0.0);
        let sample2 = MockTelemetryData::racing_sample(0.5);

        assert_ne!(sample1.rpm, sample2.rpm);
        assert_ne!(sample1.speed_ms, sample2.speed_ms);
    }

    #[test]
    fn test_mock_profile() {
        let profile = MockProfile::new("test")
            .with_name("Test Profile")
            .with_game("iracing")
            .with_car("gt3")
            .with_ffb_gain(0.8)
            .with_dor(540)
            .with_torque_cap(12.0);

        assert_eq!(profile.id.0, "test");
        assert_eq!(profile.name, "Test Profile");
        assert_eq!(profile.game, "iracing");
        assert_eq!(profile.car, Some("gt3".to_string()));
        assert_eq!(profile.ffb_gain, 0.8);
        assert_eq!(profile.dor_deg, 540);
        assert_eq!(profile.torque_cap_nm, 12.0);
    }

    #[test]
    fn test_mock_telemetry_port() -> Result<(), Box<dyn std::error::Error>> {
        let mut port = MockTelemetryPort::new();
        port.add(MockTelemetryData::new().with_rpm(1000.0));
        port.add(MockTelemetryData::new().with_rpm(2000.0));
        port.add(MockTelemetryData::new().with_rpm(3000.0));

        assert_eq!(port.len(), 3);

        let first = port.next().ok_or("expected first telemetry data")?;
        assert_eq!(first.rpm, 1000.0);

        let second = port.next().ok_or("expected second telemetry data")?;
        assert_eq!(second.rpm, 2000.0);

        port.reset();
        let first_again = port.next().ok_or("expected telemetry data after reset")?;
        assert_eq!(first_again.rpm, 1000.0);

        Ok(())
    }

    #[test]
    fn test_mock_telemetry_port_generate() {
        let port = MockTelemetryPort::generate_racing_sequence(1.0, 60);
        assert_eq!(port.len(), 60);
    }

    #[test]
    fn test_mock_telemetry_port_empty() {
        let port = MockTelemetryPort::new();
        assert!(port.is_empty());
        assert!(port.next().is_none());
    }
}
