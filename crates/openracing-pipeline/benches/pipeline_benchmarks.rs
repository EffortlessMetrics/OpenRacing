//! Pipeline Benchmarks
//!
//! This module contains Criterion benchmarks for pipeline operations
//! to verify RT performance requirements.

use criterion::{Criterion, criterion_group, criterion_main};
use openracing_filters::Frame;
use openracing_pipeline::prelude::*;
use racing_wheel_schemas::prelude::{CurvePoint, FrequencyHz, Gain, NotchFilter};
use std::hint::black_box;

fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
    match r {
        Ok(v) => v,
        Err(e) => panic!("must() failed: {:?}", e),
    }
}

fn create_test_config() -> racing_wheel_schemas::entities::FilterConfig {
    racing_wheel_schemas::entities::FilterConfig::new_complete(
        4,
        must(Gain::new(0.1)),
        must(Gain::new(0.15)),
        must(Gain::new(0.05)),
        vec![must(NotchFilter::new(
            must(FrequencyHz::new(60.0)),
            2.0,
            -12.0,
        ))],
        must(Gain::new(0.8)),
        vec![
            must(CurvePoint::new(0.0, 0.0)),
            must(CurvePoint::new(0.5, 0.6)),
            must(CurvePoint::new(1.0, 1.0)),
        ],
        must(Gain::new(0.9)),
        racing_wheel_schemas::entities::BumpstopConfig::default(),
        racing_wheel_schemas::entities::HandsOffConfig::default(),
    )
    .expect("valid config")
}

fn bench_empty_pipeline_process(c: &mut Criterion) {
    let mut pipeline = Pipeline::new();
    let mut frame = Frame::from_ffb(0.5, 5.0);

    c.bench_function("empty_pipeline_process", |b| {
        b.iter(|| {
            let _ = black_box(pipeline.process(black_box(&mut frame)));
        })
    });
}

fn bench_pipeline_swap_atomicity(c: &mut Criterion) {
    let mut pipeline1 = Pipeline::new();
    let pipeline2 = Pipeline::with_hash(0x12345678);

    c.bench_function("pipeline_swap", |b| {
        b.iter(|| {
            pipeline1.swap_at_tick_boundary(black_box(pipeline2.clone()));
        })
    });
}

fn bench_config_hash_calculation(c: &mut Criterion) {
    let config = create_test_config();

    c.bench_function("config_hash_calculation", |b| {
        b.iter(|| black_box(calculate_config_hash(black_box(&config))));
    });
}

fn bench_pipeline_validation(c: &mut Criterion) {
    let validator = PipelineValidator::new();
    let config = create_test_config();

    c.bench_function("pipeline_validation", |b| {
        b.iter(|| black_box(validator.validate_config(black_box(&config))));
    });
}

fn bench_rt_simulation_1khz(c: &mut Criterion) {
    let mut pipeline = Pipeline::new();
    let mut frames: Vec<Frame> = (0..1000)
        .map(|i| Frame::from_ffb((i as f32 / 1000.0).sin(), 5.0))
        .collect();

    let mut group = c.benchmark_group("rt_simulation");
    group.throughput(criterion::Throughput::Elements(1000));

    group.bench_function("1khz_pipeline_loop", |b| {
        b.iter(|| {
            for frame in &mut frames {
                let _ = black_box(pipeline.process(black_box(frame)));
            }
        });
    });

    group.finish();
}

fn bench_pipeline_with_response_curve(c: &mut Criterion) {
    let mut pipeline = Pipeline::new();
    pipeline.set_response_curve(openracing_curves::CurveType::Linear.to_lut());
    let mut frame = Frame::from_ffb(0.5, 5.0);

    c.bench_function("pipeline_with_response_curve", |b| {
        b.iter(|| {
            let _ = black_box(pipeline.process(black_box(&mut frame)));
        })
    });
}

fn bench_pipeline_state_snapshot(c: &mut Criterion) {
    let pipeline = Pipeline::new();

    c.bench_function("pipeline_state_snapshot", |b| {
        b.iter(|| black_box(pipeline.state_snapshot()));
    });
}

fn bench_pipeline_clone(c: &mut Criterion) {
    let pipeline = Pipeline::new();

    c.bench_function("pipeline_clone", |b| {
        b.iter(|| black_box(pipeline.clone()));
    });
}

criterion_group!(
    benches,
    bench_empty_pipeline_process,
    bench_pipeline_swap_atomicity,
    bench_config_hash_calculation,
    bench_pipeline_validation,
    bench_rt_simulation_1khz,
    bench_pipeline_with_response_curve,
    bench_pipeline_state_snapshot,
    bench_pipeline_clone,
);

criterion_main!(benches);
