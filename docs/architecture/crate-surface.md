# OpenRacing Crate Surface

## Doctrine

OpenRacing keeps strong SRP boundaries without turning every seam into a public
crates.io support promise.

```text
Cargo package boundary = public support promise
Rust module boundary   = SRP / ownership / agent-context boundary
```

A separate `Cargo.toml` carries semver, docs.rs, package metadata, feature,
release-order, and support obligations. The durable states are therefore:

- real public package;
- internal package with `publish = false`, limited to tools, tests, examples, or
  repository maintenance;
- module family under an owner crate.

Production-path implementation seams should not remain as separate hidden
`publish = false` microcrates. Design them like microcrates, but implement them
as crate-grade module families unless the policy marks them as public packages.

## Target Public Packages

The target public surface is the following 18 packages:

| Package | Public reason |
| --- | --- |
| `openracing` | Facade SDK and stable “start here” crate. |
| `openracing-engine` | Runtime engine, RT loop, device orchestration, and safety execution. |
| `openracing-ffb` | FFB effects, force models, filters, and compiled output plans. |
| `openracing-calibration` | Reusable axis/deadzone/normalization kernel. |
| `openracing-curves` | Reusable LUT/Bezier/remap math. |
| `openracing-profile` | Profile, tuning, input-map, preset, and serialization contract. |
| `openracing-hid` | Generic HID transport, descriptor/capture/replay, and vendor protocol family. |
| `openracing-pidff` | Cross-vendor HID PIDFF report and safety layer. |
| `openracing-moza` | Validated Moza family: R5, KS, ES, SR-P, HBP, and native PIDFF receipts. |
| `openracing-firmware-update` | Firmware/update safety boundary. |
| `openracing-plugin-abi` | Hard plugin ABI contract. |
| `openracing-plugin-sdk` | Plugin-author SDK without host/runtime internals. |
| `openracing-telemetry` | Normalized telemetry model, traits, streams, and orchestration. |
| `openracing-telemetry-adapters` | Game adapter family with separate cadence. |
| `openracing-telemetry-config` | Support matrix and config writers. |
| `openracing-telemetry-recorder` | Recording, playback, fixtures, and replay. |
| `openracing-service` | `wheeld` product package. |
| `wheelctl` | Operator CLI product package. |

## Internal Packages

Internal packages are allowed for repository maintenance, tests, compatibility
fixtures, examples, and unpublished app surfaces during the migration. They must
be explicitly listed in `policy/crate-boundaries.toml` and have `publish = false`.
Obvious examples are `openracing-tools`, `workspace-hack`, integration tests,
test helpers, UI experiments, compatibility fixtures, and plugin examples.

## Collapse Map

The collapse map in `policy/crate-boundaries.toml` is the machine-readable source
of truth for current packages that are not durable public packages. Each entry
records:

- the current package name;
- the future module-family path;
- the owner public crate;
- the reason it is not an independent public support promise.

The first policy PR does not move or rename code. Later PRs move packages into
module families according to the map and keep imports passing against the policy
checker.

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

`BOUNDARY.md` template:

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
- Sibling modules import through the façade.
- Internals are private or `pub(crate)`.
- Do not `pub use *` from implementation modules.
- Do not cross-family deep-import implementation modules.
- Do not add a new `Cargo.toml` for a family unless policy marks it public.

## Dependency Layering

- `openracing` may depend on public library crates only.
- `openracing-service` and `wheelctl` may depend on public libraries and
  platform/system crates, but public library crates may not depend on them.
- `openracing-engine` may depend on FFB, HID, PIDFF, Moza, profile, telemetry,
  and plugin ABI crates. It may not depend on service or wheelctl.
- `openracing-ffb` may depend on calibration, curves, and profile. It may not
  depend on engine, service, wheelctl, or HID transport.
- `openracing-hid` may depend on PIDFF only for generic PIDFF helpers if needed.
  It may not depend on engine, service, wheelctl, or telemetry.
- `openracing-moza` may depend on HID, PIDFF, curves, or calibration if needed.
  It may not depend on engine, service, or wheelctl.
- `openracing-pidff` may not depend on Moza, engine, service, or wheelctl.
- `openracing-plugin-abi` stays low-level and host-independent.
- `openracing-plugin-sdk` may depend on plugin ABI and public model crates. It
  may not depend on native/WASM host runtime internals.

## Feature Policy

Features are public API. Keep them additive, product-oriented, and stable.
Default features are sticky and must not be casually removed.

Use features such as:

```toml
[features]
default = ["std"]
std = []
serde = ["dep:serde"]
moza = ["dep:openracing-moza"]
telemetry = ["dep:openracing-telemetry"]
plugins = ["dep:openracing-plugin-sdk"]
```

Avoid extraction or former-microcrate feature names such as
`openracing-scheduler`, `openracing-pipeline`, `telemetry-lfs-crate`, or
`hid-moza-protocol`.

## Migration Sequence

1. Freeze the target public surface with this document, the policy file,
   `[workspace.metadata.publish].allow`, `workspace.default-members`, and the
   `package-surface` checker.
2. Add the facade/naming spine without collapsing packages.
3. Finish telemetry consolidation into the four target telemetry crates.
4. Promote PIDFF to `openracing-pidff`.
5. Collapse generic HID support into `openracing-hid`.
6. Collapse Moza accessories/protocols into `openracing-moza`.
7. Collapse remaining HID vendors under `openracing-hid::protocol::*`.
8. Collapse FFB implementation satellites into `openracing-ffb`.
9. Collapse engine-owned RT, safety, hardware, diagnostics, and error helpers.
10. Collapse profile storage and input maps into `openracing-profile`.
11. Split plugin-author API from service-owned host runtimes.
12. Classify or move app/tool boundaries.
13. Delete old package directories after imports are clean.
14. Run package proof for every final public crate.

## Packaging Proof

For each final public crate, run:

```text
cargo test -p <crate> --all-features --locked
cargo clippy -p <crate> --all-targets --all-features --locked -- -D warnings
cargo doc -p <crate> --all-features --no-deps --locked
cargo package -p <crate> --list
cargo publish -p <crate> --dry-run --locked
```

The package-surface checker will ratchet metadata and package-proof enforcement
once the collapse and naming migration have landed.

## Non-Goals

This policy PR does not:

- move code;
- rename packages;
- alter Moza hardware artifacts;
- run hardware-output commands;
- collapse telemetry, HID, FFB, or engine helper code;
- combine with CI optimization, Clippy policy, or hardware-lane work;
- use `publish = false` as a substitute for collapsing production-path crates.
