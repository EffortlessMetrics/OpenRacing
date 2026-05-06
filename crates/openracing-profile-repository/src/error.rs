//! Error types for profile repository operations

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during profile repository operations
#[derive(Error, Debug)]
pub enum ProfileRepositoryError {
    /// IO error during file operations
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// Profile validation failed
    #[error("Profile validation failed: {0}")]
    ValidationFailed(String),

    /// Profile migration failed
    #[error("Profile migration failed: {0}")]
    MigrationFailed(String),

    /// Schema version not supported
    #[error("Unsupported schema version: {0}")]
    UnsupportedSchemaVersion(String),

    /// Profile not found
    #[error("Profile not found: {0}")]
    ProfileNotFound(String),

    /// Signature verification failed
    #[error("Signature verification failed: {0}")]
    SignatureError(String),

    /// Invalid profile ID
    #[error("Invalid profile ID: {0}")]
    InvalidProfileId(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Cache operation failed
    #[error("Cache error: {0}")]
    CacheError(String),

    /// File path error
    #[error("File path error for {path}: {reason}")]
    FilePathError {
        /// The problematic path
        path: PathBuf,
        /// The reason for the error
        reason: String,
    },

    /// Atomic write failed
    #[error("Atomic write failed: temp file at {temp_path}, target at {target_path}")]
    AtomicWriteFailed {
        /// Path to the temporary file
        temp_path: PathBuf,
        /// Path to the target file
        target_path: PathBuf,
    },

    /// Profile scope mismatch
    #[error("Profile scope mismatch: expected {expected}, got {actual}")]
    ScopeMismatch {
        /// Expected scope
        expected: String,
        /// Actual scope
        actual: String,
    },

    /// Profile hierarchy resolution failed
    #[error("Failed to resolve profile hierarchy: {0}")]
    HierarchyResolutionFailed(String),
}

impl ProfileRepositoryError {
    /// Check if this error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::ProfileNotFound(_) => true,
            Self::ValidationFailed(_) => false,
            Self::MigrationFailed(_) => true,
            Self::UnsupportedSchemaVersion(_) => false,
            Self::SignatureError(_) => true,
            Self::IoError(_) => true,
            Self::JsonError(_) => false,
            Self::InvalidProfileId(_) => false,
            Self::ConfigError(_) => false,
            Self::CacheError(_) => true,
            Self::FilePathError { .. } => false,
            Self::AtomicWriteFailed { .. } => true,
            Self::ScopeMismatch { .. } => false,
            Self::HierarchyResolutionFailed(_) => true,
        }
    }

    /// Create a validation error with context
    pub fn validation_failed(field: &str, reason: &str) -> Self {
        Self::ValidationFailed(format!("{}: {}", field, reason))
    }

    /// Create a file path error
    pub fn file_path_error(path: impl Into<PathBuf>, reason: impl Into<String>) -> Self {
        Self::FilePathError {
            path: path.into(),
            reason: reason.into(),
        }
    }

    /// Create an atomic write error
    pub fn atomic_write_failed(temp: impl Into<PathBuf>, target: impl Into<PathBuf>) -> Self {
        Self::AtomicWriteFailed {
            temp_path: temp.into(),
            target_path: target.into(),
        }
    }
}

/// Storage-specific errors
#[derive(Error, Debug)]
pub enum StorageError {
    /// Failed to read file
    #[error("Failed to read file {path}: {source}")]
    ReadFailed {
        /// Path to the file
        path: PathBuf,
        /// Source error
        source: std::io::Error,
    },

    /// Failed to write file
    #[error("Failed to write file {path}: {source}")]
    WriteFailed {
        /// Path to the file
        path: PathBuf,
        /// Source error
        source: std::io::Error,
    },

    /// Failed to create directory
    #[error("Failed to create directory {path}: {source}")]
    DirectoryCreationFailed {
        /// Path to the directory
        path: PathBuf,
        /// Source error
        source: std::io::Error,
    },

    /// File already exists
    #[error("File already exists: {0}")]
    FileExists(PathBuf),

    /// Directory not found
    #[error("Directory not found: {0}")]
    DirectoryNotFound(PathBuf),

    /// Permission denied
    #[error("Permission denied for {operation} on {path}")]
    PermissionDenied {
        /// The operation being performed
        operation: String,
        /// The path involved
        path: PathBuf,
    },
}

impl StorageError {
    /// Create a read error
    pub fn read_failed(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::ReadFailed {
            path: path.into(),
            source,
        }
    }

    /// Create a write error
    pub fn write_failed(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::WriteFailed {
            path: path.into(),
            source,
        }
    }
}

