//! Profile and configuration-related error types.
//!
//! This module provides error types for profile loading, saving,
//! validation, and inheritance operations.

use crate::common::ErrorSeverity;

/// Profile and configuration errors.
///
/// # Examples
///
/// ```
/// use openracing_errors::{ProfileError, ErrorSeverity};
///
/// // Profile not found
/// let err = ProfileError::not_found("iracing-gt3");
/// assert_eq!(err.severity(), ErrorSeverity::Error);
/// assert!(!err.is_inheritance_error());
///
/// // Circular inheritance detection
/// let err = ProfileError::circular_inheritance("a -> b -> a");
/// assert!(err.is_inheritance_error());
///
/// // Version mismatch
/// let err = ProfileError::version_mismatch("2.0", "1.0");
/// assert_eq!(err.severity(), ErrorSeverity::Warning);
/// ```
#[derive(Debug, Clone, thiserror::Error)]
pub enum ProfileError {
    /// Profile not found
    #[error("Profile not found: {0}")]
    NotFound(String),

    /// Profile already exists
    #[error("Profile already exists: {0}")]
    AlreadyExists(String),

    /// Invalid profile format
    #[error("Invalid profile format in {path}: {reason}")]
    InvalidFormat {
        /// File path or profile source
        path: String,
        /// Reason for the format error
        reason: String,
    },

    /// Profile validation failed
    #[error("Profile validation failed: {0}")]
    ValidationFailed(String),

    /// Profile save failed
    #[error("Failed to save profile {profile}: {reason}")]
    SaveFailed {
        /// Profile identifier
        profile: String,
        /// Failure reason
        reason: String,
    },

    /// Profile load failed
    #[error("Failed to load profile from {path}: {reason}")]
    LoadFailed {
        /// File path or source
        path: String,
        /// Failure reason
        reason: String,
    },

    /// Circular inheritance detected
    #[error("Circular profile inheritance detected: {chain}")]
    CircularInheritance {
        /// The inheritance chain that was detected
        chain: String,
    },

    /// Inheritance depth exceeded
    #[error("Profile inheritance depth exceeded: {depth} levels (max: {max_depth})")]
    InheritanceDepthExceeded {
        /// Current depth
        depth: usize,
        /// Maximum allowed depth
        max_depth: usize,
    },

    /// Parent profile not found
    #[error("Parent profile not found: {parent_id}")]
    ParentNotFound {
        /// Parent profile identifier
        parent_id: String,
    },

    /// Invalid profile ID
    #[error("Invalid profile ID: {0}")]
    InvalidId(String),

    /// Profile conflict
    #[error("Profile conflict: {0}")]
    Conflict(String),

    /// Profile version mismatch
    #[error("Profile version mismatch: expected {expected}, found {found}")]
    VersionMismatch {
        /// Expected version
        expected: String,
        /// Found version
        found: String,
    },

    /// Missing required field
    #[error("Missing required field '{field}' in profile {profile}")]
    MissingField {
        /// Profile identifier
        profile: String,
        /// Missing field name
        field: String,
    },

    /// Profile locked
    #[error("Profile {0} is locked and cannot be modified")]
    Locked(String),

    /// Invalid device mapping
    #[error("Invalid device mapping in profile {profile}: device '{device}' not found")]
    InvalidDeviceMapping {
        /// Profile identifier
        profile: String,
        /// Device identifier
        device: String,
    },
}

impl ProfileError {
    /// Get the error severity.
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            ProfileError::NotFound(_) => ErrorSeverity::Error,
            ProfileError::AlreadyExists(_) => ErrorSeverity::Error,
            ProfileError::InvalidFormat { .. } => ErrorSeverity::Error,
            ProfileError::ValidationFailed(_) => ErrorSeverity::Error,
            ProfileError::SaveFailed { .. } => ErrorSeverity::Error,
            ProfileError::LoadFailed { .. } => ErrorSeverity::Error,
            ProfileError::CircularInheritance { .. } => ErrorSeverity::Error,
            ProfileError::InheritanceDepthExceeded { .. } => ErrorSeverity::Error,
            ProfileError::ParentNotFound { .. } => ErrorSeverity::Error,
            ProfileError::InvalidId(_) => ErrorSeverity::Error,
            ProfileError::Conflict(_) => ErrorSeverity::Warning,
            ProfileError::VersionMismatch { .. } => ErrorSeverity::Warning,
            ProfileError::MissingField { .. } => ErrorSeverity::Error,
            ProfileError::Locked(_) => ErrorSeverity::Warning,
            ProfileError::InvalidDeviceMapping { .. } => ErrorSeverity::Error,
        }
    }

    /// Check if this error is related to profile inheritance.
    pub fn is_inheritance_error(&self) -> bool {
        matches!(
            self,
            ProfileError::CircularInheritance { .. }
                | ProfileError::InheritanceDepthExceeded { .. }
                | ProfileError::ParentNotFound { .. }
        )
    }

    /// Check if this error is related to profile storage.
    pub fn is_storage_error(&self) -> bool {
        matches!(
            self,
            ProfileError::SaveFailed { .. } | ProfileError::LoadFailed { .. }
        )
    }

    /// Create a not found error.
    pub fn not_found(profile_id: impl Into<String>) -> Self {
        ProfileError::NotFound(profile_id.into())
    }

    /// Create an invalid format error.
    pub fn invalid_format(path: impl Into<String>, reason: impl Into<String>) -> Self {
        ProfileError::InvalidFormat {
            path: path.into(),
            reason: reason.into(),
        }
    }

    /// Create a circular inheritance error.
    pub fn circular_inheritance(chain: impl Into<String>) -> Self {
        ProfileError::CircularInheritance {
            chain: chain.into(),
        }
    }

    /// Create a version mismatch error.
    pub fn version_mismatch(expected: impl Into<String>, found: impl Into<String>) -> Self {
        ProfileError::VersionMismatch {
            expected: expected.into(),
            found: found.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_error_severity() {
        assert_eq!(
            ProfileError::not_found("test").severity(),
            ErrorSeverity::Error
        );
        assert_eq!(
            ProfileError::Locked("test".into()).severity(),
            ErrorSeverity::Warning
        );
    }

    #[test]
    fn test_profile_error_is_inheritance_error() {
        assert!(ProfileError::circular_inheritance("a -> b -> a").is_inheritance_error());
        assert!(
            ProfileError::ParentNotFound {
                parent_id: "parent".into()
            }
            .is_inheritance_error()
        );
        assert!(!ProfileError::not_found("test").is_inheritance_error());
    }

    #[test]
    fn test_profile_error_is_storage_error() {
        assert!(
            ProfileError::SaveFailed {
                profile: "test".into(),
                reason: "disk full".into()
            }
            .is_storage_error()
        );
        assert!(!ProfileError::not_found("test").is_storage_error());
    }

    #[test]
    fn test_profile_error_display() {
        let err = ProfileError::version_mismatch("2.0", "1.0");
        let msg = err.to_string();
        assert!(msg.contains("2.0"));
        assert!(msg.contains("1.0"));
    }

    #[test]
    fn test_profile_error_is_std_error() {
        let err = ProfileError::not_found("test");
        let _: &dyn std::error::Error = &err;
    }

    #[test]
    fn test_profile_error_constructors() {
        let err = ProfileError::invalid_format("/path/to/profile.yaml", "missing name field");
        assert!(matches!(err, ProfileError::InvalidFormat { .. }));

        let err = ProfileError::version_mismatch("2.0.0", "1.0.0");
        assert!(matches!(err, ProfileError::VersionMismatch { .. }));
    }
}
