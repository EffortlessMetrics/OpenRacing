# openracing-filters

RT-safe filter implementations for OpenRacing force feedback pipeline.

## Overview

This crate provides real-time safe filter implementations for the force feedback
pipeline. All filters are designed to operate at 1kHz with strict timing requirements.

## Filter Types

- **Reconstruction**: Anti-aliasing filter for smoothing high-frequency content
- **Friction**: Speed-adaptive friction simulation
- **Damper**: Speed-adaptive velocity-proportional resistance
- **Inertia**: Rotational inertia simulation
- **Notch**: Biquad notch filter for eliminating specific frequencies
- **Slew Rate**: Rate-of-change limiter
- **Curve**: Lookup table-based curve mapping
- **Response Curve**: Response curve transformation
- **Bumpstop**: Physical steering stop simulation
- **Hands-Off**: Detection of user hands-off condition

## RT Safety Guarantees

All filter implementations are RT-safe:

- No heap allocations in filter hot paths
- O(1) time complexity for all operations
- Bounded execution time
- No syscalls or I/O in filter functions
- All state types are `#[repr(C)]` for stable ABI

## Usage

```rust
use openracing_filters::prelude::*;

// Create filter states at initialization time
let mut recon_state = ReconstructionState::new(4);
let mut slew_state = SlewRateState::new(0.5);

// In the RT loop (1kHz):
let mut frame = Frame::default();
frame.ffb_in = 0.5;
frame.torque_out = 0.5;

// Apply filters (RT-safe)
reconstruction_filter(&mut frame, &mut recon_state);
slew_rate_filter(&mut frame, &mut slew_state);
```

## Speed-Adaptive Filters

The friction and damper filters support speed-adaptive behavior:

```rust
// Friction decreases at higher speeds
let friction = FrictionState::adaptive(0.1);

// Damping increases at higher speeds
let damper = DamperState::adaptive(0.1);
```

## Safety Features

### Torque Capping

```rust
// Limit maximum torque for safety
torque_cap_filter(&mut frame, 0.8); // 80% max
```

### Hands-Off Detection

```rust
// Detect when user releases wheel
let hands_off = HandsOffState::new(true, 0.05, 2.0); // 2 second timeout
hands_off_detector(&mut frame, &mut hands_off);

if frame.hands_off {
    // Apply safety behavior
}
```

## Performance

All filters are designed to meet strict RT requirements:

- Maximum execution time: < 1μs per filter
- No heap allocations in hot paths
- Cache-friendly data structures

Run benchmarks:

```bash
cargo bench --bench filter_benchmarks
```

## Testing

### Unit Tests
```bash
cargo test
```

### Property Tests
```bash
cargo test --features proptest
```

### Fuzzing Tests
```bash
cargo test fuzz_tests
```

## License

MIT OR Apache-2.0