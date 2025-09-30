//! Common utilities for integration tests

use std::sync::Arc;
use std::time::{Duration, Instant};
use anyhow::Result;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

use racing_wheel_engine::{Engine, EngineConfig};
use racing_wheel_service::{WheelService, ServiceConfig};
use racing_wheel_schemas::prelude::*;

use crate::{TestConfig, PerformanceMetrics};

/// Mock virtual device for testing
#[derive(Debug, Clone)]
pub struct VirtualDevice {
    pub id: DeviceId,
    pub name: String,
    pub capabilities: DeviceCapabilities,
    pub connected: bool,
    pub last_torque_command: f32,
    pub telemetry_data: VirtualTelemetry,
}

#[derive(Debug, Clone, Default)]
pub struct VirtualTelemetry {
    pub wheel_angle_deg: f32,
    pub wheel_speed_rad_s: f32,
    pub temperature_c: u8,
    pub fault_flags: u8,
    pub hands_on: bool,
}

impl VirtualDevice {
    pub fn new(name: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string().parse().expect("valid device id"),
            name: name.to_string(),
            capabilities: DeviceCapabilities {
                supports_pid: true,
                supports_raw_torque_1khz: true,
                supports_health_stream: true,
                supports_led_bus: true,
                max_torque: TorqueNm::from_raw(25.0), // 25 Nm
                encoder_cpr: 65535,
                min_report_period_us: 1000,
            },
            connected: true,
            last_torque_command: 0.0,
            telemetry_data: VirtualTelemetry::default(),
        }
    }

    pub fn disconnect(&mut self) {
        self.connected = false;
        info!("Virtual device {} disconnected", self.name);
    }

    pub fn reconnect(&mut self) {
        self.connected = true;
        info!("Virtual device {} reconnected", self.name);
    }

    pub fn inject_fault(&mut self, fault_type: u8) {
        self.telemetry_data.fault_flags |= fault_type;
        info!("Injected fault {} into device {}", fault_type, self.name);
    }

    pub fn clear_faults(&mut self) {
        self.telemetry_data.fault_flags = 0;
        info!("Cleared faults for device {}", self.name);
    }
}

/// Test harness for managing test environment
pub struct TestHarness {
    pub config: TestConfig,
    pub service: Option<Arc<WheelService>>,
    pub virtual_devices: Vec<Arc<RwLock<VirtualDevice>>>,
    pub metrics_collector: MetricsCollector,
    pub start_time: Instant,
}

impl TestHarness {
    pub async fn new(config: TestConfig) -> Result<Self> {
        let virtual_device = config.virtual_device;
        let mut harness = Self {
            config,
            service: None,
            virtual_devices: Vec::new(),
            metrics_collector: MetricsCollector::new(),
            start_time: Instant::now(),
        };

        if virtual_device {
            harness.add_virtual_device("Test Wheel DD1").await?;
        }

        Ok(harness)
    }

    pub async fn start_service(&mut self) -> Result<()> {
        let service_config = ServiceConfig {
            enable_rt_thread: true,
            ffb_frequency_hz: self.config.sample_rate_hz,
            enable_safety_interlocks: true,
            enable_diagnostics: self.config.enable_metrics,
            ..Default::default()
        };

        let service = WheelService::new(service_config).await?;
        self.service = Some(Arc::new(service));
        
        info!("Test service started");
        Ok(())
    }

    pub async fn add_virtual_device(&mut self, name: &str) -> Result<DeviceId> {
        let device = Arc::new(RwLock::new(VirtualDevice::new(name)));
        let device_id = device.read().await.id.clone();
        
        self.virtual_devices.push(device);
        info!("Added virtual device: {} ({})", name, device_id);
        
        Ok(device_id)
    }

    pub async fn simulate_hotplug_cycle(&mut self, device_index: usize) -> Result<()> {
        if let Some(device) = self.virtual_devices.get(device_index) {
            let mut dev = device.write().await;
            dev.disconnect();
            tokio::time::sleep(Duration::from_millis(100)).await;
            dev.reconnect();
            info!("Completed hotplug cycle for device {}", device_index);
        }
        Ok(())
    }

    pub async fn inject_fault(&mut self, device_index: usize, fault_type: u8) -> Result<()> {
        if let Some(device) = self.virtual_devices.get(device_index) {
            device.write().await.inject_fault(fault_type);
        }
        Ok(())
    }

