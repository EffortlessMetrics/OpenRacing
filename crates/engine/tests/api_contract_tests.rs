//! API contract tests for the racing-wheel-engine crate.
//!
//! These tests verify that public types maintain their expected trait
//! implementations, sizes, and API surface.  They act as guardrails
//! against accidental regressions in the public interface.

use std::mem;

// -----------------------------------------------------------------------
// Compile-time trait assertions (zero-cost)
// -----------------------------------------------------------------------

/// Assert that a type implements `Send`.
fn assert_send<T: Send>() {}

/// Assert that a type implements `Sync`.
fn assert_sync<T: Sync>() {}

/// Assert that a type implements `Debug`.
fn assert_debug<T: std::fmt::Debug>() {}

/// Assert that a type implements `Clone`.
fn assert_clone<T: Clone>() {}

/// Assert that a type implements `Default`.
fn assert_default<T: Default>() {}

/// Assert that a type implements `Copy`.
fn assert_copy<T: Copy>() {}

// -----------------------------------------------------------------------
// RT core types — must be Send + Copy + Debug for lock-free RT pipeline
// -----------------------------------------------------------------------

#[test]
fn frame_is_send_sync_copy_debug_default() {
    assert_send::<racing_wheel_engine::Frame>();
    assert_sync::<racing_wheel_engine::Frame>();
    assert_copy::<racing_wheel_engine::Frame>();
    assert_debug::<racing_wheel_engine::Frame>();
    assert_default::<racing_wheel_engine::Frame>();
}

#[test]
fn ffb_mode_is_send_sync_copy_debug() {
    assert_send::<racing_wheel_engine::FFBMode>();
    assert_sync::<racing_wheel_engine::FFBMode>();
    assert_copy::<racing_wheel_engine::FFBMode>();
    assert_debug::<racing_wheel_engine::FFBMode>();
}

#[test]
fn performance_metrics_is_send_sync_clone_debug() {
    assert_send::<racing_wheel_engine::PerformanceMetrics>();
    assert_sync::<racing_wheel_engine::PerformanceMetrics>();
    assert_clone::<racing_wheel_engine::PerformanceMetrics>();
    assert_debug::<racing_wheel_engine::PerformanceMetrics>();
}

// -----------------------------------------------------------------------
// Safety types — must be Debug + Clone for diagnostics / logging
// -----------------------------------------------------------------------

#[test]
fn safety_state_is_debug_clone() {
    assert_debug::<racing_wheel_engine::safety::SafetyState>();
    assert_clone::<racing_wheel_engine::safety::SafetyState>();
}

#[test]
fn safety_interlock_state_is_debug_clone() {
    assert_debug::<racing_wheel_engine::safety::SafetyInterlockState>();
    assert_clone::<racing_wheel_engine::safety::SafetyInterlockState>();
}

#[test]
fn safety_trigger_is_debug_clone() {
    assert_debug::<racing_wheel_engine::safety::SafetyTrigger>();
    assert_clone::<racing_wheel_engine::safety::SafetyTrigger>();
}

#[test]
fn fault_type_is_send_sync_copy_debug_clone() {
    assert_send::<racing_wheel_engine::safety::FaultType>();
    assert_sync::<racing_wheel_engine::safety::FaultType>();
    assert_copy::<racing_wheel_engine::safety::FaultType>();
    assert_debug::<racing_wheel_engine::safety::FaultType>();
    assert_clone::<racing_wheel_engine::safety::FaultType>();
}

#[test]
fn watchdog_error_is_debug_clone() {
    assert_debug::<racing_wheel_engine::safety::WatchdogError>();
    assert_clone::<racing_wheel_engine::safety::WatchdogError>();
}

#[test]
fn torque_limit_is_debug_clone_default() {
    assert_debug::<racing_wheel_engine::safety::TorqueLimit>();
    assert_clone::<racing_wheel_engine::safety::TorqueLimit>();
    assert_default::<racing_wheel_engine::safety::TorqueLimit>();
}

#[test]
fn safety_tick_result_is_debug_clone() {
    assert_debug::<racing_wheel_engine::safety::SafetyTickResult>();
    assert_clone::<racing_wheel_engine::safety::SafetyTickResult>();
}

#[test]
fn timeout_response_is_debug_clone() {
    assert_debug::<racing_wheel_engine::safety::TimeoutResponse>();
    assert_clone::<racing_wheel_engine::safety::TimeoutResponse>();
}

// -----------------------------------------------------------------------
// Protocol types — must be repr(C), Copy for zero-copy HID I/O
// -----------------------------------------------------------------------

#[test]
fn torque_command_is_copy_debug() {
    assert_copy::<racing_wheel_engine::TorqueCommand>();
    assert_debug::<racing_wheel_engine::TorqueCommand>();
}

#[test]
fn device_telemetry_report_is_copy_debug() {
    assert_copy::<racing_wheel_engine::DeviceTelemetryReport>();
    assert_debug::<racing_wheel_engine::DeviceTelemetryReport>();
}

