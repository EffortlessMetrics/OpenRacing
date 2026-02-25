//! Error types for FMEA operations.

use crate::FaultType;
use core::fmt;

/// FMEA operation errors.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum FmeaError {
    /// No FMEA entry found for the specified fault type.
    UnknownFaultType(FaultType),
    /// Fault handling failed.
    FaultHandlingFailed {
        /// Fault type that failed to handle.
        fault_type: FaultType,
        /// Reason for failure.
        reason: heapless::String<128>,
    },
    /// Threshold validation failed.
    InvalidThreshold {
        /// Name of the invalid threshold.
        name: heapless::String<32>,
        /// Reason for invalidity.
        reason: heapless::String<64>,
    },
    /// Recovery procedure failed.
    RecoveryFailed {
        /// Fault type being recovered.
        fault_type: FaultType,
        /// Reason for failure.
        reason: heapless::String<128>,
    },
    /// Soft-stop operation failed.
    SoftStopFailed {
        /// Reason for failure.
        reason: heapless::String<64>,
    },
    /// Plugin quarantine error.
    QuarantineError {
        /// Plugin identifier.
        plugin_id: heapless::String<64>,
        /// Reason for error.
        reason: heapless::String<64>,
    },
    /// FMEA matrix configuration error.
    ConfigurationError {
        /// Description of the error.
        description: heapless::String<128>,
    },
    /// Fault already active.
    FaultAlreadyActive(FaultType),
    /// No active fault to clear.
    NoActiveFault,
    /// Operation timed out.
    Timeout {
        /// Operation that timed out.
        operation: heapless::String<32>,
        /// Timeout duration in milliseconds.
        timeout_ms: u64,
    },
}

impl fmt::Display for FmeaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FmeaError::UnknownFaultType(ft) => {
                write!(f, "No FMEA entry for fault type: {}", ft)
            }
            FmeaError::FaultHandlingFailed { fault_type, reason } => {
                write!(f, "Failed to handle fault {}: {}", fault_type, reason)
            }
            FmeaError::InvalidThreshold { name, reason } => {
                write!(f, "Invalid threshold '{}': {}", name, reason)
            }
            FmeaError::RecoveryFailed { fault_type, reason } => {
                write!(f, "Recovery failed for {}: {}", fault_type, reason)
            }
            FmeaError::SoftStopFailed { reason } => {
                write!(f, "Soft-stop failed: {}", reason)
            }
            FmeaError::QuarantineError { plugin_id, reason } => {
                write!(f, "Quarantine error for plugin '{}': {}", plugin_id, reason)
            }
            FmeaError::ConfigurationError { description } => {
                write!(f, "Configuration error: {}", description)
            }
            FmeaError::FaultAlreadyActive(ft) => {
                write!(f, "Fault {} is already active", ft)
            }
            FmeaError::NoActiveFault => {
                write!(f, "No active fault to clear")
            }
            FmeaError::Timeout {
                operation,
                timeout_ms,
            } => {
                write!(
                    f,
                    "Operation '{}' timed out after {}ms",
                    operation, timeout_ms
                )
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for FmeaError {}

/// Result type for FMEA operations.
pub type FmeaResult<T> = Result<T, FmeaError>;

impl FmeaError {
    /// Create a new fault handling error.
    pub fn fault_handling_failed(fault_type: FaultType, reason: &str) -> Self {
        let mut r = heapless::String::new();
        let _ = r.push_str(reason);
        FmeaError::FaultHandlingFailed {
            fault_type,
            reason: r,
        }
    }

    /// Create a new invalid threshold error.
    pub fn invalid_threshold(name: &str, reason: &str) -> Self {
        let mut n = heapless::String::new();
        let _ = n.push_str(name);
        let mut r = heapless::String::new();
        let _ = r.push_str(reason);
        FmeaError::InvalidThreshold { name: n, reason: r }
    }

    /// Create a new recovery failed error.
    pub fn recovery_failed(fault_type: FaultType, reason: &str) -> Self {
        let mut r = heapless::String::new();
        let _ = r.push_str(reason);
        FmeaError::RecoveryFailed {
            fault_type,
            reason: r,
        }
    }

    /// Create a new soft-stop failed error.
    pub fn soft_stop_failed(reason: &str) -> Self {
        let mut r = heapless::String::new();
        let _ = r.push_str(reason);
        FmeaError::SoftStopFailed { reason: r }
    }

    /// Create a new quarantine error.
    pub fn quarantine_error(plugin_id: &str, reason: &str) -> Self {
        let mut p = heapless::String::new();
        let _ = p.push_str(plugin_id);
        let mut r = heapless::String::new();
        let _ = r.push_str(reason);
        FmeaError::QuarantineError {
            plugin_id: p,
            reason: r,
        }
    }

    /// Create a new configuration error.
    pub fn configuration_error(description: &str) -> Self {
        let mut d = heapless::String::new();
        let _ = d.push_str(description);
        FmeaError::ConfigurationError { description: d }
    }

    /// Create a new timeout error.
    pub fn timeout(operation: &str, timeout_ms: u64) -> Self {
        let mut o = heapless::String::new();
        let _ = o.push_str(operation);
        FmeaError::Timeout {
            operation: o,
            timeout_ms,
        }
    }

    /// Check if this error is recoverable.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            FmeaError::Timeout { .. }
                | FmeaError::QuarantineError { .. }
                | FmeaError::RecoveryFailed { .. }
        )
    }

    /// Check if this error requires immediate attention.
    pub fn requires_immediate_attention(&self) -> bool {
        matches!(
            self,
            FmeaError::FaultHandlingFailed { .. }
                | FmeaError::SoftStopFailed { .. }
                | FmeaError::ConfigurationError { .. }
        )
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn test_fmea_error_display() {
        let err = FmeaError::UnknownFaultType(FaultType::UsbStall);
        let s = format!("{}", err);
        assert!(s.contains("USB communication stall"));

        let err = FmeaError::fault_handling_failed(FaultType::ThermalLimit, "test reason");
        let s = format!("{}", err);
        assert!(s.contains("Thermal protection"));
        assert!(s.contains("test reason"));
    }

    #[test]
    fn test_fmea_error_helpers() {
        let err = FmeaError::invalid_threshold("thermal_limit", "must be positive");
        assert!(matches!(err, FmeaError::InvalidThreshold { .. }));

        let err = FmeaError::soft_stop_failed("ramp failed");
        assert!(matches!(err, FmeaError::SoftStopFailed { .. }));

        let err = FmeaError::timeout("recovery", 5000);
        assert!(matches!(err, FmeaError::Timeout { .. }));
    }

    #[test]
    fn test_fmea_error_recoverable() {
        assert!(FmeaError::timeout("test", 100).is_recoverable());
        assert!(
            FmeaError::QuarantineError {
                plugin_id: heapless::String::new(),
                reason: heapless::String::new(),
            }
            .is_recoverable()
        );
        assert!(
            !FmeaError::ConfigurationError {
                description: heapless::String::new(),
            }
            .is_recoverable()
        );
    }

    #[test]
    fn test_fmea_error_requires_attention() {
        assert!(
            FmeaError::SoftStopFailed {
                reason: heapless::String::new(),
            }
            .requires_immediate_attention()
        );
        assert!(!FmeaError::timeout("test", 100).requires_immediate_attention());
    }
}
