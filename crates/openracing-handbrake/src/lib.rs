//! Handbrake input protocol for sim racing
//!
//! This crate provides support for analog and digital handbrakes.
//! Supports hall effect sensors, potentiometers, and load cell handbrakes.

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::unwrap_used)]

pub mod types;
pub mod input;

pub use types::*;
pub use input::*;

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
}
