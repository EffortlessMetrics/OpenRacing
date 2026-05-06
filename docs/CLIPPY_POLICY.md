# Clippy and Policy Gates

OpenRacing treats Clippy as a governed engineering surface. The workspace lint
configuration is not a local taste file: it is the active implementation of the
Effortless Metrics Rust platform policy for panic-free, suppression-governed,
real-time-safe Rust.

## Policy goals

The policy has four goals:

1. keep production and test code panic-free by default;
2. prevent silent failure patterns such as discarded futures, ignored results,
   and hidden I/O errors;
3. make suppression explicit with narrow `#[expect(..., reason = "...")]`
   receipts instead of broad `#[allow]` carveouts; and
4. track future Rust and Clippy ratchets before the MSRV bump lands.

OpenRacing is a high-churn numeric and real-time workspace, so numeric lints that
can create broad churn start at `warn` where the common platform policy calls for
staging. The policy still records them centrally so warning debt can be counted
and promoted deliberately.

## Source of truth

The root `Cargo.toml` contains the active workspace lint levels. The
machine-readable ledger in `policy/clippy-lints.toml` mirrors those active levels
and records planned Rust 1.94 and 1.95 flips. The ledger exists so review tooling
can answer whether the manifest, policy docs, and CI checks agree.

The companion files are:

- `policy/clippy-debt.toml` for temporary lint exceptions with owner, reason,
  path, lint, and expiry;
- `policy/no-panic-allowlist.toml` for semantic panic-family exceptions using
  `path + family + selector` identity and advisory `last_seen` locations;
- `policy/non-rust-allowlist.toml` for non-Rust programming/config surfaces that
  need explicit owner, reason, classification, surface, and CI coverage; and
- `clippy.toml` for Clippy configuration and repo-specific disallowed
  methods/types/macros only, not test carveouts.

## Suppression style

Use `#[expect(..., reason = "...")]` for narrow, local suppressions. Do not add a
workspace-level allow, package-level allow, or Clippy test carveout to make a lint
quiet.

Preferred pattern:

```rust
#[expect(
    clippy::arithmetic_side_effects,
    reason = "fixed-point hardware scaling is range-checked by DeviceScale before this operation"
)]
fn scale_force(value: i32, gain: i32) -> i32 {
    value * gain
}
```

Avoid:

```rust
#[allow(clippy::arithmetic_side_effects)]
fn scale_force(value: i32, gain: i32) -> i32 {
    value * gain
}
```

If a suppression cannot be removed promptly, add an expiring entry to
`policy/clippy-debt.toml` so the exception is counted and reviewed.

## Test posture

There are no test carveouts. Tests should return `Result` where fallible setup is
needed and should use explicit assertions or helper macros instead of `unwrap`,
`expect`, `panic!`, `todo!`, `unimplemented!`, or unchecked indexing.

```rust
#[test]
fn parses_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = std::fs::read_to_string("tests/fixtures/input.rs")?;
    let parsed = parse(&fixture)?;

    ensure_eq(parsed.items.len(), 3, "fixture should expose three items")?;

    Ok(())
}
```

## OpenRacing overlays

OpenRacing inherits the common strict block and adds real-time/numeric operating
rules:

- `unsafe_code` is forbidden at the workspace level;
- unsafe operations in unsafe functions are denied to force local unsafe blocks;
- arithmetic side effects and lossy numeric casts are staged for review because
  force-feedback, HID, telemetry, and hardware-protocol code have intentional
  fixed-width arithmetic;
- async/lock lints are active because blocking and lock lifetime mistakes can
  violate real-time guarantees; and
- file/process/path footgun lints are active because service and CLI boundaries
  need explicit I/O behavior.

## Policy commands

Run the policy checks with:

```console
cargo xtask check-lint-policy
cargo xtask check-no-panic-family
cargo xtask check-file-policy
cargo xtask policy-report
```

The lint policy gate verifies manifest inheritance, active/planned lint
consistency, MSRV alignment, the absence of Clippy test carveouts, and debt-ledger
schema hygiene. The no-panic and file-policy checks provide the structured
allowlist model used by follow-up cleanup PRs.
