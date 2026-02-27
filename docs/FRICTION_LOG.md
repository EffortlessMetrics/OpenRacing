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

---

### F-002 · Duplicate config writer registration (High · Open)

**Encountered:** RC sprint

Every game's config writer must be registered in **two** separate files:
- `crates/telemetry-config/src/writers.rs` (used by `GameService` at runtime)
- `crates/telemetry-config-writers/src/lib.rs` (parallel crate)

Missing one silently causes tests to pass while runtime silently skips the writer. Caused several hard-to-debug failures.

**Remedy:** Unify into a single registry; the parallel crate should re-export from `telemetry-config` rather than duplicate logic. Track in a future cleanup issue.

---

### F-003 · Race condition: agents editing files during compilation (High · Open)

**Encountered:** RC sprint — agent-26 modifying `windows.rs` while `cargo check` was running

Concurrent file edits during active builds cause cascading compilation errors (references to constants that briefly exist/don't exist). This is especially bad for agents that progressively refine a large file.

**Remedy:** Agent orchestration should acquire a build-lock or use a worktree-per-agent pattern so edits are isolated until the agent commits. Document the worktree-per-agent rule in `AGENTS.md`.

---

### F-004 · Windows linker PDB limit in integration tests (Medium · Open)

**Encountered:** RC sprint — `racing-wheel-integration-tests` fails with LNK1318 / LNK1180 on Windows

The Windows linker hits its PDB symbol table size limit when the integration test crate is built in debug mode with all features, because it transitively pulls in every crate in the workspace.

**Remedy:** Options:
1. Build integration tests in release mode for CI on Windows.
2. Split `racing-wheel-integration-tests` into smaller crates.
3. Add `.cargo/config.toml` override to use `split-debuginfo = "packed"` on Windows.

---

### F-005 · Wrong protocol values in initial implementations (Medium · Open)

**Encountered:** RC sprint — multiple vendor protocol crates had wrong VIDs/PIDs on creation, requiring a full web-verification pass (agent-26) to fix:
- Cammus VID `0x3285 → 0x3416`
- Simucube VID `0x2D6A → 0x16D0`
- Asetek VID `0x2E5A → 0x2433`
- Simagic VID `0x2D5C → 0x3670`
- Logitech G923 PS PID `0xC266 → 0xC267`
- Thrustmaster TMX PID `0xB66D → 0xB67F`

Protocol values sourced from memory/guesses rather than verified sources.

**Remedy:** Add a `docs/protocols/SOURCES.md` that records the authoritative source (USB descriptor dump, community wiki URL, official SDK) for every VID/PID. Require a source citation when adding a new device. Add a unit test that cross-references the IDs against a checked-in golden file so a stale value causes a test failure.

---

### F-006 · Snapshot tests silently encoding wrong values (Medium · Open)

**Encountered:** RC sprint — Simagic snapshot tests were accepted with wrong legacy PIDs (`0x0101–0x0301`) and had to be regenerated after web-verification corrected the PIDs to `0x0500–0x0502`.

Snapshot tests provide no protection against "wrong but consistent" values: the first `--force-update-snapshots` run permanently bakes in whatever the code produces, even if the code is wrong.

**Remedy:** For device ID snapshots, cross-validate against the golden-source file from F-005. Add `// source: <URL>` annotations inside snapshot files for reviewers to verify. Consider a separate "verified constants" test that asserts specific known-good numeric values independently of snapshot infrastructure.

---

### F-007 · Symbol renames cascade across many test files (Medium · Open)

**Encountered:** RC sprint — `ProRacing → GPro`, `PRO_RACING → G_PRO` renames required manual fixes across:
- `logitech_e2e.rs`, `logitech_tests.rs`, `windows_property_tests.rs`, `windows.rs` (tests), etc.

No compile-time help distinguishes "this is a renamed constant" from "this constant was removed."

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
| F-008 | BeamNG gear overflow | commit cdd69f0 |
| F-009 | static_mut_refs missing | commit cdd69f0 |
| F-010 | Stale integration test name | agent-30 |
| F-011 | Linux emit_rt_event borrow error | commit 1c3fea5 |

---

## Process notes

- **Review cadence:** Check open items at the start of each sprint / major feature push.
- **Adding entries:** When you hit a friction point, add it here before moving on. Don't wait until retrospective.
- **Closing entries:** Mark **Resolved** once the fix lands in `main`; move to the archive table.
- **Escalation:** High-severity open items that block RC should be added to `ROADMAP.md` as concrete work items.
