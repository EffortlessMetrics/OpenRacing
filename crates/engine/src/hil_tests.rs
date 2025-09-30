//! Hardware-in-Loop (HIL) Tests for RT Engine with Safety Integration
//!
//! This module provides comprehensive HIL tests that validate the real-time engine
//! with integrated safety systems using synthetic FFB data and timing validation.

use crate::{
    Engine, EngineConfig,
    engine::GameInput,
    rt::FFBMode,
    safety::{SafetyState, FaultType},
    scheduler::RTSetup,
    device::VirtualDevice,
    ports::NormalizedTelemetry,
};
use racing_wheel_schemas::prelude::*;
use crate::ports::TelemetryFlags;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{info, warn};

/// HIL test configuration
#[derive(Debug, Clone)]
pub struct HILTestConfig {
    /// Test duration
    pub duration: Duration,
    /// Target update rate (Hz)
    pub update_rate_hz: f32,
    /// Maximum allowed jitter (µs)
    pub max_jitter_us: f64,
    /// Maximum allowed missed tick rate
    pub max_missed_tick_rate: f64,
    /// Enable safety fault injection
    pub enable_fault_injection: bool,
    /// Enable detailed logging
    pub enable_logging: bool,
}

impl Default for HILTestConfig {
    fn default() -> Self {
        Self {
            duration: Duration::from_secs(5),
            update_rate_hz: 1000.0,
            max_jitter_us: 250.0, // 0.25ms requirement
            max_missed_tick_rate: 0.00001, // 0.001% requirement
            enable_fault_injection: true,
            enable_logging: false,
        }
    }
}

/// Synthetic FFB data generator for testing
pub struct SyntheticFFBGenerator {
    /// Current time offset
    time_offset: Duration,
    /// FFB pattern type
    pattern: FFBPattern,
    /// Start time for pattern generation
    start_time: Instant,
}

/// FFB pattern types for synthetic data
#[derive(Debug, Clone)]
pub enum FFBPattern {
    /// Constant FFB value
    Constant(f32),
    /// Sine wave: amplitude, frequency_hz, phase
    SineWave { amplitude: f32, frequency_hz: f32, phase: f32 },
    /// Square wave: amplitude, frequency_hz, duty_cycle
    SquareWave { amplitude: f32, frequency_hz: f32, duty_cycle: f32 },
    /// Ramp: start, end, duration
    Ramp { start: f32, end: f32, duration: Duration },
    /// Step function with fault injection
    StepWithFault { normal: f32, fault: f32, fault_at: Duration, fault_duration: Duration },
}

impl SyntheticFFBGenerator {
    /// Create new generator with pattern
    pub fn new(pattern: FFBPattern) -> Self {
        Self {
            time_offset: Duration::ZERO,
            pattern,
            start_time: Instant::now(),
        }
    }

    /// Generate next FFB value
    pub fn next_value(&mut self, dt: Duration) -> f32 {
        self.time_offset += dt;
        let t = self.time_offset.as_secs_f32();

        match &self.pattern {
            FFBPattern::Constant(value) => *value,
            
            FFBPattern::SineWave { amplitude, frequency_hz, phase } => {
                amplitude * (2.0 * std::f32::consts::PI * frequency_hz * t + phase).sin()
            },
            
            FFBPattern::SquareWave { amplitude, frequency_hz, duty_cycle } => {
                let period = 1.0 / frequency_hz;
                let phase = (t % period) / period;
                if phase < *duty_cycle { *amplitude } else { -*amplitude }
            },
            
            FFBPattern::Ramp { start, end, duration } => {
                let progress = (t / duration.as_secs_f32()).clamp(0.0, 1.0);
                start + (end - start) * progress
            },
            
            FFBPattern::StepWithFault { normal, fault, fault_at, fault_duration } => {
                if self.time_offset >= *fault_at && self.time_offset <= *fault_at + *fault_duration {
                    *fault
                } else {
                    *normal
                }
            },
        }
    }

    /// Reset generator
    pub fn reset(&mut self) {
        self.time_offset = Duration::ZERO;
        self.start_time = Instant::now();
    }
}

/// HIL test result
#[derive(Debug, Clone)]
pub struct HILTestResult {
    /// Test name
    pub name: String,
    /// Test passed
    pub passed: bool,
    /// Actual test duration
    pub duration: Duration,
    /// Total frames processed
    pub total_frames: u64,
    /// Missed frames
    pub missed_frames: u64,
    /// Maximum jitter observed (µs)
    pub max_jitter_us: f64,
    /// P99 jitter (µs)
    pub p99_jitter_us: f64,
    /// Safety responses validated
    pub safety_responses: Vec<SafetyResponse>,
    /// Errors encountered
    pub errors: Vec<String>,
}

