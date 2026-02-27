//! Filter Benchmarks
//!
//! This module contains Criterion benchmarks for all filter implementations
//! to verify RT performance requirements.

use criterion::{Criterion, criterion_group, criterion_main};
use openracing_filters::prelude::*;

fn create_test_frame(ffb_in: f32, wheel_speed: f32) -> Frame {
    Frame {
        ffb_in,
        torque_out: ffb_in,
        wheel_speed,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    }
}

fn bench_reconstruction_filter(c: &mut Criterion) {
    let mut state = ReconstructionState::new(4);
    let mut frame = create_test_frame(0.5, 0.0);

    c.bench_function("reconstruction_filter", |b| {
        b.iter(|| {
            reconstruction_filter(std::hint::black_box(&mut frame), std::hint::black_box(&mut state));
        })
    });
}

fn bench_friction_filter(c: &mut Criterion) {
    let state = FrictionState::new(0.1, true);
    let mut frame = create_test_frame(0.5, 5.0);

    c.bench_function("friction_filter", |b| {
        b.iter(|| {
            friction_filter(std::hint::black_box(&mut frame), std::hint::black_box(&state));
        })
    });
}

fn bench_damper_filter(c: &mut Criterion) {
    let state = DamperState::new(0.1, true);
    let mut frame = create_test_frame(0.5, 5.0);

    c.bench_function("damper_filter", |b| {
        b.iter(|| {
            damper_filter(std::hint::black_box(&mut frame), std::hint::black_box(&state));
        })
    });
}

fn bench_inertia_filter(c: &mut Criterion) {
    let mut state = InertiaState::new(0.1);
    let mut frame = create_test_frame(0.5, 5.0);

    c.bench_function("inertia_filter", |b| {
        b.iter(|| {
            inertia_filter(std::hint::black_box(&mut frame), std::hint::black_box(&mut state));
        })
    });
}

fn bench_notch_filter(c: &mut Criterion) {
    let mut state = NotchState::new(50.0, 2.0, -6.0, 1000.0);
    let mut frame = create_test_frame(0.5, 0.0);

    c.bench_function("notch_filter", |b| {
        b.iter(|| {
            notch_filter(std::hint::black_box(&mut frame), std::hint::black_box(&mut state));
        })
    });
}

fn bench_slew_rate_filter(c: &mut Criterion) {
    let mut state = SlewRateState::new(0.5);
    let mut frame = create_test_frame(0.5, 0.0);

    c.bench_function("slew_rate_filter", |b| {
        b.iter(|| {
            slew_rate_filter(std::hint::black_box(&mut frame), std::hint::black_box(&mut state));
        })
    });
}

fn bench_curve_filter(c: &mut Criterion) {
    let points = [(0.0f32, 0.0f32), (0.5f32, 0.25f32), (1.0f32, 1.0f32)];
    let state = CurveState::new(&points);
    let mut frame = create_test_frame(0.5, 0.0);

    c.bench_function("curve_filter", |b| {
        b.iter(|| {
            curve_filter(std::hint::black_box(&mut frame), std::hint::black_box(&state));
        })
    });
}

fn bench_response_curve_filter(c: &mut Criterion) {
    let state = ResponseCurveState::linear();
    let mut frame = create_test_frame(0.5, 0.0);

    c.bench_function("response_curve_filter", |b| {
        b.iter(|| {
            response_curve_filter(std::hint::black_box(&mut frame), std::hint::black_box(&state));
        })
    });
}

fn bench_bumpstop_filter(c: &mut Criterion) {
    let mut state = BumpstopState::new(true, 450.0, 540.0, 0.8, 0.3);
    let mut frame = create_test_frame(0.5, 100.0);

    c.bench_function("bumpstop_filter", |b| {
        b.iter(|| {
            bumpstop_filter(std::hint::black_box(&mut frame), std::hint::black_box(&mut state));
        })
    });
}

fn bench_hands_off_detector(c: &mut Criterion) {
    let mut state = HandsOffState::new(true, 0.05, 2.0);
    let mut frame = create_test_frame(0.01, 0.0);

    c.bench_function("hands_off_detector", |b| {
        b.iter(|| {
            hands_off_detector(std::hint::black_box(&mut frame), std::hint::black_box(&mut state));
        })
    });
}

fn bench_torque_cap_filter(c: &mut Criterion) {
    let mut frame = create_test_frame(0.5, 0.0);

    c.bench_function("torque_cap_filter", |b| {
        b.iter(|| {
            torque_cap_filter(std::hint::black_box(&mut frame), std::hint::black_box(0.8));
        })
    });
}

fn bench_combined_filters(c: &mut Criterion) {
    let mut recon_state = ReconstructionState::new(4);
    let friction_state = FrictionState::new(0.1, true);
    let damper_state = DamperState::new(0.1, true);
    let mut inertia_state = InertiaState::new(0.1);
    let mut slew_state = SlewRateState::new(0.5);
    let response_state = ResponseCurveState::linear();

    let mut frame = create_test_frame(0.5, 5.0);

    c.bench_function("combined_filters", |b| {
        b.iter(|| {
            reconstruction_filter(std::hint::black_box(&mut frame), std::hint::black_box(&mut recon_state));
            friction_filter(std::hint::black_box(&mut frame), std::hint::black_box(&friction_state));
            damper_filter(std::hint::black_box(&mut frame), std::hint::black_box(&damper_state));
            inertia_filter(std::hint::black_box(&mut frame), std::hint::black_box(&mut inertia_state));
            slew_rate_filter(std::hint::black_box(&mut frame), std::hint::black_box(&mut slew_state));
            response_curve_filter(std::hint::black_box(&mut frame), std::hint::black_box(&response_state));
            torque_cap_filter(std::hint::black_box(&mut frame), std::hint::black_box(0.8));
        })
    });
}

fn bench_curve_state_lookup(c: &mut Criterion) {
    let state = CurveState::scurve();

    c.bench_function("curve_state_lookup", |b| {
        b.iter(|| {
            for i in 0..100 {
                let input = (i as f32) / 100.0;
                std::hint::black_box(state.lookup(std::hint::black_box(input)));
            }
        })
    });
}

fn bench_response_curve_lookup(c: &mut Criterion) {
    let state = ResponseCurveState::soft();

    c.bench_function("response_curve_lookup", |b| {
        b.iter(|| {
            for i in 0..100 {
                let input = (i as f32) / 100.0;
                std::hint::black_box(state.lookup(std::hint::black_box(input)));
            }
        })
    });
}

criterion_group!(
    benches,
    bench_reconstruction_filter,
    bench_friction_filter,
    bench_damper_filter,
    bench_inertia_filter,
    bench_notch_filter,
    bench_slew_rate_filter,
    bench_curve_filter,
    bench_response_curve_filter,
    bench_bumpstop_filter,
    bench_hands_off_detector,
    bench_torque_cap_filter,
    bench_combined_filters,
    bench_curve_state_lookup,
    bench_response_curve_lookup,
);

criterion_main!(benches);
