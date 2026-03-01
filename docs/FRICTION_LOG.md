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

**Update (shared parsing extracted):** `codemasters_shared.rs` now contains the single shared Mode 1 parsing implementation. All seven adapters delegate to it, removing ~890 lines of duplicated offset logic. This friction point is fully resolved.

---

### F-027 · Forza tire temperatures assumed Kelvin, actually Fahrenheit (Medium · Resolved)

**Encountered:** RC sprint — telemetry adapter protocol verification audit

The Forza adapter comments said "Kelvin" and converted with `k - 273.15`, but Forza Motorsport/Horizon actually sends tire temps in Fahrenheit. The correct conversion is `(f - 32) * 5/9`.

**Evidence:** The `stelmanjones/fmtel` Go library explicitly documents `// Tire temperatures in fahrenheit.` and converts with `(temp - 32) * 5 / 9`. The `mplutka/tm-bt-led` JS implementation does the same. The official Forza "Data Out" format documentation does not annotate units.

**Fix applied:** Updated `forza.rs` comments and conversion. Default fallback changed from `293.15` (20°C in Kelvin) to `68.0` (20°C in Fahrenheit).

---

### F-028 · fuel_percent × 100 bug in LFS, AMS1; f32/f64 mismatch in RaceRoom (Medium · Resolved)

**Encountered:** RC sprint — full adapter fuel_percent audit

Three adapters had fuel bugs found during the systematic audit triggered by F-026:

1. **LFS** (`lfs.rs`): `fuel * 100.0` passed to `.fuel_percent()`, but the builder clamps to [0,1] — fuel always showed 100%.
2. **Automobilista** (`automobilista.rs`): Same `* 100.0` pattern.
3. **RaceRoom** (`raceroom.rs`): FuelLeft/FuelCapacity read as `f32` (4 bytes), but R3E SDK uses `f64` (8 bytes). Reading 4 bytes of a double produces garbage. Added `read_f64_le()` helper.

**Fix applied:** Removed `* 100.0` from LFS/AMS1. Changed RaceRoom to read f64 and cast to f32. Updated snapshot files.

---

### F-029 · cargo-udeps false positives in CI Dependency Governance job (Medium · Investigating)

**Encountered:** Cleanup sprint — CI `dependency-governance` job

`cargo-udeps` flags many workspace dependencies as unused when they are actually consumed transitively, in doc-tests, or in build scripts. Examples include shared utility crates pulled in via `workspace-hack`, `cfg`-gated platform dependencies, and crates used only in `#[doc = include_str!(...)]` or `build.rs`. The false-positive rate is high enough that the job output is noisy and real unused deps are easy to miss.

**Impact:** Developers ignore the CI output because most flagged crates are legitimate. Genuinely unused dependencies accumulate without notice.

**Proposed remedy:**
1. Pin a known-good `cargo-udeps` version and re-evaluate after upstream fixes for transitive/doctest detection.
2. Add an allow-list (`udeps.toml` or inline `#[cfg_attr]` annotations) for confirmed false positives so the CI signal is actionable.
3. Consider supplementing with `cargo machete` which uses a different heuristic and may have fewer false positives for workspace setups.

---

### F-030 · Assetto Corsa adapter used wrong protocol entirely (High · Resolved)

**Encountered:** Protocol verification wave — web research against vpicon/acudp, lmirel/mfc, Kunos C# SDK

AC adapter was parsing 76-byte OutGauge packets (used by LFS, BeamNG) but Assetto Corsa actually uses its own Remote Telemetry UDP protocol with a 3-step handshake and 328-byte RTCarInfo packets. Every field offset was wrong. All AC telemetry was garbage data.

**Fix applied:** Complete rewrite to Remote Telemetry UDP — sends handshake, subscribes to updates, parses RTCarInfo. Integration test updated to mock AC server. Committed as `9365e99`.

---

### F-031 · Simagic M10 PID collision with Simucube 1 (High · Resolved)

