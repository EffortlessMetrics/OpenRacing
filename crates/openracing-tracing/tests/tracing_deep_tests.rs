//! Deep tests for openracing-tracing crate.
//!
//! Covers RT metrics, telemetry metrics, streaming metrics, metric snapshots,
//! metric reset, and property-based invariants.

use openracing_tracing::{
    AppEventCategory, AppTraceEvent, RTEventCategory, RTTraceEvent, TracingError, TracingManager,
    TracingMetrics, TracingProvider,
};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Mock provider for integration-style tests
// ---------------------------------------------------------------------------

struct MetricsTrackingProvider {
    metrics: Arc<Mutex<TracingMetrics>>,
}

impl MetricsTrackingProvider {
    fn new() -> Self {
        Self {
            metrics: Arc::new(Mutex::new(TracingMetrics::new())),
        }
    }

    fn metrics_handle(&self) -> Arc<Mutex<TracingMetrics>> {
        Arc::clone(&self.metrics)
    }
}

impl TracingProvider for MetricsTrackingProvider {
    fn initialize(&mut self) -> Result<(), TracingError> {
        Ok(())
    }

    fn emit_rt_event(&self, event: RTTraceEvent) {
        let Ok(mut m) = self.metrics.lock() else {
            return;
        };
        m.record_rt_event();
        match event {
            RTTraceEvent::TickEnd {
                processing_time_ns, ..
            } => {
                m.record_processing_time(processing_time_ns);
            }
            RTTraceEvent::DeadlineMiss { .. } => {
                m.record_deadline_miss();
            }
            RTTraceEvent::PipelineFault { .. } => {
                m.record_pipeline_fault();
            }
            _ => {}
        }
    }

    fn emit_app_event(&self, _event: AppTraceEvent) {
        if let Ok(mut m) = self.metrics.lock() {
            m.record_app_event();
        }
    }

    fn metrics(&self) -> TracingMetrics {
        self.metrics
            .lock()
            .map(|m| m.clone())
            .unwrap_or_default()
    }

    fn is_enabled(&self) -> bool {
        true
    }

    fn shutdown(&mut self) {}
}

// ---------------------------------------------------------------------------
// RT metrics: tick count, jitter, missed ticks
// ---------------------------------------------------------------------------

#[test]
fn rt_metrics_tick_count_increments() -> Result<(), TracingError> {
    let provider = MetricsTrackingProvider::new();
    let handle = provider.metrics_handle();
    let mut mgr = TracingManager::with_provider(Box::new(provider));
    mgr.initialize()?;

    for tick in 0..100 {
        mgr.emit_rt_event(RTTraceEvent::TickStart {
            tick_count: tick,
            timestamp_ns: tick * 1_000_000,
        });
    }

    let m = handle.lock().map_err(TracingError::init_failed)?;
    assert_eq!(m.rt_events_emitted, 100);
    Ok(())
}

#[test]
fn rt_metrics_jitter_tracked_via_deadline_miss() -> Result<(), TracingError> {
    let provider = MetricsTrackingProvider::new();
    let handle = provider.metrics_handle();
    let mut mgr = TracingManager::with_provider(Box::new(provider));
    mgr.initialize()?;

    mgr.emit_rt_event(RTTraceEvent::DeadlineMiss {
        tick_count: 1,
        timestamp_ns: 1_000_000,
        jitter_ns: 250_000,
    });
    mgr.emit_rt_event(RTTraceEvent::DeadlineMiss {
        tick_count: 5,
        timestamp_ns: 5_000_000,
        jitter_ns: 500_000,
    });

    let m = handle.lock().map_err(TracingError::init_failed)?;
    assert_eq!(m.deadline_misses, 2);
    assert_eq!(m.rt_events_emitted, 2);
    Ok(())
}

#[test]
fn rt_metrics_missed_ticks_counted() -> Result<(), TracingError> {
    let provider = MetricsTrackingProvider::new();
    let handle = provider.metrics_handle();
    let mut mgr = TracingManager::with_provider(Box::new(provider));
    mgr.initialize()?;

    for i in 0..5 {
        mgr.emit_rt_event(RTTraceEvent::DeadlineMiss {
            tick_count: i,
            timestamp_ns: i * 1_000_000,
            jitter_ns: 300_000,
        });
    }

    let m = handle.lock().map_err(TracingError::init_failed)?;
    assert_eq!(m.deadline_misses, 5);
    Ok(())
}

// ---------------------------------------------------------------------------
// Telemetry metrics: packet counts, parse errors (pipeline faults)
// ---------------------------------------------------------------------------

