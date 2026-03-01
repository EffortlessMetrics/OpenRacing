//! Shifter input protocol for sim racing
//!
//! This crate provides support for sequential and H-pattern shifters.
//! Supports standard USB HID gamepad reports and dedicated shifter protocols.

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::unwrap_used)]

pub mod input;
pub mod types;

pub use input::*;
pub use types::*;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ShifterError {
    #[error("Invalid gear: {0}")]
    InvalidGear(i32),

    #[error("Invalid report format")]
    InvalidReport,

    #[error("Shifter disconnected")]
    Disconnected,
}

pub type ShifterResult<T> = Result<T, ShifterError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(MAX_GEARS, 8);
        assert_eq!(NEUTRAL_GEAR, 0);
    }

    #[test]
    fn test_error_display_invalid_gear() {
        let err = ShifterError::InvalidGear(99);
        assert!(err.to_string().contains("99"));
    }

    #[test]
    fn test_error_display_invalid_report() {
        let err = ShifterError::InvalidReport;
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn test_error_display_disconnected() {
        let err = ShifterError::Disconnected;
        assert!(
            err.to_string().contains("disconnected") || err.to_string().contains("Disconnected")
        );
    }

    #[test]
    fn test_error_debug_format() {
        let err = ShifterError::InvalidGear(7);
        let debug = format!("{:?}", err);
        assert!(!debug.is_empty());

        let err2 = ShifterError::InvalidReport;
        let debug2 = format!("{:?}", err2);
        assert!(!debug2.is_empty());

        let err3 = ShifterError::Disconnected;
        let debug3 = format!("{:?}", err3);
        assert!(!debug3.is_empty());
    }

    #[test]
    fn test_shifter_result_ok() -> ShifterResult<()> {
        let val: ShifterResult<i32> = Ok(3);
        assert!(val.is_ok());
        Ok(())
    }

    #[test]
    fn test_shifter_result_err() {
        let val: ShifterResult<i32> = Err(ShifterError::InvalidGear(-1));
        assert!(val.is_err());
        assert!(matches!(val, Err(ShifterError::InvalidGear(-1))));
    }

    #[test]
    fn test_error_invalid_report_matches() {
        let result: ShifterResult<()> = Err(ShifterError::InvalidReport);
        assert!(matches!(result, Err(ShifterError::InvalidReport)));
    }
}