**Encountered:** Engine device sync — both Simagic M10 and Simucube SC1 listed at VID 0x16D0, PID 0x0D5A

Agent-20's device sync added "Simagic M10" at PID 0x0D5A, but that PID on VID 0x16D0 belongs to Simucube 1 (confirmed via official Simucube developer docs, gro-ove/actools). The Simagic M10 actually uses VID 0x0483 PID 0x0522 (shared with Alpha family via STM32 bootloader). Also had "Simagic FX" at 0x0D5B — also wrong.

**Fix applied:** Removed ghost M10/FX entries from windows.rs and linux.rs. Added correct Simucube 1 entry. Committed in `54c8b22`.

---

### F-032 · Estimated PIDs for unreleased Simagic devices (Low · Resolved)

**Encountered:** Protocol verification wave

Simagic Alpha EVO (0x0600), Neo (0x0700), and Neo Mini (0x0701) PIDs are estimates based on sequential numbering convention. No community source confirms these values. These devices may not have shipped yet.

**Impact:** If wrong, these devices won't be detected. Low risk — devices will still get some FFB from the Simagic family fallback.

**Remedy:** Acquire hardware captures or wait for community reverse engineering (JacKeTUs/simagic-ff driver updates).

**Merged to main:** PR #19 merge (d6fba74).

---

### F-033 · Simucube Wireless Wheel PID unconfirmed (Low · Resolved)

**Encountered:** Protocol verification wave

Simucube Wireless Wheel (PID 0x0D63) is listed in engine tables but not confirmed in any public source. It's a receiver, not a force feedback device, so we set torque to 0 Nm. If the PID is wrong it won't cause harm (no FFB commands sent to it).

**Merged to main:** PR #19 merge (d6fba74).

---

### F-034 · Shared USB VIDs require PID-based runtime disambiguation (Medium · Resolved)

**Encountered:** Protocol verification wave (Heusinkveld + VRS)

Two USB Vendor IDs are each shared by **three or more** sim racing hardware vendors:

**VID `0x16D0` (MCS Electronics / OpenMoko):**
- Heusinkveld pedals — PIDs `0x1156`–`0x1158`
- Simucube 2 wheelbases (Granite Devices) — PIDs `0x0D5A`–`0x0D66`
- Legacy Simagic — PID `0x0D5A`

**VID `0x0483` (STMicroelectronics):**
- VRS DirectForce Pro — PIDs `0xA355`–`0xA35A`
- Legacy Simagic (Alpha family) — PIDs `0x0522`–`0x0524`
- Cube Controls (PROVISIONAL) — PIDs `0x0C73`–`0x0C75`
- Hundreds of non-sim STM32 devices

None of these vendors own their VID; they sub-license or reuse a chip vendor's default. VID-only matching will mis-identify devices. The engine's `get_vendor_protocol()` already dispatches by PID within each shared VID, but this is fragile: any new vendor shipping on `0x0483` or `0x16D0` with a PID inside an existing range would collide silently.

Additionally, most individual PIDs for these vendors (Heusinkveld, VRS, Cube Controls) are **unverified** in external USB databases (USB-IF, linux-hardware.org, devicehunt.com). They were likely derived from hardware captures or firmware dumps rather than public registries.

**Impact:** Medium — mis-routing a wheelbase as pedals (or vice versa) could send FFB torque commands to a non-actuated device or fail to initialize FFB. Existing PID-range dispatch in `crates/engine/src/hid/vendor/mod.rs` mitigates this today.

**Remedy:** (1) Acquire USB captures from actual hardware for all unverified PIDs. (2) Consider adding a secondary check (e.g. HID usage page, product string) when VID is `0x0483` or `0x16D0` to reduce the risk of PID-only mismatches. (3) Document vendor-specific PID ranges as reserved in a shared constants file so new vendors don't accidentally overlap.

**Merged to main:** PR #19 merge (d6fba74).

---

### F-035 · PCars2 adapter rewritten from fabricated offsets to correct SMS UDP v2 format (High · Resolved)

