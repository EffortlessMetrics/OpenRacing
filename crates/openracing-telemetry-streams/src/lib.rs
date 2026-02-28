//! Telemetry streaming utilities
//!
//! This crate provides utilities for streaming and processing telemetry data.

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::unwrap_used)]

pub mod buffer;
pub mod processing;

pub use buffer::*;
pub use processing::*;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum StreamError {
    #[error("Buffer overflow")]
    BufferOverflow,
    
    #[error("Stream closed")]
    StreamClosed,
    
    #[error("Processing error: {0}")]
    ProcessingError(String),
}

pub type StreamResult<T> = Result<T, StreamError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_types() {
        let err = StreamError::BufferOverflow;
        assert_eq!(format!("{}", err), "Buffer overflow");
        
        let err = StreamError::StreamClosed;
        assert_eq!(format!("{}", err), "Stream closed");
    }
}
