//! Tracing macros for convenient event emission

/// Emit a tick start trace event
///
/// # Example
///
/// ```rust,ignore
/// use openracing_tracing::{TracingManager, trace_rt_tick_start};
///
/// let manager = TracingManager::new()?;
/// trace_rt_tick_start!(manager, 1, 1_000_000);
/// ```
#[macro_export]
macro_rules! trace_rt_tick_start {
    ($tracer:expr, $tick_count:expr, $timestamp_ns:expr) => {
        $tracer.emit_rt_event($crate::RTTraceEvent::TickStart {
            tick_count: $tick_count,
            timestamp_ns: $timestamp_ns,
        });
    };
}

/// Emit a tick end trace event
///
/// # Example
///
/// ```rust,ignore
/// use openracing_tracing::{TracingManager, trace_rt_tick_end};
///
/// let manager = TracingManager::new()?;
/// trace_rt_tick_end!(manager, 1, 1_000_000, 500);
/// ```
#[macro_export]
macro_rules! trace_rt_tick_end {
    ($tracer:expr, $tick_count:expr, $timestamp_ns:expr, $processing_time_ns:expr) => {
        $tracer.emit_rt_event($crate::RTTraceEvent::TickEnd {
            tick_count: $tick_count,
            timestamp_ns: $timestamp_ns,
            processing_time_ns: $processing_time_ns,
        });
    };
}

/// Emit a HID write trace event
///
/// # Example
///
/// ```rust,ignore
/// use openracing_tracing::{TracingManager, trace_rt_hid_write};
///
/// let manager = TracingManager::new()?;
/// trace_rt_hid_write!(manager, 1, 1_000_000, 50.5, 42);
/// ```
#[macro_export]
macro_rules! trace_rt_hid_write {
    ($tracer:expr, $tick_count:expr, $timestamp_ns:expr, $torque_nm:expr, $seq:expr) => {
        $tracer.emit_rt_event($crate::RTTraceEvent::HidWrite {
            tick_count: $tick_count,
            timestamp_ns: $timestamp_ns,
            torque_nm: $torque_nm,
            seq: $seq,
        });
    };
}

/// Emit a deadline miss trace event
///
/// # Example
///
/// ```rust,ignore
/// use openracing_tracing::{TracingManager, trace_rt_deadline_miss};
///
/// let manager = TracingManager::new()?;
/// trace_rt_deadline_miss!(manager, 1, 1_000_000, 250_000);
/// ```
#[macro_export]
macro_rules! trace_rt_deadline_miss {
    ($tracer:expr, $tick_count:expr, $timestamp_ns:expr, $jitter_ns:expr) => {
        $tracer.emit_rt_event($crate::RTTraceEvent::DeadlineMiss {
            tick_count: $tick_count,
            timestamp_ns: $timestamp_ns,
            jitter_ns: $jitter_ns,
        });
    };
}

/// Emit a pipeline fault trace event
///
/// # Example
///
/// ```rust,ignore
/// use openracing_tracing::{TracingManager, trace_rt_pipeline_fault};
///
/// let manager = TracingManager::new()?;
/// trace_rt_pipeline_fault!(manager, 1, 1_000_000, 3);
/// ```
#[macro_export]
macro_rules! trace_rt_pipeline_fault {
    ($tracer:expr, $tick_count:expr, $timestamp_ns:expr, $error_code:expr) => {
        $tracer.emit_rt_event($crate::RTTraceEvent::PipelineFault {
            tick_count: $tick_count,
            timestamp_ns: $timestamp_ns,
            error_code: $error_code as u8,
        });
    };
}

/// Conditionally emit a trace event only when tracing is enabled
///
/// # Example
///
/// ```rust,ignore
/// use openracing_tracing::{TracingManager, trace_rt_if_enabled};
///
/// let manager = TracingManager::new()?;
/// trace_rt_if_enabled!(manager, TickStart { tick_count: 1, timestamp_ns: 1000 });
/// ```
#[macro_export]
macro_rules! trace_rt_if_enabled {
    ($tracer:expr, $event:expr) => {
        if $tracer.is_enabled() {
            $tracer.emit_rt_event($event);
        }
    };
}

