# racing-wheel-telemetry-game-id

Single-responsibility microcrate that owns telemetry game-id alias normalization.

This crate is intentionally tiny and allocation-free so all telemetry layers can
share one canonical mapping without duplicating logic.
