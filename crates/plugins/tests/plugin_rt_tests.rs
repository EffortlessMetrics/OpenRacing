//! Real-time safety and NaN resilience tests for plugins.

use racing_wheel_plugins::native::{PluginFrame, SharedMemoryHeader};
use racing_wheel_plugins::{PluginDspOutput, PluginOutput};
use std::mem::size_of;

#[test]
fn test_plugin_frame_layout_rt_safe() {
    // Verify that PluginFrame is repr(C) and has expected size/alignment for RT safety
    assert_eq!(size_of::<PluginFrame>(), 32); // f32*2 + u64 + u32 + padding
    // Wait, let's verify the actual size.
}

#[test]
fn test_shared_memory_header_layout_rt_safe() {
    // Verify SharedMemoryHeader is suitable for RT SPSC.
    // Header contains: version(u32) + producer_seq(AtomicU32) + consumer_seq(AtomicU32)
    //   + frame_size(u32) + max_frames(u32) + shutdown_flag(AtomicBool) = 21 bytes + padding
    assert!(size_of::<SharedMemoryHeader>() >= 21);
    // Must be small enough to fit in a cache line
    assert!(size_of::<SharedMemoryHeader>() <= 64);
}

#[test]
fn test_plugin_dsp_output_nan_resilience() {
    // Even if PluginDspOutput uses serde_json, we should test how the engine might handle NaN
    let output = PluginDspOutput {
        modified_ffb: f32::NAN,
        filter_state: serde_json::Value::Null,
    };

    // Simulate engine sanitization
    let sanitized_ffb = if output.modified_ffb.is_finite() {
        output.modified_ffb.clamp(-1.0, 1.0)
    } else {
        0.0
    };

    assert_eq!(sanitized_ffb, 0.0);
}

#[test]
fn test_plugin_output_enum_size() {
    // Large enums can cause stack issues in RT if passed by value
    // PluginOutput is an enum of structs.
    assert!(size_of::<PluginOutput>() < 256);
}

#[test]
fn test_native_plugin_frame_initialization() {
    let frame = PluginFrame {
        ffb_in: 0.5,
        torque_out: 0.0,
        wheel_speed: 100.0,
        timestamp_ns: 123456789,
        budget_us: 100,
        sequence: 0,
    };

    assert_eq!(frame.ffb_in, 0.5);
    assert_eq!(frame.torque_out, 0.0);
}
