//! Racing Wheel Engine - Real-time Force Feedback Core
//!
//! This crate contains the real-time force feedback engine that operates at 1kHz
//! with strict timing requirements and zero-allocation hot paths.

#[cfg(feature = "rt-allocator")]
use mimalloc::MiMalloc;

#[cfg(feature = "rt-allocator")]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

pub mod rt;
pub mod pipeline;
pub mod scheduler;
pub mod safety;
pub mod device;
pub mod ffb;
pub mod protocol;
// TODO: Re-implement test harness once core functionality is stable

pub use rt::*;
pub use pipeline::*;
pub use scheduler::*;
pub use safety::*;
pub use device::*;
pub use ffb::*;
pub use protocol::*;
// TODO: Re-export test harness types once implemented