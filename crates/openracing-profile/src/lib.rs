//! Profile types and serialization
//!
//! This crate provides profile definitions for racing wheel configurations.

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::unwrap_used)]

pub mod types;
pub mod validation;

pub use types::*;
pub use validation::*;

use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum ProfileError {
    #[error("Invalid profile: {0}")]
    InvalidProfile(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    #[error("Profile not found: {0}")]
    NotFound(String),
}

pub type ProfileResult<T> = Result<T, ProfileError>;

pub fn generate_profile_id() -> String {
    Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_profile_id() {
        let id = generate_profile_id();
        assert!(!id.is_empty());
        assert!(Uuid::parse_str(&id).is_ok());
    }
}