/// Safety response validation result
#[derive(Debug, Clone)]
pub struct SafetyResponse {
    /// Fault type that triggered response
    pub fault_type: FaultType,
    /// Time from fault to response
    pub response_time: Duration,
    /// Response was within acceptable limits
    pub within_limits: bool,
    /// Expected response time limit
    pub limit: Duration,
}

/// HIL test suite for RT engine validation
pub struct HILTestSuite {
    config: HILTestConfig,
}

impl HILTestSuite {
    /// Create new HIL test suite
    pub fn new(config: HILTestConfig) -> Self {
        Self { config }
    }

    /// Run all HIL tests
    pub async fn run_all_tests(&self) -> Vec<HILTestResult> {
        let mut results = Vec::new();

        // Test 1: Basic RT loop with constant FFB
        results.push(self.test_constant_ffb().await);

        // Test 2: Dynamic FFB patterns
        results.push(self.test_dynamic_ffb_patterns().await);

        // Test 3: Safety fault injection and response
        if self.config.enable_fault_injection {
            results.push(self.test_safety_fault_injection().await);
        }

        // Test 4: Timing validation under load
        results.push(self.test_timing_under_load().await);

        // Test 5: SPSC ring buffer stress test
        results.push(self.test_spsc_ring_stress().await);

        results
    }

    /// Test 1: Basic RT loop with constant FFB
    async fn test_constant_ffb(&self) -> HILTestResult {
        info!("Running HIL Test 1: Constant FFB");

        let mut result = HILTestResult {
            name: "Constant FFB Test".to_string(),
            passed: false,
            duration: Duration::ZERO,
            total_frames: 0,
            missed_frames: 0,
            max_jitter_us: 0.0,
            p99_jitter_us: 0.0,
            safety_responses: Vec::new(),
            errors: Vec::new(),
        };

        // Create test engine
        let device_id = "hil-test-device-1".parse::<DeviceId>().unwrap();
        let device = Box::new(VirtualDevice::new(device_id.clone(), "HIL Test Device 1".to_string()));
        
        let config = EngineConfig {
            device_id,
            mode: FFBMode::RawTorque,
            max_safe_torque_nm: 5.0,
            max_high_torque_nm: 25.0,
            enable_blackbox: true,
            rt_setup: RTSetup {
                high_priority: false, // Disable for tests
                lock_memory: false,
                ..Default::default()
            },
        };

        let mut engine = match Engine::new(device, config) {
            Ok(engine) => engine,
            Err(e) => {
                result.errors.push(format!("Failed to create engine: {}", e));
                return result;
            }
        };

        // Start engine
        let test_device = Box::new(VirtualDevice::new(
            "hil-test-device-1".parse::<DeviceId>().unwrap(),
            "HIL Test Device 1".to_string()
        ));

        if let Err(e) = engine.start(test_device).await {
            result.errors.push(format!("Failed to start engine: {}", e));
            return result;
        }

        let start_time = Instant::now();

        // Generate constant FFB input
        let mut ffb_generator = SyntheticFFBGenerator::new(FFBPattern::Constant(0.5));
        let frame_interval = Duration::from_nanos((1_000_000_000.0 / self.config.update_rate_hz) as u64);

        // Send FFB data for test duration
        let mut frame_count = 0u64;
        let mut last_frame_time = Instant::now();

        while start_time.elapsed() < self.config.duration {
            let now = Instant::now();
            let dt = now.duration_since(last_frame_time);
            
            let ffb_value = ffb_generator.next_value(dt);
            
            let game_input = GameInput {
                ffb_scalar: ffb_value,
                telemetry: Some(NormalizedTelemetry {
                    ffb_scalar: ffb_value,
                    rpm: 3000.0,
                    speed_ms: 50.0,
                    slip_ratio: 0.1,
                    gear: 3,
                    flags: TelemetryFlags::default(),
                    car_id: Some("test-car".to_string()),
                    track_id: Some("test-track".to_string()),
                    timestamp: now,
                }),
                timestamp: now,
            };

            if let Err(e) = engine.send_game_input(game_input) {
                if self.config.enable_logging {
                    warn!("Failed to send game input: {}", e);
                }
            }

            frame_count += 1;
            last_frame_time = now;

            // Maintain target frame rate
            sleep(frame_interval).await;
        }

        result.duration = start_time.elapsed();
        result.total_frames = frame_count;

        // Get engine statistics
        match engine.get_stats().await {
            Ok(stats) => {
                result.missed_frames = stats.dropped_frames;
                result.max_jitter_us = stats.jitter_metrics.max_jitter_ns as f64 / 1000.0;
                result.p99_jitter_us = stats.jitter_metrics.p99_jitter_ns() as f64 / 1000.0;

                // Validate timing requirements
                let missed_rate = result.missed_frames as f64 / result.total_frames as f64;
                let timing_ok = result.max_jitter_us <= self.config.max_jitter_us;
                let missed_ok = missed_rate <= self.config.max_missed_tick_rate;

                if !timing_ok {
                    result.errors.push(format!(
                        "Max jitter {:.2}µs exceeds limit {:.2}µs",
                        result.max_jitter_us, self.config.max_jitter_us
                    ));
                }

                if !missed_ok {
                    result.errors.push(format!(
                        "Missed frame rate {:.6} exceeds limit {:.6}",
                        missed_rate, self.config.max_missed_tick_rate
                    ));
                }

                result.passed = timing_ok && missed_ok;
            }
            Err(e) => {
                result.errors.push(format!("Failed to get engine stats: {}", e));
            }
        }

        // Stop engine
        if let Err(e) = engine.stop().await {
            result.errors.push(format!("Failed to stop engine: {}", e));
        }

        if result.passed {
            info!("HIL Test 1: PASSED");
        } else {
            warn!("HIL Test 1: FAILED - {:?}", result.errors);
        }

        result
    }

