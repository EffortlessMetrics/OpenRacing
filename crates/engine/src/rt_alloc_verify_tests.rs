//! RT Allocation Verification Tests
//!
//! These tests verify the critical invariant that no heap allocations occur
//! in the real-time (RT) hot path after initialization. The RT loop runs
//! at 1kHz (1ms budget per tick), and any allocation could cause unbounded
//! latency, missed ticks, or safety interlock violations.
//!
//! The crate-level `#[global_allocator]` is set to `TrackingAllocator` in
//! test builds (see `lib.rs`), so every `Vec`, `String`, or `Box` created
//! during a guarded region is counted.

use crate::allocation_tracker::{self, AllocationGuard};
use crate::curves::{CurveLut, CurveType};
use crate::filters::*;
use crate::pipeline::Pipeline;
use crate::rt::Frame;
use crate::safety::{FaultType, SafetyService, SafetyState};
use proptest::prelude::*;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a default RT frame for testing.
fn test_frame(ffb_in: f32, torque_out: f32) -> Frame {
    Frame {
        ffb_in,
        torque_out,
        wheel_speed: 0.0,
        hands_off: false,
        ts_mono_ns: 1_000_000, // 1ms
        seq: 1,
    }
}

/// Create a safety service in safe-torque mode.
fn test_safety_service() -> SafetyService {
    SafetyService::with_timeouts(5.0, 25.0, Duration::from_secs(3), Duration::from_secs(2))
}

/// Start an allocation guard, discarding any allocations caused by test setup.
fn fresh_guard() -> AllocationGuard {
    allocation_tracker::track()
}

// ---------------------------------------------------------------------------
// 1. Pipeline processing – zero allocations
// ---------------------------------------------------------------------------

#[test]
fn pipeline_process_empty_zero_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let mut pipeline = Pipeline::new();
    let mut frame = test_frame(0.5, 0.0);

    // Warm up – discard setup allocations
    pipeline.process(&mut frame)?;

    let guard = fresh_guard();
    let mut frame = test_frame(0.5, 0.0);
    pipeline.process(&mut frame)?;

    let allocs = guard.allocations_since_start();
    assert_eq!(allocs, 0, "Empty pipeline process allocated {allocs} times");
    Ok(())
}

#[test]
fn pipeline_process_with_response_curve_zero_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let mut pipeline = Pipeline::new();
    pipeline.set_response_curve(CurveLut::linear());

    // Warm up
    let mut frame = test_frame(0.5, 0.5);
    pipeline.process(&mut frame)?;

    let guard = fresh_guard();
    for i in 0..100 {
        let mut frame = test_frame(0.5, (i as f32) / 100.0);
        pipeline.process(&mut frame)?;
    }

    let allocs = guard.allocations_since_start();
    assert_eq!(
        allocs, 0,
        "Pipeline with response curve allocated {allocs} times over 100 ticks"
    );
    Ok(())
}

#[test]
fn pipeline_process_with_exponential_curve_zero_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let curve_type = CurveType::exponential(2.0)?;
    let mut pipeline = Pipeline::new();
    pipeline.set_response_curve_from_type(&curve_type);

    let mut frame = test_frame(0.5, 0.5);
    pipeline.process(&mut frame)?;

    let guard = fresh_guard();
    for _ in 0..50 {
        let mut frame = test_frame(0.7, 0.3);
        pipeline.process(&mut frame)?;
    }

    let allocs = guard.allocations_since_start();
    assert_eq!(
        allocs, 0,
        "Pipeline with exponential curve allocated {allocs} times"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 2. Safety state transitions – zero allocations
// ---------------------------------------------------------------------------

#[test]
fn safety_clamp_torque_zero_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let service = test_safety_service();

    let guard = fresh_guard();
    for i in 0..200 {
        let requested = (i as f32 - 100.0) / 10.0; // range: -10..+10
        let _clamped = service.clamp_torque_nm(requested);
    }

    let allocs = guard.allocations_since_start();
    assert_eq!(
        allocs, 0,
        "clamp_torque_nm allocated {allocs} times over 200 calls"
    );
    Ok(())
}

