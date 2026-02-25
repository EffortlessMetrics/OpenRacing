//! Error types for hardware watchdog operations.

use alloc::string::String;

/// Errors that can occur during hardware watchdog operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HardwareWatchdogError {
    /// Watchdog is not armed.
    NotArmed,
    /// Watchdog is already armed.
    AlreadyArmed,
    /// Watchdog has timed out.
    TimedOut,
    /// Hardware communication error.
    HardwareError(String),
    /// Invalid configuration.
    InvalidConfiguration(String),
    /// State transition not allowed.
    InvalidTransition {
        /// Current state.
        from: &'static str,
        /// Attempted target state.
        to: &'static str,
    },
    /// Safe state was already triggered.
    SafeStateAlreadyTriggered,
    /// Operation would exceed WCET budget.
    WcetExceeded,
}

impl HardwareWatchdogError {
    /// Create a hardware error.
    #[must_use]
    pub fn hardware_error(msg: impl Into<String>) -> Self {
        Self::HardwareError(msg.into())
    }

    /// Create an invalid configuration error.
    #[must_use]
    pub fn invalid_configuration(msg: impl Into<String>) -> Self {
        Self::InvalidConfiguration(msg.into())
    }

    /// Create an invalid transition error.
    #[must_use]
    pub fn invalid_transition(from: &'static str, to: &'static str) -> Self {
        Self::InvalidTransition { from, to }
    }
}

impl core::fmt::Display for HardwareWatchdogError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotArmed => write!(f, "Watchdog is not armed"),
            Self::AlreadyArmed => write!(f, "Watchdog is already armed"),
            Self::TimedOut => write!(f, "Watchdog has timed out"),
            Self::HardwareError(msg) => write!(f, "Hardware error: {msg}"),
            Self::InvalidConfiguration(msg) => write!(f, "Invalid configuration: {msg}"),
            Self::InvalidTransition { from, to } => {
                write!(f, "Invalid state transition: {from} -> {to}")
            }
            Self::SafeStateAlreadyTriggered => write!(f, "Safe state already triggered"),
            Self::WcetExceeded => write!(f, "Operation would exceed WCET budget"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for HardwareWatchdogError {}

/// A specialized `Result` type for hardware watchdog operations.
pub type HardwareWatchdogResult<T> = core::result::Result<T, HardwareWatchdogError>;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn test_error_display() {
        assert_eq!(
            HardwareWatchdogError::NotArmed.to_string(),
            "Watchdog is not armed"
        );
        assert_eq!(
            HardwareWatchdogError::AlreadyArmed.to_string(),
            "Watchdog is already armed"
        );
        assert_eq!(
            HardwareWatchdogError::TimedOut.to_string(),
            "Watchdog has timed out"
        );
    }

    #[test]
    fn test_error_constructors() {
        let err = HardwareWatchdogError::hardware_error("I2C failure");
        assert!(matches!(err, HardwareWatchdogError::HardwareError(_)));

        let err = HardwareWatchdogError::invalid_configuration("timeout too low");
        assert!(matches!(
            err,
            HardwareWatchdogError::InvalidConfiguration(_)
        ));

        let err = HardwareWatchdogError::invalid_transition("TimedOut", "Armed");
        assert!(matches!(
            err,
            HardwareWatchdogError::InvalidTransition { .. }
        ));
    }
}
