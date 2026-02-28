//! Snapshot tests for openracing-tracing event formats

use openracing_tracing::{AppTraceEvent, RTTraceEvent};

#[test]
fn test_rt_tick_start_snapshot() {
    let event = RTTraceEvent::TickStart {
        tick_count: 42,
        timestamp_ns: 1_000_000_000,
    };
    insta::assert_snapshot!("rt_tick_start", format!("{}", event));
}

#[test]
fn test_rt_tick_end_snapshot() {
    let event = RTTraceEvent::TickEnd {
        tick_count: 42,
        timestamp_ns: 1_000_000_500,
        processing_time_ns: 500,
    };
    insta::assert_snapshot!("rt_tick_end", format!("{}", event));
}

#[test]
fn test_rt_hid_write_snapshot() {
    let event = RTTraceEvent::HidWrite {
        tick_count: 42,
        timestamp_ns: 1_000_000_250,
        torque_nm: 50.0,
        seq: 123,
    };
    insta::assert_snapshot!("rt_hid_write", format!("{}", event));
}

#[test]
fn test_rt_deadline_miss_snapshot() {
    let event = RTTraceEvent::DeadlineMiss {
        tick_count: 42,
        timestamp_ns: 1_000_001_000,
        jitter_ns: 250_000,
    };
    insta::assert_snapshot!("rt_deadline_miss", format!("{}", event));
}

#[test]
fn test_rt_pipeline_fault_snapshot() {
    let event = RTTraceEvent::PipelineFault {
        tick_count: 42,
        timestamp_ns: 1_000_001_000,
        error_code: 5,
    };
    insta::assert_snapshot!("rt_pipeline_fault", format!("{}", event));
}

#[test]
fn test_app_device_connected_snapshot() {
    let event = AppTraceEvent::DeviceConnected {
        device_id: "wheel-001".to_string(),
        device_name: "Simagic Alpha Mini".to_string(),
        capabilities: "torque,rotation,ffb".to_string(),
    };
    insta::assert_snapshot!("app_device_connected", format!("{}", event));
}

#[test]
fn test_app_device_disconnected_snapshot() {
    let event = AppTraceEvent::DeviceDisconnected {
        device_id: "wheel-001".to_string(),
        reason: "USB unplugged".to_string(),
    };
    insta::assert_snapshot!("app_device_disconnected", format!("{}", event));
}

#[test]
fn test_app_telemetry_started_snapshot() {
    let event = AppTraceEvent::TelemetryStarted {
        game_id: "iracing".to_string(),
        telemetry_rate_hz: 60.0,
    };
    insta::assert_snapshot!("app_telemetry_started", format!("{}", event));
}

#[test]
fn test_app_profile_applied_snapshot() {
    let event = AppTraceEvent::ProfileApplied {
        device_id: "wheel-001".to_string(),
        profile_name: "GT3_Dominant".to_string(),
        profile_hash: "sha256:abc123".to_string(),
    };
    insta::assert_snapshot!("app_profile_applied", format!("{}", event));
}

#[test]
fn test_app_safety_state_changed_snapshot() {
    let event = AppTraceEvent::SafetyStateChanged {
        device_id: "wheel-001".to_string(),
        old_state: "safe".to_string(),
        new_state: "high_torque".to_string(),
        reason: "user_consent".to_string(),
    };
    insta::assert_snapshot!("app_safety_state_changed", format!("{}", event));
}