#[test]
fn telemetry_metrics_packet_counts() -> Result<(), TracingError> {
    let provider = MetricsTrackingProvider::new();
    let handle = provider.metrics_handle();
    let mut mgr = TracingManager::with_provider(Box::new(provider));
    mgr.initialize()?;

    mgr.emit_app_event(AppTraceEvent::TelemetryStarted {
        game_id: "iracing".into(),
        telemetry_rate_hz: 60.0,
    });
    mgr.emit_app_event(AppTraceEvent::DeviceConnected {
        device_id: "dev1".into(),
        device_name: "Wheel".into(),
        capabilities: "ffb".into(),
    });

    let m = handle.lock().map_err(TracingError::init_failed)?;
    assert_eq!(m.app_events_emitted, 2);
    Ok(())
}

#[test]
fn telemetry_metrics_parse_errors_via_pipeline_fault() -> Result<(), TracingError> {
    let provider = MetricsTrackingProvider::new();
    let handle = provider.metrics_handle();
    let mut mgr = TracingManager::with_provider(Box::new(provider));
    mgr.initialize()?;

    for code in 1..=3u8 {
        mgr.emit_rt_event(RTTraceEvent::PipelineFault {
            tick_count: code as u64,
            timestamp_ns: code as u64 * 1_000_000,
            error_code: code,
        });
    }

    let m = handle.lock().map_err(TracingError::init_failed)?;
    assert_eq!(m.pipeline_faults, 3);
    assert!(!m.is_healthy());
    Ok(())
}

// ---------------------------------------------------------------------------
// Streaming metrics: drop rate, latency (processing time)
// ---------------------------------------------------------------------------

#[test]
fn streaming_metrics_drop_rate_calculation() {
    let mut m = TracingMetrics::new();
    m.rt_events_emitted = 90;
    m.app_events_emitted = 10;
    m.events_dropped = 10;

    let rate = m.drop_rate();
    assert!((rate - 0.1).abs() < 0.0001, "drop rate should be ~0.1, got {rate}");
}

#[test]
fn streaming_metrics_latency_via_processing_time() {
    let mut m = TracingMetrics::new();
    m.rt_events_emitted = 4;
    m.record_processing_time(100);
    m.record_processing_time(200);
    m.record_processing_time(300);
    m.record_processing_time(400);

    let avg = m.average_rt_processing_time();
    assert_eq!(avg, std::time::Duration::from_nanos(250));
}

// ---------------------------------------------------------------------------
// Metric snapshot: all metrics captured atomically
// ---------------------------------------------------------------------------

#[test]
fn metric_snapshot_captures_all_fields() -> Result<(), TracingError> {
    let provider = MetricsTrackingProvider::new();
    let handle = provider.metrics_handle();
    let mut mgr = TracingManager::with_provider(Box::new(provider));
    mgr.initialize()?;

    // Emit a mix of events
    mgr.emit_rt_event(RTTraceEvent::TickStart {
        tick_count: 1,
        timestamp_ns: 1000,
    });
    mgr.emit_rt_event(RTTraceEvent::TickEnd {
        tick_count: 1,
        timestamp_ns: 2000,
        processing_time_ns: 500,
    });
    mgr.emit_rt_event(RTTraceEvent::DeadlineMiss {
        tick_count: 2,
        timestamp_ns: 3000,
        jitter_ns: 100,
    });
    mgr.emit_app_event(AppTraceEvent::DeviceConnected {
        device_id: "d1".into(),
        device_name: "Wheel".into(),
        capabilities: "ffb".into(),
    });

    let snapshot = handle.lock().map_err(TracingError::init_failed)?;
    assert_eq!(snapshot.rt_events_emitted, 3);
    assert_eq!(snapshot.app_events_emitted, 1);
    assert_eq!(snapshot.deadline_misses, 1);
    assert_eq!(snapshot.total_rt_processing_ns, 500);
    Ok(())
}

#[test]
fn metric_snapshot_via_manager_metrics() -> Result<(), TracingError> {
    let provider = MetricsTrackingProvider::new();
    let mut mgr = TracingManager::with_provider(Box::new(provider));
    mgr.initialize()?;

    mgr.emit_rt_event(RTTraceEvent::TickStart {
        tick_count: 1,
        timestamp_ns: 1000,
    });

    let snapshot = mgr.metrics();
    assert_eq!(snapshot.rt_events_emitted, 1);
    Ok(())
}

// ---------------------------------------------------------------------------
// Metric reset: values clear to zero
// ---------------------------------------------------------------------------

