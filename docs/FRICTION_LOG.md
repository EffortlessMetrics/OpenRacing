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

**Fix applied:** Added `[profile.test.package.racing-wheel-integration-tests] debug = false` to workspace `Cargo.toml`. Disabling debug info for this specific package avoids generating a PDB file that exceeds the linker's symbol table limit, while leaving debug symbols enabled for all other packages. Note: `.cargo/config.toml` cannot be used because `.cargo/` is gitignored (machine-specific). (feat/r7-quirks-cleanup-v2)

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

### F-006 · Snapshot tests silently encoding wrong values (Medium · **Resolved**)

**Encountered:** RC sprint — Simagic snapshot tests were accepted with wrong legacy PIDs (`0x0101–0x0301`) and had to be regenerated after web-verification corrected the PIDs to `0x0500–0x0502`.

Snapshot tests provide no protection against "wrong but consistent" values: the first `--force-update-snapshots` run permanently bakes in whatever the code produces, even if the code is wrong.

**Fix applied (full):** Every HID vendor crate now has a `tests/id_verification.rs` test suite (15 crates total: Moza, Simagic, Cammus, VRS, Asetek, AccuForce, Fanatec, Heusinkveld, Leo Bodnar, Logitech, Simucube, Thrustmaster, OpenFFBoard, FFBeast, button-box) that asserts each VID/PID constant against the golden values in `docs/protocols/SOURCES.md`. Guidance for annotating snapshot files added to `docs/DEVELOPMENT.md` under "Snapshot tests and cross-validation". (feat/r7-quirks-cleanup-v2)

---

### F-013 · YAML sync requires manual update of two identical files (Medium · **Resolved**)

**Encountered:** R5 test coverage sprint

Both `crates/telemetry-config/src/game_support_matrix.yaml` and `crates/telemetry-support/src/game_support_matrix.yaml` must be kept identical. Every game addition requires two manual edits. The files have already diverged (see F-001); the CI diff check is the only safety net.

**Fix applied:** `scripts/sync_yaml.py` added — run `python scripts/sync_yaml.py --fix` after editing the canonical file to copy it to the mirror. The long-term single-source-of-truth refactor remains tracked under F-001. (feat/r5-test-coverage-and-integration)

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

### F-007 · Symbol renames cascade across many test files (Medium · **Resolved**)

**Encountered:** RC sprint — `ProRacing → GPro`, `PRO_RACING → G_PRO` renames required manual fixes across:
- `logitech_e2e.rs`, `logitech_tests.rs`, `windows_property_tests.rs`, `windows.rs` (tests), etc.

No compile-time help distinguishes "this is a renamed constant" from "this constant was removed."

**Fix applied (feat/r7-quirks-cleanup-v2):** The `sequence → frame_seq` rename in the telemetry adapter crates is now complete — all 20 remaining adapter files (`acc.rs`, `ac_rally.rs`, `ams2.rs`, `automobilista.rs`, `dirt_rally_2.rs`, `dirt_showdown.rs`, `dirt3.rs`, `dirt5.rs`, `f1_25.rs`, `f1_native.rs`, `f1.rs`, `grid_2019.rs`, `grid_autosport.rs`, `grid_legends.rs`, `iracing.rs`, `kartkraft.rs`, `lib.rs`, `race_driver_grid.rs`, `rfactor2.rs`, `wtcr.rs`) updated.

**Remaining (structural):** The process issue — that renames cascade silently rather than with a deprecation warning — is still open. Use the `#[deprecated(since = "...", note = "use X instead")]` attribute on renamed constants/variants for at least one release cycle before removal.

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

### F-020 · `cargo tree --duplicates` CI gate overly strict (Medium · Resolved)

**Encountered:** RC sprint (feat/r7-quirks-cleanup-v2) — the `dependency-governance` CI job ran `cargo tree --duplicates` and exited with code 1 on any output. Large workspaces using Tauri, tokio, and wasmtime inevitably have parallel major versions (`syn v1/v2`, `windows-sys`, `zip v2/v7`, etc.).

**Impact:** CI blocked on every PR, including branches that introduced zero new duplicate deps.

**Fix applied:** Changed `exit 1` to `::warning::` GitHub Actions annotation in `.github/workflows/ci.yml`. Duplicate version policy remains enforced by `cargo deny check` (in the `lint-gates` job) which respects `multiple-versions = "warn"` in `deny.toml`. Commit `b9ed332`.

---

### F-021 · Fuzz targets silently unlinked from crate dependencies (Low · Resolved)

