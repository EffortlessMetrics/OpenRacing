//! Real-time engine core types and structures

pub use openracing_errors::RTError;
pub use openracing_errors::RTResult;
use std::time::Instant;

/// Real-time frame data processed at 1kHz
#[repr(C)]
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Frame {
    /// Force feedback input from game (-1.0 to 1.0)
    pub ffb_in: f32,
    /// Torque output after filtering (-1.0 to 1.0)
    pub torque_out: f32,
    /// Wheel angular velocity in rad/s for speed-adaptive filters
    pub wheel_speed: f32,
    /// Hands-off detection flag
    pub hands_off: bool,
    /// Monotonic timestamp in nanoseconds
    pub ts_mono_ns: u64,
    /// Sequence number for device communication
    pub seq: u16,
}

impl Default for Frame {
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

/// Force feedback mode selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FFBMode {
    /// Game emits DirectInput/PID effects, device processes
    PidPassthrough,
    /// Host synthesizes torque at 1kHz, sends to device
    RawTorque,
    /// Host computes torque from game telemetry
    TelemetrySynth,
}

impl std::fmt::Display for FFBMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FFBMode::PidPassthrough => write!(f, "PID Pass-through"),
            FFBMode::RawTorque => write!(f, "Raw Torque"),
            FFBMode::TelemetrySynth => write!(f, "Telemetry Synthesis"),
        }
    }
}

/// Performance metrics for monitoring
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub total_ticks: u64,
    pub missed_ticks: u64,
    pub max_jitter_ns: u64,
    pub p99_jitter_ns: u64,
    pub last_update: Instant,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            total_ticks: 0,
            missed_ticks: 0,
            max_jitter_ns: 0,
            p99_jitter_ns: 0,
            last_update: Instant::now(),
        }
    }
}

impl PerformanceMetrics {
    pub fn missed_tick_rate(&self) -> f64 {
        if self.total_ticks == 0 {
            0.0
        } else {
            (self.missed_ticks as f64) / (self.total_ticks as f64)
        }
    }

    pub fn p99_jitter_us(&self) -> f64 {
        self.p99_jitter_ns as f64 / 1000.0
    }
}