/// Validation-specific errors
#[derive(Error, Debug)]
pub enum ValidationError {
    /// Missing required field
    #[error("Missing required field: {0}")]
    MissingField(String),

    /// Invalid field value
    #[error("Invalid value for field '{field}': {reason}")]
    InvalidValue {
        /// The field name
        field: String,
        /// The reason for invalidity
        reason: String,
    },

    /// Schema version mismatch
    #[error("Schema version mismatch: expected {expected}, got {actual}")]
    SchemaVersionMismatch {
        /// Expected version
        expected: String,
        /// Actual version
        actual: String,
    },

    /// Curve points not monotonic
    #[error("Curve points are not monotonically increasing")]
    NonMonotonicCurve,

    /// RPM bands not sorted
    #[error("RPM bands must be in ascending order")]
    UnsortedRpmBands,

    /// Value out of range
    #[error("Value {value} for field '{field}' is out of range [{min}, {max}]")]
    OutOfRange {
        /// The field name
        field: String,
        /// The invalid value
        value: f32,
        /// Minimum allowed value
        min: f32,
        /// Maximum allowed value
        max: f32,
    },
}

impl ValidationError {
    /// Create a missing field error
    pub fn missing_field(field: impl Into<String>) -> Self {
        Self::MissingField(field.into())
    }

    /// Create an invalid value error
    pub fn invalid_value(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidValue {
            field: field.into(),
            reason: reason.into(),
        }
    }

    /// Create an out of range error
    pub fn out_of_range(field: impl Into<String>, value: f32, min: f32, max: f32) -> Self {
        Self::OutOfRange {
            field: field.into(),
            value,
            min,
            max,
        }
    }
}

impl From<ValidationError> for ProfileRepositoryError {
    fn from(err: ValidationError) -> Self {
        Self::ValidationFailed(err.to_string())
    }
}

