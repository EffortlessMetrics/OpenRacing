//! Device and hardware-related error types.
//!
//! This module provides error types for device discovery, connection,
//! communication, and hardware failures.

use crate::common::ErrorSeverity;

/// Device and hardware errors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum DeviceError {
    /// Device not found
    #[error("Device not found: {0}")]
    NotFound(String),

    /// Device disconnected
    #[error("Device disconnected: {0}")]
    Disconnected(String),

    /// Connection failed
    #[error("Failed to connect to device: {0}")]
    ConnectionFailed(String),

    /// Communication error
    #[error("Communication error with device {device}: {message}")]
    CommunicationError {
        /// Device identifier
        device: String,
        /// Error message
        message: String,
    },

    /// HID error
    #[error("HID error: {0}")]
    HidError(String),

    /// Invalid device response
    #[error("Invalid response from device {device}: expected {expected} bytes, got {actual}")]
    InvalidResponse {
        /// Device identifier
        device: String,
        /// Expected byte count
        expected: usize,
        /// Actual byte count
        actual: usize,
    },

    /// Device timeout
    #[error("Device {device} timeout after {timeout_ms}ms")]
    Timeout {
        /// Device identifier
        device: String,
        /// Timeout in milliseconds
        timeout_ms: u64,
    },

    /// Unsupported device
    #[error("Unsupported device: vendor={vendor_id:#06x}, product={product_id:#06x}")]
    UnsupportedDevice {
        /// USB vendor ID
        vendor_id: u16,
        /// USB product ID
        product_id: u16,
    },

    /// Device busy
    #[error("Device {0} is busy")]
    Busy(String),

    /// Permission denied
    #[error("Permission denied for device: {0}")]
    PermissionDenied(String),

    /// Device initialization failed
    #[error("Failed to initialize device {device}: {reason}")]
    InitializationFailed {
        /// Device identifier
        device: String,
        /// Failure reason
        reason: String,
    },

    /// Firmware error
    #[error("Firmware error on device {device}: {message}")]
    FirmwareError {
        /// Device identifier
        device: String,
        /// Error message
        message: String,
    },

    /// Feature not supported
    #[error("Feature '{feature}' not supported by device {device}")]
    FeatureNotSupported {
        /// Device identifier
        device: String,
        /// Feature name
        feature: String,
    },
}

impl DeviceError {
    /// Get the error severity.
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            DeviceError::NotFound(_) => ErrorSeverity::Error,
            DeviceError::Disconnected(_) => ErrorSeverity::Critical,
            DeviceError::ConnectionFailed(_) => ErrorSeverity::Error,
            DeviceError::CommunicationError { .. } => ErrorSeverity::Error,
            DeviceError::HidError(_) => ErrorSeverity::Error,
            DeviceError::InvalidResponse { .. } => ErrorSeverity::Error,
            DeviceError::Timeout { .. } => ErrorSeverity::Warning,
            DeviceError::UnsupportedDevice { .. } => ErrorSeverity::Error,
            DeviceError::Busy(_) => ErrorSeverity::Warning,
            DeviceError::PermissionDenied(_) => ErrorSeverity::Error,
            DeviceError::InitializationFailed { .. } => ErrorSeverity::Error,
            DeviceError::FirmwareError { .. } => ErrorSeverity::Error,
            DeviceError::FeatureNotSupported { .. } => ErrorSeverity::Info,
        }
    }

    /// Check if this error indicates the device is unavailable.
    pub fn is_device_unavailable(&self) -> bool {
        matches!(
            self,
            DeviceError::NotFound(_)
                | DeviceError::Disconnected(_)
                | DeviceError::PermissionDenied(_)
        )
    }

    /// Check if retrying the operation might succeed.
    pub fn is_retryable(&self) -> bool {
        matches!(self, DeviceError::Timeout { .. } | DeviceError::Busy(_))
    }

    /// Create a not found error.
    pub fn not_found(device: impl Into<String>) -> Self {
        DeviceError::NotFound(device.into())
    }

    /// Create a disconnected error.
    pub fn disconnected(device: impl Into<String>) -> Self {
        DeviceError::Disconnected(device.into())
    }

    /// Create a timeout error.
    pub fn timeout(device: impl Into<String>, timeout_ms: u64) -> Self {
        DeviceError::Timeout {
            device: device.into(),
            timeout_ms,
        }
    }

    /// Create an unsupported device error.
    pub fn unsupported(vendor_id: u16, product_id: u16) -> Self {
        DeviceError::UnsupportedDevice {
            vendor_id,
            product_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_error_severity() {
        assert_eq!(
            DeviceError::disconnected("test").severity(),
            ErrorSeverity::Critical
        );
        assert_eq!(
            DeviceError::timeout("test", 1000).severity(),
            ErrorSeverity::Warning
        );
    }

    #[test]
    fn test_device_error_is_device_unavailable() {
        assert!(DeviceError::not_found("test").is_device_unavailable());
        assert!(DeviceError::disconnected("test").is_device_unavailable());
        assert!(!DeviceError::timeout("test", 1000).is_device_unavailable());
    }

    #[test]
    fn test_device_error_is_retryable() {
        assert!(DeviceError::timeout("test", 1000).is_retryable());
        assert!(DeviceError::Busy("test".into()).is_retryable());
        assert!(!DeviceError::not_found("test").is_retryable());
    }

    #[test]
    fn test_device_error_display() {
        let err = DeviceError::unsupported(0x1234, 0x5678);
        let msg = err.to_string();
        assert!(msg.contains("1234"));
        assert!(msg.contains("5678"));
    }

    #[test]
    fn test_device_error_is_std_error() {
        let err = DeviceError::not_found("test");
        let _: &dyn std::error::Error = &err;
    }

    #[test]
    fn test_device_error_constructors() {
        let err = DeviceError::not_found("moza-r9");
        assert!(matches!(err, DeviceError::NotFound(_)));

        let err = DeviceError::timeout("moza-r9", 500);
        assert!(matches!(err, DeviceError::Timeout { .. }));
    }
}