**Encountered:** `fuzz_simplemotion.rs` imported `racing_wheel_simplemotion_v2::parse_feedback_report` but `racing-wheel-simplemotion-v2` was absent from `fuzz/Cargo.toml`. The file compiled with a broken import and was never caught by a workspace-level `cargo check`.

**Root cause:** The `fuzz/` directory is an isolated workspace (`[workspace]` in `fuzz/Cargo.toml`). It does not inherit workspace deps and is not checked by the main `cargo check --workspace`.

**Fix applied:** Added `racing-wheel-simplemotion-v2 = { path = "../crates/simplemotion-v2" }` to `fuzz/Cargo.toml`. Commit `4a250f3`.

**Lesson:** When adding a new fuzz target, always add its crate dep to `fuzz/Cargo.toml` and verify with `cargo check --bin fuzz_<name>` from the `fuzz/` directory.

---

### F-022 · ACC2 / AC EVO telemetry — no public protocol docs (Low · Open)

**Encountered:** RC sprint — checked for ACC2 (2025) and AC EVO (2026) telemetry protocols. Neither game has published UDP telemetry documentation.

**Impact:** Cannot implement adapters without community reverse-engineering. These are high-demand titles as they gain adoption.

**Proposed remedy:** Monitor Kunos community forums and GitHub issues; implement once protocol is documented. `seb_loeb_rally.rs` and `f1_manager.rs` are maintained as intentional stubs (no telemetry protocol) for similar reasons.

---

### F-023 · PXN HID report ID byte skipped — all input field offsets off by 1 (High · Resolved)

**Encountered:** PR #18 review (Qodo comment) — `feat/r6-pxn-v2`

`crates/hid-pxn-protocol/src/input.rs` `parse()` was reading steering from `data[0..2]`,
throttle from `data[2..4]`, etc. — treating the raw HID buffer as if byte 0 were the first
data field. But by convention (consistent with every other vendor protocol crate in the
repo), byte 0 of the HID buffer is the HID report ID (`0x01`). All fields were shifted
by one, so the steering angle was actually reading the throttle, the throttle was reading
the brake, etc. Zero-byte buffers appeared to parse to "center/all-zeros" because the
report ID and all data happened to be 0 in most tests.

**Root cause:** New crate authored without cross-checking the Logitech/Fanatec/Moza parsers
for offset convention. No integration test compared parsed steering against an actual
captured HID frame, so the bug was invisible.

**Fix applied:** commit `f8f46a4` on `feat/r6-pxn-v2`:
- `NEED` constant: 10 → 11 (report ID byte + 10 data bytes)
- Added `ParseError::WrongReportId { got: u8 }` variant
- `parse()` validates `data[0] == REPORT_ID` before extracting fields
- All field reads shifted +1 (steering: `data[1..3]`, throttle: `data[3..5]`, etc.)
- All inline tests, property tests, and snapshot tests updated to prepend `REPORT_ID`

**Lesson:** Every HID protocol crate must have at least one test that constructs a minimal
known-good frame (with report ID byte) and asserts the parsed result against expected
normalised values. A golden-frame test would have caught this immediately.

---

### F-024 · `insta` snapshot tests cannot auto-create files in non-interactive CI (Medium · Open)

**Encountered:** RC sprint (feat/r7-quirks-cleanup-v2) — adding new telemetry adapter snapshot tests for `f1_25` and `wrc_10`

When a new snapshot test is added and no `.snap` file exists, `insta` in non-interactive CI mode (`INSTA_UPDATE=no`, the default) fails with "snapshot assertion failed" instead of creating the file. `INSTA_UPDATE=new` is only useful in local interactive runs; CI environments exit without writing the new snapshot to disk.

**Impact:** Every new snapshot test requires manual pre-computation of the expected YAML output and committing a `.snap` file before the test can pass in CI. This is error-prone and time-consuming for complex normalisation output.

**Remedy:**
1. Add a CI job that runs with `INSTA_UPDATE=always` and uploads the generated `.snap` files as artifacts, allowing developers to download and commit them.
2. Alternatively, add a developer script (`scripts/update_snapshots.sh`) that runs `INSTA_UPDATE=always cargo test` and commits the result.
3. Long term: use `insta`'s `force-update-snapshots` feature combined with a dedicated "snapshot refresh" CI workflow that opens a PR with the updated files.

---

### F-025 · Windows PowerShell shell sessions die immediately in agent environment (High · Open)

**Encountered:** RC sprint (feat/r7-quirks-cleanup-v2) — all new PowerShell sessions (sync and async modes) exited with no output

All new PowerShell sessions created via the agent tooling die immediately after creation — `list_powershell` returns "invalid shell ID" for any session created in the current agent turn, even for simple one-liners like `echo test`. Pre-existing sessions from prior turns complete but cannot be re-used. The root cause is unknown (possible Windows credential/profile issue, terminal initialisation error, or environmental state).

