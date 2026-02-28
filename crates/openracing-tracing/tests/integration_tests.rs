//! Integration tests for openracing-tracing

use openracing_tracing::{
    AppTraceEvent, RTTraceEvent, TracingManager, TracingMetrics, TracingProvider,
    trace_rt_deadline_miss, trace_rt_hid_write, trace_rt_pipeline_fault, trace_rt_tick_end,
    trace_rt_tick_start,
};
use std::sync::{Arc, Mutex};

struct MockProvider {
    rt_events: Arc<Mutex<Vec<RTTraceEvent>>>,
    app_events: Arc<Mutex<Vec<AppTraceEvent>>>,
    initialized: bool,
}

impl MockProvider {
    fn new() -> Self {
        Self {
            rt_events: Arc::new(Mutex::new(Vec::new())),
            app_events: Arc::new(Mutex::new(Vec::new())),
            initialized: false,
        }
    }
}

impl TracingProvider for MockProvider {
    fn initialize(&mut self) -> Result<(), openracing_tracing::TracingError> {
        self.initialized = true;
        Ok(())
    }

    fn emit_rt_event(&self, event: RTTraceEvent) {
        if let Ok(mut events) = self.rt_events.lock() {
            events.push(event);
        }
    }

    fn emit_app_event(&self, event: AppTraceEvent) {
        if let Ok(mut events) = self.app_events.lock() {
            events.push(event);
        }
    }

    fn metrics(&self) -> TracingMetrics {
        TracingMetrics::default()
    }

    fn shutdown(&mut self) {
        self.initialized = false;
    }
}

#[test]
fn test_tracing_manager_full_lifecycle() {
    let provider = MockProvider::new();
    let rt_events = provider.rt_events.clone();

    let mut manager = TracingManager::with_provider(Box::new(provider));

    manager.initialize().expect("initialization failed");
    assert!(manager.is_enabled());

    trace_rt_tick_start!(manager, 1, 1_000_000);
    trace_rt_tick_end!(manager, 1, 1_000_100, 100);
    trace_rt_hid_write!(manager, 1, 1_000_050, 50.0, 42);
    trace_rt_deadline_miss!(manager, 2, 2_000_000, 250_000);
    trace_rt_pipeline_fault!(manager, 3, 3_000_000, 5);

    let guard = rt_events.lock().expect("lock");
    assert_eq!(guard.len(), 5);
    drop(guard);

    manager.shutdown();
}

#[test]
fn test_tracing_manager_disabled_drops_events() {
    let provider = MockProvider::new();
    let rt_events = provider.rt_events.clone();

    let mut manager = TracingManager::with_provider(Box::new(provider));
    manager.initialize().ok();

    manager.emit_rt_event(RTTraceEvent::TickStart {
        tick_count: 1,
        timestamp_ns: 1000,
    });

    assert_eq!(rt_events.lock().map(|e| e.len()).unwrap_or(0), 1);

    manager.set_enabled(false);

    manager.emit_rt_event(RTTraceEvent::TickStart {
        tick_count: 2,
        timestamp_ns: 2000,
    });

    assert_eq!(rt_events.lock().map(|e| e.len()).unwrap_or(0), 1);
}

#[test]
fn test_all_rt_event_types() {
    let provider = MockProvider::new();
    let rt_events = provider.rt_events.clone();

    let manager = TracingManager::with_provider(Box::new(provider));

    let events = [
        RTTraceEvent::TickStart {
            tick_count: 1,
            timestamp_ns: 1000,
        },
        RTTraceEvent::TickEnd {
            tick_count: 1,
            timestamp_ns: 2000,
            processing_time_ns: 500,
        },
        RTTraceEvent::HidWrite {
            tick_count: 1,
            timestamp_ns: 1500,
            torque_nm: 50.0,
            seq: 1,
        },
        RTTraceEvent::DeadlineMiss {
            tick_count: 2,
            timestamp_ns: 3000,
            jitter_ns: 250,
        },
        RTTraceEvent::PipelineFault {
            tick_count: 3,
            timestamp_ns: 4000,
            error_code: 5,
        },
    ];

    for event in events {
        manager.emit_rt_event(event);
    }

    let guard = rt_events.lock().expect("lock");
    assert_eq!(guard.len(), 5);
}

#[test]
fn test_all_app_event_types() {
    let provider = MockProvider::new();
    let app_events = provider.app_events.clone();

    let manager = TracingManager::with_provider(Box::new(provider));

    let events = [
        AppTraceEvent::DeviceConnected {
            device_id: "dev1".to_string(),
            device_name: "Test Device".to_string(),
            capabilities: "torque,rotation".to_string(),
        },
        AppTraceEvent::DeviceDisconnected {
            device_id: "dev1".to_string(),
            reason: "unplugged".to_string(),
        },
        AppTraceEvent::TelemetryStarted {
            game_id: "iracing".to_string(),
            telemetry_rate_hz: 60.0,
        },
        AppTraceEvent::ProfileApplied {
            device_id: "dev1".to_string(),
            profile_name: "gt3".to_string(),
            profile_hash: "abc123".to_string(),
        },
        AppTraceEvent::SafetyStateChanged {
            device_id: "dev1".to_string(),
            old_state: "safe".to_string(),
            new_state: "warning".to_string(),
            reason: "high_torque".to_string(),
        },
    ];

    for event in events {
        manager.emit_app_event(event);
    }

    let guard = app_events.lock().expect("lock");
    assert_eq!(guard.len(), 5);
}

#[test]
fn test_platform_provider_creation() {
    let result = TracingManager::new();
    assert!(result.is_ok());
}

#[test]
fn test_event_accessors() {
    let event = RTTraceEvent::TickEnd {
        tick_count: 42,
        timestamp_ns: 12345,
        processing_time_ns: 500,
    };

    assert_eq!(event.tick_count(), 42);
    assert_eq!(event.timestamp_ns(), 12345);
    assert!(!event.is_error());

    let error_event = RTTraceEvent::DeadlineMiss {
        tick_count: 1,
        timestamp_ns: 1000,
        jitter_ns: 100,
    };
    assert!(error_event.is_error());
}

#[test]
fn test_event_display() {
    let event = RTTraceEvent::HidWrite {
        tick_count: 1,
        timestamp_ns: 1000,
        torque_nm: 50.0,
        seq: 42,
    };

    let s = format!("{}", event);
    assert!(s.contains("HidWrite"));
    assert!(s.contains("torque=50"));
    assert!(s.contains("seq=42"));
}
