//! Prelude module for convenient error handling imports.
//!
//! This module re-exports the most commonly used types and traits for
//! error handling in OpenRacing.
//!
//! # Example
//!
//! ```
//! use openracing_errors::prelude::*;
//!
//! fn my_function() -> Result<()> {
//!     let value = validate_and_load("config.yaml")?;
//!     Ok(())
//! }
//!
//! fn validate_and_load(path: &str) -> Result<String> {
//!     if path.is_empty() {
//!         return Err(ValidationError::required("path").into());
//!     }
//!     Ok(path.to_string())
//! }
//! ```

pub use crate::{
    RTResult, Result,
    common::{ErrorCategory, ErrorContext, ErrorSeverity, OpenRacingError, ResultExt},
    device::DeviceError,
    profile::ProfileError,
    rt::RTError,
    validation::ValidationError,
};

/// Macro for creating an error with context.
///
/// # Example
///
/// ```
/// use openracing_errors::prelude::*;
/// use openracing_errors::{ErrorContext, error_context};
///
/// # fn example() -> Result<()> {
/// let result: std::result::Result<(), OpenRacingError> = Err(OpenRacingError::config("test error"));
/// let ctx = error_context!("load_config", "file" => "config.yaml");
/// result.context(ctx)?;
/// # Ok(())
/// # }
/// ```
#[macro_export]
macro_rules! error_context {
    ($operation:expr, $($key:expr => $value:expr),* $(,)?) => {
        {
            let mut ctx = $crate::ErrorContext::new($operation);
            $(
                ctx = ctx.with($key, $value);
            )*
            ctx
        }
    };
}

/// Macro for creating a validation error with context.
///
/// Returns early with the given error if the condition is `false`.
///
/// # Examples
///
/// ```
/// use openracing_errors::prelude::*;
///
/// fn check_name(name: &str) -> Result<()> {
///     openracing_errors::validate!(!name.is_empty(), ValidationError::required("name"));
///     Ok(())
/// }
///
/// assert!(check_name("Alice").is_ok());
/// assert!(check_name("").is_err());
/// ```
#[macro_export]
macro_rules! validate {
    ($condition:expr, $error:expr) => {
        if !$condition {
            return Err($error.into());
        }
    };
}

/// Macro for creating a required field validation error.
///
/// # Examples
///
/// ```
/// use openracing_errors::prelude::*;
///
/// let err = openracing_errors::require!("profile_name");
/// assert_eq!(err.to_string(), "Required field 'profile_name' is missing");
/// ```
#[macro_export]
macro_rules! require {
    ($field:expr) => {
        $crate::ValidationError::required($field)
    };
}

/// Macro for creating an out of range validation error.
///
/// Returns early with an [`OutOfRange`](ValidationError::OutOfRange) error if
/// `$value` is outside `[$min, $max]`.
///
/// # Examples
///
/// ```
/// use openracing_errors::prelude::*;
///
/// fn check_gain(gain: f32) -> Result<()> {
///     openracing_errors::validate_range!("gain", gain, 0.0_f32, 1.0_f32);
///     Ok(())
/// }
///
/// assert!(check_gain(0.5).is_ok());
/// assert!(check_gain(1.5).is_err());
/// ```
#[macro_export]
macro_rules! validate_range {
    ($field:expr, $value:expr, $min:expr, $max:expr) => {
        if $value < $min || $value > $max {
            return Err($crate::ValidationError::out_of_range($field, $value, $min, $max).into());
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_context_macro() {
        let ctx = error_context!(
            "load_profile",
            "profile_id" => "test-123",
            "device" => "moza-r9"
        );
        assert!(ctx.to_string().contains("load_profile"));
        assert!(ctx.to_string().contains("profile_id"));
    }

    #[test]
    fn test_validate_macro() {
        fn test_fn() -> Result<()> {
            validate!(false, ValidationError::required("test"));
            Ok(())
        }
        let result = test_fn();
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_range_macro() {
        fn test_fn() -> Result<()> {
            validate_range!("torque", 0.5_f32, -1.0_f32, 1.0_f32);
            validate_range!("torque", 1.5_f32, -1.0_f32, 1.0_f32);
            Ok(())
        }
        let result = test_fn();
        assert!(result.is_err());
    }
}
