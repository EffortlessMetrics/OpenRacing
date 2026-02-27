//! Performance metrics collection for the racing wheel engine
//!
//! This module provides comprehensive metrics collection for observability:
//! - Real-time performance metrics (latency, jitter, CPU usage)
//! - Counters for missed ticks, torque saturation, telemetry packet loss
//! - Health event streaming for real-time monitoring
//! - Prometheus export support
//!
//! # Re-exports
//!
//! This module re-exports RT-safe types from [`openracing_atomic`] for convenience:
//! - [`AtomicCounters`] - RT-safe atomic counters
//! - [`JitterStats`] - Jitter statistics
//! - [`LatencyStats`] - Latency statistics
//!
//! See the [`openracing_atomic`] crate for full documentation.

use parking_lot::Mutex;
use prometheus::{Gauge, Histogram, IntCounter, IntGauge, Registry};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

pub use openracing_atomic::{
    AppMetricsSnapshot, AppThresholds, AtomicCounters, CounterSnapshot, JitterStats, LatencyStats,
    RTMetricsSnapshot, RTThresholds, StreamingStats,
};

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

impl From<RTMetrics> for RTMetricsSnapshot {
    fn from(metrics: RTMetrics) -> Self {
        RTMetricsSnapshot {
            total_ticks: metrics.total_ticks,
            missed_ticks: metrics.missed_ticks,
            jitter: metrics.jitter_ns,
            hid_latency: metrics.hid_latency_us,
            processing_time: metrics.processing_time_us,
            cpu_usage_percent: metrics.cpu_usage_percent,
            memory_usage_bytes: metrics.memory_usage_bytes,
        }
    }
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

impl From<AppMetrics> for AppMetricsSnapshot {
    fn from(metrics: AppMetrics) -> Self {
        AppMetricsSnapshot {
            connected_devices: metrics.connected_devices,
            torque_saturation_percent: metrics.torque_saturation_percent,
            telemetry_packet_loss_percent: metrics.telemetry_packet_loss_percent,
            safety_events: metrics.safety_events,
            profile_switches: metrics.profile_switches,
        }
    }
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

    // Device metrics
    pub hid_write_errors_total: IntCounter,

    // Health metrics
    pub health_events_total: IntCounter,
}

impl PrometheusMetrics {
    /// Create new Prometheus metrics registry
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Registry::new();

        // RT performance metrics
        let rt_ticks_total =
            IntCounter::new("wheel_rt_ticks_total", "Total number of RT ticks processed")?;

        let rt_missed_ticks_total = IntCounter::new(
            "wheel_rt_missed_ticks_total",
            "Total number of missed RT ticks (deadline violations)",
        )?;

        let rt_jitter_histogram = Histogram::with_opts(
            prometheus::HistogramOpts::new("wheel_rt_jitter_seconds", "RT tick jitter in seconds")
                .buckets(vec![
                    0.000_000_050, // 50ns
                    0.000_000_100, // 100ns
                    0.000_000_250, // 250ns (target p99)
                    0.000_000_500, // 500ns
                    0.000_001_000, // 1μs
                    0.000_002_000, // 2μs
                    0.000_005_000, // 5μs
                    0.000_010_000, // 10μs
                ]),
        )?;

