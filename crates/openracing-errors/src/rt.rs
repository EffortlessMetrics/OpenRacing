//! Real-time specific error types.
//!
//! These error types are designed for use in RT code paths with specific
//! safety guarantees:
//! - Copy semantics (no heap allocations)
//! - Pre-allocated error codes for RT-safe reporting
//! - Fixed-size representation

use core::fmt;

use crate::common::ErrorSeverity;

/// Real-time error codes (pre-allocated for RT path).
///
/// These errors are designed to be RT-safe:
/// - `Copy` semantics ensure no heap allocations
/// - Fixed `#[repr(u8)]` representation
/// - Pre-defined error codes for fast classification
///
/// # Examples
///
/// ```
/// use openracing_errors::{RTError, ErrorSeverity};
///
/// let err = RTError::TimingViolation;
///
/// // RT errors have numeric codes for efficient logging
/// assert_eq!(err.code(), 4);
///
/// // Check severity for escalation decisions
/// assert_eq!(err.severity(), ErrorSeverity::Warning);
///
/// // Check if immediate safety action is needed
/// assert!(!err.requires_safety_action());
///
/// // Recoverable errors can be retried
/// assert!(err.is_recoverable());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    /// Invalid configuration parameter in RT path
    InvalidConfig = 6,
    /// Safety interlock triggered
    SafetyInterlock = 7,
    /// Buffer overflow in RT path
    BufferOverflow = 8,
    /// Deadline missed
    DeadlineMissed = 9,
    /// Resource unavailable in RT path
    ResourceUnavailable = 10,
}

impl RTError {
    /// Get the numeric error code.
    ///
    /// # Examples
    ///
    /// ```
    /// use openracing_errors::RTError;
    ///
    /// assert_eq!(RTError::DeviceDisconnected.code(), 1);
    /// assert_eq!(RTError::TorqueLimit.code(), 2);
    /// ```
    pub fn code(self) -> u8 {
        self as u8
    }

    /// Get the error severity.
    pub fn severity(self) -> ErrorSeverity {
        match self {
            RTError::DeviceDisconnected => ErrorSeverity::Critical,
            RTError::TorqueLimit => ErrorSeverity::Critical,
            RTError::PipelineFault => ErrorSeverity::Error,
            RTError::TimingViolation => ErrorSeverity::Warning,
            RTError::RTSetupFailed => ErrorSeverity::Critical,
            RTError::InvalidConfig => ErrorSeverity::Error,
            RTError::SafetyInterlock => ErrorSeverity::Critical,
            RTError::BufferOverflow => ErrorSeverity::Warning,
            RTError::DeadlineMissed => ErrorSeverity::Critical,
            RTError::ResourceUnavailable => ErrorSeverity::Error,
        }
    }

    /// Check if this error requires immediate safety action.
    pub fn requires_safety_action(self) -> bool {
        matches!(
            self,
            RTError::DeviceDisconnected
                | RTError::TorqueLimit
                | RTError::SafetyInterlock
                | RTError::DeadlineMissed
        )
    }

    /// Check if this error is recoverable without restart.
    pub fn is_recoverable(self) -> bool {
        matches!(
            self,
            RTError::TimingViolation | RTError::BufferOverflow | RTError::ResourceUnavailable
        )
    }

    /// Create an error from a code.
    ///
    /// Returns `None` if the code does not correspond to a known error.
    ///
    /// # Examples
    ///
    /// ```
    /// use openracing_errors::RTError;
    ///
    /// assert_eq!(RTError::from_code(1), Some(RTError::DeviceDisconnected));
    /// assert_eq!(RTError::from_code(255), None);
    /// ```
    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(RTError::DeviceDisconnected),
            2 => Some(RTError::TorqueLimit),
            3 => Some(RTError::PipelineFault),
            4 => Some(RTError::TimingViolation),
            5 => Some(RTError::RTSetupFailed),
            6 => Some(RTError::InvalidConfig),
            7 => Some(RTError::SafetyInterlock),
            8 => Some(RTError::BufferOverflow),
            9 => Some(RTError::DeadlineMissed),
            10 => Some(RTError::ResourceUnavailable),
            _ => None,
        }
    }
}

impl fmt::Display for RTError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RTError::DeviceDisconnected => write!(f, "Device disconnected"),
            RTError::TorqueLimit => write!(f, "Torque limit exceeded"),
            RTError::PipelineFault => write!(f, "Pipeline processing fault"),
            RTError::TimingViolation => write!(f, "Real-time timing violation"),
            RTError::RTSetupFailed => write!(f, "Failed to apply real-time setup"),
            RTError::InvalidConfig => write!(f, "Invalid configuration parameter"),
            RTError::SafetyInterlock => write!(f, "Safety interlock triggered"),
            RTError::BufferOverflow => write!(f, "RT buffer overflow"),
            RTError::DeadlineMissed => write!(f, "RT deadline missed"),
            RTError::ResourceUnavailable => write!(f, "RT resource unavailable"),
        }
    }
}

impl std::error::Error for RTError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rt_error_codes() {
        assert_eq!(RTError::DeviceDisconnected.code(), 1);
        assert_eq!(RTError::TorqueLimit.code(), 2);
        assert_eq!(RTError::PipelineFault.code(), 3);
    }

    #[test]
    fn test_rt_error_from_code() {
        assert_eq!(RTError::from_code(1), Some(RTError::DeviceDisconnected));
        assert_eq!(RTError::from_code(255), None);
    }

    #[test]
    fn test_rt_error_severity() {
        assert_eq!(
            RTError::DeviceDisconnected.severity(),
            ErrorSeverity::Critical
        );
        assert_eq!(RTError::TimingViolation.severity(), ErrorSeverity::Warning);
    }

    #[test]
    fn test_rt_error_requires_safety_action() {
        assert!(RTError::TorqueLimit.requires_safety_action());
        assert!(!RTError::InvalidConfig.requires_safety_action());
    }

    #[test]
    fn test_rt_error_is_recoverable() {
        assert!(RTError::TimingViolation.is_recoverable());
        assert!(!RTError::DeviceDisconnected.is_recoverable());
    }

    #[test]
    fn test_rt_error_display() {
        let err = RTError::DeviceDisconnected;
        assert_eq!(err.to_string(), "Device disconnected");
    }

    #[test]
    fn test_rt_error_is_std_error() {
        let err = RTError::PipelineFault;
        let _: &dyn std::error::Error = &err;
    }

    #[test]
    fn test_rt_error_copy() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<RTError>();
    }
}
