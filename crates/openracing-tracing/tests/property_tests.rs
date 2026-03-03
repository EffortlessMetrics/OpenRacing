//! Property-based tests for openracing-tracing

#![allow(clippy::redundant_closure)]

use openracing_tracing::{AppEventCategory, AppTraceEvent, RTEventCategory, RTTraceEvent};
use proptest::prelude::*;

prop_compose! {
    fn arb_rt_event()(
        tick_count in 0u64..=u64::MAX / 2,
        timestamp_ns in 0u64..=u64::MAX / 2,
    ) -> RTTraceEvent {
        RTTraceEvent::TickStart { tick_count, timestamp_ns }
    }
}

prop_compose! {
    fn arb_tick_end()(
        tick_count in 0u64..=u64::MAX / 2,
        timestamp_ns in 0u64..=u64::MAX / 2,
        processing_time_ns in 0u64..=1_000_000u64,
    ) -> RTTraceEvent {
        RTTraceEvent::TickEnd { tick_count, timestamp_ns, processing_time_ns }
    }
}

prop_compose! {
    fn arb_hid_write()(
        tick_count in 0u64..=u64::MAX / 2,
        timestamp_ns in 0u64..=u64::MAX / 2,
        torque_nm in 0.0f32..=50.0f32,
        seq in 0u16..=u16::MAX,
    ) -> RTTraceEvent {
        RTTraceEvent::HidWrite { tick_count, timestamp_ns, torque_nm, seq }
    }
}

prop_compose! {
    fn arb_deadline_miss()(
        tick_count in 0u64..=u64::MAX / 2,
        timestamp_ns in 0u64..=u64::MAX / 2,
        jitter_ns in 0u64..=10_000_000u64,
    ) -> RTTraceEvent {
        RTTraceEvent::DeadlineMiss { tick_count, timestamp_ns, jitter_ns }
    }
}

prop_compose! {
    fn arb_pipeline_fault()(
        tick_count in 0u64..=u64::MAX / 2,
        timestamp_ns in 0u64..=u64::MAX / 2,
        error_code in 0u8..=255u8,
    ) -> RTTraceEvent {
        RTTraceEvent::PipelineFault { tick_count, timestamp_ns, error_code }
    }
}

fn arb_rt_event_full() -> impl Strategy<Value = RTTraceEvent> {
    prop_oneof![
        arb_rt_event(),
        arb_tick_end(),
        arb_hid_write(),
        arb_deadline_miss(),
        arb_pipeline_fault(),
    ]
}

proptest! {
    #[test]
    fn test_rt_event_tick_count_consistency(event in arb_rt_event_full()) {
        let tick = event.tick_count();
        match event {
            RTTraceEvent::TickStart { tick_count, .. } => prop_assert_eq!(tick, tick_count),
            RTTraceEvent::TickEnd { tick_count, .. } => prop_assert_eq!(tick, tick_count),
            RTTraceEvent::HidWrite { tick_count, .. } => prop_assert_eq!(tick, tick_count),
            RTTraceEvent::DeadlineMiss { tick_count, .. } => prop_assert_eq!(tick, tick_count),
            RTTraceEvent::PipelineFault { tick_count, .. } => prop_assert_eq!(tick, tick_count),
        }
    }

    #[test]
    fn test_rt_event_timestamp_consistency(event in arb_rt_event_full()) {
        let ts = event.timestamp_ns();
        match event {
            RTTraceEvent::TickStart { timestamp_ns, .. } => prop_assert_eq!(ts, timestamp_ns),
            RTTraceEvent::TickEnd { timestamp_ns, .. } => prop_assert_eq!(ts, timestamp_ns),
            RTTraceEvent::HidWrite { timestamp_ns, .. } => prop_assert_eq!(ts, timestamp_ns),
            RTTraceEvent::DeadlineMiss { timestamp_ns, .. } => prop_assert_eq!(ts, timestamp_ns),
            RTTraceEvent::PipelineFault { timestamp_ns, .. } => prop_assert_eq!(ts, timestamp_ns),
        }
    }

    #[test]
    fn test_rt_event_category_consistency(event in arb_rt_event_full()) {
        let category = event.category();
        match event {
            RTTraceEvent::TickStart { .. } | RTTraceEvent::TickEnd { .. } => {
                prop_assert_eq!(category, RTEventCategory::Timing)
            }
            RTTraceEvent::HidWrite { .. } => prop_assert_eq!(category, RTEventCategory::Hid),
            RTTraceEvent::DeadlineMiss { .. } | RTTraceEvent::PipelineFault { .. } => {
                prop_assert_eq!(category, RTEventCategory::Error)
            }
        }
    }

    #[test]
    fn test_rt_event_error_flag(event in arb_rt_event_full()) {
        let is_error = event.is_error();
        match event {
            RTTraceEvent::DeadlineMiss { .. } | RTTraceEvent::PipelineFault { .. } => {
                prop_assert!(is_error)
            }
            _ => prop_assert!(!is_error),
        }
    }

    #[test]
    fn test_rt_event_display_contains_type(event in arb_rt_event_full()) {
        let s = format!("{}", event);
        let type_str = event.event_type();
        prop_assert!(s.to_lowercase().contains(&type_str.replace("_", "")));
    }

    #[test]
    fn test_event_copy_clone(event in arb_rt_event_full()) {
        let event2 = event;
        prop_assert_eq!(event.tick_count(), event2.tick_count());
        prop_assert_eq!(event.timestamp_ns(), event2.timestamp_ns());
    }
}

