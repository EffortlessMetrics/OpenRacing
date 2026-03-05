//! Deep tests for tracing and observability: metric collection, trace event
//! filtering, log level handling, health check integration, and output formatting.

use openracing_tracing::{
    AppEventCategory, AppTraceEvent, RTEventCategory, RTTraceEvent, TracingError, TracingManager,
    TracingMetrics, TracingProvider,
};
use std::sync::{Arc, Mutex};

// ═══════════════════════════════════════════════════════════════════════════
// Test provider that captures all events with full metrics tracking
// ═══════════════════════════════════════════════════════════════════════════

struct CapturingProvider {
    rt_events: Arc<Mutex<Vec<RTTraceEvent>>>,
    app_events: Arc<Mutex<Vec<AppTraceEvent>>>,
    metrics: Arc<Mutex<TracingMetrics>>,
    enabled: bool,
}

impl CapturingProvider {
    fn new() -> Self {
        Self {
            rt_events: Arc::new(Mutex::new(Vec::new())),
            app_events: Arc::new(Mutex::new(Vec::new())),
            metrics: Arc::new(Mutex::new(TracingMetrics::new())),
            enabled: true,
        }
    }

    fn rt_events(&self) -> Arc<Mutex<Vec<RTTraceEvent>>> {
        Arc::clone(&self.rt_events)
    }

    fn app_events(&self) -> Arc<Mutex<Vec<AppTraceEvent>>> {
        Arc::clone(&self.app_events)
    }

    fn metrics_handle(&self) -> Arc<Mutex<TracingMetrics>> {
        Arc::clone(&self.metrics)
    }
}

impl TracingProvider for CapturingProvider {
    fn initialize(&mut self) -> Result<(), TracingError> {
        Ok(())
    }

    fn emit_rt_event(&self, event: RTTraceEvent) {
        if let Ok(mut events) = self.rt_events.lock() {
            events.push(event);
        }
        if let Ok(mut m) = self.metrics.lock() {
            m.record_rt_event();
            match event {
                RTTraceEvent::TickEnd {
                    processing_time_ns, ..
                } => m.record_processing_time(processing_time_ns),
                RTTraceEvent::DeadlineMiss { .. } => m.record_deadline_miss(),
                RTTraceEvent::PipelineFault { .. } => m.record_pipeline_fault(),
                _ => {}
            }
        }
    }

    fn emit_app_event(&self, event: AppTraceEvent) {
        if let Ok(mut events) = self.app_events.lock() {
            events.push(event);
        }
        if let Ok(mut m) = self.metrics.lock() {
            m.record_app_event();
        }
    }

