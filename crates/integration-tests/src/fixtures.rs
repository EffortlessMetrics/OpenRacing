//! Test fixtures and data for integration tests

use racing_wheel_schemas::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Test fixture for virtual device configurations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceFixture {
    pub name: String,
    pub capabilities: DeviceCapabilities,
    pub telemetry_data: TelemetryFixture,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryFixture {
    pub samples: Vec<TelemetrySample>,
    pub sample_rate_hz: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetrySample {
    pub timestamp_ms: u64,
    pub ffb_scalar: f32,
    pub rpm: f32,
    pub speed_ms: f32,
    pub slip_ratio: f32,
    pub gear: i8,
    pub flags: u32,
}

/// Profile test fixtures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileFixture {
    pub name: String,
    pub json_content: String,
    pub expected_valid: bool,
    pub expected_errors: Vec<String>,
}

/// Performance test fixtures
#[derive(Debug, Clone)]
pub struct PerformanceFixture {
    pub name: String,
    pub duration: Duration,
    pub expected_jitter_p99_ms: f64,
    pub expected_latency_p99_us: f64,
    pub load_level: LoadLevel,
}

#[derive(Debug, Clone)]
pub enum LoadLevel {
    Idle,
    Light,
    Normal,
    Heavy,
    Extreme,
}

impl DeviceFixture {
    /// Create a standard DD wheel fixture
    pub fn dd_wheel() -> Self {
        Self {
            name: "Test DD Wheel".to_string(),
            capabilities: DeviceCapabilities {
                supports_pid: true,
                supports_raw_torque_1khz: true,
                supports_health_stream: true,
                supports_led_bus: true,
                max_torque: TorqueNm::from_raw(25.0), // 25 Nm
                encoder_cpr: 65535,
                min_report_period_us: 1000,
            },
            telemetry_data: TelemetryFixture::racing_scenario(),
        }
    }

    /// Create a basic wheel fixture (PID only)
    pub fn basic_wheel() -> Self {
        Self {
            name: "Basic Wheel".to_string(),
            capabilities: DeviceCapabilities {
                supports_pid: true,
                supports_raw_torque_1khz: false,
                supports_health_stream: false,
                supports_led_bus: false,
                max_torque: TorqueNm::from_raw(8.0), // 8 Nm
                encoder_cpr: 4096,
                min_report_period_us: 2000,
            },
            telemetry_data: TelemetryFixture::basic_scenario(),
        }
    }

    /// Create a high-end wheel fixture
    pub fn high_end_wheel() -> Self {
        Self {
            name: "High-End DD Wheel".to_string(),
            capabilities: DeviceCapabilities {
                supports_pid: true,
                supports_raw_torque_1khz: true,
                supports_health_stream: true,
                supports_led_bus: true,
                max_torque: TorqueNm::from_raw(50.0), // 50 Nm
                encoder_cpr: 65535,
                min_report_period_us: 500,
            },
            telemetry_data: TelemetryFixture::high_performance_scenario(),
        }
    }
}

impl TelemetryFixture {
    /// Create racing scenario telemetry
    pub fn racing_scenario() -> Self {
        let mut samples = Vec::new();

        // Generate 10 seconds of racing telemetry at 60Hz
        for i in 0..600 {
            let time_s = i as f32 / 60.0;

            // Simulate a racing scenario with varying RPM, speed, etc.
            let rpm = 3000.0 + 2000.0 * (time_s * 0.5).sin();
            let speed = 50.0 + 30.0 * (time_s * 0.3).sin();
            let ffb = 0.3 * (time_s * 2.0).sin() + 0.1 * (time_s * 8.0).sin();

            samples.push(TelemetrySample {
                timestamp_ms: (time_s * 1000.0) as u64,
                ffb_scalar: ffb,
                rpm,
                speed_ms: speed,
                slip_ratio: 0.05 * (time_s * 4.0).sin().abs(),
                gear: ((speed / 20.0) as i8).clamp(1, 6),
                flags: if i % 120 == 0 { 0x01 } else { 0x00 }, // Occasional flag
            });
        }

        Self {
            samples,
            sample_rate_hz: 60,
        }
    }

    /// Create basic telemetry scenario
    pub fn basic_scenario() -> Self {
        let mut samples = Vec::new();

        // Simple constant telemetry
        for i in 0..300 {
            samples.push(TelemetrySample {
                timestamp_ms: (i * 33) as u64, // ~30Hz
                ffb_scalar: 0.2,
                rpm: 2500.0,
                speed_ms: 25.0,
                slip_ratio: 0.02,
                gear: 3,
                flags: 0x00,
            });
        }

        Self {
            samples,
            sample_rate_hz: 30,
        }
    }

    /// Create high-performance scenario
    pub fn high_performance_scenario() -> Self {
        let mut samples = Vec::new();

        // High-frequency, high-detail telemetry
        for i in 0..2000 {
            let time_s = i as f32 / 200.0; // 200Hz for 10 seconds

            // Complex FFB with multiple frequency components
            let base_ffb = 0.4 * (time_s * 1.5).sin();
            let road_texture = 0.1 * (time_s * 20.0).sin();
            let kerb_effect = if (time_s % 2.0) < 0.1 { 0.3 } else { 0.0 };
            let ffb = base_ffb + road_texture + kerb_effect;

            samples.push(TelemetrySample {
                timestamp_ms: (time_s * 1000.0) as u64,
                ffb_scalar: ffb.clamp(-1.0, 1.0),
                rpm: 4000.0 + 3000.0 * (time_s * 0.8).sin(),
                speed_ms: 80.0 + 40.0 * (time_s * 0.4).sin(),
                slip_ratio: 0.1 * (time_s * 6.0).sin().abs(),
                gear: ((time_s * 0.2) as i8 % 6) + 1,
                flags: if (time_s % 3.0) < 0.1 { 0x02 } else { 0x00 },
            });
        }

        Self {
            samples,
            sample_rate_hz: 200,
        }
    }
}

impl ProfileFixture {
    /// Valid profile fixture
    pub fn valid_profile() -> Self {
        Self {
            name: "Valid Profile".to_string(),
            json_content: r#"{
                "schema": "wheel.profile/1",
                "scope": {
                    "game": "iracing",
                    "car": "gt3"
                },
                "base": {
                    "ffb_gain": 0.68,
                    "dor_deg": 540,
                    "torque_cap_nm": 10.0,
                    "filters": {
                        "reconstruction": 4,
                        "friction": 0.12,
                        "damper": 0.18,
                        "inertia": 0.08,
                        "notch": [{"hz": 7.5, "q": 3.0, "gain_db": -10.0}],
                        "slew_rate": 0.85,
                        "curve": [
                            {"in": 0.0, "out": 0.0},
                            {"in": 0.5, "out": 0.6},
                            {"in": 1.0, "out": 1.0}
                        ]
                    }
                },
                "leds": {
                    "rpm_bands": [0.75, 0.82, 0.88, 0.92, 0.96],
                    "pattern": "wipe"
                }
            }"#
            .to_string(),
            expected_valid: true,
            expected_errors: vec![],
        }
    }

    /// Invalid profile fixture (missing required fields)
    pub fn invalid_profile_missing_fields() -> Self {
        Self {
            name: "Invalid Profile - Missing Fields".to_string(),
            json_content: r#"{
                "schema": "wheel.profile/1",
                "base": {
                    "ffb_gain": 0.68
                }
            }"#
            .to_string(),
            expected_valid: false,
            expected_errors: vec![
                "Missing required field: scope".to_string(),
                "Missing required field: base.dor_deg".to_string(),
            ],
        }
    }

    /// Invalid profile fixture (invalid values)
    pub fn invalid_profile_bad_values() -> Self {
        Self {
            name: "Invalid Profile - Bad Values".to_string(),
            json_content: r#"{
                "schema": "wheel.profile/1",
                "scope": {
                    "game": "iracing"
                },
                "base": {
                    "ffb_gain": 2.5,
                    "dor_deg": -100,
                    "torque_cap_nm": 0,
                    "filters": {
                        "reconstruction": 15,
                        "friction": -0.5
                    }
                }
            }"#
            .to_string(),
            expected_valid: false,
            expected_errors: vec![
                "ffb_gain must be between 0.0 and 1.0".to_string(),
                "dor_deg must be positive".to_string(),
                "torque_cap_nm must be positive".to_string(),
                "reconstruction must be between 0 and 8".to_string(),
                "friction must be non-negative".to_string(),
            ],
        }
    }

    /// Profile with non-monotonic curve
    pub fn invalid_profile_non_monotonic() -> Self {
        Self {
            name: "Invalid Profile - Non-Monotonic Curve".to_string(),
            json_content: r#"{
                "schema": "wheel.profile/1",
                "scope": {
                    "game": "iracing"
                },
                "base": {
                    "ffb_gain": 0.8,
                    "dor_deg": 900,
                    "torque_cap_nm": 15.0,
                    "filters": {
                        "curve": [
                            {"in": 0.0, "out": 0.0},
                            {"in": 0.8, "out": 0.9},
                            {"in": 0.5, "out": 0.6},
                            {"in": 1.0, "out": 1.0}
                        ]
                    }
                }
            }"#
            .to_string(),
            expected_valid: false,
            expected_errors: vec!["Curve points must be monotonic in input values".to_string()],
        }
    }
}

