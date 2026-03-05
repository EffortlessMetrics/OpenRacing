//! Example plugins for the OpenRacing plugin system.
//!
//! This crate provides reference implementations showing how to build plugins
//! that integrate with the OpenRacing force-feedback pipeline. Each example
//! targets a different plugin capability:
//!
//! | Module | Capability | Description |
//! |--------|-----------|-------------|
//! | [`road_surface`] | DSP / Haptics | Simulates road surface texture via FFB |
//! | [`telemetry_logger`] | Telemetry | Records telemetry snapshots to a ring buffer |
//! | [`dashboard_overlay`] | Telemetry | Computes dashboard data (gear, RPM bar, flags) |
//!
//! # Building as WASM
//!
//! The examples are written in pure Rust with no OS dependencies, so they
//! compile to both native and `wasm32-unknown-unknown` targets:
//!
//! ```bash
//! # Native (for tests and benchmarks)
//! cargo build -p openracing-plugin-examples
//!
//! # WASM (for sandboxed execution)
//! cargo build -p openracing-plugin-examples --target wasm32-unknown-unknown
//! ```
//!
//! # Plugin ABI
//!
//! All examples use types from [`openracing_plugin_abi`] — the stable,
//! `#[repr(C)]` ABI shared between host and plugin. See that crate's
//! documentation for the full contract.

#![deny(static_mut_refs)]
#![deny(unused_must_use)]
#![deny(clippy::unwrap_used)]

pub mod dashboard_overlay;
pub mod road_surface;
pub mod telemetry_logger;
