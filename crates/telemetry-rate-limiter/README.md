# racing-wheel-telemetry-rate-limiter

Small, focused crate for telemetry rate limiting.

## Purpose

- `RateLimiter` for fixed-rate gatekeeping.
- `AdaptiveRateLimiter` for CPU-aware adjustment.
- Monitoring stats and drop-rate reporting.

## Usage

The crate contains standalone utilities intended to be shared by service and other
runtime components that process high-rate telemetry input.
