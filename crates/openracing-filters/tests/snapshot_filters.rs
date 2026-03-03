//! Insta snapshot tests for filter outputs.
//!
//! Each test feeds a known input sequence through a filter and captures the
//! exact output as a snapshot, so any numerical regression is caught.

use openracing_filters::prelude::*;

// ---------------------------------------------------------------------------
// Helper: round to 8 decimal places so snapshots are platform-stable
// ---------------------------------------------------------------------------
fn round8(v: f32) -> f64 {
    // Promote to f64 before rounding to avoid f32 repr noise.
    let v64 = v as f64;
    (v64 * 1e8).round() / 1e8
}

fn collect_rounded(values: &[f32]) -> Vec<f64> {
    values.iter().copied().map(round8).collect()
}

// ---------------------------------------------------------------------------
// Bumpstop
// ---------------------------------------------------------------------------

#[test]
fn bumpstop_step_response_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = BumpstopState::new(true, 450.0, 540.0, 0.8, 0.3);
    let outputs: Vec<f32> = (0..30)
        .map(|_| {
            let mut frame = Frame {
                wheel_speed: 500_000.0,
                ..Frame::default()
            };
            bumpstop_filter(&mut frame, &mut state);
            frame.torque_out
        })
        .collect();

    insta::assert_debug_snapshot!(collect_rounded(&outputs));
    Ok(())
}

#[test]
fn bumpstop_sine_input_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = BumpstopState::new(true, 400.0, 450.0, 0.5, 0.2);
    let outputs: Vec<f32> = (0..40)
        .map(|i| {
            let speed = (i as f32 * 0.3).sin() * 200_000.0;
            let mut frame = Frame {
                wheel_speed: speed,
                ..Frame::default()
            };
            bumpstop_filter(&mut frame, &mut state);
            frame.torque_out
        })
        .collect();

    insta::assert_debug_snapshot!(collect_rounded(&outputs));
    Ok(())
}

// ---------------------------------------------------------------------------
// Damper – fixed mode
// ---------------------------------------------------------------------------

#[test]
fn damper_fixed_step_response_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let state = DamperState::fixed(0.15);
    let outputs: Vec<f32> = (0..20)
        .map(|i| {
            let speed = if i < 5 { 0.0 } else { 5.0 };
            let mut frame = Frame {
                wheel_speed: speed,
                ..Frame::default()
            };
            damper_filter(&mut frame, &state);
            frame.torque_out
        })
        .collect();

    insta::assert_debug_snapshot!(collect_rounded(&outputs));
    Ok(())
}

#[test]
fn damper_fixed_ramp_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let state = DamperState::fixed(0.2);
    let outputs: Vec<f32> = (0..20)
        .map(|i| {
            let speed = i as f32 * 0.5;
            let mut frame = Frame {
                wheel_speed: speed,
                ..Frame::default()
            };
            damper_filter(&mut frame, &state);
            frame.torque_out
        })
        .collect();

    insta::assert_debug_snapshot!(collect_rounded(&outputs));
    Ok(())
}

// ---------------------------------------------------------------------------
// Damper – adaptive mode
// ---------------------------------------------------------------------------

#[test]
fn damper_adaptive_step_response_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let state = DamperState::adaptive(0.15);
    let outputs: Vec<f32> = (0..20)
        .map(|i| {
            let speed = if i < 5 { 0.0 } else { 5.0 };
            let mut frame = Frame {
                wheel_speed: speed,
                ..Frame::default()
            };
            damper_filter(&mut frame, &state);
            frame.torque_out
        })
        .collect();

    insta::assert_debug_snapshot!(collect_rounded(&outputs));
    Ok(())
}

#[test]
fn damper_adaptive_ramp_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let state = DamperState::adaptive(0.2);
    let outputs: Vec<f32> = (0..20)
        .map(|i| {
            let speed = i as f32 * 0.5;
            let mut frame = Frame {
                wheel_speed: speed,
                ..Frame::default()
            };
            damper_filter(&mut frame, &state);
            frame.torque_out
        })
        .collect();

    insta::assert_debug_snapshot!(collect_rounded(&outputs));
    Ok(())
}

