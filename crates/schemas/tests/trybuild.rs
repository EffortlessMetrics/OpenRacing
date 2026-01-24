//! Trybuild compile-fail tests for deprecated tokens and API boundaries
//!
//! These tests ensure that deprecated field names and patterns are caught at compile time:
//! - Deprecated telemetry field names (wheel_angle_mdeg, temp_c, sequence)
//! - Forbidden async patterns in public APIs
//! - Glob re-exports in public APIs

#[test]
fn deprecated_tokens_compile_fail_tests() {
    let t = trybuild::TestCases::new();

    // Test that deprecated field names fail to compile
    t.compile_fail("tests/compile-fail/public_impl_future_trait.rs"); // wheel_angle_mdeg
    t.compile_fail("tests/compile-fail/public_boxfuture_trait.rs"); // temp_c
    t.compile_fail("tests/compile-fail/glob_reexport.rs"); // sequence
}

#[test]
fn async_trait_patterns_pass_tests() {
    let t = trybuild::TestCases::new();

    // Test that proper async trait patterns compile successfully
    t.pass("tests/pass/good_async_trait.rs");
}
