//! Moza Racing HID protocol: report parsing, handshake frames, and FFB encoding.
//!
//! This crate is intentionally I/O-free and allocation-free on hot paths.
//! It provides pure functions and types that can be tested and fuzzed without
//! hardware or OS-level HID plumbing.

#![deny(static_mut_refs)]

pub mod direct;
pub mod ids;
pub mod protocol;
pub mod report;
pub mod rt_types;
pub mod signature;
pub mod standalone;
pub mod types;
pub mod writer;

// Flat re-exports so callers can use `racing_wheel_hid_moza_protocol::Foo`.
pub use direct::*;
pub use ids::*;
pub use protocol::*;
pub use report::*;
pub use rt_types::*;
pub use signature::*;
pub use standalone::*;
pub use types::*;
pub use writer::*;