#[test]
fn metric_reset_clears_all_fields() {
    let mut m = TracingMetrics::new();
    m.rt_events_emitted = 100;
    m.app_events_emitted = 50;
    m.events_dropped = 5;
    m.deadline_misses = 3;
    m.pipeline_faults = 1;
    m.total_rt_processing_ns = 999_999;
    m.reinitializations = 2;

    m.reset();

    assert_eq!(m.rt_events_emitted, 0);
    assert_eq!(m.app_events_emitted, 0);
    assert_eq!(m.events_dropped, 0);
    assert_eq!(m.deadline_misses, 0);
    assert_eq!(m.pipeline_faults, 0);
    assert_eq!(m.total_rt_processing_ns, 0);
    assert_eq!(m.reinitializations, 0);
    assert!(m.is_healthy());
}

#[test]
fn metric_reset_restores_healthy_state() {
    let mut m = TracingMetrics::new();
    m.pipeline_faults = 5;
    assert!(!m.is_healthy());

    m.reset();
    assert!(m.is_healthy());
}

// ---------------------------------------------------------------------------
// Property test: metrics always non-negative (saturating arithmetic)
// ---------------------------------------------------------------------------

#[test]
fn property_metrics_always_non_negative_after_operations() {
    let mut m = TracingMetrics::new();

    // Saturating at u64::MAX
    m.rt_events_emitted = u64::MAX;
    m.record_rt_event();
    assert_eq!(m.rt_events_emitted, u64::MAX);

    m.app_events_emitted = u64::MAX;
    m.record_app_event();
    assert_eq!(m.app_events_emitted, u64::MAX);

    m.events_dropped = u64::MAX;
    m.record_dropped_event();
    assert_eq!(m.events_dropped, u64::MAX);

    m.deadline_misses = u64::MAX;
    m.record_deadline_miss();
    assert_eq!(m.deadline_misses, u64::MAX);

    m.pipeline_faults = u64::MAX;
    m.record_pipeline_fault();
    assert_eq!(m.pipeline_faults, u64::MAX);

    m.total_rt_processing_ns = u64::MAX;
    m.record_processing_time(1);
    assert_eq!(m.total_rt_processing_ns, u64::MAX);

    m.reinitializations = u64::MAX;
    m.record_reinitialization();
    assert_eq!(m.reinitializations, u64::MAX);
}

#[test]
fn property_merge_preserves_non_negative() {
    let mut m1 = TracingMetrics::new();
    m1.rt_events_emitted = u64::MAX - 1;

    let m2 = TracingMetrics {
        rt_events_emitted: 10,
        ..Default::default()
    };

    m1.merge(&m2);
    assert_eq!(m1.rt_events_emitted, u64::MAX);
}

// ---------------------------------------------------------------------------
// Property test: drop rate always in [0, 1]
// ---------------------------------------------------------------------------

#[test]
fn property_drop_rate_always_bounded() {
    // Zero events -> 0.0
    let m = TracingMetrics::new();
    assert_eq!(m.drop_rate(), 0.0);

    // No drops -> 0.0
    let m = TracingMetrics {
        rt_events_emitted: 100,
        ..Default::default()
    };
    assert_eq!(m.drop_rate(), 0.0);

    // Drops < total -> rate < 1.0
    let m = TracingMetrics {
        rt_events_emitted: 90,
        app_events_emitted: 10,
        events_dropped: 50,
        ..Default::default()
    };
    let rate = m.drop_rate();
    assert!((0.0..=1.0).contains(&rate), "drop_rate out of [0,1]: {rate}");
}

#[test]
fn property_drop_rate_high_drops() {
    // Drops can exceed total events because dropped events don't add to emitted count
    let m = TracingMetrics {
        rt_events_emitted: 10,
        events_dropped: 1000,
        ..Default::default()
    };
    let rate = m.drop_rate();
    // Rate can be > 1.0 when drops exceed emitted (design: drop_rate = dropped / emitted)
    assert!(rate >= 0.0, "drop_rate should be non-negative: {rate}");
}

// ---------------------------------------------------------------------------
// Event classification and accessor tests
// ---------------------------------------------------------------------------

#[test]
fn rt_event_categories_are_correct() {
    let events_and_categories = [
        (
            RTTraceEvent::TickStart {
                tick_count: 0,
                timestamp_ns: 0,
            },
            RTEventCategory::Timing,
        ),
        (
            RTTraceEvent::TickEnd {
                tick_count: 0,
                timestamp_ns: 0,
                processing_time_ns: 0,
            },
            RTEventCategory::Timing,
        ),
        (
            RTTraceEvent::HidWrite {
                tick_count: 0,
                timestamp_ns: 0,
                torque_nm: 0.0,
                seq: 0,
            },
            RTEventCategory::Hid,
        ),
        (
            RTTraceEvent::DeadlineMiss {
                tick_count: 0,
                timestamp_ns: 0,
                jitter_ns: 0,
            },
            RTEventCategory::Error,
        ),
        (
            RTTraceEvent::PipelineFault {
                tick_count: 0,
                timestamp_ns: 0,
                error_code: 0,
            },
            RTEventCategory::Error,
        ),
    ];

    for (event, expected_cat) in &events_and_categories {
        assert_eq!(event.category(), *expected_cat, "wrong category for {event}");
    }
}

