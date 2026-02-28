//! Benchmarks for cryptographic operations
//!
//! Run with: cargo bench --bench crypto_bench

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use openracing_crypto::prelude::*;
use openracing_crypto::trust_store::TrustStore;
use std::hint::black_box;

fn bench_key_generation(c: &mut Criterion) {
    c.bench_function("keypair_generation", |b| {
        b.iter(|| {
            let _keypair = KeyPair::generate().expect("key generation failed");
        });
    });
}

fn bench_signing(c: &mut Criterion) {
    let keypair = KeyPair::generate().expect("key generation failed");
    let data = vec![0u8; 1024]; // 1KB of data

    c.bench_function("sign_1kb", |b| {
        b.iter(|| {
            let _sig = Ed25519Signer::sign(black_box(&data), &keypair.signing_key)
                .expect("signing failed");
        });
    });

    let data_10kb = vec![0u8; 10 * 1024];
    c.bench_function("sign_10kb", |b| {
        b.iter(|| {
            let _sig = Ed25519Signer::sign(black_box(&data_10kb), &keypair.signing_key)
                .expect("signing failed");
        });
    });

    let data_1mb = vec![0u8; 1024 * 1024];
    c.bench_function("sign_1mb", |b| {
        b.iter(|| {
            let _sig = Ed25519Signer::sign(black_box(&data_1mb), &keypair.signing_key)
                .expect("signing failed");
        });
    });
}

fn bench_verification(c: &mut Criterion) {
    let keypair = KeyPair::generate().expect("key generation failed");
    let data = vec![0u8; 1024];
    let signature = Ed25519Signer::sign(&data, &keypair.signing_key).expect("signing failed");

    c.bench_function("verify_1kb", |b| {
        b.iter(|| {
            let _valid = Ed25519Verifier::verify(
                black_box(&data),
                black_box(&signature),
                black_box(&keypair.public_key),
            )
            .expect("verification failed");
        });
    });

    let data_10kb = vec![0u8; 10 * 1024];
    let signature_10kb =
        Ed25519Signer::sign(&data_10kb, &keypair.signing_key).expect("signing failed");

    c.bench_function("verify_10kb", |b| {
        b.iter(|| {
            let _valid = Ed25519Verifier::verify(
                black_box(&data_10kb),
                black_box(&signature_10kb),
                black_box(&keypair.public_key),
            )
            .expect("verification failed");
        });
    });

    let data_1mb = vec![0u8; 1024 * 1024];
    let signature_1mb =
        Ed25519Signer::sign(&data_1mb, &keypair.signing_key).expect("signing failed");

    c.bench_function("verify_1mb", |b| {
        b.iter(|| {
            let _valid = Ed25519Verifier::verify(
                black_box(&data_1mb),
                black_box(&signature_1mb),
                black_box(&keypair.public_key),
            )
            .expect("verification failed");
        });
    });
}

fn bench_fingerprint(c: &mut Criterion) {
    let keypair = KeyPair::generate().expect("key generation failed");

    c.bench_function("fingerprint_computation", |b| {
        b.iter(|| {
            let _fp = keypair.public_key.fingerprint();
        });
    });
}

fn bench_trust_store_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("trust_store");

    group.bench_function("add_key", |b| {
        let mut counter = 0u8;
        b.iter(|| {
            let mut store = TrustStore::new_in_memory();
            let key = PublicKey {
                key_bytes: [counter; 32],
                identifier: format!("test-key-{}", counter),
                comment: None,
            };
            counter = counter.wrapping_add(1);
            store
                .add_key(key, TrustLevel::Trusted, None)
                .expect("add failed");
        });
    });

    // Pre-populate a store for lookup benchmarks
    let mut store = TrustStore::new_in_memory();
    let mut fingerprints = Vec::new();
    for i in 0..100 {
        let key = PublicKey {
            key_bytes: [i as u8; 32],
            identifier: format!("test-key-{}", i),
            comment: None,
        };
        let fp = key.fingerprint();
        fingerprints.push(fp);
        store
            .add_key(key, TrustLevel::Trusted, None)
            .expect("add failed");
    }

    group.bench_function("get_public_key", |b| {
        let mut idx = 0;
        b.iter(|| {
            let fp = &fingerprints[idx % fingerprints.len()];
            idx += 1;
            let _key = store.get_public_key(fp);
        });
    });

    group.bench_function("get_trust_level", |b| {
        let mut idx = 0;
        b.iter(|| {
            let fp = &fingerprints[idx % fingerprints.len()];
            idx += 1;
            let _level = store.get_trust_level(fp);
        });
    });

    group.finish();
}

fn bench_base64_operations(c: &mut Criterion) {
    use openracing_crypto::utils::{decode_base64, encode_base64};

    let data = vec![0xABu8; 64]; // Typical signature size

    c.bench_function("base64_encode_64bytes", |b| {
        b.iter(|| {
            let _encoded = encode_base64(black_box(&data));
        });
    });

    let encoded = encode_base64(&data);
    c.bench_function("base64_decode_64bytes", |b| {
        b.iter(|| {
            let _decoded = decode_base64(black_box(&encoded)).expect("decode failed");
        });
    });
}

fn bench_signature_serialization(c: &mut Criterion) {
    let keypair = KeyPair::generate().expect("key generation failed");
    let data = b"test data for benchmarking";
    let signature = Ed25519Signer::sign(data, &keypair.signing_key).expect("signing failed");

    c.bench_function("signature_to_base64", |b| {
        b.iter(|| {
            let _encoded = signature.to_base64();
        });
    });

    let encoded = signature.to_base64();
    c.bench_function("signature_from_base64", |b| {
        b.iter(|| {
            let _sig = Signature::from_base64(black_box(&encoded)).expect("parse failed");
        });
    });
}

fn bench_metadata_creation(c: &mut Criterion) {
    let keypair = KeyPair::generate().expect("key generation failed");
    let data = b"test data for metadata benchmark";

    c.bench_function("sign_with_metadata", |b| {
        b.iter(|| {
            let _metadata = Ed25519Signer::sign_with_metadata(
                black_box(data),
                &keypair,
                "Benchmark Signer",
                ContentType::Plugin,
                Some("Benchmark signature".to_string()),
            )
            .expect("signing failed");
        });
    });
}

fn bench_throughput(c: &mut Criterion) {
    let keypair = KeyPair::generate().expect("key generation failed");

    // Measure throughput for different data sizes
    let sizes = [64, 512, 1024, 4096, 16384, 65536];

    let mut group = c.benchmark_group("signing_throughput");
    for size in sizes.iter() {
        let data = vec![0u8; *size];

        group.throughput(criterion::Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::new("sign", size), &data, |b, data| {
            b.iter(|| {
                let _sig = Ed25519Signer::sign(black_box(data), &keypair.signing_key)
                    .expect("signing failed");
            });
        });
    }
    group.finish();

    let mut group = c.benchmark_group("verification_throughput");
    for size in sizes.iter() {
        let data = vec![0u8; *size];
        let signature = Ed25519Signer::sign(&data, &keypair.signing_key).expect("signing failed");

        group.throughput(criterion::Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::new("verify", size), &data, |b, data| {
            b.iter(|| {
                let _valid = Ed25519Verifier::verify(
                    black_box(data),
                    black_box(&signature),
                    black_box(&keypair.public_key),
                )
                .expect("verification failed");
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_key_generation,
    bench_signing,
    bench_verification,
    bench_fingerprint,
    bench_trust_store_operations,
    bench_base64_operations,
    bench_signature_serialization,
    bench_metadata_creation,
    bench_throughput,
);

criterion_main!(benches);
