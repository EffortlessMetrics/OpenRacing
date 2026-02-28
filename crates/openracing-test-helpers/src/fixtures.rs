//! Test fixtures and builders for common test scenarios.
//!
//! This module provides pre-built test fixtures for devices, profiles,
//! and telemetry data.

use std::time::Duration;

#[derive(Debug, Clone)]
pub struct DeviceCapabilitiesFixture {
    pub supports_pid: bool,
    pub supports_raw_torque_1khz: bool,
    pub supports_health_stream: bool,
    pub supports_led_bus: bool,
    pub max_torque_nm: f32,
    pub encoder_cpr: u32,
    pub min_report_period_us: u32,
}

impl Default for DeviceCapabilitiesFixture {
    fn default() -> Self {
        Self::basic_wheel()
    }
}

impl DeviceCapabilitiesFixture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn basic_wheel() -> Self {
        Self {
            supports_pid: true,
            supports_raw_torque_1khz: false,
            supports_health_stream: false,
            supports_led_bus: false,
            max_torque_nm: 8.0,
            encoder_cpr: 4096,
            min_report_period_us: 2000,
        }
    }

    pub fn dd_wheel() -> Self {
        Self {
            supports_pid: true,
            supports_raw_torque_1khz: true,
            supports_health_stream: true,
            supports_led_bus: true,
            max_torque_nm: 25.0,
            encoder_cpr: 65535,
            min_report_period_us: 1000,
        }
    }

    pub fn high_end_dd() -> Self {
        Self {
            supports_pid: true,
            supports_raw_torque_1khz: true,
            supports_health_stream: true,
            supports_led_bus: true,
            max_torque_nm: 50.0,
            encoder_cpr: 65535,
            min_report_period_us: 500,
        }
    }

    pub fn with_max_torque(mut self, torque_nm: f32) -> Self {
        self.max_torque_nm = torque_nm;
        self
    }

    pub fn with_encoder_cpr(mut self, cpr: u32) -> Self {
        self.encoder_cpr = cpr;
        self
    }

    pub fn with_raw_torque(mut self, enabled: bool) -> Self {
        self.supports_raw_torque_1khz = enabled;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadLevel {
    Idle,
    Light,
    Normal,
    Heavy,
    Extreme,
}

#[derive(Debug, Clone)]
pub struct PerformanceFixture {
    pub name: String,
    pub duration: Duration,
    pub expected_jitter_p99_ms: f64,
    pub expected_latency_p99_us: f64,
    pub load_level: LoadLevel,
}

impl Default for PerformanceFixture {
    fn default() -> Self {
        Self::normal_load()
    }
}

impl PerformanceFixture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn idle() -> Self {
        Self {
            name: "Idle".to_string(),
            duration: Duration::from_secs(60),
            expected_jitter_p99_ms: 0.1,
            expected_latency_p99_us: 150.0,
            load_level: LoadLevel::Idle,
        }
    }

    pub fn light_load() -> Self {
        Self {
            name: "Light Load".to_string(),
            duration: Duration::from_secs(120),
            expected_jitter_p99_ms: 0.15,
            expected_latency_p99_us: 200.0,
            load_level: LoadLevel::Light,
        }
    }

    pub fn normal_load() -> Self {
        Self {
            name: "Normal Load".to_string(),
            duration: Duration::from_secs(180),
            expected_jitter_p99_ms: 0.2,
            expected_latency_p99_us: 250.0,
            load_level: LoadLevel::Normal,
        }
    }

    pub fn heavy_load() -> Self {
        Self {
            name: "Heavy Load".to_string(),
            duration: Duration::from_secs(300),
            expected_jitter_p99_ms: 0.25,
            expected_latency_p99_us: 300.0,
            load_level: LoadLevel::Heavy,
        }
    }

    pub fn extreme_load() -> Self {
        Self {
            name: "Extreme Load".to_string(),
            duration: Duration::from_secs(600),
            expected_jitter_p99_ms: 0.3,
            expected_latency_p99_us: 400.0,
            load_level: LoadLevel::Extreme,
        }
    }

    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    pub fn with_load_level(mut self, level: LoadLevel) -> Self {
        self.load_level = level;
        self
    }
}

