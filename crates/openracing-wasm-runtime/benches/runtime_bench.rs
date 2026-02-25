//! Criterion benchmarks for WASM runtime operations.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use openracing_wasm_runtime::{ResourceLimits, WasmRuntime};
use std::hint::black_box;
use uuid::Uuid;

fn create_process_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
                local.get 1
                f32.add
            )
        )
        "#,
    )
    .expect("Failed to parse WAT")
}

fn create_complex_process_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                (local $sum f32)
                (local $i i32)
                (local.set $sum (f32.const 0))
                (local.set $i (i32.const 0))
                (block $break
                    (loop $continue
                        (br_if $break (i32.ge_s (local.get $i) (i32.const 100)))
                        (local.set $sum 
                            (f32.add 
                                (local.get $sum) 
                                (f32.mul (local.get 0) (f32.convert_i32_s (local.get $i)))
                            )
                        )
                        (local.set $i (i32.add (local.get $i) (i32.const 1)))
                        (br $continue)
                    )
                )
                (local.get $sum)
            )
        )
        "#,
    )
    .expect("Failed to parse WAT")
}

fn bench_runtime_creation(c: &mut Criterion) {
    c.bench_function("runtime_creation/default", |b| {
        b.iter(|| black_box(WasmRuntime::new().expect("Failed to create runtime")))
    });

    c.bench_function("runtime_creation/with_limits", |b| {
        let limits = ResourceLimits::default()
            .with_memory(8 * 1024 * 1024)
            .with_fuel(5_000_000);
        b.iter(|| black_box(WasmRuntime::with_limits(limits).expect("Failed to create runtime")))
    });
}

fn bench_plugin_loading(c: &mut Criterion) {
    let mut group = c.benchmark_group("plugin_loading");

    let wasm = create_process_wasm();
    let complex_wasm = create_complex_process_wasm();

    group.bench_function("simple_wasm", |b| {
        b.iter_batched(
            || {
                let runtime = WasmRuntime::new().expect("Failed to create runtime");
                let id = Uuid::new_v4();
                (runtime, id)
            },
            |(mut runtime, id)| {
                runtime
                    .load_plugin_from_bytes(id, &wasm, vec![])
                    .expect("Failed to load");
                black_box(runtime)
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("complex_wasm", |b| {
        b.iter_batched(
            || {
                let runtime = WasmRuntime::new().expect("Failed to create runtime");
                let id = Uuid::new_v4();
                (runtime, id)
            },
            |(mut runtime, id)| {
                runtime
                    .load_plugin_from_bytes(id, &complex_wasm, vec![])
                    .expect("Failed to load");
                black_box(runtime)
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn bench_plugin_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("plugin_processing");

    let wasm = create_process_wasm();
    let complex_wasm = create_complex_process_wasm();

    let mut runtime = WasmRuntime::new().expect("Failed to create runtime");
    let simple_id = Uuid::new_v4();
    runtime
        .load_plugin_from_bytes(simple_id, &wasm, vec![])
        .expect("Failed to load");

    let complex_id = Uuid::new_v4();
    runtime
        .load_plugin_from_bytes(complex_id, &complex_wasm, vec![])
        .expect("Failed to load");

    group.bench_function("simple_process", |b| {
        b.iter(|| {
            black_box(
                runtime
                    .process(&simple_id, 1.0, 0.001)
                    .expect("Process failed"),
            )
        })
    });

    group.bench_function("complex_process", |b| {
        b.iter(|| {
            black_box(
                runtime
                    .process(&complex_id, 1.0, 0.001)
                    .expect("Process failed"),
            )
        })
    });

    group.finish();
}

fn bench_hot_reload(c: &mut Criterion) {
    let mut group = c.benchmark_group("hot_reload");

    let wasm = create_process_wasm();

    group.bench_function("reload", |b| {
        b.iter_batched(
            || {
                let mut runtime = WasmRuntime::new().expect("Failed to create runtime");
                let id = Uuid::new_v4();
                runtime
                    .load_plugin_from_bytes(id, &wasm, vec![])
                    .expect("Failed to load");
                (runtime, id)
            },
            |(mut runtime, id)| {
                runtime
                    .reload_plugin(&id, &wasm, vec![])
                    .expect("Reload failed");
                black_box(runtime)
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn bench_plugin_unload(c: &mut Criterion) {
    let wasm = create_process_wasm();

    c.bench_function("plugin_unload", |b| {
        b.iter_batched(
            || {
                let mut runtime = WasmRuntime::new().expect("Failed to create runtime");
                let id = Uuid::new_v4();
                runtime
                    .load_plugin_from_bytes(id, &wasm, vec![])
                    .expect("Failed to load");
                (runtime, id)
            },
            |(mut runtime, id)| {
                runtime.unload_plugin(&id).expect("Failed to unload");
                black_box(runtime)
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

fn bench_multiple_plugins(c: &mut Criterion) {
    let mut group = c.benchmark_group("multiple_plugins");

    let wasm = create_process_wasm();

    for count in [1, 5, 10, 20].iter() {
        group.bench_with_input(
            BenchmarkId::new("process_all", count),
            count,
            |b, &count| {
                let mut runtime = WasmRuntime::new().expect("Failed to create runtime");
                let ids: Vec<Uuid> = (0..count).map(|_| Uuid::new_v4()).collect();

                for id in &ids {
                    runtime
                        .load_plugin_from_bytes(*id, &wasm, vec![])
                        .expect("Failed to load");
                }

                b.iter(|| {
                    for id in &ids {
                        black_box(runtime.process(id, 1.0, 0.001).expect("Process failed"));
                    }
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_runtime_creation,
    bench_plugin_loading,
    bench_plugin_processing,
    bench_hot_reload,
    bench_plugin_unload,
    bench_multiple_plugins,
);

criterion_main!(benches);