    /// Test 2: Dynamic FFB patterns
    async fn test_dynamic_ffb_patterns(&self) -> HILTestResult {
        info!("Running HIL Test 2: Dynamic FFB Patterns");

        let mut result = HILTestResult {
            name: "Dynamic FFB Patterns Test".to_string(),
            passed: false,
            duration: Duration::ZERO,
            total_frames: 0,
            missed_frames: 0,
            max_jitter_us: 0.0,
            p99_jitter_us: 0.0,
            safety_responses: Vec::new(),
            errors: Vec::new(),
        };

        // Create test engine
        let device_id = "hil-test-device-2".parse::<DeviceId>().unwrap();
        let device = Box::new(VirtualDevice::new(device_id.clone(), "HIL Test Device 2".to_string()));
        
        let config = EngineConfig {
            device_id,
            mode: FFBMode::RawTorque,
            max_safe_torque_nm: 5.0,
            max_high_torque_nm: 25.0,
            enable_blackbox: true,
            rt_setup: RTSetup {
                high_priority: false,
                lock_memory: false,
                ..Default::default()
            },
        };

        let mut engine = match Engine::new(device, config) {
            Ok(engine) => engine,
            Err(e) => {
                result.errors.push(format!("Failed to create engine: {}", e));
                return result;
            }
        };

        // Start engine
        let test_device = Box::new(VirtualDevice::new(
            "hil-test-device-2".parse::<DeviceId>().unwrap(),
            "HIL Test Device 2".to_string()
        ));

        if let Err(e) = engine.start(test_device).await {
            result.errors.push(format!("Failed to start engine: {}", e));
            return result;
        }

        let start_time = Instant::now();

        // Test multiple FFB patterns
        let patterns = vec![
            FFBPattern::SineWave { amplitude: 0.8, frequency_hz: 2.0, phase: 0.0 },
            FFBPattern::SquareWave { amplitude: 0.6, frequency_hz: 1.0, duty_cycle: 0.5 },
            FFBPattern::Ramp { start: -0.5, end: 0.5, duration: Duration::from_secs(1) },
        ];

        let pattern_duration = self.config.duration / patterns.len() as u32;
        let frame_interval = Duration::from_nanos((1_000_000_000.0 / self.config.update_rate_hz) as u64);

        let mut frame_count = 0u64;

        for (i, pattern) in patterns.iter().enumerate() {
            info!("Testing pattern {}: {:?}", i + 1, pattern);
            
            let mut ffb_generator = SyntheticFFBGenerator::new(pattern.clone());
            let pattern_start = Instant::now();
            let mut last_frame_time = pattern_start;

            while pattern_start.elapsed() < pattern_duration {
                let now = Instant::now();
                let dt = now.duration_since(last_frame_time);
                
                let ffb_value = ffb_generator.next_value(dt);
                
                let game_input = GameInput {
                    ffb_scalar: ffb_value,
                    telemetry: Some(NormalizedTelemetry {
                        ffb_scalar: ffb_value,
                        rpm: 4000.0 + (ffb_value * 1000.0),
                        speed_ms: 60.0 + (ffb_value * 20.0),
                        slip_ratio: 0.05 + (ffb_value.abs() * 0.1),
                        gear: 4,
                        flags: TelemetryFlags::default(),
                        car_id: Some("test-car".to_string()),
                        track_id: Some("test-track".to_string()),
                        timestamp: now,
                    }),
                    timestamp: now,
                };

                if let Err(e) = engine.send_game_input(game_input) {
                    if self.config.enable_logging {
                        warn!("Failed to send game input: {}", e);
                    }
                }

                frame_count += 1;
                last_frame_time = now;

                sleep(frame_interval).await;
            }
        }

        result.duration = start_time.elapsed();
        result.total_frames = frame_count;

        // Get final statistics
        match engine.get_stats().await {
            Ok(stats) => {
                result.missed_frames = stats.dropped_frames;
                result.max_jitter_us = stats.jitter_metrics.max_jitter_ns as f64 / 1000.0;
                result.p99_jitter_us = stats.jitter_metrics.p99_jitter_ns() as f64 / 1000.0;

                let missed_rate = result.missed_frames as f64 / result.total_frames as f64;
                let timing_ok = result.max_jitter_us <= self.config.max_jitter_us;
                let missed_ok = missed_rate <= self.config.max_missed_tick_rate;

                result.passed = timing_ok && missed_ok;

                if !result.passed {
                    if !timing_ok {
                        result.errors.push(format!(
                            "Max jitter {:.2}µs exceeds limit {:.2}µs",
                            result.max_jitter_us, self.config.max_jitter_us
                        ));
                    }
                    if !missed_ok {
                        result.errors.push(format!(
                            "Missed frame rate {:.6} exceeds limit {:.6}",
                            missed_rate, self.config.max_missed_tick_rate
                        ));
                    }
                }
            }
            Err(e) => {
                result.errors.push(format!("Failed to get engine stats: {}", e));
            }
        }

        // Stop engine
        if let Err(e) = engine.stop().await {
            result.errors.push(format!("Failed to stop engine: {}", e));
        }

        if result.passed {
            info!("HIL Test 2: PASSED");
        } else {
            warn!("HIL Test 2: FAILED - {:?}", result.errors);
        }

        result
    }

