//! Custom assertion macros for testing.
//!
//! This module provides specialized assertions for common testing scenarios.

/// Assert that two floating-point values are approximately equal.
///
/// # Example
///
/// ```rust
/// use openracing_test_helpers::assert_approx_eq;
///
/// assert_approx_eq!(1.0, 1.0001, 0.001);
/// ```
#[macro_export]
macro_rules! assert_approx_eq {
    ($left:expr, $right:expr, $tolerance:expr $(,)?) => {
        let left = $left;
        let right = $right;
        let tolerance = $tolerance;
        let diff = (left - right).abs();
        if diff > tolerance {
            panic!(
                "assertion failed: `(left ≈ right)`\n  left: `{:?}`,\n right: `{:?}`,\n  diff: `{:?}`,\n  tolerance: `{:?}`",
                left, right, diff, tolerance
            );
        }
    };
    ($left:expr, $right:expr, $tolerance:expr, $($arg:tt)+) => {
        let left = $left;
        let right = $right;
        let tolerance = $tolerance;
        let diff = (left - right).abs();
        if diff > tolerance {
            panic!(
                "assertion failed: `(left ≈ right)`\n  left: `{:?}`,\n right: `{:?}`,\n  diff: `{:?}`,\n  tolerance: `{:?}`: {}",
                left, right, diff, tolerance, format_args!($($arg)+)
            );
        }
    };
}

/// Assert that a collection is sorted in ascending order.
///
/// # Example
///
/// ```rust
/// use openracing_test_helpers::assert_sorted;
///
/// assert_sorted!(&[1, 2, 3, 4]);
/// ```
#[macro_export]
macro_rules! assert_sorted {
    ($collection:expr $(,)?) => {
        let collection = $collection;
        let mut iter = collection.iter();
        if let Some(mut prev) = iter.next() {
            for (i, curr) in iter.enumerate() {
                if prev > curr {
                    panic!(
                        "assertion failed: collection is not sorted\n  first unsorted pair at index {}: {:?} > {:?}",
                        i, prev, curr
                    );
                }
                prev = curr;
            }
        }
    };
    ($collection:expr, $($arg:tt)+) => {
        let collection = $collection;
        let mut iter = collection.iter();
        if let Some(mut prev) = iter.next() {
            for (i, curr) in iter.enumerate() {
                if prev > curr {
                    panic!(
                        "assertion failed: collection is not sorted\n  first unsorted pair at index {}: {:?} > {:?}: {}",
                        i, prev, curr, format_args!($($arg)+)
                    );
                }
                prev = curr;
            }
        }
    };
}

/// Assert that a collection is sorted in descending order.
///
/// # Example
///
/// ```rust
/// use openracing_test_helpers::assert_sorted_desc;
///
/// assert_sorted_desc!(&[4, 3, 2, 1]);
/// ```
#[macro_export]
macro_rules! assert_sorted_desc {
    ($collection:expr $(,)?) => {
        let collection = $collection;
        let mut iter = collection.iter();
        if let Some(mut prev) = iter.next() {
            for (i, curr) in iter.enumerate() {
                if prev < curr {
                    panic!(
                        "assertion failed: collection is not sorted in descending order\n  first unsorted pair at index {}: {:?} < {:?}",
                        i, prev, curr
                    );
                }
                prev = curr;
            }
        }
    };
}

