//! Performance metrics collection for the racing wheel engine
//!
//! This module provides comprehensive metrics collection for observability:
//! - Real-time performance metrics (latency, jitter, CPU usage)
//! - Counters for missed ticks, torque saturation, telemetry packet loss
//! - Health event streaming for real-time monitoring
//! - Prometheus export support

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use prometheus::{Gauge, Histogram, IntCounter, IntGauge, Registry};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
// Removed unused tokio_stream::Stream import

/// Real-time performance metrics
#[derive(Debug, Clone)]
pub struct RTMetrics {
    /// Total number of RT ticks processed
    pub total_ticks: u64,
    /// Number of missed ticks (deadline violations)
    pub missed_ticks: u64,
    /// Current tick jitter in nanoseconds (p50, p99)
    pub jitter_ns: JitterStats,
    /// HID write latency in microseconds (p50, p99)
    pub hid_latency_us: LatencyStats,
    /// Engine processing time in microseconds (p50, p99)
    pub processing_time_us: LatencyStats,
    /// Current CPU usage percentage
    pub cpu_usage_percent: f32,
    /// Memory usage in bytes
    pub memory_usage_bytes: u64,
    /// Last update timestamp
    pub last_update: Instant,
}

/// Jitter statistics
#[derive(Debug, Clone)]
pub struct JitterStats {
    pub p50_ns: u64,
    pub p99_ns: u64,
    pub max_ns: u64,
}

/// Latency statistics
#[derive(Debug, Clone)]
pub struct LatencyStats {
    pub p50_us: u64,
    pub p99_us: u64,
    pub max_us: u64,
}

/// Application-level metrics
#[derive(Debug, Clone)]
pub struct AppMetrics {
    /// Number of connected devices
    pub connected_devices: u32,
    /// Torque saturation percentage (0-100)
    pub torque_saturation_percent: f32,
    /// Telemetry packet loss percentage
    pub telemetry_packet_loss_percent: f32,
    /// Number of safety events triggered
    pub safety_events: u64,
    /// Number of profile switches
    pub profile_switches: u64,
    /// Active game ID
    pub active_game: Option<String>,
    /// Last update timestamp
    pub last_update: Instant,
}

/// Health event for streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthEvent {
    /// Event timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Event type
    pub event_type: HealthEventType,
    /// Device ID (if applicable)
    pub device_id: Option<String>,
    /// Event severity
    pub severity: HealthSeverity,
    /// Event message
    pub message: String,
    /// Additional context data
    pub context: serde_json::Value,
}

/// Health event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthEventType {
    /// RT performance degradation
    PerformanceDegradation,
    /// Device connection/disconnection
    DeviceStatus,
    /// Safety event triggered
    SafetyEvent,
    /// Telemetry status change
    TelemetryStatus,
    /// Profile change
    ProfileChange,
    /// System resource warning
    ResourceWarning,
    /// Error condition
    Error,
}

/// Health event severity levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Prometheus metrics registry
pub struct PrometheusMetrics {
    /// Registry for all metrics
    pub registry: Registry,
    
    // RT performance metrics
    pub rt_ticks_total: IntCounter,
    pub rt_missed_ticks_total: IntCounter,
    pub rt_jitter_histogram: Histogram,
    pub rt_processing_time_histogram: Histogram,
    pub hid_write_latency_histogram: Histogram,
    
    // System metrics
    pub cpu_usage_gauge: Gauge,
    pub memory_usage_gauge: IntGauge,
    
    // Application metrics
    pub connected_devices_gauge: IntGauge,
    pub torque_saturation_gauge: Gauge,
    pub telemetry_packet_loss_gauge: Gauge,
    pub safety_events_total: IntCounter,
    pub profile_switches_total: IntCounter,
    
    // Health metrics
    pub health_events_total: IntCounter,
}

impl PrometheusMetrics {
    /// Create new Prometheus metrics registry
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Registry::new();
        
        // RT performance metrics
        let rt_ticks_total = IntCounter::new(
            "wheel_rt_ticks_total",
            "Total number of RT ticks processed"
        )?;
        
