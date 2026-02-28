//! Benchmark tests for curve evaluation.
//!
//! Run with: cargo bench --bench curve_benchmarks

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use openracing_curves::{BezierCurve, CurveLut, CurveType};

fn bench_linear_curve_evaluate(c: &mut Criterion) {
    let curve = CurveType::Linear;
    let inputs: Vec<f32> = (0..=1000).map(|i| i as f32 / 1000.0).collect();

    c.bench_function("linear_evaluate", |b| {
        b.iter(|| {
            for &input in &inputs {
                std::hint::black_box(curve.evaluate(std::hint::black_box(input)));
            }
        });
    });
}

fn bench_exponential_curve_evaluate(c: &mut Criterion) {
    let curve = CurveType::exponential(2.0).unwrap();
    let inputs: Vec<f32> = (0..=1000).map(|i| i as f32 / 1000.0).collect();

    c.bench_function("exponential_evaluate", |b| {
        b.iter(|| {
            for &input in &inputs {
                std::hint::black_box(curve.evaluate(std::hint::black_box(input)));
            }
        });
    });
}

fn bench_logarithmic_curve_evaluate(c: &mut Criterion) {
    let curve = CurveType::logarithmic(10.0).unwrap();
    let inputs: Vec<f32> = (0..=1000).map(|i| i as f32 / 1000.0).collect();

    c.bench_function("logarithmic_evaluate", |b| {
        b.iter(|| {
            for &input in &inputs {
                std::hint::black_box(curve.evaluate(std::hint::black_box(input)));
            }
        });
    });
}

fn bench_bezier_curve_map(c: &mut Criterion) {
    let curve = BezierCurve::ease_in_out();
    let inputs: Vec<f32> = (0..=1000).map(|i| i as f32 / 1000.0).collect();

    c.bench_function("bezier_map", |b| {
        b.iter(|| {
            for &input in &inputs {
                std::hint::black_box(curve.map(std::hint::black_box(input)));
            }
        });
    });
}

fn bench_lut_lookup_linear(c: &mut Criterion) {
    let lut = CurveLut::linear();
    let inputs: Vec<f32> = (0..=1000).map(|i| i as f32 / 1000.0).collect();

    c.bench_function("lut_lookup_linear", |b| {
        b.iter(|| {
            for &input in &inputs {
                std::hint::black_box(lut.lookup(std::hint::black_box(input)));
            }
        });
    });
}

fn bench_lut_lookup_bezier(c: &mut Criterion) {
    let curve = BezierCurve::ease_in_out();
    let lut = curve.to_lut();
    let inputs: Vec<f32> = (0..=1000).map(|i| i as f32 / 1000.0).collect();

    c.bench_function("lut_lookup_bezier", |b| {
        b.iter(|| {
            for &input in &inputs {
                std::hint::black_box(lut.lookup(std::hint::black_box(input)));
            }
        });
    });
}

fn bench_lut_creation(c: &mut Criterion) {
    let curve = BezierCurve::ease_in_out();

    c.bench_function("lut_creation", |b| {
        b.iter(|| std::hint::black_box(curve.to_lut()));
    });
}

fn bench_curve_type_to_lut(c: &mut Criterion) {
    let curves = vec![
        ("linear", CurveType::Linear),
        ("exponential", CurveType::exponential(2.0).unwrap()),
        ("logarithmic", CurveType::logarithmic(10.0).unwrap()),
        ("bezier", CurveType::Bezier(BezierCurve::ease_in_out())),
    ];

    for (name, curve) in curves {
        c.bench_function(&format!("to_lut_{}", name), |b| {
            b.iter(|| std::hint::black_box(curve.to_lut()));
        });
    }
}

fn bench_single_lookup_rt_path(c: &mut Criterion) {
    let lut = CurveLut::linear();

    c.bench_function("single_lut_lookup", |b| {
        b.iter(|| std::hint::black_box(lut.lookup(std::hint::black_box(0.5))));
    });
}

fn bench_rt_simulation_1khz(c: &mut Criterion) {
    let lut = CurveLut::linear();

    let mut group = c.benchmark_group("rt_simulation");
    group.throughput(Throughput::Elements(1000));

    group.bench_function("1khz_lookup_loop", |b| {
        b.iter(|| {
            for i in 0..1000 {
                let input = (i as f32 % 256.0) / 255.0;
                std::hint::black_box(lut.lookup(std::hint::black_box(input)));
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_linear_curve_evaluate,
    bench_exponential_curve_evaluate,
    bench_logarithmic_curve_evaluate,
    bench_bezier_curve_map,
    bench_lut_lookup_linear,
    bench_lut_lookup_bezier,
    bench_lut_creation,
    bench_curve_type_to_lut,
    bench_single_lookup_rt_path,
    bench_rt_simulation_1khz,
);

criterion_main!(benches);
