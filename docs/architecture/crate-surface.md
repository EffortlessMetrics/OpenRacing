# OpenRacing Crate Surface

OpenRacing keeps strong single-responsibility boundaries without treating every implementation seam as a crates.io support promise. The crate-surface policy freezes the target public package set before any package rename or code movement happens.

## Doctrine

Core rule:

```text
Cargo package boundary = public support promise
Rust module boundary   = SRP / ownership / agent-context boundary
```

A separate `Cargo.toml` creates a semver surface, a docs.rs page, package metadata obligations, a feature matrix, release-order coupling, and a long-term support promise. OpenRacing therefore uses three durable states:

* **Public package**: a durable external contract that may be published and supported independently.
* **Internal package**: a `publish = false` package for repo tooling, tests, fixtures, examples, or development-only support.
* **Module family**: an implementation seam owned by a public crate, with crate-grade folders and boundary documentation but no separate registry package.

Production-path implementation seams should migrate toward module families instead of remaining indefinitely as `publish = false` microcrates.

## Target Public Packages

The target public surface is intentionally limited to 18 packages:

| Package | Role |
| --- | --- |
| `openracing` | Facade SDK and stable start-here crate. |
| `openracing-engine` | Runtime engine, RT loop, orchestration, and safety execution. |
| `openracing-ffb` | FFB effects, force models, filters, and compiled output plans. |
| `openracing-calibration` | Reusable axis/deadzone/normalization kernel. |
| `openracing-curves` | Reusable LUT, Bezier, and remap math. |
| `openracing-profile` | Profiles, tuning, input maps, presets, and serialization contract. |
| `openracing-hid` | Generic HID transport, descriptor/capture/replay, and vendor protocol family. |
| `openracing-pidff` | Cross-vendor HID PIDFF reports and safety layer. |
| `openracing-moza` | Receipt-backed Moza family support. |
| `openracing-firmware-update` | Isolated firmware/update safety boundary. |
| `openracing-plugin-abi` | Hard plugin ABI contract. |
| `openracing-plugin-sdk` | Plugin-author SDK without host-runtime internals. |
| `openracing-telemetry` | Normalized telemetry model, traits, streams, and orchestration. |
| `openracing-telemetry-adapters` | Game adapter family with separate cadence. |
| `openracing-telemetry-config` | Support matrix and config writers. |
| `openracing-telemetry-recorder` | Recording, playback, fixtures, and replay. |
| `openracing-service` | `wheeld` product package. |
| `wheelctl` | Operator CLI product package. |

The root `[workspace.metadata.publish].allow` list and `policy/crate-boundaries.toml` must agree on this set.

## Internal Packages

Internal packages are reserved for repository maintenance, tests, examples, compatibility shims, schemas, and other non-public support code. They must use `publish = false` when they exist as workspace packages.

Examples include `openracing-tools`, `workspace-hack`, `racing-wheel-integration-tests`, `racing-wheel-ui`, `compat`, `openracing-test-helpers`, and plugin examples. Internal status is not a loophole for production-path microcrates; production seams should either become public contracts or collapse into an owner crate as module families.

## Collapse Map

The machine-readable collapse map is `policy/crate-boundaries.toml`. It records each current implementation package that should eventually become a module family under an owner crate. The first policy PR does not move code or rename packages; it only records the intended destination and gives future PRs a pass/fail target.

Major collapse families are:

* Moza accessory and report crates into `openracing-moza`.
* HID vendor protocol leaves into `openracing-hid::protocol::*`.
* HID capture and device support into `openracing-hid`.
* FFB helper crates into `openracing-ffb`.
* Engine RT, safety, diagnostics, and hardware-evidence helpers into `openracing-engine`.
* Profile storage and input maps into `openracing-profile`.
* Telemetry satellites into the four public telemetry packages.
* Plugin host runtimes into `openracing-service`, leaving only ABI and SDK public.

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

* `mod.rs` is the family façade.
* Siblings import through the façade.
* Internals are private or `pub(crate)`.
* Avoid `pub use *` from implementation modules.
* Avoid cross-family deep imports.
* Do not add a new `Cargo.toml` for a family unless the policy marks it public.

## Dependency Layering

Layering is documented here first and can be tightened by later enforcement:

* `openracing` may depend on public library crates only.
* `openracing-service` and `wheelctl` may depend on public libraries and platform/system crates, but public libraries may not depend on them.
* `openracing-engine` may depend on FFB, HID, PIDFF, Moza, profile, telemetry, and plugin ABI crates, but not on app packages.
* `openracing-ffb` may depend on calibration, curves, and profile, but not engine, service, wheelctl, or HID transport.
* `openracing-hid` may depend on PIDFF only for generic helpers and may not depend on engine, service, wheelctl, or telemetry.
* `openracing-moza` may depend on HID, PIDFF, curves, or calibration as needed, but not engine, service, or wheelctl.
* `openracing-pidff` may not depend on Moza, engine, service, or wheelctl.
* `openracing-plugin-abi` must stay low-level and host-independent.
* `openracing-plugin-sdk` may depend on plugin ABI and public model crates, but not native/WASM host-runtime internals.

## Feature Policy

Features are public API. They should be additive, product-oriented, and stable enough to support across releases.

Use product features such as:

```toml
[features]
default = ["std"]
std = []
serde = ["dep:serde"]
moza = ["dep:openracing-moza"]
telemetry = ["dep:openracing-telemetry"]
plugins = ["dep:openracing-plugin-sdk"]
```

Avoid extraction or old-microcrate feature names such as:

```toml
openracing-scheduler = []
openracing-pipeline = []
telemetry-lfs-crate = []
hid-moza-protocol = []
```

## Migration Sequence

The migration ladder is:

1. `policy-crate-surface`: add this document, the policy file, publish allowlist, default members, and checker.
2. `facade-naming-spine`: introduce `openracing` and naming transition façade work.
3. `telemetry-finish`: complete telemetry consolidation into four public telemetry crates.
4. `pidff-promote`: rename and stabilize PIDFF.
5. `hid-core-collapse`: create `openracing-hid` and move generic HID support.
6. `moza-family-collapse`: create `openracing-moza` and move Moza family leaves.
7. `hid-vendor-collapse`: move remaining vendor protocols under `openracing-hid`.
8. `ffb-collapse`: collapse FFB helper satellites.
9. `engine-helper-collapse`: collapse engine-owned runtime, safety, diagnostic, and evidence helpers.
10. `profile-collapse`: collapse profile repository and input maps.
11. `plugin-sdk-split`: keep ABI/SDK public and move host runtimes into service.
12. `app-boundary`: classify app/tool packages physically if needed.
13. `delete-old-packages`: remove old package directories once imports are clean.
14. `package-proof`: run full package proof for every final public crate.

## Packaging Proof

The package-proof phase must validate every final public crate with:

```text
cargo test -p <crate> --all-features --locked
cargo clippy -p <crate> --all-targets --all-features --locked -- -D warnings
cargo doc -p <crate> --all-features --no-deps --locked
cargo package -p <crate> --list
cargo publish -p <crate> --dry-run --locked
```

Each final public package must have workspace version/edition/rust-version/license/authors/repository metadata plus homepage, documentation, README, description, keywords, categories, and `publish = true`.

## Non-Goals

The first policy PR does not:

* Move code.
* Rename packages.
* Alter Moza hardware artifacts or receipts.
* Run hardware-output commands.
* Collapse telemetry, HID, FFB, or engine helper crates.
* Change resolver policy.
* Optimize CI or change Clippy policy.
* Use `publish = false` as a substitute for collapsing production-path seams.