impl From<StorageError> for ProfileRepositoryError {
    fn from(err: StorageError) -> Self {
        match err {
            StorageError::ReadFailed { path, source } => {
                Self::file_path_error(path, format!("Read failed: {}", source))
            }
            StorageError::WriteFailed { path, source } => {
                Self::file_path_error(path, format!("Write failed: {}", source))
            }
            StorageError::DirectoryCreationFailed { path, source } => {
                Self::file_path_error(path, format!("Directory creation failed: {}", source))
            }
            StorageError::FileExists(path) => Self::file_path_error(path, "File already exists"),
            StorageError::DirectoryNotFound(path) => {
                Self::file_path_error(path, "Directory not found")
            }
            StorageError::PermissionDenied { operation, path } => {
                Self::file_path_error(path, format!("Permission denied for {}", operation))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // ProfileRepositoryError::is_recoverable
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_recoverable_profile_not_found() {
        let err = ProfileRepositoryError::ProfileNotFound("id".to_string());
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_is_recoverable_validation_failed() {
        let err = ProfileRepositoryError::ValidationFailed("bad".to_string());
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_is_recoverable_migration_failed() {
        let err = ProfileRepositoryError::MigrationFailed("old".to_string());
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_is_recoverable_unsupported_schema() {
        let err = ProfileRepositoryError::UnsupportedSchemaVersion("99".to_string());
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_is_recoverable_signature_error() {
        let err = ProfileRepositoryError::SignatureError("bad sig".to_string());
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_is_recoverable_json_error() {
        let raw_err =
            serde_json::from_str::<serde_json::Value>("not json").expect_err("Expected JSON error");
        let err = ProfileRepositoryError::JsonError(raw_err);
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_is_recoverable_invalid_profile_id() {
        let err = ProfileRepositoryError::InvalidProfileId("???".to_string());
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_is_recoverable_config_error() {
        let err = ProfileRepositoryError::ConfigError("bad config".to_string());
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_is_recoverable_cache_error() {
        let err = ProfileRepositoryError::CacheError("cache miss".to_string());
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_is_recoverable_file_path_error() {
        let err = ProfileRepositoryError::FilePathError {
            path: PathBuf::from("/tmp/test"),
            reason: "not found".to_string(),
        };
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_is_recoverable_atomic_write_failed() {
        let err = ProfileRepositoryError::AtomicWriteFailed {
            temp_path: PathBuf::from("/tmp/temp"),
            target_path: PathBuf::from("/tmp/target"),
        };
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_is_recoverable_scope_mismatch() {
        let err = ProfileRepositoryError::ScopeMismatch {
            expected: "global".to_string(),
            actual: "car".to_string(),
        };
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_is_recoverable_hierarchy_resolution() {
        let err = ProfileRepositoryError::HierarchyResolutionFailed("no root".to_string());
        assert!(err.is_recoverable());
    }

    // -----------------------------------------------------------------------
    // Display strings
    // -----------------------------------------------------------------------

    #[test]
    fn test_display_profile_not_found() {
        let err = ProfileRepositoryError::ProfileNotFound("abc".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("abc"));
        assert!(msg.contains("not found"));
    }

    #[test]
    fn test_display_scope_mismatch() {
        let err = ProfileRepositoryError::ScopeMismatch {
            expected: "global".to_string(),
            actual: "session".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("global"));
        assert!(msg.contains("session"));
    }

    // -----------------------------------------------------------------------
    // Factory methods
    // -----------------------------------------------------------------------

    #[test]
    fn test_validation_failed_factory() {
        let err = ProfileRepositoryError::validation_failed("gain", "out of range");
        let msg = format!("{}", err);
        assert!(msg.contains("gain"));
        assert!(msg.contains("out of range"));
    }

    #[test]
    fn test_file_path_error_factory() {
        let err = ProfileRepositoryError::file_path_error("/data/profile.json", "corrupt");
        match &err {
            ProfileRepositoryError::FilePathError { path, reason } => {
                assert!(path.to_string_lossy().contains("profile.json"));
                assert_eq!(reason, "corrupt");
            }
            other => panic!("expected FilePathError, got {:?}", other),
        }
    }

    #[test]
    fn test_atomic_write_failed_factory() {
        let err = ProfileRepositoryError::atomic_write_failed("/tmp/t", "/data/p");
        match &err {
            ProfileRepositoryError::AtomicWriteFailed {
                temp_path,
                target_path,
            } => {
                assert!(temp_path.to_string_lossy().contains("t"));
                assert!(target_path.to_string_lossy().contains("p"));
            }
            other => panic!("expected AtomicWriteFailed, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // From conversions
    // -----------------------------------------------------------------------

    #[test]
    fn test_from_validation_error() {
        let err = ValidationError::MissingField("name".to_string());
        let repo_err: ProfileRepositoryError = err.into();
        let msg = format!("{}", repo_err);
        assert!(msg.contains("name"));
    }

    #[test]
    fn test_from_storage_error_read_failed() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let err = StorageError::read_failed("/data/file.json", io_err);
        let repo_err: ProfileRepositoryError = err.into();
        let msg = format!("{}", repo_err);
        assert!(msg.contains("file.json"));
    }

    #[test]
    fn test_from_storage_error_file_exists() {
        let err = StorageError::FileExists(PathBuf::from("/data/exists.json"));
        let repo_err: ProfileRepositoryError = err.into();
        let msg = format!("{}", repo_err);
        assert!(msg.contains("exists.json"));
    }

    #[test]
    fn test_from_storage_error_permission_denied() {
        let err = StorageError::PermissionDenied {
            operation: "write".to_string(),
            path: PathBuf::from("/data/locked.json"),
        };
        let repo_err: ProfileRepositoryError = err.into();
        let msg = format!("{}", repo_err);
        assert!(msg.contains("Permission denied"));
        assert!(msg.contains("write"));
    }

    // -----------------------------------------------------------------------
    // ValidationError factories
    // -----------------------------------------------------------------------

    #[test]
    fn test_validation_error_missing_field() {
        let err = ValidationError::missing_field("device_id");
        let msg = format!("{}", err);
        assert!(msg.contains("device_id"));
    }

    #[test]
    fn test_validation_error_invalid_value() {
        let err = ValidationError::invalid_value("gain", "must be in [0, 1]");
        let msg = format!("{}", err);
        assert!(msg.contains("gain"));
        assert!(msg.contains("must be in [0, 1]"));
    }

    #[test]
    fn test_validation_error_out_of_range() {
        let err = ValidationError::out_of_range("torque", 150.0, 0.0, 100.0);
        let msg = format!("{}", err);
        assert!(msg.contains("torque"));
        assert!(msg.contains("150"));
    }

    #[test]
    fn test_validation_error_non_monotonic() {
        let err = ValidationError::NonMonotonicCurve;
        let msg = format!("{}", err);
        assert!(msg.contains("monotonic"));
    }

    #[test]
    fn test_validation_error_unsorted_rpm() {
        let err = ValidationError::UnsortedRpmBands;
        let msg = format!("{}", err);
        assert!(msg.contains("RPM"));
    }

    // -----------------------------------------------------------------------
    // StorageError factories
    // -----------------------------------------------------------------------

    #[test]
    fn test_storage_error_write_failed() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let err = StorageError::write_failed("/tmp/out.json", io_err);
        let msg = format!("{}", err);
        assert!(msg.contains("out.json"));
        assert!(msg.contains("denied"));
    }
}
