//! Error types for the scheduler crate.

use std::fmt;
use std::fmt::Display;

/// Real-time error codes (pre-allocated for RT path)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RTError {
    /// Device disconnected during operation
    DeviceDisconnected = 1,
    /// Torque limit exceeded safety threshold
    TorqueLimit = 2,
    /// Pipeline processing fault
    PipelineFault = 3,
    /// Real-time timing violation (jitter exceeded threshold)
    TimingViolation = 4,
    /// Failed to apply real-time setup
    RTSetupFailed = 5,
    /// Invalid configuration parameter
    InvalidConfig = 6,
}

impl Display for RTError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RTError::DeviceDisconnected => write!(f, "Device disconnected"),
            RTError::TorqueLimit => write!(f, "Torque limit exceeded"),
            RTError::PipelineFault => write!(f, "Pipeline processing fault"),
            RTError::TimingViolation => write!(f, "Real-time timing violation"),
            RTError::RTSetupFailed => write!(f, "Failed to apply real-time setup"),
            RTError::InvalidConfig => write!(f, "Invalid configuration parameter"),
        }
    }
}

impl std::error::Error for RTError {}

/// RT-safe result type
pub type RTResult<T = ()> = Result<T, RTError>;