impl PerformanceFixture {
    /// Idle performance expectations
    pub fn idle_performance() -> Self {
        Self {
            name: "Idle Performance".to_string(),
            duration: Duration::from_secs(60),
            expected_jitter_p99_ms: 0.1,
            expected_latency_p99_us: 150.0,
            load_level: LoadLevel::Idle,
        }
    }

    /// Normal load performance expectations
    pub fn normal_load_performance() -> Self {
        Self {
            name: "Normal Load Performance".to_string(),
            duration: Duration::from_secs(180),
            expected_jitter_p99_ms: 0.2,
            expected_latency_p99_us: 250.0,
            load_level: LoadLevel::Normal,
        }
    }

    /// Heavy load performance expectations
    pub fn heavy_load_performance() -> Self {
        Self {
            name: "Heavy Load Performance".to_string(),
            duration: Duration::from_secs(300),
            expected_jitter_p99_ms: 0.25,
            expected_latency_p99_us: 300.0,
            load_level: LoadLevel::Heavy,
        }
    }
}

/// Get all device fixtures
pub fn get_device_fixtures() -> Vec<DeviceFixture> {
    vec![
        DeviceFixture::dd_wheel(),
        DeviceFixture::basic_wheel(),
        DeviceFixture::high_end_wheel(),
    ]
}

/// Get all profile fixtures
pub fn get_profile_fixtures() -> Vec<ProfileFixture> {
    vec![
        ProfileFixture::valid_profile(),
        ProfileFixture::invalid_profile_missing_fields(),
        ProfileFixture::invalid_profile_bad_values(),
        ProfileFixture::invalid_profile_non_monotonic(),
    ]
}

/// Get all performance fixtures
pub fn get_performance_fixtures() -> Vec<PerformanceFixture> {
    vec![
        PerformanceFixture::idle_performance(),
        PerformanceFixture::normal_load_performance(),
        PerformanceFixture::heavy_load_performance(),
    ]
}