// ---------------------------------------------------------------------------
// Friction
// ---------------------------------------------------------------------------

#[test]
fn friction_fixed_step_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let state = FrictionState::fixed(0.15);
    let outputs: Vec<f32> = (0..20)
        .map(|i| {
            let speed = if i < 5 { 0.0 } else { 3.0 };
            let mut frame = Frame {
                wheel_speed: speed,
                ..Frame::default()
            };
            friction_filter(&mut frame, &state);
            frame.torque_out
        })
        .collect();

    insta::assert_debug_snapshot!(collect_rounded(&outputs));
    Ok(())
}

#[test]
fn friction_adaptive_ramp_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let state = FrictionState::adaptive(0.2);
    let outputs: Vec<f32> = (0..20)
        .map(|i| {
            let speed = i as f32 * 0.5;
            let mut frame = Frame {
                wheel_speed: speed,
                ..Frame::default()
            };
            friction_filter(&mut frame, &state);
            frame.torque_out
        })
        .collect();

    insta::assert_debug_snapshot!(collect_rounded(&outputs));
    Ok(())
}

#[test]
fn friction_sine_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let state = FrictionState::fixed(0.1);
    let outputs: Vec<f32> = (0..30)
        .map(|i| {
            let speed = (i as f32 * 0.2).sin() * 5.0;
            let mut frame = Frame {
                wheel_speed: speed,
                ..Frame::default()
            };
            friction_filter(&mut frame, &state);
            frame.torque_out
        })
        .collect();

    insta::assert_debug_snapshot!(collect_rounded(&outputs));
    Ok(())
}

// ---------------------------------------------------------------------------
// Inertia
// ---------------------------------------------------------------------------

#[test]
fn inertia_step_response_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = InertiaState::new(0.15);
    let outputs: Vec<f32> = (0..20)
        .map(|i| {
            let speed = if i < 5 { 0.0 } else { 5.0 };
            let mut frame = Frame {
                wheel_speed: speed,
                ..Frame::default()
            };
            inertia_filter(&mut frame, &mut state);
            frame.torque_out
        })
        .collect();

    insta::assert_debug_snapshot!(collect_rounded(&outputs));
    Ok(())
}

#[test]
fn inertia_sine_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = InertiaState::new(0.1);
    let outputs: Vec<f32> = (0..30)
        .map(|i| {
            let speed = (i as f32 * 0.3).sin() * 5.0;
            let mut frame = Frame {
                wheel_speed: speed,
                ..Frame::default()
            };
            inertia_filter(&mut frame, &mut state);
            frame.torque_out
        })
        .collect();

    insta::assert_debug_snapshot!(collect_rounded(&outputs));
    Ok(())
}

#[test]
fn inertia_ramp_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = InertiaState::new(0.2);
    let outputs: Vec<f32> = (0..20)
        .map(|i| {
            let speed = i as f32 * 0.5;
            let mut frame = Frame {
                wheel_speed: speed,
                ..Frame::default()
            };
            inertia_filter(&mut frame, &mut state);
            frame.torque_out
        })
        .collect();

    insta::assert_debug_snapshot!(collect_rounded(&outputs));
    Ok(())
}

// ---------------------------------------------------------------------------
// Slew rate limiter
// ---------------------------------------------------------------------------

#[test]
fn slew_rate_step_up_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = SlewRateState::new(0.5);
    let outputs: Vec<f32> = (0..30)
        .map(|_| {
            let mut frame = Frame {
                torque_out: 1.0,
                ..Frame::default()
            };
            slew_rate_filter(&mut frame, &mut state);
            frame.torque_out
        })
        .collect();

    insta::assert_debug_snapshot!(collect_rounded(&outputs));
    Ok(())
}

#[test]
fn slew_rate_step_down_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = SlewRateState::new(0.5);
    state.prev_output = 1.0;
    let outputs: Vec<f32> = (0..30)
        .map(|_| {
            let mut frame = Frame {
                torque_out: -1.0,
                ..Frame::default()
            };
            slew_rate_filter(&mut frame, &mut state);
            frame.torque_out
        })
        .collect();

    insta::assert_debug_snapshot!(collect_rounded(&outputs));
    Ok(())
}