**Encountered:** RC telemetry adapter audit (2025-06)

The Project CARS 2 telemetry adapter used entirely fabricated byte offsets that did not correspond to the actual SMS UDP v2 protocol. Field positions were wrong for speed, RPM, gear, and throttle/brake inputs, resulting in garbage telemetry data at runtime.

**Fix applied:** Complete rewrite of the PCars2 adapter to use correct SMS UDP v2 packet format with verified struct offsets from the official Slightly Mad Studios documentation. Snapshot tests added and passing.

---

### F-036 · Leo Bodnar PID 0xBEEF confirmed as placeholder — no real hardware match found (Low · Resolved)

**Encountered:** RC device verification audit (2025-06)

The SLI-M entry in `hid-leo-bodnar-protocol` uses PID `0xBEEF`, which is a common development placeholder value. Checked: devicehunt.com, linux-hardware.org USB database, the-sz.com VID registry, GitHub code search, and JacKeTUs/linux-steering-wheels — no match found for VID `0x1DD2` + PID `0xBEEF`.

**Remedy:** Acquire a USB device capture from real Leo Bodnar SLI-M hardware to determine the actual PID. Until then, `0xBEEF` is flagged as provisional in code and documentation.

**Merged to main:** PR #19 merge (d6fba74).

---

### F-037 · OpenFFBoard PID 0xFFB1 absent from all sources — likely doesn't exist (Low · Resolved)

**Encountered:** RC device verification audit (2025-06)

The OpenFFBoard alt PID `0xFFB1` is listed in the protocol crate but cannot be found in: pid.codes registry (only `0xFFB0` registered), OpenFFBoard firmware source (Ultrawipf/OpenFFBoard), JacKeTUs/linux-steering-wheels, Linux kernel hid-ids.h, or any USB capture database. It may have been speculatively added for a planned firmware variant that was never released.

**Remedy:** Review OpenFFBoard firmware release history and changelogs to determine if `0xFFB1` was ever shipped. If not, consider removing or marking as deprecated.

**Merged to main:** PR #19 merge (d6fba74).

---

### F-038 · Cube Controls PIDs 0x0C73–0x0C75 unverifiable — product pages return 404 (Medium · Resolved)

**Encountered:** RC device verification audit (2025-06)

Cube Controls GT Pro, Formula CSX-3, and F-CORE PIDs `0x0C73`–`0x0C75` cannot be verified. Product pages for several models return HTTP 404. No entries found in: JacKeTUs/linux-steering-wheels, devicehunt.com VID `0x0483` database, Linux kernel hid-ids.h, or GitHub code search for USB captures. These are button boxes (input-only, non-FFB), not wheelbases.

**Fix applied (partial):** Devices reclassified as input-only in code and docs. PIDs kept as provisional placeholders with doc comments.

**Remedy:** Acquire a USB device tree capture (`lsusb -v` or USBTreeView) from real Cube Controls hardware to confirm or correct VID/PIDs.

**Merged to main:** PR #19 merge (d6fba74).

---

### F-039 · VRS DirectForce Pro PID 0xA355 confirmed via linux-steering-wheels (Low · Resolved)

**Encountered:** RC device verification audit (2025-06)

VRS DirectForce Pro PID `0xA355` was previously listed as community-reported without a specific source. Confirmed via JacKeTUs/linux-steering-wheels database and cross-referenced with linux-hardware.org USB captures under VID `0x0483`.

**No code change needed** — PID was already correct. Status upgraded from community-reported to verified in documentation.

---

### F-040 · 100% telemetry adapter snapshot test coverage achieved (Medium · Resolved)

**Encountered:** RC test coverage audit (2025-06)

Prior to this sprint, many telemetry adapters lacked snapshot tests, meaning protocol regressions could slip through undetected. A systematic audit identified all untested adapters.