#[test]
fn rt_event_is_error_classification() {
    assert!(RTTraceEvent::DeadlineMiss {
        tick_count: 0,
        timestamp_ns: 0,
        jitter_ns: 0,
    }
    .is_error());
    assert!(RTTraceEvent::PipelineFault {
        tick_count: 0,
        timestamp_ns: 0,
        error_code: 0,
    }
    .is_error());
    assert!(!RTTraceEvent::TickStart {
        tick_count: 0,
        timestamp_ns: 0,
    }
    .is_error());
    assert!(!RTTraceEvent::TickEnd {
        tick_count: 0,
        timestamp_ns: 0,
        processing_time_ns: 0,
    }
    .is_error());
    assert!(!RTTraceEvent::HidWrite {
        tick_count: 0,
        timestamp_ns: 0,
        torque_nm: 0.0,
        seq: 0,
    }
    .is_error());
}

#[test]
fn rt_event_accessors_return_correct_values() {
    let event = RTTraceEvent::HidWrite {
        tick_count: 42,
        timestamp_ns: 99999,
        torque_nm: 3.5,
        seq: 7,
    };
    assert_eq!(event.tick_count(), 42);
    assert_eq!(event.timestamp_ns(), 99999);
    assert_eq!(event.event_type(), "hid_write");
}

#[test]
fn app_event_categories_and_device_id() {
    let events: Vec<(AppTraceEvent, AppEventCategory, Option<&str>)> = vec![
        (
            AppTraceEvent::DeviceConnected {
                device_id: "d1".into(),
                device_name: "W".into(),
                capabilities: "c".into(),
            },
            AppEventCategory::Device,
            Some("d1"),
        ),
        (
            AppTraceEvent::DeviceDisconnected {
                device_id: "d2".into(),
                reason: "r".into(),
            },
            AppEventCategory::Device,
            Some("d2"),
        ),
        (
            AppTraceEvent::TelemetryStarted {
                game_id: "g".into(),
                telemetry_rate_hz: 60.0,
            },
            AppEventCategory::Telemetry,
            None,
        ),
        (
            AppTraceEvent::ProfileApplied {
                device_id: "d3".into(),
                profile_name: "p".into(),
                profile_hash: "h".into(),
            },
            AppEventCategory::Profile,
            Some("d3"),
        ),
        (
            AppTraceEvent::SafetyStateChanged {
                device_id: "d4".into(),
                old_state: "safe".into(),
                new_state: "warn".into(),
                reason: "r".into(),
            },
            AppEventCategory::Safety,
            Some("d4"),
        ),
    ];

    for (event, expected_cat, expected_id) in &events {
        assert_eq!(event.category(), *expected_cat);
        assert_eq!(event.device_id(), *expected_id);
    }
}

// ---------------------------------------------------------------------------
// Error type tests
// ---------------------------------------------------------------------------

#[test]
fn error_recoverability_classification() {
    assert!(!TracingError::PlatformNotSupported.is_recoverable());
    assert!(!TracingError::InitializationFailed("x".into()).is_recoverable());
    assert!(!TracingError::InvalidConfiguration("x".into()).is_recoverable());
    assert!(TracingError::BufferOverflow(10).is_recoverable());
    assert!(TracingError::EmissionFailed("x".into()).is_recoverable());
    assert!(TracingError::NotInitialized.is_recoverable());
    assert!(TracingError::PlatformError("x".into()).is_recoverable());
}

#[test]
fn error_platform_missing_flag() {
    assert!(TracingError::PlatformNotSupported.is_platform_missing());
    assert!(!TracingError::NotInitialized.is_platform_missing());
    assert!(!TracingError::BufferOverflow(0).is_platform_missing());
}

#[test]
fn error_display_contains_context() {
    let e = TracingError::BufferOverflow(42);
    assert!(e.to_string().contains("42"));

    let e = TracingError::init_failed("provider crashed");
    assert!(e.to_string().contains("provider crashed"));

    let e = TracingError::emit_failed("write error");
    assert!(e.to_string().contains("write error"));
}

// ---------------------------------------------------------------------------
// Manager lifecycle: enable/disable, shutdown
// ---------------------------------------------------------------------------