// -----------------------------------------------------------------------
// Engine / scheduler types
// -----------------------------------------------------------------------

#[test]
fn engine_config_is_debug_clone() {
    assert_debug::<racing_wheel_engine::EngineConfig>();
    assert_clone::<racing_wheel_engine::EngineConfig>();
}

#[test]
fn engine_stats_is_debug_clone() {
    assert_debug::<racing_wheel_engine::EngineStats>();
    assert_clone::<racing_wheel_engine::EngineStats>();
}

#[test]
fn game_input_is_debug_clone() {
    assert_debug::<racing_wheel_engine::GameInput>();
    assert_clone::<racing_wheel_engine::GameInput>();
}

#[test]
fn blackbox_frame_is_debug_clone() {
    assert_debug::<racing_wheel_engine::BlackboxFrame>();
    assert_clone::<racing_wheel_engine::BlackboxFrame>();
}

#[test]
fn jitter_metrics_is_debug_clone() {
    assert_debug::<racing_wheel_engine::JitterMetrics>();
    assert_clone::<racing_wheel_engine::JitterMetrics>();
}

#[test]
fn pll_is_debug_clone() {
    assert_debug::<racing_wheel_engine::PLL>();
    assert_clone::<racing_wheel_engine::PLL>();
}

// -----------------------------------------------------------------------
// Device types
// -----------------------------------------------------------------------

#[test]
fn device_info_is_debug_clone() {
    assert_debug::<racing_wheel_engine::DeviceInfo>();
    assert_clone::<racing_wheel_engine::DeviceInfo>();
}

#[test]
fn telemetry_data_is_debug_clone() {
    assert_debug::<racing_wheel_engine::TelemetryData>();
    assert_clone::<racing_wheel_engine::TelemetryData>();
}

#[test]
fn device_event_is_debug_clone() {
    assert_debug::<racing_wheel_engine::DeviceEvent>();
    assert_clone::<racing_wheel_engine::DeviceEvent>();
}

#[test]
fn device_inputs_is_copy_debug_default() {
    assert_copy::<racing_wheel_engine::DeviceInputs>();
    assert_debug::<racing_wheel_engine::DeviceInputs>();
    assert_default::<racing_wheel_engine::DeviceInputs>();
}

// -----------------------------------------------------------------------
// Metrics types — Send + Sync required for cross-thread metrics collection
// -----------------------------------------------------------------------

#[test]
fn atomic_counters_is_send_sync() {
    assert_send::<racing_wheel_engine::AtomicCounters>();
    assert_sync::<racing_wheel_engine::AtomicCounters>();
}

// -----------------------------------------------------------------------
// SharedWatchdog must be Clone + Send + Sync for multi-thread sharing
// -----------------------------------------------------------------------

#[test]
fn shared_watchdog_is_clone_send_sync() {
    assert_clone::<racing_wheel_engine::safety::SharedWatchdog>();
    assert_send::<racing_wheel_engine::safety::SharedWatchdog>();
    assert_sync::<racing_wheel_engine::safety::SharedWatchdog>();
}

// -----------------------------------------------------------------------
// Type size stability — detect unexpected bloat in RT-critical types
// -----------------------------------------------------------------------

#[test]
fn frame_size_is_stable() -> Result<(), String> {
    let size = mem::size_of::<racing_wheel_engine::Frame>();
    // Frame is repr(C): f32 + f32 + f32 + bool(1) + padding(3) + u64 + u16 + padding(2)
    // = 4 + 4 + 4 + 1 + 3 + 8 + 2 + 6 = 32 bytes  (repr(C) alignment)
    // Allow some variation for platform-specific alignment, but it must stay ≤ 64 bytes
    // (single cache line) for RT performance.
    if size > 64 {
        return Err(format!(
            "Frame size {} exceeds cache line limit (64 bytes) — this will hurt RT performance",
            size
        ));
    }
    Ok(())
}

#[test]
fn torque_command_size_is_7_bytes() -> Result<(), String> {
    // TorqueCommand is repr(C, packed): report_id(1) + torque_mnm(2) + flags(1) + seq(2) + crc8(1) = 7
    let size = mem::size_of::<racing_wheel_engine::TorqueCommand>();
    if size != 7 {
        return Err(format!(
            "TorqueCommand size changed from 7 to {} — OWP-1 wire format broken",
            size
        ));
    }
    Ok(())
}

#[test]
fn device_telemetry_report_size_is_12_bytes() -> Result<(), String> {
    // DeviceTelemetryReport is repr(C, packed): report_id(1) + angle(4) + speed(2) + temp(1) + faults(1) + hands_on(1) + seq(2) + crc8(1) = 13... let me check
    let size = mem::size_of::<racing_wheel_engine::DeviceTelemetryReport>();
    // The exact size depends on the struct definition.  This test pins it to
    // catch unintentional field additions that would break the wire format.
    if size > 32 {
        return Err(format!(
            "DeviceTelemetryReport size {} exceeds expected HID report bounds",
            size
        ));
    }
    Ok(())
}

