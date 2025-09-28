use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use racing_wheel_engine::{AbsoluteScheduler, Frame, Pipeline, PerformanceMetrics};
use std::time::{Duration, Instant};

fn benchmark_rt_timing(c: &mut Criterion) {
    let mut group = c.benchmark_group("rt_timing");
    
    // Set up RT scheduler
    let mut scheduler = AbsoluteScheduler::new_1khz();
    let mut pipeline = Pipeline::new();
    let mut frame = Frame::default();
    let mut metrics = PerformanceMetrics::default();
    
    group.bench_function("1khz_tick_precision", |b| {
        b.iter(|| {
            let start = Instant::now();
            
            // Simulate 10ms of 1kHz operation (10 ticks)
            for _ in 0..10 {
                let tick_start = Instant::now();
                
                // Wait for next tick
                if let Ok(tick) = scheduler.wait_for_tick() {
                    metrics.total_ticks = tick;
                    
                    // Process frame through pipeline
                    frame.seq = tick as u16;
                    frame.ts_mono_ns = tick_start.elapsed().as_nanos() as u64;
                    
                    let _ = pipeline.process(&mut frame);
                    
                    // Measure jitter
                    let jitter_ns = tick_start.elapsed().as_nanos() as u64;
                    if jitter_ns > metrics.max_jitter_ns {
                        metrics.max_jitter_ns = jitter_ns;
                    }
                } else {
                    metrics.missed_ticks += 1;
                }
            }
            
            black_box(metrics.clone())
        });
    });
    
    group.bench_function("pipeline_processing", |b| {
        b.iter(|| {
            let mut test_frame = Frame {
                ffb_in: 0.5,
                torque_out: 0.0,
                wheel_speed: 1.0,
                hands_off: false,
                ts_mono_ns: 0,
                seq: 0,
            };
            
            let result = pipeline.process(&mut test_frame);
            black_box((test_frame, result))
        });
    });
    
    group.finish();
}

fn benchmark_memory_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory");
    
    group.bench_function("zero_alloc_pipeline", |b| {
        let mut pipeline = Pipeline::new();
        let mut frame = Frame::default();
        
        b.iter(|| {
            // This should not allocate on the heap
            let result = pipeline.process(&mut frame);
            black_box((frame, result))
        });
    });
    
    group.finish();
}

criterion_group!(benches, benchmark_rt_timing, benchmark_memory_usage);
criterion_main!(benches);