    /// Test 3: Safety fault injection and response validation
    async fn test_safety_fault_injection(&self) -> HILTestResult {
        info!("Running HIL Test 3: Safety Fault Injection");

        let mut result = HILTestResult {
            name: "Safety Fault Injection Test".to_string(),
            passed: false,
            duration: Duration::ZERO,
            total_frames: 0,
            missed_frames: 0,
            max_jitter_us: 0.0,
            p99_jitter_us: 0.0,
            safety_responses: Vec::new(),
            errors: Vec::new(),
        };

        // Create test engine
        let device_id = "hil-test-device-3".parse::<DeviceId>().unwrap();
        let device = Box::new(VirtualDevice::new(device_id.clone(), "HIL Test Device 3".to_string()));
        
        let config = EngineConfig {
            device_id,
            mode: FFBMode::RawTorque,
            max_safe_torque_nm: 5.0,
            max_high_torque_nm: 25.0,
            enable_blackbox: true,
            rt_setup: RTSetup {
                high_priority: false,
                lock_memory: false,
                ..Default::default()
            },
        };

        let mut engine = match Engine::new(device, config) {
            Ok(engine) => engine,
            Err(e) => {
                result.errors.push(format!("Failed to create engine: {}", e));
                return result;
            }
        };

        // Start engine
        let test_device = Box::new(VirtualDevice::new(
            "hil-test-device-3".parse::<DeviceId>().unwrap(),
            "HIL Test Device 3".to_string()
        ));

        if let Err(e) = engine.start(test_device).await {
            result.errors.push(format!("Failed to start engine: {}", e));
            return result;
        }

        let start_time = Instant::now();

        // Test fault injection pattern - normal operation then fault
        let fault_inject_time = self.config.duration / 2;
        let fault_duration = Duration::from_millis(100);
        
        let ffb_pattern = FFBPattern::StepWithFault {
            normal: 0.3,
            fault: 1.5, // Exceeds safe torque to trigger safety response
            fault_at: fault_inject_time,
            fault_duration,
        };

        let mut ffb_generator = SyntheticFFBGenerator::new(ffb_pattern);
        let frame_interval = Duration::from_nanos((1_000_000_000.0 / self.config.update_rate_hz) as u64);

        let mut frame_count = 0u64;
        let mut last_frame_time = Instant::now();
        let mut fault_injected = false;
        let mut fault_response_time: Option<Duration> = None;

        while start_time.elapsed() < self.config.duration {
            let now = Instant::now();
            let dt = now.duration_since(last_frame_time);
            let elapsed = start_time.elapsed();
            
            let ffb_value = ffb_generator.next_value(dt);
            
            // Check if we're in fault injection period
            if elapsed >= fault_inject_time && elapsed <= fault_inject_time + fault_duration {
                if !fault_injected {
                    info!("Injecting safety fault at {:?}", elapsed);
                    fault_injected = true;
                    
                    // Simulate thermal fault
                    if let Err(e) = engine.update_safety(true, 85) { // High temperature
                        warn!("Failed to update safety: {}", e);
                    }
                }
            }

            let game_input = GameInput {
                ffb_scalar: ffb_value,
                telemetry: Some(NormalizedTelemetry {
                    ffb_scalar: ffb_value,
                    rpm: 3500.0,
                    speed_ms: 45.0,
                    slip_ratio: 0.08,
                    gear: 3,
                    flags: TelemetryFlags::default(),
                    car_id: Some("test-car".to_string()),
                    track_id: Some("test-track".to_string()),
                    timestamp: now,
                }),
                timestamp: now,
            };

            if let Err(e) = engine.send_game_input(game_input) {
                if self.config.enable_logging {
                    warn!("Failed to send game input: {}", e);
                }
            }

            // Check for safety response
            if fault_injected && fault_response_time.is_none() {
                if let Ok(stats) = engine.get_stats().await {
                    if matches!(stats.safety_state, SafetyState::Faulted { .. }) {
                        fault_response_time = Some(elapsed - fault_inject_time);
                        info!("Safety response detected at {:?}", fault_response_time.unwrap());
                    }
                }
            }

            frame_count += 1;
            last_frame_time = now;

            sleep(frame_interval).await;
        }

        result.duration = start_time.elapsed();
        result.total_frames = frame_count;

        // Validate safety response
        if let Some(response_time) = fault_response_time {
            let safety_response = SafetyResponse {
                fault_type: FaultType::ThermalLimit,
                response_time,
                within_limits: response_time <= Duration::from_millis(50), // 50ms requirement
                limit: Duration::from_millis(50),
            };

            result.safety_responses.push(safety_response.clone());

            if !safety_response.within_limits {
                result.errors.push(format!(
                    "Safety response time {:.2}ms exceeds 50ms limit",
                    response_time.as_secs_f64() * 1000.0
                ));
            }
        } else {
            result.errors.push("No safety response detected to fault injection".to_string());
        }

        // Get final statistics
        match engine.get_stats().await {
            Ok(stats) => {
                result.missed_frames = stats.dropped_frames;
                result.max_jitter_us = stats.jitter_metrics.max_jitter_ns as f64 / 1000.0;
                result.p99_jitter_us = stats.jitter_metrics.p99_jitter_ns() as f64 / 1000.0;

                let missed_rate = result.missed_frames as f64 / result.total_frames as f64;
                let timing_ok = result.max_jitter_us <= self.config.max_jitter_us;
                let missed_ok = missed_rate <= self.config.max_missed_tick_rate;
                let safety_ok = result.safety_responses.iter().all(|r| r.within_limits);

                result.passed = timing_ok && missed_ok && safety_ok;

                if !result.passed {
                    if !timing_ok {
                        result.errors.push(format!(
                            "Max jitter {:.2}µs exceeds limit {:.2}µs",
                            result.max_jitter_us, self.config.max_jitter_us
                        ));
                    }
                    if !missed_ok {
                        result.errors.push(format!(
                            "Missed frame rate {:.6} exceeds limit {:.6}",
                            missed_rate, self.config.max_missed_tick_rate
                        ));
                    }
                }
            }
            Err(e) => {
                result.errors.push(format!("Failed to get engine stats: {}", e));
            }
        }

        // Stop engine
        if let Err(e) = engine.stop().await {
            result.errors.push(format!("Failed to stop engine: {}", e));
        }

        if result.passed {
            info!("HIL Test 3: PASSED");
        } else {
            warn!("HIL Test 3: FAILED - {:?}", result.errors);
        }

        result
    }