        let rt_missed_ticks_total = IntCounter::new(
            "wheel_rt_missed_ticks_total", 
            "Total number of missed RT ticks (deadline violations)"
        )?;
        
        let rt_jitter_histogram = Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "wheel_rt_jitter_seconds",
                "RT tick jitter in seconds"
            ).buckets(vec![
                0.000_000_050, // 50ns
                0.000_000_100, // 100ns
                0.000_000_250, // 250ns (target p99)
                0.000_000_500, // 500ns
                0.000_001_000, // 1μs
                0.000_002_000, // 2μs
                0.000_005_000, // 5μs
                0.000_010_000, // 10μs
            ])
        )?;
        
        let rt_processing_time_histogram = Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "wheel_rt_processing_time_seconds",
                "RT processing time per tick in seconds"
            ).buckets(vec![
                0.000_000_010, // 10μs
                0.000_000_050, // 50μs (target median)
                0.000_000_100, // 100μs
                0.000_000_200, // 200μs (target p99)
                0.000_000_500, // 500μs
                0.000_001_000, // 1ms
            ])
        )?;
        
        let hid_write_latency_histogram = Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "wheel_hid_write_latency_seconds",
                "HID write latency in seconds"
            ).buckets(vec![
                0.000_000_100, // 100μs
                0.000_000_300, // 300μs (target p99)
                0.000_000_500, // 500μs
                0.000_001_000, // 1ms
                0.000_002_000, // 2ms
                0.000_005_000, // 5ms
            ])
        )?;
        
        // System metrics
        let cpu_usage_gauge = Gauge::new(
            "wheel_cpu_usage_percent",
            "CPU usage percentage"
        )?;
        
        let memory_usage_gauge = IntGauge::new(
            "wheel_memory_usage_bytes",
            "Memory usage in bytes"
        )?;
        
        // Application metrics
        let connected_devices_gauge = IntGauge::new(
            "wheel_connected_devices",
            "Number of connected devices"
        )?;
        
        let torque_saturation_gauge = Gauge::new(
            "wheel_torque_saturation_percent",
            "Torque saturation percentage"
        )?;
        
        let telemetry_packet_loss_gauge = Gauge::new(
            "wheel_telemetry_packet_loss_percent",
            "Telemetry packet loss percentage"
        )?;
        
        let safety_events_total = IntCounter::new(
            "wheel_safety_events_total",
            "Total number of safety events triggered"
        )?;
        
        let profile_switches_total = IntCounter::new(
            "wheel_profile_switches_total",
            "Total number of profile switches"
        )?;
        
        let health_events_total = IntCounter::new(
            "wheel_health_events_total",
            "Total number of health events emitted"
        )?;
        
        // Register all metrics
        registry.register(Box::new(rt_ticks_total.clone()))?;
        registry.register(Box::new(rt_missed_ticks_total.clone()))?;
        registry.register(Box::new(rt_jitter_histogram.clone()))?;
        registry.register(Box::new(rt_processing_time_histogram.clone()))?;
        registry.register(Box::new(hid_write_latency_histogram.clone()))?;
        registry.register(Box::new(cpu_usage_gauge.clone()))?;
        registry.register(Box::new(memory_usage_gauge.clone()))?;
        registry.register(Box::new(connected_devices_gauge.clone()))?;
        registry.register(Box::new(torque_saturation_gauge.clone()))?;
        registry.register(Box::new(telemetry_packet_loss_gauge.clone()))?;
        registry.register(Box::new(safety_events_total.clone()))?;
        registry.register(Box::new(profile_switches_total.clone()))?;
        registry.register(Box::new(health_events_total.clone()))?;
        
        Ok(Self {
            registry,
            rt_ticks_total,
            rt_missed_ticks_total,
            rt_jitter_histogram,
            rt_processing_time_histogram,
            hid_write_latency_histogram,
            cpu_usage_gauge,
            memory_usage_gauge,
            connected_devices_gauge,
            torque_saturation_gauge,
            telemetry_packet_loss_gauge,
            safety_events_total,
            profile_switches_total,
            health_events_total,
        })
    }
    
    /// Update RT performance metrics
    pub fn update_rt_metrics(&self, metrics: &RTMetrics) {
        self.rt_ticks_total.inc_by(metrics.total_ticks);
        self.rt_missed_ticks_total.inc_by(metrics.missed_ticks);
        
        // Convert nanoseconds to seconds for Prometheus
        self.rt_jitter_histogram.observe(metrics.jitter_ns.p99_ns as f64 / 1_000_000_000.0);
        self.rt_processing_time_histogram.observe(metrics.processing_time_us.p99_us as f64 / 1_000_000.0);
        self.hid_write_latency_histogram.observe(metrics.hid_latency_us.p99_us as f64 / 1_000_000.0);
        
        self.cpu_usage_gauge.set(metrics.cpu_usage_percent as f64);
        self.memory_usage_gauge.set(metrics.memory_usage_bytes as i64);
    }
    
    /// Update application metrics
    pub fn update_app_metrics(&self, metrics: &AppMetrics) {
        self.connected_devices_gauge.set(metrics.connected_devices as i64);
        self.torque_saturation_gauge.set(metrics.torque_saturation_percent as f64);
        self.telemetry_packet_loss_gauge.set(metrics.telemetry_packet_loss_percent as f64);
        self.safety_events_total.inc_by(metrics.safety_events);
        self.profile_switches_total.inc_by(metrics.profile_switches);
    }
    
    /// Record a health event
    pub fn record_health_event(&self) {
        self.health_events_total.inc();
    }
}

