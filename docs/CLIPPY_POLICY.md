# Clippy policy

OpenRacing treats Clippy as a governed engineering surface for real-time, safety-critical Rust. The workspace policy is intentionally stricter than a local style file: it is a shared baseline, a machine-readable ledger, and a set of `xtask` checks that make exceptions explicit.

## Baseline

The root `Cargo.toml` owns the active workspace lint block. Every workspace crate inherits it with:

```toml
[lints]
workspace = true
```

The baseline covers:

- panic-free production and test code (`panic`, `unwrap_used`, `expect_used`, `todo`, `unimplemented`, `unreachable`);
- AST, UTF-8, indexing, and slicing safety;
- silent-failure prevention for ignored futures, locks, `Result`s, and line iterators;
- async/concurrency footguns;
- unsafe and memory reviewability;
- staged numeric correctness for real-time and telemetry paths;
- filesystem, process, path, API, trait, and reviewability lints;
- suppression governance.

## No test carveouts

The policy is workspace panic-free, not merely production panic-free. Do not add these Clippy configuration flags:

```toml
allow-unwrap-in-tests = true
allow-expect-in-tests = true
allow-panic-in-tests = true
allow-indexing-slicing-in-tests = true
allow-dbg-in-tests = true
```

Prefer `Result`-returning tests and explicit assertion helpers over `unwrap`, `expect`, or panic-driven setup.

## Suppression style

Use narrow `#[expect(..., reason = "...")]` suppressions when a local exception is reviewed. Do not use broad `#[allow]` attributes to silence policy without a reason.

A suppression should explain why the exception is safe, why the lint cannot be fixed now, and when the exception should be revisited.

## Policy ledgers

The policy directory is the source of truth for reviewed exceptions and future ratchets:

- `policy/clippy-lints.toml` tracks active lints and planned Rust 1.94/1.95 flips.
- `policy/clippy-debt.toml` tracks temporary rollout debt with owner, reason, path, lint, and expiry.
- `policy/no-panic-allowlist.toml` reserves the semantic allowlist shape for narrow panic-family exceptions.
- `policy/non-rust-allowlist.toml` tracks non-Rust programming files with owner, reason, surface, classification, and coverage.

Debt is allowed during staged rollout. Silent debt is not.

## Planned Rust upgrades

OpenRacing is ratcheted to MSRV 1.93. The policy ledger tracks lints planned for Rust 1.94 and 1.95 before the workspace flips to those compilers. Planned lints must stay out of the active `Cargo.toml` block until the MSRV bump lands.

## Checks

Use these commands when changing lint policy or policy ledgers:

```sh
cargo xtask check-lint-policy
cargo xtask check-file-policy
cargo xtask check-no-panic-family
cargo xtask policy-report
```

`check-lint-policy` verifies MSRV alignment, workspace lint inheritance, the active lint block, lack of test carveouts, planned lint tracking, and debt entry shape.

`check-file-policy` verifies non-Rust programming files are covered by structured TOML policy entries.

`check-no-panic-family` validates the semantic no-panic allowlist schema. Follow-up PRs should migrate remaining panic-family occurrences into precise semantic entries or remove them.