#[test]
fn slew_rate_sine_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = SlewRateState::new(1.0);
    let outputs: Vec<f32> = (0..30)
        .map(|i| {
            let target = (i as f32 * 0.3).sin();
            let mut frame = Frame {
                torque_out: target,
                ..Frame::default()
            };
            slew_rate_filter(&mut frame, &mut state);
            frame.torque_out
        })
        .collect();

    insta::assert_debug_snapshot!(collect_rounded(&outputs));
    Ok(())
}

// ---------------------------------------------------------------------------
// Notch filter
// ---------------------------------------------------------------------------

#[test]
fn notch_impulse_response_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = NotchState::new(50.0, 2.0, -6.0, 1000.0);
    let outputs: Vec<f32> = (0..30)
        .map(|i| {
            let input = if i == 0 { 1.0 } else { 0.0 };
            let mut frame = Frame {
                torque_out: input,
                ..Frame::default()
            };
            notch_filter(&mut frame, &mut state);
            frame.torque_out
        })
        .collect();

    insta::assert_debug_snapshot!(collect_rounded(&outputs));
    Ok(())
}

#[test]
fn notch_sine_at_center_frequency_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let center_freq = 50.0_f32;
    let sample_rate = 1000.0_f32;
    let mut state = NotchState::new(center_freq, 2.0, -6.0, sample_rate);

    let outputs: Vec<f32> = (0..40)
        .map(|i| {
            let t = i as f32 / sample_rate;
            let input = (2.0 * std::f32::consts::PI * center_freq * t).sin();
            let mut frame = Frame {
                torque_out: input,
                ..Frame::default()
            };
            notch_filter(&mut frame, &mut state);
            frame.torque_out
        })
        .collect();

    insta::assert_debug_snapshot!(collect_rounded(&outputs));
    Ok(())
}

#[test]
fn notch_step_response_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = NotchState::new(100.0, 1.0, -3.0, 1000.0);
    let outputs: Vec<f32> = (0..30)
        .map(|_| {
            let mut frame = Frame {
                torque_out: 1.0,
                ..Frame::default()
            };
            notch_filter(&mut frame, &mut state);
            frame.torque_out
        })
        .collect();

    insta::assert_debug_snapshot!(collect_rounded(&outputs));
    Ok(())
}

// ---------------------------------------------------------------------------
// Hands-off detector
// ---------------------------------------------------------------------------

#[test]
fn hands_off_low_torque_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = HandsOffState::new(true, 0.05, 0.02); // 20ms timeout = 20 ticks
    let results: Vec<bool> = (0..40)
        .map(|_| {
            let mut frame = Frame {
                torque_out: 0.01,
                ..Frame::default()
            };
            hands_off_detector(&mut frame, &mut state);
            frame.hands_off
        })
        .collect();

    insta::assert_debug_snapshot!(results);
    Ok(())
}

#[test]
fn hands_off_intermittent_torque_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = HandsOffState::new(true, 0.05, 0.01); // 10ms timeout = 10 ticks
    let results: Vec<bool> = (0..40)
        .map(|i| {
            // Every 15 ticks, apply a burst of torque
            let torque = if i % 15 == 0 { 0.2 } else { 0.01 };
            let mut frame = Frame {
                torque_out: torque,
                ..Frame::default()
            };
            hands_off_detector(&mut frame, &mut state);
            frame.hands_off
        })
        .collect();

    insta::assert_debug_snapshot!(results);
    Ok(())
}

#[test]
fn hands_off_step_on_off_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = HandsOffState::new(true, 0.05, 0.01); // 10 tick timeout
    let results: Vec<bool> = (0..40)
        .map(|i| {
            // Hands on for first 20 ticks, then off
            let torque = if i < 20 { 0.3 } else { 0.01 };
            let mut frame = Frame {
                torque_out: torque,
                ..Frame::default()
            };
            hands_off_detector(&mut frame, &mut state);
            frame.hands_off
        })
        .collect();

    insta::assert_debug_snapshot!(results);
    Ok(())
}