**Fix applied:** Snapshot tests added for all 56 telemetry adapters (100% coverage). Each test verifies that a representative packet produces the expected `TelemetryData` output, catching field mapping regressions and byte offset errors.

---

### F-041 · 126 additional unwrap/expect calls eliminated from 8 test files (Medium · Resolved)

**Encountered:** RC test quality audit (2025-06)

Per the project's testing rules (no `unwrap()`/`expect()` in tests), a sweep of test files found 126 remaining instances across 8 files. These could mask errors by panicking instead of producing clear test failure messages.

**Fix applied:** All 126 calls replaced with `Result`-returning test functions, explicit assertions, or `?` propagation. Zero `unwrap()`/`expect()` calls remain in test code.

---

### F-042 · Asetek Tony Kanaan torque corrected 18→27 Nm, added 8 proptest properties (Medium · Resolved)

**Encountered:** RC device verification audit (2025-06)

The Asetek Tony Kanaan Edition wheelbase was listed at 18 Nm torque, but the official Asetek spec sheet and JacKeTUs/universal-pidff both list 27 Nm. Additionally, the Asetek protocol crate lacked property-based tests for edge cases.

**Fix applied:** Tony Kanaan `max_torque_nm()` corrected from 18.0 to 27.0. Eight proptest property tests added covering torque scaling, command serialization, and round-trip invariants.

---

### F-051 · Leo Bodnar PID 0xBEEF is a placeholder, needs real USB PID (Low · Resolved)

**Encountered:** Wave 15 RC hardening (2025-06)

The SLI-M entry in `hid-leo-bodnar-protocol` used PID `0xBEEF`, which is a common development placeholder. No public USB database lists this PID for VID `0x1DD2`. The placeholder has been replaced with community-estimated PID `0x1301` (source: OpenFlight compat DB, sim racing community reports). The product name was corrected from "SLI-M" (non-existent) to "SLI-Pro" (actual Leo Bodnar product). PID still needs hardware capture to fully confirm. See also F-036.

**Remedy:** PID updated to `0x1301` (community estimate). Full confirmation still requires a real USB device capture from SLI-Pro hardware.

---

### F-052 · OpenFFBoard PID 0xFFB1 unverified (Low · Open)

**Encountered:** Wave 15 RC hardening (2025-06)

The OpenFFBoard alt PID `0xFFB1` is listed in the protocol crate but cannot be found in pid.codes (only `0xFFB0` registered), OpenFFBoard firmware source, or any USB capture database. It may have been speculatively added for a firmware variant that was never released. See also F-037.

**Remedy:** Review OpenFFBoard firmware release history. If `0xFFB1` was never shipped, remove or deprecate the entry.

---

### F-053 · macOS not in CI matrix (Medium · Open)

**Encountered:** Wave 15 RC hardening (2025-06)

The CI workflow matrix covers Linux and Windows but does not include macOS. macOS is a supported platform (macOS 10.15+) with platform-specific code paths (e.g., `thread_policy_set` for RT scheduling). Platform-specific compile errors and behavioral differences can go undetected until manual testing.

**Remedy:** Add a macOS runner (`macos-latest`) to the CI matrix for at least the build and test jobs. Consider using `macos-13` for x86_64 and `macos-14` for ARM64 coverage.

---

### F-054 · No MSRV check job in CI (Low · Open)

**Encountered:** Wave 15 RC hardening (2025-06)

There is no CI job that builds against the minimum supported Rust version (MSRV). The `rust-toolchain.toml` pins a specific toolchain, but there is no verification that the codebase compiles on older supported Rust versions. Accidental use of newer Rust features could break downstream users on older toolchains.

**Remedy:** Add a CI job that installs the MSRV toolchain (from `Cargo.toml` `rust-version` field or `rust-toolchain.toml`) and runs `cargo check --workspace`. Consider using `cargo-msrv` or a dedicated matrix entry.

---

### F-055 · 44 unwrap/expect remaining in test files (convention violation) (Medium · Open)

**Encountered:** Wave 15 RC hardening (2025-06)