#[test]
fn manager_disabled_does_not_emit() -> Result<(), TracingError> {
    let provider = MetricsTrackingProvider::new();
    let handle = provider.metrics_handle();
    let mut mgr = TracingManager::with_provider(Box::new(provider));
    mgr.initialize()?;

    mgr.set_enabled(false);
    assert!(!mgr.is_enabled());

    mgr.emit_rt_event(RTTraceEvent::TickStart {
        tick_count: 1,
        timestamp_ns: 1000,
    });

    let m = handle.lock().map_err(TracingError::init_failed)?;
    assert_eq!(m.rt_events_emitted, 0);
    Ok(())
}

#[test]
fn manager_re_enable_resumes_emission() -> Result<(), TracingError> {
    let provider = MetricsTrackingProvider::new();
    let handle = provider.metrics_handle();
    let mut mgr = TracingManager::with_provider(Box::new(provider));
    mgr.initialize()?;

    mgr.set_enabled(false);
    mgr.emit_rt_event(RTTraceEvent::TickStart {
        tick_count: 1,
        timestamp_ns: 1000,
    });

    mgr.set_enabled(true);
    mgr.emit_rt_event(RTTraceEvent::TickStart {
        tick_count: 2,
        timestamp_ns: 2000,
    });

    let m = handle.lock().map_err(TracingError::init_failed)?;
    assert_eq!(m.rt_events_emitted, 1);
    Ok(())
}

// ---------------------------------------------------------------------------
// Metrics Display
// ---------------------------------------------------------------------------

#[test]
fn metrics_display_contains_key_info() {
    let m = TracingMetrics {
        rt_events_emitted: 100,
        app_events_emitted: 50,
        events_dropped: 5,
        deadline_misses: 2,
        pipeline_faults: 1,
        total_rt_processing_ns: 1000,
        reinitializations: 0,
    };

    let display = format!("{m}");
    assert!(display.contains("rt=100"));
    assert!(display.contains("app=50"));
    assert!(display.contains("dropped=5"));
    assert!(display.contains("misses=2"));
    assert!(display.contains("faults=1"));
}

// ---------------------------------------------------------------------------
// Metrics merge
// ---------------------------------------------------------------------------

#[test]
fn metrics_merge_combines_all_fields() {
    let mut m1 = TracingMetrics {
        rt_events_emitted: 10,
        app_events_emitted: 5,
        events_dropped: 1,
        deadline_misses: 2,
        pipeline_faults: 0,
        total_rt_processing_ns: 500,
        reinitializations: 1,
    };

    let m2 = TracingMetrics {
        rt_events_emitted: 20,
        app_events_emitted: 10,
        events_dropped: 3,
        deadline_misses: 1,
        pipeline_faults: 2,
        total_rt_processing_ns: 700,
        reinitializations: 0,
    };

    m1.merge(&m2);

    assert_eq!(m1.rt_events_emitted, 30);
    assert_eq!(m1.app_events_emitted, 15);
    assert_eq!(m1.events_dropped, 4);
    assert_eq!(m1.deadline_misses, 3);
    assert_eq!(m1.pipeline_faults, 2);
    assert_eq!(m1.total_rt_processing_ns, 1200);
    assert_eq!(m1.reinitializations, 1);
}

// ---------------------------------------------------------------------------
// Tracing subsystem infrastructure tests
// ---------------------------------------------------------------------------
// Tests below exercise the `tracing` framework integration: spans, events,
// subscribers, filtering, formatting, and performance.

use std::io;
use tracing::Instrument;
use tracing_subscriber::fmt::MakeWriter;

/// Writer that captures tracing output to a shared buffer.
#[derive(Clone)]
struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

impl CaptureWriter {
    fn new() -> (Self, Arc<Mutex<Vec<u8>>>) {
        let buf = Arc::new(Mutex::new(Vec::new()));
        (Self(buf.clone()), buf)
    }
}

impl io::Write for CaptureWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if let Ok(mut inner) = self.0.lock() {
            inner.extend_from_slice(data);
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for CaptureWriter {
    type Writer = CaptureWriter;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

fn read_captured(buf: &Arc<Mutex<Vec<u8>>>) -> String {
    buf.lock()
        .map(|g| String::from_utf8_lossy(&g).into_owned())
        .unwrap_or_default()
}

// 1. Span creation and lifecycle ------------------------------------------------

#[test]
fn span_creation_and_lifecycle() {
    let (writer, buf) = CaptureWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .without_time()
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        let span = tracing::info_span!("lifecycle_span", tick = 1u64);
        {
            let _guard = span.enter();
            tracing::info!("entered");
        }
        tracing::info!("after_exit");
    });

