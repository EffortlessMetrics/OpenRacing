//! Recording throughput benchmarks
//!
//! Measures the performance of blackbox recording operations.

#![allow(clippy::unwrap_used)]

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use openracing_diagnostic::{
    BlackboxConfig, BlackboxRecorder, FrameData, SafetyStateSimple, StreamA, TelemetryData,
};
use std::hint::black_box;
use tempfile::TempDir;

fn bench_stream_a_recording(c: &mut Criterion) {
    let mut group = c.benchmark_group("stream_a");

    let mut stream = StreamA::new();
    let frame = FrameData {
        ffb_in: 0.5,
        torque_out: 0.3,
        wheel_speed: 10.0,
        hands_off: false,
        ts_mono_ns: 1_000_000_000,
        seq: 1,
    };
    let node_outputs = vec![0.1, 0.2, 0.3];

    group.bench_function("record_frame", |b| {
        b.iter(|| {
            stream.record_frame(
                black_box(frame.clone()),
                black_box(&node_outputs),
                black_box(SafetyStateSimple::SafeTorque),
                black_box(150),
            )
        })
    });

    group.finish();
}

fn bench_stream_a_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("stream_a_serialization");

    for count in [100, 500, 1000].iter() {
        group.bench_with_input(BenchmarkId::new("records", count), count, |b, &count| {
            b.iter_batched(
                || {
                    let mut stream = StreamA::new();
                    let frame = FrameData::default();
                    for _ in 0..count {
                        let _ = stream.record_frame(
                            frame.clone(),
                            &[0.1, 0.2],
                            SafetyStateSimple::SafeTorque,
                            100,
                        );
                    }
                    stream
                },
                |mut stream| black_box(stream.get_data()),
                criterion::BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

fn bench_blackbox_recording(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let config = BlackboxConfig::new("bench-device", temp_dir.path());

    let mut group = c.benchmark_group("blackbox_recording");
    group.throughput(Throughput::Elements(1));

    let frame = FrameData {
        ffb_in: 0.5,
        torque_out: 0.3,
        wheel_speed: 10.0,
        hands_off: false,
        ts_mono_ns: 1_000_000_000,
        seq: 1,
    };

    group.bench_function("record_frame", |b| {
        b.iter_batched(
            || BlackboxRecorder::new(config.clone()).unwrap(),
            |mut recorder| {
                recorder
                    .record_frame(
                        black_box(frame.clone()),
                        black_box(&[0.1, 0.2]),
                        black_box(SafetyStateSimple::SafeTorque),
                        black_box(100),
                    )
                    .unwrap();
                recorder
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn bench_telemetry_recording(c: &mut Criterion) {
    let mut group = c.benchmark_group("telemetry_recording");

    let temp_dir = TempDir::new().unwrap();
    let config = BlackboxConfig::new("bench-device", temp_dir.path());
    let mut recorder = BlackboxRecorder::new(config).unwrap();

    let telemetry = TelemetryData {
        ffb_scalar: 0.8,
        rpm: 3000.0,
        speed_ms: 25.0,
        slip_ratio: 0.1,
        gear: 3,
        car_id: Some("test_car".to_string()),
        track_id: Some("test_track".to_string()),
    };

    group.bench_function("record_telemetry", |b| {
        b.iter(|| recorder.record_telemetry(black_box(telemetry.clone())))
    });

    group.finish();
}

fn bench_full_recording_workflow(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_workflow");

    group.bench_function("record_1000_frames", |b| {
        b.iter_batched(
            || {
                let temp_dir = TempDir::new().unwrap();
                let config = BlackboxConfig::new("bench-device", temp_dir.path());
                (BlackboxRecorder::new(config).unwrap(), temp_dir)
            },
            |(mut recorder, _temp_dir)| {
                let frame = FrameData::default();
                for i in 0..1000 {
                    let mut f = frame.clone();
                    f.seq = i as u16;
                    let _ =
                        recorder.record_frame(f, &[0.1, 0.2], SafetyStateSimple::SafeTorque, 100);
                }
                black_box(recorder)
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_stream_a_recording,
    bench_stream_a_serialization,
    bench_blackbox_recording,
    bench_telemetry_recording,
    bench_full_recording_workflow,
);

criterion_main!(benches);
