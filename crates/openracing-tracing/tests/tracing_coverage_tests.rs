//! Tracing events, metrics, errors, and manager coverage expansion tests.
//!
//! Covers: RT event construction and accessors, category filtering, app events,
//! metrics arithmetic and health checks, error classification, manager lifecycle.

use openracing_tracing::{
    AppTraceEvent, RTTraceEvent, TracingError, TracingManager, TracingMetrics, TracingProvider,
    events::{AppEventCategory, RTEventCategory},
};
use std::time::Duration;

#[allow(unused_imports)]
use std::sync::{Arc, Mutex};

type R = Result<(), TracingError>;

// ═══════════════════════════════════════════════════════════════════════════
// Mock provider for testing
// ═══════════════════════════════════════════════════════════════════════════

#[allow(dead_code)]
struct CountingProvider {
    rt_count: Arc<Mutex<u64>>,
    app_count: Arc<Mutex<u64>>,
    initialized: bool,
}

#[allow(dead_code)]
impl CountingProvider {
    fn new() -> (Self, Arc<Mutex<u64>>, Arc<Mutex<u64>>) {
        let rt = Arc::new(Mutex::new(0u64));
        let app = Arc::new(Mutex::new(0u64));
        (
            Self {
                rt_count: rt.clone(),
                app_count: app.clone(),
                initialized: false,
            },
            rt,
            app,
        )
    }
}

impl TracingProvider for CountingProvider {
    fn initialize(&mut self) -> Result<(), TracingError> {
        self.initialized = true;
        Ok(())
    }

    fn emit_rt_event(&self, _event: RTTraceEvent) {
        if let Ok(mut c) = self.rt_count.lock() {
            *c += 1;
        }
    }

    fn emit_app_event(&self, _event: AppTraceEvent) {
        if let Ok(mut c) = self.app_count.lock() {
            *c += 1;
        }
    }

    fn metrics(&self) -> TracingMetrics {
        let mut m = TracingMetrics::new();
        if let Ok(c) = self.rt_count.lock() {
            m.rt_events_emitted = *c;
        }
        m
    }

    fn is_enabled(&self) -> bool {
        self.initialized
    }

    fn shutdown(&mut self) {
        self.initialized = false;
    }
}

// A simpler mock that always reports enabled
struct SimpleProvider;

impl TracingProvider for SimpleProvider {
    fn initialize(&mut self) -> Result<(), TracingError> {
        Ok(())
    }
    fn emit_rt_event(&self, _event: RTTraceEvent) {}
    fn emit_app_event(&self, _event: AppTraceEvent) {}
    fn metrics(&self) -> TracingMetrics {
        TracingMetrics::new()
    }
    fn shutdown(&mut self) {}
}

// ═══════════════════════════════════════════════════════════════════════════
// RT event tests
// ═══════════════════════════════════════════════════════════════════════════

mod rt_events {
    use super::*;

    #[test]
    fn tick_start_accessors() {
        let event = RTTraceEvent::TickStart {
            tick_count: 42,
            timestamp_ns: 999_999,
        };
        assert_eq!(event.tick_count(), 42);
        assert_eq!(event.timestamp_ns(), 999_999);
        assert_eq!(event.event_type(), "tick_start");
        assert_eq!(event.category(), RTEventCategory::Timing);
        assert!(!event.is_error());
    }

    #[test]
    fn tick_end_accessors() {
        let event = RTTraceEvent::TickEnd {
            tick_count: 10,
            timestamp_ns: 2_000_000,
            processing_time_ns: 500,
        };
        assert_eq!(event.tick_count(), 10);
        assert_eq!(event.timestamp_ns(), 2_000_000);
        assert_eq!(event.event_type(), "tick_end");
        assert_eq!(event.category(), RTEventCategory::Timing);
        assert!(!event.is_error());
    }

