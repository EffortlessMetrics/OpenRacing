//! Racing Wheel UI - Tauri application
//!
//! This crate provides the OpenRacing desktop application built with Tauri.
//! It communicates with the wheeld service via IPC to manage racing wheel
//! devices, profiles, and telemetry.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Tauri UI Application                      │
//! ├─────────────────────────────────────────────────────────────┤
//! │  Frontend (HTML/JS)                                          │
//! │    ├── Device List View                                      │
//! │    ├── Device Detail View                                    │
//! │    ├── Profile Management                                    │
//! │    └── Telemetry Display                                     │
//! ├─────────────────────────────────────────────────────────────┤
//! │  Tauri Commands (Rust)                                       │
//! │    ├── list_devices()                                        │
//! │    ├── get_device_status()                                   │
//! │    ├── apply_profile()                                       │
//! │    ├── get_telemetry()                                       │
//! │    └── emergency_stop()                                      │
//! ├─────────────────────────────────────────────────────────────┤
//! │  IPC Client (gRPC)                                           │
//! │    └── WheelServiceClient                                    │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    wheeld Service                            │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Requirements Coverage
//!
//! - **7.1**: Device list display
//! - **7.2**: Device status display
//! - **7.3**: Profile loading and application
//! - **7.4**: Real-time telemetry display
//! - **7.5**: User-friendly error messages
//! - **7.6**: IPC communication with wheeld service

#![deny(static_mut_refs)]
#![deny(unused_must_use)]
#![deny(clippy::unwrap_used)]

pub mod commands;
pub mod error;
pub mod safety;

pub use commands::*;
pub use error::*;
pub use safety::*;