    pub async fn collect_metrics(&mut self) -> PerformanceMetrics {
        self.metrics_collector.collect().await
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        if let Some(service) = &self.service {
            // Graceful shutdown
            info!("Shutting down test service");
        }
        self.service = None;
        Ok(())
    }
}

/// Metrics collector for performance monitoring
pub struct MetricsCollector {
    jitter_samples: Vec<f64>,
    hid_latency_samples: Vec<f64>,
    missed_ticks: u64,
    total_ticks: u64,
    start_time: Instant,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            jitter_samples: Vec::new(),
            hid_latency_samples: Vec::new(),
            missed_ticks: 0,
            total_ticks: 0,
            start_time: Instant::now(),
        }
    }

    pub fn record_jitter(&mut self, jitter_ms: f64) {
        self.jitter_samples.push(jitter_ms);
    }

    pub fn record_hid_latency(&mut self, latency_us: f64) {
        self.hid_latency_samples.push(latency_us);
    }

    pub fn record_tick(&mut self, missed: bool) {
        self.total_ticks += 1;
        if missed {
            self.missed_ticks += 1;
        }
    }

    pub async fn collect(&self) -> PerformanceMetrics {
        let mut metrics = PerformanceMetrics::default();
        
        metrics.total_ticks = self.total_ticks;
        metrics.missed_ticks = self.missed_ticks;

        if !self.jitter_samples.is_empty() {
            let mut sorted_jitter = self.jitter_samples.clone();
            sorted_jitter.sort_by(|a, b| a.partial_cmp(b).unwrap());
            
            metrics.jitter_p50_ms = percentile(&sorted_jitter, 0.5);
            metrics.jitter_p99_ms = percentile(&sorted_jitter, 0.99);
        }

        if !self.hid_latency_samples.is_empty() {
            let mut sorted_latency = self.hid_latency_samples.clone();
            sorted_latency.sort_by(|a, b| a.partial_cmp(b).unwrap());
            
            metrics.hid_latency_p50_us = percentile(&sorted_latency, 0.5);
            metrics.hid_latency_p99_us = percentile(&sorted_latency, 0.99);
        }

        // Collect system metrics
        let system = sysinfo::System::new_all();
        if let Some(process) = system.processes().values().next() {
            metrics.cpu_usage_percent = process.cpu_usage() as f64;
            metrics.memory_usage_mb = process.memory() as f64 / 1024.0 / 1024.0;
        }

        metrics
    }

    pub fn reset(&mut self) {
        self.jitter_samples.clear();
        self.hid_latency_samples.clear();
        self.missed_ticks = 0;
        self.total_ticks = 0;
        self.start_time = Instant::now();
    }
}

/// Calculate percentile from sorted samples
fn percentile(sorted_samples: &[f64], p: f64) -> f64 {
    if sorted_samples.is_empty() {
        return 0.0;
    }
    
    let index = (p * (sorted_samples.len() - 1) as f64).round() as usize;
    sorted_samples[index.min(sorted_samples.len() - 1)]
}

/// Timing utilities for precise measurements
pub struct TimingUtils;

impl TimingUtils {
    /// Measure execution time of a closure
    pub async fn measure_async<F, Fut, T>(f: F) -> (T, Duration)
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        let start = Instant::now();
        let result = f().await;
        let duration = start.elapsed();
        (result, duration)
    }

    /// Create a precise timer for RT measurements
    pub fn create_rt_timer(frequency_hz: u32) -> RTTimer {
        RTTimer::new(frequency_hz)
    }
}

/// Real-time timer for precise scheduling
pub struct RTTimer {
    period: Duration,
    next_tick: Instant,
}

impl RTTimer {
    pub fn new(frequency_hz: u32) -> Self {
        let period = Duration::from_nanos(1_000_000_000 / frequency_hz as u64);
        Self {
            period,
            next_tick: Instant::now() + period,
        }
    }

    pub async fn wait_for_next_tick(&mut self) -> Duration {
        let now = Instant::now();
        let jitter = if now > self.next_tick {
            now - self.next_tick
        } else {
            Duration::ZERO
        };

        if now < self.next_tick {
            tokio::time::sleep_until(self.next_tick.into()).await;
        }

        self.next_tick += self.period;
        jitter
    }
}