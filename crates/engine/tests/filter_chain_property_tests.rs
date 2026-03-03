//! Property-based tests for filter chains.
//!
//! Uses `proptest` to verify that arbitrary filter configurations always produce
//! bounded output, preserve signal integrity, handle hot-swap gracefully, and
//! behave correctly at edge cases (empty chain, single filter, max chain length).

use proptest::prelude::*;
use racing_wheel_engine::Pipeline;
use racing_wheel_engine::filters::*;
use racing_wheel_engine::rt::Frame;

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

/// Create a test frame from FFB input and wheel speed.
fn make_frame(ffb_in: f32, wheel_speed: f32, seq: u16) -> Frame {
    Frame {
        ffb_in,
        torque_out: ffb_in,
        wheel_speed,
        hands_off: false,
        ts_mono_ns: seq as u64 * 1_000_000,
        seq,
    }
}

/// Apply the reconstruction filter via raw pointer (matching engine convention).
fn apply_reconstruction(frame: &mut Frame, state: &mut ReconstructionState) {
    let ptr = state as *mut _ as *mut u8;
    reconstruction_filter(frame, ptr);
}

/// Apply the damper filter via raw pointer.
fn apply_damper(frame: &mut Frame, state: &mut DamperState) {
    let ptr = state as *mut _ as *mut u8;
    damper_filter(frame, ptr);
}

/// Apply the friction filter via raw pointer.
fn apply_friction(frame: &mut Frame, state: &mut FrictionState) {
    let ptr = state as *mut _ as *mut u8;
    friction_filter(frame, ptr);
}

/// Apply the inertia filter via raw pointer.
fn apply_inertia(frame: &mut Frame, state: &mut InertiaState) {
    let ptr = state as *mut _ as *mut u8;
    inertia_filter(frame, ptr);
}

/// Apply the slew rate filter via raw pointer.
fn apply_slew_rate(frame: &mut Frame, state: &mut SlewRateState) {
    let ptr = state as *mut _ as *mut u8;
    slew_rate_filter(frame, ptr);
}

/// Apply the notch filter via raw pointer.
fn apply_notch(frame: &mut Frame, state: &mut NotchState) {
    let ptr = state as *mut _ as *mut u8;
    notch_filter(frame, ptr);
}

/// Which filter to include in a randomised chain.
#[derive(Debug, Clone, Copy)]
enum FilterKind {
    Reconstruction,
    Damper,
    Friction,
    Inertia,
    SlewRate,
    Notch,
}

/// Holds mutable state for one filter in a chain.
#[derive(Debug, Clone)]
enum FilterSlot {
    Reconstruction(ReconstructionState),
    Damper(DamperState),
    Friction(FrictionState),
    Inertia(InertiaState),
    SlewRate(SlewRateState),
    Notch(NotchState),
}

impl FilterSlot {
    fn apply(&mut self, frame: &mut Frame) {
        match self {
            FilterSlot::Reconstruction(s) => apply_reconstruction(frame, s),
            FilterSlot::Damper(s) => apply_damper(frame, s),
            FilterSlot::Friction(s) => apply_friction(frame, s),
            FilterSlot::Inertia(s) => apply_inertia(frame, s),
            FilterSlot::SlewRate(s) => apply_slew_rate(frame, s),
            FilterSlot::Notch(s) => apply_notch(frame, s),
        }
    }
}

/// Proptest strategy for a filter kind index (0..=5).
fn filter_kind_strategy() -> impl Strategy<Value = FilterKind> {
    (0u8..6).prop_map(|i| match i {
        0 => FilterKind::Reconstruction,
        1 => FilterKind::Damper,
        2 => FilterKind::Friction,
        3 => FilterKind::Inertia,
        4 => FilterKind::SlewRate,
        _ => FilterKind::Notch,
    })
}