/// Assert that a sequence is monotonically increasing.
///
/// Unlike `assert_sorted`, this also verifies strict inequality (no duplicates).
///
/// # Example
///
/// ```rust
/// use openracing_test_helpers::assert_monotonic;
///
/// assert_monotonic!(&[1, 2, 3, 4]);
/// ```
#[macro_export]
macro_rules! assert_monotonic {
    ($collection:expr $(,)?) => {
        let collection = $collection;
        let mut iter = collection.iter();
        if let Some(mut prev) = iter.next() {
            for (i, curr) in iter.enumerate() {
                if prev >= curr {
                    panic!(
                        "assertion failed: sequence is not strictly monotonic\n  violation at index {}: {:?} >= {:?}",
                        i, prev, curr
                    );
                }
                prev = curr;
            }
        }
    };
    ($collection:expr, $($arg:tt)+) => {
        let collection = $collection;
        let mut iter = collection.iter();
        if let Some(mut prev) = iter.next() {
            for (i, curr) in iter.enumerate() {
                if prev >= curr {
                    panic!(
                        "assertion failed: sequence is not strictly monotonic\n  violation at index {}: {:?} >= {:?}: {}",
                        i, prev, curr, format_args!($($arg)+)
                    );
                }
                prev = curr;
            }
        }
    };
}

/// Assert that a sequence is monotonically decreasing.
///
/// # Example
///
/// ```rust
/// use openracing_test_helpers::assert_monotonic_desc;
///
/// assert_monotonic_desc!(&[4, 3, 2, 1]);
/// ```
#[macro_export]
macro_rules! assert_monotonic_desc {
    ($collection:expr $(,)?) => {
        let collection = $collection;
        let mut iter = collection.iter();
        if let Some(mut prev) = iter.next() {
            for (i, curr) in iter.enumerate() {
                if prev <= curr {
                    panic!(
                        "assertion failed: sequence is not strictly monotonic decreasing\n  violation at index {}: {:?} <= {:?}",
                        i, prev, curr
                    );
                }
                prev = curr;
            }
        }
    };
}

/// Assert that a string contains a substring.
///
/// # Example
///
/// ```rust
/// use openracing_test_helpers::assert_contains;
///
/// assert_contains!("hello world", "world");
/// ```
#[macro_export]
macro_rules! assert_contains {
    ($haystack:expr, $needle:expr $(,)?) => {
        let haystack = $haystack;
        let needle = $needle;
        if !haystack.contains(needle) {
            panic!(
                "assertion failed: string does not contain substring\n  haystack: `{:?}`\n  needle: `{:?}`",
                haystack, needle
            );
        }
    };
    ($haystack:expr, $needle:expr, $($arg:tt)+) => {
        let haystack = $haystack;
        let needle = $needle;
        if !haystack.contains(needle) {
            panic!(
                "assertion failed: string does not contain substring\n  haystack: `{:?}`\n  needle: `{:?}`: {}",
                haystack, needle, format_args!($($arg)+)
            );
        }
    };
}

/// Assert that a result is `Ok`.
///
/// # Example
///
/// ```rust
/// use openracing_test_helpers::assert_ok;
///
/// assert_ok!(Ok::<i32, &str>(42));
/// ```
#[macro_export]
macro_rules! assert_ok {
    ($result:expr $(,)?) => {
        match $result {
            Ok(v) => v,
            Err(e) => panic!("assertion failed: expected Ok, got Err({:?})", e),
        }
    };
    ($result:expr, $($arg:tt)+) => {
        match $result {
            Ok(v) => v,
            Err(e) => panic!("assertion failed: expected Ok, got Err({:?}): {}", e, format_args!($($arg)+)),
        }
    };
}

/// Assert that a result is `Err`.
///
/// # Example
///
/// ```rust
/// use openracing_test_helpers::assert_err;
///
/// assert_err!(Err::<i32, &str>("error"));
/// ```
#[macro_export]
macro_rules! assert_err {
    ($result:expr $(,)?) => {
        match $result {
            Err(e) => e,
            Ok(v) => panic!("assertion failed: expected Err, got Ok({:?})", v),
        }
    };
    ($result:expr, $($arg:tt)+) => {
        match $result {
            Err(e) => e,
            Ok(v) => panic!("assertion failed: expected Err, got Ok({:?}): {}", v, format_args!($($arg)+)),
        }
    };
}

