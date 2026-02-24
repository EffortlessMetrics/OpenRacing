//! Unwrap helpers with good error messages.
//!
//! These helpers replace `unwrap()` and `expect()` in test code, providing
//! better error messages with `#[track_caller]` for accurate panic locations.
//!
//! # When to use
//!
//! - Use `must` when you have a `Result` that should succeed in tests
//! - Use `must_some` when you have an `Option` that should be `Some`
//! - Use `must_parse` when parsing strings that should parse successfully
//! - Use async variants for async test code

use std::fmt::Debug;
use std::str::FromStr;

/// Unwrap a `Result`, panicking with context on error.
///
/// This helper provides better error messages than `unwrap()` by including
/// the error value in the panic message.
///
/// # Example
///
/// ```rust
/// use openracing_test_helpers::must;
///
/// let result: Result<i32, &str> = Ok(42);
/// let value = must(result);
/// assert_eq!(value, 42);
/// ```
///
/// # Panics
///
/// Panics if the result is `Err`, with a message including the error value.
#[track_caller]
pub fn must<T, E: Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(v) => v,
        Err(e) => panic!("must: unexpected Err: {e:?}"),
    }
}

/// Unwrap an `Option`, panicking with a custom message if `None`.
///
/// # Example
///
/// ```rust
/// use openracing_test_helpers::must_some;
///
/// let option = Some(42);
/// let value = must_some(option, "expected a value");
/// assert_eq!(value, 42);
/// ```
///
/// # Panics
///
/// Panics if the option is `None`, with the provided message.
#[track_caller]
pub fn must_some<T>(option: Option<T>, msg: &str) -> T {
    match option {
        Some(v) => v,
        None => panic!("must_some: {msg}"),
    }
}

/// Parse a string into a type, panicking on failure.
///
/// # Example
///
/// ```rust
/// use openracing_test_helpers::must_parse;
///
/// let value: i32 = must_parse("42");
/// assert_eq!(value, 42);
/// ```
///
/// # Panics
///
/// Panics if parsing fails.
#[track_caller]
pub fn must_parse<T: FromStr>(s: &str) -> T
where
    T::Err: Debug,
{
    s.parse()
        .unwrap_or_else(|e| panic!("must_parse: failed to parse {s:?}: {e:?}"))
}

/// Unwrap a `Result` with a custom context message.
///
/// # Example
///
/// ```rust
/// use openracing_test_helpers::must_with;
///
/// let result: Result<i32, &str> = Err("oops");
/// // let value = must_with(result, "failed to get value"); // would panic
/// ```
///
/// # Panics
///
/// Panics if the result is `Err`, with the context and error value.
#[track_caller]
pub fn must_with<T, E: Debug>(result: Result<T, E>, context: &str) -> T {
    match result {
        Ok(v) => v,
        Err(e) => panic!("must_with: {context}: {e:?}"),
    }
}

/// Unwrap an `Option` or provide a default value.
///
/// Unlike `must_some`, this returns a default instead of panicking.
/// Useful when you want to provide a fallback in tests.
///
/// # Example
///
/// ```rust
/// use openracing_test_helpers::must_some_or;
///
/// let option: Option<i32> = None;
/// let value = must_some_or(option, 0);
/// assert_eq!(value, 0);
/// ```
pub fn must_some_or<T>(option: Option<T>, default: T) -> T {
    option.unwrap_or(default)
}

/// Unwrap a `Result` or compute a default from the error.
///
/// # Example
///
/// ```rust
/// use openracing_test_helpers::must_or_else;
///
/// let result: Result<i32, &str> = Err("oops");
/// let value = must_or_else(result, |e| e.len() as i32);
/// assert_eq!(value, 4);
/// ```
pub fn must_or_else<T, E, F>(result: Result<T, E>, f: F) -> T
where
    F: FnOnce(E) -> T,
{
    result.unwrap_or_else(f)
}

#[cfg(feature = "mock")]
mod async_helpers {
    use super::*;
    use std::future::Future;

