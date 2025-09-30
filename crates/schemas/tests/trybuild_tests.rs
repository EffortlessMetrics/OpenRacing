//! Compile-fail tests to ensure deprecated schema usage fails at compile time
//! 
//! These tests use trybuild to verify that code using deprecated field names,
//! missing required fields, or incorrect API usage fails to compile.

#[test]
fn compile_fail_tests() {
    let t = trybuild::TestCases::new();
    
    // Test that deprecated field names fail to compile
    t.compile_fail("tests/compile_fail/deprecated_fields.rs");
    
    // Test that missing required fields fail to compile  
    t.compile_fail("tests/compile_fail/missing_filter_fields.rs");
    
    // Test that incorrect DeviceId construction fails to compile
    t.compile_fail("tests/compile_fail/device_id_construction.rs");
}

#[test]
fn compile_pass_tests() {
    let t = trybuild::TestCases::new();
    
    // Test that correct usage compiles successfully
    t.pass("tests/compile_pass/correct_usage.rs");
}