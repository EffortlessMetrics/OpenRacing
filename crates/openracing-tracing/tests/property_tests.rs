//! Property-based tests for openracing-tracing

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
                prop_assert_eq!(id, Some(device_id.as_str()));
            }
            AppTraceEvent::DeviceDisconnected { device_id, .. } => {
                prop_assert_eq!(id, Some(device_id.as_str()));
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