#[derive(Debug, Clone)]
pub struct ProfileFixture {
    pub name: String,
    pub game: String,
    pub car: Option<String>,
    pub ffb_gain: f32,
    pub dor_deg: u16,
    pub torque_cap_nm: f32,
    pub is_valid: bool,
    pub expected_errors: Vec<String>,
}

impl Default for ProfileFixture {
    fn default() -> Self {
        Self::valid()
    }
}

impl ProfileFixture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn valid() -> Self {
        Self {
            name: "Valid Profile".to_string(),
            game: "iracing".to_string(),
            car: Some("gt3".to_string()),
            ffb_gain: 0.68,
            dor_deg: 540,
            torque_cap_nm: 10.0,
            is_valid: true,
            expected_errors: vec![],
        }
    }

    pub fn invalid_gain() -> Self {
        Self {
            name: "Invalid Gain".to_string(),
            game: "iracing".to_string(),
            car: None,
            ffb_gain: 2.5,
            dor_deg: 900,
            torque_cap_nm: 10.0,
            is_valid: false,
            expected_errors: vec!["ffb_gain must be between 0.0 and 1.0".to_string()],
        }
    }

    pub fn invalid_dor() -> Self {
        Self {
            name: "Invalid DOR".to_string(),
            game: "iracing".to_string(),
            car: None,
            ffb_gain: 0.8,
            dor_deg: 0,
            torque_cap_nm: 10.0,
            is_valid: false,
            expected_errors: vec!["dor_deg must be positive".to_string()],
        }
    }

    pub fn invalid_torque_cap() -> Self {
        Self {
            name: "Invalid Torque Cap".to_string(),
            game: "iracing".to_string(),
            car: None,
            ffb_gain: 0.8,
            dor_deg: 900,
            torque_cap_nm: 0.0,
            is_valid: false,
            expected_errors: vec!["torque_cap_nm must be positive".to_string()],
        }
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

    pub fn with_dor(mut self, dor: u16) -> Self {
        self.dor_deg = dor;
        self
    }
}

#[derive(Debug, Clone)]
pub struct TelemetryFixture {
    pub name: String,
    pub sample_rate_hz: u32,
    pub duration_s: f32,
    pub base_rpm: f32,
    pub base_speed_ms: f32,
    pub ffb_amplitude: f32,
}

impl Default for TelemetryFixture {
    fn default() -> Self {
        Self::basic()
    }
}

impl TelemetryFixture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn basic() -> Self {
        Self {
            name: "Basic".to_string(),
            sample_rate_hz: 60,
            duration_s: 10.0,
            base_rpm: 3000.0,
            base_speed_ms: 50.0,
            ffb_amplitude: 0.3,
        }
    }

    pub fn racing() -> Self {
        Self {
            name: "Racing".to_string(),
            sample_rate_hz: 60,
            duration_s: 10.0,
            base_rpm: 5000.0,
            base_speed_ms: 80.0,
            ffb_amplitude: 0.5,
        }
    }

    pub fn high_performance() -> Self {
        Self {
            name: "High Performance".to_string(),
            sample_rate_hz: 200,
            duration_s: 10.0,
            base_rpm: 7000.0,
            base_speed_ms: 120.0,
            ffb_amplitude: 0.7,
        }
    }

    pub fn with_sample_rate(mut self, rate_hz: u32) -> Self {
        self.sample_rate_hz = rate_hz;
        self
    }

    pub fn with_duration(mut self, duration_s: f32) -> Self {
        self.duration_s = duration_s;
        self
    }

    pub fn total_samples(&self) -> usize {
        (self.duration_s * self.sample_rate_hz as f32) as usize
    }
}

