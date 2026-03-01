//! Handbrake input protocol for sim racing
//!
//! This crate provides support for analog and digital handbrakes.
//! Supports hall effect sensors, potentiometers, and load cell handbrakes.

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::unwrap_used)]

pub mod input;
pub mod types;

pub use input::*;
pub use types::*;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum HandbrakeError {
    #[error("Invalid position value: {0}")]
    InvalidPosition(u16),

    #[error("Handbrake disconnected")]
    Disconnected,
}

pub type HandbrakeResult<T> = Result<T, HandbrakeError>;

pub const MAX_ANALOG_VALUE: u16 = 0xFFFF;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(MAX_ANALOG_VALUE, 0xFFFF);
    }

    #[test]
    fn test_error_display_invalid_position() {
        let err = HandbrakeError::InvalidPosition(1234);
        assert!(err.to_string().contains("1234"));
    }

    #[test]
    fn test_error_display_disconnected() {
        let err = HandbrakeError::Disconnected;
        assert!(
            err.to_string().contains("disconnected") || err.to_string().contains("Disconnected")
        );
    }

    #[test]
    fn test_error_debug_format() {
        let err = HandbrakeError::InvalidPosition(42);
        let debug = format!("{:?}", err);
        assert!(!debug.is_empty());

        let err2 = HandbrakeError::Disconnected;
        let debug2 = format!("{:?}", err2);
        assert!(!debug2.is_empty());
    }

    #[test]
    fn test_handbrake_result_ok() -> HandbrakeResult<()> {
        let val: HandbrakeResult<u32> = Ok(42);
        assert!(val.is_ok());
        Ok(())
    }

    #[test]
    fn test_handbrake_result_err() {
        let val: HandbrakeResult<u32> = Err(HandbrakeError::InvalidPosition(999));
        assert!(val.is_err());
        assert!(matches!(val, Err(HandbrakeError::InvalidPosition(999))));
    }
}
