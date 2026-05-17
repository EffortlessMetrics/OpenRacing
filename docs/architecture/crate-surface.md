# OpenRacing Crate Surface

## Doctrine

OpenRacing uses one rule for crate-surface decisions:

```text
Cargo package boundary = public support promise
Rust module boundary   = SRP / ownership / agent-context boundary
```

A Cargo package is not just a folder. It creates a semver surface, a docs.rs page, package metadata obligations, a feature matrix, a release-order node, and a support promise. The intended durable states are therefore:

- a real public package listed in `policy/crate-boundaries.toml` and `[workspace.metadata.publish].allow`;
- an internal package with `publish = false` for tools, tests, examples, or dev-only infrastructure; or
- a module family under an owner crate.

Production-path implementation seams should not remain as long-lived `publish = false` microcrates. Design seams like microcrates, implement most seams as module families, and publish only durable public contracts.

## Target Public Packages

The target public surface is eighteen packages:

| Package | Role |
| --- | --- |
| `openracing` | Facade SDK and stable start-here crate. |
| `openracing-engine` | Runtime engine, RT loop, device orchestration, and safety execution. |
| `openracing-ffb` | FFB effects, force models, filters, and compiled output plans. |
| `openracing-calibration` | Reusable axis, deadzone, and normalization kernel. |
| `openracing-curves` | Reusable LUT, Bezier, and remap math. |
| `openracing-profile` | Profile, tuning, input-map, preset, and serialization contract. |
| `openracing-hid` | Generic HID transport, descriptor/capture/replay, and vendor protocol families. |
| `openracing-pidff` | Cross-vendor HID PIDFF report and safety layer. |
| `openracing-moza` | Validated Moza family, including R5, KS, ES, SR-P, HBP, and PIDFF receipts. |
| `openracing-firmware-update` | Firmware/update safety boundary. |
| `openracing-plugin-abi` | Hard plugin ABI contract. |
| `openracing-plugin-sdk` | Plugin-author SDK without host/runtime internals. |
| `openracing-telemetry` | Normalized telemetry model, traits, streams, and orchestration. |
| `openracing-telemetry-adapters` | Game adapter family with separate cadence. |
| `openracing-telemetry-config` | Support matrix and config writers. |
| `openracing-telemetry-recorder` | Recording, playback, fixtures, and replay. |
| `openracing-service` | `wheeld` product package. |
| `wheelctl` | Operator CLI product package. |

`openracing-curves` remains public because it is reusable math. `openracing-moza` remains public because the Moza family is receipt-backed and validated rather than a generic HID leaf.

## Internal Packages

Internal packages are repository tooling, tests, examples, compatibility shims, UI applications, or workspace mechanics. They must be listed under `[internal].packages` in `policy/crate-boundaries.toml` and should use `publish = false` when safe.

Current internal package classes include:

- repository tools (`openracing-tools`);
- workspace mechanics (`workspace-hack`);
- apps and UI not part of the public library contract (`racing-wheel-ui`);
- integration/test support (`racing-wheel-integration-tests`, `openracing-test-helpers`, `compat`);
- examples (`openracing-plugin-examples` / `plugin-examples`); and
- release or repository maintenance helpers such as changelog tooling.

## Collapse Map

`policy/crate-boundaries.toml` is the machine-readable source of truth for the collapse map. Each `[[collapse]]` entry declares:

- the current workspace package name;
- the destination module family path;
- the owner public package; and
- the reason the seam is not a durable public support promise.

The first policy PR does not move code. Collapse entries are rails for later PRs and include transitional notes while current packages still exist.

## Module-Family Standard

Every collapsed seam should keep a crate-grade folder boundary:

```text
src/<family>/
  mod.rs
  error.rs
  types.rs
  state.rs
  validate.rs
  encode.rs
  decode.rs
  tests.rs
  fixtures/
  BOUNDARY.md
```

`BOUNDARY.md` should use this template:

```text
# Boundary: <family>

Owner:
Purpose:
Public façade:
Internal modules:
Allowed dependencies:
Forbidden dependencies:
Invariants:
Tests:
Non-goals:
Migration source:
```