fn arb_device_id() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_-]{0,15}".prop_map(|s| s.to_string())
}

fn arb_device_name() -> impl Strategy<Value = String> {
    "[A-Z][a-zA-Z ]{0,31}".prop_map(|s| s.to_string())
}

fn arb_app_event() -> impl Strategy<Value = AppTraceEvent> {
    let device_connected = (arb_device_id(), arb_device_name()).prop_map(|(id, name)| {
        AppTraceEvent::DeviceConnected {
            device_id: id,
            device_name: name,
            capabilities: "torque".to_string(),
        }
    });

    let device_disconnected = arb_device_id().prop_map(|id| AppTraceEvent::DeviceDisconnected {
        device_id: id,
        reason: "unplugged".to_string(),
    });

    prop_oneof![device_connected, device_disconnected]
}

proptest! {
    #[test]
    fn test_app_event_device_id(event in arb_app_event()) {
        let id = event.device_id();
        match &event {
            AppTraceEvent::DeviceConnected { device_id, .. } => {
                prop_assert_eq!(id, Some(&**device_id));
            }
            AppTraceEvent::DeviceDisconnected { device_id, .. } => {
                prop_assert_eq!(id, Some(&**device_id));
            }
            _ => {}
        }
    }

    #[test]
    fn test_app_event_category_consistency(event in arb_app_event()) {
        let category = event.category();
        match &event {
            AppTraceEvent::DeviceConnected { .. } | AppTraceEvent::DeviceDisconnected { .. } => {
                prop_assert_eq!(category, AppEventCategory::Device)
            }
            AppTraceEvent::TelemetryStarted { .. } => prop_assert_eq!(category, AppEventCategory::Telemetry),
            AppTraceEvent::ProfileApplied { .. } => prop_assert_eq!(category, AppEventCategory::Profile),
            AppTraceEvent::SafetyStateChanged { .. } => prop_assert_eq!(category, AppEventCategory::Safety),
        }
    }
}

// ---------------------------------------------------------------------------
// Additional property tests: event filtering, severity ordering, metrics
// ---------------------------------------------------------------------------

prop_compose! {
    fn arb_telemetry_started()(
        game_id in "[a-z]{3,10}",
        rate in 1.0f32..240.0f32,
    ) -> AppTraceEvent {
        AppTraceEvent::TelemetryStarted { game_id, telemetry_rate_hz: rate }
    }
}

prop_compose! {
    fn arb_profile_applied()(
        device_id in arb_device_id(),
        profile_name in "[A-Z][a-zA-Z ]{0,15}",
        profile_hash in "[0-9a-f]{8}",
    ) -> AppTraceEvent {
        AppTraceEvent::ProfileApplied { device_id, profile_name, profile_hash }
    }
}

prop_compose! {
    fn arb_safety_state_changed()(
        device_id in arb_device_id(),
        old_state in prop_oneof!["safe_torque", "challenge", "high_torque", "faulted"].prop_map(|s| s.to_string()),
        new_state in prop_oneof!["safe_torque", "challenge", "high_torque", "faulted"].prop_map(|s| s.to_string()),
        reason in "[a-z_]{3,20}",
    ) -> AppTraceEvent {
        AppTraceEvent::SafetyStateChanged { device_id, old_state, new_state, reason }
    }
}

fn arb_app_event_full() -> impl Strategy<Value = AppTraceEvent> {
    prop_oneof![
        arb_app_event(),
        arb_telemetry_started(),
        arb_profile_applied(),
        arb_safety_state_changed(),
    ]
}