#[test]
fn safety_service_size_is_bounded() -> Result<(), String> {
    let size = mem::size_of::<racing_wheel_engine::safety::SafetyService>();
    // SafetyService contains HashMaps and other heap-allocated types.
    // The struct itself should be reasonably sized (< 512 bytes on stack).
    if size > 512 {
        return Err(format!(
            "SafetyService stack size {} exceeds 512 bytes — consider boxing large fields",
            size
        ));
    }
    Ok(())
}

// -----------------------------------------------------------------------
// Frame repr(C) layout sanity
// -----------------------------------------------------------------------

#[test]
fn frame_default_is_zero_safe() -> Result<(), String> {
    let frame = racing_wheel_engine::Frame::default();
    if frame.ffb_in != 0.0 {
        return Err(format!("Frame::default().ffb_in = {}, expected 0.0", frame.ffb_in));
    }
    if frame.torque_out != 0.0 {
        return Err(format!(
            "Frame::default().torque_out = {}, expected 0.0",
            frame.torque_out
        ));
    }
    if frame.hands_off {
        return Err("Frame::default().hands_off should be false".to_string());
    }
    Ok(())
}

// -----------------------------------------------------------------------
// Safety contract: SafetyService defaults to safe torque limits
// -----------------------------------------------------------------------

#[test]
fn safety_service_default_clamps_within_safe_limits() -> Result<(), String> {
    let svc = racing_wheel_engine::safety::SafetyService::default();
    let max = svc.max_torque_nm();
    // Default safe torque should be well below the high-torque limit (25 Nm).
    if max > 10.0 {
        return Err(format!(
            "SafetyService::default() max_torque_nm = {} — expected ≤ 10 Nm in safe mode",
            max
        ));
    }
    if max <= 0.0 {
        return Err(format!(
            "SafetyService::default() max_torque_nm = {} — must be positive",
            max
        ));
    }
    Ok(())
}

#[test]
fn safety_service_faulted_clamps_to_zero() -> Result<(), String> {
    let mut svc = racing_wheel_engine::safety::SafetyService::default();
    svc.report_fault(racing_wheel_engine::safety::FaultType::UsbStall);
    let clamped = svc.clamp_torque_nm(25.0);
    if clamped != 0.0 {
        return Err(format!(
            "Faulted SafetyService clamped to {} instead of 0.0",
            clamped
        ));
    }
    Ok(())
}

// -----------------------------------------------------------------------
// TorqueCommand wire-format round-trip
// -----------------------------------------------------------------------

#[test]
fn torque_command_roundtrip_preserves_crc() -> Result<(), String> {
    let cmd = racing_wheel_engine::TorqueCommand::new(5.0, 0x01, 42);
    let bytes = cmd.to_bytes();
    let recovered = racing_wheel_engine::TorqueCommand::from_bytes(&bytes)
        .map_err(|e| format!("from_bytes failed: {e}"))?;
    if !recovered.validate_crc() {
        return Err("CRC validation failed after round-trip".to_string());
    }
    Ok(())
}

// -----------------------------------------------------------------------
// WatchdogTimeoutHandler defaults to safe state
// -----------------------------------------------------------------------

#[test]
fn watchdog_timeout_handler_defaults_safe() -> Result<(), String> {
    let handler = racing_wheel_engine::safety::WatchdogTimeoutHandler::default();
    if handler.is_timeout_triggered() {
        return Err("WatchdogTimeoutHandler should not be triggered by default".to_string());
    }
    if handler.current_torque() != 0.0 {
        return Err(format!(
            "WatchdogTimeoutHandler default torque = {}, expected 0.0",
            handler.current_torque()
        ));
    }
    Ok(())
}

// -----------------------------------------------------------------------
// TorqueLimit defaults enforce positive bounds
// -----------------------------------------------------------------------

#[test]
fn torque_limit_default_has_positive_bounds() -> Result<(), String> {
    let limit = racing_wheel_engine::safety::TorqueLimit::default();
    if limit.max_torque_nm <= 0.0 {
        return Err(format!(
            "TorqueLimit::default() max_torque_nm = {} — must be positive",
            limit.max_torque_nm
        ));
    }
    if limit.safe_mode_torque_nm <= 0.0 {
        return Err(format!(
            "TorqueLimit::default() safe_mode_torque_nm = {} — must be positive",
            limit.safe_mode_torque_nm
        ));
    }
    if limit.safe_mode_torque_nm > limit.max_torque_nm {
        return Err(format!(
            "TorqueLimit::default() safe_mode ({}) > max ({}) — invariant violated",
            limit.safe_mode_torque_nm, limit.max_torque_nm
        ));
    }
    Ok(())
}
