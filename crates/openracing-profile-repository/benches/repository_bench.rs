//! Performance benchmarks for profile repository operations

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use ed25519_dalek::SigningKey;
use openracing_profile_repository::{ProfileRepository, ProfileRepositoryConfig, ProfileSigner};
use racing_wheel_schemas::prelude::{BaseSettings, Profile, ProfileId, ProfileScope};
use rand::rngs::OsRng;
use std::hint::black_box;
use tempfile::TempDir;

fn create_test_profile(id: &str) -> Profile {
    let profile_id = ProfileId::new(id.to_string()).expect("valid id");
    Profile::new(
        profile_id,
        ProfileScope::global(),
        BaseSettings::default(),
        format!("Benchmark Profile {}", id),
    )
}

fn bench_save_profile(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("runtime");

    c.bench_function("save_profile", |b| {
        b.iter(|| {
            rt.block_on(async {
                let temp_dir = TempDir::new().expect("temp dir");
                let config = ProfileRepositoryConfig::new(temp_dir.path());
                let repo = ProfileRepository::new(config).await.expect("repo");
                let profile = create_test_profile("bench_save");
                repo.save_profile(&profile, None).await.expect("save");
            });
        });
    });
}

fn bench_load_profile(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("runtime");

    c.bench_function("load_profile", |b| {
        b.iter(|| {
            rt.block_on(async {
                let temp_dir = TempDir::new().expect("temp dir");
                let config = ProfileRepositoryConfig::new(temp_dir.path());
                let repo = ProfileRepository::new(config).await.expect("repo");
                let profile = create_test_profile("bench_load");
                repo.save_profile(&profile, None).await.expect("save");
                repo.load_profile(&profile.id).await.expect("load");
            });
        });
    });
}

fn bench_list_profiles(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("runtime");

    let mut group = c.benchmark_group("list_profiles");
    for count in [10, 50, 100].iter() {
        group.bench_with_input(BenchmarkId::new("count", count), count, |b, &count| {
            b.iter(|| {
                rt.block_on(async move {
                    let temp_dir = TempDir::new().expect("temp dir");
                    let config = ProfileRepositoryConfig::new(temp_dir.path());
                    let repo = ProfileRepository::new(config).await.expect("repo");
                    for i in 0..count {
                        let profile = create_test_profile(&format!("list_{}", i));
                        repo.save_profile(&profile, None).await.expect("save");
                    }
                    repo.list_profiles().await.expect("list");
                });
            });
        });
    }
    group.finish();
}

fn bench_sign_profile(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("runtime");

    c.bench_function("sign_profile", |b| {
        b.iter(|| {
            rt.block_on(async {
                let temp_dir = TempDir::new().expect("temp dir");
                let config = ProfileRepositoryConfig::new(temp_dir.path());
                let repo = ProfileRepository::new(config).await.expect("repo");
                let profile = create_test_profile("bench_sign");
                let mut csprng = OsRng;
                let signing_key = SigningKey::generate(&mut csprng);
                repo.save_profile(&profile, Some(&signing_key))
                    .await
                    .expect("save");
            });
        });
    });
}

fn bench_merge_profiles(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("runtime");

    c.bench_function("merge_profiles", |b| {
        b.iter(|| {
            rt.block_on(async {
                let temp_dir = TempDir::new().expect("temp dir");
                let config = ProfileRepositoryConfig::new(temp_dir.path());
                let repo = ProfileRepository::new(config).await.expect("repo");
                let base = create_test_profile("merge_base");
                let other = create_test_profile("merge_other");
                repo.merge_profiles_deterministic(&base, &other)
                    .expect("merge");
            });
        });
    });
}

fn bench_hash_json(c: &mut Criterion) {
    let json = r#"{
        "schema": "wheel.profile/1",
        "scope": { "game": "iracing" },
        "base": {
            "ffbGain": 0.8,
            "dorDeg": 540,
            "torqueCapNm": 15.0,
            "filters": {
                "reconstruction": 4,
                "friction": 0.1,
                "damper": 0.15,
                "inertia": 0.05,
                "notchFilters": [],
                "slewRate": 0.8,
                "curvePoints": [
                    {"input": 0.0, "output": 0.0},
                    {"input": 1.0, "output": 1.0}
                ]
            }
        }
    }"#;

    c.bench_function("hash_json", |b| {
        b.iter(|| ProfileSigner::hash_json(black_box(json)));
    });
}

criterion_group!(
    benches,
    bench_save_profile,
    bench_load_profile,
    bench_list_profiles,
    bench_sign_profile,
    bench_merge_profiles,
    bench_hash_json,
);

criterion_main!(benches);
