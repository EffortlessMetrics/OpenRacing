//! Simple test for tracing functionality

#[cfg(test)]
mod tests {
    use crate::tracing::*;
    use openracing_tracing::platform::FallbackProvider;

    #[test]
    fn test_tracing_manager_creation() {
        let manager = TracingManager::new();
        assert!(manager.is_ok());
    }

    #[test]
    fn test_fallback_provider() {
        let mut provider = FallbackProvider::new();
        assert!(provider.initialize().is_ok());

        // Test RT event emission
        provider.emit_rt_event(RTTraceEvent::TickStart {
            tick_count: 1,
            timestamp_ns: 1000000,
        });

        // Test app event emission
        provider.emit_app_event(AppTraceEvent::DeviceConnected {
            device_id: "test-device".to_string(),
            device_name: "Test Device".to_string(),
            capabilities: "test-caps".to_string(),
        });

        let metrics = provider.metrics();
        assert_eq!(metrics.rt_events_emitted, 0); // Fallback doesn't count events

        provider.shutdown();
    }

    #[test]
    fn test_rt_trace_events() -> Result<(), Box<dyn std::error::Error>> {
        let events = [
            RTTraceEvent::TickStart {
                tick_count: 1,
                timestamp_ns: 1000000,
            },
            RTTraceEvent::TickEnd {
                tick_count: 1,
                timestamp_ns: 1001000,
                processing_time_ns: 500,
            },
            RTTraceEvent::HidWrite {
                tick_count: 1,
                timestamp_ns: 1000500,
                torque_nm: 5.5,
                seq: 42,
            },
            RTTraceEvent::DeadlineMiss {
                tick_count: 2,
                timestamp_ns: 2000000,
                jitter_ns: 250000,
            },
            RTTraceEvent::PipelineFault {
                tick_count: 3,
                timestamp_ns: 3000000,
                error_code: 3,
            },
        ];

        let mut provider = FallbackProvider::new();
        provider.initialize()?;

        for event in events {
            provider.emit_rt_event(event);
        }
        Ok(())
    }

    #[test]
    fn test_app_trace_events() -> Result<(), Box<dyn std::error::Error>> {
        let events = [
            AppTraceEvent::DeviceConnected {
                device_id: "dev1".to_string(),
                device_name: "Device 1".to_string(),
                capabilities: "caps1".to_string(),
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
                new_state: "high_torque".to_string(),
                reason: "user_consent".to_string(),
            },
        ];

        let mut provider = FallbackProvider::new();
        provider.initialize()?;

        for event in events {
            provider.emit_app_event(event);
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_tracing_manager_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
        let mut manager = TracingManager::new()?;

        // Initialize
        manager.initialize()?;

        // Test enabling/disabling
        manager.set_enabled(false);
        manager.emit_rt_event(RTTraceEvent::TickStart {
            tick_count: 1,
            timestamp_ns: 1000000,
        });

        manager.set_enabled(true);
        manager.emit_rt_event(RTTraceEvent::TickEnd {
            tick_count: 1,
            timestamp_ns: 1001000,
            processing_time_ns: 500,
        });

        // Test app events
        manager.emit_app_event(AppTraceEvent::DeviceConnected {
            device_id: "test".to_string(),
            device_name: "Test".to_string(),
            capabilities: "test".to_string(),
        });

        // Get metrics
        let _metrics = manager.metrics();

        // Shutdown
        manager.shutdown();
        Ok(())
    }
}
