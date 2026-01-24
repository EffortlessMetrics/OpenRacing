# AGENTS.md

This file guides automated agents working in this repository. Follow it alongside `docs/DEVELOPMENT.md`.

## Project summary
- OpenRacing is a Rust workspace for safety-critical, real-time force feedback.
- The real-time (RT) path runs at 1kHz with strict latency and allocation rules.
- Plugins support both WASM (safe, sandboxed) and native (fast, RT) implementations.

## Key locations
- `crates/engine`: core RT pipeline, safety, diagnostics
- `crates/plugins`: plugin runtime (WASM + native)
- `crates/cli`: CLI tooling
- `crates/service`: background service and integration
- `crates/integration-tests`: integration + acceptance tests
- `docs/`: development, ADRs, and system design

## Must-follow engineering rules
- **No RT allocations** after initialization. Avoid heap usage in RT code paths.
- **No blocking in RT**: no I/O, locks, or syscalls in RT hot paths.
- **No `static mut`**: use `OnceLock`, `LazyLock`, atomics, or other safe patterns.
- Keep execution **bounded and deterministic** in RT code.
- Respect safety interlocks and fault response guarantees.

## Architecture changes
- Significant architectural changes require an ADR. See `docs/adr/README.md`.

## Code style and linting
- Format: `cargo fmt --all`
- Lints: `cargo clippy --all-targets --all-features -- -D warnings`
- Prefer small, readable diffs; keep APIs consistent across crates.

## Testing and validation
- Unit + integration tests: `cargo test --all-features --workspace`
- RT performance profile: `cargo build --profile rt --bin wheeld`
- Benchmarks: `cargo bench --bench rt_timing`
- Performance gates: `python scripts/validate_performance.py bench_results.json --strict`
- ADR validation: `python scripts/validate_adr.py --verbose`
- Docs index: `python scripts/generate_docs_index.py`
- Docs build: `cargo doc --all-features --workspace`

## Dependency and config hygiene
- Use workspace dependencies where possible (see root `Cargo.toml`).
- If you add or update dependencies, update `Cargo.lock`.
- Check `deny.toml` for allowed licenses and advisories.

## Platform considerations
- This project targets Windows, Linux, and macOS.
- Avoid OS-specific assumptions unless the module is platform-specific.
- Keep cross-platform code paths behaviorally aligned.

## When editing safety-critical code
- Add or update tests (including fault injection where relevant).
- Validate timing and performance requirements.
- Document behavioral changes in `docs/` and/or ADRs.
