//! Convenience re-exports for common types

pub use crate::error::{ProfileRepositoryError, StorageError, ValidationError};
pub use crate::migration::MigrationAdapter;
pub use crate::repository::{ProfileRepository, ProfileRepositoryConfig};
pub use crate::signature::{ProfileSignature, TrustState};
pub use crate::storage::FileStorage;
pub use crate::validation::ProfileValidationContext;

pub use racing_wheel_schemas::prelude::{
    BaseSettings, FilterConfig, HapticsConfig, LedConfig, Profile, ProfileId, ProfileMetadata,
    ProfileScope,
};
