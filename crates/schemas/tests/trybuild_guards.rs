//! Trybuild compile-fail guards for removed tokens and patterns
//!
//! These tests ensure that deprecated field names and forbidden patterns
//! are caught at compile time to prevent regression.

#[test]
fn deprecated_telemetry_fields_fail() {
    let t = trybuild::TestCases::new();
    
    // Test that deprecated field names fail to compile
    t.compile_fail("tests/compile-fail/deprecated_wheel_angle_mdeg.rs");
    t.compile_fail("tests/compile-fail/deprecated_temp_c.rs");
    t.compile_fail("tests/compile-fail/deprecated_sequence_field.rs");
    t.compile_fail("tests/compile-fail/deprecated_faults_field.rs");
    t.compile_fail("tests/compile-fail/deprecated_wheel_speed_mrad_s.rs");
}

#[test]
fn forbidden_async_patterns_fail() {
    let t = trybuild::TestCases::new();
    
    // Test that forbidden async patterns fail to compile
    t.compile_fail("tests/compile-fail/public_impl_future_trait.rs");
    t.compile_fail("tests/compile-fail/public_boxfuture_trait.rs");
}

#[test]
fn forbidden_glob_reexports_fail() {
    let t = trybuild::TestCases::new();
    
    // Test that glob re-exports fail to compile
    t.compile_fail("tests/compile-fail/root_glob_reexport.rs");
    t.compile_fail("tests/compile-fail/module_glob_reexport.rs");
}

#[test]
fn cross_crate_private_imports_fail() {
    let t = trybuild::TestCases::new();
    
    // Test that cross-crate private imports fail to compile
    t.compile_fail("tests/compile-fail/private_module_import.rs");
    t.compile_fail("tests/compile-fail/internal_module_import.rs");
}

#[test]
fn proper_patterns_pass() {
    let t = trybuild::TestCases::new();
    
    // Test that proper patterns compile successfully
    t.pass("tests/pass/correct_telemetry_usage.rs");
    t.pass("tests/pass/proper_async_trait.rs");
    t.pass("tests/pass/explicit_prelude_import.rs");
}