    /// Test 4: Timing validation under load
    async fn test_timing_under_load(&self) -> HILTestResult {
        info!("Running HIL Test 4: Timing Under Load");

        let mut result = HILTestResult {
            name: "Timing Under Load Test".to_string(),
            passed: false,
            duration: Duration::ZERO,
            total_frames: 0,
            missed_frames: 0,
            max_jitter_us: 0.0,
            p99_jitter_us: 0.0,
            safety_responses: Vec::new(),
            errors: Vec::new(),
        };

        // Create test engine with higher load
        let device_id = "hil-test-device-4".parse::<DeviceId>().unwrap();
        let device = Box::new(VirtualDevice::new(device_id.clone(), "HIL Test Device 4".to_string()));
        
        let config = EngineConfig {
            device_id,
            mode: FFBMode::RawTorque,
            max_safe_torque_nm: 5.0,
            max_high_torque_nm: 25.0,
            enable_blackbox: true, // Adds processing load
            rt_setup: RTSetup {
                high_priority: false,
                lock_memory: false,
                ..Default::default()
            },
        };

        let mut engine = match Engine::new(device, config) {
            Ok(engine) => engine,
            Err(e) => {
                result.errors.push(format!("Failed to create engine: {}", e));
                return result;
            }
        };

        // Start engine
        let test_device = Box::new(VirtualDevice::new(
            "hil-test-device-4".parse::<DeviceId>().unwrap(),
            "HIL Test Device 4".to_string()
        ));

        if let Err(e) = engine.start(test_device).await {
            result.errors.push(format!("Failed to start engine: {}", e));
            return result;
        }

        let start_time = Instant::now();

        // Generate high-frequency changing FFB to stress the system
        let mut ffb_generator = SyntheticFFBGenerator::new(FFBPattern::SineWave {
            amplitude: 0.9,
            frequency_hz: 10.0, // High frequency changes
            phase: 0.0,
        });

        let frame_interval = Duration::from_nanos((1_000_000_000.0 / self.config.update_rate_hz) as u64);
        let mut frame_count = 0u64;
        let mut last_frame_time = Instant::now();

        // Add computational load by sending complex telemetry
        while start_time.elapsed() < self.config.duration {
            let now = Instant::now();
            let dt = now.duration_since(last_frame_time);
            
            let ffb_value = ffb_generator.next_value(dt);
            
            // Complex telemetry data to add processing load
            let game_input = GameInput {
                ffb_scalar: ffb_value,
                telemetry: Some(NormalizedTelemetry {
                    ffb_scalar: ffb_value,
                    rpm: 5000.0 + (ffb_value * 2000.0),
                    speed_ms: 80.0 + (ffb_value * 40.0),
                    slip_ratio: 0.15 + (ffb_value.abs() * 0.2),
                    gear: if ffb_value > 0.5 { 5 } else { 4 },
                    flags: TelemetryFlags {
                        yellow_flag: true,
                        red_flag: true,
                        blue_flag: true,
                        checkered_flag: true,
                        pit_limiter: true,
                        drs_enabled: true,
                        ers_available: true,
                        in_pit: true,
                    }, // All flags set
                    car_id: Some("high-performance-car".to_string()),
                    track_id: Some("complex-track-layout".to_string()),
                    timestamp: now,
                }),
                timestamp: now,
            };

            if let Err(e) = engine.send_game_input(game_input) {
                if self.config.enable_logging {
                    warn!("Failed to send game input: {}", e);
                }
            }

            frame_count += 1;
            last_frame_time = now;

            sleep(frame_interval).await;
        }

        result.duration = start_time.elapsed();
        result.total_frames = frame_count;

        // Get statistics and validate under load
        match engine.get_stats().await {
            Ok(stats) => {
                result.missed_frames = stats.dropped_frames;
                result.max_jitter_us = stats.jitter_metrics.max_jitter_ns as f64 / 1000.0;
                result.p99_jitter_us = stats.jitter_metrics.p99_jitter_ns() as f64 / 1000.0;

                let missed_rate = result.missed_frames as f64 / result.total_frames as f64;
                let timing_ok = result.max_jitter_us <= self.config.max_jitter_us;
                let missed_ok = missed_rate <= self.config.max_missed_tick_rate;

                result.passed = timing_ok && missed_ok;

                if !result.passed {
                    if !timing_ok {
                        result.errors.push(format!(
                            "Max jitter under load {:.2}µs exceeds limit {:.2}µs",
                            result.max_jitter_us, self.config.max_jitter_us
                        ));
                    }
                    if !missed_ok {
                        result.errors.push(format!(
                            "Missed frame rate under load {:.6} exceeds limit {:.6}",
                            missed_rate, self.config.max_missed_tick_rate
                        ));
                    }
                }
            }
            Err(e) => {
                result.errors.push(format!("Failed to get engine stats: {}", e));
            }
        }

        // Stop engine
        if let Err(e) = engine.stop().await {
            result.errors.push(format!("Failed to stop engine: {}", e));
        }

        if result.passed {
            info!("HIL Test 4: PASSED");
        } else {
            warn!("HIL Test 4: FAILED - {:?}", result.errors);
        }

        result
    }

