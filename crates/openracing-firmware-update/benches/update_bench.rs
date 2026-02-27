//! Benchmarks for firmware update operations

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use openracing_firmware_update::delta::{
    apply_simple_patch, compress_data, compute_data_hash, create_simple_patch, decompress_data,
};
use openracing_firmware_update::prelude::*;

fn bench_compression(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression");

    for size in [100, 1000, 10000, 100000].iter() {
        let data: Vec<u8> = (0..*size).map(|i| (i % 256) as u8).collect();

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::new("compress", size), &data, |b, data| {
            b.iter(|| compress_data(data).expect("Compression failed"));
        });

        let compressed = compress_data(&data).expect("Compression failed");
        group.bench_with_input(
            BenchmarkId::new("decompress", size),
            &compressed,
            |b, data| {
                b.iter(|| decompress_data(data).expect("Decompression failed"));
            },
        );
    }

    group.finish();
}

fn bench_hashing(c: &mut Criterion) {
    let mut group = c.benchmark_group("hashing");

    for size in [100, 1000, 10000, 100000].iter() {
        let data: Vec<u8> = (0..*size).map(|i| (i % 256) as u8).collect();

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::new("sha256", size), &data, |b, data| {
            b.iter(|| compute_data_hash(data));
        });
    }

    group.finish();
}

fn bench_delta_patching(c: &mut Criterion) {
    let mut group = c.benchmark_group("delta_patching");

    for size in [100, 1000, 10000].iter() {
        let old_data: Vec<u8> = (0..*size).map(|i| (i % 256) as u8).collect();
        let new_data: Vec<u8> = old_data.iter().map(|&b| b.wrapping_add(1)).collect();

        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(
            BenchmarkId::new("create_patch", size),
            &(&old_data, &new_data),
            |b, (old, new)| {
                b.iter(|| {
                    let rt = tokio::runtime::Runtime::new().expect("Runtime creation failed");
                    rt.block_on(async {
                        let old_path = std::env::temp_dir().join("bench_old.bin");
                        let new_path = std::env::temp_dir().join("bench_new.bin");
                        tokio::fs::write(&old_path, old)
                            .await
                            .expect("Write failed");
                        tokio::fs::write(&new_path, new)
                            .await
                            .expect("Write failed");
                        create_delta_patch(&old_path, &new_path)
                            .await
                            .expect("Patch creation failed");
                        tokio::fs::remove_file(&old_path).await.ok();
                        tokio::fs::remove_file(&new_path).await.ok();
                    })
                });
            },
        );

        let patch = create_simple_patch(&old_data, &new_data).expect("Patch creation failed");
        group.bench_with_input(
            BenchmarkId::new("apply_patch", size),
            &(&old_data, &patch),
            |b, (old, patch)| {
                b.iter(|| apply_simple_patch(old, patch).expect("Patch application failed"));
            },
        );
    }

    group.finish();
}

fn bench_hardware_version(c: &mut Criterion) {
    let mut group = c.benchmark_group("hardware_version");

    let versions = ["1.0", "1.2.3", "10.20.30", "2.0", "10.0"];

    group.bench_function("parse", |b| {
        b.iter(|| {
            for v in &versions {
                let _ = HardwareVersion::parse(v);
            }
        });
    });

    let parsed: Vec<_> = versions
        .iter()
        .map(|v| HardwareVersion::parse(v).expect("Parse failed"))
        .collect();

    group.bench_function("compare", |b| {
        b.iter(|| {
            for i in 0..parsed.len() {
                for j in 0..parsed.len() {
                    let _ = parsed[i].cmp(&parsed[j]);
                }
            }
        });
    });

    group.finish();
}

fn bench_bundle_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("bundle");

    let create_image = |size: usize| -> FirmwareImage {
        let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
        let hash = compute_data_hash(&data);
        FirmwareImage {
            device_model: "test-wheel".to_string(),
            version: semver::Version::new(1, 0, 0),
            min_hardware_version: None,
            max_hardware_version: None,
            data,
            hash,
            size_bytes: size as u64,
            build_timestamp: chrono::Utc::now(),
            release_notes: None,
            signature: None,
        }
    };

    for size in [100, 1000, 10000].iter() {
        let image = create_image(*size);
        let metadata = BundleMetadata::default();

        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(
            BenchmarkId::new("create_none", size),
            &(&image, &metadata),
            |b, (img, meta)| {
                b.iter(|| {
                    FirmwareBundle::new(img, (*meta).clone(), CompressionType::None)
                        .expect("Bundle creation failed")
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("create_gzip", size),
            &(&image, &metadata),
            |b, (img, meta)| {
                b.iter(|| {
                    FirmwareBundle::new(img, (*meta).clone(), CompressionType::Gzip)
                        .expect("Bundle creation failed")
                });
            },
        );

        let bundle = FirmwareBundle::new(&image, metadata.clone(), CompressionType::Gzip)
            .expect("Bundle creation failed");
        let serialized = bundle.serialize().expect("Serialization failed");

        group.bench_with_input(BenchmarkId::new("parse", size), &serialized, |b, data| {
            b.iter(|| FirmwareBundle::parse(data).expect("Bundle parsing failed"));
        });
    }

    group.finish();
}

fn bench_partition_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("partition");

    group.bench_function("partition_other", |b| {
        b.iter(|| {
            let mut p = Partition::A;
            for _ in 0..1000 {
                p = p.other();
            }
            p
        });
    });

    group.finish();
}

fn bench_update_state(c: &mut Criterion) {
    let mut group = c.benchmark_group("update_state");

    let states = [
        UpdateState::Idle,
        UpdateState::Downloading { progress: 50 },
        UpdateState::Verifying,
        UpdateState::Flashing { progress: 75 },
        UpdateState::Rebooting,
        UpdateState::Complete,
        UpdateState::Failed {
            error: "test".to_string(),
            recoverable: true,
        },
    ];

    group.bench_function("is_in_progress", |b| {
        b.iter(|| {
            let mut count = 0;
            for state in &states {
                if state.is_in_progress() {
                    count += 1;
                }
            }
            count
        });
    });

    group.bench_function("should_block_ffb", |b| {
        b.iter(|| {
            let mut count = 0;
            for state in &states {
                if state.should_block_ffb() {
                    count += 1;
                }
            }
            count
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_compression,
    bench_hashing,
    bench_delta_patching,
    bench_hardware_version,
    bench_bundle_operations,
    bench_partition_operations,
    bench_update_state,
);

criterion_main!(benches);
