//! Benchmarks for FMEA operations.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use openracing_fmea::prelude::*;
use std::time::Duration;

fn bench_fault_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("fault_detection");

    // USB fault detection
    group.bench_function("detect_usb_fault_no_fault", |b| {
        let mut fmea = FmeaSystem::new();
        b.iter(|| fmea.detect_usb_fault(black_box(0), black_box(Some(Duration::ZERO))));
    });

    group.bench_function("detect_usb_fault_at_threshold", |b| {
        let mut fmea = FmeaSystem::new();
        b.iter(|| fmea.detect_usb_fault(black_box(3), black_box(Some(Duration::ZERO))));
    });

    // Encoder fault detection
    group.bench_function("detect_encoder_fault_valid", |b| {
        let mut fmea = FmeaSystem::new();
        b.iter(|| fmea.detect_encoder_fault(black_box(1.5)));
    });

    group.bench_function("detect_encoder_fault_nan", |b| {
        let mut fmea = FmeaSystem::new();
        b.iter(|| fmea.detect_encoder_fault(black_box(f32::NAN)));
    });

    // Thermal fault detection
    group.bench_function("detect_thermal_fault", |b| {
        let mut fmea = FmeaSystem::new();
        b.iter(|| fmea.detect_thermal_fault(black_box(75.0), black_box(false)));
    });

    // Timing violation detection
    group.bench_function("detect_timing_violation", |b| {
        let mut fmea = FmeaSystem::new();
        b.iter(|| fmea.detect_timing_violation(black_box(300)));
    });

    group.finish();
}

fn bench_fault_handling(c: &mut Criterion) {
    let mut group = c.benchmark_group("fault_handling");

    group.bench_function("handle_fault_soft_stop", |b| {
        let mut fmea = FmeaSystem::new();
        b.iter(|| fmea.handle_fault(black_box(FaultType::UsbStall), black_box(10.0)));
    });

    group.bench_function("handle_fault_quarantine", |b| {
        let mut fmea = FmeaSystem::new();
        b.iter(|| fmea.handle_fault(black_box(FaultType::PluginOverrun), black_box(10.0)));
    });

    group.bench_function("clear_fault", |b| {
        let mut fmea = FmeaSystem::new();
        b.iter(|| {
            fmea.handle_fault(FaultType::UsbStall, 10.0).unwrap();
            fmea.clear_fault()
        });
    });

    group.finish();
}

fn bench_soft_stop(c: &mut Criterion) {
    let mut group = c.benchmark_group("soft_stop");

    group.bench_function("start_soft_stop", |b| {
        let mut ctrl = SoftStopController::new();
        b.iter(|| {
            ctrl.start_soft_stop(black_box(10.0));
            ctrl.reset();
        });
    });

    group.bench_function("update_soft_stop", |b| {
        let mut ctrl = SoftStopController::new();
        ctrl.start_soft_stop(10.0);
        b.iter(|| ctrl.update(black_box(Duration::from_micros(100))));
    });

    group.bench_function("full_soft_stop_cycle", |b| {
        let mut ctrl = SoftStopController::new();
        b.iter(|| {
            ctrl.start_soft_stop(10.0);
            for _ in 0..100 {
                ctrl.update(Duration::from_micros(500));
            }
            ctrl.reset();
        });
    });

    group.finish();
}

fn bench_fmea_system_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("system_creation");

    group.bench_function("new_fmea_system", |b| {
        b.iter(|| FmeaSystem::new());
    });

    group.bench_function("new_fmea_system_custom_thresholds", |b| {
        b.iter(|| {
            let thresholds = FaultThresholds::conservative();
            FmeaSystem::with_thresholds(thresholds)
        });
    });

    group.finish();
}

fn bench_audio_alerts(c: &mut Criterion) {
    let mut group = c.benchmark_group("audio_alerts");

    group.bench_function("trigger_alert", |b| {
        let mut system = AudioAlertSystem::new();
        b.iter(|| system.trigger(black_box(AudioAlert::DoubleBeep), black_box(0)));
    });

    group.bench_function("update_alerts", |b| {
        let mut system = AudioAlertSystem::new();
        system.trigger(AudioAlert::DoubleBeep, 0);
        b.iter(|| system.update(black_box(100)));
    });

    group.finish();
}

fn bench_fmea_matrix(c: &mut Criterion) {
    let mut group = c.benchmark_group("fmea_matrix");

    group.bench_function("get_entry", |b| {
        let matrix = FmeaMatrix::with_defaults();
        b.iter(|| matrix.get(black_box(FaultType::UsbStall)));
    });

    group.bench_function("insert_entry", |b| {
        let mut matrix = FmeaMatrix::new();
        let entry = FmeaEntry::new(FaultType::UsbStall);
        matrix.insert(entry).unwrap();
        b.iter(|| {
            let entry = FmeaEntry::new(black_box(FaultType::ThermalLimit));
            matrix.insert(entry)
        });
    });

    group.finish();
}

fn bench_recovery(c: &mut Criterion) {
    let mut group = c.benchmark_group("recovery");

    group.bench_function("create_recovery_context", |b| {
        b.iter(|| RecoveryContext::new(black_box(FaultType::UsbStall)));
    });

    group.bench_function("advance_recovery_step", |b| {
        let mut ctx = RecoveryContext::new(FaultType::UsbStall);
        ctx.start(Duration::ZERO);
        b.iter(|| ctx.advance_step(black_box(Duration::from_millis(100))));
    });

    group.bench_function("check_recovery_timeout", |b| {
        let mut ctx = RecoveryContext::new(FaultType::UsbStall);
        ctx.start(Duration::ZERO);
        b.iter(|| ctx.is_timed_out(black_box(Duration::from_secs(1))));
    });

    group.finish();
}

fn bench_statistics(c: &mut Criterion) {
    let mut group = c.benchmark_group("statistics");

    group.bench_function("get_fault_statistics", |b| {
        let fmea = FmeaSystem::new();
        b.iter(|| fmea.fault_statistics().count());
    });

    group.bench_function("reset_detection_state", |b| {
        let mut fmea = FmeaSystem::new();
        b.iter(|| fmea.reset_detection_state(black_box(FaultType::UsbStall)));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_fault_detection,
    bench_fault_handling,
    bench_soft_stop,
    bench_fmea_system_creation,
    bench_audio_alerts,
    bench_fmea_matrix,
    bench_recovery,
    bench_statistics,
);

criterion_main!(benches);