    #[test]
    fn hid_write_accessors() {
        let event = RTTraceEvent::HidWrite {
            tick_count: 5,
            timestamp_ns: 1_500_000,
            torque_nm: 3.25,
            seq: 42,
        };
        assert_eq!(event.tick_count(), 5);
        assert_eq!(event.event_type(), "hid_write");
        assert_eq!(event.category(), RTEventCategory::Hid);
        assert!(!event.is_error());
    }

    #[test]
    fn deadline_miss_is_error() {
        let event = RTTraceEvent::DeadlineMiss {
            tick_count: 100,
            timestamp_ns: 5_000_000,
            jitter_ns: 250_000,
        };
        assert!(event.is_error());
        assert_eq!(event.category(), RTEventCategory::Error);
        assert_eq!(event.event_type(), "deadline_miss");
    }

    #[test]
    fn pipeline_fault_is_error() {
        let event = RTTraceEvent::PipelineFault {
            tick_count: 200,
            timestamp_ns: 6_000_000,
            error_code: 0xAB,
        };
        assert!(event.is_error());
        assert_eq!(event.category(), RTEventCategory::Error);
        assert_eq!(event.event_type(), "pipeline_fault");
    }

    #[test]
    fn rt_event_display_contains_type() {
        let events = [
            RTTraceEvent::TickStart {
                tick_count: 1,
                timestamp_ns: 100,
            },
            RTTraceEvent::TickEnd {
                tick_count: 1,
                timestamp_ns: 200,
                processing_time_ns: 100,
            },
            RTTraceEvent::HidWrite {
                tick_count: 1,
                timestamp_ns: 300,
                torque_nm: 1.0,
                seq: 1,
            },
            RTTraceEvent::DeadlineMiss {
                tick_count: 1,
                timestamp_ns: 400,
                jitter_ns: 50,
            },
            RTTraceEvent::PipelineFault {
                tick_count: 1,
                timestamp_ns: 500,
                error_code: 1,
            },
        ];

        let expected = [
            "TickStart",
            "TickEnd",
            "HidWrite",
            "DeadlineMiss",
            "PipelineFault",
        ];

        for (event, name) in events.iter().zip(expected.iter()) {
            let s = format!("{event}");
            assert!(s.contains(name), "Display of {name} should contain name");
        }
    }