Despite the F-041 cleanup (126 calls removed), 44 `unwrap()`/`expect()` calls remain across test files. Per project convention (no `unwrap()`/`expect()` in tests), these should be replaced with `Result`-returning test functions, explicit assertions, or `?` propagation.

**Remedy:** Sweep remaining test files and replace all `unwrap()`/`expect()` calls. Add a CI lint or clippy configuration to prevent new instances.

---

## Resolved (archive)

| ID | Title | Resolved In |
|----|-------|-------------|
| F-003 | Agent file-edit race during compilation | PR #19 merge (d6fba74) |
| F-004 | Windows linker PDB limit in integration tests | PR #19 merge (d6fba74) |
| F-006 | Snapshot tests silently encoding wrong values | PR #19 merge (d6fba74) |
| F-007 | Symbol renames cascade (sequence→frame_seq complete) | PR #19 merge (d6fba74) |
| F-008 | BeamNG gear overflow | PR #19 merge (d6fba74) |
| F-009 | static_mut_refs missing | PR #19 merge (d6fba74) |
| F-010 | Stale integration test name | PR #19 merge (d6fba74) |
| F-011 | Linux emit_rt_event borrow error | PR #19 merge (d6fba74) |
| F-013 | No developer sync tool for game support matrix | PR #19 merge (d6fba74) |
| F-014 | Agent race conditions on shared branch | PR #19 merge (d6fba74) |
| F-015 | Workspace-hack requires manual regeneration | PR #19 merge (d6fba74) |
| F-016 | bench_results.json generation undocumented | PR #19 merge (d6fba74) |
| F-017 | `cargo tree --duplicates` CI check too strict | PR #19 merge (d6fba74) |
| F-018 | `fuzz_simplemotion` missing dep in fuzz/Cargo.toml | PR #19 merge (d6fba74) |
| F-019 | 6 SimHub adapters returned empty stub telemetry | PR #19 merge (d6fba74) |
| F-026 | Codemasters Mode 1 UDP adapters wrong byte offsets | PR #19 merge (d6fba74) |
| F-027 | Forza tire temp assumed Kelvin, actually Fahrenheit | PR #19 merge (d6fba74) |
| F-028 | fuel_percent × 100 bug in LFS, AMS1, RaceRoom f64 | PR #19 merge (d6fba74) |
| F-030 | Assetto Corsa adapter used OutGauge instead of Remote Telemetry | PR #19 merge (d6fba74) |
| F-031 | Simagic M10/Simucube 1 PID collision at 0x0D5A | PR #19 merge (d6fba74) |
| F-032 | Estimated PIDs for unreleased Simagic devices | PR #19 merge (d6fba74) |
| F-033 | Simucube Wireless Wheel PID unconfirmed | PR #19 merge (d6fba74) |
| F-034 | Shared USB VIDs require PID-based runtime disambiguation | PR #19 merge (d6fba74) |
| F-035 | PCars2 adapter rewritten to correct SMS UDP v2 format | PR #19 merge (d6fba74) |
| F-036 | Leo Bodnar PID 0xBEEF confirmed as placeholder | PR #19 merge (d6fba74) |
| F-037 | OpenFFBoard PID 0xFFB1 absent from all sources | PR #19 merge (d6fba74) |
| F-038 | Cube Controls PIDs 0x0C73–0x0C75 unverifiable | PR #19 merge (d6fba74) |
| F-039 | VRS DirectForce Pro PID 0xA355 confirmed via linux-steering-wheels | PR #19 merge (d6fba74) |
| F-040 | 100% telemetry adapter snapshot test coverage (56/56 adapters) | PR #19 merge (d6fba74) |
| F-041 | 126 unwrap/expect calls eliminated from 8 test files | PR #19 merge (d6fba74) |
| F-042 | Asetek Tony Kanaan torque corrected 18→27 Nm + 8 proptest properties | PR #19 merge (d6fba74) |

---

## Recent Progress

