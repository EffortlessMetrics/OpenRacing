# RC Readiness Report

**Branch:** `feat/wave15-rc-hardening`
**Generated:** 2025-03-01
**Commit:** `d4e86d81e698aa572095986a87dfc2e8b9eabbcc`

## Test Summary

| Metric | Count |
|--------|------:|
| **Total tests** | **7,592** |
| Unit tests | 5,572 |
| Snapshot tests | 816 |
| Property tests (proptest) | 742 |
| End-to-end (E2E) tests | 435 |
| BDD / acceptance tests | 27 |
| Fuzz targets | 82 |
| Integration test files | 34 |

## Test Types Present

| Type | Files | Notes |
|------|------:|-------|
| Proptest files | 100 | Property-based testing across protocol & engine crates |
| Snapshot test files | 47 | `insta` snapshots for protocol encoding & telemetry |
| Integration test files | 34 | `crates/integration-tests/tests/*.rs` |
| Fuzz targets | 82 | `fuzz/fuzz_targets/` — covers all protocols & telemetry parsers |
| Benchmark suites | 1 | `benches/` — RT timing benchmarks |

## Coverage by Crate Category

| Category | Tests | Key crates |
|----------|------:|------------|
| Telemetry | 1,050 | `telemetry-adapters`, `telemetry-core`, `telemetry-config`, `telemetry-orchestrator` |
| Engine | 795 | `engine` (RT pipeline, filters, HID, safety) |
| Protocols | 593 | `hid-*-protocol`, `simplemotion-v2`, `hbp`, `moza-wheelbase-report` |
| Plugins | 403 | `plugins`, `openracing-wasm-runtime`, `openracing-native-plugin`, `openracing-plugin-abi` |
| Service | 299 | `service` (daemon, IPC, crypto, firmware updates) |
| Schemas | 249 | `schemas` (JSON schema validation, migration, profile inheritance) |
| Integration tests | 241 | `integration-tests` (E2E device pipelines, RC validation) |
| Safety | 193 | `openracing-fmea`, `openracing-watchdog`, `openracing-hardware-watchdog` |
| Filters | 158 | `openracing-filters` (snapshot + property tests) |
| Other / utilities | 3,611 | Curves, crypto, calibration, errors, scheduler, IPC, tracing, etc. |

## Strengths

- **Protocol coverage is comprehensive**: every HID protocol crate has unit, snapshot,
  property, and E2E tests plus a dedicated fuzz target.
- **Telemetry adapters** cover 40+ games with snapshot regression tests across multiple
  schema versions (v2–v9).
- **Property-based testing** is deeply integrated: 742 proptest cases exercise encoding
  round-trips, ID mappings, and safety invariants.
- **Fuzz testing** covers 82 targets spanning all protocol parsers and telemetry decoders.
- **Safety-critical paths** (FMEA, watchdog, hardware watchdog) have dedicated test suites
  including fault-injection and property tests.
- **RC-specific integration tests** exist (`rc_integration_tests.rs`, 48 tests).

## Known Gaps

| Gap | Severity | Notes |
|-----|----------|-------|
| No line-level code coverage (e.g., `llvm-cov`) | Medium | Test count is high but uncovered branches are unknown |
| UI crate excluded from test run | Low | `racing-wheel-ui` excluded via `--exclude`; needs separate GUI test strategy |
| Benchmark suite is minimal | Low | Single bench file; RT timing validation relies on CI perf gates |
| `compat` crate has 0 unit tests | Medium | Legacy compatibility layer has no dedicated tests |
| Doc-tests not counted | Low | `cargo test` doc-tests run but are not enumerated in `--list` output |
| No mutation testing in CI | Low | `mutants.toml` exists but results are stale (`mutants.out.old/`) |
