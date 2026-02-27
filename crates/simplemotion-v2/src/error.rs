//! SimpleMotion V2 error types.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SmError {
    #[error("Invalid report length: expected {expected}, got {actual}")]
    InvalidLength { expected: usize, actual: usize },

    #[error("Invalid command type: {0}")]
    InvalidCommandType(u8),

    #[error("Invalid parameter address: {0}")]
    InvalidParameter(u16),

    #[error("Device error: {0}")]
    DeviceError(String),

    #[error("Communication error: {0}")]
    CommunicationError(String),

    #[error("CRC mismatch: expected {expected}, got {actual}")]
    CrcMismatch { expected: u8, actual: u8 },

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Encode error: {0}")]
    EncodeError(String),
}

pub type SmResult<T> = Result<T, SmError>;

impl From<std::io::Error> for SmError {
    fn from(e: std::io::Error) -> Self {
        SmError::CommunicationError(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = SmError::InvalidLength {
            expected: 64,
            actual: 32,
        };
        assert_eq!(
            err.to_string(),
            "Invalid report length: expected 64, got 32"
        );
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "device not found");
        let sm_err: SmError = io_err.into();
        assert!(matches!(sm_err, SmError::CommunicationError(_)));
    }
}