**Impact:** Cannot run any shell commands locally — `cargo build`, `git status`, `cargo fmt`, etc. are all unavailable. Must work around by using task sub-agents (which have working shells in a subprocess), or by using file-reading tools only. Significantly degrades agent productivity.

**Remedy:**
1. Investigate Windows PowerShell profile or credential issue — check `$PROFILE` and Windows Event Log.
2. Try resetting the agent environment or restarting VS Code.
3. As a process improvement: document that task sub-agents provide a working shell fallback when the main session shell is broken.

---

### F-026 · Codemasters Mode 1 UDP adapters had systematically wrong byte offsets (High · Resolved)

**Encountered:** RC sprint — telemetry adapter review / byte-offset audit

Seven adapters sharing the Codemasters legacy 66-float UDP packet (DiRT Rally 2, DiRT 3, DiRT 4, Dirt Showdown, GRID 2019, GRID Legends, Race Driver GRID) all had incorrect byte offsets. The offsets were shifted — e.g., throttle was read at offset 108 (actually `wheel_speed_fl`), RPM at 140 (actually `g_force_lon`), gear at offset 124 (actually `brakes`). The `fuel_percent` calculation also multiplied by 100, but the `NormalizedTelemetry` builder clamps to `[0, 1]`, causing fuel to always show 100%. `MIN_PACKET_SIZE` was 252 instead of the correct 264 bytes (66 × f32 LE = 264 bytes).

**Root cause:** The original offsets were likely transcribed from an unverified or mislabelled source and then copy-pasted across all seven files. No golden-packet test existed to catch the mismatch.

**Fix applied:** All seven adapter files corrected to match the community-verified layout documented in community tools like `dr2_logger`. `MIN_PACKET_SIZE` updated to 264. Fuel calculation changed to pass the raw `[0, 1]` value. Affected files: `dirt_rally_2.rs`, `dirt3.rs`, `dirt4.rs`, `dirt_showdown.rs`, `grid_2019.rs`, `grid_legends.rs`, `race_driver_grid.rs`.

**Lesson:** Adapters that share a common packet format should extract the shared parsing logic into a single helper (e.g., `codemasters_mode1_parse()`) so offset definitions exist in exactly one place. Any adapter sharing a format via copy-paste should be flagged in code review.

---

## Resolved (archive)

| ID | Title | Resolved In |
|----|-------|-------------|
| F-007 | Symbol renames cascade (sequence→frame_seq complete) | feat/r7-quirks-cleanup-v2 |
| F-003 | Agent file-edit race during compilation | AGENTS.md worktree rules (feat/r7) |
| F-004 | Windows linker PDB limit in integration tests | Cargo.toml profile.test override (feat/r7) |
| F-006 | Snapshot tests silently encoding wrong values | id_verification tests for all 15 HID crates (feat/r7) |
| F-014 | Agent race conditions on shared branch | AGENTS.md worktree rules (feat/r7) |
| F-015 | Workspace-hack requires manual regeneration | .githooks/pre-commit + AGENTS.md (feat/r7) |
| F-008 | BeamNG gear overflow | commit cdd69f0 |
| F-009 | static_mut_refs missing | commit cdd69f0 |
| F-010 | Stale integration test name | agent-30 |
| F-011 | Linux emit_rt_event borrow error | commit 1c3fea5 |
| F-013 | No developer sync tool for game support matrix | scripts/sync_yaml.py |
| F-016 | bench_results.json generation undocumented | CLAUDE.md update (feat/r7) |
| F-017 | `cargo tree --duplicates` CI check too strict | CI change b9ed332 (feat/r7-quirks-cleanup-v2) |
| F-018 | `fuzz_simplemotion` missing dep in fuzz/Cargo.toml | commit 4a250f3 (feat/r7-quirks-cleanup-v2) |
| F-019 | 6 SimHub adapters returned empty stub telemetry | simhub.rs rewrite e8d9a20 (feat/r7-quirks-cleanup-v2) |
| F-026 | Codemasters Mode 1 UDP adapters wrong byte offsets | 7 adapter files corrected to community-verified layout |

---

## Process notes

- **Review cadence:** Check open items at the start of each sprint / major feature push.
- **Adding entries:** When you hit a friction point, add it here before moving on. Don't wait until retrospective.
- **Closing entries:** Mark **Resolved** once the fix lands in `main`; move to the archive table.
- **Escalation:** High-severity open items that block RC should be added to `ROADMAP.md` as concrete work items.