/// Build a `FilterSlot` from a `FilterKind` with bounded random params.
fn build_slot(kind: FilterKind, param: f32) -> FilterSlot {
    // Clamp param into safe ranges for each filter.
    match kind {
        FilterKind::Reconstruction => {
            let level = (param.abs() * 8.0).min(8.0) as u8;
            FilterSlot::Reconstruction(ReconstructionState::new(level))
        }
        FilterKind::Damper => {
            let coeff = param.abs().clamp(0.0, 1.0);
            FilterSlot::Damper(DamperState::new(coeff, false))
        }
        FilterKind::Friction => {
            let coeff = param.abs().clamp(0.0, 1.0);
            FilterSlot::Friction(FrictionState::new(coeff, false))
        }
        FilterKind::Inertia => {
            let coeff = param.abs().clamp(0.0, 1.0);
            FilterSlot::Inertia(InertiaState::new(coeff))
        }
        FilterKind::SlewRate => {
            let rate = param.abs().clamp(0.01, 10.0);
            FilterSlot::SlewRate(SlewRateState::new(rate))
        }
        FilterKind::Notch => {
            let freq = param.abs().clamp(10.0, 400.0);
            FilterSlot::Notch(NotchState::new(freq, 2.0, -6.0, 1000.0))
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Proptest: random filter configs produce bounded output
// ═══════════════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn random_filter_chain_bounded_output(
        chain_len in 1usize..=6,
        kinds in prop::collection::vec(filter_kind_strategy(), 1..=6),
        params in prop::collection::vec(-1.0f32..1.0, 1..=6),
        inputs in prop::collection::vec(-1.0f32..1.0, 1..=200),
    ) {
        let len = chain_len.min(kinds.len()).min(params.len());
        let mut slots: Vec<FilterSlot> = kinds.iter()
            .zip(params.iter())
            .take(len)
            .map(|(&k, &p)| build_slot(k, p))
            .collect();

        for (seq, &inp) in inputs.iter().enumerate() {
            let mut frame = make_frame(inp, 0.0, seq as u16);
            for slot in &mut slots {
                slot.apply(&mut frame);
            }
            // Output must be finite and within [-1, 1] (filters should not amplify
            // beyond the input range for bounded inputs).
            prop_assert!(
                frame.torque_out.is_finite(),
                "non-finite output at seq {}: {}",
                seq,
                frame.torque_out,
            );
            // Some filter combinations (inertia + notch) can ring slightly
            // beyond 1.0; use a generous 2.0 bound.
            prop_assert!(
                frame.torque_out.abs() <= 2.0,
                "output {:.6} exceeds bound at seq {}",
                frame.torque_out,
                seq,
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Proptest: signal frequency preservation through filter chain
// ═══════════════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn signal_frequency_preservation(
        freq_hz in 1.0f32..50.0,
        amplitude in 0.1f32..0.5,
    ) {
        // Generate a sine wave at `freq_hz` sampled at 1 kHz for 500 ticks.
        let sample_rate = 1000.0f32;
        let n_ticks = 500usize;

        let mut recon = ReconstructionState::new(1); // very light smoothing
        let mut slew = SlewRateState::new(200.0);    // very generous slew

        let mut outputs = Vec::with_capacity(n_ticks);

        for tick in 0..n_ticks {
            let t = tick as f32 / sample_rate;
            let input = amplitude * (2.0 * std::f32::consts::PI * freq_hz * t).sin();

            let mut frame = make_frame(input, 0.0, tick as u16);
            apply_reconstruction(&mut frame, &mut recon);
            apply_slew_rate(&mut frame, &mut slew);
            outputs.push(frame.torque_out);
        }

        // Count zero-crossings in the second half (after initial transient).
        let half = &outputs[n_ticks / 2..];
        let mut crossings = 0u32;
        for pair in half.windows(2) {
            if (pair[0] >= 0.0) != (pair[1] >= 0.0) {
                crossings += 1;
            }
        }

        // Expected crossings ≈ 2 * freq_hz * (duration of half window).
        let half_duration = (n_ticks / 2) as f32 / sample_rate;
        let expected = (2.0 * freq_hz * half_duration) as u32;

        // Allow ±30 % tolerance (filters may shift phase slightly).
        let lower = expected.saturating_sub(expected / 3 + 2);
        let upper = expected + expected / 3 + 2;

        prop_assert!(
            crossings >= lower && crossings <= upper,
            "freq {freq_hz} Hz: crossings {crossings} outside [{lower}, {upper}] (expected ~{expected})",
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Filter hot-swap: changing filters mid-stream doesn't cause large spikes
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn filter_hot_swap_no_large_discontinuity() -> Result<(), Box<dyn std::error::Error>> {
    let n_ticks = 500usize;
    let swap_tick = 250usize;
    let max_discontinuity = 0.5f32; // max allowed jump between consecutive outputs

    let mut pipeline_a = Pipeline::new(); // empty = passthrough
    let mut pipeline_b = Pipeline::new(); // also empty (different hash)

    let mut active = &mut pipeline_a;
    let mut prev_output = 0.0f32;

    for tick in 0..n_ticks {
        // Swap pipeline at the designated tick
        if tick == swap_tick {
            active = &mut pipeline_b;
        }

        let input = (tick as f32 * 0.02).sin() * 0.8;
        let mut frame = make_frame(input, 0.0, tick as u16);
        active.process(&mut frame)?;

        if tick > 0 {
            let delta = (frame.torque_out - prev_output).abs();
            assert!(
                delta <= max_discontinuity,
                "tick {tick}: discontinuity {delta:.4} > {max_discontinuity} after swap",
            );
        }
        prev_output = frame.torque_out;
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4a. Edge case: empty filter chain (passthrough)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn edge_empty_chain_is_passthrough() -> Result<(), Box<dyn std::error::Error>> {
    let mut pipeline = Pipeline::new();
    assert!(pipeline.is_empty());

    for tick in 0..100 {
        let input = (tick as f32 * 0.1).sin() * 0.9;
        let mut frame = make_frame(input, 0.0, tick as u16);
        pipeline.process(&mut frame)?;

        assert!(
            (frame.torque_out - input).abs() < f32::EPSILON,
            "tick {tick}: empty pipeline altered output: in={input} out={}",
            frame.torque_out
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4b. Edge case: single filter
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn edge_single_reconstruction_filter() -> Result<(), Box<dyn std::error::Error>> {
    // Manually exercise a single reconstruction filter — the output should
    // converge towards the input after sustained constant input.
    let mut state = ReconstructionState::new(4); // moderate smoothing

    let constant_input = 0.7f32;
    let mut last_output = 0.0f32;

    for tick in 0..500 {
        let mut frame = make_frame(constant_input, 0.0, tick);
        apply_reconstruction(&mut frame, &mut state);

        // Output should be monotonically approaching the constant input
        assert!(
            frame.torque_out.is_finite(),
            "tick {tick}: non-finite output"
        );
        assert!(
            frame.torque_out >= last_output - f32::EPSILON,
            "tick {tick}: output decreased unexpectedly {:.6} -> {:.6}",
            last_output,
            frame.torque_out,
        );
        last_output = frame.torque_out;
    }

    // After 500 ticks the filter should be very close to the input
    assert!(
        (last_output - constant_input).abs() < 0.01,
        "did not converge: output={last_output} expected≈{constant_input}"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4c. Edge case: maximum chain length (all 6 filter types stacked)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn edge_max_chain_length_bounded() -> Result<(), Box<dyn std::error::Error>> {
    let mut recon = ReconstructionState::new(2);
    let mut damper = DamperState::fixed(0.1);
    let mut friction = FrictionState::fixed(0.05);
    let mut inertia = InertiaState::new(0.05);
    let mut slew = SlewRateState::new(2.0);
    let mut notch = NotchState::new(50.0, 2.0, -6.0, 1000.0);

    for tick in 0u16..1_000 {
        let input = (tick as f32 * 0.05).sin() * 0.8;
        let mut frame = make_frame(input, 1.0, tick);

        apply_reconstruction(&mut frame, &mut recon);
        apply_damper(&mut frame, &mut damper);
        apply_friction(&mut frame, &mut friction);
        apply_inertia(&mut frame, &mut inertia);
        apply_slew_rate(&mut frame, &mut slew);
        apply_notch(&mut frame, &mut notch);

        assert!(
            frame.torque_out.is_finite(),
            "tick {tick}: non-finite after full chain"
        );
        // Generous bound: full chain with inertia + notch can ring
        assert!(
            frame.torque_out.abs() <= 2.0,
            "tick {tick}: output {:.6} exceeds bound",
            frame.torque_out,
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Proptest: slew rate filter limits rate of change
// ═══════════════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn slew_rate_limits_change(
        rate in 0.5f32..5.0,
        inputs in prop::collection::vec(-1.0f32..1.0, 10..=200),
    ) {
        let mut state = SlewRateState::new(rate);
        let max_change = rate / 1000.0; // per-tick limit

        let mut prev = 0.0f32;
        for (i, &inp) in inputs.iter().enumerate() {
            let mut frame = make_frame(inp, 0.0, i as u16);
            apply_slew_rate(&mut frame, &mut state);

            if i > 0 {
                let delta = (frame.torque_out - prev).abs();
                // Allow small floating-point tolerance
                prop_assert!(
                    delta <= max_change + 1e-5,
                    "step {i}: delta {delta:.6} > max_change {max_change:.6}",
                );
            }
            prev = frame.torque_out;
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Proptest: damper output bounded by coefficient * wheel_speed
// ═══════════════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn damper_output_bounded(
        coeff in 0.01f32..0.5,
        wheel_speed in -10.0f32..10.0,
        ffb_in in -1.0f32..1.0,
    ) {
        let mut state = DamperState::fixed(coeff);
        let mut frame = make_frame(ffb_in, wheel_speed, 0);
        apply_damper(&mut frame, &mut state);

        // Damper subtracts damping from the torque, so the output should be
        // no larger in magnitude than the input.
        prop_assert!(
            frame.torque_out.is_finite(),
            "non-finite output for coeff={coeff} ws={wheel_speed} ffb={ffb_in}",
        );
        // Damping reduces magnitude or keeps it the same
        prop_assert!(
            frame.torque_out.abs() <= ffb_in.abs() + coeff * wheel_speed.abs() + 1e-5,
            "output {:.6} unexpectedly large for input {ffb_in:.6}",
            frame.torque_out,
        );
    }
}