#[test]
fn safety_clamp_torque_special_values_zero_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let service = test_safety_service();

    let guard = fresh_guard();
    let _a = service.clamp_torque_nm(f32::NAN);
    let _b = service.clamp_torque_nm(f32::INFINITY);
    let _c = service.clamp_torque_nm(f32::NEG_INFINITY);
    let _d = service.clamp_torque_nm(0.0);

    let allocs = guard.allocations_since_start();
    assert_eq!(
        allocs, 0,
        "clamp_torque_nm with special values allocated {allocs} times"
    );
    Ok(())
}

#[test]
fn safety_max_torque_nm_zero_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let service = test_safety_service();

    let guard = fresh_guard();
    for _ in 0..100 {
        let _t = service.max_torque_nm();
    }

    let allocs = guard.allocations_since_start();
    assert_eq!(
        allocs, 0,
        "max_torque_nm allocated {allocs} times over 100 calls"
    );
    Ok(())
}

#[test]
fn safety_state_read_zero_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let service = test_safety_service();

    let guard = fresh_guard();
    let _state = service.state();

    let allocs = guard.allocations_since_start();
    assert_eq!(allocs, 0, "reading safety state allocated {allocs} times");
    Ok(())
}

#[test]
fn safety_report_fault_into_preallocated_map_zero_alloc() -> Result<(), Box<dyn std::error::Error>>
{
    // SafetyService pre-allocates all fault entries in the HashMap at construction.
    // Reporting a known fault should NOT allocate because the key already exists.
    let mut service = test_safety_service();

    let guard = fresh_guard();
    service.report_fault(FaultType::UsbStall);

    let allocs = guard.allocations_since_start();
    assert_eq!(
        allocs, 0,
        "report_fault for pre-allocated key allocated {allocs} times"
    );

    // Verify state transitioned
    assert!(
        matches!(service.state(), SafetyState::Faulted { .. }),
        "Expected Faulted state"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 3. Torque output / frame calculations – zero allocations
// ---------------------------------------------------------------------------

#[test]
fn frame_creation_and_manipulation_zero_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let guard = fresh_guard();

    let mut frame = Frame {
        ffb_in: 0.8,
        torque_out: 0.0,
        wheel_speed: 5.0,
        hands_off: false,
        ts_mono_ns: 1_000_000,
        seq: 42,
    };

    // Simulate RT tick: copy input → clamp
    frame.torque_out = frame.ffb_in;
    frame.torque_out = frame.torque_out.clamp(-1.0, 1.0);
    let _seq = frame.seq.wrapping_add(1);

    let allocs = guard.allocations_since_start();
    assert_eq!(
        allocs, 0,
        "Frame creation/manipulation allocated {allocs} times"
    );
    Ok(())
}

#[test]
fn torque_clamping_chain_zero_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let service = test_safety_service();
    let mut pipeline = Pipeline::new();
    pipeline.set_response_curve(CurveLut::linear());

    // Warm up
    let mut frame = test_frame(0.5, 0.5);
    pipeline.process(&mut frame)?;

    let guard = fresh_guard();
    for i in 0..100 {
        let ffb = ((i as f32) / 50.0) - 1.0; // -1.0 to +1.0
        let mut frame = test_frame(ffb, ffb);

        // Full RT chain: pipeline → safety clamp
        pipeline.process(&mut frame)?;
        let _out = service.clamp_torque_nm(frame.torque_out * 25.0);
    }

    let allocs = guard.allocations_since_start();
    assert_eq!(
        allocs, 0,
        "Full RT chain allocated {allocs} times over 100 ticks"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 4. CurveLut lookup – zero allocations and bounded execution
// ---------------------------------------------------------------------------

#[test]
fn curve_lut_lookup_zero_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let lut = CurveLut::linear();

    let guard = fresh_guard();
    for i in 0..=255 {
        let input = (i as f32) / 255.0;
        let _output = lut.lookup(input);
    }

    let allocs = guard.allocations_since_start();
    assert_eq!(
        allocs, 0,
        "CurveLut::lookup allocated {allocs} times over 256 lookups"
    );
    Ok(())
}

#[test]
fn curve_lut_lookup_boundary_values_zero_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let lut = CurveLut::linear();

    let guard = fresh_guard();
    let _a = lut.lookup(0.0);
    let _b = lut.lookup(1.0);
    let _c = lut.lookup(-0.1); // below range
    let _d = lut.lookup(1.1); // above range

    let allocs = guard.allocations_since_start();
    assert_eq!(
        allocs, 0,
        "CurveLut boundary lookups allocated {allocs} times"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 5. Bounded execution time
// ---------------------------------------------------------------------------

#[test]
fn pipeline_process_within_budget() -> Result<(), Box<dyn std::error::Error>> {
    let mut pipeline = Pipeline::new();
    pipeline.set_response_curve(CurveLut::linear());

    // Warm up
    let mut frame = test_frame(0.5, 0.5);
    pipeline.process(&mut frame)?;

    let iterations = 1_000;
    let start = Instant::now();
    for i in 0..iterations {
        let mut frame = test_frame(0.5, (i as f32) / (iterations as f32));
        pipeline.process(&mut frame)?;
    }
    let elapsed = start.elapsed();

    // Budget: 1ms per tick at 1kHz. Empty pipeline should be well under 50µs each.
    let avg_us = elapsed.as_micros() as f64 / iterations as f64;
    assert!(
        avg_us < 50.0,
        "Average pipeline process time {avg_us:.1}µs exceeds 50µs budget"
    );
    Ok(())
}

#[test]
fn safety_clamp_within_budget() -> Result<(), Box<dyn std::error::Error>> {
    let service = test_safety_service();

    let iterations = 10_000;
    let start = Instant::now();
    for i in 0..iterations {
        let _clamped = service.clamp_torque_nm((i as f32 - 5000.0) / 100.0);
    }
    let elapsed = start.elapsed();

    let avg_ns = elapsed.as_nanos() as f64 / iterations as f64;
    assert!(
        avg_ns < 1_000.0,
        "Average clamp_torque_nm time {avg_ns:.0}ns exceeds 1µs budget"
    );
    Ok(())
}

#[test]
fn curve_lut_lookup_within_budget() -> Result<(), Box<dyn std::error::Error>> {
    let lut = CurveLut::linear();

    let iterations = 10_000;
    let start = Instant::now();
    for i in 0..iterations {
        let _output = lut.lookup((i as f32) / (iterations as f32));
    }
    let elapsed = start.elapsed();

    let avg_ns = elapsed.as_nanos() as f64 / iterations as f64;
    assert!(
        avg_ns < 500.0,
        "Average CurveLut lookup time {avg_ns:.0}ns exceeds 500ns budget"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 6. Pre-allocated buffer sizing and type verification
// ---------------------------------------------------------------------------

#[test]
fn frame_is_repr_c_and_fixed_size() -> Result<(), Box<dyn std::error::Error>> {
    // Frame must be a fixed-size, stack-only type
    let frame_size = std::mem::size_of::<Frame>();
    assert!(
        frame_size <= 64,
        "Frame is {frame_size} bytes – should be ≤64 for cache-line friendliness"
    );

    // Frame must be Copy (no heap pointers)
    let f1 = test_frame(0.5, 0.3);
    let f2 = f1; // Copy
    assert_eq!(f1.ffb_in, f2.ffb_in);
    Ok(())
}

#[test]
fn curve_lut_uses_fixed_size_array() -> Result<(), Box<dyn std::error::Error>> {
    // CurveLut must use a fixed [f32; 256] table, not Vec<f32>
    let lut_size = std::mem::size_of::<CurveLut>();
    let expected_min = 256 * std::mem::size_of::<f32>(); // 1024 bytes for [f32; 256]

    assert!(
        lut_size >= expected_min,
        "CurveLut is {lut_size} bytes – expected at least {expected_min} for [f32; 256]"
    );
    Ok(())
}

#[test]
fn safety_state_enum_is_fixed_size() -> Result<(), Box<dyn std::error::Error>> {
    let state_size = std::mem::size_of::<SafetyState>();
    // SafetyState is an enum with Instant fields – should be stack-only, no heap
    assert!(
        state_size <= 128,
        "SafetyState is {state_size} bytes – should be ≤128 (no heap indirection)"
    );
    Ok(())
}

#[test]
fn filter_states_are_fixed_size() -> Result<(), Box<dyn std::error::Error>> {
    // All filter state types must be fixed-size (no Vec/String/Box fields)
    let sizes = [
        (
            "ReconstructionState",
            std::mem::size_of::<ReconstructionState>(),
        ),
        ("FrictionState", std::mem::size_of::<FrictionState>()),
        ("DamperState", std::mem::size_of::<DamperState>()),
        ("InertiaState", std::mem::size_of::<InertiaState>()),
        ("NotchState", std::mem::size_of::<NotchState>()),
        ("SlewRateState", std::mem::size_of::<SlewRateState>()),
        ("BumpstopState", std::mem::size_of::<BumpstopState>()),
        ("HandsOffState", std::mem::size_of::<HandsOffState>()),
    ];

    for (name, size) in &sizes {
        assert!(
            *size > 0 && *size <= 512,
            "{name} is {size} bytes – expected 1..512 for a fixed-size filter state"
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 7. Individual filter node zero-allocation tests
// ---------------------------------------------------------------------------

#[test]
fn torque_cap_filter_zero_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let max_torque = 0.9f32;
    let state_ptr = &max_torque as *const f32 as *mut u8;

    let guard = fresh_guard();
    for i in 0..100 {
        let mut frame = test_frame(0.5, ((i as f32) / 50.0) - 1.0);
        crate::filters::torque_cap_filter(&mut frame, state_ptr);
        assert!(frame.torque_out.abs() <= max_torque + f32::EPSILON);
    }

    let allocs = guard.allocations_since_start();
    assert_eq!(
        allocs, 0,
        "torque_cap_filter allocated {allocs} times over 100 calls"
    );
    Ok(())
}

#[test]
fn torque_cap_filter_nan_inf_zero_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let max_torque = 0.9f32;
    let state_ptr = &max_torque as *const f32 as *mut u8;

    let guard = fresh_guard();

    let mut frame = test_frame(0.5, f32::NAN);
    crate::filters::torque_cap_filter(&mut frame, state_ptr);
    assert_eq!(frame.torque_out, 0.0, "NaN must map to 0.0 (safe state)");

    let mut frame = test_frame(0.5, f32::INFINITY);
    crate::filters::torque_cap_filter(&mut frame, state_ptr);
    assert_eq!(
        frame.torque_out, 0.0,
        "Infinity must map to 0.0 (safe state)"
    );

    let allocs = guard.allocations_since_start();
    assert_eq!(
        allocs, 0,
        "torque_cap_filter with NaN/Inf allocated {allocs} times"
    );
    Ok(())
}

#[test]
fn slew_rate_filter_zero_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let mut slew_state = SlewRateState::new(100.0);
    let state_ptr = &mut slew_state as *mut SlewRateState as *mut u8;

    // Warm up
    let mut frame = test_frame(0.0, 0.0);
    crate::filters::slew_rate_filter(&mut frame, state_ptr);

    let guard = fresh_guard();
    for _ in 0..100 {
        let mut frame = test_frame(0.5, 1.0);
        crate::filters::slew_rate_filter(&mut frame, state_ptr);
        assert!(frame.torque_out.is_finite());
    }

    let allocs = guard.allocations_since_start();
    assert_eq!(
        allocs, 0,
        "slew_rate_filter allocated {allocs} times over 100 calls"
    );
    Ok(())
}

#[test]
fn notch_filter_zero_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let mut notch_state = NotchState::new(60.0, 2.0, -12.0, 1000.0);
    let state_ptr = &mut notch_state as *mut NotchState as *mut u8;

    // Warm up
    let mut frame = test_frame(0.5, 0.5);
    crate::filters::notch_filter(&mut frame, state_ptr);

    let guard = fresh_guard();
    for _ in 0..100 {
        let mut frame = test_frame(0.5, 0.5);
        crate::filters::notch_filter(&mut frame, state_ptr);
        assert!(frame.torque_out.is_finite());
    }

    let allocs = guard.allocations_since_start();
    assert_eq!(
        allocs, 0,
        "notch_filter allocated {allocs} times over 100 calls"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 8. Performance metrics – zero allocations
// ---------------------------------------------------------------------------

#[test]
fn performance_metrics_calculations_zero_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let metrics = crate::rt::PerformanceMetrics {
        total_ticks: 100_000,
        missed_ticks: 5,
        max_jitter_ns: 500_000,
        p99_jitter_ns: 250_000,
        ..Default::default()
    };

    let guard = fresh_guard();
    let _rate = metrics.missed_tick_rate();
    let _jitter = metrics.p99_jitter_us();

    let allocs = guard.allocations_since_start();
    assert_eq!(
        allocs, 0,
        "PerformanceMetrics calculations allocated {allocs} times"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// 9. Proptest: allocation-free processing across diverse inputs
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn prop_pipeline_process_zero_alloc(
        ffb_in in -1.0f32..=1.0,
        torque_out in -1.0f32..=1.0,
        wheel_speed in 0.0f32..=100.0,
        seq in 0u16..=u16::MAX,
    ) {
        let mut pipeline = Pipeline::new();
        pipeline.set_response_curve(CurveLut::linear());

        let mut frame = Frame {
            ffb_in,
            torque_out,
            wheel_speed,
            hands_off: false,
            ts_mono_ns: 1_000_000,
            seq,
        };

        // Warm up
        let _ = pipeline.process(&mut frame);

        let guard = fresh_guard();
        let mut frame = Frame {
            ffb_in,
            torque_out,
            wheel_speed,
            hands_off: false,
            ts_mono_ns: 1_000_000,
            seq,
        };
        let result = pipeline.process(&mut frame);
        prop_assert!(result.is_ok(), "Pipeline process failed: {:?}", result);

        let allocs = guard.allocations_since_start();
        prop_assert_eq!(allocs, 0, "Pipeline allocated {} times for input ffb_in={}", allocs, ffb_in);
    }

    #[test]
    fn prop_safety_clamp_zero_alloc(
        requested in -100.0f32..=100.0,
    ) {
        let service = test_safety_service();

        let guard = fresh_guard();
        let clamped = service.clamp_torque_nm(requested);

        let allocs = guard.allocations_since_start();
        prop_assert_eq!(allocs, 0, "clamp_torque_nm allocated for input {}", requested);

        // Verify clamped value is within bounds
        let max = service.max_torque_nm();
        prop_assert!(clamped.abs() <= max + f32::EPSILON,
            "Clamped value {} exceeds max torque {}", clamped, max);
    }

    #[test]
    fn prop_curve_lut_lookup_zero_alloc(
        input in -0.5f32..=1.5,
    ) {
        let lut = CurveLut::linear();

        let guard = fresh_guard();
        let _output = lut.lookup(input.clamp(0.0, 1.0));

        let allocs = guard.allocations_since_start();
        prop_assert_eq!(allocs, 0, "CurveLut lookup allocated for input {}", input);
    }

    #[test]
    fn prop_torque_cap_filter_zero_alloc(
        torque_out in -2.0f32..=2.0,
        max_torque in 0.1f32..=1.0,
    ) {
        let state_ptr = &max_torque as *const f32 as *mut u8;

        let guard = fresh_guard();
        let mut frame = test_frame(0.5, torque_out);
        crate::filters::torque_cap_filter(&mut frame, state_ptr);

        let allocs = guard.allocations_since_start();
        prop_assert_eq!(allocs, 0, "torque_cap_filter allocated for torque_out={}", torque_out);
        prop_assert!(frame.torque_out.is_finite(), "Output must be finite");
        prop_assert!(frame.torque_out.abs() <= max_torque + f32::EPSILON,
            "Output {} exceeds cap {}", frame.torque_out, max_torque);
    }

    #[test]
    fn prop_full_rt_chain_zero_alloc(
        ffb_in in -1.0f32..=1.0,
        max_safe in 1.0f32..=10.0,
    ) {
        let service = SafetyService::with_timeouts(
            max_safe,
            max_safe * 2.0,
            Duration::from_secs(3),
            Duration::from_secs(2),
        );

        let mut pipeline = Pipeline::new();
        pipeline.set_response_curve(CurveLut::linear());

        // Warm up
        let mut warmup = test_frame(ffb_in, ffb_in);
        let _ = pipeline.process(&mut warmup);

        let guard = fresh_guard();
        let mut frame = test_frame(ffb_in, ffb_in);
        let result = pipeline.process(&mut frame);
        prop_assert!(result.is_ok());

        let _clamped = service.clamp_torque_nm(frame.torque_out * max_safe);
        let allocs = guard.allocations_since_start();
        prop_assert_eq!(allocs, 0, "Full RT chain allocated {} times", allocs);
    }
}
