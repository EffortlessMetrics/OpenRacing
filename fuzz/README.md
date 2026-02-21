# OpenRacing Fuzz Targets

LibFuzzer-based fuzz targets for the EA F1 25 native UDP packet parser.

## Requirements

- Rust nightly toolchain
- `cargo-fuzz`: `cargo install cargo-fuzz`
- Linux or macOS (libFuzzer is not supported on Windows without WSL)

## Running Fuzz Targets

```sh
# Fuzz the end-to-end normalize() entry point (recommended first target)
cargo +nightly fuzz run fuzz_f1_25_normalize

# Fuzz the packet header parser
cargo +nightly fuzz run fuzz_f1_25_header

# Fuzz CarTelemetry (packet_id=6) parsing
cargo +nightly fuzz run fuzz_f1_25_car_telemetry

# Fuzz CarStatus (packet_id=7) parsing
cargo +nightly fuzz run fuzz_f1_25_car_status
```

## CI Integration

Fuzz targets are verified via `cargo check` in the stable toolchain CI pipeline.
LibFuzzer-based continuous fuzzing is intended to run in a dedicated Linux CI job
using `cargo +nightly fuzz run`.

## Corpus

Seed corpus files should be placed in `corpus/<target_name>/`. Crashing inputs
are saved to `artifacts/<target_name>/`.