/// Assert that an option is `Some`.
///
/// # Example
///
/// ```rust
/// use openracing_test_helpers::assert_some;
///
/// assert_some!(Some(42));
/// ```
#[macro_export]
macro_rules! assert_some {
    ($option:expr $(,)?) => {
        match $option {
            Some(v) => v,
            None => panic!("assertion failed: expected Some, got None"),
        }
    };
    ($option:expr, $($arg:tt)+) => {
        match $option {
            Some(v) => v,
            None => panic!("assertion failed: expected Some, got None: {}", format_args!($($arg)+)),
        }
    };
}

/// Assert that an option is `None`.
///
/// # Example
///
/// ```rust
/// use openracing_test_helpers::assert_none;
///
/// assert_none!(None::<i32>);
/// ```
#[macro_export]
macro_rules! assert_none {
    ($option:expr $(,)?) => {
        match $option {
            None => (),
            Some(v) => panic!("assertion failed: expected None, got Some({:?})", v),
        }
    };
    ($option:expr, $($arg:tt)+) => {
        match $option {
            None => (),
            Some(v) => panic!("assertion failed: expected None, got Some({:?}): {}", v, format_args!($($arg)+)),
        }
    };
}

/// Assert that a collection is empty.
///
/// # Example
///
/// ```rust
/// use openracing_test_helpers::assert_empty;
///
/// assert_empty!(&[]);
/// ```
#[macro_export]
macro_rules! assert_empty {
    ($collection:expr $(,)?) => {
        let collection = $collection;
        if !collection.is_empty() {
            panic!("assertion failed: collection is not empty (len = {})", collection.len());
        }
    };
    ($collection:expr, $($arg:tt)+) => {
        let collection = $collection;
        if !collection.is_empty() {
            panic!("assertion failed: collection is not empty (len = {}): {}", collection.len(), format_args!($($arg)+));
        }
    };
}

/// Assert that a collection is not empty.
///
/// # Example
///
/// ```rust
/// use openracing_test_helpers::assert_not_empty;
///
/// assert_not_empty!(&[1, 2, 3]);
/// ```
#[macro_export]
macro_rules! assert_not_empty {
    ($collection:expr $(,)?) => {
        let collection = $collection;
        if collection.is_empty() {
            panic!("assertion failed: collection is empty");
        }
    };
    ($collection:expr, $($arg:tt)+) => {
        let collection = $collection;
        if collection.is_empty() {
            panic!("assertion failed: collection is empty: {}", format_args!($($arg)+));
        }
    };
}