/// Atomic counters for RT-safe metrics collection
pub struct AtomicCounters {
    pub total_ticks: AtomicU64,
    pub missed_ticks: AtomicU64,
    pub safety_events: AtomicU64,
    pub profile_switches: AtomicU64,
    pub telemetry_packets_received: AtomicU64,
    pub telemetry_packets_lost: AtomicU64,
    pub torque_saturation_samples: AtomicU64,
    pub torque_saturation_count: AtomicU64,
}

impl AtomicCounters {
    pub fn new() -> Self {
        Self {
            total_ticks: AtomicU64::new(0),
            missed_ticks: AtomicU64::new(0),
            safety_events: AtomicU64::new(0),
            profile_switches: AtomicU64::new(0),
            telemetry_packets_received: AtomicU64::new(0),
            telemetry_packets_lost: AtomicU64::new(0),
            torque_saturation_samples: AtomicU64::new(0),
            torque_saturation_count: AtomicU64::new(0),
        }
    }
    
    /// Increment tick counter (RT-safe)
    #[inline]
    pub fn inc_tick(&self) {
        self.total_ticks.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Increment missed tick counter (RT-safe)
    #[inline]
    pub fn inc_missed_tick(&self) {
        self.missed_ticks.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record torque saturation sample (RT-safe)
    #[inline]
    pub fn record_torque_saturation(&self, is_saturated: bool) {
        self.torque_saturation_samples.fetch_add(1, Ordering::Relaxed);
        if is_saturated {
            self.torque_saturation_count.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    /// Increment safety event counter
    pub fn inc_safety_event(&self) {
        self.safety_events.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Increment profile switch counter
    pub fn inc_profile_switch(&self) {
        self.profile_switches.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record telemetry packet received
    pub fn inc_telemetry_received(&self) {
        self.telemetry_packets_received.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record telemetry packet lost
    pub fn inc_telemetry_lost(&self) {
        self.telemetry_packets_lost.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Get current values and reset counters
    pub fn get_and_reset(&self) -> (u64, u64, u64, u64, u64, u64, u64, u64) {
        (
            self.total_ticks.swap(0, Ordering::Relaxed),
            self.missed_ticks.swap(0, Ordering::Relaxed),
            self.safety_events.swap(0, Ordering::Relaxed),
            self.profile_switches.swap(0, Ordering::Relaxed),
            self.telemetry_packets_received.swap(0, Ordering::Relaxed),
            self.telemetry_packets_lost.swap(0, Ordering::Relaxed),
            self.torque_saturation_samples.swap(0, Ordering::Relaxed),
            self.torque_saturation_count.swap(0, Ordering::Relaxed),
        )
    }
}

/// Health event streaming service
pub struct HealthEventStreamer {
    sender: broadcast::Sender<HealthEvent>,
    _receiver: broadcast::Receiver<HealthEvent>,
}

impl HealthEventStreamer {
    /// Create new health event streamer
    pub fn new(buffer_size: usize) -> Self {
        let (sender, receiver) = broadcast::channel(buffer_size);
        Self {
            sender,
            _receiver: receiver,
        }
    }
    
    /// Emit a health event
    pub fn emit(&self, event: HealthEvent) -> Result<(), broadcast::error::SendError<HealthEvent>> {
        self.sender.send(event).map(|_| ())
    }
    
    /// Subscribe to health events
    pub fn subscribe(&self) -> BroadcastStream<HealthEvent> {
        BroadcastStream::new(self.sender.subscribe())
    }
    
    /// Create a health event
    pub fn create_event(
        event_type: HealthEventType,
        severity: HealthSeverity,
        message: String,
        device_id: Option<String>,
        context: serde_json::Value,
    ) -> HealthEvent {
        HealthEvent {
            timestamp: chrono::Utc::now(),
            event_type,
            device_id,
            severity,
            message,
            context,
        }
    }
}

/// Metrics collector that aggregates data from various sources
pub struct MetricsCollector {
    prometheus_metrics: Arc<PrometheusMetrics>,
    atomic_counters: Arc<AtomicCounters>,
    health_streamer: Arc<HealthEventStreamer>,
    system_monitor: SystemMonitor,
    last_collection: Instant,
}

impl MetricsCollector {
    /// Create new metrics collector
    pub fn new() -> Result<Self, prometheus::Error> {
        let prometheus_metrics = Arc::new(PrometheusMetrics::new()?);
        let atomic_counters = Arc::new(AtomicCounters::new());
        let health_streamer = Arc::new(HealthEventStreamer::new(1000)); // Buffer 1000 events
        let system_monitor = SystemMonitor::new();
        
        Ok(Self {
            prometheus_metrics,
            atomic_counters,
            health_streamer,
            system_monitor,
            last_collection: Instant::now(),
        })
    }
    
    /// Get Prometheus registry for export
    pub fn prometheus_registry(&self) -> &Registry {
        &self.prometheus_metrics.registry
    }
    
    /// Get atomic counters for RT use
    pub fn atomic_counters(&self) -> Arc<AtomicCounters> {
        self.atomic_counters.clone()
    }
    
    /// Get health event streamer
    pub fn health_streamer(&self) -> Arc<HealthEventStreamer> {
        self.health_streamer.clone()
    }
    
    /// Collect and update all metrics
    pub async fn collect_metrics(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let now = Instant::now();
        let collection_interval = now.duration_since(self.last_collection);
        
        // Get atomic counter values
        let (
            total_ticks,
            missed_ticks,
            safety_events,
            profile_switches,
            telemetry_received,
            telemetry_lost,
            torque_samples,
            torque_saturated,
        ) = self.atomic_counters.get_and_reset();
        
        // Calculate derived metrics
        let telemetry_loss_percent = if telemetry_received > 0 {
            (telemetry_lost as f32 / (telemetry_received + telemetry_lost) as f32) * 100.0
        } else {
            0.0
        };
        
        let torque_saturation_percent = if torque_samples > 0 {
            (torque_saturated as f32 / torque_samples as f32) * 100.0
        } else {
            0.0
        };
        
        // Get system metrics
        let (cpu_usage, memory_usage) = self.system_monitor.get_system_metrics().await;
        
        // Create RT metrics
        let rt_metrics = RTMetrics {
            total_ticks,
            missed_ticks,
            jitter_ns: JitterStats {
                p50_ns: 0, // TODO: Implement histogram tracking
                p99_ns: 0,
                max_ns: 0,
            },
            hid_latency_us: LatencyStats {
                p50_us: 0, // TODO: Implement histogram tracking
                p99_us: 0,
                max_us: 0,
            },
            processing_time_us: LatencyStats {
                p50_us: 0, // TODO: Implement histogram tracking
                p99_us: 0,
                max_us: 0,
            },
            cpu_usage_percent: cpu_usage,
            memory_usage_bytes: memory_usage,
            last_update: now,
        };
        
        // Create app metrics
        let app_metrics = AppMetrics {
            connected_devices: 0, // TODO: Get from device manager
            torque_saturation_percent,
            telemetry_packet_loss_percent: telemetry_loss_percent,
            safety_events,
            profile_switches,
            active_game: None, // TODO: Get from game service
            last_update: now,
        };
        
        // Update Prometheus metrics
        self.prometheus_metrics.update_rt_metrics(&rt_metrics);
        self.prometheus_metrics.update_app_metrics(&app_metrics);
        
        // Emit health events for critical conditions
        if missed_ticks > 0 {
            let event = HealthEventStreamer::create_event(
                HealthEventType::PerformanceDegradation,
                HealthSeverity::Warning,
                format!("Missed {} RT ticks in {}ms", missed_ticks, collection_interval.as_millis()),
                None,
                serde_json::json!({
                    "missed_ticks": missed_ticks,
                    "total_ticks": total_ticks,
                    "collection_interval_ms": collection_interval.as_millis()
                }),
            );
            
            if let Err(e) = self.health_streamer.emit(event) {
                tracing::warn!("Failed to emit health event: {}", e);
            } else {
                self.prometheus_metrics.record_health_event();
            }
        }
        
        if torque_saturation_percent > 90.0 {
            let event = HealthEventStreamer::create_event(
                HealthEventType::PerformanceDegradation,
                HealthSeverity::Warning,
                format!("High torque saturation: {:.1}%", torque_saturation_percent),
                None,
                serde_json::json!({
                    "torque_saturation_percent": torque_saturation_percent,
                    "samples": torque_samples,
                    "saturated": torque_saturated
                }),
            );
            
            if let Err(e) = self.health_streamer.emit(event) {
                tracing::warn!("Failed to emit health event: {}", e);
            } else {
                self.prometheus_metrics.record_health_event();
            }
        }
        
        self.last_collection = now;
        Ok(())
    }
}

/// System resource monitoring
pub struct SystemMonitor {
    system: sysinfo::System,
    process_pid: sysinfo::Pid,
}

impl SystemMonitor {
    pub fn new() -> Self {
        let mut system = sysinfo::System::new();
        system.refresh_all();
        
        let process_pid = sysinfo::get_current_pid().unwrap_or(sysinfo::Pid::from(0));
        
        Self {
            system,
            process_pid,
        }
    }
    
    /// Get current system metrics (CPU usage %, memory usage bytes)
    pub async fn get_system_metrics(&mut self) -> (f32, u64) {
        // Refresh system information
        self.system.refresh_cpu();
        self.system.refresh_process(self.process_pid);
        
        // Get process-specific metrics
        let cpu_usage = if let Some(process) = self.system.process(self.process_pid) {
            process.cpu_usage()
        } else {
            0.0
        };
        
        let memory_usage = if let Some(process) = self.system.process(self.process_pid) {
            process.memory()
        } else {
            0
        };
        
        (cpu_usage, memory_usage)
    }
}

/// Alerting thresholds for metrics validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertingThresholds {
    /// Maximum allowed jitter in nanoseconds (p99)
    pub max_jitter_ns: u64,
    /// Maximum allowed processing time in microseconds (p99)
    pub max_processing_time_us: u64,
    /// Maximum allowed HID write latency in microseconds (p99)
    pub max_hid_latency_us: u64,
    /// Maximum allowed CPU usage percentage
    pub max_cpu_usage_percent: f32,
    /// Maximum allowed memory usage in bytes
    pub max_memory_usage_bytes: u64,
    /// Maximum allowed missed tick rate (per second)
    pub max_missed_tick_rate: f64,
    /// Maximum allowed torque saturation percentage
    pub max_torque_saturation_percent: f32,
    /// Maximum allowed telemetry packet loss percentage
    pub max_telemetry_loss_percent: f32,
}

impl Default for AlertingThresholds {
    fn default() -> Self {
        Self {
            max_jitter_ns: 250_000,        // 250μs (requirement: ≤0.25ms)
            max_processing_time_us: 200,    // 200μs (requirement: ≤200μs p99)
            max_hid_latency_us: 300,        // 300μs (requirement: ≤300μs p99)
            max_cpu_usage_percent: 3.0,     // 3% (requirement: <3% of one core)
            max_memory_usage_bytes: 150 * 1024 * 1024, // 150MB (requirement: <150MB RSS)
            max_missed_tick_rate: 0.001,    // 0.1% (requirement: <0.001% missed ticks)
            max_torque_saturation_percent: 95.0, // 95%
            max_telemetry_loss_percent: 5.0,     // 5%
        }
    }
}

/// Metrics validator for alerting
pub struct MetricsValidator {
    thresholds: AlertingThresholds,
}

impl MetricsValidator {
    pub fn new(thresholds: AlertingThresholds) -> Self {
        Self { thresholds }
    }
    
    /// Validate RT metrics against thresholds
    pub fn validate_rt_metrics(&self, metrics: &RTMetrics) -> Vec<String> {
        let mut violations = Vec::new();
        
        if metrics.jitter_ns.p99_ns > self.thresholds.max_jitter_ns {
            violations.push(format!(
                "Jitter p99 {}ns exceeds threshold {}ns",
                metrics.jitter_ns.p99_ns, self.thresholds.max_jitter_ns
            ));
        }
        
        if metrics.processing_time_us.p99_us > self.thresholds.max_processing_time_us {
            violations.push(format!(
                "Processing time p99 {}μs exceeds threshold {}μs",
                metrics.processing_time_us.p99_us, self.thresholds.max_processing_time_us
            ));
        }
        
        if metrics.hid_latency_us.p99_us > self.thresholds.max_hid_latency_us {
            violations.push(format!(
                "HID latency p99 {}μs exceeds threshold {}μs",
                metrics.hid_latency_us.p99_us, self.thresholds.max_hid_latency_us
            ));
        }
        
        if metrics.cpu_usage_percent > self.thresholds.max_cpu_usage_percent {
            violations.push(format!(
                "CPU usage {:.1}% exceeds threshold {:.1}%",
                metrics.cpu_usage_percent, self.thresholds.max_cpu_usage_percent
            ));
        }
        
        if metrics.memory_usage_bytes > self.thresholds.max_memory_usage_bytes {
            violations.push(format!(
                "Memory usage {}MB exceeds threshold {}MB",
                metrics.memory_usage_bytes / (1024 * 1024),
                self.thresholds.max_memory_usage_bytes / (1024 * 1024)
            ));
        }
        
        violations
    }
    
    /// Validate app metrics against thresholds
    pub fn validate_app_metrics(&self, metrics: &AppMetrics) -> Vec<String> {
        let mut violations = Vec::new();
        
        if metrics.torque_saturation_percent > self.thresholds.max_torque_saturation_percent {
            violations.push(format!(
                "Torque saturation {:.1}% exceeds threshold {:.1}%",
                metrics.torque_saturation_percent, self.thresholds.max_torque_saturation_percent
            ));
        }
        
        if metrics.telemetry_packet_loss_percent > self.thresholds.max_telemetry_loss_percent {
            violations.push(format!(
                "Telemetry packet loss {:.1}% exceeds threshold {:.1}%",
                metrics.telemetry_packet_loss_percent, self.thresholds.max_telemetry_loss_percent
            ));
        }
        
        violations
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;

    #[test]
    fn test_prometheus_metrics_creation() {
        let metrics = PrometheusMetrics::new();
        assert!(metrics.is_ok());
    }

    #[test]
    fn test_atomic_counters() {
        let counters = AtomicCounters::new();
        
        // Test tick counting
        counters.inc_tick();
        counters.inc_tick();
        counters.inc_missed_tick();
        
        // Test torque saturation
        counters.record_torque_saturation(true);
        counters.record_torque_saturation(false);
        counters.record_torque_saturation(true);
        
        let (total_ticks, missed_ticks, _, _, _, _, torque_samples, torque_saturated) = 
            counters.get_and_reset();
        
        assert_eq!(total_ticks, 2);
        assert_eq!(missed_ticks, 1);
        assert_eq!(torque_samples, 3);
        assert_eq!(torque_saturated, 2);
        
        // Verify reset
        let (total_ticks, _, _, _, _, _, _, _) = counters.get_and_reset();
        assert_eq!(total_ticks, 0);
    }

    #[tokio::test]
    async fn test_health_event_streaming() {
        let streamer = HealthEventStreamer::new(10);
        let mut stream = streamer.subscribe();
        
        // Emit a test event
        let event = HealthEventStreamer::create_event(
            HealthEventType::DeviceStatus,
            HealthSeverity::Info,
            "Test event".to_string(),
            Some("test-device".to_string()),
            serde_json::json!({"test": true}),
        );
        
        streamer.emit(event.clone()).unwrap();
        
        // Receive the event
        let received = stream.next().await.unwrap().unwrap();
        assert_eq!(received.message, "Test event");
        assert_eq!(received.device_id, Some("test-device".to_string()));
    }

    #[tokio::test]
    async fn test_metrics_collector() {
        let mut collector = MetricsCollector::new().unwrap();
        
        // Simulate some activity
        let counters = collector.atomic_counters();
        counters.inc_tick();
        counters.inc_tick();
        counters.inc_missed_tick();
        counters.record_torque_saturation(true);
        
        // Collect metrics
        collector.collect_metrics().await.unwrap();
        
        // Verify health event was emitted for missed tick
        let mut stream = collector.health_streamer().subscribe();
        
        // Trigger another collection with missed ticks
        counters.inc_missed_tick();
        collector.collect_metrics().await.unwrap();
        
        // Should receive a health event
        let event = tokio::time::timeout(
            Duration::from_millis(100),
            stream.next()
        ).await;
        
        assert!(event.is_ok());
    }

    #[test]
    fn test_alerting_thresholds() {
        let thresholds = AlertingThresholds::default();
        let validator = MetricsValidator::new(thresholds);
        
        // Test RT metrics validation
        let rt_metrics = RTMetrics {
            total_ticks: 1000,
            missed_ticks: 0,
            jitter_ns: JitterStats {
                p50_ns: 100_000,
                p99_ns: 300_000, // Exceeds 250μs threshold
                max_ns: 500_000,
            },
            hid_latency_us: LatencyStats {
                p50_us: 100,
                p99_us: 200, // Within 300μs threshold
                max_us: 400,
            },
            processing_time_us: LatencyStats {
                p50_us: 50,
                p99_us: 150, // Within 200μs threshold
                max_us: 300,
            },
            cpu_usage_percent: 2.5, // Within 3% threshold
            memory_usage_bytes: 100 * 1024 * 1024, // Within 150MB threshold
            last_update: Instant::now(),
        };
        
        let violations = validator.validate_rt_metrics(&rt_metrics);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("Jitter p99"));
        
        // Test app metrics validation
        let app_metrics = AppMetrics {
            connected_devices: 2,
            torque_saturation_percent: 96.0, // Exceeds 95% threshold
            telemetry_packet_loss_percent: 3.0, // Within 5% threshold
            safety_events: 0,
            profile_switches: 5,
            active_game: Some("iracing".to_string()),
            last_update: Instant::now(),
        };
        
        let violations = validator.validate_app_metrics(&app_metrics);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("Torque saturation"));
    }

    #[tokio::test]
    async fn test_system_monitor() {
        let mut monitor = SystemMonitor::new();
        let (cpu_usage, memory_usage) = monitor.get_system_metrics().await;
        
        // Basic sanity checks
        assert!(cpu_usage >= 0.0);
        assert!(memory_usage > 0);
    }
}