### Protocol Verification Wave (Web-Verified)
- **Moza Racing**: All 11 wheelbase PIDs verified against JacKeTUs/universal-pidff (Linux kernel 6.15). All torque specs confirmed from mozaracing.com. FFB quirks correct. No changes needed.
- **Simucube**: SC2 Sport torque corrected 15→17 Nm, SC2 Ultimate 35→32 Nm (from official docs). Added Simucube 1 PID 0x0D5A. SC-Link Hub PID corrected 0x0D62→0x0D66.
- **Simagic**: EVO Sport 15→9 Nm, EVO 20→12 Nm, EVO Pro 30→18 Nm (from simagic.com). Removed ghost M10/FX entries.
- **Assetto Corsa**: Complete rewrite from OutGauge (76 bytes) to Remote Telemetry UDP (328 bytes) with 3-step handshake. All field offsets corrected.
- **ACC**: Fixed isReadonly field inversion (byte==0 means readonly in Kunos SDK).
- **BeamNG**: Verified correct (OutGauge protocol matches InSim.txt).

### Protocol Verification Wave 2 — Cammus / FFBeast / PXN (Web-Verified)
- **Cammus**: VID `0x3416`, C5 PID `0x0301`, C12 PID `0x0302` — all confirmed against Linux kernel `hid-ids.h` (`USB_VENDOR_ID_CAMMUS`), `hid-universal-pidff.c`, and JacKeTUs/linux-steering-wheels (Platinum support). Torque values C5=5 Nm, C12=12 Nm unchanged. No code changes needed.
- **FFBeast**: VID `0x045B`, Joystick PID `0x58F9`, Rudder PID `0x5968`, Wheel PID `0x59D7` — all confirmed against Linux kernel `hid-ids.h` (`USB_VENDOR_ID_FFBEAST`), `hid-universal-pidff.c`, FFBeast C/C++ API reference (`USB_VID=1115`, `WHEEL_PID_FS=22999`), and JacKeTUs/linux-steering-wheels. Protocol uses ±10000 signed 16-bit torque scale. Dead links fixed: `HF-Robotics/FFBeast` repo (404) and `ffbeast.com` (domain for sale) replaced with `ffbeast.github.io`.
- **PXN**: No `hid-pxn-protocol` crate exists in this branch (was on `feat/r6-pxn-v2`). Linux kernel confirms VID `0x11FF` (`USB_VENDOR_ID_LITE_STAR`), PIDs: V10=`0x3245`, V12=`0x1212`, V12 Lite=`0x1112`/`0x1211`. No V9 PID found in kernel or community sources. PXN uses `HID_PIDFF_QUIRK_PERIODIC_SINE_ONLY` quirk in `hid-universal-pidff`. Torque specs not verified — PXN official site does not publish peak Nm values.