    fn metrics(&self) -> TracingMetrics {
        self.metrics.lock().map(|m| m.clone()).unwrap_or_default()
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn shutdown(&mut self) {
        self.enabled = false;
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. Metric collection – counters, gauges via TracingMetrics
// ═══════════════════════════════════════════════════════════════════════════

mod metrics_collection {
    use super::*;

    #[test]
    fn counter_rt_events_increments() -> Result<(), TracingError> {
        let mut m = TracingMetrics::new();
        assert_eq!(m.rt_events_emitted, 0);

        for _ in 0..100 {
            m.record_rt_event();
        }
        assert_eq!(m.rt_events_emitted, 100);
        Ok(())
    }

    #[test]
    fn counter_app_events_increments() -> Result<(), TracingError> {
        let mut m = TracingMetrics::new();
        for _ in 0..50 {
            m.record_app_event();
        }
        assert_eq!(m.app_events_emitted, 50);
        Ok(())
    }

    #[test]
    fn counter_dropped_events_tracks_losses() -> Result<(), TracingError> {
        let mut m = TracingMetrics::new();
        for _ in 0..10 {
            m.record_dropped_event();
        }
        assert_eq!(m.events_dropped, 10);
        Ok(())
    }

    #[test]
    fn gauge_deadline_misses_accumulated() -> Result<(), TracingError> {
        let mut m = TracingMetrics::new();
        m.record_deadline_miss();
        m.record_deadline_miss();
        m.record_deadline_miss();
        assert_eq!(m.deadline_misses, 3);
        Ok(())
    }

    #[test]
    fn gauge_pipeline_faults_accumulated() -> Result<(), TracingError> {
        let mut m = TracingMetrics::new();
        m.record_pipeline_fault();
        assert_eq!(m.pipeline_faults, 1);
        assert!(!m.is_healthy());
        Ok(())
    }

    #[test]
    fn histogram_processing_time_accumulated() -> Result<(), TracingError> {
        let mut m = TracingMetrics::new();
        m.record_rt_event();
        m.record_processing_time(500);
        m.record_rt_event();
        m.record_processing_time(300);

        assert_eq!(m.total_rt_processing_ns, 800);
        let avg = m.average_rt_processing_time();
        assert_eq!(avg.as_nanos(), 400);
        Ok(())
    }

    #[test]
    fn average_processing_time_zero_when_no_events() -> Result<(), TracingError> {
        let m = TracingMetrics::new();
        assert_eq!(m.average_rt_processing_time(), std::time::Duration::ZERO);
        Ok(())
    }

    #[test]
    fn reinitialization_counter_tracks_restarts() -> Result<(), TracingError> {
        let mut m = TracingMetrics::new();
        m.record_reinitialization();
        m.record_reinitialization();
        assert_eq!(m.reinitializations, 2);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. Metric overflow / saturation handling
// ═══════════════════════════════════════════════════════════════════════════

mod metrics_overflow {
    use super::*;

    #[test]
    fn rt_event_counter_saturates_at_max() -> Result<(), TracingError> {
        let mut m = TracingMetrics::new();
        m.rt_events_emitted = u64::MAX;
        m.record_rt_event();
        assert_eq!(m.rt_events_emitted, u64::MAX);
        Ok(())
    }

    #[test]
    fn app_event_counter_saturates_at_max() -> Result<(), TracingError> {
        let mut m = TracingMetrics::new();
        m.app_events_emitted = u64::MAX;
        m.record_app_event();
        assert_eq!(m.app_events_emitted, u64::MAX);
        Ok(())
    }

    #[test]
    fn dropped_counter_saturates_at_max() -> Result<(), TracingError> {
        let mut m = TracingMetrics::new();
        m.events_dropped = u64::MAX;
        m.record_dropped_event();
        assert_eq!(m.events_dropped, u64::MAX);
        Ok(())
    }

    #[test]
    fn processing_time_saturates_at_max() -> Result<(), TracingError> {
        let mut m = TracingMetrics::new();
        m.total_rt_processing_ns = u64::MAX;
        m.record_processing_time(1);
        assert_eq!(m.total_rt_processing_ns, u64::MAX);
        Ok(())
    }

    #[test]
    fn deadline_miss_counter_saturates_at_max() -> Result<(), TracingError> {
        let mut m = TracingMetrics::new();
        m.deadline_misses = u64::MAX;
        m.record_deadline_miss();
        assert_eq!(m.deadline_misses, u64::MAX);
        Ok(())
    }

    #[test]
    fn pipeline_fault_counter_saturates_at_max() -> Result<(), TracingError> {
        let mut m = TracingMetrics::new();
        m.pipeline_faults = u64::MAX;
        m.record_pipeline_fault();
        assert_eq!(m.pipeline_faults, u64::MAX);
        Ok(())
    }

    #[test]
    fn reinitialization_counter_saturates_at_max() -> Result<(), TracingError> {
        let mut m = TracingMetrics::new();
        m.reinitializations = u64::MAX;
        m.record_reinitialization();
        assert_eq!(m.reinitializations, u64::MAX);
        Ok(())
    }

    #[test]
    fn merge_saturates_instead_of_overflowing() -> Result<(), TracingError> {
        let mut m1 = TracingMetrics::new();
        m1.rt_events_emitted = u64::MAX - 10;

        let mut m2 = TracingMetrics::new();
        m2.rt_events_emitted = 100;

        m1.merge(&m2);
        assert_eq!(m1.rt_events_emitted, u64::MAX);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. Trace event recording and filtering via TracingManager
// ═══════════════════════════════════════════════════════════════════════════

mod event_recording {
    use super::*;

    #[test]
    fn manager_records_all_rt_event_types() -> Result<(), TracingError> {
        let provider = CapturingProvider::new();
        let rt = provider.rt_events();
        let mut manager = TracingManager::with_provider(Box::new(provider));
        manager.initialize()?;

        let events = vec![
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
                torque_nm: 5.0,
                seq: 1,
            },
            RTTraceEvent::DeadlineMiss {
                tick_count: 2,
                timestamp_ns: 3000,
                jitter_ns: 100,
            },
            RTTraceEvent::PipelineFault {
                tick_count: 3,
                timestamp_ns: 4000,
                error_code: 42,
            },
        ];

        for event in &events {
            manager.emit_rt_event(*event);
        }

        let captured = rt.lock().map_err(|_| TracingError::PlatformNotSupported)?;
        assert_eq!(captured.len(), 5);
        assert_eq!(captured[0], events[0]);
        assert_eq!(captured[4], events[4]);
        Ok(())
    }

    #[test]
    fn manager_records_all_app_event_types() -> Result<(), TracingError> {
        let provider = CapturingProvider::new();
        let app = provider.app_events();
        let mut manager = TracingManager::with_provider(Box::new(provider));
        manager.initialize()?;

        manager.emit_app_event(AppTraceEvent::DeviceConnected {
            device_id: "dev-1".to_string(),
            device_name: "Wheel".to_string(),
            capabilities: "ffb,pedals".to_string(),
        });
        manager.emit_app_event(AppTraceEvent::DeviceDisconnected {
            device_id: "dev-1".to_string(),
            reason: "unplugged".to_string(),
        });
        manager.emit_app_event(AppTraceEvent::TelemetryStarted {
            game_id: "iracing".to_string(),
            telemetry_rate_hz: 60.0,
        });
        manager.emit_app_event(AppTraceEvent::ProfileApplied {
            device_id: "dev-1".to_string(),
            profile_name: "drift".to_string(),
            profile_hash: "abc123".to_string(),
        });
        manager.emit_app_event(AppTraceEvent::SafetyStateChanged {
            device_id: "dev-1".to_string(),
            old_state: "SafeTorque".to_string(),
            new_state: "HighTorqueActive".to_string(),
            reason: "challenge_passed".to_string(),
        });

        let captured = app.lock().map_err(|_| TracingError::PlatformNotSupported)?;
        assert_eq!(captured.len(), 5);
        Ok(())
    }

    #[test]
    fn disabled_manager_drops_events() -> Result<(), TracingError> {
        let provider = CapturingProvider::new();
        let rt = provider.rt_events();
        let mut manager = TracingManager::with_provider(Box::new(provider));
        manager.set_enabled(false);

        manager.emit_rt_event(RTTraceEvent::TickStart {
            tick_count: 1,
            timestamp_ns: 1000,
        });

        let captured = rt.lock().map_err(|_| TracingError::PlatformNotSupported)?;
        assert_eq!(captured.len(), 0);
        Ok(())
    }

    #[test]
    fn re_enable_resumes_recording() -> Result<(), TracingError> {
        let provider = CapturingProvider::new();
        let rt = provider.rt_events();
        let mut manager = TracingManager::with_provider(Box::new(provider));

        manager.set_enabled(false);
        manager.emit_rt_event(RTTraceEvent::TickStart {
            tick_count: 1,
            timestamp_ns: 1000,
        });

        manager.set_enabled(true);
        manager.emit_rt_event(RTTraceEvent::TickStart {
            tick_count: 2,
            timestamp_ns: 2000,
        });

        let captured = rt.lock().map_err(|_| TracingError::PlatformNotSupported)?;
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].tick_count(), 2);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. Event category filtering
// ═══════════════════════════════════════════════════════════════════════════

mod event_filtering {
    use super::*;

    #[test]
    fn rt_timing_events_have_timing_category() -> Result<(), TracingError> {
        let start = RTTraceEvent::TickStart {
            tick_count: 0,
            timestamp_ns: 0,
        };
        let end = RTTraceEvent::TickEnd {
            tick_count: 0,
            timestamp_ns: 0,
            processing_time_ns: 0,
        };

        assert_eq!(start.category(), RTEventCategory::Timing);
        assert_eq!(end.category(), RTEventCategory::Timing);
        assert!(!start.is_error());
        assert!(!end.is_error());
        Ok(())
    }

    #[test]
    fn rt_hid_events_have_hid_category() -> Result<(), TracingError> {
        let hid = RTTraceEvent::HidWrite {
            tick_count: 0,
            timestamp_ns: 0,
            torque_nm: 0.0,
            seq: 0,
        };
        assert_eq!(hid.category(), RTEventCategory::Hid);
        assert!(!hid.is_error());
        Ok(())
    }

    #[test]
    fn rt_error_events_have_error_category() -> Result<(), TracingError> {
        let miss = RTTraceEvent::DeadlineMiss {
            tick_count: 0,
            timestamp_ns: 0,
            jitter_ns: 0,
        };
        let fault = RTTraceEvent::PipelineFault {
            tick_count: 0,
            timestamp_ns: 0,
            error_code: 0,
        };

        assert_eq!(miss.category(), RTEventCategory::Error);
        assert_eq!(fault.category(), RTEventCategory::Error);
        assert!(miss.is_error());
        assert!(fault.is_error());
        Ok(())
    }

    #[test]
    fn app_event_categories_correctly_classified() -> Result<(), TracingError> {
        let connected = AppTraceEvent::DeviceConnected {
            device_id: "d".to_string(),
            device_name: "n".to_string(),
            capabilities: "c".to_string(),
        };
        let disconnected = AppTraceEvent::DeviceDisconnected {
            device_id: "d".to_string(),
            reason: "r".to_string(),
        };
        let telemetry = AppTraceEvent::TelemetryStarted {
            game_id: "g".to_string(),
            telemetry_rate_hz: 60.0,
        };
        let profile = AppTraceEvent::ProfileApplied {
            device_id: "d".to_string(),
            profile_name: "p".to_string(),
            profile_hash: "h".to_string(),
        };
        let safety = AppTraceEvent::SafetyStateChanged {
            device_id: "d".to_string(),
            old_state: "a".to_string(),
            new_state: "b".to_string(),
            reason: "r".to_string(),
        };

        assert_eq!(connected.category(), AppEventCategory::Device);
        assert_eq!(disconnected.category(), AppEventCategory::Device);
        assert_eq!(telemetry.category(), AppEventCategory::Telemetry);
        assert_eq!(profile.category(), AppEventCategory::Profile);
        assert_eq!(safety.category(), AppEventCategory::Safety);
        Ok(())
    }

    #[test]
    fn app_event_device_id_returns_correct_values() -> Result<(), TracingError> {
        let connected = AppTraceEvent::DeviceConnected {
            device_id: "wheel-1".to_string(),
            device_name: "G29".to_string(),
            capabilities: "ffb".to_string(),
        };
        assert_eq!(connected.device_id(), Some("wheel-1"));

        let telemetry = AppTraceEvent::TelemetryStarted {
            game_id: "iracing".to_string(),
            telemetry_rate_hz: 60.0,
        };
        assert_eq!(telemetry.device_id(), None);
        Ok(())
    }

    #[test]
    fn filter_rt_events_by_category() -> Result<(), TracingError> {
        let provider = CapturingProvider::new();
        let rt = provider.rt_events();
        let mut manager = TracingManager::with_provider(Box::new(provider));
        manager.initialize()?;

        // Emit mixed events
        for i in 0..10u64 {
            manager.emit_rt_event(RTTraceEvent::TickStart {
                tick_count: i,
                timestamp_ns: i * 1000,
            });
            if i % 3 == 0 {
                manager.emit_rt_event(RTTraceEvent::DeadlineMiss {
                    tick_count: i,
                    timestamp_ns: i * 1000,
                    jitter_ns: 200,
                });
            }
        }

        let captured = rt.lock().map_err(|_| TracingError::PlatformNotSupported)?;
        let error_events: Vec<_> = captured.iter().filter(|e| e.is_error()).collect();
        let timing_events: Vec<_> = captured
            .iter()
            .filter(|e| e.category() == RTEventCategory::Timing)
            .collect();

        assert_eq!(error_events.len(), 4); // i=0,3,6,9
        assert_eq!(timing_events.len(), 10);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. Output formatting (Display)
// ═══════════════════════════════════════════════════════════════════════════

mod output_formatting {
    use super::*;

    #[test]
    fn rt_tick_start_display_format() -> Result<(), TracingError> {
        let event = RTTraceEvent::TickStart {
            tick_count: 42,
            timestamp_ns: 1_000_000,
        };
        let s = format!("{event}");
        assert!(s.contains("TickStart"));
        assert!(s.contains("42"));
        assert!(s.contains("1000000"));
        Ok(())
    }

    #[test]
    fn rt_tick_end_display_format() -> Result<(), TracingError> {
        let event = RTTraceEvent::TickEnd {
            tick_count: 100,
            timestamp_ns: 2_000_000,
            processing_time_ns: 500,
        };
        let s = format!("{event}");
        assert!(s.contains("TickEnd"));
        assert!(s.contains("proc=500ns"));
        Ok(())
    }

    #[test]
    fn rt_hid_write_display_format() -> Result<(), TracingError> {
        let event = RTTraceEvent::HidWrite {
            tick_count: 1,
            timestamp_ns: 1000,
            torque_nm: 7.5,
            seq: 99,
        };
        let s = format!("{event}");
        assert!(s.contains("HidWrite"));
        assert!(s.contains("torque=7.5"));
        assert!(s.contains("seq=99"));
        Ok(())
    }

    #[test]
    fn rt_deadline_miss_display_format() -> Result<(), TracingError> {
        let event = RTTraceEvent::DeadlineMiss {
            tick_count: 5,
            timestamp_ns: 5000,
            jitter_ns: 250_000,
        };
        let s = format!("{event}");
        assert!(s.contains("DeadlineMiss"));
        assert!(s.contains("jitter=250000ns"));
        Ok(())
    }

    #[test]
    fn rt_pipeline_fault_display_format() -> Result<(), TracingError> {
        let event = RTTraceEvent::PipelineFault {
            tick_count: 10,
            timestamp_ns: 10_000,
            error_code: 255,
        };
        let s = format!("{event}");
        assert!(s.contains("PipelineFault"));
        assert!(s.contains("error=255"));
        Ok(())
    }

    #[test]
    fn app_event_display_all_variants() -> Result<(), TracingError> {
        let events: Vec<AppTraceEvent> = vec![
            AppTraceEvent::DeviceConnected {
                device_id: "dev-1".to_string(),
                device_name: "G29".to_string(),
                capabilities: "ffb".to_string(),
            },
            AppTraceEvent::DeviceDisconnected {
                device_id: "dev-1".to_string(),
                reason: "timeout".to_string(),
            },
            AppTraceEvent::TelemetryStarted {
                game_id: "ac".to_string(),
                telemetry_rate_hz: 60.0,
            },
            AppTraceEvent::ProfileApplied {
                device_id: "dev-1".to_string(),
                profile_name: "drift".to_string(),
                profile_hash: "abc".to_string(),
            },
            AppTraceEvent::SafetyStateChanged {
                device_id: "dev-1".to_string(),
                old_state: "Safe".to_string(),
                new_state: "Active".to_string(),
                reason: "ack".to_string(),
            },
        ];

        for event in &events {
            let s = format!("{event}");
            assert!(!s.is_empty());
        }
        Ok(())
    }

    #[test]
    fn metrics_display_includes_key_counters() -> Result<(), TracingError> {
        let mut m = TracingMetrics::new();
        m.rt_events_emitted = 1000;
        m.app_events_emitted = 50;
        m.events_dropped = 5;
        m.deadline_misses = 2;
        m.pipeline_faults = 0;

        let s = format!("{m}");
        assert!(s.contains("rt=1000"));
        assert!(s.contains("app=50"));
        assert!(s.contains("dropped=5"));
        assert!(s.contains("misses=2"));
        assert!(s.contains("faults=0"));
        Ok(())
    }

    #[test]
    fn rt_event_type_strings_are_stable() -> Result<(), TracingError> {
        assert_eq!(
            RTTraceEvent::TickStart {
                tick_count: 0,
                timestamp_ns: 0
            }
            .event_type(),
            "tick_start"
        );
        assert_eq!(
            RTTraceEvent::TickEnd {
                tick_count: 0,
                timestamp_ns: 0,
                processing_time_ns: 0
            }
            .event_type(),
            "tick_end"
        );
        assert_eq!(
            RTTraceEvent::HidWrite {
                tick_count: 0,
                timestamp_ns: 0,
                torque_nm: 0.0,
                seq: 0
            }
            .event_type(),
            "hid_write"
        );
        assert_eq!(
            RTTraceEvent::DeadlineMiss {
                tick_count: 0,
                timestamp_ns: 0,
                jitter_ns: 0
            }
            .event_type(),
            "deadline_miss"
        );
        assert_eq!(
            RTTraceEvent::PipelineFault {
                tick_count: 0,
                timestamp_ns: 0,
                error_code: 0
            }
            .event_type(),
            "pipeline_fault"
        );
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. Health check via TracingMetrics
// ═══════════════════════════════════════════════════════════════════════════

mod health_checks {
    use super::*;

    #[test]
    fn healthy_when_no_events() -> Result<(), TracingError> {
        let m = TracingMetrics::new();
        assert!(m.is_healthy());
        Ok(())
    }

    #[test]
    fn healthy_with_low_drop_rate() -> Result<(), TracingError> {
        let mut m = TracingMetrics::new();
        m.rt_events_emitted = 10_000;
        m.events_dropped = 50; // 0.5% < 1%
        assert!(m.is_healthy());
        Ok(())
    }

    #[test]
    fn unhealthy_with_high_drop_rate() -> Result<(), TracingError> {
        let mut m = TracingMetrics::new();
        m.rt_events_emitted = 100;
        m.events_dropped = 5; // 5% > 1%
        assert!(!m.is_healthy());
        Ok(())
    }

    #[test]
    fn unhealthy_with_pipeline_faults() -> Result<(), TracingError> {
        let mut m = TracingMetrics::new();
        m.rt_events_emitted = 10_000;
        m.pipeline_faults = 1;
        assert!(!m.is_healthy());
        Ok(())
    }

    #[test]
    fn health_restored_after_reset() -> Result<(), TracingError> {
        let mut m = TracingMetrics::new();
        m.pipeline_faults = 5;
        m.events_dropped = 1000;
        assert!(!m.is_healthy());

        m.reset();
        assert!(m.is_healthy());
        assert_eq!(m.pipeline_faults, 0);
        assert_eq!(m.events_dropped, 0);
        assert_eq!(m.rt_events_emitted, 0);
        Ok(())
    }

    #[test]
    fn drop_rate_zero_when_no_events() -> Result<(), TracingError> {
        let m = TracingMetrics::new();
        assert!((m.drop_rate() - 0.0).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    fn drop_rate_includes_both_rt_and_app_events() -> Result<(), TracingError> {
        let mut m = TracingMetrics::new();
        m.rt_events_emitted = 80;
        m.app_events_emitted = 20;
        m.events_dropped = 10; // 10 / 100 = 0.10

        assert!((m.drop_rate() - 0.10).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn health_degradation_detected_via_manager() -> Result<(), TracingError> {
        let provider = CapturingProvider::new();
        let metrics_handle = provider.metrics_handle();
        let mut manager = TracingManager::with_provider(Box::new(provider));
        manager.initialize()?;

        // Emit normal events
        for i in 0..100u64 {
            manager.emit_rt_event(RTTraceEvent::TickStart {
                tick_count: i,
                timestamp_ns: i * 1000,
            });
        }

        let metrics = manager.metrics();
        assert!(metrics.is_healthy());

        // Simulate degradation by injecting faults
        if let Ok(mut m) = metrics_handle.lock() {
            m.record_pipeline_fault();
        }

        let metrics = manager.metrics();
        assert!(!metrics.is_healthy());
        Ok(())
    }

    #[test]
    fn merge_preserves_health_state() -> Result<(), TracingError> {
        let mut healthy = TracingMetrics::new();
        healthy.rt_events_emitted = 1000;

        let mut unhealthy = TracingMetrics::new();
        unhealthy.pipeline_faults = 1;

        healthy.merge(&unhealthy);
        assert!(!healthy.is_healthy(), "merged metrics should be unhealthy");
        Ok(())
    }

    #[test]
    fn manager_lifecycle_initialize_and_shutdown() -> Result<(), TracingError> {
        let provider = CapturingProvider::new();
        let mut manager = TracingManager::with_provider(Box::new(provider));

        assert!(manager.is_enabled());
        manager.initialize()?;

        manager.emit_rt_event(RTTraceEvent::TickStart {
            tick_count: 0,
            timestamp_ns: 0,
        });

        manager.shutdown();
        Ok(())
    }

    #[test]
    fn manager_debug_format_includes_type() -> Result<(), TracingError> {
        let provider = CapturingProvider::new();
        let manager = TracingManager::with_provider(Box::new(provider));
        let dbg = format!("{manager:?}");
        assert!(dbg.contains("TracingManager"));
        assert!(dbg.contains("enabled"));
        Ok(())
    }

    #[test]
    fn default_manager_creation() -> Result<(), TracingError> {
        let manager = TracingManager::default();
        let dbg = format!("{manager:?}");
        assert!(dbg.contains("TracingManager"));
        Ok(())
    }
}