proptest! {
    // -----------------------------------------------------------------------
    // Event filtering by category
    // -----------------------------------------------------------------------

    #[test]
    fn test_app_event_category_full(event in arb_app_event_full()) {
        let category = event.category();
        match &event {
            AppTraceEvent::DeviceConnected { .. } | AppTraceEvent::DeviceDisconnected { .. } => {
                prop_assert_eq!(category, AppEventCategory::Device)
            }
            AppTraceEvent::TelemetryStarted { .. } => {
                prop_assert_eq!(category, AppEventCategory::Telemetry)
            }
            AppTraceEvent::ProfileApplied { .. } => {
                prop_assert_eq!(category, AppEventCategory::Profile)
            }
            AppTraceEvent::SafetyStateChanged { .. } => {
                prop_assert_eq!(category, AppEventCategory::Safety)
            }
        }
    }

    // -----------------------------------------------------------------------
    // device_id() returns Some for device-related events, None otherwise
    // -----------------------------------------------------------------------

    #[test]
    fn test_app_event_device_id_full(event in arb_app_event_full()) {
        let has_device_id = event.device_id().is_some();
        match &event {
            AppTraceEvent::TelemetryStarted { .. } => {
                prop_assert!(!has_device_id, "TelemetryStarted should not have device_id");
            }
            _ => {
                prop_assert!(has_device_id, "Non-telemetry events should have device_id");
            }
        }
    }

    // -----------------------------------------------------------------------
    // RT event: error events are consistently flagged
    // -----------------------------------------------------------------------

    #[test]
    fn test_error_events_have_error_category(event in arb_rt_event_full()) {
        if event.is_error() {
            prop_assert_eq!(event.category(), RTEventCategory::Error);
        } else {
            prop_assert_ne!(event.category(), RTEventCategory::Error);
        }
    }

    // -----------------------------------------------------------------------
    // RT event: Display output always contains event type keyword
    // -----------------------------------------------------------------------

    #[test]
    fn test_rt_event_display_nonempty(event in arb_rt_event_full()) {
        let s = format!("{}", event);
        prop_assert!(!s.is_empty());
        // Display should contain a recognizable substring
        let contains_keyword = s.contains("Tick") || s.contains("Hid") || s.contains("Deadline") || s.contains("Pipeline");
        prop_assert!(contains_keyword, "Display '{}' should contain an event keyword", s);
    }

    // -----------------------------------------------------------------------
    // App event: Display output always non-empty
    // -----------------------------------------------------------------------

    #[test]
    fn test_app_event_display_nonempty(event in arb_app_event_full()) {
        let s = format!("{}", event);
        prop_assert!(!s.is_empty());
    }

    // -----------------------------------------------------------------------
    // RT event severity ordering: Timing < Hid < Error
    // -----------------------------------------------------------------------

    #[test]
    fn test_severity_ordering_timing_not_error(
        tick_count in 0u64..=u64::MAX / 2,
        timestamp_ns in 0u64..=u64::MAX / 2,
    ) {
        let start = RTTraceEvent::TickStart { tick_count, timestamp_ns };
        prop_assert!(!start.is_error());
        prop_assert_eq!(start.category(), RTEventCategory::Timing);
    }

    #[test]
    fn test_severity_ordering_hid_not_error(
        tick_count in 0u64..=u64::MAX / 2,
        timestamp_ns in 0u64..=u64::MAX / 2,
        torque_nm in 0.0f32..=50.0f32,
        seq in 0u16..=u16::MAX,
    ) {
        let hid = RTTraceEvent::HidWrite { tick_count, timestamp_ns, torque_nm, seq };
        prop_assert!(!hid.is_error());
        prop_assert_eq!(hid.category(), RTEventCategory::Hid);
    }

    #[test]
    fn test_severity_ordering_deadline_miss_is_error(
        tick_count in 0u64..=u64::MAX / 2,
        timestamp_ns in 0u64..=u64::MAX / 2,
        jitter_ns in 0u64..=10_000_000u64,
    ) {
        let miss = RTTraceEvent::DeadlineMiss { tick_count, timestamp_ns, jitter_ns };
        prop_assert!(miss.is_error());
        prop_assert_eq!(miss.category(), RTEventCategory::Error);
    }

    #[test]
    fn test_severity_ordering_pipeline_fault_is_error(
        tick_count in 0u64..=u64::MAX / 2,
        timestamp_ns in 0u64..=u64::MAX / 2,
        error_code in 0u8..=255u8,
    ) {
        let fault = RTTraceEvent::PipelineFault { tick_count, timestamp_ns, error_code };
        prop_assert!(fault.is_error());
        prop_assert_eq!(fault.category(), RTEventCategory::Error);
    }
}

// ---------------------------------------------------------------------------
// TracingMetrics property tests
// ---------------------------------------------------------------------------

