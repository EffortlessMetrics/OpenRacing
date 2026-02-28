//! Profile repository re-exports
//!
//! This module re-exports types from the `openracing-profile-repository` crate
//! for backward compatibility.

pub use openracing_profile_repository::{
    ProfileRepository, ProfileRepositoryConfig, ProfileRepositoryError, ProfileSignature,
    TrustState,
};

// Re-export commonly used types from schemas
pub use racing_wheel_schemas::prelude::{
    BaseSettings, FilterConfig, HapticsConfig, LedConfig, Profile, ProfileId, ProfileMetadata,
    ProfileScope,
};