    /// Test 5: SPSC ring buffer stress test
    async fn test_spsc_ring_stress(&self) -> HILTestResult {
        info!("Running HIL Test 5: SPSC Ring Stress Test");

        let mut result = HILTestResult {
            name: "SPSC Ring Stress Test".to_string(),
            passed: false,
            duration: Duration::ZERO,
            total_frames: 0,
            missed_frames: 0,
            max_jitter_us: 0.0,
            p99_jitter_us: 0.0,
            safety_responses: Vec::new(),
            errors: Vec::new(),
        };

        // Create test engine
        let device_id = "hil-test-device-5".parse::<DeviceId>().unwrap();
        let device = Box::new(VirtualDevice::new(device_id.clone(), "HIL Test Device 5".to_string()));
        
        let config = EngineConfig {
            device_id,
            mode: FFBMode::RawTorque,
            max_safe_torque_nm: 5.0,
            max_high_torque_nm: 25.0,
            enable_blackbox: true,
            rt_setup: RTSetup {
                high_priority: false,
                lock_memory: false,
                ..Default::default()
            },
        };

        let mut engine = match Engine::new(device, config) {
            Ok(engine) => engine,
            Err(e) => {
                result.errors.push(format!("Failed to create engine: {}", e));
                return result;
            }
        };

        // Start engine
        let test_device = Box::new(VirtualDevice::new(
            "hil-test-device-5".parse::<DeviceId>().unwrap(),
            "HIL Test Device 5".to_string()
        ));

        if let Err(e) = engine.start(test_device).await {
            result.errors.push(format!("Failed to start engine: {}", e));
            return result;
        }

        let start_time = Instant::now();

        // Stress test: Send data at higher rate than processing to test ring buffer
        let send_rate_hz = self.config.update_rate_hz * 1.5; // 50% higher than processing rate
        let send_interval = Duration::from_nanos((1_000_000_000.0 / send_rate_hz) as u64);

        let mut ffb_generator = SyntheticFFBGenerator::new(FFBPattern::SineWave {
            amplitude: 0.7,
            frequency_hz: 5.0,
            phase: 0.0,
        });

        let mut frame_count = 0u64;
        let mut send_failures = 0u64;
        let mut last_frame_time = Instant::now();

        while start_time.elapsed() < self.config.duration {
            let now = Instant::now();
            let dt = now.duration_since(last_frame_time);
            
            let ffb_value = ffb_generator.next_value(dt);
            
            let game_input = GameInput {
                ffb_scalar: ffb_value,
                telemetry: Some(NormalizedTelemetry {
                    ffb_scalar: ffb_value,
                    rpm: 4500.0,
                    speed_ms: 65.0,
                    slip_ratio: 0.12,
                    gear: 4,
                    flags: TelemetryFlags::default(),
                    car_id: Some("test-car".to_string()),
                    track_id: Some("test-track".to_string()),
                    timestamp: now,
                }),
                timestamp: now,
            };

            match engine.send_game_input(game_input) {
                Ok(()) => {},
                Err(e) => {
                    send_failures += 1;
                    if self.config.enable_logging && send_failures % 100 == 0 {
                        warn!("Send failure #{}: {}", send_failures, e);
                    }
                }
            }

            frame_count += 1;
            last_frame_time = now;

            sleep(send_interval).await;
        }

        result.duration = start_time.elapsed();
        result.total_frames = frame_count;

        // Validate ring buffer behavior
        let send_failure_rate = send_failures as f64 / frame_count as f64;
        let acceptable_failure_rate = 0.1; // 10% failures acceptable under stress

        if send_failure_rate > acceptable_failure_rate {
            result.errors.push(format!(
                "Send failure rate {:.2}% exceeds acceptable limit {:.2}%",
                send_failure_rate * 100.0, acceptable_failure_rate * 100.0
            ));
        }

        // Get final statistics
        match engine.get_stats().await {
            Ok(stats) => {
                result.missed_frames = stats.dropped_frames;
                result.max_jitter_us = stats.jitter_metrics.max_jitter_ns as f64 / 1000.0;
                result.p99_jitter_us = stats.jitter_metrics.p99_jitter_ns() as f64 / 1000.0;

                let missed_rate = result.missed_frames as f64 / result.total_frames as f64;
                let timing_ok = result.max_jitter_us <= self.config.max_jitter_us;
                let missed_ok = missed_rate <= self.config.max_missed_tick_rate;
                let ring_ok = send_failure_rate <= acceptable_failure_rate;

                result.passed = timing_ok && missed_ok && ring_ok;

                if !result.passed {
                    if !timing_ok {
                        result.errors.push(format!(
                            "Max jitter {:.2}µs exceeds limit {:.2}µs",
                            result.max_jitter_us, self.config.max_jitter_us
                        ));
                    }
                    if !missed_ok {
                        result.errors.push(format!(
                            "Missed frame rate {:.6} exceeds limit {:.6}",
                            missed_rate, self.config.max_missed_tick_rate
                        ));
                    }
                }
            }
            Err(e) => {
                result.errors.push(format!("Failed to get engine stats: {}", e));
            }
        }

        // Stop engine
        if let Err(e) = engine.stop().await {
            result.errors.push(format!("Failed to stop engine: {}", e));
        }

        if result.passed {
            info!("HIL Test 5: PASSED");
        } else {
            warn!("HIL Test 5: FAILED - {:?}", result.errors);
        }

        result
    }