/// Assert that a value is within a range.
///
/// # Example
///
/// ```rust
/// use openracing_test_helpers::assert_in_range;
///
/// assert_in_range!(5, 0..=10);
/// ```
#[macro_export]
macro_rules! assert_in_range {
    ($value:expr, $range:expr $(,)?) => {
        let value = $value;
        let range = $range;
        if !range.contains(&value) {
            panic!("assertion failed: value {:?} is not in range {:?}", value, range);
        }
    };
    ($value:expr, $range:expr, $($arg:tt)+) => {
        let value = $value;
        let range = $range;
        if !range.contains(&value) {
            panic!("assertion failed: value {:?} is not in range {:?}: {}", value, range, format_args!($($arg)+));
        }
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_assert_approx_eq_pass() {
        assert_approx_eq!(1.0, 1.0001, 0.001);
    }

    #[test]
    #[should_panic(expected = "assertion failed: `(left ≈ right)`")]
    fn test_assert_approx_eq_fail() {
        assert_approx_eq!(1.0, 1.1, 0.001);
    }

    #[test]
    fn test_assert_sorted_pass() {
        assert_sorted!(&[1, 2, 3, 4]);
        assert_sorted!(&[1, 1, 2, 3]);
        assert_sorted!(&[] as &[i32]);
        assert_sorted!(&[1]);
    }

    #[test]
    #[should_panic(expected = "collection is not sorted")]
    fn test_assert_sorted_fail() {
        assert_sorted!(&[3, 2, 1]);
    }

    #[test]
    fn test_assert_sorted_desc_pass() {
        assert_sorted_desc!(&[4, 3, 2, 1]);
        assert_sorted_desc!(&[4, 4, 3, 2]);
    }

    #[test]
    #[should_panic(expected = "not sorted in descending order")]
    fn test_assert_sorted_desc_fail() {
        assert_sorted_desc!(&[1, 2, 3]);
    }

    #[test]
    fn test_assert_monotonic_pass() {
        assert_monotonic!(&[1, 2, 3, 4]);
        assert_monotonic!(&[] as &[i32]);
        assert_monotonic!(&[1]);
    }

    #[test]
    #[should_panic(expected = "not strictly monotonic")]
    fn test_assert_monotonic_fail_duplicates() {
        assert_monotonic!(&[1, 2, 2, 3]);
    }

    #[test]
    #[should_panic(expected = "not strictly monotonic")]
    fn test_assert_monotonic_fail_decreasing() {
        assert_monotonic!(&[3, 2, 1]);
    }

    #[test]
    fn test_assert_monotonic_desc_pass() {
        assert_monotonic_desc!(&[4, 3, 2, 1]);
    }

    #[test]
    #[should_panic(expected = "not strictly monotonic decreasing")]
    fn test_assert_monotonic_desc_fail() {
        assert_monotonic_desc!(&[1, 2, 3]);
    }

    #[test]
    fn test_assert_contains_pass() {
        assert_contains!("hello world", "world");
    }

    #[test]
    #[should_panic(expected = "does not contain substring")]
    fn test_assert_contains_fail() {
        assert_contains!("hello", "world");
    }

    #[test]
    fn test_assert_ok_pass() {
        let result: Result<i32, &str> = Ok(42);
        let value = assert_ok!(result);
        assert_eq!(value, 42);
    }

    #[test]
    #[should_panic(expected = "expected Ok, got Err")]
    fn test_assert_ok_fail() {
        let result: Result<i32, &str> = Err("error");
        assert_ok!(result);
    }

    #[test]
    fn test_assert_err_pass() {
        let result: Result<i32, &str> = Err("error");
        let err = assert_err!(result);
        assert_eq!(err, "error");
    }

    #[test]
    #[should_panic(expected = "expected Err, got Ok")]
    fn test_assert_err_fail() {
        let result: Result<i32, &str> = Ok(42);
        assert_err!(result);
    }

    #[test]
    fn test_assert_some_pass() {
        let option = Some(42);
        let value = assert_some!(option);
        assert_eq!(value, 42);
    }

    #[test]
    #[should_panic(expected = "expected Some, got None")]
    fn test_assert_some_fail() {
        let option: Option<i32> = None;
        assert_some!(option);
    }

    #[test]
    fn test_assert_none_pass() {
        let option: Option<i32> = None;
        assert_none!(option);
    }

    #[test]
    #[should_panic(expected = "expected None, got Some")]
    fn test_assert_none_fail() {
        let option = Some(42);
        assert_none!(option);
    }

    #[test]
    fn test_assert_empty_pass() {
        assert_empty!(&[] as &[i32]);
        assert_empty!(&String::new());
        assert_empty!(&Vec::<i32>::new());
    }

    #[test]
    #[should_panic(expected = "collection is not empty")]
    fn test_assert_empty_fail() {
        assert_empty!(&[1, 2, 3]);
    }

    #[test]
    fn test_assert_not_empty_pass() {
        assert_not_empty!(&[1, 2, 3]);
    }

    #[test]
    #[should_panic(expected = "collection is empty")]
    fn test_assert_not_empty_fail() {
        assert_not_empty!(&[] as &[i32]);
    }

    #[test]
    fn test_assert_in_range_pass() {
        assert_in_range!(5, 0..=10);
        assert_in_range!(0, 0..=10);
        assert_in_range!(10, 0..=10);
    }

    #[test]
    #[should_panic(expected = "is not in range")]
    fn test_assert_in_range_fail() {
        assert_in_range!(11, 0..=10);
    }
}
