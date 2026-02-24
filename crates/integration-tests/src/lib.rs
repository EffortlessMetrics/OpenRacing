//! Comprehensive integration test suite for Racing Wheel Software
//!
//! This crate provides end-to-end testing capabilities including:
//! - User journey validation (UJ-01 through UJ-04)
//! - Performance gates for CI (jitter ≤0.25ms, HID latency ≤300μs)
//! - Soak testing for 48-hour continuous operation
//! - Acceptance tests mapped to requirement IDs
//! - Hot-plug stress testing

#![deny(rust_2018_idioms)]
#![deny(warnings)]
#![deny(unused_must_use)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::print_stdout)]

pub mod acceptance;
pub mod common;
pub mod fanatec_virtual;
pub mod fixtures;
pub mod gates;
pub mod logitech_virtual;
pub mod moza_virtual;
pub mod performance;
pub mod soak;
pub mod stress;
pub mod user_journeys;

use anyhow::Result;
use std::time::Duration;
use tracing::info;

/// Performance thresholds as defined in requirements
pub const MAX_JITTER_P99_MS: f64 = 0.25;
pub const MAX_HID_LATENCY_P99_US: f64 = 300.0;
pub const FFB_FREQUENCY_HZ: u32 = 1000;
pub const SOAK_TEST_DURATION: Duration = Duration::from_secs(48 * 60 * 60); // 48 hours

/// Test configuration for different test types
#[derive(Debug, Clone)]
pub struct TestConfig {
    pub duration: Duration,
    pub sample_rate_hz: u32,
    pub enable_tracing: bool,
    pub enable_metrics: bool,
    pub virtual_device: bool,
    pub stress_level: StressLevel,
}

#[derive(Debug, Clone, Copy)]
pub enum StressLevel {
    Light,
    Medium,
    Heavy,
    Extreme,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            duration: Duration::from_secs(60),
            sample_rate_hz: FFB_FREQUENCY_HZ,
            enable_tracing: true,
            enable_metrics: true,
            virtual_device: true,
            stress_level: StressLevel::Medium,
        }
    }
}

/// Test result with performance metrics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TestResult {
    pub passed: bool,
    pub duration: Duration,
    pub metrics: PerformanceMetrics,
    pub errors: Vec<String>,
    pub requirement_coverage: Vec<String>,
}

/// Performance metrics collected during tests
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PerformanceMetrics {
    pub jitter_p50_ms: f64,
    pub jitter_p99_ms: f64,
    pub hid_latency_p50_us: f64,
    pub hid_latency_p99_us: f64,
    pub missed_ticks: u64,
    pub total_ticks: u64,
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: f64,
    pub max_torque_saturation_percent: f64,
}

impl PerformanceMetrics {
    /// Check if metrics meet performance gates
    pub fn meets_performance_gates(&self) -> bool {
        self.jitter_p99_ms <= MAX_JITTER_P99_MS
            && self.hid_latency_p99_us <= MAX_HID_LATENCY_P99_US
            && self.missed_ticks == 0
    }

    /// Generate performance report
    pub fn report(&self) -> String {
        format!(
            "Performance Metrics:\n\
             - Jitter P50/P99: {:.3}ms / {:.3}ms (gate: ≤{:.3}ms)\n\
             - HID Latency P50/P99: {:.1}μs / {:.1}μs (gate: ≤{:.1}μs)\n\
             - Missed Ticks: {} / {} ({:.6}%)\n\
             - CPU Usage: {:.1}%\n\
             - Memory Usage: {:.1}MB\n\
             - Max Torque Saturation: {:.1}%",
            self.jitter_p50_ms,
            self.jitter_p99_ms,
            MAX_JITTER_P99_MS,
            self.hid_latency_p50_us,
            self.hid_latency_p99_us,
            MAX_HID_LATENCY_P99_US,
            self.missed_ticks,
            self.total_ticks,
            (self.missed_ticks as f64 / self.total_ticks as f64) * 100.0,
            self.cpu_usage_percent,
            self.memory_usage_mb,
            self.max_torque_saturation_percent
        )
    }
}

/// Initialize test environment with proper logging and tracing
pub fn init_test_environment() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("racing_wheel=debug,integration_tests=debug")
        .with_test_writer()
        .try_init()
        .ok(); // Ignore error if already initialized

    info!("Integration test environment initialized");
    Ok(())
}

/// Cleanup test environment
pub fn cleanup_test_environment() -> Result<()> {
    info!("Cleaning up test environment");
    // Cleanup any temporary files, stop services, etc.
    Ok(())
}