    /// Await a future that returns `Result`, unwrapping with context on error.
    ///
    /// This is the async version of [`must`].
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use openracing_test_helpers::must_async;
    ///
    /// async fn get_value() -> Result<i32, String> { Ok(42) }
    ///
    /// #[tokio::test]
    /// async fn test_something() {
    ///     let value = must_async(get_value()).await;
    ///     assert_eq!(value, 42);
    /// }
    /// ```
    #[track_caller]
    pub async fn must_async<F, T, E>(future: F) -> T
    where
        F: Future<Output = Result<T, E>>,
        E: Debug,
    {
        match future.await {
            Ok(v) => v,
            Err(e) => panic!("must_async: unexpected Err: {e:?}"),
        }
    }

    /// Await a future that returns `Option`, unwrapping with context on `None`.
    ///
    /// This is the async version of [`must_some`].
    #[track_caller]
    pub async fn must_some_async<F, T>(future: F, msg: &str) -> T
    where
        F: Future<Output = Option<T>>,
    {
        match future.await {
            Some(v) => v,
            None => panic!("must_some_async: {msg}"),
        }
    }
}

#[cfg(feature = "mock")]
pub use async_helpers::{must_async, must_some_async};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_must_ok() {
        let result: Result<i32, &str> = Ok(42);
        assert_eq!(must(result), 42);
    }

    #[test]
    #[should_panic(expected = "must: unexpected Err")]
    fn test_must_err() {
        let result: Result<i32, &str> = Err("test error");
        let _ = must(result);
    }

    #[test]
    fn test_must_some_present() {
        let option = Some(42);
        assert_eq!(must_some(option, "expected value"), 42);
    }

    #[test]
    #[should_panic(expected = "must_some: expected value")]
    fn test_must_some_none() {
        let option: Option<i32> = None;
        let _ = must_some(option, "expected value");
    }

    #[test]
    fn test_must_parse_success() {
        let value: i32 = must_parse("42");
        assert_eq!(value, 42);
    }

    #[test]
    #[should_panic(expected = "must_parse: failed to parse")]
    fn test_must_parse_failure() {
        let _: i32 = must_parse("not a number");
    }

    #[test]
    fn test_must_with_ok() {
        let result: Result<i32, &str> = Ok(42);
        assert_eq!(must_with(result, "context"), 42);
    }

    #[test]
    #[should_panic(expected = "must_with: context")]
    fn test_must_with_err() {
        let result: Result<i32, &str> = Err("error");
        let _ = must_with(result, "context");
    }

    #[test]
    fn test_must_some_or_some() {
        assert_eq!(must_some_or(Some(42), 0), 42);
    }

    #[test]
    fn test_must_some_or_none() {
        assert_eq!(must_some_or(None::<i32>, 0), 0);
    }

    #[test]
    fn test_must_or_else_ok() {
        assert_eq!(must_or_else(Ok::<i32, &str>(42), |_| 0), 42);
    }

    #[test]
    fn test_must_or_else_err() {
        assert_eq!(
            must_or_else(Err::<i32, &str>("error"), |e| e.len() as i32),
            5
        );
    }

    #[test]
    fn test_track_caller_points_to_call_site() {
        use std::panic;

        let location = panic::catch_unwind(|| {
            let _: i32 = must_parse("bad");
        })
        .unwrap_err();

        let msg = location.downcast_ref::<String>().unwrap();
        assert!(msg.contains("must_parse"));
    }

    #[cfg(feature = "mock")]
    mod async_tests {
        use super::*;

        #[tokio::test]
        async fn test_must_async_ok() {
            async fn get() -> Result<i32, String> {
                Ok(42)
            }
            assert_eq!(must_async(get()).await, 42);
        }

        #[tokio::test]
        #[should_panic(expected = "must_async: unexpected Err")]
        async fn test_must_async_err() {
            async fn get() -> Result<i32, String> {
                Err("error".to_string())
            }
            let _ = must_async(get()).await;
        }

        #[tokio::test]
        async fn test_must_some_async_some() {
            async fn get() -> Option<i32> {
                Some(42)
            }
            assert_eq!(must_some_async(get(), "expected").await, 42);
        }

        #[tokio::test]
        #[should_panic(expected = "must_some_async: expected")]
        async fn test_must_some_async_none() {
            async fn get() -> Option<i32> {
                None
            }
            let _ = must_some_async(get(), "expected").await;
        }
    }
}