pub fn get_device_fixtures() -> Vec<DeviceCapabilitiesFixture> {
    vec![
        DeviceCapabilitiesFixture::basic_wheel(),
        DeviceCapabilitiesFixture::dd_wheel(),
        DeviceCapabilitiesFixture::high_end_dd(),
    ]
}

pub fn get_profile_fixtures() -> Vec<ProfileFixture> {
    vec![
        ProfileFixture::valid(),
        ProfileFixture::invalid_gain(),
        ProfileFixture::invalid_dor(),
        ProfileFixture::invalid_torque_cap(),
    ]
}

pub fn get_performance_fixtures() -> Vec<PerformanceFixture> {
    vec![
        PerformanceFixture::idle(),
        PerformanceFixture::light_load(),
        PerformanceFixture::normal_load(),
        PerformanceFixture::heavy_load(),
        PerformanceFixture::extreme_load(),
    ]
}

pub fn get_telemetry_fixtures() -> Vec<TelemetryFixture> {
    vec![
        TelemetryFixture::basic(),
        TelemetryFixture::racing(),
        TelemetryFixture::high_performance(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_capabilities_fixtures() {
        let basic = DeviceCapabilitiesFixture::basic_wheel();
        assert!(!basic.supports_raw_torque_1khz);
        assert_eq!(basic.max_torque_nm, 8.0);

        let dd = DeviceCapabilitiesFixture::dd_wheel();
        assert!(dd.supports_raw_torque_1khz);
        assert_eq!(dd.max_torque_nm, 25.0);

        let high_end = DeviceCapabilitiesFixture::high_end_dd();
        assert_eq!(high_end.max_torque_nm, 50.0);
    }

    #[test]
    fn test_device_capabilities_builder() {
        let custom = DeviceCapabilitiesFixture::dd_wheel()
            .with_max_torque(30.0)
            .with_encoder_cpr(32768);

        assert_eq!(custom.max_torque_nm, 30.0);
        assert_eq!(custom.encoder_cpr, 32768);
    }

    #[test]
    fn test_performance_fixtures() {
        let idle = PerformanceFixture::idle();
        assert_eq!(idle.load_level, LoadLevel::Idle);

        let heavy = PerformanceFixture::heavy_load();
        assert_eq!(heavy.load_level, LoadLevel::Heavy);
        assert!(heavy.duration.as_secs() > idle.duration.as_secs());
    }

    #[test]
    fn test_profile_fixtures() {
        let valid = ProfileFixture::valid();
        assert!(valid.is_valid);
        assert!(valid.expected_errors.is_empty());

        let invalid = ProfileFixture::invalid_gain();
        assert!(!invalid.is_valid);
        assert!(!invalid.expected_errors.is_empty());
    }

    #[test]
    fn test_profile_fixture_builder() {
        let profile = ProfileFixture::valid()
            .with_game("acc")
            .with_car("ferrari_488_gt3")
            .with_ffb_gain(0.75)
            .with_dor(720);

        assert_eq!(profile.game, "acc");
        assert_eq!(profile.car, Some("ferrari_488_gt3".to_string()));
        assert_eq!(profile.ffb_gain, 0.75);
        assert_eq!(profile.dor_deg, 720);
    }

    #[test]
    fn test_telemetry_fixtures() {
        let basic = TelemetryFixture::basic();
        assert_eq!(basic.sample_rate_hz, 60);

        let hp = TelemetryFixture::high_performance();
        assert_eq!(hp.sample_rate_hz, 200);
    }

    #[test]
    fn test_telemetry_total_samples() {
        let fixture = TelemetryFixture::basic()
            .with_sample_rate(100)
            .with_duration(5.0);

        assert_eq!(fixture.total_samples(), 500);
    }

    #[test]
    fn test_get_fixtures() {
        assert_eq!(get_device_fixtures().len(), 3);
        assert_eq!(get_profile_fixtures().len(), 4);
        assert_eq!(get_performance_fixtures().len(), 5);
        assert_eq!(get_telemetry_fixtures().len(), 3);
    }
}
