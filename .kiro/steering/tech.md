# Technology Stack

## Language & Toolchain
- Rust 1.89+ (Edition 2024)
- Cargo workspace with resolver v2
- Dual license: MIT OR Apache-2.0

## Core Dependencies
- **Async Runtime**: tokio 1.49+ (full features)
- **Serialization**: serde, serde_json, serde_yaml_ng
- **IPC/RPC**: tonic, prost (Protocol Buffers)
- **Tracing**: tracing, tracing-subscriber
- **Memory**: mimalloc (RT allocator), parking_lot, crossbeam
- **HID**: hidapi, libudev (Linux)
- **Schema Validation**: jsonschema
- **Testing**: criterion, proptest, mockall, wiremock

## Build Profiles
- `dev`: Standard development with panic=abort
- `release`: LTO enabled, single codegen unit, panic=abort
- `rt`: Inherits release, no debug, no overflow checks (for RT components)

## Common Commands

```bash
# Build
cargo build --release
cargo build --profile rt --bin wheeld

# Test
cargo test --all-features --workspace

# Format & Lint
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings

# Benchmarks
cargo bench --bench rt_timing

# Documentation
cargo doc --all-features --workspace

# Dependency audit
cargo deny check
cargo audit
```

## Code Quality Gates
- rustfmt: max_width=100, edition=2024
- clippy: cognitive-complexity-threshold=30, too-many-arguments-threshold=8
- cargo-deny: License allowlist (MIT, Apache-2.0, BSD, ISC)

## Required Lint Attributes
All non-test crates must include:
```rust
#![deny(static_mut_refs)]
#![deny(unused_must_use)]
#![deny(clippy::unwrap_used)]
```

## Memory Safety Rules
- No `static mut` - use `std::sync::OnceLock` or atomics
- Prefer `AtomicBool`, `OnceLock`, `LazyLock` over unsafe static patterns