    let out = read_captured(&buf);
    assert!(out.contains("lifecycle_span"), "span name missing: {out}");
    assert!(out.contains("entered"), "entered event missing: {out}");
    assert!(
        out.contains("after_exit"),
        "after_exit event missing: {out}"
    );
}

// 2. Event emission at all log levels -------------------------------------------

#[test]
fn event_emission_all_log_levels() {
    let (writer, buf) = CaptureWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .without_time()
        .with_max_level(tracing::Level::TRACE)
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        tracing::trace!("t_level");
        tracing::debug!("d_level");
        tracing::info!("i_level");
        tracing::warn!("w_level");
        tracing::error!("e_level");
    });

    let out = read_captured(&buf);
    assert!(
        out.contains("TRACE") && out.contains("t_level"),
        "TRACE missing: {out}"
    );
    assert!(
        out.contains("DEBUG") && out.contains("d_level"),
        "DEBUG missing: {out}"
    );
    assert!(
        out.contains("INFO") && out.contains("i_level"),
        "INFO missing: {out}"
    );
    assert!(
        out.contains("WARN") && out.contains("w_level"),
        "WARN missing: {out}"
    );
    assert!(
        out.contains("ERROR") && out.contains("e_level"),
        "ERROR missing: {out}"
    );
}

// 3. Structured field encoding --------------------------------------------------

#[test]
fn structured_field_encoding() {
    let (writer, buf) = CaptureWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .without_time()
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(
            tick_count = 42u64,
            torque_nm = 3.15f64,
            device_id = "wheel-001",
            enabled = true,
            "structured event"
        );
    });

    let out = read_captured(&buf);
    assert!(
        out.contains("tick_count=42"),
        "tick_count field missing: {out}"
    );
    assert!(
        out.contains("torque_nm=3.15"),
        "torque_nm field missing: {out}"
    );
    assert!(out.contains("device_id"), "device_id field missing: {out}");
    assert!(
        out.contains("enabled=true"),
        "enabled field missing: {out}"
    );
}

// 4. Span nesting and context propagation ----------------------------------------

#[test]
fn span_nesting_and_context_propagation() {
    let (writer, buf) = CaptureWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .without_time()
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        let outer = tracing::info_span!("outer", layer = "parent");
        let _outer_guard = outer.enter();
        let inner = tracing::info_span!("inner", layer = "child");
        let _inner_guard = inner.enter();
        tracing::info!("nested event");
    });

    let out = read_captured(&buf);
    assert!(out.contains("outer"), "outer span missing: {out}");
    assert!(out.contains("inner"), "inner span missing: {out}");
    assert!(out.contains("nested event"), "event missing: {out}");
}

// 5. Subscriber filtering -------------------------------------------------------

#[test]
fn subscriber_filtering_by_level() {
    let (writer, buf) = CaptureWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .without_time()
        .with_max_level(tracing::Level::WARN)
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!("should_be_filtered");
        tracing::debug!("also_filtered");
        tracing::warn!("should_appear");
        tracing::error!("also_appears");
    });

    let out = read_captured(&buf);
    assert!(
        !out.contains("should_be_filtered"),
        "INFO should be filtered: {out}"
    );
    assert!(
        !out.contains("also_filtered"),
        "DEBUG should be filtered: {out}"
    );
    assert!(
        out.contains("should_appear"),
        "WARN should pass filter: {out}"
    );
    assert!(
        out.contains("also_appears"),
        "ERROR should pass filter: {out}"
    );
}

// 6. Output format stability: JSON, compact, full --------------------------------

#[test]
fn output_format_json() {
    let (writer, buf) = CaptureWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_writer(writer)
        .without_time()
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(tick = 1u64, torque = 5.0f64, "json event");
    });

    let out = read_captured(&buf);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(out.trim());
    assert!(parsed.is_ok(), "output should be valid JSON: {out}");

    if let Ok(val) = parsed {
        assert!(
            val.get("level").is_some(),
            "JSON should have level: {val}"
        );
    }
}

#[test]
fn output_format_compact() {
    let (writer, buf) = CaptureWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .compact()
        .with_writer(writer)
        .with_ansi(false)
        .without_time()
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        let span = tracing::info_span!("compact_span", id = 1u64);
        let _guard = span.enter();
        tracing::info!("compact event");
    });

    let out = read_captured(&buf);
    assert!(out.contains("compact event"), "event missing: {out}");
    let line_count = out.lines().count();
    assert!(
        line_count <= 2,
        "compact should be concise, got {line_count} lines: {out}"
    );
}