Rules:

- `mod.rs` is the façade.
- Siblings import through the façade.
- Internals are private or `pub(crate)`.
- Do not use `pub use *` from implementation modules.
- Do not perform cross-family deep imports.
- Do not add a new `Cargo.toml` for a family unless policy marks it public.

## Dependency Layering

Layering starts as documentation and is enforced incrementally by `openracing-tools package-surface`:

- `openracing` may depend on public library crates only.
- `openracing-service` and `wheelctl` may depend on public libraries and platform/system crates, but public libraries may not depend on them.
- `openracing-engine` may depend on `openracing-ffb`, `openracing-hid`, `openracing-pidff`, `openracing-moza`, `openracing-profile`, telemetry, and `openracing-plugin-abi`; it may not depend on `wheelctl` or `openracing-service`.
- `openracing-ffb` may depend on calibration, curves, and profile; it may not depend on engine, service, wheelctl, or HID transport.
- `openracing-hid` may depend on PIDFF only for generic PIDFF helpers; it may not depend on engine, service, wheelctl, or telemetry.
- `openracing-moza` may depend on HID, PIDFF, curves, and calibration as needed; it may not depend on engine, service, or wheelctl.
- `openracing-pidff` may not depend on Moza, engine, service, or wheelctl.
- `openracing-plugin-abi` stays low-level and host-independent.
- `openracing-plugin-sdk` may depend on plugin ABI and public model crates, but not native/WASM host runtime internals.

## Feature Policy

Features are public API. They should be additive, product-oriented, and stable. Default features are sticky because removing one can be semver-incompatible.

Prefer product features:

```toml
[features]
default = ["std"]
std = []
serde = ["dep:serde"]
moza = ["dep:openracing-moza"]
telemetry = ["dep:openracing-telemetry"]
plugins = ["dep:openracing-plugin-sdk"]
```

Avoid extraction or former-microcrate features:

```toml
openracing-scheduler = []
openracing-pipeline = []
telemetry-lfs-crate = []
hid-moza-protocol = []
```

## Migration Sequence

1. `policy-crate-surface`: add this doctrine, policy, allowlist, default members, and checker. No code moves.
2. `facade-naming-spine`: introduce the public naming spine and facade crate.
3. `telemetry-finish`: finish telemetry consolidation into core, adapters, config, and recorder.
4. `pidff-promote`: rename and stabilize PIDFF.
5. `hid-core-collapse`: create the public HID family and move generic HID support.
6. `moza-family-collapse`: create the public Moza family.
7. `hid-vendor-collapse`: move remaining vendor protocol leaves under HID.
8. `ffb-collapse`: collapse FFB filters and pipeline under `openracing-ffb`.
9. `engine-helper-collapse`: collapse RT, safety, hardware evidence, diagnostic, and error helpers.
10. `profile-collapse`: move input maps and profile repository under `openracing-profile`.
11. `plugin-sdk-split`: keep plugin ABI/SDK public and move host runtimes to service.
12. `app-boundary`: classify or move apps and internal tools physically.
13. `delete-old-packages`: remove old package directories after imports are clean.
14. `package-proof`: run package proof for every final public package.

## Packaging Proof

For each final public package, the package-proof PR must run:

```text
cargo test -p <crate> --all-features --locked
cargo clippy -p <crate> --all-targets --all-features --locked -- -D warnings
cargo doc -p <crate> --all-features --no-deps --locked
cargo package -p <crate> --list
cargo publish -p <crate> --dry-run --locked
```

Final public packages should have workspace version, edition, rust-version, license, authors, repository, homepage, documentation, readme, description, keywords, categories, and `publish = true`. Internal packages should have `publish = false` and an internal-purpose description.

## Non-Goals

This policy PR deliberately does not:

- move code;
- rename packages;
- alter Moza hardware artifacts;
- run hardware-output commands;
- collapse telemetry, HID, FFB, profile, plugin, or engine helper crates;
- combine the rail with CI optimization, Clippy policy, or hardware-lane work;
- switch the workspace resolver; or
- use `publish = false` as a substitute for collapsing production-path crates.