    #[test]
    fn rt_event_equality() {
        let a = RTTraceEvent::TickStart {
            tick_count: 1,
            timestamp_ns: 100,
        };
        let b = RTTraceEvent::TickStart {
            tick_count: 1,
            timestamp_ns: 100,
        };
        let c = RTTraceEvent::TickStart {
            tick_count: 2,
            timestamp_ns: 100,
        };

        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// App event tests
// ═══════════════════════════════════════════════════════════════════════════

mod app_events {
    use super::*;

    #[test]
    fn device_connected_category() {
        let event = AppTraceEvent::DeviceConnected {
            device_id: "dev-1".to_string(),
            device_name: "Test Wheel".to_string(),
            capabilities: "FFB".to_string(),
        };
        assert_eq!(event.category(), AppEventCategory::Device);
        assert_eq!(event.device_id(), Some("dev-1"));
    }

    #[test]
    fn device_disconnected_category() {
        let event = AppTraceEvent::DeviceDisconnected {
            device_id: "dev-1".to_string(),
            reason: "USB removed".to_string(),
        };
        assert_eq!(event.category(), AppEventCategory::Device);
        assert_eq!(event.device_id(), Some("dev-1"));
    }

    #[test]
    fn telemetry_started_no_device_id() {
        let event = AppTraceEvent::TelemetryStarted {
            game_id: "iracing".to_string(),
            telemetry_rate_hz: 60.0,
        };
        assert_eq!(event.category(), AppEventCategory::Telemetry);
        assert_eq!(event.device_id(), None);
    }

    #[test]
    fn profile_applied_category() {
        let event = AppTraceEvent::ProfileApplied {
            device_id: "dev-2".to_string(),
            profile_name: "drift".to_string(),
            profile_hash: "abc123".to_string(),
        };
        assert_eq!(event.category(), AppEventCategory::Profile);
        assert_eq!(event.device_id(), Some("dev-2"));
    }

    #[test]
    fn safety_state_changed_category() {
        let event = AppTraceEvent::SafetyStateChanged {
            device_id: "dev-3".to_string(),
            old_state: "SafeTorque".to_string(),
            new_state: "HighTorqueActive".to_string(),
            reason: "user confirmed".to_string(),
        };
        assert_eq!(event.category(), AppEventCategory::Safety);
        assert_eq!(event.device_id(), Some("dev-3"));
    }

    #[test]
    fn app_event_display_all_variants() {
        let events = [
            AppTraceEvent::DeviceConnected {
                device_id: "d1".to_string(),
                device_name: "W".to_string(),
                capabilities: "C".to_string(),
            },
            AppTraceEvent::DeviceDisconnected {
                device_id: "d1".to_string(),
                reason: "R".to_string(),
            },
            AppTraceEvent::TelemetryStarted {
                game_id: "g".to_string(),
                telemetry_rate_hz: 60.0,
            },
            AppTraceEvent::ProfileApplied {
                device_id: "d1".to_string(),
                profile_name: "P".to_string(),
                profile_hash: "H".to_string(),
            },
            AppTraceEvent::SafetyStateChanged {
                device_id: "d1".to_string(),
                old_state: "A".to_string(),
                new_state: "B".to_string(),
                reason: "R".to_string(),
            },
        ];

        for event in &events {
            let s = format!("{event}");
            assert!(!s.is_empty(), "Display should not be empty");
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Metrics tests
// ═══════════════════════════════════════════════════════════════════════════

mod metrics {
    use super::*;

    #[test]
    fn metrics_default_all_zero() {
        let m = TracingMetrics::new();
        assert_eq!(m.rt_events_emitted, 0);
        assert_eq!(m.app_events_emitted, 0);
        assert_eq!(m.events_dropped, 0);
        assert_eq!(m.deadline_misses, 0);
        assert_eq!(m.pipeline_faults, 0);
        assert_eq!(m.total_rt_processing_ns, 0);
        assert_eq!(m.reinitializations, 0);
    }

    #[test]
    fn record_rt_event_increments() {
        let mut m = TracingMetrics::new();
        m.record_rt_event();
        m.record_rt_event();
        m.record_rt_event();
        assert_eq!(m.rt_events_emitted, 3);
    }

    #[test]
    fn record_app_event_increments() {
        let mut m = TracingMetrics::new();
        m.record_app_event();
        assert_eq!(m.app_events_emitted, 1);
    }

    #[test]
    fn record_dropped_event_increments() {
        let mut m = TracingMetrics::new();
        m.record_dropped_event();
        m.record_dropped_event();
        assert_eq!(m.events_dropped, 2);
    }

    #[test]
    fn record_deadline_miss_increments() {
        let mut m = TracingMetrics::new();
        m.record_deadline_miss();
        assert_eq!(m.deadline_misses, 1);
    }

    #[test]
    fn record_pipeline_fault_increments() {
        let mut m = TracingMetrics::new();
        m.record_pipeline_fault();
        assert_eq!(m.pipeline_faults, 1);
    }

    #[test]
    fn record_processing_time_accumulates() {
        let mut m = TracingMetrics::new();
        m.record_processing_time(100);
        m.record_processing_time(200);
        assert_eq!(m.total_rt_processing_ns, 300);
    }

    #[test]
    fn record_reinitialization_increments() {
        let mut m = TracingMetrics::new();
        m.record_reinitialization();
        m.record_reinitialization();
        assert_eq!(m.reinitializations, 2);
    }

    #[test]
    fn average_processing_time_zero_events() {
        let m = TracingMetrics::new();
        assert_eq!(m.average_rt_processing_time(), Duration::ZERO);
    }

    #[test]
    fn average_processing_time_correct() {
        let mut m = TracingMetrics::new();
        m.rt_events_emitted = 4;
        m.total_rt_processing_ns = 1000;
        assert_eq!(m.average_rt_processing_time(), Duration::from_nanos(250));
    }

    #[test]
    fn drop_rate_zero_when_no_events() {
        let m = TracingMetrics::new();
        assert_eq!(m.drop_rate(), 0.0);
    }

    #[test]
    fn drop_rate_correct() {
        let mut m = TracingMetrics::new();
        m.rt_events_emitted = 90;
        m.app_events_emitted = 10;
        m.events_dropped = 10;
        // drop_rate = 10 / 100 = 0.1
        assert!((m.drop_rate() - 0.1).abs() < 0.0001);
    }

    #[test]
    fn healthy_when_clean() {
        let m = TracingMetrics::new();
        assert!(m.is_healthy());
    }

    #[test]
    fn unhealthy_with_high_drop_rate() {
        let mut m = TracingMetrics::new();
        m.rt_events_emitted = 100;
        m.events_dropped = 10; // 10% drop rate
        assert!(!m.is_healthy());
    }

    #[test]
    fn unhealthy_with_pipeline_faults() {
        let mut m = TracingMetrics::new();
        m.pipeline_faults = 1;
        assert!(!m.is_healthy());
    }

    #[test]
    fn merge_combines_metrics() {
        let mut m1 = TracingMetrics::new();
        m1.rt_events_emitted = 100;
        m1.app_events_emitted = 50;
        m1.events_dropped = 5;
        m1.deadline_misses = 2;
        m1.total_rt_processing_ns = 1000;

        let mut m2 = TracingMetrics::new();
        m2.rt_events_emitted = 200;
        m2.app_events_emitted = 30;
        m2.events_dropped = 3;
        m2.pipeline_faults = 1;
        m2.total_rt_processing_ns = 2000;
        m2.reinitializations = 1;

        m1.merge(&m2);

        assert_eq!(m1.rt_events_emitted, 300);
        assert_eq!(m1.app_events_emitted, 80);
        assert_eq!(m1.events_dropped, 8);
        assert_eq!(m1.deadline_misses, 2);
        assert_eq!(m1.pipeline_faults, 1);
        assert_eq!(m1.total_rt_processing_ns, 3000);
        assert_eq!(m1.reinitializations, 1);
    }

    #[test]
    fn reset_clears_all() {
        let mut m = TracingMetrics::new();
        m.rt_events_emitted = 999;
        m.pipeline_faults = 5;
        m.reset();

        assert_eq!(m.rt_events_emitted, 0);
        assert_eq!(m.pipeline_faults, 0);
    }

    #[test]
    fn saturating_add_at_max() {
        let mut m = TracingMetrics::new();
        m.rt_events_emitted = u64::MAX;
        m.record_rt_event();
        assert_eq!(m.rt_events_emitted, u64::MAX);
    }

    #[test]
    fn display_format_contains_key_fields() {
        let mut m = TracingMetrics::new();
        m.rt_events_emitted = 100;
        m.app_events_emitted = 50;
        m.events_dropped = 2;

        let s = format!("{m}");
        assert!(s.contains("rt=100"));
        assert!(s.contains("app=50"));
        assert!(s.contains("dropped=2"));
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Error tests
// ═══════════════════════════════════════════════════════════════════════════

mod errors {
    use super::*;

    #[test]
    fn platform_not_supported_not_recoverable() {
        let e = TracingError::PlatformNotSupported;
        assert!(!e.is_recoverable());
        assert!(e.is_platform_missing());
    }

    #[test]
    fn initialization_failed_not_recoverable() {
        let e = TracingError::InitializationFailed("test".to_string());
        assert!(!e.is_recoverable());
        assert!(!e.is_platform_missing());
    }

    #[test]
    fn emission_failed_is_recoverable() {
        let e = TracingError::EmissionFailed("transient".to_string());
        assert!(e.is_recoverable());
    }

    #[test]
    fn not_initialized_is_recoverable() {
        let e = TracingError::NotInitialized;
        assert!(e.is_recoverable());
    }

    #[test]
    fn buffer_overflow_is_recoverable() {
        let e = TracingError::BufferOverflow(42);
        assert!(e.is_recoverable());
    }

    #[test]
    fn invalid_configuration_not_recoverable() {
        let e = TracingError::InvalidConfiguration("bad".to_string());
        assert!(!e.is_recoverable());
    }

    #[test]
    fn platform_error_is_recoverable() {
        let e = TracingError::PlatformError("timeout".to_string());
        assert!(e.is_recoverable());
    }

    #[test]
    fn init_failed_constructor() {
        let e = TracingError::init_failed("context info");
        assert!(matches!(e, TracingError::InitializationFailed(_)));
        assert!(e.to_string().contains("context info"));
    }

    #[test]
    fn emit_failed_constructor() {
        let e = TracingError::emit_failed("write error");
        assert!(matches!(e, TracingError::EmissionFailed(_)));
        assert!(e.to_string().contains("write error"));
    }

    #[test]
    fn error_display_all_variants() {
        let errors: Vec<TracingError> = vec![
            TracingError::PlatformNotSupported,
            TracingError::InitializationFailed("init".to_string()),
            TracingError::EmissionFailed("emit".to_string()),
            TracingError::NotInitialized,
            TracingError::BufferOverflow(999),
            TracingError::InvalidConfiguration("cfg".to_string()),
            TracingError::PlatformError("plat".to_string()),
        ];

        for err in &errors {
            let s = err.to_string();
            assert!(!s.is_empty(), "Error display should not be empty");
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Manager tests
// ═══════════════════════════════════════════════════════════════════════════

mod manager {
    use super::*;

    #[test]
    fn manager_new_succeeds() -> R {
        let mut manager = TracingManager::with_provider(Box::new(SimpleProvider));
        manager.initialize()?;
        assert!(manager.is_enabled());
        manager.shutdown();
        Ok(())
    }

    #[test]
    fn manager_with_custom_provider() -> R {
        let mut manager = TracingManager::with_provider(Box::new(SimpleProvider));
        manager.initialize()?;
        assert!(manager.is_enabled());
        Ok(())
    }

    #[test]
    fn manager_disable_drops_events() -> R {
        let mut manager = TracingManager::with_provider(Box::new(SimpleProvider));
        manager.initialize()?;

        manager.set_enabled(false);
        assert!(!manager.is_enabled());

        // Events should be silently dropped
        manager.emit_rt_event(RTTraceEvent::TickStart {
            tick_count: 1,
            timestamp_ns: 100,
        });
        manager.emit_app_event(AppTraceEvent::TelemetryStarted {
            game_id: "test".to_string(),
            telemetry_rate_hz: 60.0,
        });
        Ok(())
    }

    #[test]
    fn manager_metrics_accessible() -> R {
        let mut manager = TracingManager::with_provider(Box::new(SimpleProvider));
        manager.initialize()?;

        let metrics = manager.metrics();
        assert_eq!(metrics.rt_events_emitted, 0);
        Ok(())
    }

    #[test]
    fn manager_debug_format() {
        let manager = TracingManager::with_provider(Box::new(SimpleProvider));
        let debug = format!("{manager:?}");
        assert!(debug.contains("TracingManager"));
        assert!(debug.contains("enabled"));
    }

    #[test]
    fn manager_default_creation() {
        // Default should not panic even if platform provider fails
        let _manager = TracingManager::default();
    }
}