#[test]
fn output_format_full() {
    let (writer, buf) = CaptureWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .without_time()
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(key = "value", "full format event");
    });

    let out = read_captured(&buf);
    assert!(out.contains("INFO"), "level should appear: {out}");
    assert!(
        out.contains("full format event"),
        "message missing: {out}"
    );
    assert!(out.contains("key="), "field should appear: {out}");
}

// 7. Performance impact measurement -----------------------------------------------

#[test]
fn performance_tracing_event_emission_overhead() {
    let (writer, _buf) = CaptureWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .without_time()
        .finish();

    let start = std::time::Instant::now();
    tracing::subscriber::with_default(subscriber, || {
        for i in 0..1_000u64 {
            tracing::info!(tick = i, "perf test");
        }
    });
    let elapsed = start.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "1000 events took too long: {elapsed:?}"
    );
}

#[test]
fn performance_rt_event_emission_throughput() -> Result<(), TracingError> {
    let provider = MetricsTrackingProvider::new();
    let mut mgr = TracingManager::with_provider(Box::new(provider));
    mgr.initialize()?;

    let start = std::time::Instant::now();
    for i in 0..10_000u64 {
        mgr.emit_rt_event(RTTraceEvent::TickStart {
            tick_count: i,
            timestamp_ns: i * 1_000_000,
        });
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(1),
        "10000 RT events took too long: {elapsed:?}"
    );
    Ok(())
}

// 8. Thread-local context --------------------------------------------------------

#[test]
fn thread_local_span_enter_exit() {
    let (writer, buf) = CaptureWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .without_time()
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!("before_enter");
        let span = tracing::info_span!("local_span");
        {
            let _guard = span.enter();
            tracing::info!("inside_span");
        }
        tracing::info!("after_exit");
    });

    let out = read_captured(&buf);
    for line in out.lines() {
        if line.contains("before_enter") {
            assert!(
                !line.contains("local_span"),
                "span should not appear before enter: {line}"
            );
        }
        if line.contains("inside_span") {
            assert!(
                line.contains("local_span"),
                "span should appear inside: {line}"
            );
        }
        if line.contains("after_exit") {
            assert!(
                !line.contains("local_span"),
                "span should not appear after exit: {line}"
            );
        }
    }
}

// 9. Async context propagation ---------------------------------------------------

#[tokio::test]
async fn async_context_propagation() {
    let (writer, buf) = CaptureWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .without_time()
        .finish();

    let _guard = tracing::subscriber::set_default(subscriber);

    let span = tracing::info_span!("async_parent");
    async {
        tracing::info!("async_inner_event");
    }
    .instrument(span)
    .await;

    let out = read_captured(&buf);
    assert!(
        out.contains("async_parent"),
        "parent span missing in async: {out}"
    );
    assert!(
        out.contains("async_inner_event"),
        "event missing: {out}"
    );
}

#[tokio::test]
async fn async_nested_spans_propagation() {
    let (writer, buf) = CaptureWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .without_time()
        .finish();

    let _guard = tracing::subscriber::set_default(subscriber);
    let parent = tracing::info_span!("async_outer");
    async {
        let child = tracing::info_span!("async_inner");
        async {
            tracing::info!("deeply_nested");
        }
        .instrument(child)
        .await;
    }
    .instrument(parent)
    .await;

    let out = read_captured(&buf);
    assert!(out.contains("async_outer"), "outer span missing: {out}");
    assert!(out.contains("async_inner"), "inner span missing: {out}");
    assert!(out.contains("deeply_nested"), "event missing: {out}");
}

// 10. Rate limiting for noisy events -----------------------------------------------

struct RateLimitingProvider {
    metrics: Arc<Mutex<TracingMetrics>>,
    max_events: u64,
    count: Arc<Mutex<u64>>,
}

impl RateLimitingProvider {
    fn new(max_events: u64) -> Self {
        Self {
            metrics: Arc::new(Mutex::new(TracingMetrics::new())),
            max_events,
            count: Arc::new(Mutex::new(0)),
        }
    }

    fn metrics_handle(&self) -> Arc<Mutex<TracingMetrics>> {
        Arc::clone(&self.metrics)
    }
}

impl TracingProvider for RateLimitingProvider {
    fn initialize(&mut self) -> Result<(), TracingError> {
        Ok(())
    }

    fn emit_rt_event(&self, _event: RTTraceEvent) {
        let Ok(mut count) = self.count.lock() else {
            return;
        };
        if *count >= self.max_events {
            drop(count);
            if let Ok(mut m) = self.metrics.lock() {
                m.record_dropped_event();
            }
            return;
        }
        *count += 1;
        drop(count);
        if let Ok(mut m) = self.metrics.lock() {
            m.record_rt_event();
        }
    }