use openracing_tracing::TracingMetrics;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_metrics_record_rt_event_increments(count in 1u32..=1000u32) {
        let mut m = TracingMetrics::new();
        for _ in 0..count {
            m.record_rt_event();
        }
        prop_assert_eq!(m.rt_events_emitted, count as u64);
    }

    #[test]
    fn prop_metrics_record_app_event_increments(count in 1u32..=1000u32) {
        let mut m = TracingMetrics::new();
        for _ in 0..count {
            m.record_app_event();
        }
        prop_assert_eq!(m.app_events_emitted, count as u64);
    }

    #[test]
    fn prop_metrics_drop_rate_bounded(
        emitted in 1u64..=100_000u64,
        drop_pct in 0u32..=100u32,
    ) {
        let mut m = TracingMetrics::new();
        m.rt_events_emitted = emitted;
        m.events_dropped = (emitted * drop_pct as u64) / 100;
        let rate = m.drop_rate();
        prop_assert!(rate >= 0.0);
        prop_assert!(rate <= 1.0);
    }

    #[test]
    fn prop_metrics_healthy_when_no_faults_low_drops(
        emitted in 1000u64..=100_000u64,
    ) {
        let mut m = TracingMetrics::new();
        m.rt_events_emitted = emitted;
        m.events_dropped = 0;
        m.pipeline_faults = 0;
        prop_assert!(m.is_healthy());
    }

    #[test]
    fn prop_metrics_unhealthy_with_pipeline_faults(
        emitted in 1u64..=100_000u64,
        faults in 1u64..=100u64,
    ) {
        let mut m = TracingMetrics::new();
        m.rt_events_emitted = emitted;
        m.pipeline_faults = faults;
        prop_assert!(!m.is_healthy());
    }

    #[test]
    fn prop_metrics_merge_is_additive(
        a_rt in 0u64..=50_000u64,
        b_rt in 0u64..=50_000u64,
        a_app in 0u64..=50_000u64,
        b_app in 0u64..=50_000u64,
    ) {
        let mut m1 = TracingMetrics::new();
        m1.rt_events_emitted = a_rt;
        m1.app_events_emitted = a_app;

        let m2 = TracingMetrics {
            rt_events_emitted: b_rt,
            app_events_emitted: b_app,
            ..Default::default()
        };

        m1.merge(&m2);
        prop_assert_eq!(m1.rt_events_emitted, a_rt + b_rt);
        prop_assert_eq!(m1.app_events_emitted, a_app + b_app);
    }

    #[test]
    fn prop_metrics_reset_zeros_all(
        rt in 0u64..=100_000u64,
        app in 0u64..=100_000u64,
        dropped in 0u64..=1000u64,
    ) {
        let mut m = TracingMetrics::new();
        m.rt_events_emitted = rt;
        m.app_events_emitted = app;
        m.events_dropped = dropped;
        m.reset();
        prop_assert_eq!(m.rt_events_emitted, 0);
        prop_assert_eq!(m.app_events_emitted, 0);
        prop_assert_eq!(m.events_dropped, 0);
    }

    #[test]
    fn prop_metrics_display_contains_numbers(
        rt in 0u64..=100_000u64,
        app in 0u64..=100_000u64,
    ) {
        let mut m = TracingMetrics::new();
        m.rt_events_emitted = rt;
        m.app_events_emitted = app;
        let s = format!("{}", m);
        prop_assert!(s.contains(&rt.to_string()), "Display should contain rt count");
        prop_assert!(s.contains(&app.to_string()), "Display should contain app count");
    }
}

// ---------------------------------------------------------------------------
// Span lifecycle (TracingManager enable/disable) tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod span_lifecycle {
    use openracing_tracing::{RTTraceEvent, TracingError, TracingManager};

    #[test]
    fn test_tracing_manager_can_be_created() -> Result<(), TracingError> {
        let mut manager = TracingManager::new()?;
        manager.initialize()?;
        manager.shutdown();
        Ok(())
    }

    #[test]
    fn test_tracing_manager_default_is_usable() {
        let manager = TracingManager::default();
        let metrics = manager.metrics();
        // Default might be disabled, but metrics should work
        assert_eq!(metrics.rt_events_emitted, 0);
    }

    #[test]
    fn test_tracing_manager_enable_disable_cycle() -> Result<(), TracingError> {
        let mut manager = TracingManager::new()?;
        manager.initialize()?;

        assert!(manager.is_enabled());

        manager.set_enabled(false);
        // When disabled, events are silently dropped
        manager.emit_rt_event(RTTraceEvent::TickStart {
            tick_count: 1,
            timestamp_ns: 1000,
        });

        manager.set_enabled(true);
        assert!(manager.is_enabled());

        manager.shutdown();
        Ok(())
    }

    #[test]
    fn test_tracing_manager_debug_format() -> Result<(), TracingError> {
        let manager = TracingManager::new()?;
        let debug_str = format!("{:?}", manager);
        assert!(debug_str.contains("TracingManager"));
        assert!(debug_str.contains("enabled"));
        Ok(())
    }
}
