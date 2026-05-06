# Clippy policy

OpenRacing treats linting as an engineering policy surface, not a local taste file.
The workspace uses one strict baseline for panic-free Rust, silent-failure
prevention, suppression governance, and reviewability. Repo-specific exceptions
must be explicit policy data or narrow source-level expectations with a reason.

## Active baseline

The active lint levels live in the root `Cargo.toml` under
`[workspace.lints.rust]` and `[workspace.lints.clippy]`. Every workspace crate
inherits those lints with:

```toml
[lints]
workspace = true
```

The machine-readable ledger is `policy/clippy-lints.toml`. It records the same
active lints, the MSRV (`1.93`), and planned Rust 1.94/1.95 flips before they are
activated in `Cargo.toml`.

## Panic-free workspace

The standard is workspace panic-free, including tests. Do not add Clippy test
carveouts such as `allow-unwrap-in-tests`, `allow-expect-in-tests`,
`allow-panic-in-tests`, `allow-indexing-slicing-in-tests`, or
`allow-dbg-in-tests` to `clippy.toml`.

Prefer fallible tests that return `Result` and use explicit assertions or test
helpers instead of `unwrap`, `expect`, or panic-driven setup:

```rust
#[test]
fn parses_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = std::fs::read_to_string("tests/fixtures/input.rs")?;
    let parsed = parse(&fixture)?;

    ensure_eq(parsed.items.len(), 3, "fixture should expose three items")?;

    Ok(())
}
```

## Suppressions

Use `#[expect(..., reason = "...")]` for narrow, reviewed suppressions. Do not
use silent `#[allow(...)]` suppressions unless a future policy file explicitly
permits that exact exception.

Temporary lint debt belongs in `policy/clippy-debt.toml` with `lint`, `path`,
`owner`, `reason`, and `expires`. Expired debt fails the policy gate.

## OpenRacing overlays

OpenRacing is a real-time, numeric, hardware-facing workspace. Numeric and async
lints that can create broad churn are initially warning-level where appropriate,
while panic-family, unsafe/memory, path/process, and suppression-governance lints
are denied. RT code must still obey the project rules: no allocation after
initialization, no blocking in hot paths, bounded execution, and explicit safety
interlocks.

## Policy checks

Run the policy gate with:

```sh
cargo xtask check-lint-policy
```

The gate verifies:

1. `workspace.package.rust-version` matches `policy/clippy-lints.toml`.
2. Workspace members inherit workspace lints.
3. Active lints in `policy/clippy-lints.toml` match the root `Cargo.toml` lint block.
4. Planned Rust 1.94/1.95 lints stay planned until the MSRV bump.
5. `clippy.toml` does not contain panic/test carveouts.
6. `policy/clippy-debt.toml` entries have required fields and are not expired.

The same xtask also provides a compact summary:

```sh
cargo xtask policy-report
```
