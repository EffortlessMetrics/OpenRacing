# Friction Log

Running record of pain points, blockers, and technical debt encountered during development. Reviewed periodically to drive improvements to tooling, process, and architecture.

Each entry has: **date**, **severity** (Low/Medium/High), **status** (Open/Resolved/Won't Fix), and a description + proposed remedy.

---

## Active Issues

### F-001 · Dual game support matrix sync (High · Open)

**Encountered:** RC sprint (multi-vendor/game push)

Two YAML files must always contain identical game entries:
- `crates/telemetry-config/src/game_support_matrix.yaml` (runtime)
- `crates/telemetry-support/src/game_support_matrix.yaml` (tests)

Every time a game is added, both files must be updated manually. When they diverge, `GameService::new()` panics with an opaque "Missing config writer" error rather than a clear sync error.

**Remedy:** Generate one file from the other at build time, or introduce a CI check that diffs the two files and fails if they differ. Long term: merge into a single source of truth consumed by both crates.

**Update (CI check):** `.github/workflows/yaml-sync-check.yml` + `scripts/check_yaml_sync.py` confirmed present. The workflow runs on every push/PR and fails with a clear diff message if the files diverge.

**Update (sync script):** `scripts/sync_yaml.py` added — run `python scripts/sync_yaml.py --fix` after editing the canonical file to keep both files in sync. See F-013 (Resolved). The single-source-of-truth refactor remains a long-term goal.

**Current state:** Files are now identical. The `dirt_rally_2` content divergence (6 lines of `supported_fields`) and `raceroom` omission were resolved. See F-013.

---

### F-013 · No developer-facing sync tool for game support matrix (Medium · Resolved)

**Encountered:** RC sprint / feat/r5-test-coverage-and-integration — F-001 kept recurring because developers had no easy command to sync the two YAML files after editing the canonical one.

The CI check (`check_yaml_sync.py`) would catch divergence at PR time, but offered no fix path — developers had to manually copy the file or hunt down the right diff. This caused repeated F-001/F-013 CI failures.

**Fix applied:** `scripts/sync_yaml.py` added. Run `python scripts/sync_yaml.py --fix` after editing `crates/telemetry-config/src/game_support_matrix.yaml` to copy it to the mirror. Use `--check` in CI or pre-commit hooks to verify without writing. Documented in `docs/DEVELOPMENT.md` under "Keeping game support matrix files in sync". Requires Python 3.8+, no external dependencies.

---

### F-002 · Duplicate config writer registration (High · **Resolved**)

**Encountered:** RC sprint

Every game's config writer must be registered in **two** separate files:
- `crates/telemetry-config/src/writers.rs` (used by `GameService` at runtime)
- `crates/telemetry-config-writers/src/lib.rs` (parallel crate)

Missing one silently causes tests to pass while runtime silently skips the writer. Caused several hard-to-debug failures.

**Fix applied:** `crates/telemetry-config-writers/src/lib.rs` is now the single source of truth for all ConfigWriter implementations. The four writers that existed only in the duplicate (`GranTurismo7ConfigWriter`, `AssettoCorsaConfigWriter`, `ForzaMotorsportConfigWriter`, `BeamNGDriveConfigWriter`) were migrated there. `crates/telemetry-config/src/writers.rs` was replaced with a single re-export line: `pub use racing_wheel_telemetry_config_writers::*;`. The `telemetry-config` crate now lists `racing-wheel-telemetry-config-writers` as a workspace dependency.

---

### F-012 · Manual telemetry configuration required per game (Low · Resolved)

**Encountered:** RC sprint — users had to manually enable UDP telemetry in each game's settings menu and enter the correct port/IP. Easy to miss; caused "no telemetry" support tickets.

**Fix applied:** `crates/service/src/game_auto_configure.rs` writes the required telemetry config file on first game detection; `crates/service/src/game_telemetry_bridge.rs` auto-starts/stops the matching adapter when the game process starts/exits. All 29 supported games are now plug-and-play with zero user setup steps.

---

### F-003 · Race condition: agents editing files during compilation (High · **Resolved**)

**Encountered:** RC sprint — agent-26 modifying `windows.rs` while `cargo check` was running

Concurrent file edits during active builds cause cascading compilation errors (references to constants that briefly exist/don't exist). This is especially bad for agents that progressively refine a large file.

**Fix applied:** `AGENTS.md` updated with a "Multi-agent / worktree rules" section that mandates `git worktree add` per agent, isolation to the agent's own worktree, and the cherry-pick rebase pattern after squash merges. See feat/r7-quirks-cleanup-v2.

---

### F-004 · Windows linker PDB limit in integration tests (Medium · **Resolved**)

**Encountered:** RC sprint — `racing-wheel-integration-tests` fails with LNK1318 / LNK1180 on Windows

The Windows linker hits its PDB symbol table size limit when the integration test crate is built in debug mode with all features, because it transitively pulls in every crate in the workspace.

**Fix applied:** Added `.cargo/config.toml` with:
```toml
[profile.test.package.racing-wheel-integration-tests]
debug = false
```
This disables debug info for the integration test binary on all platforms, avoiding the MSVC symbol limit. Stack traces via `RUST_BACKTRACE=1` still show file:line because Rust embeds panic location by default. (feat/r7-quirks-cleanup-v2)

---

### F-005 · Wrong protocol values in initial implementations (Medium · Resolved)

**Encountered:** RC sprint — multiple vendor protocol crates had wrong VIDs/PIDs on creation, requiring a full web-verification pass (agent-26) to fix:
- Cammus VID `0x3285 → 0x3416`
- Simucube VID `0x2D6A → 0x16D0`
- Asetek VID `0x2E5A → 0x2433`
- Simagic VID `0x2D5C → 0x3670`
- Logitech G923 PS PID `0xC266 → 0xC267`
- Thrustmaster TMX PID `0xB66D → 0xB67F`

Protocol values sourced from memory/guesses rather than verified sources.

**Remedy:** Add a `docs/protocols/SOURCES.md` that records the authoritative source (USB descriptor dump, community wiki URL, official SDK) for every VID/PID. Require a source citation when adding a new device. Add a unit test that cross-references the IDs against a checked-in golden file so a stale value causes a test failure.

**Fix applied:** `docs/protocols/SOURCES.md` added — tables every VID/PID for all 12 vendor protocol crates, with per-entry status (Verified / Community / Estimated) and source URLs. Unit tests added at `crates/hid-moza-protocol/tests/id_verification.rs` that assert all Moza VID/PID constants against the golden values in SOURCES.md, so any future stale constant causes a test failure.

---

### F-006 · Snapshot tests silently encoding wrong values (Medium · Open)

**Encountered:** RC sprint — Simagic snapshot tests were accepted with wrong legacy PIDs (`0x0101–0x0301`) and had to be regenerated after web-verification corrected the PIDs to `0x0500–0x0502`.

Snapshot tests provide no protection against "wrong but consistent" values: the first `--force-update-snapshots` run permanently bakes in whatever the code produces, even if the code is wrong.

**Current state:** Snapshot files in `crates/hid-button-box-protocol/tests/snapshots/` contain plausible values (e.g. `buttons=0x00000000, axis_x=0, axis_y=0, hat=Up` for zero-byte input). No cross-validation against the golden-source file from F-005 has been added; the structural problem — that snapshot tests cannot distinguish "correct" from "consistently wrong" — remains.

**Remedy:** For device ID snapshots, cross-validate against the golden-source file from F-005. Add `// source: <URL>` annotations inside snapshot files for reviewers to verify. Consider a separate "verified constants" test that asserts specific known-good numeric values independently of snapshot infrastructure.

---

### F-013 · YAML sync requires manual update of two identical files (Medium · Open)

**Encountered:** R5 test coverage sprint

Both `crates/telemetry-config/src/game_support_matrix.yaml` and `crates/telemetry-support/src/game_support_matrix.yaml` must be kept identical. Every game addition requires two manual edits. The files have already diverged (see F-001); the CI diff check is the only safety net.

**Remedy:** Make one file a symlink of the other, or auto-generate one from the other in a `build.rs` build script. Long-term: merge into a single source of truth consumed by both crates (tracked in F-001).

---

### F-014 · Agent race conditions on shared branch (High · **Resolved**)

**Encountered:** R5 test coverage sprint — multiple agents operating on `feat/r5-test-coverage-and-integration`

Multiple agents running concurrently on the same branch can cause YAML divergence, merge conflicts, and `workspace-hack` drift. The risk compounds when agents edit overlapping files without coordination.

**Fix applied:** `AGENTS.md` updated with a "Multi-agent / worktree rules" section (same fix as F-003): `git worktree` per agent, isolated directories, cherry-pick rebase pattern, and `cargo hakari generate` reminder. See feat/r7-quirks-cleanup-v2.

---

### F-015 · Workspace-hack requires manual regeneration (Low · **Resolved**)

**Encountered:** R5 test coverage sprint — adding new crates caused `workspace-hack` drift detected by CI

After adding new crates or changing feature flags, `cargo hakari generate` must be re-run manually to keep `workspace-hack/` in sync. CI catches the drift but the fix always requires a manual step.

**Fix applied:** Created `.githooks/pre-commit` — a versioned hook that runs `cargo hakari verify` and diffs the two `game_support_matrix.yaml` files before every commit. Also added `scripts/pre-commit/check-hakari.sh` as a standalone helper. `AGENTS.md` updated with hook setup instructions (`git config core.hooksPath .githooks`) and reminder to run `cargo hakari generate` when adding crates. (feat/r7-quirks-cleanup-v2)

---

### F-016 · `bench_results.json` generation undocumented (Low · **Resolved**)

**Encountered:** R5 maintenance review — `scripts/validate_performance.py bench_results.json --strict` referenced in `CLAUDE.md` but the file is never present in the repo

`bench_results.json` must be generated by running `cargo bench --bench rt_timing` with the env vars `BENCHMARK_JSON_OUTPUT=1` and `BENCHMARK_JSON_PATH=bench_results.json`. This is documented only inside `benches/rt_timing.rs` itself, not in `CLAUDE.md`, `README.md`, or any CI workflow.

**Fix applied:** `CLAUDE.md` updated — "Benchmarks and performance" section now shows the full two-step command: generate with `BENCHMARK_JSON_OUTPUT=1 BENCHMARK_JSON_PATH=bench_results.json cargo bench --bench rt_timing`, then validate. (feat/r7-quirks-cleanup-f007)

---

### F-007 · Symbol renames cascade across many test files (Medium · **Partially Resolved**)

**Encountered:** RC sprint — `ProRacing → GPro`, `PRO_RACING → G_PRO` renames required manual fixes across:
- `logitech_e2e.rs`, `logitech_tests.rs`, `windows_property_tests.rs`, `windows.rs` (tests), etc.

No compile-time help distinguishes "this is a renamed constant" from "this constant was removed."

**Partial fix applied (feat/r7-quirks-cleanup-f007):** The `sequence → frame_seq` rename in the telemetry adapter crates is now complete — all 20 remaining adapter files (`acc.rs`, `ac_rally.rs`, `ams2.rs`, `automobilista.rs`, `dirt_rally_2.rs`, `dirt_showdown.rs`, `dirt3.rs`, `dirt5.rs`, `f1_25.rs`, `f1_native.rs`, `f1.rs`, `grid_2019.rs`, `grid_autosport.rs`, `grid_legends.rs`, `iracing.rs`, `kartkraft.rs`, `lib.rs`, `race_driver_grid.rs`, `rfactor2.rs`, `wtcr.rs`) updated. Function names (`test_parse_realtime_sequence_from_fixtures`) were preserved.

**Remaining:** The structural problem — that renames cascade silently rather than with a deprecation warning — is still open.

**Remedy:** Use the `#[deprecated(since = "...", note = "use X instead")]` attribute on renamed constants/variants for at least one release cycle before removal. This turns silent breakage into a clear deprecation warning.

---

### F-011 · Linux `emit_rt_event` borrow error hidden on Windows (Medium · Resolved)

**Encountered:** PR #15 CI — `UI Isolation Build (ubuntu-24.04)` failed with E0596:
`cannot borrow 'file' as mutable, as it is not declared as mutable` in
`crates/openracing-tracing/src/platform/linux.rs`.

`LinuxTracepointsProvider::trace_file` was `Option<File>` but `TracingProvider::emit_rt_event` takes `&self`, so `write_all` couldn't borrow it mutably. This compiled fine on Windows (only the Windows provider is compiled on that platform).

**Fix applied:** Wrapped in `Option<Mutex<File>>`; `emit_rt_event` uses `try_lock()` — contended writes increment `events_dropped` instead of blocking the RT thread. Commit `1c3fea5`.

**Lesson:** Platform-specific code must be CI-checked on all platforms. A Linux-only compile error was invisible during all local Windows development. See also F-003: the CI platform matrix is the only safety net for cross-platform bugs.

---

### F-008 · BeamNG gear value overflow (Resolved)

**Encountered:** RC sprint — gear field stored as `i8`, underflowed at `0x80` (reverse/neutral boundary in the game's UDP packet)

**Fix applied:** `crates/telemetry-adapters/src/beamng.rs` — cast via `u8` first before `as i8`.

**Lesson:** Game UDP telemetry fields that encode "neutral/reverse" as values near 127/128 should be parsed as `u8` and then mapped to a domain type, never directly to `i8`.

---

### F-009 · `static_mut_refs` denial missing from several crates (Resolved)

**Encountered:** RC sprint — CI caught 4 crates missing `#![deny(static_mut_refs)]`

**Fix applied:** agent-22 added the attribute to `openracing-watchdog` and related crates.

**Lesson:** The attribute should be added by a workspace-level `[lints]` table in `Cargo.toml` so new crates inherit it automatically. Track this as a follow-up cleanup.

---

### F-010 · Integration test function named after old API (Resolved)

**Encountered:** RC sprint — `scenario_pro_racing_uses_1080_degree_range` persisted in `logitech_e2e.rs` long after the `ProRacing → GPro` rename and the `1080° → 900°` correction.

**Fix applied:** agent-30 renamed it to `scenario_g_pro_uses_900_degree_range`.

**Lesson:** Function names in integration tests are documentation; they should be reviewed when the underlying protocol constant is renamed.

---

## Resolved (archive)

| ID | Title | Resolved In |
|----|-------|-------------|
| F-003 | Agent file-edit race during compilation | AGENTS.md worktree rules (feat/r7) |
| F-004 | Windows linker PDB limit in integration tests | .cargo/config.toml debug=false (feat/r7) |
| F-014 | Agent race conditions on shared branch | AGENTS.md worktree rules (feat/r7) |
| F-015 | Workspace-hack requires manual regeneration | .githooks/pre-commit + AGENTS.md (feat/r7) |
| F-008 | BeamNG gear overflow | commit cdd69f0 |
| F-009 | static_mut_refs missing | commit cdd69f0 |
| F-010 | Stale integration test name | agent-30 |
| F-011 | Linux emit_rt_event borrow error | commit 1c3fea5 |
| F-013 | No developer sync tool for game support matrix | scripts/sync_yaml.py |
| F-016 | bench_results.json generation undocumented | CLAUDE.md update (feat/r7) |

---

## Process notes

- **Review cadence:** Check open items at the start of each sprint / major feature push.
- **Adding entries:** When you hit a friction point, add it here before moving on. Don't wait until retrospective.
- **Closing entries:** Mark **Resolved** once the fix lands in `main`; move to the archive table.
- **Escalation:** High-severity open items that block RC should be added to `ROADMAP.md` as concrete work items.
