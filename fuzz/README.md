# OpenRacing Fuzz Targets

LibFuzzer-based fuzz targets for protocol parsers, telemetry adapters, and HID
device input handlers across the OpenRacing codebase.

## Requirements

- Rust nightly toolchain
- `cargo-fuzz`: `cargo install cargo-fuzz`
- Linux or macOS (libFuzzer is not supported on Windows without WSL)

## Running Fuzz Targets

```sh
# Telemetry adapters
cargo +nightly fuzz run fuzz_f1_25_normalize       # EA F1 25 end-to-end
cargo +nightly fuzz run fuzz_f1_25_header          # F1 25 packet header
cargo +nightly fuzz run fuzz_f1_25_car_telemetry   # F1 25 CarTelemetry
cargo +nightly fuzz run fuzz_f1_25_car_status      # F1 25 CarStatus
cargo +nightly fuzz run fuzz_acc_udp               # Assetto Corsa Competizione UDP
cargo +nightly fuzz run fuzz_codemasters_udp       # Codemasters UDP (Dirt/WRC)
cargo +nightly fuzz run fuzz_dirt_rally_2          # Dirt Rally 2.0
cargo +nightly fuzz run fuzz_gran_turismo_7        # Gran Turismo 7
cargo +nightly fuzz run fuzz_rbr                   # Richard Burns Rally

# HID device protocol parsers
cargo +nightly fuzz run fuzz_asetek_input
cargo +nightly fuzz run fuzz_cammus_direct
cargo +nightly fuzz run fuzz_fanatec_input
cargo +nightly fuzz run fuzz_ffbeast_input
cargo +nightly fuzz run fuzz_heusinkveld_input
cargo +nightly fuzz run fuzz_logitech_input
cargo +nightly fuzz run fuzz_moza_input
cargo +nightly fuzz run fuzz_moza_direct_torque_encode
cargo +nightly fuzz run fuzz_moza_handshake_frames
cargo +nightly fuzz run fuzz_moza_hbp_report
cargo +nightly fuzz run fuzz_moza_wheelbase_report
cargo +nightly fuzz run fuzz_openffboard_input
cargo +nightly fuzz run fuzz_simagic_input
cargo +nightly fuzz run fuzz_simplemotion
cargo +nightly fuzz run fuzz_simucube_input
cargo +nightly fuzz run fuzz_thrustmaster_input
cargo +nightly fuzz run fuzz_vrs_input

# Schema / protocol
cargo +nightly fuzz run fuzz_ks_report
cargo +nightly fuzz run fuzz_srp_report
```

## CI Integration

Fuzz targets are verified via `cargo check` in the stable toolchain CI pipeline.
LibFuzzer-based continuous fuzzing is intended to run in a dedicated Linux CI job
using `cargo +nightly fuzz run`.

## Corpus

Seed corpus files should be placed in `corpus/<target_name>/`. Crashing inputs
are saved to `artifacts/<target_name>/`.