    /// Generate test report
    pub fn generate_report(&self, results: &[HILTestResult]) -> String {
        let mut report = String::new();

        report.push_str("# HIL Test Suite Report\n\n");

        let passed_count = results.iter().filter(|r| r.passed).count();
        let total_count = results.len();

        report.push_str(&format!("## Summary\n"));
        report.push_str(&format!("- Total tests: {}\n", total_count));
        report.push_str(&format!("- Passed: {}\n", passed_count));
        report.push_str(&format!("- Failed: {}\n", total_count - passed_count));
        report.push_str(&format!("- Success rate: {:.1}%\n\n", 
            (passed_count as f64 / total_count as f64) * 100.0));

        report.push_str("## Test Results\n\n");

        for result in results {
            report.push_str(&format!("### {}\n", result.name));
            report.push_str(&format!("- Status: {}\n", if result.passed { "PASSED" } else { "FAILED" }));
            report.push_str(&format!("- Duration: {:.2}s\n", result.duration.as_secs_f64()));
            report.push_str(&format!("- Total frames: {}\n", result.total_frames));
            report.push_str(&format!("- Missed frames: {}\n", result.missed_frames));
            report.push_str(&format!("- Max jitter: {:.2} µs\n", result.max_jitter_us));
            report.push_str(&format!("- P99 jitter: {:.2} µs\n", result.p99_jitter_us));

            if !result.safety_responses.is_empty() {
                report.push_str("- Safety responses:\n");
                for response in &result.safety_responses {
                    report.push_str(&format!("  - {:?}: {:.2}ms (limit: {:.2}ms) - {}\n",
                        response.fault_type,
                        response.response_time.as_secs_f64() * 1000.0,
                        response.limit.as_secs_f64() * 1000.0,
                        if response.within_limits { "OK" } else { "EXCEEDED" }
                    ));
                }
            }

            if !result.errors.is_empty() {
                report.push_str("- Errors:\n");
                for error in &result.errors {
                    report.push_str(&format!("  - {}\n", error));
                }
            }

            report.push_str("\n");
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_synthetic_ffb_generator() {
        let mut generator = SyntheticFFBGenerator::new(FFBPattern::Constant(0.5));
        
        let value1 = generator.next_value(Duration::from_millis(1));
        let value2 = generator.next_value(Duration::from_millis(1));
        
        assert_eq!(value1, 0.5);
        assert_eq!(value2, 0.5);
    }

    #[tokio::test]
    async fn test_sine_wave_pattern() {
        let mut generator = SyntheticFFBGenerator::new(FFBPattern::SineWave {
            amplitude: 1.0,
            frequency_hz: 1.0,
            phase: 0.0,
        });
        
        let value_at_0 = generator.next_value(Duration::ZERO);
        let value_at_quarter = generator.next_value(Duration::from_millis(250));
        
        assert!((value_at_0 - 0.0).abs() < 0.1);
        assert!((value_at_quarter - 1.0).abs() < 0.1);
    }

    #[tokio::test]
    async fn test_hil_test_suite_creation() {
        let config = HILTestConfig::default();
        let suite = HILTestSuite::new(config);
        
        // Just verify we can create the suite
        assert_eq!(suite.config.update_rate_hz, 1000.0);
    }

    #[tokio::test]
    async fn test_hil_basic_test() {
        let config = HILTestConfig {
            duration: Duration::from_millis(100), // Very short test
            enable_fault_injection: false,
            enable_logging: false,
            ..Default::default()
        };
        
        let suite = HILTestSuite::new(config);
        let result = suite.test_constant_ffb().await;
        
        // Test should complete without crashing
        assert_eq!(result.name, "Constant FFB Test");
        assert!(result.total_frames > 0);
    }
}
