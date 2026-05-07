# Clippy / lint policy

OpenRacing uses a layered lint policy. The intent is **strictness without
brittleness**: deny by default, allow by receipt, expire exceptions, measure
drift.

## Where the policy lives

| Concern | File |
|---|---|
| Declarative policy + planned MSRV flips | `policy/clippy-lints.toml` |
| Burndown ledger (warn → deny) | `policy/clippy-debt.toml` |
| Authoritative panic-family exceptions | `policy/no-panic-allowlist.toml` |
| Authoritative non-Rust file surface | `policy/non-rust-allowlist.toml` |
| Active lint levels (single source of truth for the compiler) | `[workspace.lints]` in root `Cargo.toml` |
| Clippy thresholds (complexity, name length, etc.) | `clippy.toml` |

`policy/clippy-lints.toml` is **what should be true**. The
`[workspace.lints]` block in `Cargo.toml` is **what is true today**.
`scripts/check_lint_policy.py` enforces that the second matches the first
modulo the staged debt entries in `policy/clippy-debt.toml`.

## Two-rail design

Clippy lints catch local bad shapes near the code, in the editor, and in
CI. They cannot, however, carry full exception metadata: who owns the
exception, why it exists, when it expires, what selector identifies it.

The **semantic no-panic checker** (`scripts/check_no_panic_family.py`)
owns that ledger, keyed by `path + family + selector`, with `last_seen`
line/column hints used only for human review (never for matching).

This dual-rail design means:

1. Strict Clippy posture stays sharp in the editor and CI.
2. Intentional exceptions are explicit, owned, and time-boxed.
3. We can promote individual lints from `warn` to `deny` only when the
   semantic checker reports zero unreceipted findings.

## Stages

OpenRacing uses a four-stage rollout:

1. **Stage A — current.** Hard-deny lints with verified zero debt
   land in `[workspace.lints]` immediately. Panic-family and other
   noisy lints with existing debt are **`allow`** rather than `warn`,
   because existing CI runs `cargo clippy ... -- -D warnings` for
   non-test crates; promoting them to `warn` here would convert them
   to errors and break unrelated jobs (smoke / acceptance / soak / …).
   The semantic checker (`scripts/check_no_panic_family.py`) enumerates
   findings without blocking and remains the authoritative drift
   reporter for panic-family lints during burndown.
2. **Stage B — burndown.** Each entry in `policy/clippy-debt.toml` is
   addressed by its named `target_pr`. Findings are either fixed or
   receipted into `policy/no-panic-allowlist.toml`. As each lint
   reaches zero unreceipted findings on default-members, it is
   promoted from `allow` → `warn` (its own dedicated PR).
3. **Stage C — strict flip.** When all debt entries reach `warn`
   level cleanly, panic-family lints are promoted to `deny`.
   Exceptions require both a TOML entry and an
   `#[expect(..., reason = "policy:no-panic:<id>")]` attribute.
4. **Stage D — MSRV flips.** When MSRV bumps, the lints listed under
   `[[planned]]` in `policy/clippy-lints.toml` activate.

The repo currently sits at Stage A.

## Forbidden patterns

These are rejected by `scripts/check_lint_policy.py`:

* Test carveouts in `clippy.toml`
  (`allow-unwrap-in-tests`, `allow-expect-in-tests`,
  `allow-panic-in-tests`, `allow-indexing-slicing-in-tests`,
  `allow-dbg-in-tests`).
* Bare `#[allow(...)]` attributes inside crate sources.
  Use `#[expect(..., reason = "...")]` instead.
* Lint exceptions without an owner, reason, or expiry.
* Crate `Cargo.toml` files that do not inherit
  `[lints] workspace = true`.
* Global `-D warnings` while `policy/clippy-debt.toml` still has
  warn-stage entries.

## Adding a new exception

1. Triage the finding: if it is fixable, fix it.
2. If it must remain, identify its **family** (`unwrap`, `expect`,
   `panic_macro`, `indexing`, …) and its **selector**
   (kind, container, callee, receiver fingerprint).
3. Add an entry to `policy/no-panic-allowlist.toml` with `owner`,
   `reason`, `classification`, and `expires`.
4. In source, attach
   `#[expect(clippy::<lint>, reason = "policy:no-panic:<id>")]`
   to the smallest enclosing item.
5. Run `python scripts/check_no_panic_family.py` to verify no drift.

## Promoting a lint

To promote a lint from `warn` to `deny`:

1. Run `python scripts/check_no_panic_family.py` and confirm zero
   unreceipted findings for that lint family.
2. Remove the matching entry from `policy/clippy-debt.toml`.
3. Update the level in `[workspace.lints]` in root `Cargo.toml` and in
   `policy/clippy-lints.toml`.
4. Land the change in a dedicated PR (e.g.
   `policy/promote-<lint>-to-deny`).

## Repo class

OpenRacing is the `numeric_ffi_rt` repo class:

* `unsafe_code` is `deny`, **not** `forbid`. We have legitimate RT, HID,
  and FFI islands; each one is receipted with a `// SAFETY:` comment.
* `arithmetic_side_effects` starts at `allow` because RT/numeric code
  uses overflow-checked arithmetic via dedicated helpers and the lint is
  too noisy across the curve/filter pipeline to be useful in Stage A.
* Numeric correctness lints (`float_cmp`, `cast_possible_*`, etc.) are
  `warn` and burn down via the calibration / FFB cleanup PR.
