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
pub mod test_harness;
pub mod ports;
pub mod policies;
pub mod profile_service;

pub use rt::*;
pub use pipeline::*;
pub use scheduler::*;
pub use safety::*;
pub use ffb::*;
pub use test_harness::*;
pub use profile_service::*;

// Re-export specific items to avoid conflicts
pub use device::{VirtualDevice, VirtualHidPort, DeviceEvent, TelemetryData, DeviceInfo};
pub use ports::{
    HidDevice, HidPort, TelemetryPort, ProfileRepo, ProfileRepoError, 
    NormalizedTelemetry, TelemetryFlags, ProfileContext, DeviceHealthStatus,
    TelemetryStatistics, ConfigurationStatus, ConfigChange, RepositoryStatus
};
pub use policies::{SafetyPolicy, ProfileHierarchyPolicy, SafetyViolation, ProfileHierarchyError};
pub use protocol::{TorqueCommand, DeviceTelemetryReport, DeviceCapabilitiesReport};