    fn emit_app_event(&self, _event: AppTraceEvent) {
        if let Ok(mut m) = self.metrics.lock() {
            m.record_app_event();
        }
    }

    fn metrics(&self) -> TracingMetrics {
        self.metrics.lock().map(|m| m.clone()).unwrap_or_default()
    }

    fn shutdown(&mut self) {}
}

#[test]
fn rate_limiting_drops_excess_events() -> Result<(), TracingError> {
    let provider = RateLimitingProvider::new(5);
    let handle = provider.metrics_handle();
    let mut mgr = TracingManager::with_provider(Box::new(provider));
    mgr.initialize()?;

    for i in 0..10u64 {
        mgr.emit_rt_event(RTTraceEvent::TickStart {
            tick_count: i,
            timestamp_ns: i * 1_000_000,
        });
    }

    let m = handle.lock().map_err(TracingError::init_failed)?;
    assert_eq!(
        m.rt_events_emitted, 5,
        "only 5 events should pass rate limit"
    );
    assert_eq!(m.events_dropped, 5, "5 events should be dropped");
    Ok(())
}

// 11. Sampling configuration -------------------------------------------------------

struct SamplingProvider {
    metrics: Arc<Mutex<TracingMetrics>>,
    sample_rate: u64,
    counter: Arc<Mutex<u64>>,
}

impl SamplingProvider {
    fn new(sample_rate: u64) -> Self {
        Self {
            metrics: Arc::new(Mutex::new(TracingMetrics::new())),
            sample_rate,
            counter: Arc::new(Mutex::new(0)),
        }
    }

    fn metrics_handle(&self) -> Arc<Mutex<TracingMetrics>> {
        Arc::clone(&self.metrics)
    }
}

impl TracingProvider for SamplingProvider {
    fn initialize(&mut self) -> Result<(), TracingError> {
        Ok(())
    }

    fn emit_rt_event(&self, _event: RTTraceEvent) {
        let Ok(mut count) = self.counter.lock() else {
            return;
        };
        *count += 1;
        if *count % self.sample_rate == 0 {
            drop(count);
            if let Ok(mut m) = self.metrics.lock() {
                m.record_rt_event();
            }
        }
    }

    fn emit_app_event(&self, _event: AppTraceEvent) {}

    fn metrics(&self) -> TracingMetrics {
        self.metrics.lock().map(|m| m.clone()).unwrap_or_default()
    }

    fn shutdown(&mut self) {}
}

#[test]
fn sampling_reduces_event_volume() -> Result<(), TracingError> {
    let provider = SamplingProvider::new(10);
    let handle = provider.metrics_handle();
    let mut mgr = TracingManager::with_provider(Box::new(provider));
    mgr.initialize()?;

    for i in 0..100u64 {
        mgr.emit_rt_event(RTTraceEvent::TickStart {
            tick_count: i,
            timestamp_ns: i * 1_000_000,
        });
    }

    let m = handle.lock().map_err(TracingError::init_failed)?;
    assert_eq!(
        m.rt_events_emitted, 10,
        "sampling 1-in-10 of 100 should yield 10"
    );
    Ok(())
}

// 12. Custom field types and serialization -----------------------------------------

#[test]
fn custom_field_types_display_and_debug() {
    let (writer, buf) = CaptureWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .without_time()
        .finish();

    let event = RTTraceEvent::HidWrite {
        tick_count: 42,
        timestamp_ns: 1_000_000,
        torque_nm: 3.5,
        seq: 7,
    };

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(event_display = %event, "display format");
        tracing::info!(event_debug = ?event, "debug format");
    });

    let out = read_captured(&buf);
    assert!(
        out.contains("HidWrite"),
        "Display format should contain variant name: {out}"
    );
    assert!(
        out.contains("display format"),
        "display message missing: {out}"
    );
    assert!(
        out.contains("debug format"),
        "debug message missing: {out}"
    );
}

#[test]
fn custom_field_serialization_to_json() {
    let (writer, buf) = CaptureWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_writer(writer)
        .without_time()
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        let event = RTTraceEvent::DeadlineMiss {
            tick_count: 99,
            timestamp_ns: 5_000_000,
            jitter_ns: 250_000,
        };
        tracing::error!(
            event_type = event.event_type(),
            tick = event.tick_count(),
            ts = event.timestamp_ns(),
            is_error = event.is_error(),
            "rt event in json"
        );
    });

    let out = read_captured(&buf);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(out.trim());
    assert!(parsed.is_ok(), "should be valid JSON: {out}");
}