/// Emit a device connected app event
///
/// # Example
///
/// ```rust,ignore
/// use openracing_tracing::{TracingManager, trace_app_device_connected};
///
/// let manager = TracingManager::new()?;
/// trace_app_device_connected!(manager, "dev1", "Test Device", "torque,rotation");
/// ```
#[macro_export]
macro_rules! trace_app_device_connected {
    ($tracer:expr, $device_id:expr, $device_name:expr, $capabilities:expr) => {
        $tracer.emit_app_event($crate::AppTraceEvent::DeviceConnected {
            device_id: $device_id.to_string(),
            device_name: $device_name.to_string(),
            capabilities: $capabilities.to_string(),
        });
    };
}

/// Emit a device disconnected app event
///
/// # Example
///
/// ```rust,ignore
/// use openracing_tracing::{TracingManager, trace_app_device_disconnected};
///
/// let manager = TracingManager::new()?;
/// trace_app_device_disconnected!(manager, "dev1", "unplugged");
/// ```
#[macro_export]
macro_rules! trace_app_device_disconnected {
    ($tracer:expr, $device_id:expr, $reason:expr) => {
        $tracer.emit_app_event($crate::AppTraceEvent::DeviceDisconnected {
            device_id: $device_id.to_string(),
            reason: $reason.to_string(),
        });
    };
}

/// Emit a safety state changed app event
///
/// # Example
///
/// ```rust,ignore
/// use openracing_tracing::{TracingManager, trace_app_safety_state_changed};
///
/// let manager = TracingManager::new()?;
/// trace_app_safety_state_changed!(manager, "dev1", "safe", "high_torque", "user_consent");
/// ```
#[macro_export]
macro_rules! trace_app_safety_state_changed {
    ($tracer:expr, $device_id:expr, $old_state:expr, $new_state:expr, $reason:expr) => {
        $tracer.emit_app_event($crate::AppTraceEvent::SafetyStateChanged {
            device_id: $device_id.to_string(),
            old_state: $old_state.to_string(),
            new_state: $new_state.to_string(),
            reason: $reason.to_string(),
        });
    };
}

#[cfg(test)]
mod tests {
    use crate::{AppTraceEvent, RTTraceEvent, TracingManager, TracingMetrics, TracingProvider};
    use std::sync::{Arc, Mutex};

    struct TestProvider {
        events: Arc<Mutex<Vec<RTTraceEvent>>>,
        app_events: Arc<Mutex<Vec<AppTraceEvent>>>,
    }

    impl TestProvider {
        fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
                app_events: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl TracingProvider for TestProvider {
        fn initialize(&mut self) -> Result<(), crate::TracingError> {
            Ok(())
        }

        fn emit_rt_event(&self, event: RTTraceEvent) {
            if let Ok(mut e) = self.events.lock() {
                e.push(event);
            }
        }

        fn emit_app_event(&self, event: AppTraceEvent) {
            if let Ok(mut e) = self.app_events.lock() {
                e.push(event);
            }
        }

        fn metrics(&self) -> TracingMetrics {
            TracingMetrics::default()
        }

        fn shutdown(&mut self) {}
    }

    #[test]
    fn test_trace_macros() {
        let provider = TestProvider::new();
        let events = provider.events.clone();
        let app_events = provider.app_events.clone();

        let manager = TracingManager::with_provider(Box::new(provider));

        trace_rt_tick_start!(manager, 1, 1000);
        trace_rt_tick_end!(manager, 1, 2000, 500);
        trace_rt_hid_write!(manager, 1, 1500, 50.0, 42);
        trace_rt_deadline_miss!(manager, 2, 3000, 250);
        trace_rt_pipeline_fault!(manager, 3, 4000, 5);

        let guard = events.lock().expect("lock");
        assert_eq!(guard.len(), 5);
        drop(guard);

        trace_app_device_connected!(manager, "dev1", "Test", "caps");
        trace_app_device_disconnected!(manager, "dev1", "reason");
        trace_app_safety_state_changed!(manager, "dev1", "old", "new", "reason");

        let guard = app_events.lock().expect("lock");
        assert_eq!(guard.len(), 3);
    }
}