        let rt_processing_time_histogram = Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "wheel_rt_processing_time_seconds",
                "RT processing time per tick in seconds",
            )
            .buckets(vec![
                0.000_000_010, // 10μs
                0.000_000_050, // 50μs (target median)
                0.000_000_100, // 100μs
                0.000_000_200, // 200μs (target p99)
                0.000_000_500, // 500μs
                0.000_001_000, // 1ms
            ]),
        )?;

        let hid_write_latency_histogram = Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "wheel_hid_write_latency_seconds",
                "HID write latency in seconds",
            )
            .buckets(vec![
                0.000_000_100, // 100μs
                0.000_000_300, // 300μs (target p99)
                0.000_000_500, // 500μs
                0.000_001_000, // 1ms
                0.000_002_000, // 2ms
                0.000_005_000, // 5ms
            ]),
        )?;

        // System metrics
        let cpu_usage_gauge = Gauge::new("wheel_cpu_usage_percent", "CPU usage percentage")?;

        let memory_usage_gauge =
            IntGauge::new("wheel_memory_usage_bytes", "Memory usage in bytes")?;

        // Application metrics
        let connected_devices_gauge =
            IntGauge::new("wheel_connected_devices", "Number of connected devices")?;

        let torque_saturation_gauge = Gauge::new(
            "wheel_torque_saturation_percent",
            "Torque saturation percentage",
        )?;

        let telemetry_packet_loss_gauge = Gauge::new(
            "wheel_telemetry_packet_loss_percent",
            "Telemetry packet loss percentage",
        )?;

        let safety_events_total = IntCounter::new(
            "wheel_safety_events_total",
            "Total number of safety events triggered",
        )?;

        let profile_switches_total = IntCounter::new(
            "wheel_profile_switches_total",
            "Total number of profile switches",
        )?;

        let hid_write_errors_total = IntCounter::new(
            "wheel_hid_write_errors_total",
            "Total number of HID write errors",
        )?;

        let health_events_total = IntCounter::new(
            "wheel_health_events_total",
            "Total number of health events emitted",
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
        registry.register(Box::new(hid_write_errors_total.clone()))?;
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
            hid_write_errors_total,
            health_events_total,
        })
    }

    /// Update RT performance metrics
    pub fn update_rt_metrics(&self, metrics: &RTMetrics) {
        self.rt_ticks_total.inc_by(metrics.total_ticks);
        self.rt_missed_ticks_total.inc_by(metrics.missed_ticks);

        // Convert nanoseconds to seconds for Prometheus
        self.rt_jitter_histogram
            .observe(metrics.jitter_ns.p99_ns as f64 / 1_000_000_000.0);
        self.rt_processing_time_histogram
            .observe(metrics.processing_time_us.p99_us as f64 / 1_000_000.0);
        self.hid_write_latency_histogram
            .observe(metrics.hid_latency_us.p99_us as f64 / 1_000_000.0);

        self.cpu_usage_gauge.set(metrics.cpu_usage_percent as f64);
        self.memory_usage_gauge
            .set(metrics.memory_usage_bytes as i64);
    }

    /// Update application metrics
    pub fn update_app_metrics(&self, metrics: &AppMetrics) {
        self.connected_devices_gauge
            .set(metrics.connected_devices as i64);
        self.torque_saturation_gauge
            .set(metrics.torque_saturation_percent as f64);
        self.telemetry_packet_loss_gauge
            .set(metrics.telemetry_packet_loss_percent as f64);
        self.safety_events_total.inc_by(metrics.safety_events);
        self.profile_switches_total.inc_by(metrics.profile_switches);
    }

    /// Update device metrics
    pub fn update_device_metrics(&self, hid_write_errors: u64) {
        self.hid_write_errors_total.inc_by(hid_write_errors);
    }

    /// Record a health event
    pub fn record_health_event(&self) {
        self.health_events_total.inc();
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
    #[allow(clippy::result_large_err)]
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

/// RT-safe sample queues for histogram recording (internal wrapper)
///
/// Uses lock-free bounded queues to allow the RT path to push samples
/// without blocking. Samples are drained by the collector and recorded
/// into hdrhistogram for percentile calculation.
pub(crate) struct InternalSampleQueues {
    /// Jitter samples in nanoseconds
    jitter_ns: crossbeam::queue::ArrayQueue<u64>,
    /// Processing time samples in nanoseconds
    processing_time_ns: crossbeam::queue::ArrayQueue<u64>,
    /// HID latency samples in nanoseconds
    hid_latency_ns: crossbeam::queue::ArrayQueue<u64>,
}

impl Default for InternalSampleQueues {
    fn default() -> Self {
        Self::new()
    }
}

impl InternalSampleQueues {
    /// Create new sample queues with default capacity
    pub fn new() -> Self {
        Self {
            jitter_ns: crossbeam::queue::ArrayQueue::new(10000),
            processing_time_ns: crossbeam::queue::ArrayQueue::new(10000),
            hid_latency_ns: crossbeam::queue::ArrayQueue::new(10000),
        }
    }

    /// Push a jitter sample (RT-safe, drops on overflow)
    #[inline]
    #[allow(dead_code)]
    pub fn push_jitter(&self, ns: u64) {
        let _ = self.jitter_ns.push(ns);
    }

    /// Push a processing time sample (RT-safe, drops on overflow)
    #[inline]
    #[allow(dead_code)]
    pub fn push_processing_time(&self, ns: u64) {
        let _ = self.processing_time_ns.push(ns);
    }

    /// Push a HID latency sample (RT-safe, drops on overflow)
    #[inline]
    #[allow(dead_code)]
    pub fn push_hid_latency(&self, ns: u64) {
        let _ = self.hid_latency_ns.push(ns);
    }

    /// Pop a jitter sample
    #[inline]
    pub fn pop_jitter(&self) -> Option<u64> {
        self.jitter_ns.pop()
    }

    /// Pop a processing time sample
    #[inline]
    pub fn pop_processing_time(&self) -> Option<u64> {
        self.processing_time_ns.pop()
    }

    /// Pop a HID latency sample
    #[inline]
    pub fn pop_hid_latency(&self) -> Option<u64> {
        self.hid_latency_ns.pop()
    }
}

/// Metrics collector that aggregates data from various sources
pub struct MetricsCollector {
    prometheus_metrics: Arc<PrometheusMetrics>,
    atomic_counters: Arc<AtomicCounters>,
    health_streamer: Arc<HealthEventStreamer>,
    system_monitor: SystemMonitor,
    last_collection: Instant,
    /// RT-safe sample queues for histogram data
    sample_queues: Arc<InternalSampleQueues>,
    /// HDR histograms for percentile calculations (non-RT access only)
    jitter_histogram: Mutex<hdrhistogram::Histogram<u64>>,
    processing_histogram: Mutex<hdrhistogram::Histogram<u64>>,
    latency_histogram: Mutex<hdrhistogram::Histogram<u64>>,
}

impl MetricsCollector {
    /// Create new metrics collector
    pub fn new() -> Result<Self, prometheus::Error> {
        let prometheus_metrics = Arc::new(PrometheusMetrics::new()?);
        let atomic_counters = Arc::new(AtomicCounters::new());
        let health_streamer = Arc::new(HealthEventStreamer::new(1000)); // Buffer 1000 events
        let system_monitor = SystemMonitor::new();
        let sample_queues = Arc::new(InternalSampleQueues::new());

        // Initialize HDR histograms (1ns to 1s range, 3 significant figures)
        // These provide accurate percentile calculations
        let jitter_histogram = Mutex::new(
            hdrhistogram::Histogram::<u64>::new_with_bounds(1, 1_000_000_000, 3).map_err(|e| {
                prometheus::Error::Msg(format!("jitter histogram init failed: {e}"))
            })?,
        );
        let processing_histogram = Mutex::new(
            hdrhistogram::Histogram::<u64>::new_with_bounds(1, 1_000_000_000, 3).map_err(|e| {
                prometheus::Error::Msg(format!("processing histogram init failed: {e}"))
            })?,
        );
        let latency_histogram = Mutex::new(
            hdrhistogram::Histogram::<u64>::new_with_bounds(1, 1_000_000_000, 3).map_err(|e| {
                prometheus::Error::Msg(format!("latency histogram init failed: {e}"))
            })?,
        );

        Ok(Self {
            prometheus_metrics,
            atomic_counters,
            health_streamer,
            system_monitor,
            last_collection: Instant::now(),
            sample_queues,
            jitter_histogram,
            processing_histogram,
            latency_histogram,
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

    /// Get sample queues for RT use.
    #[allow(dead_code)]
    pub(crate) fn sample_queues(&self) -> Arc<InternalSampleQueues> {
        self.sample_queues.clone()
    }

    /// Get health event streamer
    pub fn health_streamer(&self) -> Arc<HealthEventStreamer> {
        self.health_streamer.clone()
    }

    /// Drain samples from RT queues into histograms
    fn drain_samples(&self) {
        // Drain jitter samples
        {
            let mut hist = self.jitter_histogram.lock();
            while let Some(sample) = self.sample_queues.pop_jitter() {
                let _ = hist.record(sample);
            }
        }

        // Drain processing time samples
        {
            let mut hist = self.processing_histogram.lock();
            while let Some(sample) = self.sample_queues.pop_processing_time() {
                let _ = hist.record(sample);
            }
        }

        // Drain HID latency samples
        {
            let mut hist = self.latency_histogram.lock();
            while let Some(sample) = self.sample_queues.pop_hid_latency() {
                let _ = hist.record(sample);
            }
        }
    }

    /// Get jitter statistics from histogram
    fn get_jitter_stats(&self) -> JitterStats {
        let hist = self.jitter_histogram.lock();
        JitterStats::from_values(
            hist.value_at_quantile(0.5),
            hist.value_at_quantile(0.99),
            hist.max(),
        )
    }

    /// Get processing time statistics from histogram (in microseconds)
    fn get_processing_stats(&self) -> LatencyStats {
        let hist = self.processing_histogram.lock();
        LatencyStats::from_values(
            hist.value_at_quantile(0.5) / 1000,
            hist.value_at_quantile(0.99) / 1000,
            hist.max() / 1000,
        )
    }

    /// Get HID latency statistics from histogram (in microseconds)
    fn get_latency_stats(&self) -> LatencyStats {
        let hist = self.latency_histogram.lock();
        LatencyStats::from_values(
            hist.value_at_quantile(0.5) / 1000,
            hist.value_at_quantile(0.99) / 1000,
            hist.max() / 1000,
        )
    }

    /// Collect and update all metrics
    pub async fn collect_metrics(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let now = Instant::now();
        let collection_interval = now.duration_since(self.last_collection);

        // Get atomic counter values
        let snapshot = self.atomic_counters.snapshot_and_reset();

        // Calculate derived metrics
        let telemetry_loss_percent = snapshot.telemetry_loss_percent();
        let torque_saturation_percent = snapshot.torque_saturation_percent();

        // Get system metrics
        let (cpu_usage, memory_usage) = self.system_monitor.get_system_metrics().await;

        // Drain RT samples into histograms and compute statistics
        self.drain_samples();
        let jitter_stats = self.get_jitter_stats();
        let processing_stats = self.get_processing_stats();
        let latency_stats = self.get_latency_stats();

        // Create RT metrics
        let rt_metrics = RTMetrics {
            total_ticks: snapshot.total_ticks,
            missed_ticks: snapshot.missed_ticks,
            jitter_ns: jitter_stats,
            hid_latency_us: latency_stats,
            processing_time_us: processing_stats,
            cpu_usage_percent: cpu_usage,
            memory_usage_bytes: memory_usage,
            last_update: now,
        };

        // Create app metrics
        let app_metrics = AppMetrics {
            connected_devices: 0, // TODO: Get from device manager
            torque_saturation_percent,
            telemetry_packet_loss_percent: telemetry_loss_percent,
            safety_events: snapshot.safety_events,
            profile_switches: snapshot.profile_switches,
            active_game: None, // TODO: Get from game service
            last_update: now,
        };

        // Update Prometheus metrics
        self.prometheus_metrics.update_rt_metrics(&rt_metrics);
        self.prometheus_metrics.update_app_metrics(&app_metrics);
        self.prometheus_metrics
            .update_device_metrics(snapshot.hid_write_errors);

        // Emit health events for critical conditions
        if snapshot.missed_ticks > 0 {
            let event = HealthEventStreamer::create_event(
                HealthEventType::PerformanceDegradation,
                HealthSeverity::Warning,
                format!(
                    "Missed {} RT ticks in {}ms",
                    snapshot.missed_ticks,
                    collection_interval.as_millis()
                ),
                None,
                serde_json::json!({
                    "missed_ticks": snapshot.missed_ticks,
                    "total_ticks": snapshot.total_ticks,
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
                    "samples": snapshot.torque_saturation_samples,
                    "saturated": snapshot.torque_saturation_count
                }),
            );

            if let Err(e) = self.health_streamer.emit(event) {
                tracing::warn!("Failed to emit health event: {}", e);
            } else {
                self.prometheus_metrics.record_health_event();
            }
        }

        if snapshot.hid_write_errors > 0 {
            let event = HealthEventStreamer::create_event(
                HealthEventType::Error,
                HealthSeverity::Warning,
                format!(
                    "HID write errors: {} in {}ms",
                    snapshot.hid_write_errors,
                    collection_interval.as_millis()
                ),
                None,
                serde_json::json!({
                    "hid_write_errors": snapshot.hid_write_errors,
                    "collection_interval_ms": collection_interval.as_millis()
                }),
            );

            if let Err(e) = self.health_streamer.emit(event) {
                tracing::warn!("Failed to emit HID write error health event: {}", e);
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

impl Default for SystemMonitor {
    fn default() -> Self {
        Self::new()
    }
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
        self.system.refresh_cpu_all();
        self.system
            .refresh_processes(sysinfo::ProcessesToUpdate::Some(&[self.process_pid]), true);

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
            max_jitter_ns: 250_000,                    // 250μs (requirement: ≤0.25ms)
            max_processing_time_us: 200,               // 200μs (requirement: ≤200μs p99)
            max_hid_latency_us: 300,                   // 300μs (requirement: ≤300μs p99)
            max_cpu_usage_percent: 3.0,                // 3% (requirement: <3% of one core)
            max_memory_usage_bytes: 150 * 1024 * 1024, // 150MB (requirement: <150MB RSS)
            max_missed_tick_rate: 0.001,               // 0.1% (requirement: <0.001% missed ticks)
            max_torque_saturation_percent: 95.0,       // 95%
            max_telemetry_loss_percent: 5.0,           // 5%
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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::time::Duration;
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

        // Test HID write error counting
        counters.inc_hid_write_error();
        counters.inc_hid_write_error();
        counters.inc_hid_write_error();

        let snapshot = counters.snapshot_and_reset();

        assert_eq!(snapshot.total_ticks, 2);
        assert_eq!(snapshot.missed_ticks, 1);
        assert_eq!(snapshot.torque_saturation_samples, 3);
        assert_eq!(snapshot.torque_saturation_count, 2);
        assert_eq!(snapshot.hid_write_errors, 3);

        // Verify reset
        let after = counters.snapshot();
        assert_eq!(after.total_ticks, 0);
        assert_eq!(after.hid_write_errors, 0);
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
        let event = tokio::time::timeout(Duration::from_millis(100), stream.next()).await;

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
            jitter_ns: JitterStats::from_values(100_000, 300_000, 500_000),
            hid_latency_us: LatencyStats::from_values(100, 200, 400),
            processing_time_us: LatencyStats::from_values(50, 150, 300),
            cpu_usage_percent: 2.5,
            memory_usage_bytes: 100 * 1024 * 1024,
            last_update: Instant::now(),
        };

        let violations = validator.validate_rt_metrics(&rt_metrics);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("Jitter p99"));

        // Test app metrics validation
        let app_metrics = AppMetrics {
            connected_devices: 2,
            torque_saturation_percent: 96.0,
            telemetry_packet_loss_percent: 3.0,
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

    #[test]
    fn test_jitter_stats_from_openracing_atomic() {
        let stats = JitterStats::from_values(100, 200, 500);
        assert_eq!(stats.p50_ns, 100);
        assert_eq!(stats.p99_ns, 200);
        assert_eq!(stats.max_ns, 500);
    }

    #[test]
    fn test_latency_stats_from_openracing_atomic() {
        let stats = LatencyStats::from_values(50, 150, 300);
        assert_eq!(stats.p50_us, 50);
        assert_eq!(stats.p99_us, 150);
        assert_eq!(stats.max_us, 300);
    }

    #[test]
    fn test_rt_metrics_to_snapshot() {
        let rt_metrics = RTMetrics {
            total_ticks: 1000,
            missed_ticks: 5,
            jitter_ns: JitterStats::from_values(100, 200, 500),
            hid_latency_us: LatencyStats::from_values(50, 100, 200),
            processing_time_us: LatencyStats::from_values(30, 80, 150),
            cpu_usage_percent: 2.5,
            memory_usage_bytes: 50_000_000,
            last_update: Instant::now(),
        };

        let snapshot: RTMetricsSnapshot = rt_metrics.into();
        assert_eq!(snapshot.total_ticks, 1000);
        assert_eq!(snapshot.missed_ticks, 5);
    }

    #[test]
    fn test_app_metrics_to_snapshot() {
        let app_metrics = AppMetrics {
            connected_devices: 2,
            torque_saturation_percent: 25.0,
            telemetry_packet_loss_percent: 1.0,
            safety_events: 5,
            profile_switches: 10,
            active_game: None,
            last_update: Instant::now(),
        };

        let snapshot: AppMetricsSnapshot = app_metrics.into();
        assert_eq!(snapshot.connected_devices, 2);
        assert_eq!(snapshot.torque_saturation_percent, 25.0);
    }
}