### Protocol Verification Wave 3 — Full Vendor Sweep (Web-Verified)
- **Asetek**: Invicta torque corrected 18→12 Nm, Forte corrected 25→18 Nm, Tony Kanaan corrected 25→27 Nm (from asetek.com spec sheets and JacKeTUs/universal-pidff).
- **rFactor 2**: Adapter completely rewritten from `rF2State.h` (rF2SharedMemoryMap SDK). All shared memory struct offsets verified against the authoritative header. Field mapping corrected for vehicle telemetry, scoring, and extended data.
- **Simucube**: SC2 Sport torque corrected 15→17 Nm, SC2 Ultimate torque corrected 35→32 Nm (Granite Devices official specs). Simucube 1 PID `0x0D5A` added and verified.
- **Simagic**: EVO Sport 15→9 Nm, EVO 20→12 Nm, EVO Pro 30→18 Nm (simagic.com). PID collision with Simucube at `0x0483:0x0522` resolved via `iProduct` string disambiguation.
- **Thrustmaster**: T500 RS PID `0xB677` corrected — was mislabeled as T150 Pro per linux-hardware.org and devicehunt.com. T-GT and T-GT II PIDs confirmed unknown (T-GT II reuses T300 PIDs per hid-tmff2 README).
- **Moza Racing**: All 11 wheelbase PIDs re-confirmed correct against JacKeTUs/universal-pidff and mozaracing.com. No changes needed.
- **Cammus**: VID `0x3416`, C5 `0x0301`, C12 `0x0302` — all confirmed correct against Linux kernel `hid-ids.h`. No changes needed.
- **FFBeast**: Dead links (`HF-Robotics/FFBeast` repo 404, `ffbeast.com` domain for sale) replaced with `ffbeast.github.io`. PIDs confirmed against Linux kernel `hid-ids.h`.
- **Cube Controls**: Reclassified as button boxes (input-only, non-FFB). PIDs remain provisional/unconfirmed pending hardware capture.
- **Leo Bodnar**: VID `0x1DD2` confirmed via USB VID registry (the-sz.com). SLI-M PID `0xBEEF` flagged as placeholder — not found in any public USB database.
- **AccuForce**: PID `0x804C` confirmed (NXP VID `0x1FC9`). V1 vs V2 torque differences documented (V1=7 Nm, V2=12 Nm).
- **OpenFFBoard**: Main PID `0xFFB0` confirmed via pid.codes registry. Alt PID `0xFFB1` remains unverified (no independent source).
- **Heusinkveld**: VID `0x16D0` confirmed (shared with Simucube — disambiguated by PID range `0x115x`).
- **VRS DirectForce**: VID `0x0483` confirmed (STMicroelectronics generic). VID collision with Simagic legacy documented and resolved via `iProduct` string.
- **Assetto Corsa**: Complete rewrite from OutGauge (76 bytes) to Remote Telemetry UDP (328 bytes) with 3-step handshake.
- **ACC**: Fixed `isReadonly` field inversion (byte==0 means readonly in Kunos SDK).

### Engine Device Table Sync
- 50+ missing devices added to linux.rs (VRS, Heusinkveld, Cammus, OpenFFBoard, FFBeast, etc.)
- AccuForce Pro capabilities corrected (12 Nm, PID support, 1 kHz)
- Cube Controls capabilities corrected (input-only devices, torque set to 0 Nm; PIDs still unconfirmed)
- Asetek Tony Kanaan torque corrected (25→20 Nm)

### RC Cleanup Sprint (2025-06)
- **PCars2**: Adapter completely rewritten from fabricated offsets to correct SMS UDP v2 format (F-035)
- **Telemetry snapshot tests**: 100% coverage achieved — 56/56 adapters have snapshot tests (F-040)
- **Test quality**: 126 `unwrap()`/`expect()` calls eliminated from 8 test files (F-041)
- **Asetek Tony Kanaan**: Torque corrected 18→27 Nm; 8 proptest property tests added (F-042)
- **VRS DirectForce Pro**: PID `0xA355` independently confirmed via linux-steering-wheels (F-039)
- **Device PID audit**: Leo Bodnar `0xBEEF` (F-036), OpenFFBoard `0xFFB1` (F-037), Cube Controls `0x0C73`–`0x0C75` (F-038) flagged as unverifiable — all need hardware captures

### Earlier Progress
- Project CARS 3 adapter added
- Codemasters shared parsing extracted into `codemasters_shared.rs` (~890 lines of duplicated offset logic removed)
- Forza Horizon 4 324-byte packet support added
- Cube Controls protocol tests added (46 tests)
- TODO comments cleaned up across engine, CLI, and diagnostics crates
- Portable shebangs (`#!/usr/bin/env bash`) applied to shell scripts

---

## Process notes

- **Review cadence:** Check open items at the start of each sprint / major feature push.
- **Adding entries:** When you hit a friction point, add it here before moving on. Don't wait until retrospective.
- **Closing entries:** Mark **Resolved** once the fix lands in `main`; move to the archive table.
- **Escalation:** High-severity open items that block RC should be added to `ROADMAP.md` as concrete work items.
